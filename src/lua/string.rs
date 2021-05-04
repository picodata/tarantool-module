use std::{slice, str};

use crate::lua::ffi;

use crate::lua::error::{Error, Result};
use crate::lua::types::LuaRef;
use crate::lua::util::{assert_stack, StackGuard};

/// Handle to an internal Lua string.
///
/// Unlike Rust strings, Lua strings may not be valid UTF-8.
#[derive(Clone, Debug)]
pub struct String(pub(crate) LuaRef);

impl String {
    /// Get a `&str` slice if the Lua string is valid UTF-8.
    ///
    /// # Examples
    ///
    /// ```
    /// # use rlua::{Lua, String, Result};
    /// # fn main() -> Result<()> {
    /// # Lua::new().context(|lua_context| {
    /// let globals = lua_context.globals();
    ///
    /// let version: String = globals.get("_VERSION")?;
    /// assert!(version.to_str().unwrap().contains("Lua"));
    ///
    /// let non_utf8: String = lua_context.load(r#"  "test\xff"  "#).eval()?;
    /// assert!(non_utf8.to_str().is_err());
    /// # Ok(())
    /// # })
    /// # }
    /// ```
    pub fn to_str(&self) -> Result<&str> {
        str::from_utf8(self.as_bytes()).map_err(|e| Error::FromLuaConversionError {
            from: "string",
            to: "&str",
            message: Some(e.to_string()),
        })
    }

    /// Get the bytes that make up this string.
    ///
    /// The returned slice will not contain the terminating nul byte, but will contain any nul
    /// bytes embedded into the Lua string.
    ///
    /// # Examples
    ///
    /// ```
    /// # use rlua::{Lua, String, Result};
    /// # fn main() -> Result<()> {
    /// # Lua::new().context(|lua_context| -> Result<()> {
    /// let non_utf8: String = lua_context.load(r#"  "test\xff"  "#).eval()?;
    /// assert!(non_utf8.to_str().is_err());    // oh no :(
    /// assert_eq!(non_utf8.as_bytes(), &b"test\xff"[..]);
    /// # Ok(())
    /// # })
    /// # }
    /// ```
    pub fn as_bytes(&self) -> &[u8] {
        let nulled = self.as_bytes_with_nul();
        &nulled[..nulled.len() - 1]
    }

    /// Get the bytes that make up this string, including the trailing nul byte.
    pub fn as_bytes_with_nul(&self) -> &[u8] {
        let ctx = &self.0.ctx;
        unsafe {
            let _sg = StackGuard::new(ctx.state);
            assert_stack(ctx.state, 1);

            ctx.push_ref(&self.0);
            rlua_debug_assert!(
                ffi::lua_type(ctx.state, -1) == ffi::LUA_TSTRING,
                "string ref is not string type"
            );

            let mut size = 0;
            // This will not trigger a 'm' error, because the reference is guaranteed to be of
            // string type
            let data = ffi::lua_tolstring(ctx.state, -1, &mut size);

            slice::from_raw_parts(data as *const u8, size + 1)
        }
    }
}

impl AsRef<[u8]> for String {
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

// Lua strings are basically &[u8] slices, so implement PartialEq for anything resembling that.
//
// This makes our `String` comparable with `Vec<u8>`, `[u8]`, `&str`, `String` and `rlua::String`
// itself.
//
// The only downside is that this disallows a comparison with `Cow<str>`, as that only implements
// `AsRef<str>`, which collides with this impl. Requiring `AsRef<str>` would fix that, but limit us
// in other ways.
impl<T> PartialEq<T> for String
where
    T: AsRef<[u8]>,
{
    fn eq(&self, other: &T) -> bool {
        self.as_bytes() == other.as_ref()
    }
}
