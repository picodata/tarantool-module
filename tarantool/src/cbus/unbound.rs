use super::{LCPipe, Message};
use crate::cbus::RecvError;
use crate::fiber::Cond;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// A synchronization component between producers and a consumer.
struct Waker {
    /// synchronize a waker, signal when waker is up to date
    condition: Arc<Cond>,
    /// indicate that waker already up to date
    woken: AtomicBool,
}

impl Waker {
    fn new(cond: Cond) -> Self {
        Self {
            condition: Arc::new(cond),
            woken: AtomicBool::new(false),
        }
    }

    /// Release waker if it lock in [`Waker::wait`].
    fn wakeup(&self, pipe: &LCPipe) {
        let do_wake = self
            .woken
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_ok();
        if do_wake {
            let cond = Arc::clone(&self.condition);
            let msg = Message::new(move || {
                cond.signal();
            });
            pipe.push_message(msg);
        }
    }

    /// Lock until waker is woken up, or return instantly if waker already woken.
    fn wait(&self) {
        while self
            .woken
            .compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            self.condition.wait();
        }
    }
}

/// A unbound mpsc channel based on tarantool cbus.
/// This a channel between any arbitrary threads (producers) and a cord (consumer).
/// Cord - a thread with `libev` event loop inside (typically tx thread).
pub struct Channel<T> {
    /// [`crossbeam_queue::SegQueue`] is used as lock free buffer, internally this is a linked list with buckets
    list: crossbeam_queue::SegQueue<T>,
    /// synchronize receiver and producers
    waker: Waker,
    /// indicate that all producers are disconnected from channel
    disconnected: AtomicBool,
}

impl<T> Channel<T> {
    /// Create a new channel.
    pub fn new() -> Self {
        let cond = Cond::new();
        Self {
            list: crossbeam_queue::SegQueue::new(),
            waker: Waker::new(cond),
            disconnected: AtomicBool::new(false),
        }
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
    /// use tarantool::cbus::unbound;
    /// let chan = unbound::Channel::new();
    /// let (sender, receiver) = chan.split("some_endpoint");
    /// ```
    pub fn split(self, cbus_endpoint: &str) -> (Sender<T>, Receiver<T>) {
        let chan = Arc::new(self);
        let s = SenderInner {
            chan: Arc::clone(&chan),
            pipe: LCPipe::new(cbus_endpoint),
        };
        let r = Receiver {
            chan: Arc::clone(&chan),
        };
        (Sender { inner: Arc::new(s) }, r)
    }
}

impl<T> Default for Channel<T> {
    fn default() -> Self {
        Self::new()
    }
}

struct SenderInner<T> {
    chan: Arc<Channel<T>>,
    pipe: LCPipe,
}

unsafe impl<T> Send for SenderInner<T> {}

impl<T> Drop for SenderInner<T> {
    fn drop(&mut self) {
        self.chan.disconnected.store(true, Ordering::Release);
        self.chan.waker.wakeup(&self.pipe);
    }
}

/// A sending-half of unbound channel. Can be used in any context (tarantool cord or arbitrary thread).
/// Messages can be sent through this channel with [`Sender::send`].
/// Clone the sender if you need one more producer.
#[derive(Clone)]
pub struct Sender<T> {
    inner: Arc<SenderInner<T>>,
}

unsafe impl<T> Send for Sender<T> {}

impl<T> Sender<T> {
    /// Attempts to send a value on this channel.
    ///
    /// # Arguments
    ///
    /// * `message`: message to send
    pub fn send(&self, msg: T) {
        self.inner.chan.list.push(msg);
        // wake up a sleeping receiver
        self.inner.chan.waker.wakeup(&self.inner.pipe);
    }
}

/// Receiver part of unbound channel. Must be used in cord context.
pub struct Receiver<T> {
    chan: Arc<Channel<T>>,
}

unsafe impl<T> Send for Receiver<T> {}

impl<T> Receiver<T> {
    /// Attempts to wait for a value on this receiver, returns a [`RecvError::Disconnected`]
    /// when all of producers are dropped.
    pub fn receive(&self) -> Result<T, RecvError> {
        loop {
            if let Some(msg) = self.chan.list.pop() {
                return Ok(msg);
            }

            if self.chan.disconnected.load(Ordering::Acquire) {
                return Err(RecvError::Disconnected);
            }

            self.chan.waker.wait();
        }
    }

    /// Return message count in receiver buffer.
    pub fn len(&self) -> usize {
        self.chan.list.len()
    }

    /// Return true if receiver message buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
