//! Allows a future to execute for a maximum amount of time.
//!
//! See [`Timeout`] documentation for more details.
//!
//! [`Timeout`]: struct@Timeout
use std::future::Future;
use std::pin::Pin;
use std::task::Context;
use std::task::Poll;
use std::time::Duration;

use super::context::ContextExt;
use crate::fiber;
use crate::time::Instant;

/// Error returned by [`Timeout`]
#[derive(thiserror::Error, Debug, PartialEq, Eq)]
pub enum Error<E> {
    #[error("deadline expired")]
    Expired,
    #[error("{0}")]
    Failed(#[from] E),
}

pub type Result<T, E> = std::result::Result<T, Error<E>>;

/// Future returned by [`timeout`](timeout).
#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Timeout<F> {
    future: F,
    /// This flag allows to make one more poll
    /// to inner future after actual timeout, true by default
    extra_check: bool,
    deadline: Option<Instant>,
}

/// Requires a `Future` to complete before the specified duration has elapsed.
///
/// If the future completes before the duration has elapsed, then the completed
/// value is returned. Otherwise, an error is returned and the future is
/// canceled.
///
/// A `timeout` equal to [`Duration::ZERO`] guarantees that awaiting this future
/// will **not** result in a fiber yield.
///
/// ```no_run
/// use tarantool::fiber::r#async::*;
/// use tarantool::fiber;
/// use std::time::Duration;
///
/// let (tx, rx) = oneshot::channel::<i32>();
///
/// // Wrap the future with a `Timeout` set to expire in 10 milliseconds.
/// if let Err(_) = fiber::block_on(timeout::timeout(Duration::from_millis(10), rx)) {
///     println!("did not receive value within 10 ms");
/// }
/// ```
#[inline(always)]
pub fn timeout<F: Future>(timeout: Duration, f: F) -> Timeout<F> {
    Timeout {
        future: f,
        extra_check: true,
        deadline: fiber::clock().checked_add(timeout),
    }
}

/// Like [`timeout`], but with an explicit deadline.
#[inline(always)]
pub fn deadline<F: Future>(deadline: Instant, f: F) -> Timeout<F> {
    Timeout {
        future: f,
        extra_check: true,
        deadline: Some(deadline),
    }
}

impl<F: Future> Timeout<F> {
    /// Disable extra check after timeout
    pub fn no_extra_check(self) -> Self {
        let mut timeout = self;
        timeout.extra_check = false;
        timeout
    }

    #[inline]
    fn pin_get_future(self: Pin<&mut Self>) -> Pin<&mut F> {
        // This is okay because `future` is pinned when `self` is.
        unsafe { self.map_unchecked_mut(|s| &mut s.future) }
    }
}

impl<F, T, E> Future for Timeout<F>
where
    F: Future<Output = std::result::Result<T, E>>,
{
    type Output = Result<T, E>;
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let deadline = match self.deadline {
            Some(v) => v,
            // Wait forever
            None => return Poll::Pending,
        };

        let extra_check = self.extra_check;

        if fiber::clock() >= deadline {
            // Even though we have already timed out
            // By default we poll inner future one more time
            if extra_check {
                if let Poll::Ready(v) = self.pin_get_future().poll(cx) {
                    return Poll::Ready(v.map_err(Error::Failed));
                }
            }
            return Poll::Ready(Err(Error::Expired));
        }

        // First, try polling the future
        if let Poll::Ready(v) = self.pin_get_future().poll(cx) {
            return Poll::Ready(v.map_err(Error::Failed));
        }

        // SAFETY: This is safe as long as the `Context` really
        // is the `ContextExt`. It's always true within provided
        // `block_on` async runtime.
        unsafe { ContextExt::set_deadline(cx, deadline) };
        Poll::Pending
    }
}

/// Futures implementing this trait can be constrained with a timeout (see
/// [`Timeout`]).
///
/// **NOTE**: this trait is implemented for all type implementing
/// [`std::future::Future`], but it must be used **only** with futures from
/// [`crate::fiber::async`] otherwise the behaviour is undefined.
pub trait IntoTimeout: Future + Sized {
    /// Adds timeout to a future. See [`Timeout`].
    #[inline(always)]
    fn timeout(self, timeout: Duration) -> Timeout<Self> {
        self::timeout(timeout, self)
    }

    /// Adds a deadline to the future. See [`Timeout`].
    #[inline(always)]
    fn deadline(self, deadline: Instant) -> Timeout<Self> {
        self::deadline(deadline, self)
    }
}

impl<T> IntoTimeout for T where T: Future + Sized {}

#[cfg(feature = "internal_test")]
mod tests {
    use super::*;
    use crate::fiber;
    use crate::fiber::check_yield;
    use crate::fiber::r#async::{oneshot, RecvError};
    use crate::fiber::YieldResult::{DidntYield, Yielded};
    use crate::test::util::ok;
    use std::time::Duration;

    const _0_SEC: Duration = Duration::ZERO;
    const _1_SEC: Duration = Duration::from_secs(1);

    #[crate::test(tarantool = "crate")]
    fn instant_future() {
        let fut = async { 78 };
        assert_eq!(fiber::block_on(fut), 78);

        let fut = timeout(Duration::ZERO, async { ok(79) });
        assert_eq!(fiber::block_on(fut), Ok(79));
    }

    #[crate::test(tarantool = "crate")]
    fn actual_timeout_promise() {
        let (tx, rx) = oneshot::channel::<i32>();
        let fut = async move { rx.timeout(_0_SEC).await };

        let jh = fiber::start_async(fut);
        assert_eq!(jh.join(), Err(Error::Expired));
        drop(tx);
    }

    #[crate::test(tarantool = "crate")]
    fn drop_tx_before_timeout() {
        let (tx, rx) = oneshot::channel::<i32>();
        let fut = async move { rx.timeout(_1_SEC).await };

        let jh = fiber::start(move || fiber::block_on(fut));
        drop(tx);
        assert_eq!(jh.join(), Err(Error::Failed(RecvError)));
    }

    #[crate::test(tarantool = "crate")]
    fn send_tx_before_timeout() {
        let (tx, rx) = oneshot::channel::<i32>();
        let fut = async move { rx.timeout(_1_SEC).await };

        let jh = fiber::start(move || fiber::block_on(fut));
        tx.send(400).unwrap();
        assert_eq!(jh.join(), Ok(400));
    }

    #[crate::test(tarantool = "crate")]
    fn timeout_duration_max() {
        // must not panic
        fiber::block_on(timeout(Duration::MAX, async { ok(1) })).unwrap();
    }

    #[crate::test(tarantool = "crate")]
    fn await_actually_yields() {
        // ready future, no timeout -> no yield
        assert_eq!(
            check_yield(|| fiber::block_on(async { 101 })),
            DidntYield(101)
        );

        // ready future, 0 timeout -> no yield
        assert_eq!(
            check_yield(|| fiber::block_on(timeout(Duration::ZERO, async { ok(202) }))),
            DidntYield(Ok(202))
        );

        // ready future, positive timeout -> no yield
        assert_eq!(
            check_yield(|| fiber::block_on(timeout(Duration::from_secs(1), async { ok(303) }))),
            DidntYield(Ok(303))
        );

        // pending future, no timeout -> yield
        let (_tx, rx) = oneshot::channel::<i32>();
        let f = check_yield(|| fiber::start(|| fiber::block_on(rx)));
        // the yield happens as soon as fiber::start is called,
        // but if fiber::block_on didn't yield we wouldn't even get here,
        // so this check is totally legit
        assert!(matches!(f, Yielded(_)));
        // we leak some memory here, but avoid a panic.
        // Don't do this in your code
        std::mem::forget(f);

        // pending future, 0 timeout -> no yield
        let (_tx, rx) = oneshot::channel::<i32>();
        assert_eq!(
            check_yield(|| fiber::block_on(timeout(Duration::ZERO, rx))),
            DidntYield(Err(Error::Expired))
        );

        // pending future, deadline is now -> no yield
        let (_tx, rx) = oneshot::channel::<i32>();
        let now = fiber::clock();
        assert_eq!(
            check_yield(|| fiber::block_on(deadline(now, rx))),
            DidntYield(Err(Error::Expired))
        );

        // pending future, deadline is past -> no yield
        let (_tx, rx) = oneshot::channel::<i32>();
        let one_second_ago = now.saturating_sub(Duration::from_secs(1));
        assert_eq!(
            check_yield(|| fiber::block_on(deadline(one_second_ago, rx))),
            DidntYield(Err(Error::Expired))
        );

        // pending future, positive timeout -> yield
        let (_tx, rx) = oneshot::channel::<i32>();
        assert_eq!(
            check_yield(|| fiber::block_on(timeout(Duration::from_millis(10), rx))),
            Yielded(Err(Error::Expired))
        );

        // pending future, deadline in future -> yield
        let (_tx, rx) = oneshot::channel::<i32>();
        let in_10_millis = fiber::clock().saturating_add(Duration::from_millis(10));
        assert_eq!(
            check_yield(|| fiber::block_on(deadline(in_10_millis, rx))),
            Yielded(Err(Error::Expired))
        );
    }
}
