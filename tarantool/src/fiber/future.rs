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

pub struct Timer {
    fib: Option<UnitJoinHandle<'static>>,
    state: Rc<TimerState>,
}

impl Timer {
    pub fn new(duration: Duration) -> Self {
        let (state, state_fib) = Rc::new(TimerState::default()).into_clones();
        Self {
            state,
            fib: Some(start_proc(move || {
                let mut last_awoke = Instant::now();
                let mut time_left = duration;
                while !time_left.is_zero() {
                    sleep(time_left);
                    time_left = time_left.saturating_sub(last_awoke.elapsed());
                    last_awoke = Instant::now();
                }
                if let Some(waker) = state_fib.waker.take() {
                    // we have been polled before completion, so someone is
                    // expecting us to wake it up
                    waker.wake()
                }
                state_fib.is_complete.set(true);
            })),
        }
    }
}

impl Future for Timer {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, ctx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.state.is_complete.get() {
            // poll shouldn't be called on a ready future,
            // but better safe than sorry
            if let Some(jh) = self.fib.take() {
                // joinable fiber must be joined to cleanup resources
                jh.join()
            }
            Poll::Ready(())
        } else {
            let waker = match self.state.waker.take() {
                // been polled before and `waker` is still fresh
                Some(waker) if waker.will_wake(ctx.waker()) => waker,
                _ => ctx.waker().clone(),
            };
            self.state.waker.set(Some(waker));
            Poll::Pending
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
    fiber::{Cond, UnitJoinHandle, start_proc, sleep},
    util::IntoClones,
};

use std::{
    cell::{Cell, RefCell},
    future::Future,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};
