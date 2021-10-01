use std::ffi::{CStr, CString};

use crate::ffi::lua::{
    luaT_state, lua_State, lua_getglobal, lua_isnil, lua_pcall, lua_pop, lua_pushinteger,
    lua_pushstring, lua_tointeger, lua_tostring,
};

pub struct LuaState {
    pub inner: *mut lua_State,
}

impl LuaState {
    pub fn global() -> Self {
        unsafe {
            LuaState {
                inner: luaT_state(),
            }
        }
    }

    pub fn call<A, R>(&self, name: &str, args: &A) -> Result<R, LuaCallError>
    where
        A: ToLuaTable,
        R: FromLuaValue,
    {
        let name = CString::new(name).expect("incorrect name string");
        unsafe {
            lua_getglobal(self.inner, name.into_raw());
            args.push_fields(self)
                .map_err(|e| LuaCallError::SetArgs(e))?;
            let res = lua_pcall(self.inner, args.fields_count(), 1, 0);
            if res != 0 {
                let msg = CStr::from_ptr(lua_tostring(self.inner, -1));
                return Err(LuaCallError::Runtime(msg.to_string_lossy().to_string()));
            }
        };
        let result = R::from_lua_value(self).map_err(|e| LuaCallError::GetResult(e))?;
        Ok(result)
    }
}

pub enum LuaValue {
    Number(f64),
    String(String),
    // ...
}

#[derive(Debug, Fail)]
pub enum LuaCallError {
    #[fail(display = "Failed to set arguments: {}", _0)]
    SetArgs(ToLuaConversionError),

    #[fail(display = "Failed to get result: {}", _0)]
    GetResult(FromLuaConversionError),

    #[fail(display = "Runtime error: {}", _0)]
    Runtime(String),
}

#[derive(Debug, Fail)]
pub enum ToLuaConversionError {
    #[fail(display = "Unknown error")]
    Unknown,
}

#[derive(Debug, Fail)]
pub enum FromLuaConversionError {
    #[fail(display = "Value is Nil")]
    NilValue,
}

pub trait ToLuaTable {
    fn to_lua_table(&self) -> Result<(), ToLuaConversionError>;
    fn fields_count(&self) -> i32;
    fn push_fields(&self, state: &LuaState) -> Result<(), ToLuaConversionError>;
}

pub trait ToLuaValue {
    fn push_lua_value(&self, state: &LuaState) -> Result<(), ToLuaConversionError>;
}

impl ToLuaValue for i32 {
    fn push_lua_value(&self, state: &LuaState) -> Result<(), ToLuaConversionError> {
        unsafe { lua_pushinteger(state.inner, (*self) as isize) }
        Ok(())
    }
}

impl ToLuaValue for &str {
    fn push_lua_value(&self, state: &LuaState) -> Result<(), ToLuaConversionError> {
        let name = CString::new(*self).expect("incorrect string");
        unsafe { lua_pushstring(state.inner, name.into_raw()) };
        Ok(())
    }
}

pub trait FromLuaValue: Sized {
    fn from_lua_value(state: &LuaState) -> Result<Self, FromLuaConversionError>;
}

impl FromLuaValue for i32 {
    fn from_lua_value(state: &LuaState) -> Result<Self, FromLuaConversionError> {
        let result = unsafe {
            if lua_isnil(state.inner, -1) {
                Err(FromLuaConversionError::NilValue)
            } else {
                Ok(lua_tointeger(state.inner, -1) as Self)
            }
        };
        unsafe { lua_pop(state.inner, 1) };
        result
    }
}
