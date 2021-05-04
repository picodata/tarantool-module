use std::iter::FromIterator;
use std::ops::{Deref, DerefMut};
use std::result::Result as StdResult;

use crate::lua::context::Context;
use crate::lua::error::Result;
use crate::lua::value::{FromLua, FromLuaMulti, MultiValue, Nil, ToLua, ToLuaMulti};

/// Result is convertible to `MultiValue` following the common Lua idiom of returning the result
/// on success, or in the case of an error, returning `nil` and an error message.
impl<T: ToLua, E: ToLua> ToLuaMulti for StdResult<T, E> {
    fn to_lua_multi(self, ctx: &Context) -> Result<MultiValue> {
        let mut result = MultiValue::new();

        match self {
            Ok(v) => result.push_front(v.to_lua(ctx)?),
            Err(e) => {
                result.push_front(e.to_lua(ctx)?);
                result.push_front(Nil);
            }
        }

        Ok(result)
    }
}

impl<T: ToLua> ToLuaMulti for T {
    fn to_lua_multi(self, ctx: &Context) -> Result<MultiValue> {
        let mut v = MultiValue::new();
        v.push_front(self.to_lua(ctx)?);
        Ok(v)
    }
}

impl<T: FromLua> FromLuaMulti for T {
    fn from_lua_multi(mut values: MultiValue, ctx: &Context) -> Result<Self> {
        Ok(T::from_lua(values.pop_front().unwrap_or(Nil), ctx)?)
    }
}

impl ToLuaMulti for MultiValue {
    fn to_lua_multi(self, _: &Context) -> Result<MultiValue> {
        Ok(self)
    }
}

impl FromLuaMulti for MultiValue {
    fn from_lua_multi(values: MultiValue, _: &Context) -> Result<Self> {
        Ok(values)
    }
}

/// Wraps a variable number of `T`s.
///
/// Can be used to work with variadic functions more easily. Using this type as the last argument of
/// a Rust callback will accept any number of arguments from Lua and convert them to the type `T`
/// using [`FromLua`]. `Variadic<T>` can also be returned from a callback, returning a variable
/// number of values to Lua.
///
/// The [`MultiValue`] type is equivalent to `Variadic<Value>`.
pub struct Variadic<T>(Vec<T>);

impl<T> Variadic<T> {
    /// Creates an empty `Variadic` wrapper containing no values.
    pub fn new() -> Variadic<T> {
        Variadic(Vec::new())
    }
}

impl<T> Default for Variadic<T> {
    fn default() -> Variadic<T> {
        Variadic::new()
    }
}

impl<T> FromIterator<T> for Variadic<T> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        Variadic(Vec::from_iter(iter))
    }
}

impl<T> IntoIterator for Variadic<T> {
    type Item = T;
    type IntoIter = <Vec<T> as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

impl<T> Deref for Variadic<T> {
    type Target = Vec<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for Variadic<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T: ToLua> ToLuaMulti for Variadic<T> {
    fn to_lua_multi(self, ctx: &Context) -> Result<MultiValue> {
        self.0.into_iter().map(|e| e.to_lua(ctx)).collect()
    }
}

impl<T: FromLua> FromLuaMulti for Variadic<T> {
    fn from_lua_multi(values: MultiValue, ctx: &Context) -> Result<Self> {
        values
            .into_iter()
            .map(|e| T::from_lua(e, ctx))
            .collect::<Result<Vec<T>>>()
            .map(Variadic)
    }
}

macro_rules! impl_tuple {
    () => (
        impl ToLuaMulti for () {
            fn to_lua_multi(self, _: &Context) -> Result<MultiValue> {
                Ok(MultiValue::new())
            }
        }

        impl FromLuaMulti for () {
            fn from_lua_multi(_: MultiValue, _: &Context) -> Result<Self> {
                Ok(())
            }
        }
    );

    ($last:ident $($name:ident)*) => (
        impl<$($name,)* $last> ToLuaMulti for ($($name,)* $last,)
            where $($name: ToLua,)*
                  $last: ToLuaMulti
        {
            #[allow(unused_mut)]
            #[allow(non_snake_case)]
            fn to_lua_multi(self, ctx: &Context) -> Result<MultiValue> {
                let ($($name,)* $last,) = self;

                let mut results = $last.to_lua_multi(ctx)?;
                push_reverse!(results, $($name.to_lua(ctx)?,)*);
                Ok(results)
            }
        }

        impl<$($name,)* $last> FromLuaMulti for ($($name,)* $last,)
            where $($name: FromLua,)*
                  $last: FromLuaMulti
        {
            #[allow(unused_mut)]
            #[allow(non_snake_case)]
            fn from_lua_multi(mut values: MultiValue, ctx: &Context) -> Result<Self> {
                $(let $name = values.pop_front().unwrap_or(Nil);)*
                let $last = FromLuaMulti::from_lua_multi(values, ctx)?;
                Ok(($(FromLua::from_lua($name, ctx)?,)* $last,))
            }
        }
    );
}

macro_rules! push_reverse {
    ($multi_value:expr, $first:expr, $($rest:expr,)*) => (
        push_reverse!($multi_value, $($rest,)*);
        $multi_value.push_front($first);
    );

    ($multi_value:expr, $first:expr) => (
        $multi_value.push_front($first);
    );

    ($multi_value:expr,) => ();
}

impl_tuple!();
impl_tuple!(A);
impl_tuple!(A B);
impl_tuple!(A B C);
impl_tuple!(A B C D);
impl_tuple!(A B C D E);
impl_tuple!(A B C D E F);
impl_tuple!(A B C D E F G);
impl_tuple!(A B C D E F G H);
impl_tuple!(A B C D E F G H I);
impl_tuple!(A B C D E F G H I J);
impl_tuple!(A B C D E F G H I J K);
impl_tuple!(A B C D E F G H I J K L);
impl_tuple!(A B C D E F G H I J K L M);
impl_tuple!(A B C D E F G H I J K L M N);
impl_tuple!(A B C D E F G H I J K L M N O);
impl_tuple!(A B C D E F G H I J K L M N O P);
