pub struct Executor {

}

////////////////////////////////////////////////////////////////////////////////
// Timer
////////////////////////////////////////////////////////////////////////////////

#[derive(Default)]
pub struct TimerState {
    is_complete: Cell<bool>,
    waker: Cell<Option<Waker>>,
}

impl std::fmt::Debug for TimerState {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if let Some(waker) = self.waker.take() {
            let w = Some(&waker);
            let res = f.debug_struct("TimerState")
                .field("is_complete", &self.is_complete)
                .field("waker", &w)
                .finish();
            self.waker.set(Some(waker));
            res
        } else {
            f.debug_struct("TimerState")
                .field("is_complete", &self.is_complete)
                .field("waker", &None::<Waker>)
                .finish()
        }
    }
}

#[derive(Debug)]
pub enum Timer {
    Ready,
    Pending {
        fib: Option<UnitJoinHandle<'static>>,
        state: Rc<TimerState>,
    }
}

impl Timer {
    pub fn new(duration: Duration) -> Self {
        let (state, state_fib) = Rc::new(TimerState::default()).into_clones();
        Self::Pending {
            state,
            fib: Some(start_proc(move || {
                let mut last_awoke = Instant::now();
                let mut time_left = duration;
                while !time_left.is_zero() {
                    sleep(time_left);
                    time_left = time_left.saturating_sub(last_awoke.elapsed());
                    last_awoke = Instant::now();
                }
                state_fib.is_complete.set(true);
                if let Some(waker) = state_fib.waker.take() {
                    // we have been polled before completion, so someone is
                    // expecting us to wake it up
                    waker.wake()
                }
            })),
        }
    }
}

impl Future for Timer {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        match dbg!(&mut *self) {
            Self::Ready => Poll::Ready(()),
            Self::Pending { state, fib } => {
                if state.is_complete.get() {
                    if let Some(jh) = fib.take() {
                        // joinable fiber must be joined to cleanup resources
                        jh.join()
                    }
                    *self = Self::Ready;
                    Poll::Ready(())
                } else {
                    let waker = match state.waker.take() {
                        // been polled before and `waker` is still fresh
                        Some(waker) if waker.will_wake(ctx.waker()) => waker,
                        _ => ctx.waker().clone(),
                    };
                    state.waker.set(Some(waker));
                    Poll::Pending
                }
            }
        }
    }
}

impl Drop for Timer {
    fn drop(&mut self) {
        if let Self::Pending { fib, .. } = self {
            if let Some(jh) = fib.take() {
                jh.join()
            }
        }
    }
}

impl FusedFuture for Timer {
    fn is_terminated(&self) -> bool {
        matches!(&*self, Self::Ready)
    }
}

////////////////////////////////////////////////////////////////////////////////
/// Channel
////////////////////////////////////////////////////////////////////////////////

pub struct Channel<T> {
    pub inner: fiber::Channel<T>,
}

pub struct RecvState<T> {
    value: Cell<Option<Option<T>>>,
    waker: Cell<Option<Waker>>,
}

impl<T: std::fmt::Debug> std::fmt::Debug for RecvState<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        if let Some(waker) = self.waker.take() {
            let w = Some(&waker);
            let res = f.debug_struct("RecvState")
                .field("waker", &w)
                .finish();
            self.waker.set(Some(waker));
            res
        } else {
            f.debug_struct("RecvState")
                .field("waker", &None::<Waker>)
                .finish()
        }
    }
}

#[derive(Debug)]
pub enum Recv<'a, T> {
    Terminated,
    Ready(Option<T>),
    Pending {
        fib: Option<UnitJoinHandle<'a>>,
        state: Rc<RecvState<T>>
    },
}

impl<'a, T: Unpin + std::fmt::Debug> Future for Recv<'a, T> {
    type Output = Option<T>;

    fn poll(mut self: Pin<&mut Self>, ctx: &mut Context) -> Poll<Self::Output> {
        match dbg!(&mut *self) {
            Self::Terminated => unreachable!("wtf?"),
            Self::Ready(value) => {
                let value = value.take();
                *self = Self::Terminated;
                Poll::Ready(value)
            }
            Self::Pending { state, fib } => {
                if let Some(value) = state.value.take() {
                    if let Some(jh) = fib.take() {
                        jh.join()
                    }
                    *self = Self::Terminated;
                    Poll::Ready(value)
                } else {
                    let waker = match state.waker.take() {
                        Some(waker) if waker.will_wake(ctx.waker()) => waker,
                        _ => ctx.waker().clone(),
                    };
                    state.waker.set(Some(waker));
                    Poll::Pending
                }
            }
        }
    }
}

impl<'a, T: Unpin + std::fmt::Debug> FusedFuture for Recv<'a, T> {
    fn is_terminated(&self) -> bool {
        matches!(self, Self::Terminated)
    }
}

impl<'a, T> Drop for Recv<'a, T> {
    fn drop(&mut self) {
        if let Self::Pending { fib, .. } = self {
            if let Some(jh) = fib.take() {
                jh.join()
            }
        }
    }
}

impl<T> Channel<T> {
    pub fn recv(&self) -> Recv<T> {
        match self.inner.try_recv() {
            Ok(v) => Recv::Ready(Some(v)),
            Err(fiber::TryRecvError::Disconnected) => Recv::Ready(None),
            Err(fiber::TryRecvError::Empty) => {
                let (state, state_fib) = Rc::new(
                    RecvState { value: Cell::new(None), waker: Cell::new(None) }
                ).into_clones();
                Recv::Pending {
                    state,
                    fib: Some(start_proc(move || {
                        state_fib.value.set(Some(self.inner.recv()));
                        if let Some(waker) = state_fib.waker.take() {
                            waker.wake()
                        }
                    }))
                }
            }
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// RcWake
////////////////////////////////////////////////////////////////////////////////

pub trait RcWake {
    fn wake_by_ref(self: &Rc<Self>);

    fn wake(self: Rc<Self>) {
        self.wake_by_ref()
    }

    fn into_waker(self: Rc<Self>) -> Waker
    where
        Self: Sized,
    {
        unsafe { Waker::from_raw(imp::raw_waker(self)) }
    }
}

////////////////////////////////////////////////////////////////////////////////
// imp
////////////////////////////////////////////////////////////////////////////////

mod imp {
    pub(super) fn raw_waker<W>(w: Rc<W>) -> RawWaker
    where
        W: RcWake,
    {
        RawWaker::new(Rc::into_raw(w).cast(), raw_waker_vt::<W>())
    }

    pub fn raw_waker_vt<W>() -> &'static RawWakerVTable
    where
        W: RcWake,
    {
        return &RawWakerVTable::new(
            raw_clone::<W>,
            raw_wake::<W>,
            raw_wake_by_ref::<W>,
            raw_drop::<W>,
        );
    }

    unsafe fn raw_clone<W>(data: * const()) -> RawWaker
    where
        W: RcWake,
    {
        // ManuallyDrop means don't touch the refcount for the current reference
        let rc = ManuallyDrop::new(Rc::from_raw(data.cast::<W>()));
        // Increase refcount and move the result into the new RawWaker
        let res = Rc::clone(&rc);
        raw_waker::<W>(res)
    }

    unsafe fn raw_wake<W>(data: * const())
    where
        W: RcWake,
    {
        W::wake(Rc::from_raw(data.cast::<W>()))
    }

    unsafe fn raw_wake_by_ref<W>(data: * const())
    where
        W: RcWake,
    {
        // ManuallyDrop means don't touch the refcount for the current reference
        let rc = ManuallyDrop::new(Rc::from_raw(data.cast::<W>()));
        W::wake_by_ref(&rc);
    }

    unsafe fn raw_drop<W>(data: * const()) {
        drop(Rc::<W>::from_raw(data.cast::<W>()))
    }

    use super::RcWake;
    use std::{
        mem::ManuallyDrop,
        rc::Rc,
        task::{RawWaker, RawWakerVTable},
    };
}

////////////////////////////////////////////////////////////////////////////////
// use
////////////////////////////////////////////////////////////////////////////////

use crate::{
    fiber::{self, Cond, UnitJoinHandle, start_proc, sleep},
    util::IntoClones,
};

use futures::future::FusedFuture;

use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};
