#![allow(non_camel_case_types)]
#![allow(non_upper_case_globals)]
#![allow(non_snake_case)]
use std::os::raw::{c_char, c_int, c_void};

use bitflags::bitflags;
use va_list::VaList;

use super::fiber;

// Cbus lcpipe.
#[cfg(feature = "picodata")]
#[repr(C)]
pub struct LCPipe {
    _unused: [u8; 0],
}

#[cfg(feature = "picodata")]
extern "C" {
    pub fn lcpipe_new(name: *const c_char) -> *mut LCPipe;
    pub fn lcpipe_push_now(lcpipe: *mut LCPipe, cmsg: *mut c_void);
    pub fn lcpipe_delete(lcpipe: *mut LCPipe);
    pub fn cbus_endpoint_new(endpoint: *mut *mut c_void, name: *const c_char) -> c_int;
    pub fn cbus_endpoint_delete(endpoint: *mut c_void) -> c_int;
    pub fn cbus_loop(endpoint: *mut c_void);
    pub fn cbus_process(endpoint: *mut c_void);
}

// COIO.
bitflags! {
    /// Event type(s) to wait. Can be `READ` or/and `WRITE`
    pub struct CoIOFlags: c_int {
        const READ = 1;
        const WRITE = 2;
    }
}

// Functions requiring VaList.
// Unfortunately bindgen cant deal with VaLists automatically.
// See
//    https://github.com/rust-lang/rust/issues/44930
//    https://github.com/rust-lang/rust-bindgen/issues/2631
//    https://github.com/rust-lang/rust-bindgen/issues/2154
extern "C" {
    pub fn coio_call(func: Option<unsafe extern "C" fn(VaList) -> c_int>, ...) -> isize;
}

pub type FiberFunc = Option<unsafe extern "C" fn(VaList) -> c_int>;

extern "C" {
    pub static mut log_write_flightrec: ::std::option::Option<
        unsafe extern "C" fn(
            level: ::std::os::raw::c_int,
            filename: *const ::std::os::raw::c_char,
            line: ::std::os::raw::c_int,
            error: *const ::std::os::raw::c_char,
            format: *const ::std::os::raw::c_char,
            ap: VaList,
        ),
    >;
}

pub(crate) const TUPLE_FIELD_BY_PATH_OLD_API: &str = "tuple_field_raw_by_full_path\0";
pub(crate) const TUPLE_FIELD_BY_PATH_NEW_API: &str = "box_tuple_field_by_path\0";

// FIXME: we shouldnt need that. Tuple needs to be exported so bindgen
// is able to see it. Note that in order for that to be usable
//we need to be able to pass different header files to it.
// module.h of vanilla tarantool vs its picodata counterpart

#[cfg(not(feature = "picodata"))]
#[repr(C)]
pub struct BoxTuple {
    refs: u16,
    format_id: u16,
    bsize: u32,
    data_offset: u16,
}

#[cfg(feature = "picodata")]
#[repr(C, packed)]
pub struct BoxTuple {
    refs: u8,
    _flags: u8,
    format_id: u16,
    data_offset: u16,
    bsize: u32,
}

/// list entry and head structure
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct rlist {
    pub prev: *mut rlist,
    pub next: *mut rlist,
}

/// Channel - fiber communication media.
///
/// A channel is a media to deliver messages between fibers.
/// Any fiber can read or write to/from a channel. Many
/// readers and writers can work with a channel concurrently.
/// A message sent to a channel is read by the first fiber
/// reading from it. If a channel is empty, the reader blocks
/// and waits for a message. If a channel has no reader, the
/// writer waits for the reader to arrive. If a channel is
/// buffered, i.e. has an associated buffer for messages, it
/// is possible for a writer to "drop" the message in a channel
/// until a writer arrives. In case of multiple readers,
/// messages are delivered in FIFO order. In case of multiple
/// writers, the first writer to come is released of its message
/// first.
///
/// If a channel has a buffer of size N, and the buffer
/// is full (contains N messages), and there is a queue of writers,
/// the moment the first reader arrives and reads the first message
/// from a buffer, the first fiber from the wait queue is awoken,
/// and puts its message to the end of the buffer.
///
/// A channel, once created is "open". I.e. anyone can read or
/// write to/from a channel. A channel can be closed at any time,
/// in which case, all messages currently buffered in a channel
/// are destroyed, waiting readers or writers awoken with an error.
///
/// Waiting for a message, a reader, or space in a buffer can also
/// return error in case of a wait timeout or cancellation (when the
/// waiting fiber is cancelled).
///
/// Sending a message to a closed channel, as well as reading
/// a message from such channel, always fails.
///
/// Channel memory layout
/// ---------------------
/// Channel structure has a fixed size. If a channel is created
/// with a buffer, the buffer must be allocated in a continuous
/// memory chunk, directly after the channel itself.
/// fiber_channel_memsize() can be used to find out the amount
/// of memory necessary to store a channel, given the desired
/// buffer size.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct fiber_channel {
    /// Channel buffer size, if the channel is buffered.
    pub size: u32,

    /// The number of messages in the buffer.
    pub count: u32,

    /// Readers blocked waiting for messages while the channel
    /// buffers is empty and/or there are no writers, or
    /// Writers blocked waiting for empty space while the
    /// channel buffer is full and/or there are no readers.
    pub waiters: rlist,

    /// Ring buffer read position.
    pub beg: u32,

    pub is_closed: bool,

    /// Channel buffer, if any.
    pub buf: *mut *mut ipc_msg,
}

/// A base structure for an IPC message.
///
/// A message at any moment can be either:
/// - new
/// - in a channel, waiting to get delivered
/// - delivered
///
/// When a channel is destroyed, all messages buffered by the
/// channel must be destroyed as well. The destroy callback is
/// therefore necessary to free any message-specific resources in
/// case of delivery failure.
#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct ipc_msg {
    pub destroy: Option<unsafe extern "C" fn(*mut ipc_msg)>,
}

pub type ev_tstamp = f64;

crate::define_dlsym_reloc! {
    /// Allocate and construct a channel.
    ///
    /// Uses `malloc()`.
    ///
    /// - `size`:  of the channel buffer
    /// - returns: new channel
    ///
    /// ```no_run
    /// use tarantool::ffi::tarantool::fiber_channel_new;
    /// let ch = unsafe { fiber_channel_new(10) };
    /// ```
    pub fn fiber_channel_new(size: u32) -> *mut fiber_channel;

    /// Destroy and free an IPC channel.
    ///
    /// - `ch`: channel
    pub fn fiber_channel_delete(ch: *mut fiber_channel);

    /// Destroy all buffered messages and close the channel.
    ///
    /// - `ch`: channel
    pub fn fiber_channel_close(ch: *mut fiber_channel);

    /// Send a message over a channel within given time.
    ///
    /// - `channel`
    /// - `msg`:  a message with a custom destructor
    /// - `timeout`
    /// **Returns:**
    /// - `0`: success
    /// - `-1`: failure
    ///     - errno=ETIMEDOUT if timeout exceeded,
    ///     - errno=ECANCEL if the fiber is cancelled
    ///     - errno=EBADF if the channel is closed while waiting on it.
    ///
    pub fn fiber_channel_put_msg_timeout(
        ch: *mut fiber_channel,
        msg: *mut ipc_msg,
        timeout: ev_tstamp,
    ) -> c_int;

    /// Get a message from the channel, or time out.
    /// The caller is responsible for message destruction.
    /// **Returns:**
    /// - `0`: success
    /// - `-1`: failure (timeout or channel is closed)
    pub fn fiber_channel_get_msg_timeout(
        ch: *mut fiber_channel,
        msg: *mut *mut ipc_msg,
        timeout: ev_tstamp,
    ) -> c_int;

    /// Check if the channel has reader fibers that wait
    /// for new messages.
    pub fn fiber_channel_has_readers(ch: *mut fiber_channel) -> bool;

    /// Check if the channel has writer fibers that wait
    /// for readers.
    pub fn fiber_channel_has_writers(ch: *mut fiber_channel) -> bool;

    /// Set a pointer to context for the fiber. Can be used to avoid calling
    /// `fiber_start` which means no yields.
    ///
    /// `f`    fiber to set the context for
    /// `ctx`  context for the fiber function
    pub fn fiber_set_ctx(f: *mut fiber, ctx: *mut c_void);

    /// Get the context for the fiber which was set via the `fiber_set_ctx`
    /// function. Can be used to avoid calling `fiber_start` which means no yields.
    ///
    /// Returns context for the fiber function set by `fiber_set_ctx` function
    ///
    /// See also [`fiber_set_ctx`].
    pub fn fiber_get_ctx(f: *mut fiber) -> *mut c_void;

}

/// Channel buffer size.
///
/// # Safety
/// `ch` must point to a valid instance of [`fiber_channel`]
#[inline(always)]
pub unsafe fn fiber_channel_size(ch: *mut fiber_channel) -> u32 {
    (*ch).size
}

/// The number of messages in the buffer.
/// There may be more messages outstanding
/// if the buffer is full.
///
/// # Safety
/// `ch` must point to a valid instance of [`fiber_channel`]
#[inline(always)]
pub unsafe fn fiber_channel_count(ch: *mut fiber_channel) -> u32 {
    (*ch).count
}

/// True if the channel is closed for both for reading
/// and writing.
///
/// # Safety
/// `ch` must point to a valid instance of [`fiber_channel`]
#[inline(always)]
pub unsafe fn fiber_channel_is_closed(ch: *mut fiber_channel) -> bool {
    (*ch).is_closed
}

/// # Safety
/// `ch` must point to a valid instance of [`fiber_channel`]
#[inline(always)]
pub unsafe fn fiber_channel_is_empty(ch: *mut fiber_channel) -> bool {
    (*ch).count == 0
}

#[repr(C)]
#[derive(Copy, Clone)]
pub union ipc_data {
    pub data: *mut c_void,
    pub i: c_int,
}

/// A message implementation to pass simple value across
/// a channel.
#[repr(C)]
#[derive(Copy, Clone)]
pub struct ipc_value {
    pub base: ipc_msg,
    pub data_union: ipc_data,
}

crate::define_dlsym_reloc! {
    pub fn ipc_value_new() -> *mut ipc_value;
    pub fn ipc_value_delete(msg: *mut ipc_msg);
}

extern "C" {
    #[link_name = "log_level"]
    pub static mut LOG_LEVEL: c_int;

    // #[link_name = "_say"]
    // pub static mut SAY_FN: SayFunc;
}
