#![cfg(feature = "picodata")]

use std::sync::{Arc, Mutex as StdMutex};
use std::thread::ThreadId;
use std::time::Duration;
use std::{mem, thread};
use tarantool::cbus;
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
