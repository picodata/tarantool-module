use std::os::raw::c_int;

use crate::lua::ffi;

use crate::lua::error::Result;
use crate::lua::function::Function;
use crate::lua::string::String;
use crate::lua::table::Table;
use crate::lua::types::{Integer, LuaRef, Number};
use crate::lua::util::{
    assert_stack, check_stack, protect_lua, protect_lua_closure, push_string, StackGuard,
};
use crate::lua::value::{Nil, ToLua, Value};

/// Top level Lua struct which holds Lua stack/thread itself.
#[derive(Debug)]
pub struct Context {
    pub(crate) state: *mut ffi::lua_State,
    pub(crate) ref_thread: *mut ffi::lua_State,
    pub(crate) ref_stack_size: c_int,
    pub(crate) ref_stack_max: c_int,
    pub(crate) ref_free: Vec<c_int>,
}

impl Drop for Context {
    fn drop(&mut self) {
        unsafe {
            rlua_debug_assert!(
                ffi::lua_gettop(self.ref_thread) == self.ref_stack_max
                    && self.ref_stack_max as usize == self.ref_free.len(),
                "reference leak detected"
            );
        }
    }
}

impl Context {
    pub unsafe fn new() -> Context {
        let state = ffi::luaT_state();

        let ref_thread = ffi::lua_newthread(state);
        ffi::luaL_ref(state, ffi::LUA_REGISTRYINDEX);

        rlua_debug_assert!(ffi::lua_gettop(state) == 0, "stack leak during creation");
        assert_stack(state, ffi::LUA_MINSTACK);

        Context {
            state: state,
            ref_thread: ref_thread,
            ref_stack_size: ffi::LUA_MINSTACK - 1,
            ref_stack_max: 0,
            ref_free: Vec::new(),
        }
    }

    /// Create and return an interned Lua string.  Lua strings can be arbitrary [u8] data including
    /// embedded nulls, so in addition to `&str` and `&String`, you can also pass plain `&[u8]`
    /// here.
    pub fn create_string<S>(&self, s: &S) -> Result<String>
    where
        S: ?Sized + AsRef<[u8]>,
    {
        unsafe {
            let _sg = StackGuard::new(self.state);
            assert_stack(self.state, 4);
            push_string(self.state, s)?;
            Ok(String(self.pop_ref()))
        }
    }

    /// Creates and returns a new table.
    pub fn create_table(&self) -> Result<Table> {
        unsafe {
            let _sg = StackGuard::new(self.state);
            assert_stack(self.state, 3);
            unsafe extern "C" fn new_table(state: *mut ffi::lua_State) -> c_int {
                ffi::lua_newtable(state);
                1
            }
            protect_lua(self.state, 0, new_table)?;
            Ok(Table(self.pop_ref()))
        }
    }

    /// Creates a table and fills it with values from an iterator.
    pub fn create_table_from<K, V, I>(&self, cont: I) -> Result<Table>
    where
        K: ToLua,
        V: ToLua,
        I: IntoIterator<Item = (K, V)>,
    {
        unsafe {
            let _sg = StackGuard::new(self.state);
            // `Lua` instance assumes that on any callback, the Lua stack has at least LUA_MINSTACK
            // slots available to avoid panics.
            check_stack(self.state, 5 + ffi::LUA_MINSTACK)?;

            unsafe extern "C" fn new_table(state: *mut ffi::lua_State) -> c_int {
                ffi::lua_newtable(state);
                1
            }
            protect_lua(self.state, 0, new_table)?;

            for (k, v) in cont {
                self.push_value(k.to_lua(&self)?)?;
                self.push_value(v.to_lua(&self)?)?;
                unsafe extern "C" fn raw_set(state: *mut ffi::lua_State) -> c_int {
                    ffi::lua_rawset(state, -3);
                    1
                }
                protect_lua(self.state, 3, raw_set)?;
            }
            Ok(Table(self.pop_ref()))
        }
    }

    /// Creates a table from an iterator of values, using `1..` as the keys.
    pub fn create_sequence_from<T, I>(&self, cont: I) -> Result<Table>
    where
        T: ToLua,
        I: IntoIterator<Item = T>,
    {
        self.create_table_from(cont.into_iter().enumerate().map(|(k, v)| (k + 1, v)))
    }

    /// Attempts to coerce a Lua value into a String in a manner consistent with Lua's internal
    /// behavior.
    ///
    /// To succeed, the value must be a string (in which case this is a no-op), an integer, or a
    /// number.
    pub fn coerce_string(&self, v: Value) -> Result<Option<String>> {
        Ok(match v {
            Value::String(s) => Some(s),
            v => unsafe {
                let _sg = StackGuard::new(self.state);
                assert_stack(self.state, 4);

                self.push_value(v)?;
                if protect_lua_closure(self.state, 1, 1, |state| {
                    !ffi::lua_tostring(state, -1).is_null()
                })? {
                    Some(String(self.pop_ref()))
                } else {
                    None
                }
            },
        })
    }

    /// Attempts to coerce a Lua value into an integer in a manner consistent with Lua's internal
    /// behavior.
    ///
    /// To succeed, the value must be an integer, a floating point number that has an exact
    /// representation as an integer, or a string that can be converted to an integer. Refer to the
    /// Lua manual for details.
    pub fn coerce_integer(&self, v: Value) -> Result<Option<Integer>> {
        Ok(match v {
            Value::Integer(i) => Some(i),
            v => unsafe {
                let _sg = StackGuard::new(self.state);
                assert_stack(self.state, 2);

                self.push_value(v)?;
                let mut isint = 0;
                let i = ffi::lua_tointegerx(self.state, -1, &mut isint);
                if isint == 0 {
                    None
                } else {
                    Some(i)
                }
            },
        })
    }

    /// Attempts to coerce a Lua value into a Number in a manner consistent with Lua's internal
    /// behavior.
    ///
    /// To succeed, the value must be a number or a string that can be converted to a number. Refer
    /// to the Lua manual for details.
    pub fn coerce_number(&self, v: Value) -> Result<Option<Number>> {
        Ok(match v {
            Value::Number(n) => Some(n),
            v => unsafe {
                let _sg = StackGuard::new(self.state);
                assert_stack(self.state, 2);

                self.push_value(v)?;
                let mut isnum = 0;
                let n = ffi::lua_tonumberx(self.state, -1, &mut isnum);
                if isnum == 0 {
                    None
                } else {
                    Some(n)
                }
            },
        })
    }

    // Uses 2 stack spaces, does not call checkstack.
    pub unsafe fn push_value(&self, value: Value) -> Result<()> {
        match value {
            Value::Nil => {
                ffi::lua_pushnil(self.state);
            }

            Value::Boolean(b) => {
                ffi::lua_pushboolean(self.state, if b { 1 } else { 0 });
            }

            Value::Integer(i) => {
                ffi::lua_pushinteger(self.state, i);
            }

            Value::Number(n) => {
                ffi::lua_pushnumber(self.state, n);
            }

            Value::String(s) => {
                self.push_ref(&s.0);
            }

            Value::Table(t) => {
                self.push_ref(&t.0);
            }

            Value::Function(f) => {
                self.push_ref(&f.0);
            }
        }

        Ok(())
    }

    // Uses 2 stack spaces, does not call checkstack.
    pub unsafe fn pop_value(&self) -> Value {
        match ffi::lua_type(self.state, -1) {
            ffi::LUA_TNIL => {
                ffi::lua_pop(self.state, 1);
                Nil
            }

            ffi::LUA_TBOOLEAN => {
                let b = Value::Boolean(ffi::lua_toboolean(self.state, -1) != 0);
                ffi::lua_pop(self.state, 1);
                b
            }

            ffi::LUA_TNUMBER => {
                if ffi::lua_isinteger(self.state, -1) != 0 {
                    let i = Value::Integer(ffi::lua_tointeger(self.state, -1));
                    ffi::lua_pop(self.state, 1);
                    i
                } else {
                    let n = Value::Number(ffi::lua_tonumber(self.state, -1));
                    ffi::lua_pop(self.state, 1);
                    n
                }
            }

            ffi::LUA_TSTRING => Value::String(String(self.pop_ref())),

            ffi::LUA_TTABLE => Value::Table(Table(self.pop_ref())),

            ffi::LUA_TFUNCTION => Value::Function(Function(self.pop_ref())),

            _ => rlua_panic!("LUA_TNONE in pop_value"),
        }
    }

    // Pushes a LuaRef value onto the stack, uses 1 stack space, does not call checkstack
    pub(crate) unsafe fn push_ref(&self, lref: &LuaRef) {
        ffi::lua_pushvalue(self.ref_thread, lref.index);
        ffi::lua_xmove(self.ref_thread, self.state, 1);
    }

    // Pops the topmost element of the stack and stores a reference to it.  This pins the object,
    // preventing garbage collection until the returned `LuaRef` is dropped.
    //
    // References are stored in the stack of a specially created auxiliary thread that exists only
    // to store reference values.  This is much faster than storing these in the registry, and also
    // much more flexible and requires less bookkeeping than storing them directly in the currently
    // used stack.  The implementation is somewhat biased towards the use case of a relatively small
    // number of short term references being created, and `RegistryKey` being used for long term
    // references.
    pub(crate) unsafe fn pop_ref(&self) -> LuaRef {
        ffi::lua_xmove(self.state, self.ref_thread, 1);
        let index = self.ref_stack_pop();
        LuaRef { ctx: self, index }
    }

    pub(crate) fn clone_ref(&self, lref: &LuaRef) -> LuaRef {
        unsafe {
            ffi::lua_pushvalue(self.ref_thread, lref.index);
            let index = self.ref_stack_pop();
            LuaRef { ctx: self, index }
        }
    }

    pub(crate) fn drop_ref(&mut self, lref: &mut LuaRef) {
        unsafe {
            ffi::lua_pushnil(self.ref_thread);
            ffi::lua_replace(self.ref_thread, lref.index);
            self.ref_free.push(lref.index);
        }
    }

    /// Returns a handle to the global environment.
    pub fn globals(&self) -> Table {
        unsafe {
            let _sg = StackGuard::new(self.state);
            assert_stack(self.state, 2);
            ffi::lua_rawgeti(self.state, ffi::LUA_REGISTRYINDEX, ffi::LUA_RIDX_GLOBALS);
            Table(self.pop_ref())
        }
    }

    unsafe fn ref_stack_pop(&mut self) -> c_int {
        if let Some(free) = self.ref_free.pop() {
            ffi::lua_replace(self.ref_thread, free);
            free
        } else {
            if self.ref_stack_max >= self.ref_stack_size {
                // It is a user error to create enough references to exhaust the Lua max stack size for
                // the ref thread.
                if ffi::lua_checkstack(self.ref_thread, self.ref_stack_size) == 0 {
                    rlua_panic!("cannot create a Lua reference, out of auxiliary stack space");
                }
                self.ref_stack_size *= 2;
            }
            self.ref_stack_max += 1;
            self.ref_stack_max
        }
    }
}
