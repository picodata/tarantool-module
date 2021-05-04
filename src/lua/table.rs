use std::marker::PhantomData;
use std::os::raw::c_int;

use crate::lua::ffi;

use crate::lua::error::Result;
use crate::lua::types::{Integer, LuaRef};
use crate::lua::util::{assert_stack, protect_lua, protect_lua_closure, StackGuard};
use crate::lua::value::{FromLua, Nil, ToLua, Value};

/// Handle to an internal Lua table.
#[derive(Clone, Debug)]
pub struct Table(pub(crate) LuaRef);

impl Table {
    /// Sets a key-value pair in the table.
    ///
    /// If the value is `nil`, this will effectively remove the pair.
    ///
    /// This might invoke the `__newindex` metamethod.
    pub fn set<K: ToLua, V: ToLua>(&self, key: K, value: V) -> Result<()> {
        let ctx = self.0.ctx;
        let key = key.to_lua(&ctx)?;
        let value = value.to_lua(&ctx)?;
        unsafe {
            let _sg = StackGuard::new(ctx.state);
            assert_stack(ctx.state, 6);

            ctx.push_ref(&self.0);
            ctx.push_value(key)?;
            ctx.push_value(value)?;

            unsafe extern "C" fn set_table(state: *mut ffi::lua_State) -> c_int {
                ffi::lua_settable(state, -3);
                1
            }
            protect_lua(ctx.state, 3, set_table)
        }
    }

    /// Gets the value associated to `key` from the table.
    ///
    /// If no value is associated to `key`, returns the `nil` value.
    ///
    /// This might invoke the `__index` metamethod.
    pub fn get<K: ToLua, V: FromLua>(&self, key: K) -> Result<V> {
        let ctx = self.0.ctx;
        let key = key.to_lua(&ctx)?;
        let value = unsafe {
            let _sg = StackGuard::new(ctx.state);
            assert_stack(ctx.state, 5);

            ctx.push_ref(&self.0);
            ctx.push_value(key)?;

            unsafe extern "C" fn get_table(state: *mut ffi::lua_State) -> c_int {
                ffi::lua_gettable(state, -2);
                1
            }
            protect_lua(ctx.state, 2, get_table)?;
            ctx.pop_value()
        };
        V::from_lua(value, &ctx)
    }

    /// Checks whether the table contains a non-nil value for `key`.
    pub fn contains_key<K: ToLua>(&self, key: K) -> Result<bool> {
        let ctx = self.0.ctx;
        let key = key.to_lua(&ctx)?;

        unsafe {
            let _sg = StackGuard::new(ctx.state);
            assert_stack(ctx.state, 5);

            ctx.push_ref(&self.0);
            ctx.push_value(key)?;

            unsafe extern "C" fn get_table(state: *mut ffi::lua_State) -> c_int {
                ffi::lua_gettable(state, -2);
                1
            }
            protect_lua(ctx.state, 2, get_table)?;

            let has = ffi::lua_isnil(ctx.state, -1) == 0;
            Ok(has)
        }
    }

    /// Returns the result of the Lua `#` operator.
    ///
    /// This might invoke the `__len` metamethod.
    pub fn len(&self) -> Result<Integer> {
        let ctx = self.0.ctx;
        unsafe {
            let _sg = StackGuard::new(ctx.state);
            assert_stack(ctx.state, 4);
            ctx.push_ref(&self.0);
            protect_lua_closure(ctx.state, 1, 0, |state| ffi::luaL_len(state, -1))
        }
    }

    /// Consume this table and return an iterator over the pairs of the table.
    ///
    /// This works like the Lua `pairs` function, but does not invoke the `__pairs` metamethod.
    ///
    /// The pairs are wrapped in a [`Result`], since they are lazily converted to `K` and `V` types.
    ///
    /// # Note
    ///
    /// While this method consumes the `Table` object, it can not prevent code from mutating the
    /// table while the iteration is in progress. Refer to the [Lua manual] for information about
    /// the consequences of such mutation.
    ///
    /// # Examples
    ///
    /// Iterate over all globals:
    ///
    /// ```
    /// # use rlua::{Lua, Result, Value};
    /// # fn main() -> Result<()> {
    /// # Lua::new().context(|lua_context| {
    /// let globals = lua_context.globals();
    ///
    /// for pair in globals.pairs::<Value, Value>() {
    ///     let (key, value) = pair?;
    /// #   let _ = (key, value);   // used
    ///     // ...
    /// }
    /// # Ok(())
    /// # })
    /// # }
    /// ```
    ///
    /// [`Result`]: type.Result.html
    /// [Lua manual]: http://www.ctx.org/manual/5.4/manual.html#pdf-next
    pub fn pairs<K: FromLua, V: FromLua>(self) -> TablePairs<K, V> {
        TablePairs {
            table: self.0,
            next_key: Some(Nil),
            _phantom: PhantomData,
        }
    }

    /// Consume this table and return an iterator over all values in the sequence part of the table.
    ///
    /// The iterator will yield all values `t[1]`, `t[2]`, and so on, until a `nil` value is
    /// encountered. This mirrors the behaviour of Lua's `ipairs` function and will invoke the
    /// `__index` metamethod according to the usual rules. However, the deprecated `__ipairs`
    /// metatable will not be called.
    ///
    /// Just like [`pairs`], the values are wrapped in a [`Result`].
    ///
    /// # Note
    ///
    /// While this method consumes the `Table` object, it can not prevent code from mutating the
    /// table while the iteration is in progress. Refer to the [Lua manual] for information about
    /// the consequences of such mutation.
    ///
    /// # Examples
    ///
    /// ```
    /// # use rlua::{Lua, Result, Table};
    /// # fn main() -> Result<()> {
    /// # Lua::new().context(|lua_context| {
    /// let my_table: Table = lua_context.load(r#"
    ///     {
    ///         [1] = 4,
    ///         [2] = 5,
    ///         [4] = 7,
    ///         key = 2
    ///     }
    /// "#).eval()?;
    ///
    /// let expected = [4, 5];
    /// for (&expected, got) in expected.iter().zip(my_table.sequence_values::<u32>()) {
    ///     assert_eq!(expected, got?);
    /// }
    /// # Ok(())
    /// # })
    /// # }
    /// ```
    ///
    /// [`pairs`]: #method.pairs
    /// [`Result`]: type.Result.html
    /// [Lua manual]: http://www.ctx.org/manual/5.4/manual.html#pdf-next
    pub fn sequence_values<V: FromLua>(self) -> TableSequence<V> {
        TableSequence {
            table: self.0,
            index: Some(1),
            _phantom: PhantomData,
        }
    }
}

/// An iterator over the pairs of a Lua table.
///
/// This struct is created by the [`Table::pairs`] method.
///
/// [`Table::pairs`]: struct.Table.html#method.pairs
pub struct TablePairs<K, V> {
    table: LuaRef,
    next_key: Option<Value>,
    _phantom: PhantomData<(K, V)>,
}

impl<K, V> Iterator for TablePairs<K, V>
where
    K: FromLua,
    V: FromLua,
{
    type Item = Result<(K, V)>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(next_key) = self.next_key.take() {
            let ctx = self.table.ctx;

            let res = (|| {
                let res = unsafe {
                    let _sg = StackGuard::new(ctx.state);
                    assert_stack(ctx.state, 6);

                    ctx.push_ref(&self.table);
                    ctx.push_value(next_key)?;

                    if protect_lua_closure(ctx.state, 2, ffi::LUA_MULTRET, |state| {
                        ffi::lua_next(state, -2) != 0
                    })? {
                        ffi::lua_pushvalue(ctx.state, -2);
                        let key = ctx.pop_value();
                        let value = ctx.pop_value();
                        self.next_key = Some(ctx.pop_value());

                        Some((key, value))
                    } else {
                        None
                    }
                };

                Ok(if let Some((key, value)) = res {
                    Some((K::from_lua(key, &ctx)?, V::from_lua(value, &ctx)?))
                } else {
                    None
                })
            })();

            match res {
                Ok(Some((key, value))) => Some(Ok((key, value))),
                Ok(None) => None,
                Err(e) => Some(Err(e)),
            }
        } else {
            None
        }
    }
}

/// An iterator over the sequence part of a Lua table.
///
/// This struct is created by the [`Table::sequence_values`] method.
///
/// [`Table::sequence_values`]: struct.Table.html#method.sequence_values
pub struct TableSequence<V> {
    table: LuaRef,
    index: Option<Integer>,
    _phantom: PhantomData<V>,
}

impl<V> Iterator for TableSequence<V>
where
    V: FromLua,
{
    type Item = Result<V>;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(index) = self.index.take() {
            let ctx = self.table.ctx;

            let res = unsafe {
                let _sg = StackGuard::new(ctx.state);
                assert_stack(ctx.state, 5);

                ctx.push_ref(&self.table);
                match protect_lua_closure(ctx.state, 1, 1, |state| ffi::lua_geti(state, -1, index))
                {
                    Ok(ffi::LUA_TNIL) => None,
                    Ok(_) => {
                        let value = ctx.pop_value();
                        self.index = Some(index + 1);
                        Some(Ok(value))
                    }
                    Err(err) => Some(Err(err)),
                }
            };

            match res {
                Some(Ok(r)) => Some(V::from_lua(r, &ctx)),
                Some(Err(err)) => Some(Err(err)),
                None => None,
            }
        } else {
            None
        }
    }
}
