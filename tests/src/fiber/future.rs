pub fn timer() {
    let f = async {
        Timer::new(100.millis()).await;
        "done"
    };
    let mut f = Box::pin(f);

    let task = Waker(Cell::new(false));
    let (task, task_rx) = Rc::new(task).into_clones();
    let waker = task.into_waker();

    assert_eq!(f.as_mut().poll(&mut Context::from_waker(&waker)), Poll::Pending);
    assert_eq!(task_rx.0.get(), false);

    let res = loop {
        match f.as_mut().poll(&mut Context::from_waker(&waker)) {
            Poll::Pending => sleep(100.millis()),
            Poll::Ready(res) => break res,
        }
    };
    assert_eq!(res, "done");
    assert_eq!(task_rx.0.get(), true);

}

struct Waker<T>(T);

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

pub fn tmp() {
    let (rx, tx) = fiber::Channel::new(1).into_clones();

    let mut f = Box::pin(async {
        let ch = fiber::future::Channel { inner: rx };
        let mut r = ch.recv();
        let mut f = futures::select!(
            v = r => Some(v),
            _ = Timer::new(100.millis()) => None,
        );
        if f.is_none() {
            tx.send("hello").unwrap();
            f = futures::select!(
                v = r => Some(v),
                _ = Timer::new(100.millis()) => unreachable!(),
            );
        }
        f.unwrap()
    });

    let my_cond = fiber::Cond::new();
    let waker = Rc::new(Waker(&my_cond)).into_waker();

    let res = loop {
        match f.as_mut().poll(&mut Context::from_waker(&waker)) {
            Poll::Pending => {}
            Poll::Ready(res) => break res,
        }

        my_cond.wait();
    };

    assert_eq!(res, Some("hello"))
}

struct Task {
    future: Pin<Box<dyn Future<Output = ()>>>,
    loc: &'static std::panic::Location<'static>,
}

#[derive(Default)]
struct Executor {
    queue: UnsafeCell<VecDeque<Task>>,
    deadlines: UnsafeCell<Vec<Instant>>,
    has_new_tasks: Cell<bool>,
    cond: Rc<fiber::Cond>,
}

impl Executor {
    fn new() -> Self {
        Self::default()
    }

    fn next_wait_since(&self, now: Instant) -> Option<Duration> {
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

    fn next_wait(&self) -> Option<Duration> {
        let now = Instant::now();
        self.next_wait_since(now)
    }

    fn has_tasks(&self) -> bool {
        !unsafe { &*self.queue.get() }.is_empty()
    }

    #[track_caller]
    fn spawn(&self, future: impl Future<Output = ()> + 'static) {
        self.has_new_tasks.set(true);
        unsafe { &mut *self.queue.get() }.push_back(
            Task {
                future: Box::pin(future),
                loc: std::panic::Location::caller(),
            }
        );
    }

    async fn sleep(&self, timeout: Duration) {
        let deadline = Instant::now() + timeout;
        unsafe { &mut *self.deadlines.get() }.push(deadline);
        Sleep { deadline }.await
    }

    fn do_loop(&self) {
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
    fn block_on<T: 'static>(&self, future: impl Future<Output = T> + 'static) -> T {
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

struct Channel<T> {
    data: VecDeque<T>,
    tx_count: usize,
    rx_count: usize,
}

struct Sender<T> {
    ch: NonNull<UnsafeCell<Channel<T>>>,
}

impl<T> Sender<T> {
    fn from_raw(ch: NonNull<UnsafeCell<Channel<T>>>) -> Self {
        unsafe { &mut *ch.as_ref().get() }.tx_count += 1;
        Self { ch }
    }

    async fn send(&self, v: T) -> Option<()> {
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

struct Receiver<T> {
    ch: NonNull<UnsafeCell<Channel<T>>>,
}

impl<T> Receiver<T> {
    fn from_raw(ch: NonNull<UnsafeCell<Channel<T>>>) -> Self {
        unsafe { &mut *ch.as_ref().get() }.rx_count += 1;
        Self { ch }
    }

    async fn recv(&self) -> Option<T> {
        if CanReceive(self).await {
            self.do_recv()
        } else {
            None
        }
    }

    fn do_recv(&self) -> Option<T> {
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

fn channel<T>(size: usize) -> (Sender<T>, Receiver<T>) {
    let ch = Box::into_raw(Box::new(UnsafeCell::new(Channel {
        data: VecDeque::with_capacity(size),
        tx_count: 0,
        rx_count: 0,
    })));
    let ch = unsafe { NonNull::new_unchecked(ch) };
    (Sender::from_raw(ch), Receiver::from_raw(ch))
}

pub fn no_fibers() {
    eprintln!("output:");
    let (exe1, exe2, exe3) = Rc::new(Executor::new()).into_clones();
    let jh = fiber::defer(||
        while exe1.has_tasks() {
            exe1.do_loop()
        }
    );
    let (tx, rx) = channel(8);
    let res = exe1.block_on(async move {
        exe2.spawn(async move {
            exe3.sleep(100.millis()).await;
            tx.send("hello").await;
        });
        rx.recv().await.unwrap()
    });
    assert_eq!(res, "hello");
    jh.join();
}

/// Just an experiment with fibers/mio. Doesn't actually use any futures
pub fn socket() {
    use mio::{*, net::{TcpListener, TcpStream}};
    use std::{
        io::{Read, Write, ErrorKind::{Interrupted, WouldBlock}},
    };
    let addr = "127.0.0.1:42069".parse().unwrap();

    let serv = fiber::defer(|| -> std::io::Result<()> {
        let mut serv = TcpListener::bind(addr)?;
        let mut poll = Poll::new()?;
        let mut events = Events::with_capacity(1024);
        poll.registry()
            .register(&mut serv, Token(0), Interest::READABLE)?;
        let mut conn = None;

        let mut buf = vec![0; 4096];
        let mut count_rx = 0;
        let mut count_tx = 0;

        loop {
            poll.poll(&mut events, Some(Duration::ZERO))?;

            for event in &events {
                match event.token() {
                    Token(0) => {
                        if conn.is_some() {
                            continue
                        }
                        let mut new_conn = match serv.accept() {
                            Ok((new_conn, _)) => new_conn,
                            Err(e) if e.kind() == WouldBlock => break,
                            Err(e) => return Err(e),
                        };

                        poll.registry()
                            .register(&mut new_conn, Token(42), Interest::READABLE)?;

                        conn = Some(new_conn)
                    }
                    Token(42) => {
                        if conn.is_none() {
                            continue
                        }
                        if event.is_readable() {
                            match conn.as_ref().unwrap().read(&mut buf[count_rx..]) {
                                Ok(0) => { conn.take(); break }
                                Ok(n) => count_rx += n,
                                Err(err) if err.kind() == WouldBlock => break,
                                Err(err) if err.kind() == Interrupted => continue,
                                Err(err) => return Err(err),
                            }

                            dbg!(String::from_utf8_lossy(&buf[0..count_rx]));

                            if count_rx >= 4 && &buf[0..4] == b"ping" {
                                count_rx = 0;
                                let mut curr_conn = conn.take().unwrap();
                                poll.registry()
                                    .reregister(
                                        &mut curr_conn, Token(69), Interest::WRITABLE
                                    )?;
                                conn = Some(curr_conn);
                            }
                        }
                    }
                    Token(69) => {
                        if conn.is_none() {
                            continue
                        }
                        if event.is_writable() {
                            static DATA: &[u8] = b"pong";
                            match conn.as_ref().unwrap().write(&DATA[count_tx..]) {
                                Ok(n) => { count_tx += n }
                                Err(e)
                                    if matches!(e.kind(), Interrupted | WouldBlock)
                                        => {}
                                Err(e) => return Err(e),
                            }

                            dbg!(String::from_utf8_lossy(&DATA[0..count_tx]));

                            if count_tx == 4 {
                                return Ok(())
                            }
                        }
                    }
                    Token(_) => {
                        dbg!(event);
                    }
                }
            }

            fiber::sleep(Duration::ZERO)
        }
    });

    let clnt = fiber::defer(|| -> std::io::Result<()> {
        let mut clnt = TcpStream::connect(addr)?;
        let mut poll = Poll::new()?;
        let mut events = Events::with_capacity(1024);
        poll.registry()
            .register(&mut clnt, Token(1), Interest::WRITABLE)?;

        let data = b"ping";

        let mut buf = vec![0; 4096];
        let mut count_rx = 0;
        let mut count_tx = 0;

        loop {
            poll.poll(&mut events, Some(Duration::ZERO))?;

            for event in &events {
                match event.token() {
                    Token(1) => {
                        if !event.is_writable() {
                            continue
                        }
                        match clnt.write(&data[count_tx..]) {
                            Ok(n) => count_tx += n,
                            Err(err) if err.kind() == WouldBlock => break,
                            Err(err) if err.kind() == Interrupted => continue,
                            Err(e) => return Err(e),
                        }

                        dbg!(String::from_utf8_lossy(&data[..count_tx]));

                        if count_tx == 4 {
                            count_tx = 0;
                            poll.registry()
                                .reregister(&mut clnt, Token(2), Interest::READABLE)?;
                        }
                    }
                    Token(2) => {
                        if !event.is_readable() {
                            continue
                        }
                        match clnt.read(&mut buf[count_rx..]) {
                            Ok(n) => count_rx += n,
                            Err(err) if err.kind() == WouldBlock => break,
                            Err(err) if err.kind() == Interrupted => continue,
                            Err(err) => return Err(err),
                        }

                        dbg!(String::from_utf8_lossy(&buf[0..count_rx]));

                        if count_rx >= 4 && &buf[0..4] == b"pong" {
                            return Ok(())
                        }
                    }
                    Token(_) => { dbg!(event); }
                }
            }

            fiber::sleep(Duration::ZERO)
        }
    });

    assert!(serv.join().is_ok());
    assert!(clnt.join().is_ok());
}

////////////////////////////////////////////////////////////////////////////////
// use
////////////////////////////////////////////////////////////////////////////////

use std::{
    cell::{Cell, UnsafeCell},
    collections::VecDeque,
    future::Future,
    marker::PhantomData,
    ptr::NonNull,
    pin::Pin,
    rc::Rc,
    task::{Context, Poll},
    time::{Duration, Instant},
};

use tarantool::{
    clock::INFINITY,
    fiber::{
        self,
        Cond,
        future::{RcWake, Timer},
        sleep,
    },
    util::{IntoClones, ToDuration},
};
