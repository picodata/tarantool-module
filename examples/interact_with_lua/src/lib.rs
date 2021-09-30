#[macro_use]
extern crate tarantool_derive;

use std::os::raw::c_int;
use tarantool::lua::{LuaState, ToLuaTable};
use tarantool::tuple::{FunctionArgs, FunctionCtx};

#[derive(ToLuaTable)]
struct Args {
    a: i32,
    b: i32,
}

#[derive(ToLuaTable)]
struct Args2(i32, i32);

#[no_mangle]
pub extern "C" fn run(_: FunctionCtx, _: FunctionArgs) -> c_int {
    let lua = LuaState::global();

    let result: i32 = lua.call("sum", &Args { a: 99, b: 100 }).unwrap();
    assert_eq!(result, 199);

    let result: i32 = lua.call("sum", &Args2(97, 98)).unwrap();
    assert_eq!(result, 195);

    0
}
