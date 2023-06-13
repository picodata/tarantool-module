use super::{LCPipe, Message, MessageHop};
use crate::cbus::RecvError;
use crate::fiber::Cond;
use std::cell::UnsafeCell;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A oneshot channel based on tarantool cbus. This a channel between any arbitrary thread and a cord.
/// Cord - a thread with `libev` event loop inside (typically tx thread).
pub struct Channel<T> {
    message: UnsafeCell<Option<T>>,
    /// Condition variable for synchronize consumer (cord) and producer,
    /// using an [`Arc`] instead of raw pointer cause there is a situation
    /// when channel dropped before cbus endpoint receive a cond
    cond: Arc<Cond>,
    /// Atomic flag, signaled that sender already have a data for receiver
    ready: AtomicBool,
}

unsafe impl<T> Sync for Channel<T> where T: Send {}

unsafe impl<T> Send for Channel<T> where T: Send {}

/// A sending-half of oneshot channel. Can be used in any context (tarantool cord or arbitrary thread).
/// Messages can be sent through this channel with [`Sender::send`].
///
/// If sender dropped before [`Sender::send`] is calling then [`Receiver::receive`] will return with [`RecvError::Disconnected`].
/// It is safe to drop sender when [`Receiver::receive`] is not calling.
pub struct Sender<T> {
    channel: Arc<Channel<T>>,
    pipe: Arc<LCPipe>,
}

/// Receiver part of oneshot channel. Must be used in cord context.
pub struct Receiver<T> {
    channel: Arc<Channel<T>>,
}

impl<T> Channel<T> {
    /// Create a new channel.
    pub fn new() -> Self {
        Self {
            message: UnsafeCell::new(None),
            ready: AtomicBool::new(false),
            cond: Arc::new(Cond::new()),
        }
    }

    /// Split channel into sender and receiver parts with already created [`LCPipe`] instance.
    /// This method is useful if you want to avoid any memory allocations.
    /// Typically better use a [`Self::split_on_pipe`] method that create a new lcpipe instance,
    /// lcpipe is pretty small structure so overhead is not big.
    ///
    /// # Arguments
    ///
    /// * `pipe`: lcpipe - a cbus communication channel
    ///
    /// returns: (Sender<T>, Receiver<T>)
    ///
    /// # Examples
    ///
    /// ```no_run,ignore
    /// use std::sync::Arc;
    /// use tarantool::cbus::oneshot;
    /// use tarantool::ffi::cbus::LCPipe;
    ///
    /// let pipe = LCPipe::new("some_endpoint");
    /// let chan = oneshot::Channel::new();
    /// let (sender, receiver) = chan.split_on_pipe(Arc::new(pipe));
    /// ```
    pub fn split_on_pipe(self, pipe: Arc<LCPipe>) -> (Sender<T>, Receiver<T>) {
        let channel = Arc::new(self);
        (
            Sender {
                channel: channel.clone(),
                pipe,
            },
            Receiver { channel },
        )
    }

    /// Split channel into sender and receiver parts.
    ///
    /// # Arguments
    ///
    /// * `cbus_endpoint`: cbus endpoint name. Note that the tx thread (or any other cord)
    /// must have a fiber occupied by the endpoint cbus_loop.
    ///
    /// returns: (Sender<T>, Receiver<T>)
    ///
    /// # Examples
    ///
    /// ```no_run,ignore
    /// use tarantool::cbus::oneshot;
    /// let chan = oneshot::Channel::new();
    /// let (sender, receiver) = chan.split("some_endpoint");
    /// ```
    pub fn split(self, cbus_endpoint: &str) -> (Sender<T>, Receiver<T>) {
        self.split_on_pipe(Arc::new(LCPipe::new(cbus_endpoint)))
    }
}

impl<T> Sender<T> {
    /// Attempts to send a value on this channel.
    ///
    /// # Arguments
    ///
    /// * `message`: message to send
    pub fn send(self, message: T) {
        unsafe { *self.channel.message.get() = Some(message) };
        self.channel.ready.store(true, Ordering::Release);
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let hop = MessageHop::new(|b: Box<Message<Arc<Cond>>>| {
            let cond = b.user_data();
            cond.signal();
        });
        let msg = Message::new(hop, Arc::clone(&self.channel.cond));
        self.pipe.push_message(msg);
    }
}

impl<T> Receiver<T> {
    /// Attempts to wait for a value on this receiver, returns a [`RecvError`]
    /// if the corresponding channel has hung up (sender was dropped).
    pub fn receive(self) -> Result<T, RecvError> {
        if !self.channel.ready.swap(false, Ordering::Acquire) {
            // assume that situation when [`crate::fiber::Cond::signal()`] called before
            // [`crate::fiber::Cond::wait()`] and after swap `ready` to false  is never been happen,
            // cause signal and wait both calling in tx thread (or any other cord) and there is now yields between it
            self.channel.cond.wait();
        }
        unsafe { self.channel.message.get().as_mut().unwrap().take() }
            .ok_or(RecvError::Disconnected)
    }
}

impl<T> Default for Channel<T> {
    fn default() -> Self {
        Self::new()
    }
}
