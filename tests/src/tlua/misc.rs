use tarantool::tlua::{
    self,
    AsLua,
    Lua,
    LuaFunction,
    LuaTable,
    PushGuard,
    TuplePushError::{First, Other},
};
use crate::common::LuaStackIntegrityGuard;

pub fn print() {
    let lua = tarantool::lua_state();
    let print: LuaFunction<_> = lua.get("print").unwrap();
    let () = print.call_with_args("hello").unwrap();
}

pub fn json() {
    let lua = tarantool::lua_state();
    let require: LuaFunction<_> = lua.get("require").unwrap();
    let json: LuaTable<_> = require.call_with_args("json").unwrap();
    let encode: LuaFunction<_> = json.get("encode").unwrap();
    let mut table = std::collections::HashMap::new();
    let res: String = encode.call_with_args(vec![1, 2, 3]).unwrap();
    assert_eq!(res, "[1,2,3]");
    table.insert("a", "b");
    let res: String = encode.call_with_args(table).unwrap();
    assert_eq!(res, r#"{"a":"b"}"#);
}

#[rustfmt::skip]
pub fn dump_stack() {
    eprintln!();
    let lua = Lua::new();
    lua.openlibs();
    let mut buf = std::io::Cursor::new(Vec::with_capacity(0x1000));
    let lua = lua
        .push("hello")
        .push(3.14)
        .push(false)
        .push(420);
    unsafe {
        tarantool::tlua::debug::dump_stack_raw_to(lua.as_lua(), &mut buf).unwrap()
    }
    assert_eq!(
        String::from_utf8_lossy(buf.into_inner().as_slice()),
        r#"1: string(hello)
2: number(3.14)
3: boolean(false)
4: number(420)
"#
    )
}

#[rustfmt::skip]
pub fn dump_stack_raw() {
    eprintln!();
    let lua = Lua::new();
    lua.openlibs();
    let mut buf = std::io::Cursor::new(Vec::with_capacity(0x1000));
    let lua = lua
        .push("hello")
        .push(3.14)
        .push(false)
        .push(420);
    unsafe {
        tarantool::tlua::debug::dump_stack_raw_to(lua.as_lua(), &mut buf).unwrap();
    }
    assert_eq!(
        String::from_utf8_lossy(buf.into_inner().as_slice()),
        r#"1: string(hello)
2: number(3.14)
3: boolean(false)
4: number(420)
"#
    )
}

pub fn error_during_push_tuple() {
    #[derive(Debug, PartialEq, Eq)]
    struct CustomError;
    #[derive(Debug, PartialEq, Eq, Hash)]
    struct S;
    impl<L: AsLua> tlua::Push<L> for S {
        type Err = CustomError;
        fn push_to_lua(&self, lua: L) -> Result<PushGuard<L>, (CustomError, L)> {
            Err((CustomError, lua))
        }
    }
    impl<L: AsLua> tlua::PushOne<L> for S {}
    impl<L: AsLua> tlua::PushInto<L> for S {
        type Err = CustomError;
        fn push_into_lua(self, lua: L) -> Result<PushGuard<L>, (CustomError, L)> {
            Err((CustomError, lua))
        }
    }
    impl<L: AsLua> tlua::PushOneInto<L> for S {}

    let lua = Lua::new();

    let lua = {
        let _guard = LuaStackIntegrityGuard::new("push_tuple_by_val_error", &lua);
        let (e, lua) = lua.try_push((1, 2, 3, S)).unwrap_err();
        assert_eq!(e, Other(Other(Other(First(CustomError)))));
        lua
    };

    let lua = {
        let _guard = LuaStackIntegrityGuard::new("push_tuple_by_ref_error", &lua);
        let (e, lua) = lua.try_push(&(1, 2, 3, S)).unwrap_err();
        assert_eq!(e, Other(Other(Other(First(CustomError)))));
        lua
    };

    drop(lua);
}

