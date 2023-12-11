use std::time::Duration;
use std::time::Instant;
use tarantool::fiber;

#[tarantool::proc]
fn main() {
    let service_fiber_id = fiber::Builder::new()
        .name("super important service")
        .func(my_service)
        .defer_non_joinable()
        .unwrap();
    let service_fiber_id = service_fiber_id.expect("fiber_id is supported in picodata");

    fiber::cancel(service_fiber_id);
    println!("main exited");
}

fn my_service() {
    let fiber_name = fiber::name();
    while !fiber::is_cancelled() {
        // Do some important work
        println!("[{}], doing some work!", fiber_name);
        fiber::sleep(Duration::from_secs(1));
    }

    println!("[{}] we've been cancelled!", fiber_name);
}
