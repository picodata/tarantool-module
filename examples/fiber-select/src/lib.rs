use std::time::Duration;
use std::time::Instant;
use tarantool::fiber;

#[tarantool::proc]
fn main() {
    fiber::set_name("main fiber");
    let service_fiber_id = fiber::Builder::new()
        .name("super important service")
        .func(my_service)
        .defer_non_joinable()
        .expect("creating a fiber shouldn't fail")
        .expect("fiber_id is supported in picodata");

    fiber::cancel(service_fiber_id);
    println!("main exited");
}

fn my_service() {
    let fiber_name = fiber::name();
    let fiber_id = fiber::id();
    while !fiber::is_cancelled() {
        // Do some important work
        println!("[{fiber_id}:{fiber_name}] doing some work!");
        fiber::sleep(Duration::from_secs(1));
    }

    println!("[{fiber_name}] we've been cancelled!");
}

////////////////////////////////////////////////////////////////////////////////
// auto cancellable fiber
////////////////////////////////////////////////////////////////////////////////

fn hacky_auto_cancellable_fiber_example() {
    let f_id = spawn_cancellable(|| {
        let fiber_name = fiber::name();
        let fiber_id = fiber::id();
        loop {
            println!("[{fiber_id}:{fiber_name}] still alive");
            sleep_and_check(Duration::from_millis(100));
        }
    });
    fiber::sleep(Duration::from_secs(2));
    fiber::cancel(f_id);
}

fn sleep_and_check(duration: Duration) {
    fiber::sleep(duration);
    raise_if_cancelled();
}

fn spawn_cancellable<F: FnOnce()>(f: F) -> fiber::FiberId {
    fiber::Builder::new()
        .func(|| {
            let lua = tarantool::lua_state();
            _ = tarantool::tlua::protected_call(lua, |_| f());
        })
        .name("auto-cancellable")
        .start_non_joinable()
        .unwrap()
        .unwrap()
}

fn raise_if_cancelled() {
    if fiber::is_cancelled() {
        let lua = tarantool::lua_state();
        tarantool::tlua::error!(lua, "cancelled");
    }
}

////////////////////////////////////////////////////////////////////////////////
// mincore_bench
////////////////////////////////////////////////////////////////////////////////

fn mincore_bench() {
    eprintln!("debug assertions enabled: {}", cfg!(debug_assertions));
    // eprintln!("MINCORE: {}", fiber::MINCORE);
    eprintln!("---");

    const N: u64 = 1000;

    for _ in 0..3 {
        let mut t_total: f64 = 0.0;
        let mut t_sd_temp: f64 = 0.0;
        for _ in 0..N {
            fiber::Builder::new()
                .func(|| {
                    let ctx = unsafe { fiber::try_context_mut().unwrap() };
                    let t = (ctx.time_started - ctx.time_created).as_secs_f64();
                    t_total += t;
                    t_sd_temp += t * t / (N as f64);
                })
                .start()
                .unwrap()
                .join();
        }
        let mean = t_total / (N as f64);
        let sd = f64::sqrt((t_sd_temp - mean * mean) as _);
        eprintln!(
            "start joinable {:?} +-{:?}",
            Duration::from_secs_f64(mean),
            Duration::from_secs_f64(sd)
        );
    }
    eprintln!("---");

    for _ in 0..3 {
        let mut t_total: f64 = 0.0;
        let mut t_sd_temp: f64 = 0.0;
        for _ in 0..N {
            fiber::Builder::new()
                .func(|| {
                    let ctx = unsafe { fiber::try_context_mut().unwrap() };
                    let t = (ctx.time_started - ctx.time_created).as_secs_f64();
                    t_total += t;
                    t_sd_temp += t * t / (N as f64);
                })
                .defer()
                .unwrap()
                .join();
        }
        let mean = t_total / (N as f64);
        let sd = f64::sqrt((t_sd_temp - mean * mean) as _);
        eprintln!(
            "defer joinable {:?} +-{:?}",
            Duration::from_secs_f64(mean),
            Duration::from_secs_f64(sd)
        );
    }
    eprintln!("---");

    for _ in 0..3 {
        let mut t_total: f64 = 0.0;
        let mut t_sd_temp: f64 = 0.0;
        for _ in 0..N {
            let id = fiber::Builder::new()
                .func(|| {
                    let ctx = unsafe { fiber::try_context_mut().unwrap() };
                    let t = (ctx.time_started - ctx.time_created).as_secs_f64();
                    t_total += t;
                    t_sd_temp += t * t / (N as f64);
                })
                .start_non_joinable()
                .unwrap()
                .unwrap();
            fiber::cancel(id);
        }
        let mean = t_total / (N as f64);
        let sd = f64::sqrt((t_sd_temp - mean * mean) as _);
        eprintln!(
            "start non-joinable {:?} +-{:?}",
            Duration::from_secs_f64(mean),
            Duration::from_secs_f64(sd)
        );
    }
    eprintln!("---");

    for _ in 0..3 {
        let mut t_total: f64 = 0.0;
        let mut t_sd_temp: f64 = 0.0;
        for _ in 0..N {
            let id = fiber::Builder::new()
                .func(|| {
                    let ctx = unsafe { fiber::try_context_mut().unwrap() };
                    let t = (ctx.time_started - ctx.time_created).as_secs_f64();
                    t_total += t;
                    t_sd_temp += t * t / (N as f64);
                })
                .defer_non_joinable()
                .unwrap()
                .unwrap();
            fiber::reschedule();
            fiber::cancel(id);
        }
        let mean = t_total / (N as f64);
        let sd = f64::sqrt((t_sd_temp - mean * mean) as _);
        eprintln!(
            "defer non-joinable {:?} +-{:?}",
            Duration::from_secs_f64(mean),
            Duration::from_secs_f64(sd)
        );
    }
}
