use tarantool::proc;

#[proc]
fn loop_set_error() {
    tarantool::set_error!(0, "oops");

    // first call to format with given format string results in some memory allocation,
    // which we don't care about
    format!("oops: {}", 0);

    // check that we don't leak any memory when calling set_error with format args
    for i in 0..100 {
        tarantool::set_error!(0, "oops: {}", i);
    }
}

#[proc]
fn leak_mem() {
    let data: Vec<_> = (0..1_000_000).into_iter().collect();
    std::mem::forget(data);
}
