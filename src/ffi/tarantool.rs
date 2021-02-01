use std::os::raw::{c_int};

use super::types::*;

extern "C" {
    pub fn luaT_state() -> *mut lua_State;
    pub fn luaT_call(l: *mut lua_State, nargs: c_int, nreturns: c_int) -> isize;
}