//! tide is useless, it's just an abstraction over async_h1
//! can we use single-threaded tokio in a separate fiber? whose going to be
//! wakign it up?
use tarantool::{
    coio::coio_wait,
    ffi::tarantool::{CoIOFlags, TIMEOUT_INFINITY},
    fiber::{self, future::Executor},
    util::IntoClones,
    r#async::poll_fn,
};

use std::{
    collections::HashMap,
    cell::RefCell,
    fmt,
    io,
    os::unix::io::AsRawFd,
    rc::Rc,
    time::{Instant, Duration},
    task::Poll,
};

use mio::{
    net,
    Token,
};

use tide::{Request, listener::{Listener, ToListener}};

#[derive(Debug)]
struct TcpListener {
    server: Option<tide::Server<()>>,
    addr: std::net::SocketAddr,
    listener: Option<net::TcpListener>,
    poll: Rc<RefCell<mio::Poll>>,
    token: Token,
    evs: Rc<RefCell<HashMap<Token, mio::event::Event>>>,
}

impl TcpListener {
    fn new(
        addr: &str,
        poll: Rc<RefCell<mio::Poll>>,
        token: Token,
        evs: Rc<RefCell<HashMap<Token, mio::event::Event>>>,
    ) -> Self {
        Self {
            server: None,
            addr: addr.parse().unwrap(),
            listener: None,
            poll,
            token,
            evs,
        }
    }
}

unsafe impl Send for TcpListener {}
unsafe impl Sync for TcpListener {}

impl fmt::Display for TcpListener {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("fuck you")
    }
}

impl ToListener<()> for TcpListener {
    type Listener = Self;

    fn to_listener(self) -> io::Result<Self::Listener> {
        Ok(self)
    }
}

#[async_trait::async_trait]
impl Listener<()> for TcpListener {
    async fn bind(&mut self, server: tide::Server<()>) -> io::Result<()> {
        self.server = Some(server);
        // TODO: non blocking
        self.listener = Some(net::TcpListener::bind(self.addr)?);
        self.poll.borrow_mut().registry()
            .register(self.listener.as_mut().unwrap(), self.token, mio::Interest::READABLE)?;
        Ok(())
    }

    async fn accept(&mut self) -> io::Result<()> {
        let new_conn = poll_fn(|ctx| {
            let maybe_event = self.evs.borrow_mut().remove(&self.token);
            if let Some(event) = maybe_event {
                match self.listener.as_ref().unwrap().accept() {
                    Ok((new_conn, _)) => Poll::Ready(Ok(new_conn)),
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        self.evs.borrow_mut().insert(self.token, event);
                        Poll::Pending
                    }
                    Err(e) => Poll::Ready(Err(e)),
                }
            } else {
                Poll::Pending
            }
        }).await;
    }

    fn info(&self) -> Vec<tide::listener::ListenInfo> {
        todo!()
    }
}

#[tarantool::proc]
fn web() {
    let (exe1, exe2) = Rc::new(Executor::new()).into_clones();

    let executor_jh = fiber::defer(|| {
        let mut last_awoke = Instant::now();
        while exe1.has_tasks() {
            exe1.do_loop();
            eprintln!("slept for {:?}", last_awoke.elapsed());
            last_awoke = Instant::now();
        }
    });

    let (ev, ev2) = Rc::new(RefCell::new(HashMap::new())).into_clones();
    let (poll, poll2) = Rc::new(RefCell::new(mio::Poll::new().unwrap())).into_clones();
    let io_cond = exe1.cond.clone();

    let io_jh = fiber::defer(|| {
        let mut events = mio::Events::with_capacity(1024);
        loop {
            poll.borrow_mut().poll(&mut events, Some(Duration::ZERO)).unwrap();
            // TODO: process events
            if !events.is_empty() {
                io_cond.signal()
            }
            for e in &events {
                match ev.borrow_mut().entry(e.token()) {
                    std::collections::hash_map::Entry::Occupied(_) => {
                        panic!("previous event wasn't handled: {:?}", e.token())
                    }
                    std::collections::hash_map::Entry::Vacant(kv) => {
                        kv.insert(e.clone());
                    }
                }
            }
            let fd = poll.borrow().as_raw_fd();
            coio_wait(
                fd,
                CoIOFlags::READ,
                unsafe { TIMEOUT_INFINITY },
            ).unwrap();
        }
    });

    exe1.block_on(async move {
        let mut app = tide::new();

        app.at("/").get(|request: Request<_>| async move {
            Ok(format!(
                "Hi! You reached this app through: {}",
                request.local_addr().unwrap_or("an unknown port")
            ))
        });

        let l = TcpListener::new("localhost:42069", poll2, Token(42069), ev2);
        app.listen(l).await.unwrap();
    });

    executor_jh.join();
    std::mem::forget(io_jh);
}
