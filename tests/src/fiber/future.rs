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

use crate::LISTEN;
use tarantool::{
    net_box::{Conn, ConnOptions, ConnTriggers, Options},
    fiber::{
        self,
        Cond,
        future::{Executor, Waker, RcWake, channel, Timer},
        sleep,
    },
    util::{IntoClones, ToDuration},
};

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

pub fn tmp() {
    let (rx, tx) = fiber::Channel::new(1).into_clones();

    let mut f = Box::pin(async {
        let ch = fiber::future::deprecated::Channel { inner: rx };
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

pub fn no_fibers() {
    eprintln!("output:");

    let port = unsafe { LISTEN };
    let conn_options = ConnOptions {
        user: "test_user".to_string(),
        password: "password".to_string(),
        ..ConnOptions::default()
    };
    let conn = Conn::new(("localhost", port), conn_options, None).unwrap();
    let (res,) = conn
        .eval("return 'whats up'", &((),), &Options::default())
        .unwrap().unwrap()
        .into_struct::<(String,)>().unwrap();
    assert_eq!(res, "whats up");

    let (exe1, exe2, exe3) = Rc::new(Executor::new()).into_clones();
    let jh = fiber::defer(|| {
        let mut last_awoke = Instant::now();
        while exe1.has_tasks() {
            exe1.do_loop();
            eprintln!("slept for {:?}", last_awoke.elapsed());
            last_awoke = Instant::now();
        }
    });
    let (tx, rx) = channel(8);
    let (tx1, tx2) = tx.into_clones();
    let (a, b) = exe1.block_on(async move {
        exe2.spawn(async move {
            exe3.sleep(100.millis()).await;
            tx1.send("hello".to_string()).await;
        });
        exe2.spawn(async move {
            let port = unsafe { LISTEN };
            let conn_options = ConnOptions {
                user: "test_user".to_string(),
                password: "password".to_string(),
                ..ConnOptions::default()
            };
            let conn = Conn::new(("localhost", port), conn_options, None).unwrap();
            let (res,) = conn
                .eval_async("return 'whats up'", &((),)).await
                .unwrap().unwrap()
                .into_struct::<(String,)>().unwrap();
            tx2.send(res).await;
        });
        (rx.recv().await.unwrap(), rx.recv().await.unwrap())
    });
    assert_eq!(a, "whats up");
    assert_eq!(b, "hello");
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
