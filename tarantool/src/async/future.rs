use std::{
    rc::Rc,
    time::Duration,
};
use crate::fiber::Cond;

////////////////////////////////////////////////////////////////////////////////
/// Syncer
////////////////////////////////////////////////////////////////////////////////

pub trait Syncer {
    fn wait(&self);
    fn wake(&self);
    fn wait_timeout(&self, timeout: Duration) -> WaitTimeout;
}

enum WaitTimeout {
    Ok,
    TimedOut,
}

impl Syncer for Rc<Cond> {
    fn wait(&self) {
        Cond::wait(self);
    }

    fn wait_timeout(&self, timeout: Duration) -> WaitTimeout {
        match Cond::wait_timeout(self, timeout) {
            true => WaitTimeout::Ok,
            false => WaitTimeout::TimedOut,
        }
    }

    fn wake(&self) {
        Cond::signal(self)
    }
}

////////////////////////////////////////////////////////////////////////////////
/// Timer
////////////////////////////////////////////////////////////////////////////////

pub struct Timer;

////////////////////////////////////////////////////////////////////////////////
/// Channel
////////////////////////////////////////////////////////////////////////////////

pub mod mpmc {
    pub mod fixed {
        use std::{
            cell::{Cell, UnsafeCell},
            marker::PhantomData,
            mem::MaybeUninit,
            num::NonZeroUsize,
            ptr::{drop_in_place, NonNull},
            rc::Rc,
        };
        use crate::fiber::Cond;
        use super::super::Syncer;

        #[inline]
        pub fn channel<T, const N: usize>() -> (Sender<T, N, Rc<Cond>>, Receiver<T, N, Rc<Cond>>) {
            let chan_box = Box::new(ChannelBox::new(Rc::new(Cond::new())));
            // Box::into_raw returns a non-null pointer
            let raw = unsafe { NonNull::new_unchecked(Box::into_raw(chan_box)) };
            (Sender::new(raw), Receiver::new(raw))
        }

        ////////////////////////////////////////////////////////////////////////
        /// ChannelBox
        ////////////////////////////////////////////////////////////////////////

        pub struct ChannelBox<T, const N: usize, S: Syncer> {
            data: UnsafeCell<[MaybeUninit<T>; N]>,
            /// First occupied spot. If `tail == head` the buffer is empty.
            tail: Cell<usize>,
            /// First empty spot. If `tail == head` the buffer is empty.
            head: Cell<usize>,
            rx_count: Cell<Option<NonZeroUsize>>,
            tx_count: Cell<Option<NonZeroUsize>>,
            sync: S,
        }

        impl<T, const N: usize, S: Syncer> ChannelBox<T, N, S> {
            #[inline]
            fn new(sync: S) -> Self {
                Self {
                    data: UnsafeCell::new([MaybeUninit::uninit(); N]),
                    tail: Cell::new(0),
                    head: Cell::new(0),
                    rx_count: Cell::new(None),
                    tx_count: Cell::new(None),
                    sync,
                }
            }

            #[inline]
            fn is_full(&self) -> bool {
                N - self.len() == 1
            }

            /// Return number of occupied spots in the buffer.
            #[inline]
            fn len(&self) -> usize {
                self.head().wrapping_sub(self.tail()) % N
            }

            #[inline]
            fn is_empty(&self) -> bool {
                self.tail == self.head
            }

            #[inline]
            fn tail(&self) -> usize {
                self.tail.get()
            }

            #[inline]
            fn head(&self) -> usize {
                self.head.get()
            }

            /// # Safety
            /// `self.is_full()` must not be `true`, otherwise the data will
            #[inline]
            unsafe fn push_back(&self, v: T) {
                let head = self.head();
                self.head.set(head.wrapping_add(1) % N);
                std::ptr::write(self.data.get_mut()[head].as_mut_ptr(), v);
            }

            #[inline]
            fn try_push_back(&self, v: T) -> Result<(), T> {
                if self.is_full() {
                    return Err(v)
                }

                unsafe { self.push_back(v) };

                Ok(())
            }

            /// # Safety
            /// `self.is_empty()` must not be `true`, otherwise undefined
            /// behavior
            #[inline]
            unsafe fn pop_front(&self) -> T {
                let tail = self.tail();
                self.tail.set(tail.wrapping_add(1) % N);
                self.data.get_mut()[tail].assume_init()
            }

            #[inline]
            fn try_pop_front(&self) -> Option<T> {
                if self.is_empty() {
                    return None
                }
                Some(unsafe { self.pop_front() })
            }

            #[inline]
            fn try_send(&self, v: T) -> Result<(), TrySendError<T>> {
                if self.rx().is_none() {
                    // Only a receiver can create another receiver so nobody
                    // will ever be able to receive this message
                    return Err(TrySendError::Disconnected(v))
                }

                let was_empty = self.is_empty();

                if let Err(v) = self.try_push_back(v) {
                    Err(TrySendError::Full(v))
                } else {
                    if was_empty {
                        self.sync.wake()
                    }

                    Ok(())
                }
            }

            fn send(&self, v: T) -> Result<(), T> {
                if self.rx().is_none() {
                    // Only a receiver can create another receiver so nobody
                    // will ever be able to receive this message
                    return Err(v)
                }

                while self.is_full() {
                    self.sync.wait()
                }

                if self.rx().is_none() {
                    return Err(v)
                }

                let was_empty = self.is_empty();
                unsafe { self.push_back(v) }
                if was_empty {
                    self.sync.wake()
                }

                Ok(())
            }

            #[inline]
            fn try_recv(&self) -> Result<T, TryRecvError> {
                if self.tx().is_none() && self.is_empty() {
                    // Only a sender can create another sender so nobody
                    // will ever be able to send us a message
                    return Err(TryRecvError::Disconnected)
                }

                let was_full = self.is_full();

                if let Some(v) = self.try_pop_front() {
                    if was_full {
                        self.sync.wake()
                    }
                    Ok(v)
                } else {
                    Err(TryRecvError::Empty)
                }
            }

            #[inline]
            fn recv(&self) -> Option<T> {
                if self.tx().is_none() && self.is_empty() {
                    // Only a sender can create another sender so nobody
                    // will ever be able to send us a message
                    return None
                }

                while self.is_empty() {
                    self.sync.wait()
                }

                let was_full = self.is_full();
                let v = self.pop_front();
                if was_full {
                    self.sync.wake()
                }

                Some(v)
            }

            #[inline]
            fn no_refs(&self) -> bool {
                self.rx().is_none() && self.tx().is_none()
            }

            #[inline]
            fn inc_rx(&self) {
                Self::inc(&self.rx_count)
            }

            #[inline]
            fn inc_tx(&self) {
                Self::inc(&self.tx_count)
            }

            #[inline]
            fn inc(count: &Cell<Option<NonZeroUsize>>) {
                let new_count = unsafe {
                    NonZeroUsize::new_unchecked(
                        // ignoring possibility of overflow
                        1 + count.take().map(|c| c.get()).unwrap_or(0)
                    )
                };
                count.set(Some(new_count))
            }

            #[inline]
            fn dec_tx(&self) {
                Self::dec(&self.tx_count)
            }

            #[inline]
            fn dec_rx(&self) {
                Self::dec(&self.rx_count)
            }

            #[inline]
            fn dec(count: &Cell<Option<NonZeroUsize>>) {
                if let Some(c) = count.take() {
                    count.set(NonZeroUsize::new(c.get() - 1))
                } else {
                    panic!("decrement called on a zero reference count")
                }
            }

            #[inline]
            fn tx(&self) -> Option<NonZeroUsize> {
                self.tx_count.get()
            }

            #[inline]
            fn rx(&self) -> Option<NonZeroUsize> {
                self.rx_count.get()
            }
        }

        impl<T, const N: usize, S: Syncer> Drop for ChannelBox<T, N, S> {
            fn drop(&mut self) {
                assert!(self.no_refs());
                if self.tail() <= self.head() {
                    for i in self.tail()..self.head() {
                        drop_in_place(self.data.get_mut()[i].as_mut_ptr())
                    }
                } else {
                    for i in 0..self.head() {
                        drop_in_place(self.data.get_mut()[i].as_mut_ptr())
                    }
                    for i in self.tail()..N {
                        drop_in_place(self.data.get_mut()[i].as_mut_ptr())
                    }
                }
            }
        }

        ////////////////////////////////////////////////////////////////////////
        /// Errors
        ////////////////////////////////////////////////////////////////////////

        pub enum TrySendError<T> {
            Disconnected(T),
            Full(T),
        }

        pub enum SendTimeoutError<T> {
            Disconnected(T),
            Timeout(T),
        }

        pub enum TryRecvError {
            Disconnected,
            Empty,
        }

        pub enum RecvTimeoutError {
            Disconnected,
            Timeout,
        }

        ////////////////////////////////////////////////////////////////////////
        /// Sender/Receiver
        ////////////////////////////////////////////////////////////////////////

        macro_rules! impl_channel_part {
            ($t:ident, $inc:ident, $dec:ident) => {
                pub struct $t<T, const N: usize, S: Syncer> {
                    inner: NonNull<ChannelBox<T, N, S>>,
                    marker: PhantomData<ChannelBox<T, N, S>>,
                }

                impl<T, const N: usize, S: Syncer> $t<T, N, S> {
                    #[inline]
                    fn new(inner: NonNull<ChannelBox<T, N, S>>) -> Self {
                        inner.as_ref().$inc();
                        Self { inner, marker: PhantomData, }
                    }
                }

                impl<T, const N: usize, S: Syncer> Clone for $t<T, N, S> {
                    #[inline]
                    fn clone(&self) -> Self {
                        Self::new(self.inner)
                    }
                }

                impl<T, const N: usize, S: Syncer> Drop for $t<T, N, S> {
                    fn drop(&mut self) {
                        self.inner.as_ref().$dec();
                        if self.inner.as_ref().no_refs() {
                            drop_in_place(self.inner.as_ptr())
                        }
                    }
                }
            }
        }

        impl_channel_part!{Sender, inc_tx, dec_tx}
        impl_channel_part!{Receiver, inc_rx, dec_rx}
    }
}
