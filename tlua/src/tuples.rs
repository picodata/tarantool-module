use std::num::NonZeroI32;
use std::fmt::Debug;

use crate::{
    AsLua,
    Push,
    PushInto,
    PushOne,
    PushOneInto,
    PushGuard,
    LuaRead,
    LuaState,
    Void,
};

macro_rules! tuple_impl {
    ($ty:ident) => {
        impl<LU, $ty> Push<LU> for ($ty,)
        where
            LU: AsLua,
            $ty: Push<LU>,
        {
            type Err = TuplePushError<
                <$ty as Push<LU>>::Err,
                Void,
            >;

            #[inline]
            fn push_to_lua(&self, lua: LU) -> Result<PushGuard<LU>, (Self::Err, LU)> {
                self.0.push_to_lua(lua)
                    .map_err(|(e, l)| (TuplePushError::First(e), l))
            }
        }

        impl<LU, $ty> PushOne<LU> for ($ty,)
        where
            LU: AsLua,
            $ty: PushOne<LU>,
        {
        }

        impl<LU, $ty> PushInto<LU> for ($ty,)
        where
            LU: AsLua,
            $ty: PushInto<LU>,
        {
            type Err = TuplePushError<
                <$ty as PushInto<LU>>::Err,
                Void,
            >;

            #[inline]
            fn push_into_lua(self, lua: LU) -> Result<PushGuard<LU>, (Self::Err, LU)> {
                self.0.push_into_lua(lua)
                    .map_err(|(e, l)| (TuplePushError::First(e), l))
            }
        }

        impl<LU, $ty> PushOneInto<LU> for ($ty,)
        where
            LU: AsLua,
            $ty: PushOneInto<LU>,
        {
        }

        impl<LU, $ty> LuaRead<LU> for ($ty,)
        where
            LU: AsLua,
            $ty: LuaRead<LU>,
        {
            #[inline(always)]
            fn n_values_expected() -> i32 {
                $ty::n_values_expected()
            }

            #[inline(always)]
            fn lua_read_at_position(lua: LU, index: NonZeroI32) -> Result<($ty,), LU> {
                Self::read_at_and_count(lua, index).map(|(v, _)| v)
            }

            #[inline(always)]
            fn read_at_and_count(lua: LU, index: NonZeroI32) -> Result<(($ty,), i32), LU> {
                LuaRead::read_at_and_count(lua, index).map(|(v, count)| ((v,), count))
            }
        }
    };

    ($first:ident, $($other:ident),+) => {
        #[allow(non_snake_case)]
        impl<LU, $first, $($other),+> Push<LU> for ($first, $($other),+)
        where
            LU: AsLua,
            $first: Push<LuaState>,
            $( $other: Push<LuaState>, )+
        {
            type Err = TuplePushError<
                <$first as Push<LuaState>>::Err,
                <($($other,)+) as Push<LuaState>>::Err,
            >;

            #[inline]
            fn push_to_lua(&self, lua: LU) -> Result<PushGuard<LU>, (Self::Err, LU)> {
                use TuplePushError::{First, Other};
                match self {
                    ($first, $($other),+) => {
                        let error = |e| e;
                        let pushed = match lua.as_lua().try_push($first) {
                            Ok(pushed) => pushed,
                            Err((err, _)) => return Err((error(First(err)), lua)),
                        };
                        let total = move || pushed.forget_internal();

                        $(
                            let error = |e| error(Other(e));
                            let pushed = match lua.as_lua().try_push($other) {
                                Ok(pushed) => pushed,
                                // TODO(gmoshkin): return an error capturing the
                                // previously created pushguards so that the
                                // caller can handle partially pushed tuples
                                Err((err, _)) => return Err((error(First(err)), lua)),
                            };
                            let total = move || pushed.forget_internal() + total();
                        )+

                        unsafe {
                            Ok(PushGuard::new(lua, total()))
                        }
                    }
                }
            }
        }

        #[allow(non_snake_case)]
        impl<LU, $first, $($other),+> PushInto<LU> for ($first, $($other),+)
        where
            LU: AsLua,
            $first: PushInto<LuaState>,
            $( $other: PushInto<LuaState>, )+
        {
            type Err = TuplePushError<
                <$first as PushInto<LuaState>>::Err,
                <($($other,)+) as PushInto<LuaState>>::Err,
            >;

            #[inline]
            fn push_into_lua(self, lua: LU) -> Result<PushGuard<LU>, (Self::Err, LU)> {
                use TuplePushError::{First, Other};
                match self {
                    ($first, $($other),+) => {
                        let first_pushed = match lua.as_lua().try_push($first) {
                            Ok(pushed) => pushed,
                            Err((err, _)) => return Err((First(err), lua)),
                        };

                        let other_pushed = match lua.as_lua().try_push(($($other,)+)) {
                            Ok(pushed) => pushed,
                            // TODO(gmoshkin): return an error capturing the
                            // first_pushed pushguard so that the caller can
                            // handle partially pushed tuples
                            Err((err, _)) => return Err((Other(err), lua)),
                        };

                        let total = first_pushed.forget_internal()
                            + other_pushed.forget_internal();

                        unsafe {
                            Ok(PushGuard::new(lua, total))
                        }
                    }
                }
            }
        }

        // TODO: what if T or U are also tuples? indices won't match
        #[allow(unused_assignments)]
        #[allow(non_snake_case)]
        impl<LU, $first, $($other),+> LuaRead<LU> for ($first, $($other),+)
        where
            LU: AsLua,
            $first: for<'a> LuaRead<&'a LU>,
            $($other: for<'a> LuaRead<&'a LU>),+
        {
            #[inline(always)]
            fn n_values_expected() -> i32 {
                $first::n_values_expected() $( + $other::n_values_expected() )+
            }

            #[inline(always)]
            fn lua_read_at_position(lua: LU, index: NonZeroI32) -> Result<($first, $($other),+), LU> {
                Self::read_at_and_count(lua, index).map(|(v, _)| v)
            }

            #[inline]
            fn read_at_and_count(lua: LU, index: NonZeroI32) -> Result<(($first, $($other),+), i32), LU> {
                let top = unsafe { $crate::ffi::lua_gettop(lua.as_lua()) };

                let ($first, count) = match $first::read_at_and_count(&lua, index) {
                    Ok(v) => v,
                    Err(_) => return Err(lua)
                };

                let mut i: i32 = index.into();
                // i += 1;
                let candidate = i + count;
                i = match i {
                    i32::MIN ..= -1 => candidate.min(0),
                    0 => unreachable!("cannot be zero"),
                    1 ..= i32::MAX => candidate.min(top),
                };
                let mut total_count = count;

                $(
                    let ($other, count) = match $other::read_at_mz_and_count(&lua, i) {
                        Ok(v) => v,
                        Err(_) => return Err(lua)
                    };
                    // The 0 index is the special case that should not be walked
                    // over. Either we return Err on it or we handle the
                    // situation correctly (e.g. Option<T>, (), ...)
                    // i = if i == 0 { 0  } else { i + 1  };
                    let candidate = i + count;
                    i = match i {
                        i32::MIN ..= -1 => candidate.min(0),
                        0 => 0,
                        1 ..= i32::MAX => candidate.min(top),
                    };
                    total_count += count;
                )+

                Ok((($first, $($other),+), total_count))

            }
        }

        tuple_impl!{ $($other),+ }
    };
}

tuple_impl!(A, B, C, D, E, F, G, H, I, J, K, L, M);

/// Error that can happen when pushing multiple values at once.
// TODO: implement Error on that thing
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum TuplePushError<C, O> {
    First(C),
    Other(O),
}

impl<F, O> TuplePushError<F, O> {
    pub fn first(self) -> F
    where
        O: Into<Void>,
    {
        match self {
            Self::First(f) => f,
            Self::Other(_) => unreachable!("no way to construct an instance of Void"),
        }
    }

    pub fn other(self) -> O
    where
        F: Into<Void>,
    {
        match self {
            Self::First(_) => unreachable!("no way to construct an instance of Void"),
            Self::Other(o) => o,
        }
    }
}

macro_rules! impl_tuple_push_error {
    [@t] => { Void };
    [@t $h:tt $($t:tt)*] => { TuplePushError<$h, impl_tuple_push_error![@t $($t)*]> };
    () => {};
    ($h:tt $($t:tt)*) => {
        impl<$h, $($t,)*> From<impl_tuple_push_error![@t $h $($t)*]> for Void
        where
            $h: Into<Void>,
            $( $t: Into<Void>, )*
        {
            #[inline]
            fn from(_: impl_tuple_push_error![@t $h $($t)*]) -> Void {
                unreachable!("There's no way to create an instance of Void")
            }
        }
        impl_tuple_push_error!{ $($t)* }
    };
}

impl_tuple_push_error!{A B C D E F G H I J K L M}
