//! Re-exports most types with an extra `Lua*` prefix to prevent name clashes.

pub use crate::lua::{
    Context as LuaContext, Error as LuaError, ExternalError as LuaExternalError,
    ExternalResult as LuaExternalResult, FromLua, FromLuaMulti, Function as LuaFunction,
    Integer as LuaInteger, MultiValue as LuaMultiValue, Nil as LuaNil, Number as LuaNumber,
    Result as LuaResult, String as LuaString, Table as LuaTable, TablePairs as LuaTablePairs,
    TableSequence as LuaTableSequence, ToLua, ToLuaMulti, Value as LuaValue,
};
