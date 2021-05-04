use std::collections::{BTreeMap, HashMap};
use std::ffi::{CStr, CString};
use std::hash::{BuildHasher, Hash};
use std::string::String as StdString;

use bstr::{BStr, BString};
use num_traits::cast;

use crate::lua::context::Context;
use crate::lua::error::{Error, Result};
use crate::lua::function::Function;
use crate::lua::string::String;
use crate::lua::table::Table;
use crate::lua::types::Number;
use crate::lua::value::{FromLua, Nil, ToLua, Value};

impl ToLua for Value {
    fn to_lua(self, _: &Context) -> Result<Value> {
        Ok(self)
    }
}

impl FromLua for Value {
    fn from_lua(lua_value: Value, _: &Context) -> Result<Self> {
        Ok(lua_value)
    }
}

impl ToLua for String {
    fn to_lua(self, _: &Context) -> Result<Value> {
        Ok(Value::String(self))
    }
}

impl FromLua for String {
    fn from_lua(value: Value, ctx: &Context) -> Result<String> {
        let ty = value.type_name();
        ctx.coerce_string(value)?
            .ok_or_else(|| Error::FromLuaConversionError {
                from: ty,
                to: "String",
                message: Some("expected string or number".to_string()),
            })
    }
}

impl ToLua for Table {
    fn to_lua(self, _: &Context) -> Result<Value> {
        Ok(Value::Table(self))
    }
}

impl FromLua for Table {
    fn from_lua(value: Value, _: &Context) -> Result<Table> {
        match value {
            Value::Table(table) => Ok(table),
            _ => Err(Error::FromLuaConversionError {
                from: value.type_name(),
                to: "table",
                message: None,
            }),
        }
    }
}

impl ToLua for Function {
    fn to_lua(self, _: &Context) -> Result<Value> {
        Ok(Value::Function(self))
    }
}

impl FromLua for Function {
    fn from_lua(value: Value, _: &Context) -> Result<Function> {
        match value {
            Value::Function(table) => Ok(table),
            _ => Err(Error::FromLuaConversionError {
                from: value.type_name(),
                to: "function",
                message: None,
            }),
        }
    }
}

impl ToLua for bool {
    fn to_lua(self, _: &Context) -> Result<Value> {
        Ok(Value::Boolean(self))
    }
}

impl FromLua for bool {
    fn from_lua(v: Value, _: &Context) -> Result<Self> {
        match v {
            Value::Nil => Ok(false),
            Value::Boolean(b) => Ok(b),
            _ => Ok(true),
        }
    }
}

impl ToLua for StdString {
    fn to_lua(self, ctx: &Context) -> Result<Value> {
        Ok(Value::String(ctx.create_string(&self)?))
    }
}

impl FromLua for StdString {
    fn from_lua(value: Value, ctx: &Context) -> Result<Self> {
        let ty = value.type_name();
        Ok(ctx
            .coerce_string(value)?
            .ok_or_else(|| Error::FromLuaConversionError {
                from: ty,
                to: "String",
                message: Some("expected string or number".to_string()),
            })?
            .to_str()?
            .to_owned())
    }
}

impl<'a> ToLua for &'a str {
    fn to_lua(self, ctx: &Context) -> Result<Value> {
        Ok(Value::String(ctx.create_string(self)?))
    }
}

impl ToLua for CString {
    fn to_lua(self, ctx: &Context) -> Result<Value> {
        Ok(Value::String(ctx.create_string(self.as_bytes())?))
    }
}

impl FromLua for CString {
    fn from_lua(value: Value, ctx: &Context) -> Result<Self> {
        let ty = value.type_name();
        let string = ctx
            .coerce_string(value)?
            .ok_or_else(|| Error::FromLuaConversionError {
                from: ty,
                to: "CString",
                message: Some("expected string or number".to_string()),
            })?;

        match CStr::from_bytes_with_nul(string.as_bytes_with_nul()) {
            Ok(s) => Ok(s.into()),
            Err(_) => Err(Error::FromLuaConversionError {
                from: ty,
                to: "CString",
                message: Some("invalid C-style string".to_string()),
            }),
        }
    }
}

impl<'a> ToLua for &'a CStr {
    fn to_lua(self, ctx: &Context) -> Result<Value> {
        Ok(Value::String(ctx.create_string(self.to_bytes())?))
    }
}

impl<'a> ToLua for BString {
    fn to_lua(self, ctx: &Context) -> Result<Value> {
        Ok(Value::String(ctx.create_string(&self)?))
    }
}

impl FromLua for BString {
    fn from_lua(value: Value, ctx: &Context) -> Result<Self> {
        let ty = value.type_name();
        Ok(BString::from(
            ctx.coerce_string(value)?
                .ok_or_else(|| Error::FromLuaConversionError {
                    from: ty,
                    to: "String",
                    message: Some("expected string or number".to_string()),
                })?
                .as_bytes()
                .to_vec(),
        ))
    }
}

impl<'a> ToLua for &BStr {
    fn to_lua(self, ctx: &Context) -> Result<Value> {
        Ok(Value::String(ctx.create_string(&self)?))
    }
}

macro_rules! lua_convert_int {
    ($x:ty) => {
        impl ToLua for $x {
            fn to_lua(self, _: &Context) -> Result<Value> {
                if let Some(i) = cast(self) {
                    Ok(Value::Integer(i))
                } else {
                    cast(self)
                        .ok_or_else(|| Error::ToLuaConversionError {
                            from: stringify!($x),
                            to: "number",
                            message: Some("out of range".to_owned()),
                        })
                        .map(Value::Number)
                }
            }
        }

        impl FromLua for $x {
            fn from_lua(value: Value, ctx: &Context) -> Result<Self> {
                let ty = value.type_name();
                (if let Some(i) = ctx.coerce_integer(value.clone())? {
                    cast(i)
                } else {
                    cast(ctx.coerce_number(value)?.ok_or_else(|| {
                        Error::FromLuaConversionError {
                            from: ty,
                            to: stringify!($x),
                            message: Some(
                                "expected number or string coercible to number".to_string(),
                            ),
                        }
                    })?)
                })
                .ok_or_else(|| Error::FromLuaConversionError {
                    from: ty,
                    to: stringify!($x),
                    message: Some("out of range".to_owned()),
                })
            }
        }
    };
}

lua_convert_int!(i8);
lua_convert_int!(u8);
lua_convert_int!(i16);
lua_convert_int!(u16);
lua_convert_int!(i32);
lua_convert_int!(u32);
lua_convert_int!(i64);
lua_convert_int!(u64);
lua_convert_int!(i128);
lua_convert_int!(u128);
lua_convert_int!(isize);
lua_convert_int!(usize);

macro_rules! lua_convert_float {
    ($x:ty) => {
        impl ToLua for $x {
            fn to_lua(self, _: &Context) -> Result<Value> {
                Ok(Value::Number(self as Number))
            }
        }

        impl FromLua for $x {
            fn from_lua(value: Value, ctx: &Context) -> Result<Self> {
                let ty = value.type_name();
                ctx.coerce_number(value)?
                    .ok_or_else(|| Error::FromLuaConversionError {
                        from: ty,
                        to: stringify!($x),
                        message: Some("expected number or string coercible to number".to_string()),
                    })
                    .and_then(|n| {
                        cast(n).ok_or_else(|| Error::FromLuaConversionError {
                            from: ty,
                            to: stringify!($x),
                            message: Some("number out of range".to_string()),
                        })
                    })
            }
        }
    };
}

lua_convert_float!(f32);
lua_convert_float!(f64);

impl<T: ToLua> ToLua for Vec<T> {
    fn to_lua(self, ctx: &Context) -> Result<Value> {
        Ok(Value::Table(ctx.create_sequence_from(self)?))
    }
}

impl<T: FromLua> FromLua for Vec<T> {
    fn from_lua(value: Value, _: &Context) -> Result<Self> {
        if let Value::Table(table) = value {
            table.sequence_values().collect()
        } else {
            Err(Error::FromLuaConversionError {
                from: value.type_name(),
                to: "Vec",
                message: Some("expected table".to_string()),
            })
        }
    }
}

impl<K: Eq + Hash + ToLua, V: ToLua, S: BuildHasher> ToLua for HashMap<K, V, S> {
    fn to_lua(self, ctx: &Context) -> Result<Value> {
        Ok(Value::Table(ctx.create_table_from(self)?))
    }
}

impl<K: Eq + Hash + FromLua, V: FromLua, S: BuildHasher + Default> FromLua for HashMap<K, V, S> {
    fn from_lua(value: Value, _: &Context) -> Result<Self> {
        if let Value::Table(table) = value {
            table.pairs().collect()
        } else {
            Err(Error::FromLuaConversionError {
                from: value.type_name(),
                to: "HashMap",
                message: Some("expected table".to_string()),
            })
        }
    }
}

impl<K: Ord + ToLua, V: ToLua> ToLua for BTreeMap<K, V> {
    fn to_lua(self, ctx: &Context) -> Result<Value> {
        Ok(Value::Table(ctx.create_table_from(self)?))
    }
}

impl<K: Ord + FromLua, V: FromLua> FromLua for BTreeMap<K, V> {
    fn from_lua(value: Value, _: &Context) -> Result<Self> {
        if let Value::Table(table) = value {
            table.pairs().collect()
        } else {
            Err(Error::FromLuaConversionError {
                from: value.type_name(),
                to: "BTreeMap",
                message: Some("expected table".to_string()),
            })
        }
    }
}

impl<T: ToLua> ToLua for Option<T> {
    fn to_lua(self, ctx: &Context) -> Result<Value> {
        match self {
            Some(val) => val.to_lua(ctx),
            None => Ok(Nil),
        }
    }
}

impl<T: FromLua> FromLua for Option<T> {
    fn from_lua(value: Value, ctx: &Context) -> Result<Self> {
        match value {
            Nil => Ok(None),
            value => Ok(Some(T::from_lua(value, ctx)?)),
        }
    }
}
