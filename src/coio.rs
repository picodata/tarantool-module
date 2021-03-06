//! Cooperative input/output
//!
//! See also:
//! - [C API reference: Module coio](https://www.tarantool.io/en/doc/latest/dev_guide/reference_capi/coio/)
use std::convert::TryFrom;
use std::ffi::c_void;
use std::io;
use std::io::{Read, Write};
use std::mem::forget;
use std::net::{SocketAddr, TcpListener, TcpStream, ToSocketAddrs};
use std::os::raw::c_char;
use std::os::unix::io::{AsRawFd, IntoRawFd, RawFd};
use std::time::Duration;

use failure::_core::ptr::null_mut;

use crate::error::{Error, TarantoolError};
use crate::ffi::tarantool as ffi;
use crate::fiber::unpack_callback;

const TIMEOUT_INFINITY: f64 = 365.0 * 86400.0 * 100.0;

/// Uses CoIO main loop to poll read/write events from wrapped socket
pub struct CoIOStream {
    fd: RawFd,
}

impl CoIOStream {
    /// Convert fd-like object to CoIO stream
    pub fn new<T>(inner: T) -> Result<CoIOStream, io::Error>
    where
        T: IntoRawFd,
    {
        let fd = inner.into_raw_fd();
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL, 0) };
        if flags < 0 {
            return Err(io::Error::last_os_error());
        }

        if unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 } {
            Err(io::Error::last_os_error())
        } else {
            Ok(CoIOStream { fd })
        }
    }

    /// Connect to remote TCP socket
    pub fn connect<A: ToSocketAddrs>(addr: A) -> Result<CoIOStream, io::Error> {
        let inner_stream = TcpStream::connect(addr)?;
        inner_stream.set_nonblocking(true)?;
        Ok(CoIOStream {
            fd: inner_stream.into_raw_fd(),
        })
    }

    /// Opens a TCP connection to a remote host with a timeout.
    pub fn connect_timeout(addr: &SocketAddr, timeout: Duration) -> Result<CoIOStream, io::Error> {
        let inner_stream = TcpStream::connect_timeout(addr, timeout)?;
        inner_stream.set_nonblocking(true)?;
        Ok(CoIOStream {
            fd: inner_stream.into_raw_fd(),
        })
    }

    /// Pull some bytes from this source into the specified buffer. Returns how many bytes were read or 0 on timeout.
    pub fn read_with_timeout(
        &mut self,
        buf: &mut [u8],
        timeout: Option<Duration>,
    ) -> Result<usize, io::Error> {
        read(self.fd, buf, timeout)
    }

    /// Write a buffer into this writer. Returning how many bytes were written or 0 on timeout.
    pub fn write_with_timeout(
        &mut self,
        buf: &[u8],
        timeout: Option<Duration>,
    ) -> Result<usize, io::Error> {
        write(self.fd, buf, timeout)
    }
}

impl IntoRawFd for CoIOStream {
    fn into_raw_fd(self) -> RawFd {
        let fd = self.fd;
        forget(self);
        fd
    }
}

impl AsRawFd for CoIOStream {
    fn as_raw_fd(&self) -> RawFd {
        self.fd
    }
}

impl Read for CoIOStream {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, io::Error> {
        self.read_with_timeout(buf, None)
    }
}

impl Write for CoIOStream {
    fn write(&mut self, buf: &[u8]) -> Result<usize, io::Error> {
        self.write_with_timeout(buf, None)
    }

    fn flush(&mut self) -> Result<(), io::Error> {
        Ok(())
    }
}

impl Drop for CoIOStream {
    fn drop(&mut self) {
        unsafe { ffi::coio_close(self.fd) };
    }
}

/// Uses CoIO main loop to poll incoming connections from wrapped socket listener
pub struct CoIOListener {
    inner: TcpListener,
}

impl CoIOListener {
    /// Accept a new incoming connection from this listener.
    pub fn accept(&self) -> Result<CoIOStream, io::Error> {
        loop {
            let res = self.inner.accept();
            return match res {
                Ok((stream, _)) => CoIOStream::new(stream),

                Err(e) => {
                    if e.kind() == io::ErrorKind::WouldBlock {
                        coio_wait(
                            self.inner.as_raw_fd(),
                            ffi::CoIOFlags::READ,
                            TIMEOUT_INFINITY,
                        )?;
                        continue;
                    }
                    Err(e)
                }
            };
        }
    }

    pub fn inner_listener(&mut self) -> &mut TcpListener {
        &mut self.inner
    }
}

impl TryFrom<TcpListener> for CoIOListener {
    type Error = io::Error;

    fn try_from(value: TcpListener) -> Result<Self, Self::Error> {
        value.set_nonblocking(true)?;
        Ok(Self { inner: value })
    }
}

/// Wait until `READ` or `WRITE` event on socket (`fd`). Yields.
///
/// - `fd` - non-blocking socket file description
/// - `events` - requested events to wait. Combination of [CoIOFlags::READ | CoIOFlags::WRITE](struct.CoIOFlags.html) bit flags.
/// - `timeoout` - timeout in seconds.
pub fn coio_wait(fd: RawFd, flags: ffi::CoIOFlags, timeout: f64) -> Result<(), io::Error> {
    match unsafe { ffi::coio_wait(fd, flags.bits(), timeout) } {
        0 => Err(io::ErrorKind::TimedOut.into()),
        _ => Ok(()),
    }
}

/// Create new eio task with specified function and
/// arguments. Yield and wait until the task is complete
/// or a timeout occurs.
///
/// This function doesn't throw exceptions to avoid double error
/// checking: in most cases it's also necessary to check the return
/// value of the called function and perform necessary actions. If
/// func sets errno, the errno is preserved across the call.
///
/// Returns:
/// - `-1` and `errno = ENOMEM` if failed to create a task
/// - the function return (errno is preserved).
///
/// ```
/// struct FuncArgs {}
///
/// fn func(args: FuncArgs) -> i32 {}
///
/// if coio_call(func, FuncArgs{}) == -1 {
///		// handle errors.
/// }
/// ```
pub fn coio_call<F, T>(callback: &mut F, arg: T) -> isize
where
    F: FnMut(Box<T>) -> i32,
{
    let (callback_ptr, trampoline) = unsafe { unpack_callback(callback) };
    unsafe { ffi::coio_call(trampoline, callback_ptr, Box::into_raw(Box::<T>::new(arg))) }
}

/// Fiber-friendly version of `getaddrinfo(3)`.
///
/// - `host` - host name, i.e. "tarantool.org"
/// - `port` - service name, i.e. "80" or "http"
/// - `hints` - hints, see `getaddrinfo(3)`
/// - `timeout` - timeout
pub fn getaddrinfo(
    host: &str,
    port: &str,
    hints: &libc::addrinfo,
    timeout: f64,
) -> Result<libc::addrinfo, Error> {
    let mut result: *mut libc::addrinfo = null_mut();
    if unsafe {
        ffi::coio_getaddrinfo(
            host.as_ptr() as *const c_char,
            port.as_ptr() as *const c_char,
            &*hints,
            &mut result,
            timeout,
        )
    } < 0
    {
        Err(TarantoolError::last().into())
    } else {
        Ok(unsafe { result.read() })
    }
}

#[inline(always)]
pub(crate) fn read(
    fd: RawFd,
    buf: &mut [u8],
    timeout: Option<Duration>,
) -> Result<usize, io::Error> {
    let buf_len = buf.len();
    let result = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut c_void, buf_len) };
    if result >= 0 {
        return Ok(result as usize);
    }

    let err = io::Error::last_os_error();
    if err.kind() != io::ErrorKind::WouldBlock {
        return Err(err);
    }

    let timeout = match timeout {
        None => TIMEOUT_INFINITY,
        Some(timeout) => timeout.as_secs_f64(),
    };

    coio_wait(fd, ffi::CoIOFlags::READ, timeout)?;
    let result = unsafe { libc::read(fd, buf.as_mut_ptr() as *mut c_void, buf_len) };
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(result as usize)
    }
}

#[inline(always)]
pub(crate) fn write(fd: RawFd, buf: &[u8], timeout: Option<Duration>) -> Result<usize, io::Error> {
    let result = unsafe { libc::write(fd, buf.as_ptr() as *mut c_void, buf.len()) };
    if result >= 0 {
        return Ok(result as usize);
    }

    let err = io::Error::last_os_error();
    if err.kind() != io::ErrorKind::WouldBlock {
        return Err(err);
    }

    let timeout = match timeout {
        None => TIMEOUT_INFINITY,
        Some(timeout) => timeout.as_secs_f64(),
    };

    coio_wait(fd, ffi::CoIOFlags::WRITE, timeout)?;
    let result = unsafe { libc::write(fd, buf.as_ptr() as *mut c_void, buf.len()) };
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(result as usize)
    }
}
