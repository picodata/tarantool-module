//! Async runtime based on Tarantool fibers.
//! Also includes sycnhronization primitives and useful traits for working with futures.
//!
//! Use [`block_on`] to start the runtime.
//! ## Example
//! ```no_run
//! async fn foo() {
//!     // do something
//! }
//!
//! use tarantool::fiber;
//! fiber::block_on(async {
//!     foo().await;
//!     // ... some other code
//! });
//! ```
//!
//! See also:
//! - Synchronization Primitives:
//!   - [`mutex`]
//! - Channels
//!   - [`oneshot`]
//!   - [`watch`]
//! - Extension Traits:
//!   - [`timeout::IntoTimeout`]
//!   - [`IntoOnDrop`]

use std::{future::Future, pin::Pin, rc::Rc, task::Poll, time::Duration};

use futures::pin_mut;

pub mod mutex;
pub mod oneshot;
pub mod timeout;
pub mod watch;

pub use mutex::Mutex;

/// Error that happens on the receiver side of the channel.
#[derive(thiserror::Error, Debug, PartialEq, Eq)]
#[error("sender dropped")]
pub struct RecvError;

mod waker {
    use crate::fiber;
    use std::rc::Rc;
    use std::task::RawWaker;
    use std::task::RawWakerVTable;
    use std::task::Waker;

    #[derive(Default)]
    pub struct FiberWaker {
        cond: fiber::Cond,
    }

    impl FiberWaker {
        pub fn cond(&self) -> &fiber::Cond {
            &self.cond
        }

        pub fn wake(&self) {
            self.cond.broadcast()
        }
    }

    unsafe impl Send for FiberWaker {}
    unsafe impl Sync for FiberWaker {}

    pub fn with_rcw(rcw: Rc<FiberWaker>) -> Waker {
        let raw_waker = raw_waker(rcw);
        unsafe { Waker::from_raw(raw_waker) }
    }

    fn raw_waker(rcw: Rc<FiberWaker>) -> RawWaker {
        const RC_WAKER_VT: RawWakerVTable = RawWakerVTable::new(
            rc_waker_clone,
            rc_waker_wake,
            rc_waker_wake_by_ref,
            rc_waker_drop,
        );
        let ptr: *const () = Rc::into_raw(rcw).cast();
        RawWaker::new(ptr, &RC_WAKER_VT)
    }

    unsafe fn rc_waker_clone(data: *const ()) -> RawWaker {
        let rcw: Rc<FiberWaker> = {
            // Clone it manually
            Rc::increment_strong_count(data);
            Rc::from_raw(data.cast())
        };
        raw_waker(rcw)
    }

    /// Represents `fn wake(self)`, must consume the data
    unsafe fn rc_waker_wake(data: *const ()) {
        let rcw: Rc<FiberWaker> = Rc::from_raw(data.cast());
        rcw.wake();
        drop(rcw);
    }

    /// Represents `fn wake_by_ref(&self)`, must NOT consume the data
    unsafe fn rc_waker_wake_by_ref(data: *const ()) {
        let rcw: Rc<FiberWaker> = Rc::from_raw(data.cast());
        rcw.wake();
        std::mem::forget(rcw);
    }

    unsafe fn rc_waker_drop(data: *const ()) {
        let rcw: Rc<FiberWaker> = Rc::from_raw(data.cast());
        drop(rcw)
    }
}

/// A wrapper around a future which has on_drop behavior.
/// See [`on_drop`].
pub struct OnDrop<Fut, Fun: FnOnce()> {
    future: Fut,
    on_drop: Option<Fun>,
}

impl<Fut: Future, Fun: FnOnce()> OnDrop<Fut, Fun> {
    #[inline]
    fn pin_get_future(self: Pin<&mut Self>) -> Pin<&mut Fut> {
        // This is okay because `future` is pinned when `self` is.
        unsafe { self.map_unchecked_mut(|s| &mut s.future) }
    }
}

impl<Fut: Future, Fun: FnOnce()> Future for OnDrop<Fut, Fun> {
    type Output = Fut::Output;

    fn poll(self: Pin<&mut Self>, cx: &mut std::task::Context<'_>) -> Poll<Self::Output> {
        self.pin_get_future().poll(cx)
    }
}

impl<Fut, Fun: FnOnce()> Drop for OnDrop<Fut, Fun> {
    fn drop(&mut self) {
        (self.on_drop.take().unwrap())()
    }
}

/// Adds a closure to the future, which will be executed when the future is dropped.
/// This can be useful to cleanup external resources on future cancelation or completion.
pub fn on_drop<Fut: Future, Fun: FnOnce()>(future: Fut, on_drop: Fun) -> OnDrop<Fut, Fun> {
    OnDrop {
        future,
        on_drop: Some(on_drop),
    }
}

/// Futures implementing this trait can attach a closure
/// which will be executed when the future is dropped.
/// See [`on_drop`].
pub trait IntoOnDrop: Future + Sized {
    /// Adds on_drop closure to a future. See [`on_drop`].
    #[inline]
    fn on_drop<Fun: FnOnce()>(self, on_drop: Fun) -> OnDrop<Self, Fun> {
        self::on_drop(self, on_drop)
    }
}

impl<T> IntoOnDrop for T where T: Future + Sized {}

/// Runs a future to completion on the fiber-based runtime. This is the async runtime’s entry point.
///
/// This runs the given future on the current fiber, blocking until it is complete, and yielding its resolved result.
///
/// For examples see module level documentation in [`super::async`].
pub fn block_on<F: Future>(future: F) -> F::Output {
    let rcw: Rc<waker::FiberWaker> = Default::default();
    let waker = waker::with_rcw(rcw.clone());

    let mut cx = std::task::Context::from_waker(&waker);

    pin_mut!(future);
    loop {
        if let Poll::Ready(output) = future.as_mut().poll(&mut cx) {
            return output;
        }

        // TODO: Do an unconditional `fiber_yield()`,
        // update channels, mutexes and etc. based on `cond` to support this
        rcw.cond().wait();
    }
}

/// An async friendly version of [fiber::sleep](crate::fiber::sleep). Prefer this version when working in async
/// contexts.
pub async fn sleep(time: Duration) {
    use timeout::IntoTimeout as _;

    // We can't just do a `fiber::sleep` as we need this to work well with other futures
    let (tx, rx) = oneshot::channel::<()>();
    rx.timeout(time).await.unwrap_err();
    drop(tx);
}

#[cfg(feature = "internal_test")]
mod tests {
    use std::cell::Cell;

    use super::timeout::IntoTimeout as _;
    use super::*;
    use crate::fiber;
    use crate::test::util::{always_pending, ok};

    #[crate::test(tarantool = "crate")]
    fn sleep_wakes_up() {
        let before_sleep = fiber::clock();
        let sleep_for = Duration::from_millis(100);

        let should_yield = fiber::check_yield(|| fiber::block_on(sleep(sleep_for)));

        assert_eq!(should_yield, fiber::YieldResult::Yielded(()));
        assert!(before_sleep.elapsed() >= sleep_for);
    }

    #[crate::test(tarantool = "crate")]
    fn on_drop_is_executed() {
        block_on(async {
            // Future is canceled
            let mut executed = false;
            always_pending()
                .on_drop(|| executed = true)
                .timeout(Duration::from_secs(0))
                .await
                .unwrap_err();
            assert!(executed);

            // Future completes
            let mut executed = false;
            std::future::ready(ok(()))
                .on_drop(|| executed = true)
                .timeout(Duration::from_secs(0))
                .await
                .unwrap();
            assert!(executed);
        });
    }

    #[crate::test(tarantool = "crate")]
    fn nested_on_drop_is_executed() {
        let executed = Rc::new(Cell::new(false));
        let executed_clone = executed.clone();
        let f = async { always_pending().on_drop(|| executed_clone.set(true)).await };
        block_on(async {
            f.timeout(Duration::from_secs(0)).await.unwrap_err();
        });
        assert!(executed.get());
    }
}
