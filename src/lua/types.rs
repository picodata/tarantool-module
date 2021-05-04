use std::fmt;
use std::os::raw::c_int;

use crate::lua::ffi;

use crate::lua::context::Context;

/// Type of Lua integer numbers.
pub type Integer = ffi::lua_Integer;
/// Type of Lua floating point numbers.
pub type Number = ffi::lua_Number;

pub(crate) struct LuaRef {
    pub(crate) ctx: Context,
    pub(crate) index: c_int,
}

impl fmt::Debug for LuaRef {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "Ref({})", self.index)
    }
}

impl Clone for LuaRef {
    fn clone(&self) -> Self {
        self.ctx.clone_ref(self)
    }
}

impl Drop for LuaRef {
    fn drop(&mut self) {
        self.ctx.drop_ref(self)
    }
}
