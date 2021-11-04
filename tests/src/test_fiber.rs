use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::Duration,
};

use tarantool::fiber::{
    self, fiber_yield, is_cancelled, sleep, Cond, Fiber, FiberAttr
};
use tarantool::hlua::{
    AsMutLua,
    Lua,
    LuaFunction
};
use tarantool::util::IntoTupleOfClones;
use tarantool::ffi::lua;

pub fn test_fiber_new() {
    let mut fiber = Fiber::new("test_fiber", &mut |_| 0);
    fiber.set_joinable(true);
    fiber.start(());
    fiber.join();
}

pub fn test_fiber_new_with_attr() {
    let mut attr = FiberAttr::new();
    attr.set_stack_size(100_000).unwrap();

    let mut fiber = Fiber::new_with_attr("test_fiber", &attr, &mut |_| 0);
    fiber.set_joinable(true);
    fiber.start(());
    fiber.join();
}

pub fn test_fiber_arg() {
    let mut fiber = Fiber::new("test_fiber", &mut |x| {
        assert_eq!(*x, 99);
        0
    });
    fiber.set_joinable(true);
    fiber.start(99);
    fiber.join();
}

pub fn test_fiber_cancel() {
    let mut fiber = Fiber::new("test_fiber", &mut |_| {
        assert_eq!(is_cancelled(), false);
        sleep(Duration::from_millis(10));
        assert_eq!(is_cancelled(), true);
        0
    });
    fiber.set_joinable(true);
    fiber.start(());
    fiber.cancel();
    fiber.join();
}

pub fn test_fiber_wake() {
    let mut fiber = Fiber::new("test_fiber", &mut |_| {
        fiber_yield();
        0
    });
    fiber.set_joinable(true);
    fiber.start(());
    sleep(Duration::from_millis(10));
    fiber.wakeup();
    fiber.join();
}

pub fn test_fiber_wake_multiple() {
    let res = Rc::new(RefCell::new(vec![]));
    let mut fibers = vec![];
    for (i, c) in (1..).zip(&['a', 'b', 'c']) {
        let mut fiber = Fiber::new(
            &format!("test_fiber_{}", c),
            &mut |r: Box<Rc<RefCell<Vec<i32>>>>| {
                fiber_yield();
                r.borrow_mut().push(i);
                0
            }
        );
        fiber.start(res.clone());
        fiber.wakeup();
        fibers.push(fiber);
    }

    for f in &mut fibers {
        f.set_joinable(true);
    }

    res.borrow_mut().push(0);
    for f in fibers {
        f.join();
    }
    res.borrow_mut().push(4);

    let res = res.borrow().iter().copied().collect::<Vec<_>>();
    // This is what we want:
    // assert_eq(res, vec![0, 1, 2, 3, 4]);
    // This is what we get:
    assert_eq!(res, vec![0, 3, 3, 3, 4]);
    // Because `Fiber` doesn't work with closures. `i` is passed by reference
    // and by the time the first fiber starts executing, it is equal to 3.
    // This is actually undefined behavior, so adding this test is probably a
    // bad idea
}

pub fn test_fiber_cond_signal() {
    let cond = Rc::new(Cond::new());
    let mut fiber = Fiber::new("test_fiber", &mut |cond: Box<Rc<Cond>>| {
        (*cond).wait();
        0
    });
    fiber.set_joinable(true);
    fiber.start(cond.clone());
    sleep(Duration::from_millis(10));
    cond.signal();
    fiber.join();
}

pub fn test_fiber_cond_broadcast() {
    let cond = Rc::new(Cond::new());

    let mut fiber_a = Fiber::new("test_fiber_a", &mut |cond: Box<Rc<Cond>>| {
        (*cond).wait();
        0
    });
    fiber_a.set_joinable(true);
    fiber_a.start(cond.clone());

    let mut fiber_b = Fiber::new("test_fiber_b", &mut |cond: Box<Rc<Cond>>| {
        (*cond).wait();
        0
    });
    fiber_b.set_joinable(true);
    fiber_b.start(cond.clone());

    sleep(Duration::from_millis(10));
    cond.broadcast();
    fiber_a.join();
    fiber_b.join();
}

pub fn test_fiber_cond_timeout() {
    let cond = Rc::new(Cond::new());
    let mut fiber = Fiber::new("test_fiber", &mut |cond: Box<Rc<Cond>>| {
        let r = (*cond).wait_timeout(Duration::from_millis(10));
        assert!(!r);
        0
    });
    fiber.set_joinable(true);
    fiber.start(cond.clone());
    sleep(Duration::from_millis(20));
    cond.signal();
    fiber.join();
}

////////////////////////////////////////////////////////////////////////////////
// New
////////////////////////////////////////////////////////////////////////////////

pub fn test_immediate() {
    let jh = fiber::Builder::new()
        .func(|| 69)
        .start()
        .unwrap();
    let res = jh.join();
    assert_eq!(res, 69);

    let jh = fiber::start(|| 420);
    let res = jh.join();
    assert_eq!(res, 420);
}

pub fn test_immediate_with_attrs() {
    let jh = fiber::Builder::new()
        .name("boo")
        .stack_size(100_000).unwrap()
        .func(|| 42)
        .start()
        .unwrap();
    let res = jh.join();
    assert_eq!(res, 42);
}

pub fn test_multiple_immediate() {
    let mut res = vec![];
    let fibers = vec![vec![1, 2], vec![3, 4], vec![5, 6]]
        .into_iter()
        .map(|v|
            fiber::start(move || {
                v.into_iter().map(|e| e + 1).collect::<Vec::<_>>()
            })
        )
        .collect::<Vec<_>>();
    res.push(1);
    res.extend(
        fibers.into_iter()
            .map(fiber::JoinHandle::join)
            .flatten()
    );
    res.push(8);
    assert_eq!(res, vec![1, 2, 3, 4, 5, 6, 7, 8]);
}

pub fn test_unit_immediate() {
    let jh = fiber::Builder::new()
        .func(|| ())
        .start()
        .unwrap();
    let () = jh.join();

    let () = fiber::start_proc(|| ()).join();
}

pub fn test_unit_immediate_with_attrs() {
    let jh = fiber::Builder::new()
        .name("boo")
        .stack_size(100_000).unwrap()
        .proc(|| ())
        .start()
        .unwrap();
    let () = jh.join();
}

pub fn test_multiple_unit_immediate() {
    let res = Rc::new(RefCell::new(vec![]));
    let fibers = vec![vec![1, 2], vec![3, 4], vec![5, 6]]
        .into_iter()
        .map(|v| {
            let res_ref = res.clone();
            fiber::start_proc(move || {
                res_ref.borrow_mut().extend(
                    v.into_iter().map(|e| e + 1).collect::<Vec::<_>>()
                )
            })
        })
        .collect::<Vec<_>>();
    res.borrow_mut().push(8);
    for f in fibers {
        f.join()
    }
    res.borrow_mut().push(9);
    let res = res.borrow().iter().copied().collect::<Vec<_>>();
    assert_eq!(res, vec![2, 3, 4, 5, 6, 7, 8, 9]);
}

pub fn test_deferred() {
    let jh = fiber::Builder::new()
        .func(|| 13)
        .defer()
        .unwrap();
    assert_eq!(jh.join(), 13);

    let jh = fiber::defer(|| 42);
    assert_eq!(jh.join(), 42);
}

pub fn test_deferred_with_attrs() {
    let res = fiber::Builder::new()
        .name("boo")
        .stack_size(100_000).unwrap()
        .func(|| 15)
        .defer()
        .unwrap()
        .join();
    assert_eq!(res, 15);
}

pub fn test_multiple_deferred() {
    let mut res = vec![];
    let fibers = vec![vec![1, 2], vec![3, 4], vec![5, 6]]
        .into_iter()
        .map(|v|
            fiber::defer(move || {
                v.into_iter().map(|e| e + 1).collect::<Vec::<_>>()
            })
        )
        .collect::<Vec<_>>();
    res.push(1);
    res.extend(
        fibers.into_iter()
            .map(fiber::LuaJoinHandle::join)
            .flatten()
    );
    res.push(8);
    assert_eq!(res, vec![1, 2, 3, 4, 5, 6, 7, 8]);
}

pub fn test_unit_deferred() {
    let jh = fiber::Builder::new()
        .proc(|| ())
        .defer()
        .unwrap();
    let () = jh.join();

    let res = Rc::new(Cell::new(0));
    let jh = {
        let res = res.clone();
        fiber::defer_proc(move || res.set(42))
    };
    assert_eq!(res.get(), 0);
    jh.join();
    assert_eq!(res.get(), 42);
}

pub fn test_unit_deferred_with_attrs() {
    let () = fiber::Builder::new()
        .name("boo")
        .stack_size(100_000).unwrap()
        .proc(|| ())
        .defer()
        .unwrap()
        .join();
}

pub fn test_multiple_unit_deferred() {
    let res = Rc::new(RefCell::new(vec![]));
    let fibers = vec![vec![1, 2], vec![3, 4], vec![5, 6]]
        .into_iter()
        .map(|v| {
            let res_ref = res.clone();
            fiber::defer_proc(move ||
                res_ref.borrow_mut().extend(
                    v.into_iter().map(|e| e + 1).collect::<Vec::<_>>()
                )
            )
        })
        .collect::<Vec<_>>();
    res.borrow_mut().push(1);
    for f in fibers {
        f.join()
    }
    res.borrow_mut().push(8);
    let res = res.borrow().iter().copied().collect::<Vec<_>>();
    assert_eq!(res, vec![1, 2, 3, 4, 5, 6, 7, 8]);
}

fn fiber_csw() -> i32 {
    static mut FUNCTION_DEFINED: bool = false;
    let mut lua: Lua = crate::hlua::global();

    if unsafe { !FUNCTION_DEFINED } {
        lua.execute::<()>(r#"
        function fiber_csw()
        local fiber = require('fiber')
        return fiber.info()[fiber.id()].csw
        end
        "#).unwrap();
        unsafe { FUNCTION_DEFINED = true; }
    }

    return lua.get::<LuaFunction<_>, _>("fiber_csw").unwrap().call().unwrap();
}

struct LuaStackIntegrityGuard {
    name: &'static str,
}

impl LuaStackIntegrityGuard {
    fn new(name: &'static str) -> Self {
        let mut lua: Lua = crate::hlua::global();
        let l = lua.as_mut_lua().state_ptr();
        unsafe { lua::lua_pushlstring(l, name.as_bytes().as_ptr() as *mut i8, name.len()) };
        Self{name}
    }
}

impl Drop for LuaStackIntegrityGuard {
    fn drop(&mut self) {
        let mut lua: Lua = crate::hlua::global();
        let l = lua.as_mut_lua().state_ptr();

        let msg = unsafe {
            let cstr = lua::lua_tostring(l, -1);
            if cstr.is_null() {
                panic!("Lua stack integrity violation");
            }
            let msg = std::ffi::CStr::from_ptr(cstr).to_str().unwrap();
            lua::lua_pop(l, 1);
            msg
        };

        assert_eq!(msg, self.name);
    }
}

pub fn immediate_yields() {
    let _guard = LuaStackIntegrityGuard::new("immediate_fiber_guard");

    let (tx, rx) = Rc::new(Cell::new(0)).clones();
    let csw1 = fiber_csw();
    fiber::start(move || tx.set(69));
    let csw2 = fiber_csw();

    assert_eq!(rx.get(), 69);
    assert_eq!(csw2, csw1+1);
}

pub fn deferred_doesnt_yield() {
    let _guard = LuaStackIntegrityGuard::new("deferred_fiber_guard");

    let (tx, rx) = Rc::new(Cell::new(0)).clones();
    let csw1 = fiber_csw();
    fiber::defer(move || tx.set(96));
    let csw2 = fiber_csw();

    assert_eq!(rx.get(), 0);
    assert_eq!(csw2, csw1);

    fiber::sleep(Duration::ZERO);
    assert_eq!(rx.get(), 96);
}

pub fn start_error() {
    let _guard = LuaStackIntegrityGuard::new("fiber_error_guard");

    let _spoiler = LuaContextSpoiler::new();

    match fiber::LuaFiber::new(fiber::LuaFiberFunc(|| ())).spawn() {
        Err(e) => assert_eq!(
            format!("{}", e),
            "Lua error: Execution error: Artificial error"
        ),
        _ => panic!(),
    }

    struct LuaContextSpoiler;

    impl LuaContextSpoiler {
        fn new() -> Self {
            let mut lua: Lua = crate::hlua::global();
            lua.execute::<()>(r#"
            _fiber_new_backup = package.loaded.fiber.new
            package.loaded.fiber.new = function() error("Artificial error", 0) end
            "#).unwrap();
            Self
        }
    }

    impl Drop for LuaContextSpoiler {
        fn drop(&mut self) {
            let mut lua: Lua = crate::hlua::global();
            lua.execute::<()>(r#"
            package.loaded.fiber.new = _fiber_new_backup
            _fiber_new_backup = nil
            "#).unwrap();
        }
    }
}

pub fn require_error() {
    let _guard = LuaStackIntegrityGuard::new("fiber_error_guard");

    let _spoiler = LuaContextSpoiler::new();

    match fiber::LuaFiber::new(fiber::LuaFiberFunc(|| ())).spawn() {
        Err(e) => assert_eq!(
            format!("{}", e),
            "Lua error: Execution error: Artificial require error"
        ),
        _ => panic!(),
    }

    struct LuaContextSpoiler;

    impl LuaContextSpoiler {
        fn new() -> Self {
            let mut lua: Lua = crate::hlua::global();
            lua.execute::<()>(r#"
            _fiber_backup = package.loaded.fiber
            package.loaded.fiber = nil
            package.preload.fiber = function() error("Artificial require error", 0) end
            "#).unwrap();
            Self
        }
    }

    impl Drop for LuaContextSpoiler {
        fn drop(&mut self) {
            let mut lua: Lua = crate::hlua::global();
            lua.execute::<()>(r#"
            package.preload.fiber = nil
            package.loaded.fiber = _fiber_backup
            _fiber_backup = nil
            "#).unwrap();
        }
    }
}

struct DropCounter(Rc<Cell<usize>>);

impl Drop for DropCounter {
    fn drop(&mut self) {
        let old_count = self.0.get();
        self.0.set(old_count + 1);
    }
}

fn capture_value<T>(_: &T) {}

pub fn start_dont_join() {
    let (tx, rx) = Rc::new(Cell::new(0)).clones();
    let f = fiber::start(move || DropCounter(tx));

    assert_eq!(rx.get(), 0);
    drop(f);
    assert_eq!(rx.get(), 1);
}

pub fn start_proc_dont_join() {
    let (tx, rx) = Rc::new(Cell::new(0)).clones();
    let d = DropCounter(tx);

    assert_eq!(rx.get(), 0);
    let f = fiber::start_proc(move || capture_value(&d));
    assert_eq!(rx.get(), 1);
    drop(f);
    assert_eq!(rx.get(), 1);
}

pub fn defer_dont_join() {
    let _guard = LuaStackIntegrityGuard::new("defer_dont_join");

    let tx = Rc::new(Cell::new(0));
    let rx = Rc::downgrade(&tx);
    let f = fiber::defer(move || DropCounter(tx));

    assert_eq!(rx.strong_count(), 1);
    assert_eq!(rx.upgrade().unwrap().get(), 0);
    drop(f);
    // There's a memory leak that we can't do anything about if we drop the
    // LuaJoinHandle without joining it first
    assert_eq!(rx.strong_count(), 1);
    assert_eq!(rx.upgrade().unwrap().get(), 0);
}

pub fn defer_proc_dont_join() {
    let _guard = LuaStackIntegrityGuard::new("defer_proc_dont_join");

    let tx = Rc::new(Cell::new(0));
    let rx = Rc::downgrade(&tx);
    let d = DropCounter(tx);
    let f = fiber::defer_proc(move || capture_value(&d));

    assert_eq!(rx.strong_count(), 1);
    assert_eq!(rx.upgrade().unwrap().get(), 0);
    drop(f);
    // There's a memory leak that we can't do anything about if we drop the
    // LuaJoinHandle without joining it first
    assert_eq!(rx.strong_count(), 1);
    assert_eq!(rx.upgrade().unwrap().get(), 0);
}

pub fn immediate_with_cond() {
    let msgs = Rc::new(RefCell::new(vec![]));
    let cond = Rc::new(fiber::Cond::new());

    let fibers = (1..=3).map(|i| {
        let msgs = msgs.clone();
        let cond = cond.clone();
        fiber::start_proc(move || {
            msgs.borrow_mut().push(i);
            cond.wait();
            msgs.borrow_mut().push(i + 3);
        })
    })
        .collect::<Vec<_>>();

    assert_eq!(*msgs.borrow(), vec![1, 2, 3]);

    cond.broadcast();
    fiber::sleep(Duration::ZERO);

    assert_eq!(*msgs.borrow(), vec![1, 2, 3, 4, 5, 6]);

    for f in fibers {
        f.join()
    }
}

pub fn deferred_with_cond() {
    let msgs = Rc::new(RefCell::new(vec![]));
    let cond = Rc::new(fiber::Cond::new());

    let fibers = (1..=3).map(|i| {
        let msgs = msgs.clone();
        let cond = cond.clone();
        fiber::defer_proc(move || {
            msgs.borrow_mut().push(i);
            cond.wait();
            msgs.borrow_mut().push(i + 3);
        })
    })
        .collect::<Vec<_>>();

    assert!(msgs.borrow().is_empty());

    fiber::sleep(Duration::ZERO);

    assert_eq!(*msgs.borrow(), vec![1, 2, 3]);

    cond.broadcast();
    fiber::sleep(Duration::ZERO);

    assert_eq!(*msgs.borrow(), vec![1, 2, 3, 4, 5, 6]);

    for f in fibers {
        f.join()
    }
}

pub fn channel_send_self() {
    let (tx, rx) = fiber::channel(1);

    tx.send("hello").unwrap();

    assert_eq!(rx.recv().unwrap(), "hello");
}

pub fn channel_send_full() {
    let (tx, _rx) = fiber::channel(0);

    let e = tx.send_timeout("echo1", Duration::from_micros(1)).unwrap_err();
    assert_eq!(e, fiber::SendError::Timeout("echo1"));

    let e = tx.try_send("echo2").unwrap_err();
    assert_eq!(e, fiber::TrySendError::Full("echo2"));
}

pub fn channel_recv_empty() {
    let (_tx, rx) = fiber::channel::<()>(0);

    let e = rx.recv_timeout(Duration::from_micros(1)).unwrap_err();
    assert_eq!(e, fiber::RecvError::Timeout);

    let e = rx.try_recv().unwrap_err();
    assert_eq!(e, fiber::TryRecvError::Empty);
}

pub fn channel_unbuffered() {
    let (tx, rx) = fiber::channel(0);

    let f = fiber::defer(move || rx.recv().unwrap());

    let csw_before = fiber_csw();

    tx.send("hello").unwrap();

    assert_eq!(fiber_csw() - csw_before, 1);

    assert_eq!(f.join(), "hello")
}

pub fn channel_drop_sender() {
    let (tx, rx) = fiber::channel::<()>(0);

    let f = fiber::defer(move || rx.recv());

    drop(tx);

    assert_eq!(f.join(), None);
}

pub fn channel_dont_drop_msg() {
    let (tx, rx) = fiber::channel(1);
    tx.send("don't drop this").unwrap();
    drop(tx);
    assert_eq!(rx.recv(), Some("don't drop this"));
}

pub fn channel_1v2() {
    let (tx, rx1) = fiber::channel(0);
    let rx2 = rx1.clone();
    let f1 = fiber::defer(move || rx1.recv().unwrap());
    let f2 = fiber::defer(move || rx2.recv().unwrap());
    tx.send("hello").unwrap();
    assert_eq!(f1.join(), "hello");
    tx.send("what's up").unwrap();
    assert_eq!(f2.join(), "what's up");
}

pub fn channel_2v1() {
    let (tx1, rx) = fiber::channel(0);
    let tx2 = tx1.clone();
    let f1 = fiber::defer(move || tx1.send("how ya doin?").unwrap());
    let f2 = fiber::defer(move || tx2.send("what's good").unwrap());
    assert_eq!(rx.recv(), Some("how ya doin?"));
    assert_eq!(rx.recv(), Some("what's good"));
    f1.join();
    f2.join();
}

pub fn channel_drop() {
    let (drop_count_tx, drop_count_rx) = Rc::new(Cell::new(0)).clones();
    let (s1, s2, s3) = S(drop_count_tx).clones();

    let (tx, rx) = fiber::channel(3);
    tx.send(s1).unwrap();
    tx.send(s2).unwrap();
    tx.send(s3).unwrap();

    assert_eq!(drop_count_rx.get(), 0);
    drop((tx, rx));
    assert_eq!(drop_count_rx.get(), 3);

    #[derive(Clone, Debug)]
    struct S(Rc<Cell<usize>>);
    impl Drop for S {
        fn drop(&mut self) {
            let old_count = self.0.get();
            self.0.set(old_count + 1);
        }
    }
}

pub fn channel_circle() {
    let ((tx1, tx2, tx3), (rx1, rx2, rx3)) = fiber::channel_clones(0);

    let f1 = fiber::defer_proc(move || {
        let mut msg = rx1.recv().unwrap();
        Vec::push(&mut msg, 3);
        tx1.send(msg).unwrap()
    });

    let f2 = fiber::defer(move || {
        tx2.send(vec![1]).unwrap();
        rx2.recv().unwrap()
    });

    let mut msg = rx3.recv().unwrap();
    msg.push(2);
    tx3.send(msg).unwrap();

    assert_eq!(f2.join(), vec![1, 2, 3]);
    let () = f1.join();
}

pub fn channel_iter() {
    let ((tx1, tx2, tx3), (rx,)) = fiber::channel_clones(0);
    let f1 = fiber::defer_proc(move || tx1.send(1).unwrap());
    let f2 = fiber::defer_proc(move || tx2.send(2).unwrap());
    let f3 = fiber::defer_proc(move || tx3.send(3).unwrap());
    let mut i = 0;
    for msg in &rx {
        i += 1;
        assert_eq!(msg, i);
        if i == 3 { break }
    }
    f1.join();
    f2.join();
    f3.join();
}

pub fn channel_into_iter() {
    let ((tx1, tx2, tx3), (rx,)) = fiber::channel_clones(0);
    let f1 = fiber::defer_proc(move || tx1.send(1).unwrap());
    let f2 = fiber::defer_proc(move || tx2.send(2).unwrap());
    let f3 = fiber::defer_proc(move || tx3.send(3).unwrap());
    let mut i = 0;
    for msg in rx {
        i += 1;
        assert_eq!(msg, i);
        if i == 3 { break }
    }
    f1.join();
    f2.join();
    f3.join();
}

pub fn channel_try_iter() {
    let (tx, rx) = fiber::channel(3);
    tx.send(1).unwrap();
    tx.send(2).unwrap();
    tx.send(3).unwrap();
    assert_eq!(rx.try_iter().collect::<Vec<_>>(), vec![1, 2, 3]);
}

pub fn channel_as_mutex() {
    let ((lock1, lock2, lock3), (release1, release2, release3)) =
        fiber::channel_clones(1);
    let ((log0, log1, log2, log3), (log_out,)) = fiber::channel_clones(14);
    let shared_resource = std::cell::UnsafeCell::new(vec![]);
    let sr = shared_resource.get();

    let f1 = fiber::defer_proc(move || {
        log1.send("f1:lock").unwrap();
        lock1.send(()).unwrap();      // Capture the lock
        log1.send("f1:critical").unwrap();
        fiber::sleep(Duration::ZERO);       // Tease the other fibers
        unsafe { (&mut *sr).push(1); }      // Access the critical section
        let () = release1.recv().unwrap(); // Release the lock
        log1.send("f1:release").unwrap();
    });

    let f2 = fiber::defer_proc(move || {
        log2.send("f2:lock").unwrap();
        lock2.send(()).unwrap();      // Capture the lock
        log2.send("f2:critical").unwrap();
        fiber::sleep(Duration::ZERO);       // Tease the other fibers
        unsafe { (&mut *sr).push(2); }      // Access the critical section
        let () = release2.recv().unwrap(); // Release the lock
        log2.send("f2:release").unwrap();
    });

    let f3 = fiber::defer_proc(move || {
        log3.send("f3:lock").unwrap();
        lock3.send(()).unwrap();      // Capture the lock
        log3.send("f3:critical").unwrap();
        fiber::sleep(Duration::ZERO);       // Tease the other fibers
        log3.send("f3:release").unwrap();
        unsafe { (&mut *sr).push(3); }      // Access the critical section
        let () = release3.recv().unwrap(); // Release the lock
    });

    log0.send("main:sleep").unwrap();
    fiber::sleep(Duration::ZERO);

    log0.send("main:join(f2)").unwrap();
    f2.join();
    log0.send("main:join(f1)").unwrap();
    f1.join();
    log0.send("main:join(f3)").unwrap();
    f3.join();
    log0.send("main:done").unwrap();

    assert_eq!(unsafe { &*sr }, &[1, 2, 3]);

    assert_eq!(
        log_out.try_iter().collect::<Vec<_>>(),
        vec![
            "main:sleep",
                        "f1:lock",
                        "f1:critical",
                                    "f2:lock",
                                                "f3:lock",
            "main:join(f2)",
                        "f1:release",
                                    "f2:critical",
                                    "f2:release",
                                                "f3:critical",
            "main:join(f1)",
            "main:join(f3)",
                                                "f3:release",
            "main:done",
        ]
    );
}

