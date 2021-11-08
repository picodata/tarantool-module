use std::option::Option;
pub extern crate ffi;

use crate::{
    AsLua,
    AsMutLua,
    Push,
    PushGuard,
    LuaRead,
    LuaFunctionCallError,
    LuaError
};

#[macro_export]
macro_rules! lua_push {
    ($lua:expr, $value:expr,$error_reaction:expr ) => {
        unsafe {
            match ($value).push_to_lua( dereference_and_corrupt_mut_ref( $lua ) ) {
                Ok(mut guard) => {
                    std::mem::swap($lua , & mut guard.lua );
                    guard.forget();
                    true
                }
                Err( ( _lua_push_error, _ ) ) => {
                    $error_reaction;
                    false
                }
            }
        }
    }
}

unsafe fn dereference_and_corrupt_mut_ref< 'a, R>( refr : & mut R) -> R
where
    R : 'a
{
    let mut ret : R = std::mem::MaybeUninit::zeroed().assume_init();
    std::mem::swap( refr, & mut ret );
    ret
}


#[macro_export]
macro_rules! wrap_lua_read_error {
    ($lua:expr, $expected_type:expr ) => {
        LuaFunctionCallError::LuaError(LuaError::WrongType{
            rust_expected: $expected_type,
            lua_actual: unsafe {
                let lua_type = ffi::lua_type($lua.raw_lua.state_ptr(), -1);
                let typename = ffi::lua_typename($lua.raw_lua.state_ptr(), lua_type);
                std::ffi::CStr::from_ptr(typename).to_string_lossy().into_owned()
            }
        })
    }
}

#[macro_export]
macro_rules! lua_get {
    ($lua:expr,
     $number_of_retvalues:expr,
     $success_reaction:expr,
     $error_reaction:expr,
     $expected_type:ty ) => {
        {
            let new_raw_lua = $lua.as_lua();
            let new_lua = PushGuard {
                lua: $lua,
                size: 0,
                raw_lua: new_raw_lua,
            };
            match LuaRead::lua_read_at_position( new_lua, -$number_of_retvalues ) {
                Ok(ret_value) => {
                    $success_reaction;
                    Some(ret_value)
                },
                Err( _read_err ) => {
                    $error_reaction;
                    None
                },
                _ => unreachable!("Logic Error"),
            }
        }
    }
}

#[macro_export]
macro_rules! text_lua_error_wrap {
    ($text:expr, $error_type:ident) => {
        LuaFunctionCallError::LuaError(LuaError::$error_type($text.to_string()) )
    };
}

pub fn common_call<'selftime, 'lua, Ret, Args, L, ErrorReaction> (
    lua_state :& 'selftime mut  L,
    number_of_additional_args : i32,
    top_of_stack : i32,
    mut error_reaction : ErrorReaction,
    args : Args,
) -> Option<Ret>
where
    Ret  : LuaRead<L> + LuaRead< PushGuard<& 'selftime mut L> >,
    Args : Push<L>,
    L : AsMutLua<'lua>,
    ErrorReaction : FnMut( LuaFunctionCallError<LuaError> )->(),
{
    let raw_lua = lua_state.as_lua().state_ptr();
    if ! lua_push!(
             lua_state, args,
             { error_reaction(
                   text_lua_error_wrap!(
                       "Push arguments failed!!!",
                       ExecutionError) )}  ) {
        return None;
    }
    //let _xxx = LuaFunctionCallError::LuaError(LuaError::ExecutionError("Push arguments failed!!!".to_string()) );
    let numargs = unsafe { ffi::lua_gettop( raw_lua ) as i32 } - top_of_stack - 2 + number_of_additional_args ;
    if !unsafe { ffi::lua_isfunction(raw_lua, -numargs - 1) } {
        error_reaction( text_lua_error_wrap!("Stack corrupted", ExecutionError) );
        return None;
    }
    let pcall_error = unsafe {
        ffi::lua_pcall( raw_lua, numargs as i32, ffi::LUA_MULTRET as i32 , 0 as i32 )
    };
    static NAMESBYCODES : &'static [&'static str] = &[
        "", // LUA_OK = 0
        "", // LUA_YIELD = 1
        "Runtime error!!!", // LUA_ERRRUN = 2
        "", // LUA_ERRSYNTAX = 3
        "Memory allocation error!!!" , // LUA_ERRMEM = 4
        "Error handling function failed!!!", //LUA_ERRERR = 5
        "Unknown error: too big error code" // wrong error code
    ];
    if ( pcall_error as i64 ) != 0 {
        error_reaction(
            text_lua_error_wrap!(
                format!(
                    "Call failed: {}",
                    NAMESBYCODES[std::cmp::min(pcall_error as usize, 6usize)]
                ),
                ExecutionError
            )
        );
        return None;
    } else {
        //verify_rettype_matching
    }
    let new_top_of_stack = unsafe { ffi::lua_gettop( raw_lua ) };
    if new_top_of_stack < top_of_stack {
        error_reaction( text_lua_error_wrap!(
            "Wrong return arguments number!!! Lua stack corrupted!!!", ExecutionError) );
        return None;
    }
    let number_of_retvalues : i32 = new_top_of_stack - top_of_stack;
    lua_get!(
        lua_state,
        number_of_retvalues,
        unsafe { ffi::lua_settop( raw_lua, top_of_stack ); }, // on success
        //ffi::lua_settop( raw_lua, top_of_stack ), // on success
        error_reaction( text_lua_error_wrap!("Read return valued failed!!!", ExecutionError) ), // on error
        Ret // expected type
    )
}
