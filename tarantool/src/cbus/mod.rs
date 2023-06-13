#![cfg(any(feature = "picodata", doc))]

//! Tarantool cbus integration and channels.
//!
//! Original cbus provides a means of communication between separate threads:
//! - cpipe - channel between two cords. Cord is a separate thread with `libev` event loop inside it.
//! - lcpipe - channel between any arbitrary thread and cord.
//!
//! For the purposes of communication tx thread (where code of stored proc is working) and external threads
//! we should use a `lcpipe`.
//!
//! ## lcpipe schema
//!
//! Let's see how `lcpipe` woks, there are a number of entities participating in the exchange of messages:
//! - message - a unit of communication, may have a user defined payload
//! - message hop - defines how message will be handled on consumer side
//! - endpoint - message consumer, identified by name. Any endpoint occupies a single fiber for execute a cbus loop
//! - lcpipe - delivery message from producer to the consumer (endpoint), delivery never blocks consumer or producer
//!
//! Now schematically:
//!
//! ```text
//!                                            TX thread
//!                                      ┌────────────────────────┐
//! ┌───────────────┐ msg 1              │                        │
//! │               ├──────────  lcpipe 1│   ┌───────────┐        │
//! │   thread 1    │ msg 2     ─────────┼─► │endpoint 1 │fiber 1 │
//! │               ├──────────          │   └───────────┘        │
//! └───────────────┘                    │                        │
//!                              lcpipe 2│   ┌───────────┐        │
//! ┌───────────────┐           ─────────┼─► │endpoint 2 │fiber 2 │
//! │               │ msg 3   │          │   └───────────┘        │
//! │   thread 2    ├─────────┤          │                        │
//! │               ├─────────┤          │                        │
//! └───────────────┘ msg 4   │          │       ....             │
//!                           │          │                        │
//! ┌───────────────┐ msg 5   │          │   ┌───────────┐        │
//! │               ├─────────┤          │   │           │fiber N │
//! │   thread 3    ├─────────┘          │   └───────────┘        │
//! │               │ msg 6              │                        │
//! └───────────────┘                    │                        │
//!                                      └────────────────────────┘
//!```
//!
//! ## Cbus based channels
//!
//! The main idea of cbus based channels - use `lcpipe` to send a message about the need to unlock the consumer fiber.
//! Unlock consumer always means that there is a new data for consuming, but consumer not always locking
//! on try to receiver, if data is already available - lock is redundant.
//! For implementing a consumer lock and unlock a [`crate::fiber::Cond`] is used.

pub mod oneshot;
pub mod unbound;

use crate::ffi;
use crate::ffi::tarantool::{
    cbus_endpoint_delete, cbus_endpoint_new, cbus_loop, lcpipe_delete, lcpipe_new, lcpipe_push_now,
};
use std::ffi::CString;
use std::os::raw::c_void;
use std::ptr;

#[derive(Debug, thiserror::Error)]
pub enum RecvError {
    #[error("sending half of a channel is disconnected")]
    Disconnected,
}

#[derive(Debug, thiserror::Error)]
pub enum CbusError {
    #[error("failed to create cbus endpoint")]
    CreateEndpointFail,
}

#[repr(C)]
struct STailQEntry {
    next: *const STailQEntry,
}

/// One hop in a message travel route. Next destination defined by `_pipe` field,
/// but for `lcpipe` there is only one hop supported, so `_pipe` field must always be NULL.
#[repr(C)]
pub struct MessageHop<T> {
    f: fn(Box<Message<T>>),
    _pipe: *const c_void,
}

impl<T> MessageHop<T> {
    /// Create a hop.
    ///
    /// # Arguments
    ///
    /// * `f`: callback, called when consumer (cord) handle a message
    pub fn new(f: fn(Box<Message<T>>)) -> Self {
        Self {
            f,
            _pipe: ptr::null(),
        }
    }

    pub fn boxed(self) -> Box<Self> {
        Box::new(self)
    }
}

/// A message traveling between thread and cord.
#[repr(C)]
pub struct Message<T> {
    fifo: STailQEntry,
    route: *const MessageHop<T>,
    hop: *const MessageHop<T>,
    user_data: T,
}

impl<T> Message<T> {
    /// Create a new cbus message.
    ///
    /// # Arguments
    ///
    /// * `hop`: a message hop, define how message will be processed in consumer side
    /// * `user_data`: data received by the consumer
    pub fn new(hop: MessageHop<T>, user_data: T) -> Message<T> {
        // leaks hop, now it will be freed when message is dropped
        let hop = hop.boxed();
        let hop = Box::leak(hop);
        Message {
            fifo: STailQEntry { next: ptr::null() },
            route: hop as *const MessageHop<T>,
            hop: hop as *const MessageHop<T>,
            user_data,
        }
    }

    /// Return an underlying payload.
    pub fn user_data(&self) -> &T {
        &self.user_data
    }

    pub fn boxed(self) -> Box<Self> {
        Box::new(self)
    }
}

impl<T> Drop for Message<T> {
    fn drop(&mut self) {
        _ = unsafe { Box::from_raw(self.route as *mut MessageHop<T>) };
    }
}

/// Cbus endpoint. Endpoint is a message consumer on a cord side.
pub struct Endpoint {
    endpoint: *const (),
}

impl Endpoint {
    /// Create a new cbus endpoint
    ///
    /// # Arguments
    ///
    /// * `name`: endpoint name
    pub fn new(name: &str) -> Result<Self, CbusError> {
        let mut endpoint: *mut () = ptr::null_mut();
        let endpoint_ptr: *mut *mut () = &mut endpoint;
        let name = CString::new(name).unwrap();
        let err = unsafe { cbus_endpoint_new(endpoint_ptr as *mut *mut c_void, name.as_ptr()) };
        if err != 0 {
            return Err(CbusError::CreateEndpointFail);
        }

        Ok(Endpoint { endpoint })
    }

    /// Run the message delivery loop until the current fiber is cancelled.
    pub fn cbus_loop(&self) {
        unsafe { cbus_loop(self.endpoint as *mut c_void) }
    }
}

impl Drop for Endpoint {
    fn drop(&mut self) {
        let err = unsafe { cbus_endpoint_delete(self.endpoint as *mut c_void) };
        if err != 0 {
            panic!("failed to destroy cbus endpoint");
        }
    }
}

/// A uni-directional FIFO queue from any thread to cord.
pub struct LCPipe {
    pipe: *mut ffi::tarantool::LCPipe,
}

unsafe impl Send for LCPipe {}

unsafe impl Sync for LCPipe {}

impl LCPipe {
    /// Create and initialize a pipe and connect it to the consumer.
    /// The call returns only when the consumer, identified by endpoint name, has joined the bus.
    pub fn new(endpoint_name: &str) -> Self {
        let endpoint =
            CString::new(endpoint_name).expect("endpoint name may not contain interior null bytes");

        Self {
            pipe: unsafe { lcpipe_new(endpoint.as_ptr()) },
        }
    }

    /// Push a new message into pipe. Message will be flushed to consumer queue (but not handled) immediately.
    pub fn push_message<T>(&self, msg: Message<T>) {
        let msg = msg.boxed();
        // leaks a message, there is no `Box::from_raw` later, because it will happen implicitly
        // when [`MessageHop::f`] called
        let msg = Box::leak(msg);
        unsafe { lcpipe_push_now(self.pipe, msg as *mut Message<T> as *mut c_void) }
    }
}

impl Drop for LCPipe {
    fn drop(&mut self) {
        unsafe { lcpipe_delete(self.pipe) };
    }
}
