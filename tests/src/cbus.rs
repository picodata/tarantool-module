#![cfg(feature = "picodata")]

use std::sync::Arc;
use std::thread::{JoinHandle, ThreadId};
use std::time::Duration;
use std::{mem, thread};
use tarantool::cbus;
use tarantool::cbus::{oneshot, unbound, RecvError};
use tarantool::cbus::{Message, MessageHop};
use tarantool::fiber;
use tarantool::fiber::{check_yield, Cond, Fiber, YieldResult};

fn run_cbus_endpoint(endpoint_name: &str) -> Fiber<'static, ()> {
    let mut fiber = fiber::Fiber::new("cbus_fiber", &mut |_: Box<()>| {
        let cbus_endpoint = cbus::Endpoint::new(endpoint_name).unwrap();
        cbus_endpoint.cbus_loop();
        0
    });
    fiber.start(());
    fiber
}

#[tarantool::test]
pub fn cbus_send_message_test() {
    static mut TX_THREAD_ID: Option<ThreadId> = None;
    static mut SENDER_THREAD_ID: Option<ThreadId> = None;

    let mut cbus_fiber = run_cbus_endpoint("cbus_send_message_test");

    struct CondPtr(*const Cond);
    unsafe impl Send for CondPtr {}

    let cond = Cond::new();
    let cond_ptr = CondPtr(&cond as *const Cond);

    let thread = thread::spawn(move || {
        unsafe { SENDER_THREAD_ID = Some(thread::current().id()) };
        let pipe = cbus::LCPipe::new("cbus_send_message_test");
        let hop = MessageHop::new(|msg: Box<Message<CondPtr>>| {
            unsafe { TX_THREAD_ID = Some(thread::current().id()) };
            let cond = unsafe { msg.user_data().0.as_ref().unwrap() };
            cond.broadcast();
        });
        let msg = Message::new(hop, cond_ptr);
        pipe.push_message(msg);
    });

    cond.wait();

    unsafe {
        assert!(SENDER_THREAD_ID.is_some());
        assert!(TX_THREAD_ID.is_some());
        assert_ne!(SENDER_THREAD_ID, TX_THREAD_ID);
    }

    thread.join().unwrap();
    cbus_fiber.cancel();
}

#[tarantool::test]
pub fn oneshot_test() {
    let mut cbus_fiber = run_cbus_endpoint("oneshot_test");

    let chan = oneshot::Channel::new();
    let (sender, receiver) = chan.split("oneshot_test");
    let thread = thread::spawn(move || {
        thread::sleep(Duration::from_secs(1));
        sender.send(1);
    });

    assert_eq!(
        check_yield(|| { receiver.receive().unwrap() }),
        YieldResult::Yielded(1)
    );
    thread.join().unwrap();

    let chan = oneshot::Channel::new();
    let (sender, receiver) = chan.split("oneshot_test");
    let thread = thread::spawn(move || {
        sender.send(2);
    });
    thread.join().unwrap();

    assert_eq!(
        check_yield(|| { receiver.receive().unwrap() }),
        YieldResult::DidntYield(2)
    );

    cbus_fiber.cancel();
}

#[tarantool::test]
pub fn oneshot_multiple_channels_test() {
    let mut cbus_fiber = run_cbus_endpoint("oneshot_multiple_channels_test");

    let pipe = cbus::LCPipe::new("oneshot_multiple_channels_test");
    let pipe = Arc::new(pipe);

    let chan1 = oneshot::Channel::new();
    let (sender1, receiver1) = chan1.split_on_pipe(Arc::clone(&pipe));
    let chan2 = oneshot::Channel::new();
    let (sender2, receiver2) = chan2.split_on_pipe(Arc::clone(&pipe));

    let thread1 = thread::spawn(move || {
        thread::sleep(Duration::from_secs(1));
        sender1.send("1");
    });

    let thread2 = thread::spawn(move || {
        thread::sleep(Duration::from_secs(2));
        sender2.send("2");
    });

    let result2 = receiver2.receive();
    let result1 = receiver1.receive();

    assert!(matches!(result1, Ok("1")));
    assert!(matches!(result2, Ok("2")));

    thread1.join().unwrap();
    thread2.join().unwrap();
    cbus_fiber.cancel();
}

#[tarantool::test]
pub fn oneshot_sender_drop_test() {
    let mut cbus_fiber = run_cbus_endpoint("oneshot_sender_drop_test");

    let chan1 = oneshot::Channel::<()>::new();
    let (sender, receiver) = chan1.split("oneshot_sender_drop_test");

    let thread = thread::spawn(move || {
        thread::sleep(Duration::from_secs(1));
        mem::drop(sender)
    });

    let result = receiver.receive();
    assert!(matches!(result, Err(RecvError::Disconnected)));

    thread.join().unwrap();
    cbus_fiber.cancel();
}

#[tarantool::test]
pub fn unbound_test() {
    let mut cbus_fiber = run_cbus_endpoint("unbound_test");

    let chan = unbound::Channel::new();
    let (tx, rx) = chan.split("unbound_test");

    let thread = thread::spawn(move || {
        for i in 0..1000 {
            tx.send(i);
            if i % 100 == 0 {
                thread::sleep(Duration::from_millis(1000));
            }
        }
    });

    assert_eq!(
        check_yield(|| {
            let mut recv_results = vec![];
            for _ in 0..1000 {
                recv_results.push(rx.receive().unwrap());
            }
            recv_results
        }),
        YieldResult::Yielded((0..1000).collect::<Vec<_>>())
    );
    thread.join().unwrap();
    cbus_fiber.cancel();
}

#[tarantool::test]
pub fn unbound_disconnect_test() {
    let mut cbus_fiber = run_cbus_endpoint("unbound_disconnect_test");

    let chan = unbound::Channel::new();
    let (tx, rx) = chan.split("unbound_disconnect_test");

    let thread = thread::spawn(move || {
        tx.send(1);
        tx.send(2);
    });

    assert!(matches!(rx.receive(), Ok(1)));
    assert!(matches!(rx.receive(), Ok(2)));
    assert!(matches!(rx.receive(), Err(RecvError::Disconnected)));

    thread.join().unwrap();
    cbus_fiber.cancel();
}

#[tarantool::test]
pub fn unbound_mpsc_test() {
    const MESSAGES_PER_PRODUCER: i32 = 10_000;
    let mut cbus_fiber = run_cbus_endpoint("unbound_mpsc_test");

    let chan = unbound::Channel::new();
    let (tx, rx) = chan.split("unbound_mpsc_test");

    fn create_producer(sender: unbound::Sender<i32>) -> JoinHandle<()> {
        thread::spawn(move || {
            for i in 0..MESSAGES_PER_PRODUCER {
                sender.send(i);
            }
        })
    }

    let jh1 = create_producer(tx.clone());
    let jh2 = create_producer(tx.clone());
    let jh3 = create_producer(tx);

    for _ in 0..MESSAGES_PER_PRODUCER * 3 {
        assert!(matches!(rx.receive(), Ok(_)));
    }
    assert!(matches!(rx.receive(), Err(RecvError::Disconnected)));

    jh1.join().unwrap();
    jh2.join().unwrap();
    jh3.join().unwrap();
    cbus_fiber.cancel();
}
