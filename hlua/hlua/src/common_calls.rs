use std::option::Option;
pub extern crate ffi;

use crate::{
    AsLua,
    AsMutLua,
    Push,
    PushGuard,
    LuaRead,
    LuaFunctionCallError,
    LuaError,
    tuples::VerifyLuaTuple,
    reflection::get_name_of_type
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

#[inline(always)]
unsafe fn dereference_and_corrupt_mut_ref< 'a, R>( refr : & mut R) -> R
where
    R : 'a
{
    let mut ret : R = std::mem::MaybeUninit::zeroed().assume_init();
    std::mem::swap( refr, & mut ret );
    std::mem::forget( refr );
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

#[macro_export]
macro_rules! verify_ret_type {
    ($expected_type:ty, $raw_lua:expr, $stackposition:expr, $offset:expr, $ind:expr, $out_error:expr ) => {
        let lua_type_code = unsafe {ffi::lua_type( $raw_lua, $stackposition-($offset) ) };
        let rustexpected_code = get_lua_type_code!($expected_type) as i32;
        println!("exp {} , lua {}", lua_type_code, rustexpected_code );
        if ( rustexpected_code != (ffi::LUA_TNONE as i32) &&
             rustexpected_code != lua_type_code ) {
            // wrong error type
            $out_error.add( &wrap_ret_type_error!( $expected_type, lua_type_code, $stackposition, $offset, $raw_lua, $ind ) );
        }
    };
}

#[inline(always)]
pub fn common_call<'selftime, 'lua, Ret, Args, L, ErrorReaction> (
    lua_state :& 'selftime mut  L,
    number_of_additional_args : i32, //  // top of stack BEFORE pushing function
    stack_restoring_value : i32,
    mut error_reaction : ErrorReaction,
    args : Args,
) -> Option<Ret>
where
    Ret  : LuaRead< PushGuard<& 'selftime mut L> > + VerifyLuaTuple,
    Args : Push<L>,
    L : AsMutLua<'lua>,
    ErrorReaction : FnMut( LuaFunctionCallError<LuaError> )->(),
{
    println!("COMMON_CALL {}",get_name_of_type::<Ret>());
    let raw_lua = lua_state.as_lua().state_ptr();
    let stack_before_args = unsafe { ffi::lua_gettop( raw_lua ) as i32 };
    let function_stackpos = stack_before_args - number_of_additional_args - 1;
    println!("COMMON_CALL topstack1={}",stack_before_args);
    if !unsafe { ffi::lua_isfunction(raw_lua, function_stackpos) } {
        error_reaction( text_lua_error_wrap!("Stack corrupted !!!", ExecutionError) );
        unsafe {ffi::lua_settop( raw_lua, stack_restoring_value ); };
        return None;
    }
    if ! lua_push!(
             lua_state,
             args,
             { error_reaction(
                   text_lua_error_wrap!(
                       "Push arguments failed!!!",
                       ExecutionError) )}  ) {
        unsafe {ffi::lua_settop( raw_lua, stack_restoring_value ); };
        return None;
    }
    //let _xxx = LuaFunctionCallError::LuaError(LuaError::ExecutionError("Push arguments failed!!!".to_string()) );
    let stack_after_args = unsafe { ffi::lua_gettop( raw_lua ) as i32 };
    println!("COMMON_CALL topstack2={}",stack_after_args);
    let numargs = stack_after_args - stack_before_args + number_of_additional_args ;
    println!("COMMON_CALL numargs={}",numargs);
    if !unsafe { ffi::lua_isfunction(raw_lua, - numargs - 1 ) } {
        error_reaction( text_lua_error_wrap!("Stack corrupted", ExecutionError) );
        unsafe {ffi::lua_settop( raw_lua, stack_restoring_value ); };
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
    
    let new_top_of_stack = unsafe { ffi::lua_gettop( raw_lua ) };
    /*if new_top_of_stack < top_of_stack {
        error_reaction( text_lua_error_wrap!(
            "Wrong return arguments number!!! Lua stack corrupted!!!", ExecutionError) );
        return None;
    }*/

    // Attention! pcall pops args AND function pointer!
    let number_of_retvalues : i32 = new_top_of_stack - function_stackpos;
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
        unsafe {ffi::lua_settop( raw_lua, stack_restoring_value ); };
        return None;
    } else {
        //verify_rettype_matching
        let mut err = LuaError::NoError;
        println!("COMMON_CALL numretvalues={}",number_of_retvalues);
        <Ret as VerifyLuaTuple>::check( raw_lua, 0, number_of_retvalues, & mut err );
        if !err.is_no_error() {
            error_reaction(  LuaFunctionCallError::LuaError(err) );
            unsafe {ffi::lua_settop( raw_lua, stack_restoring_value ); };
            return None;
        }
    }
    lua_get!(
        lua_state,
        number_of_retvalues,
        unsafe { ffi::lua_settop( raw_lua, stack_restoring_value ); }, // on success
        //ffi::lua_settop( raw_lua, top_of_stack ), // on success
        error_reaction( text_lua_error_wrap!("Read return valued failed!!!", ExecutionError) ), // on error
        Ret // expected type
    )
}


#[macro_export]
macro_rules! wrap_ret_type_error {
    ($expected_type:ty, $lua_code:expr, $stackposition:expr,$offset:expr, $raw_lua:expr, $ind:expr) => {
        LuaError::WrongType{
            rust_expected: std::any::type_name::<$expected_type>().to_string(),
            lua_actual: unsafe {
                let lua_type = ffi::lua_type( $raw_lua, $stackposition-($offset) );
                let typename = ffi::lua_typename($raw_lua, lua_type);
                std::ffi::CStr::from_ptr(typename).to_string_lossy().into_owned()
            },
            index : $ind,
        }
    };
}

#[macro_export]
macro_rules! get_lua_type_code {
    ($luatype:ty) => {
        {
            static TYPEID : &'static [i32] = &[
                ffi::LUA_TNONE   as i32, //Nchar       = 0,
                ffi::LUA_TNUMBER as i32, //Nu8         = 1,
                ffi::LUA_TNUMBER as i32, //Ni8         = 2,
                ffi::LUA_TNUMBER as i32, //Nu16        = 3,
                ffi::LUA_TNUMBER as i32, //Ni16        = 4,
                ffi::LUA_TNUMBER as i32, //Nu32        = 5,
                ffi::LUA_TNUMBER as i32, //Ni32        = 6,
                ffi::LUA_TNONE   as i32, //Nu64        = 7,
                ffi::LUA_TNONE   as i32, //Ni64        = 8,
                ffi::LUA_TNONE   as i32, //Nu128       = 9,
                ffi::LUA_TNONE   as i32, //Ni128       = 10,
                ffi::LUA_TNUMBER as i32, //Nf32        = 11,
                ffi::LUA_TNUMBER as i32, //Nf64        = 12,
                ffi::LUA_TNUMBER as i32, //Nisize      = 13,
                ffi::LUA_TNUMBER as i32, //Nusize      = 14,
                ffi::LUA_TBOOLEAN as i32, //Nbool       = 15,
                ffi::LUA_TSTRING as i32, //NString     = 16,
                ffi::LUA_TNONE as i32, // any other type
            ];
            static MAX_TYPE_CODE : i32 =  ReflectionCode::NString as i32 + 1;
            let luatype_code : ReflectionCode = refl_get_reflection_type_code_of!($luatype);
            TYPEID[ std::cmp::min(luatype_code as i32,MAX_TYPE_CODE) as usize ]
        }
    }
}
