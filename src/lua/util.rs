use std::borrow::Cow;
use std::os::raw::{c_char, c_int, c_void};
use std::{ptr, slice};

use crate::lua::ffi;

use crate::lua::error::{Error, Result};

// Checks that Lua has enough free stack space for future stack operations.  On failure, this will
// panic with an internal error message.
pub unsafe fn assert_stack(state: *mut ffi::lua_State, amount: c_int) {
    // TODO: This should only be triggered when there is a logic error in `rlua`.  In the future,
    // when there is a way to be confident about stack safety and test it, this could be enabled
    // only when `cfg!(debug_assertions)` is true.
    rlua_assert!(
        ffi::lua_checkstack(state, amount) != 0,
        "out of stack space"
    );
}

// Checks that Lua has enough free stakc space and returns `Error::StackError` on failure.
pub unsafe fn check_stack(state: *mut ffi::lua_State, amount: c_int) -> Result<()> {
    if ffi::lua_checkstack(state, amount) == 0 {
        Err(Error::StackError)
    } else {
        Ok(())
    }
}

pub struct StackGuard {
    state: *mut ffi::lua_State,
    top: c_int,
}

impl StackGuard {
    // Creates a StackGuard instance with wa record of the stack size, and on Drop will check the
    // stack size and drop any extra elements.  If the stack size at the end is *smaller* than at
    // the beginning, this is considered a fatal logic error and will result in a panic.
    pub unsafe fn new(state: *mut ffi::lua_State) -> StackGuard {
        StackGuard {
            state,
            top: ffi::lua_gettop(state),
        }
    }
}

impl Drop for StackGuard {
    fn drop(&mut self) {
        unsafe {
            let top = ffi::lua_gettop(self.state);
            if top > self.top {
                ffi::lua_settop(self.state, self.top);
            } else if top < self.top {
                rlua_panic!("{} too many stack values popped", self.top - top);
            }
        }
    }
}

// Call a function that calls into the Lua API and may trigger a Lua error (longjmp) in a safe way.
// Wraps the inner function in a call to `lua_pcall`, so the inner function only has access to a
// limited lua stack.  `nargs` is the same as the the parameter to `lua_pcall`, and `nresults` is
// always LUA_MULTRET.  Internally uses 2 extra stack spaces, and does not call checkstack.
// Provided function must *never* panic.
pub unsafe fn protect_lua(
    state: *mut ffi::lua_State,
    nargs: c_int,
    f: unsafe extern "C" fn(*mut ffi::lua_State) -> c_int,
) -> Result<()> {
    let stack_start = ffi::lua_gettop(state) - nargs;

    ffi::lua_pushcfunction(state, error_traceback);
    ffi::lua_pushcfunction(state, f);
    if nargs > 0 {
        ffi::lua_rotate(state, stack_start + 1, 2);
    }

    let ret = ffi::lua_pcall(state, nargs, ffi::LUA_MULTRET, stack_start + 1);
    ffi::lua_remove(state, stack_start + 1);

    if ret == ffi::LUA_OK {
        Ok(())
    } else {
        Err(pop_error(state, ret))
    }
}

// Call a function that calls into the Lua API and may trigger a Lua error (longjmp) in a safe way.
// Wraps the inner function in a call to `lua_pcall`, so the inner function only has access to a
// limited lua stack.  `nargs` and `nresults` are similar to the parameters of `lua_pcall`, but the
// given function return type is not the return value count, instead the inner function return
// values are assumed to match the `nresults` param.  Internally uses 3 extra stack spaces, and does
// not call checkstack.  Provided function must *not* panic, and since it will generally be
// lonjmping, should not contain any values that implement Drop.
pub unsafe fn protect_lua_closure<F, R>(
    state: *mut ffi::lua_State,
    nargs: c_int,
    nresults: c_int,
    f: F,
) -> Result<R>
where
    F: Fn(*mut ffi::lua_State) -> R,
    R: Copy,
{
    union URes<R: Copy> {
        uninit: (),
        init: R,
    }

    struct Params<F, R: Copy> {
        function: F,
        result: URes<R>,
        nresults: c_int,
    }

    unsafe extern "C" fn do_call<F, R>(state: *mut ffi::lua_State) -> c_int
    where
        R: Copy,
        F: Fn(*mut ffi::lua_State) -> R,
    {
        let params = ffi::lua_touserdata(state, -1) as *mut Params<F, R>;
        ffi::lua_pop(state, 1);

        (*params).result.init = ((*params).function)(state);

        if (*params).nresults == ffi::LUA_MULTRET {
            ffi::lua_gettop(state)
        } else {
            (*params).nresults
        }
    }

    let stack_start = ffi::lua_gettop(state) - nargs;

    ffi::lua_pushcfunction(state, error_traceback);
    ffi::lua_pushcfunction(state, do_call::<F, R>);
    if nargs > 0 {
        ffi::lua_rotate(state, stack_start + 1, 2);
    }

    let mut params = Params {
        function: f,
        result: URes { uninit: () },
        nresults,
    };

    ffi::lua_pushlightuserdata(state, &mut params as *mut Params<F, R> as *mut c_void);
    let ret = ffi::lua_pcall(state, nargs + 1, nresults, stack_start + 1);
    ffi::lua_remove(state, stack_start + 1);

    if ret == ffi::LUA_OK {
        // LUA_OK is only returned when the do_call function has completed successfully, so
        // params.result is definitely initialized.
        Ok(params.result.init)
    } else {
        Err(pop_error(state, ret))
    }
}

// Pops an error off of the stack and interprets it as the appropriate lua error.
// Uses 2 stack spaces, does not call lua_checkstack.
pub unsafe fn pop_error(state: *mut ffi::lua_State, err_code: c_int) -> Error {
    rlua_debug_assert!(
        err_code != ffi::LUA_OK && err_code != ffi::LUA_YIELD,
        "pop_error called with non-error return code"
    );

    let err_string = to_string(state, -1).into_owned();
    ffi::lua_pop(state, 1);

    match err_code {
        ffi::LUA_ERRRUN => Error::RuntimeError(err_string),
        ffi::LUA_ERRSYNTAX => {
            Error::SyntaxError {
                // This seems terrible, but as far as I can tell, this is exactly what the
                // stock Lua REPL does.
                incomplete_input: err_string.ends_with("<eof>"),
                message: err_string,
            }
        }
        ffi::LUA_ERRERR => {
            // This error is raised when the error handler raises an error too many times
            // recursively, and continuing to trigger the error handler would cause a stack
            // overflow.  It is not very useful to differentiate between this and "ordinary"
            // runtime errors, so we handle them the same way.
            Error::RuntimeError(err_string)
        }
        ffi::LUA_ERRMEM => Error::MemoryError(err_string),
        _ => rlua_panic!("unrecognized lua error code"),
    }
}

// Takes an error at the top of the stack and if it is some lua type,
// prints the error along with a traceback.
pub unsafe extern "C" fn error_traceback(state: *mut ffi::lua_State) -> c_int {
    // I believe luaL_traceback requires this much free stack to not error.
    const LUA_TRACEBACK_STACK: c_int = 11;

    if ffi::lua_checkstack(state, 2) == 0 {
        // If we don't have enough stack space to even check the error type, do nothing so we don't
        // risk shadowing a rust panic.
    } else if ffi::lua_checkstack(state, LUA_TRACEBACK_STACK) != 0 {
        let s = ffi::luaL_tolstring(state, -1, ptr::null_mut());
        ffi::luaL_traceback(state, state, s, 0);
        ffi::lua_remove(state, -2);
    }
    1
}

// Internally uses 4 stack spaces, does not call checkstack
pub unsafe fn push_string<S: ?Sized + AsRef<[u8]>>(
    state: *mut ffi::lua_State,
    s: &S,
) -> Result<()> {
    protect_lua_closure(state, 0, 1, |state| {
        let s = s.as_ref();
        ffi::lua_pushlstring(state, s.as_ptr() as *const c_char, s.len());
    })
}

// Converts the given lua value to a string in a reasonable format without causing a Lua error or
// panicking.
unsafe fn to_string<'a>(state: *mut ffi::lua_State, index: c_int) -> Cow<'a, str> {
    match ffi::lua_type(state, index) {
        ffi::LUA_TNONE => "<none>".into(),
        ffi::LUA_TNIL => "<nil>".into(),
        ffi::LUA_TBOOLEAN => (ffi::lua_toboolean(state, index) != 1).to_string().into(),
        ffi::LUA_TLIGHTUSERDATA => {
            format!("<lightuserdata {:?}>", ffi::lua_topointer(state, index)).into()
        }
        ffi::LUA_TNUMBER => {
            let mut isint = 0;
            let i = ffi::lua_tointegerx(state, -1, &mut isint);
            if isint == 0 {
                ffi::lua_tonumber(state, index).to_string().into()
            } else {
                i.to_string().into()
            }
        }
        ffi::LUA_TSTRING => {
            let mut size = 0;
            let data = ffi::lua_tolstring(state, index, &mut size);
            String::from_utf8_lossy(slice::from_raw_parts(data as *const u8, size))
        }
        ffi::LUA_TTABLE => format!("<table {:?}>", ffi::lua_topointer(state, index)).into(),
        ffi::LUA_TFUNCTION => format!("<function {:?}>", ffi::lua_topointer(state, index)).into(),
        ffi::LUA_TUSERDATA => format!("<userdata {:?}>", ffi::lua_topointer(state, index)).into(),
        ffi::LUA_TTHREAD => format!("<thread {:?}>", ffi::lua_topointer(state, index)).into(),
        _ => "<unknown>".into(),
    }
}
