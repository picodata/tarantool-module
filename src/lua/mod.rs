//! # High-level bindings to Lua
//!
//! This crate provides safe high-level bindings to the [Lua programming language].
//!
//! Most of code is borrowed (copied) from https://github.com/amethyst/rlua.
//!
//! # The `Lua` object
//!
//! The main type exported by this library is the [`Lua`] struct. In addition to methods for
//! [executing] Lua chunks or [evaluating] Lua expressions, it provides methods for creating Lua
//! values and accessing the table of [globals].
//!
//! # Converting data
//!
//! The [`ToLua`] and [`FromLua`] traits allow conversion from Rust types to Lua values and vice
//! versa. They are implemented for many data structures found in Rust's standard library.
//!
//! For more general conversions, the [`ToLuaMulti`] and [`FromLuaMulti`] traits allow converting
//! between Rust types and *any number* of Lua values.
//!
//! Most code crate is generic over implementors of those traits, so in most places the normal
//! Rust data structures are accepted without having to write any boilerplate.
//!
//! [Lua programming language]: https://www.lua.org/
//! [`Lua`]: struct.Lua.html
//! [globals]: struct.Context.html#method.globals
//! [`ToLua`]: trait.ToLua.html
//! [`FromLua`]: trait.FromLua.html
//! [`ToLuaMulti`]: trait.ToLuaMulti.html
//! [`FromLuaMulti`]: trait.FromLuaMulti.html

// Deny warnings inside doc tests / examples. When this isn't present, rustdoc doesn't show *any*
// warnings at all.
#![doc(test(attr(deny(warnings))))]

#[macro_use]
mod macros;

mod context;
mod conversion;
mod error;
mod ffi;
mod function;
mod multi;
mod string;
mod table;
mod types;
mod util;
mod value;

pub use crate::lua::context::Context;
pub use crate::lua::error::{Error, ExternalError, ExternalResult, Result};
pub use crate::lua::function::Function;
pub use crate::lua::multi::Variadic;
pub use crate::lua::string::String;
pub use crate::lua::table::{Table, TablePairs, TableSequence};
pub use crate::lua::types::{Integer, Number};
pub use crate::lua::value::{FromLua, FromLuaMulti, MultiValue, Nil, ToLua, ToLuaMulti, Value};

pub mod prelude;
