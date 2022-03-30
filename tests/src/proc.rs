use tarantool::{
    self,
    tuple::{FunctionArgs, FunctionCtx},
};
use std::os::raw::c_int;

#[tarantool::proc(tarantool = "::tarantool")]
fn stored_proc_example(
    x: i32,
    _y: String,
    _z: Vec<(String, Vec<(i32, i32, i32)>)>,
) -> Result<(i32, i32), String> {
    if x == 3 {
        Ok((1, 2))
    } else {
        Err("fuck".into())
    }
}

pub fn smoke() {
    fn check(f: unsafe extern "C" fn(FunctionCtx, FunctionArgs) -> c_int) -> *const () {
        &f as *const _ as _
    }
    assert_ne!(check(stored_proc_example), std::ptr::null());
}

