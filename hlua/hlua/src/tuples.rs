use crate::{
    AsMutLua,
    AsLua,
    Push,
    PushOne,
    PushGuard,
    LuaRead,
    Void,
    LuaContext,
    LuaError,
    LuaFunctionCallError,
    reflection::ReflectionCode,
    reflection::type_name_of_val,
    wrap_ret_type_error,
    verify_ret_type,
    text_lua_error_wrap,
    get_lua_type_code,
    refl_get_reflection_type_code_of,
    make_collection,
};


pub struct TupleWrap<E>(pub E);

pub trait VerifyLuaTuple{
   fn check(
       raw_lua : * mut ffi::lua_State,
       stackpos: i32,
       number_elements : i32,
       error : & mut LuaError);
}
//verify_ret_type!( $first, raw_lua, num_of_elements + 1 - check_index, check_index, err );

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
                stackpos: i32,
                number_lua_elements : i32,
                error : & mut LuaError ) ->()
            {
                let mut len_of_tuple = 1;
                if len_of_tuple != number_lua_elements {
                    error.add( &LuaError::ExecutionError("The expected number of returned arguments does not match the actual number of returned arguments!!!".to_string()) );
                    return;
                }
                verify_ret_type!( $ty, raw_lua, stackpos, len_of_tuple, 0, error );
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
                stackpos: i32,
                number_lua_elements : i32,
                error : & mut LuaError ) ->()
            {
                let mut len_of_tuple = 1;
                $(
                    // без этой строчки он ругается. как подавить ошибку дешевле?
                    let str2 = std::any::type_name::<$other>().to_string();
                    len_of_tuple += 1;
                )+
                if len_of_tuple != number_lua_elements {
                    error.add( &LuaError::ExecutionError("The expected number of returned arguments does not match the actual number of returned arguments!!!".to_string()) );
                    return;
                }
                verify_ret_type!( $first, raw_lua, stackpos, len_of_tuple, 1, error );
                let mut offset = len_of_tuple;
                let mut index = 1;
                $(
                    offset -= 1;
                    index += 1;
                    verify_ret_type!( $other, raw_lua, stackpos, offset, index, error );
                    let str2 = std::any::type_name::<$other>().to_string();
                    len_of_tuple += 1;
                )+
            }
        }

        tuple_impl!($($other),+);
    );
}

tuple_impl!(A, B, C, D, E, F, G, H, I, J, K, L, M);

/*
#[allow(unused_assignments)]
#[allow(non_snake_case)]
impl<E> VerifyLuaTuple for TupleWrap<E>
{
    #[inline(always)]
    fn check(
        raw_lua : * mut ffi::lua_State,
        stackpos: i32,
        number_lua_elements : i32,
        error : & mut LuaError ) ->()
    {
        let mut len_of_tuple = 1;
        if len_of_tuple != number_lua_elements {
            error.add( &LuaError::ExecutionError("The expected number of returned arguments does not match the actual number of returned arguments!!!".to_string()) );
            return;
        }
        verify_ret_type!( E, raw_lua, stackpos, len_of_tuple, 0, error );
    }
}

impl<'lua, LU, E> LuaRead<LU> for TupleWrap<E> where LU: AsMutLua<'lua>, TupleWrap<E>: LuaRead<LU> {
    #[inline]
    fn lua_read_at_position(lua: LU, index: i32) -> Result<TupleWrap<E>, LU> {
        <E as LuaRead<LU> >::lua_read_at_position(lua, index)
    }
}*/

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
