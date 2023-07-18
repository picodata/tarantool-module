//! Contains an implementation of a custom async coio based [`TcpStream`].
//!
//! ## Example
//! ```no_run
//! # async {
//! use futures::AsyncReadExt;
//! use tarantool::network::client::tcp::TcpStream;
//!
//! let mut stream = TcpStream::connect("localhost", 8080)
//!     .await
//!     .unwrap();
//! let mut buf = vec![];
//! let read_size = stream
//!     .read(&mut buf)
//!     .await
//!     .unwrap();
//! # };
//! ```

use std::cell::Cell;
use std::ffi::{CString, NulError};
use std::future::Future;
use std::io::{self, ErrorKind};
use std::mem::{self};
use std::net::{SocketAddr, SocketAddrV4, SocketAddrV6, ToSocketAddrs};
use std::os::unix::io::RawFd;
use std::os::unix::prelude::IntoRawFd;
use std::pin::Pin;
use std::rc::Rc;
use std::task::{Context, Poll};

use futures::{AsyncRead, AsyncWrite};

use crate::ffi::tarantool::{self as ffi, CAResAddrInfo, CAResAddrInfoNode};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("failed to resolve host address by domain name")]
    ResolveAddress,
    #[error("input parameters contain ffi incompatible strings: {0}")]
    ConstructCString(NulError),
    #[error("failed to connect to supplied address: {0}")]
    Connect(io::Error),
    #[error("failed to set socket to nonblocking mode: {0}")]
    SetNonBlock(io::Error),
    #[error("unknown address family: {0}")]
    UnknownAddressFamily(u16),
    #[error("write half of the stream is closed")]
    WriteClosed,
}

/// Async TcpStream based on fibers and coio.
///
/// Use [timeout][t] on top of read or write operations on [`TcpStream`]
/// to set the max time to wait for an operation.
///
/// Atention should be payed that [`TcpStream`] is not [`futures::select`] friendly when awaiting multiple streams
/// As there is no coio support to await multiple file descriptors yet.
/// Though it can be used with [`futures::join`] without problems.
///
/// See module level [documentation](super::tcp) for examples.
///
/// [t]: crate::fiber::async::timeout::timeout
#[derive(Debug)]
pub struct TcpStream {
    fd: RawFd,

    coio_event_memory: *mut libc::c_void,
}

impl TcpStream {
    /// Creates a [`TcpStream`] to `url`.
    /// `resolve_timeout` - address resolution timeout.
    ///
    /// This functions makes the fiber **yield**.
    pub async fn connect(url: &str, port: u16) -> Result<TcpStream, Error> {
        let sockaddr = SocketAddress::resolve(url, port).await?;

        let stream = std::net::TcpStream::connect(sockaddr).map_err(Error::Connect)?;
        stream.set_nonblocking(true).map_err(Error::SetNonBlock)?;

        Ok(Self {
            fd: stream.into_raw_fd(),
            coio_event_memory: core::ptr::null_mut(),
        })
    }

    pub fn coio_event_add(&mut self, flags: ffi::CoIOFlags) {
        if !self.coio_event_memory.is_null() {
            unsafe {
                crate::ffi::tarantool::coio_wait_event_update(self.coio_event_memory, flags.bits());
            }
            return;
        }

        self.coio_event_memory =
            unsafe { crate::ffi::tarantool::coio_wait_event_register(self.fd, flags.bits()) };
    }

    pub fn coio_event_remove(&mut self) {
        if self.coio_event_memory.is_null() {
            return;
        }

        unsafe { crate::ffi::tarantool::coio_wait_event_free(self.coio_event_memory) };
    }

    /// Close token for [`TcpStream`] to be able to close it from other fibers.
    pub fn close_token(&self) -> CloseToken {
        CloseToken(self.fd)
    }
}

impl AsyncWrite for TcpStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let (result, err) = (
            // `self.fd` must be nonblocking for this to work correctly
            unsafe { libc::write(self.fd, buf.as_ptr() as *const libc::c_void, buf.len()) },
            io::Error::last_os_error(),
        );

        if result >= 0 {
            return Poll::Ready(Ok(result as usize));
        }
        match err.kind() {
            io::ErrorKind::WouldBlock => {
                self.coio_event_add(ffi::CoIOFlags::WRITE);

                Poll::Pending
            }
            io::ErrorKind::Interrupted => {
                // Return poll pending without setting coio wait
                // so that write can be retried immediately.
                Poll::Pending
            }
            _ => Poll::Ready(Err(err)),
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // [`TcpStream`] similarily to std does not buffer anything,
        // so there is nothing to flush.
        //
        // If buffering is needed use [`futures::io::BufWriter`] on top of this stream.
        Poll::Ready(Ok(()))
    }

    fn poll_close(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Poll::Ready(self.close_token().close())
    }
}

impl AsyncRead for TcpStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        _cx: &mut Context<'_>,
        buf: &mut [u8],
    ) -> Poll<io::Result<usize>> {
        let (result, err) = (
            // `self.fd` must be nonblocking for this to work correctly
            unsafe { libc::read(self.fd, buf.as_mut_ptr() as *mut libc::c_void, buf.len()) },
            io::Error::last_os_error(),
        );

        if result >= 0 {
            return Poll::Ready(Ok(result as usize));
        }
        match err.kind() {
            io::ErrorKind::WouldBlock => {
                self.coio_event_add(ffi::CoIOFlags::READ);

                Poll::Pending
            }
            io::ErrorKind::Interrupted => {
                // Return poll pending without setting coio wait
                // so that read can be retried immediately.
                Poll::Pending
            }
            _ => Poll::Ready(Err(err)),
        }
    }
}

impl Drop for TcpStream {
    fn drop(&mut self) {
        let _ = self.close_token().close();
        self.coio_event_remove();
    }
}

/// Close token for [`TcpStream`] to be able to close it from other fibers.
#[derive(Debug)]
pub struct CloseToken(RawFd);

impl CloseToken {
    pub fn close(&self) -> io::Result<()> {
        let (res, err) = (
            unsafe { ffi::coio_close(self.0) },
            io::Error::last_os_error(),
        );
        if res != 0 {
            Err(err)
        } else {
            Ok(())
        }
    }
}

struct SocketAddress(*mut CAResAddrInfo, u16);

impl SocketAddress {
    pub async fn resolve(url: &str, port: u16) -> Result<Self, Error> {
        // avoiding the destory calls on each future poll event:
        // we need drop only after future function completed
        let host = Rc::new(CString::new(url).map_err(Error::ConstructCString)?);
        let addrinfo = GetAddrInfo::from_url(&host).await?;

        Ok(SocketAddress(addrinfo, port))
    }
}

impl Drop for SocketAddress {
    fn drop(&mut self) {
        unsafe {
            crate::ffi::tarantool::coio_ares_freeaddrinfo(self.0);
        }
    }
}

impl ToSocketAddrs for SocketAddress {
    type Iter = std::vec::IntoIter<SocketAddr>;

    fn to_socket_addrs(&self) -> io::Result<std::vec::IntoIter<SocketAddr>> {
        let mut error = io::Error::from(ErrorKind::NotFound);
        let addrinfo = self.0;
        let port = self.1;

        if addrinfo.is_null() {
            return Err(error);
        }

        let mut socket_address_vector: Vec<SocketAddr> = Vec::new();

        let mut addr = unsafe { *addrinfo }.nodes;
        while !addr.is_null() {
            let res = cares_to_rs_socket_addr(addr, port);

            match res {
                Ok(sockaddr) => socket_address_vector.push(sockaddr),
                Err(err) => error = err,
            }

            addr = unsafe { *addr }.ai_next;
        }

        // one erroneous node still not means that others too. So, let's return
        // an error only when no one node translated to the correct address
        if socket_address_vector.is_empty() {
            return Err(error);
        }

        Ok(socket_address_vector.into_iter())
    }
}

fn cares_to_rs_socket_addr(
    node: *const CAResAddrInfoNode,
    port: u16,
) -> Result<SocketAddr, io::Error> {
    if node.is_null() {
        return Err(io::Error::from(ErrorKind::NotFound));
    }

    unsafe {
        let addr = (*node).ai_addr;

        match (*addr).sa_family as libc::c_int {
            libc::AF_INET => {
                let addr: *mut libc::sockaddr_in = mem::transmute(addr);
                (*addr).sin_port = port;
                let octets: [u8; 4] = (*addr).sin_addr.s_addr.to_ne_bytes();

                Ok(SocketAddr::V4(SocketAddrV4::new(octets.into(), port)))
            }
            libc::AF_INET6 => {
                let addr: *mut libc::sockaddr_in6 = mem::transmute(addr);
                (*addr).sin6_port = port;
                let octets = (*addr).sin6_addr.s6_addr;
                let flow_info = (*addr).sin6_flowinfo;
                let scope_id = (*addr).sin6_scope_id;

                Ok(SocketAddr::V6(SocketAddrV6::new(
                    octets.into(),
                    port,
                    flow_info,
                    scope_id,
                )))
            }
            _ => Err(io::Error::from(ErrorKind::InvalidData)),
        }
    }
}

/// Request for address resolution. After being set it will be ready to be awaited by `block_on`.
///
/// `host` is a user input parameter.
/// Since getaddrinfo actually uses only hostname (and not uses port) for the resolution, only
/// host provided in this structure.
/// `result` is a place where the dynamically allocated list with the resolution results will be
/// placed during the resolving process. It is an output parameter of the future.
/// `err` is also an output flag which may be installed to true if error occured during the
/// resolution process. If it happened - the future implemented for this struct will return an
/// error too.
///
/// `active` is an internal flag to monitor if current dns reques still in progress and avoid to
/// resend it on each poll.
/// `memory` is a dynamically allocated context of the current dns request. Needed for internal use.
/// Only C code knows how to operate with it. Allocates on future creation and deallocates on drop.
#[derive(Debug)]
struct GetAddrInfo {
    host: Rc<CString>,
    result: *mut CAResAddrInfo,
    err: Rc<Cell<bool>>,

    active: bool,
    memory: *mut std::os::raw::c_void,
}

impl GetAddrInfo {
    pub fn from_url(url: &Rc<CString>) -> Self {
        GetAddrInfo {
            host: Rc::clone(url),
            result: core::ptr::null_mut(),
            err: Rc::new(Cell::new(false)),

            active: false,
            memory: core::ptr::null_mut(),
        }
    }
}

impl Future for GetAddrInfo {
    type Output = Result<*mut CAResAddrInfo, Error>;

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.err.get() {
            return Poll::Ready(Err(Error::ResolveAddress));
        }

        if self.result.is_null() {
            if self.active {
                return Poll::Pending;
            }

            self.active = true;

            self.memory = unsafe {
                crate::ffi::tarantool::coio_ares_getaddrinfo(
                    self.host.as_ptr(),
                    &mut self.result as *mut _,
                    self.err.as_ptr(),
                )
            };

            // result may be returned immediately for localhost and other /etc/hosts entries:
            // it means we need to doublecheck the result here to avoid the futures output misorder
            if self.err.get() {
                return Poll::Ready(Err(Error::ResolveAddress));
            }
            if !self.result.is_null() {
                return Poll::Ready(Ok(self.result));
            }

            Poll::Pending
        } else {
            Poll::Ready(Ok(self.result))
        }
    }
}

impl Drop for GetAddrInfo {
    fn drop(&mut self) {
        unsafe {
            crate::ffi::tarantool::coio_ares_request_cancel(self.memory);
        }
    }
}

#[cfg(feature = "internal_test")]
mod tests {
    use super::*;

    use crate::fiber::r#async::timeout::{self, IntoTimeout};
    use crate::fiber::{self};
    use crate::test::util::always_pending;
    use crate::test::util::TARANTOOL_LISTEN;

    use std::net::TcpListener;
    use std::thread;
    use std::time::Duration;

    use futures::{AsyncReadExt, AsyncWriteExt, FutureExt};

    const _10_SEC: Duration = Duration::from_secs(10);
    const _0_SEC: Duration = Duration::from_secs(0);

    #[crate::test(tarantool = "crate")]
    fn resolve_address() {
        let _ = fiber::block_on(async {
            GetAddrInfo::from_url(&Rc::new(CString::new("localhost").unwrap()))
                .timeout(_10_SEC)
                .await
        })
        .unwrap();
    }

    #[crate::test(tarantool = "crate")]
    fn resolve_address_error() {
        let err = fiber::block_on(async {
            GetAddrInfo::from_url(&Rc::new(CString::new("invalid domain name").unwrap()))
                .timeout(_10_SEC)
                .await
        })
        .unwrap_err()
        .to_string();
        assert_eq!(err, "failed to resolve host address by domain name")
    }

    #[crate::test(tarantool = "crate")]
    fn resolve_address_timeout() {
        let _ = fiber::block_on(async {
            GetAddrInfo::from_url(&Rc::new(CString::new("invalid.com").unwrap()))
                .timeout(_0_SEC)
                .await
        });
    }

    fn make_two_futures_for_select(
        addr1: &str,
        addr2: &str,
    ) -> (
        futures::future::Fuse<
            impl futures::Future<Output = std::result::Result<*mut CAResAddrInfo, super::Error>>,
        >,
        futures::future::Fuse<
            impl futures::Future<Output = std::result::Result<*mut CAResAddrInfo, super::Error>>,
        >,
    ) {
        let (first, second) = (
            GetAddrInfo::from_url(&Rc::new(CString::new(addr1).unwrap())).fuse(),
            GetAddrInfo::from_url(&Rc::new(CString::new(addr2).unwrap())).fuse(),
        );

        (first, second)
    }

    #[crate::test(tarantool = "crate")]
    fn async_resolve_select_two_valid() {
        // in contradiction of async_resolve_select_localhost() test,
        // this one is proof of correct work for each connection speeds
        fiber::block_on(async {
            let (long_time, immediate) = make_two_futures_for_select("google.com", "localhost");

            futures::pin_mut!(long_time);
            futures::pin_mut!(immediate);

            let first_completed_future = futures::select! {
                _ = long_time => {
                    "long_time"
                },
                res = immediate => {
                    assert!(res.is_ok());
                    "immediate"
                },
            };

            assert!(first_completed_future == "immediate");
        });
    }

    #[crate::test(tarantool = "crate")]
    fn async_resolve_select_exit_on_timeout() {
        // invalid.com is a special domain name like localhost or example.com but guaranteed unreachable
        fiber::block_on(async {
            let (unreachable_1, unreachable_2) = (
                GetAddrInfo::from_url(&Rc::new(CString::new("invalid.com").unwrap()))
                    .timeout(Duration::from_secs(0))
                    .fuse(),
                GetAddrInfo::from_url(&Rc::new(CString::new("invalid.com").unwrap()))
                    .timeout(Duration::from_secs(0))
                    .fuse(),
            );
            futures::pin_mut!(unreachable_1);
            futures::pin_mut!(unreachable_2);

            futures::select! {
                res = unreachable_1 => {
                    assert!(res.is_err());
                },
                res = unreachable_2 => {
                    panic!("Select must not exit here. {:?}", res);
                },
            };

            // if select returned - test passed
        });
    }

    #[crate::test(tarantool = "crate")]
    fn async_resolve_select_localhost() {
        fiber::block_on(async {
            let (long_time, immediate) = make_two_futures_for_select("invalid.com", "localhost");

            futures::pin_mut!(long_time);
            futures::pin_mut!(immediate);

            let first_completed_future = futures::select! {
                _ = long_time => {
                    "long_time"
                },
                res = immediate => {
                    assert!(res.is_ok());
                    "immediate"
                },
            };

            assert!(first_completed_future == "immediate");
        });
    }

    #[crate::test(tarantool = "crate")]
    fn async_resolve_select_ip() {
        fiber::block_on(async {
            // on ip resolving c-ares can't resolve the localhost immediately via getaddrinfo_cb
            // it means sock_state_cb invoked and ev_io event registered. So, we can use this info
            // to test with guaranteed reachable ip, which will be resolved after io_cb
            let (long_time, immediate) =
                make_two_futures_for_select("invalid.com", "127.0.0.1" /*"5.255.255.242"*/);

            futures::pin_mut!(long_time);
            futures::pin_mut!(immediate);

            let first_completed_future = futures::select! {
                _ = long_time => {
                    "long_time"
                },
                res = immediate => {
                    assert!(res.is_ok());
                    "immediate"
                },
            };

            assert!(first_completed_future == "immediate");
        });
    }

    #[crate::test(tarantool = "crate")]
    fn async_resolve_select_normal_behavior() {
        fiber::block_on(async {
            let (long_time, short_time) = (
                GetAddrInfo::from_url(&Rc::new(CString::new("invalid.com").unwrap())).fuse(),
                GetAddrInfo::from_url(&Rc::new(CString::new("ya.ru").unwrap())).fuse(),
            );

            futures::pin_mut!(long_time);
            futures::pin_mut!(short_time);

            let first_completed_future = futures::select! {
                _ = long_time => {
                    "long_time"
                },
                res = short_time => {
                    assert!(res.is_ok());
                    "short_time"
                },
            };

            assert!(first_completed_future == "short_time");
        });
    }

    #[crate::test(tarantool = "crate")]
    fn connect() {
        let _ = fiber::block_on(TcpStream::connect("localhost", TARANTOOL_LISTEN).timeout(_10_SEC))
            .unwrap();
    }

    #[crate::test(tarantool = "crate")]
    async fn read() {
        let mut stream = TcpStream::connect("localhost", TARANTOOL_LISTEN)
            .timeout(_10_SEC)
            .await
            .unwrap();
        // Read greeting
        let mut buf = vec![0; 128];
        stream.read_exact(&mut buf).timeout(_10_SEC).await.unwrap();
    }

    #[crate::test(tarantool = "crate")]
    async fn read_timeout() {
        let mut stream = TcpStream::connect("localhost", TARANTOOL_LISTEN)
            .timeout(_10_SEC)
            .await
            .unwrap();
        // Read greeting
        let mut buf = vec![0; 128];
        assert_eq!(
            stream
                .read_exact(&mut buf)
                .timeout(_0_SEC)
                .await
                .unwrap_err()
                .to_string(),
            "deadline expired"
        );
    }

    #[crate::test(tarantool = "crate")]
    fn write() {
        let (sender, receiver) = std::sync::mpsc::channel();
        let listener = TcpListener::bind("127.0.0.1:3302").unwrap();
        // Spawn listener
        thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let mut buf = vec![];
                <std::net::TcpStream as std::io::Read>::read_to_end(&mut stream, &mut buf).unwrap();
                sender.send(buf).unwrap();
            }
        });
        // Send data
        {
            fiber::block_on(async {
                let mut stream = TcpStream::connect("localhost", 3302)
                    .timeout(_10_SEC)
                    .await
                    .unwrap();
                timeout::timeout(_10_SEC, stream.write_all(&[1, 2, 3]))
                    .await
                    .unwrap();
                timeout::timeout(_10_SEC, stream.write_all(&[4, 5]))
                    .await
                    .unwrap();
            });
        }
        let buf = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(buf, vec![1, 2, 3, 4, 5])
    }

    #[crate::test(tarantool = "crate")]
    fn split() {
        let (sender, receiver) = std::sync::mpsc::channel();
        let listener = TcpListener::bind("127.0.0.1:3303").unwrap();
        // Spawn listener
        thread::spawn(move || {
            for stream in listener.incoming() {
                let mut stream = stream.unwrap();
                let mut buf = vec![0; 5];
                <std::net::TcpStream as std::io::Read>::read_exact(&mut stream, &mut buf).unwrap();
                <std::net::TcpStream as std::io::Write>::write_all(&mut stream, &buf.clone())
                    .unwrap();
                sender.send(buf).unwrap();
            }
        });
        // Send and read data
        {
            let stream =
                fiber::block_on(TcpStream::connect("localhost", 3303).timeout(_10_SEC)).unwrap();
            let (mut reader, mut writer) = stream.split();
            let reader_handle = fiber::start_async(async move {
                let mut buf = vec![0; 5];
                timeout::timeout(_10_SEC, reader.read_exact(&mut buf))
                    .await
                    .unwrap();
                assert_eq!(buf, vec![1, 2, 3, 4, 5])
            });
            let writer_handle = fiber::start_async(async move {
                timeout::timeout(_10_SEC, writer.write_all(&[1, 2, 3]))
                    .await
                    .unwrap();
                timeout::timeout(_10_SEC, writer.write_all(&[4, 5]))
                    .await
                    .unwrap();
            });
            writer_handle.join();
            reader_handle.join();
        }
        let buf = receiver.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(buf, vec![1, 2, 3, 4, 5])
    }

    #[crate::test(tarantool = "crate")]
    fn join_correct_timeout() {
        {
            fiber::block_on(async {
                let mut stream = TcpStream::connect("localhost", TARANTOOL_LISTEN)
                    .timeout(_10_SEC)
                    .await
                    .unwrap();
                // Read greeting
                let mut buf = vec![0; 128];
                let (is_err, is_ok) = futures::join!(
                    timeout::timeout(_0_SEC, always_pending()),
                    timeout::timeout(_10_SEC, stream.read_exact(&mut buf))
                );
                assert_eq!(is_err.unwrap_err().to_string(), "deadline expired");
                is_ok.unwrap();
            });
        }
        // Testing with different order in join
        {
            fiber::block_on(async {
                let mut stream = TcpStream::connect("localhost", TARANTOOL_LISTEN)
                    .timeout(_10_SEC)
                    .await
                    .unwrap();
                // Read greeting
                let mut buf = vec![0; 128];
                let (is_ok, is_err) = futures::join!(
                    timeout::timeout(_10_SEC, stream.read_exact(&mut buf)),
                    timeout::timeout(_0_SEC, always_pending())
                );
                assert_eq!(is_err.unwrap_err().to_string(), "deadline expired");
                is_ok.unwrap();
            });
        }
    }

    #[crate::test(tarantool = "crate")]
    fn select_correct_timeout() {
        {
            fiber::block_on(async {
                let mut stream = TcpStream::connect("localhost", TARANTOOL_LISTEN)
                    .timeout(_10_SEC)
                    .await
                    .unwrap();
                // Read greeting
                let mut buf = vec![0; 128];
                let f1 = timeout::timeout(_0_SEC, always_pending()).fuse();
                let f2 = timeout::timeout(_10_SEC, stream.read_exact(&mut buf)).fuse();
                futures::pin_mut!(f1);
                futures::pin_mut!(f2);
                let is_err = futures::select!(
                    res = f1 => res.is_err(),
                    res = f2 => res.is_err()
                );
                assert!(is_err);
            });
        }
        // Testing with different future timeouting first
        {
            fiber::block_on(async {
                let mut stream = TcpStream::connect("localhost", TARANTOOL_LISTEN)
                    .timeout(_10_SEC)
                    .await
                    .unwrap();
                // Read greeting
                let mut buf = vec![0; 128];
                let f1 = timeout::timeout(Duration::from_secs(15), always_pending()).fuse();
                let f2 = timeout::timeout(_10_SEC, stream.read_exact(&mut buf)).fuse();
                futures::pin_mut!(f1);
                futures::pin_mut!(f2);
                let is_ok = futures::select!(
                    res = f1 => res.is_ok(),
                    res = f2 => res.is_ok()
                );
                assert!(is_ok);
            });
        }
    }

    #[crate::test(tarantool = "crate")]
    fn select_works() {
        crate::fiber::block_on(async {
            let listener = std::net::TcpListener::bind("127.0.0.1:3304").unwrap();
            // Responsive listener
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let mut stream = stream.unwrap();
                    std::thread::yield_now(); /* make sure that unresponsive listener polled too */
                    <std::net::TcpStream as std::io::Write>::write(&mut stream, &[1, 2, 3])
                        .unwrap();
                }
            });

            let (tx, rx) = std::sync::mpsc::channel::<()>();
            let listener = std::net::TcpListener::bind("127.0.0.1:3305").unwrap();
            // Unresponsive listener
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    let _stream = stream.unwrap();
                    loop {
                        if let Ok(()) = rx.try_recv() {
                            break;
                        }
                    }
                }
            });

            let mut tcp = crate::network::client::tcp::TcpStream::connect("127.0.0.1", 3304)
                .await
                .unwrap();
            let mut buf_1 = vec![0; 3];
            let responsive_fut = tcp.read_exact(&mut buf_1).fuse();

            let mut tcp = crate::network::client::tcp::TcpStream::connect("127.0.0.1", 3305)
                .await
                .unwrap();
            let mut buf_2 = vec![0; 3];
            let unresponsive_fut = tcp.read_exact(&mut buf_2).fuse();

            futures::pin_mut!(unresponsive_fut);
            futures::pin_mut!(responsive_fut);
            let first_completed_future = futures::select! {
                _ = responsive_fut => {
                    // This future should be the first to complete
                    // and even though the other future hangs forever, `select` finishes as soon as the
                    // the first future does.
                    "responsive"
                    },
                _ = unresponsive_fut => {
                    "unresponsive"
                },
            };

            assert!(first_completed_future == "responsive");
            let _ = tx.send(());
        });
    }
}
