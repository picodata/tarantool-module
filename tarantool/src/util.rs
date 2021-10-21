use crate::{
    ffi::{lua::{self, *}, tarantool::*},
    hlua::*,
};

macro_rules! c_ptr {
    ($s:literal) => {
        concat!($s, "\0").as_bytes().as_ptr() as *mut i8
    };
}

pub fn dump_stack(lua: *mut lua_State) {
    define_dump();
    let top = unsafe { lua_gettop(lua) };
    for i in 1..=top {
        unsafe {
            lua_getglobal(lua, c_ptr!("YEGO_dump_value"));
            lua_pushvalue(lua, i);
            if lua_pcall(lua, 1, 0, 0) == LUA_ERRRUN {
                eprintln!(
                    "\x1b[31m{:?}\x1b[0m",
                    std::ffi::CStr::from_ptr(lua_tostring(lua, -1)),
                );
            };
        }
        // let type_code = unsafe { lua_type(lua, i) };
        // eprintln!("{}: {}", i, type_name(type_code));
    }
}

pub fn dump_global_stack() {
    dump_stack(unsafe { luaT_state() })
}

pub fn tarantool_state() -> Lua<'static> {
    unsafe { Lua::from_existing_state(luaT_state(), false) }
}

pub fn type_name(code: i32) -> std::borrow::Cow<'static, str> {
    match code {
        LUA_TNIL => "nil".into(),
        LUA_TBOOLEAN => "boolean".into(),
        LUA_TLIGHTUSERDATA => "lightuserdata".into(),
        LUA_TNUMBER => "number".into(),
        LUA_TSTRING => "string".into(),
        LUA_TTABLE => "table".into(),
        LUA_TFUNCTION => "function".into(),
        LUA_TUSERDATA => "userdata".into(),
        LUA_TTHREAD => "thread".into(),
        _ => format!("wtf is {}?", code).into(),
    }
}

pub fn dump_globals() {
    let mut lua = tarantool_state();
    let l = unsafe { luaT_state() };
    lua.execute::<()>(r#"
        function dump(v)
            for k, v in pairs(v) do
                io.stderr:write(string.format("'%s': %s\n", k, v))
            end
        end
    "#).unwrap();
    unsafe { lua::lua_getglobal(l, b"dump\0".as_ptr() as *const i8) };
    unsafe { lua::lua_pushvalue(l, lua::LUA_GLOBALSINDEX) };
    if 2 == unsafe { lua::lua_pcall(l, 1, 0, 0) } {
        dbg!(unsafe { std::ffi::CStr::from_ptr(lua::lua_tostring(l, -1)) });
    }
}

pub fn define_dump() {
    let mut lua = tarantool_state();
    lua.execute::<()>(r#"
        function YEGO_dump_value(v)
            io.stderr:write(string.format("%s(%s)\n", type(v), v))
        end
    "#).unwrap();
}
