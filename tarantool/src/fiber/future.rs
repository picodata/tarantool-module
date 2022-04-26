////////////////////////////////////////////////////////////////////////////////
// Executor
////////////////////////////////////////////////////////////////////////////////

#[derive(Default)]
pub struct Executor {
    queue: UnsafeCell<VecDeque<Task>>,
    deadlines: UnsafeCell<Vec<Instant>>,
    has_new_tasks: Cell<bool>,
    pub cond: Rc<fiber::Cond>,
}

impl Executor {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn next_wait_since(&self, now: Instant) -> Option<Duration> {
        let mut min_dl = None;
        unsafe { &mut *self.deadlines.get() }.retain(|&dl|
            now <= dl && {
                if min_dl.map(|min| dl < min).unwrap_or(true) {
                    min_dl = Some(dl)
                }
                true
            }
        );
        min_dl.map(|dl| dl - now)
    }

    pub fn next_wait(&self) -> Option<Duration> {
        let now = Instant::now();
        self.next_wait_since(now)
    }

    pub fn has_tasks(&self) -> bool {
        !unsafe { &*self.queue.get() }.is_empty()
    }

    #[track_caller]
    pub fn spawn(&self, future: impl Future<Output = ()> + 'static) {
        self.has_new_tasks.set(true);
        unsafe { &mut *self.queue.get() }.push_back(
            Task {
                future: Box::pin(future),
                loc: std::panic::Location::caller(),
            }
        );
    }

    pub async fn sleep(&self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        unsafe { &mut *self.deadlines.get() }.push(deadline);
        Sleep { deadline }.await
    }

    pub fn do_loop(&self) {
        let queue = unsafe { &mut *self.queue.get() };
        let mut first_time = true;
        while first_time || self.has_new_tasks.get() {
            first_time = false;
            self.has_new_tasks.set(false);
            // only iterate over tasks pushed before this function was called
            for _ in 0..queue.len() {
                let mut task = queue.pop_front().unwrap();
                eprint!("task @ {} ", task.loc);
                let waker = Rc::new(Waker(self.cond.clone())).into_waker();
                match task.future.as_mut().poll(&mut Context::from_waker(&waker)) {
                    Poll::Pending => {
                        eprintln!("pending");
                        // this tasks will be checked on the next iteration
                        queue.push_back(task)
                    }
                    Poll::Ready(()) => {
                        eprintln!("ready");
                    }
                }
            }
        }
        const DEFAULT_WAIT: Duration = Duration::from_secs(3);
        if !queue.is_empty() {
            self.cond.wait_timeout(dbg!(self.next_wait().unwrap_or(DEFAULT_WAIT)));
        }
    }

    #[track_caller]
    pub fn block_on<T: 'static>(&self, future: impl Future<Output = T> + 'static) -> T {
        let (tx, rx) = channel(1);
        let (cond_tx, cond_rx) = Rc::new(Cond::new()).into_clones();
        self.spawn(async move {
            tx.send(future.await).await.unwrap();
            cond_tx.signal()
        });
        cond_rx.wait();
        rx.do_recv().unwrap()
    }
}

////////////////////////////////////////////////////////////////////////////////
// Task
////////////////////////////////////////////////////////////////////////////////

struct Task {
    future: Pin<Box<dyn Future<Output = ()>>>,
    loc: &'static std::panic::Location<'static>,
}

////////////////////////////////////////////////////////////////////////////////
// Waker
////////////////////////////////////////////////////////////////////////////////

pub struct Waker<T>(pub T);

impl RcWake for Waker<Cell<bool>> {
    fn wake_by_ref(self: &Rc<Self>) {
        (**self).0.set(true)
    }
}

impl<'a> RcWake for Waker<&'a fiber::Cond> {
    fn wake_by_ref(self: &Rc<Self>) {
        self.0.signal()
    }
}

impl<'a> RcWake for Waker<Rc<fiber::Cond>> {
    fn wake_by_ref(self: &Rc<Self>) {
        self.0.signal()
    }
}

////////////////////////////////////////////////////////////////////////////////
// Sleep
////////////////////////////////////////////////////////////////////////////////

struct Sleep {
    deadline: Instant,
}

impl Future for Sleep {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _: &mut Context) -> Poll<Self::Output> {
        if self.deadline <= Instant::now() {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// Timer
////////////////////////////////////////////////////////////////////////////////

#[derive(Default)]
pub struct TimerState {
    is_complete: Cell<bool>,
    waker: Cell<Option<task::Waker>>,
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
                .field("waker", &None::<task::Waker>)
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

struct Channel<T> {
    data: VecDeque<T>,
    tx_count: usize,
    rx_count: usize,
}

pub struct Sender<T> {
    ch: NonNull<UnsafeCell<Channel<T>>>,
}

impl<T> Sender<T> {
    fn from_raw(ch: NonNull<UnsafeCell<Channel<T>>>) -> Self {
        unsafe { &mut *ch.as_ref().get() }.tx_count += 1;
        Self { ch }
    }

    pub async fn send(&self, v: T) -> Option<()> {
        if CanSend(self).await {
            unsafe { &mut *self.ch.as_ref().get() }.data.push_back(v);
            Some(())
        } else {
            None
        }
    }
}

struct CanSend<'a, T>(&'a Sender<T>);

impl<'a, T> Future for CanSend<'a, T> {
    type Output = bool;

    fn poll(self: Pin<&mut Self>, _: &mut Context) -> Poll<Self::Output> {
        let ch = unsafe { &mut *self.0.ch.as_ref().get() };
        if ch.rx_count == 0 {
            Poll::Ready(false)
        } else if ch.data.capacity() == ch.data.len() {
            Poll::Pending
        } else {
            Poll::Ready(true)
        }
    }
}

impl<T> Clone for Sender<T> {
    fn clone(&self) -> Self {
        Self::from_raw(self.ch)
    }
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        let ch = unsafe { &mut *self.ch.as_ref().get() };
        assert_ne!(ch.tx_count, 0);
        ch.tx_count -= 1;
        if ch.rx_count == 0 && ch.tx_count == 0 {
            let _ = ch;
            drop(unsafe { Box::from_raw(self.ch.as_ptr()) })
        }
    }
}

pub struct Receiver<T> {
    ch: NonNull<UnsafeCell<Channel<T>>>,
}

impl<T> Receiver<T> {
    fn from_raw(ch: NonNull<UnsafeCell<Channel<T>>>) -> Self {
        unsafe { &mut *ch.as_ref().get() }.rx_count += 1;
        Self { ch }
    }

    pub async fn recv(&self) -> Option<T> {
        if CanReceive(self).await {
            self.do_recv()
        } else {
            None
        }
    }

    pub fn do_recv(&self) -> Option<T> {
        unsafe { &mut *self.ch.as_ref().get() }.data.pop_front()
    }
}

struct CanReceive<'a, T>(&'a Receiver<T>);

impl<'a, T> Future for CanReceive<'a, T> {
    type Output = bool;

    fn poll(self: Pin<&mut Self>, _: &mut Context) -> Poll<Self::Output> {
        let ch = unsafe { &*self.0.ch.as_ref().get() };
        if ch.data.is_empty() {
            if ch.tx_count == 0 {
                Poll::Ready(false)
            } else {
                Poll::Pending
            }
        } else {
            Poll::Ready(true)
        }
    }
}

impl<T> Clone for Receiver<T> {
    fn clone(&self) -> Self {
        Self::from_raw(self.ch)
    }
}

impl<T> Drop for Receiver<T> {
    fn drop(&mut self) {
        let ch = unsafe { &mut *self.ch.as_ref().get() };
        assert_ne!(ch.rx_count, 0);
        ch.rx_count -= 1;
        if ch.rx_count == 0 && ch.tx_count == 0 {
            let _ = ch;
            drop(unsafe { Box::from_raw(self.ch.as_ptr()) })
        }
    }
}

pub fn channel<T>(size: usize) -> (Sender<T>, Receiver<T>) {
    let ch = Box::into_raw(Box::new(UnsafeCell::new(Channel {
        data: VecDeque::with_capacity(size),
        tx_count: 0,
        rx_count: 0,
    })));
    let ch = unsafe { NonNull::new_unchecked(ch) };
    (Sender::from_raw(ch), Receiver::from_raw(ch))
}

////////////////////////////////////////////////////////////////////////////////
// fiber Channel
////////////////////////////////////////////////////////////////////////////////

pub mod deprecated {
    use super::*;

    pub struct Channel<T> {
        pub inner: fiber::Channel<T>,
    }

    pub struct RecvState<T> {
        value: Cell<Option<Option<T>>>,
        waker: Cell<Option<task::Waker>>,
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
                    .field("waker", &None::<task::Waker>)
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
}

////////////////////////////////////////////////////////////////////////////////
// RcWake
////////////////////////////////////////////////////////////////////////////////

pub trait RcWake {
    fn wake_by_ref(self: &Rc<Self>);

    fn wake(self: Rc<Self>) {
        self.wake_by_ref()
    }

    fn into_waker(self: Rc<Self>) -> task::Waker
    where
        Self: Sized,
    {
        unsafe { task::Waker::from_raw(imp::raw_waker(self)) }
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
    cell::{Cell, RefCell, UnsafeCell},
    collections::VecDeque,
    future::Future,
    pin::Pin,
    ptr::NonNull,
    rc::Rc,
    task::{Context, Poll, self},
    time::{Duration, Instant},
};
