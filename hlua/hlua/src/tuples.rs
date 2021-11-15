use crate::{
    AsMutLua,
    AsLua,
    Push,
    PushOne,
    PushGuard,
    LuaRead,
    Void,
    LuaError,
    reflection::ReflectionCode,
    reflection::get_name_of_type,
    wrap_ret_type_error,
    get_lua_type_from_stack,
    verify_ret_type,
    get_lua_type_code,
    refl_get_reflection_type_code_of,
    make_collection,
};


pub trait VerifyLuaTuple{
   fn check(
       raw_lua : * mut ffi::lua_State,
       restore_stack_value: & mut i32,
       stackpos: i32,
       number_elements : i32,
       error : & mut LuaError);
}

impl VerifyLuaTuple for ()
{
    #[inline(always)]
    fn check(
        _raw_lua : * mut ffi::lua_State,
        _restore_stack_value: & mut i32,
        _stackpos: i32,
        number_lua_elements : i32,
        error : & mut LuaError ) ->()
    {
        if number_lua_elements != 0 {
            error.add( &LuaError::ExecutionError(format!(
                "Unexpected number of result values!!! (expected 0, got {}) 3",
                number_lua_elements) ) );
        }
    }
}

macro_rules! tuple_impl {
    ($ty:ident) => (
        impl<'lua, LU, $ty> Push<LU> for ($ty,) where LU: AsMutLua<'lua>, $ty: Push<LU> {
            type Err = <$ty as Push<LU>>::Err;

            #[inline]
            fn push_to_lua(self, lua: LU) -> Result<PushGuard<LU>, (Self::Err, LU)> {
                self.0.push_to_lua(lua)
            }
        }

        impl<'lua, LU, $ty> PushOne<LU> for ($ty,) where LU: AsMutLua<'lua>, $ty: PushOne<LU> {
        }

        impl<'lua, LU, $ty> LuaRead<LU> for ($ty,) where LU: AsMutLua<'lua>, $ty: LuaRead<LU> {
            #[inline]
            fn lua_read_at_position(lua: LU, index: i32) -> Result<($ty,), LU> {
                LuaRead::lua_read_at_position(lua, index).map(|v| (v,))
            }
        }
        #[allow(unused_assignments)]
        #[allow(non_snake_case)]
        impl<$ty> VerifyLuaTuple for ($ty,)
        {
            #[inline(always)]
            fn check(
                raw_lua : * mut ffi::lua_State,
                restore_stack_value: & mut i32,
                stackpos: i32,
                number_lua_elements : i32,
                error : & mut LuaError ) ->()
            {
                let mut len_of_tuple = 1;
                let canbe_fun_or_table = true;
                if len_of_tuple != number_lua_elements {
                    len_of_tuple = if get_name_of_type::<$ty>() != "((),)" {
                        len_of_tuple
                    } else {
                        0
                    };
                    if number_lua_elements != number_lua_elements   {
                        error.add( &LuaError::ExecutionError(format!(
                            "Unexpected number of result values!!! (expected 1, got {}) 1",
                            number_lua_elements) ) );
                        return;
                    }
                }
                verify_ret_type!(
                    $ty,
                    raw_lua,
                    stackpos,
                    len_of_tuple,
                    restore_stack_value,
                    0,
                    canbe_fun_or_table,
                    error );
            }
        }
    );

    ($first:ident, $($other:ident),+) => (
        #[allow(non_snake_case)]
        impl<'lua, LU, FE, OE, $first, $($other),+> Push<LU> for ($first, $($other),+)
            where LU: AsMutLua<'lua>,
                  $first: for<'a> Push<&'a mut LU, Err = FE>,
                  ($($other,)+): for<'a> Push<&'a mut LU, Err = OE>
        {
            type Err = TuplePushError<FE, OE>;

            #[inline]
            fn push_to_lua(self, mut lua: LU) -> Result<PushGuard<LU>, (Self::Err, LU)> {
                match self {
                    ($first, $($other),+) => {
                        let mut total = 0;

                        let first_err = match $first.push_to_lua(&mut lua) {
                            Ok(pushed) => { total += pushed.forget_internal(); None },
                            Err((err, _)) => Some(err),
                        };

                        if let Some(err) = first_err {
                            return Err((TuplePushError::First(err), lua));
                        }

                        let rest = ($($other,)+);
                        let other_err = match rest.push_to_lua(&mut lua) {
                            Ok(pushed) => { total += pushed.forget_internal(); None },
                            Err((err, _)) => Some(err),
                        };

                        if let Some(err) = other_err {
                            return Err((TuplePushError::Other(err), lua));
                        }

                        let raw_lua = lua.as_lua();
                        Ok(PushGuard { lua: lua, size: total, raw_lua: raw_lua })
                    }
                }
            }
        }

        // TODO: what if T or U are also tuples? indices won't match
        #[allow(unused_assignments)]
        #[allow(non_snake_case)]
        impl<'lua, LU, $first: for<'a> LuaRead<&'a mut LU>, $($other: for<'a> LuaRead<&'a mut LU>),+>
            LuaRead<LU> for ($first, $($other),+) where LU: AsLua<'lua>
        {
            #[inline]
            fn lua_read_at_position(mut lua: LU, index: i32) -> Result<($first, $($other),+), LU> {                
                let mut i = index;

                let $first: $first = match LuaRead::lua_read_at_position(&mut lua, i) {
                    Ok(v) => v,
                    Err(_) => return Err(lua)
                };

                i += 1;

                $(
                    let $other: $other = match LuaRead::lua_read_at_position(&mut lua, i) {
                        Ok(v) => v,
                        Err(_) => return Err(lua)
                    };
                    i += 1;
                )+

                Ok(($first, $($other),+))

            }
        }
        #[allow(unused_assignments)]
        #[allow(non_snake_case)]
        impl<$first, $($other),+>
        VerifyLuaTuple for ($first, $($other),+)
        {
            //type ErrorReaction = FnMut( LuaFunctionCallError<LuaError> )-> ();
            #[inline(always)]
            fn check(
                raw_lua : * mut ffi::lua_State,
                restore_stack_value: & mut i32,
                stackpos: i32,
                number_lua_elements : i32,
                error : & mut LuaError ) ->()
            {
                let mut len_of_tuple = 1;
                let mut canbe_fun_or_table = true;
                $(
                    // без этой строчки он ругается. как подавить ошибку дешевле?
                    let _str2 = std::any::type_name::<$other>().to_string();
                    len_of_tuple += 1;
                )+
                if len_of_tuple != number_lua_elements {
                    error.add( &LuaError::ExecutionError(format!(
                        "Unexpected number of result values!!! (expected {}, got {}) 2",
                        len_of_tuple,
                        number_lua_elements) ) );
                    return;
                }
                canbe_fun_or_table = verify_ret_type!(
                    $first,
                    raw_lua,
                    stackpos,
                    len_of_tuple,
                    restore_stack_value,
                    1,
                    canbe_fun_or_table,
                    error );
                let mut offset = len_of_tuple;
                let mut index = 1;
                $(
                    offset -= 1;
                    index += 1;
                    canbe_fun_or_table = verify_ret_type!(
                        $other,
                        raw_lua,
                        stackpos,
                        offset,
                        restore_stack_value,
                        index,
                        canbe_fun_or_table,
                        error );
                    let _str2 = std::any::type_name::<$other>().to_string();
                    len_of_tuple += 1;
                )+
            }
        }

        tuple_impl!($($other),+);
    );
}

tuple_impl!(A, B, C, D, E, F, G, H, I, J, K, L, M);


/// Error that can happen when pushing multiple values at once.
// TODO: implement Error on that thing
#[derive(Debug, Copy, Clone)]
pub enum TuplePushError<C, O> {
    First(C),
    Other(O),
}

impl From<TuplePushError<Void, Void>> for Void {
    #[inline]
    fn from(_: TuplePushError<Void, Void>) -> Void {
        unreachable!()
    }
}
