use tarantool::{
    hlua::{LuaFunction, LuaTable, yego::*},
    ffi::tarantool::luaT_state,
};

pub fn print() {
    let mut lua = crate::hlua::global();

    let mut print: LuaFunction<_> = lua.get("print").unwrap();
    let () = print.call_with_args("hello").unwrap();
}

pub fn json() {
    let mut lua = crate::hlua::global();
    let mut require: LuaFunction<_> = lua.get("require").unwrap();
    let mut json: LuaTable<_> = require.call_with_args("json").unwrap();
    let mut encode: LuaFunction<_> = json.get("encode").unwrap();
    let mut table = std::collections::HashMap::new();
    let res: String = encode.call_with_args(vec![1, 2, 3]).unwrap();
    assert_eq!(res, "[1,2,3]");
    table.insert("a", "b");
    let res: String = encode.call_with_args(table).unwrap();
    assert_eq!(res, r#"{"a":"b"}"#);
}

pub fn yego() {
    let lua = EmptyStack(unsafe { luaT_state() });
    let lua = lua.push_integer(420);
    assert_eq!(lua.to_integer(Minus1), 420);
}

