use crate::error::IntoBoxError;
use crate::ffi::tarantool as ffi;
use crate::tuple::{FunctionCtx, RawByteBuf, RawBytes, Tuple, TupleBuffer};
use serde::Serialize;
use std::os::raw::c_int;
use std::path::Path;

macro_rules! unwrap_or_report_err {
    ($res:expr) => {
        match $res {
            Ok(o) => o,
            Err(e) => {
                e.set_last_error();
                -1
            }
        }
    };
}

////////////////////////////////////////////////////////////////////////////////
// Proc
////////////////////////////////////////////////////////////////////////////////

/// Description of a tarantool stored procedure defined using the
/// `#[`[`tarantool::proc`]`]` macro attribute.
///
/// [`tarantool::proc`]: macro@crate::proc
#[derive(Debug, Clone)]
pub struct Proc {
    name: &'static str,
    proc: ffi::Proc,
    public: bool,
}

impl Proc {
    /// Create a new stored proc description.
    ///
    /// This function is called when `#[`[`tarantool::proc`]`]` attribute is
    /// used, so users don't usually use it directly.
    ///
    /// See also [`module_path`]
    ///
    /// [`tarantool::proc`]: macro@crate::proc
    /// [`module_path`]: module_path()
    #[inline(always)]
    pub const fn new(name: &'static str, proc: ffi::Proc) -> Self {
        Self {
            name,
            proc,
            public: false,
        }
    }

    #[inline(always)]
    pub const fn with_public(mut self, public: bool) -> Self {
        self.public = public;
        self
    }

    /// Get the name of the stored procedure NOT including the module name.
    #[inline(always)]
    pub const fn name(&self) -> &'static str {
        self.name
    }

    /// Get the proc's function pointer.
    ///
    /// This function is usually not necessary for defining tarantool's stored
    /// procedures, the name is enough. But it is there if you need it for some
    /// reason.
    #[inline(always)]
    pub const fn proc(&self) -> ffi::Proc {
        self.proc
    }

    /// Returns `true` if the proc has `pub` visibility specifier, but can be
    /// overriden with the `public` attribute.
    ///
    /// Can be used when choosing which stored procedures the "public" role
    /// should have access to.
    ///
    /// See <https://www.tarantool.io/en/doc/latest/reference/reference_lua/box_space/_user/>
    /// for more info about role "public".
    #[inline(always)]
    pub const fn is_public(&self) -> bool {
        self.public
    }
}

// In picodata, we cannot guarantee that tarantool module will be linked
// exactly once due to possible stale versions from various dependencies.
// This whole feature is here to disable the distributed slice definition
// in all but one instance of this crate; otherwise we'll trigger linkme's
// "duplicate distributed slice" check introduced in 0.3.1.
#[cfg(feature = "stored_procs_slice")]
pub use stored_procs_slice::*;
#[cfg(feature = "stored_procs_slice")]
mod stored_procs_slice {
    use super::*;

    // Linkme distributed_slice exports a symbol with the given name, so we must
    // make sure the name is unique, so as not to conflict with distributed slices
    // from other crates or any other global symbols.
    /// *INTERNAL API* It is only marked `pub` because it needs to be accessed
    /// from procedural macros.
    #[doc(hidden)]
    #[::linkme::distributed_slice]
    pub static TARANTOOL_MODULE_STORED_PROCS: [Proc] = [..];

    /// Returns a slice of all stored procedures defined using the
    /// `#[`[`tarantool::proc`]`]` macro attribute.
    ///
    /// The order of procs in the slice is undefined.
    ///
    /// [`tarantool::proc`]: macro@crate::proc
    #[inline(always)]
    pub fn all_procs() -> &'static [Proc] {
        &TARANTOOL_MODULE_STORED_PROCS
    }
}

#[cfg(not(feature = "stored_procs_slice"))]
pub fn all_procs() -> &'static [Proc] {
    panic!("`stored_procs_slice` feature is disabled, calling this function doesn't make sense");
}

////////////////////////////////////////////////////////////////////////////////
// module_name
////////////////////////////////////////////////////////////////////////////////

/// Returns a path to the dynamically linked ojbect in which the symbol pointed
/// to by `sym` is defined.
///
/// This can be used to dynamically figure out the module name for tarantool's
/// stored procedure definition, for example by passing in a pointer to the
/// function defined using `#[`[`tarantool::proc`]`]` macro attribute, but is
/// NOT GUARANTEED TO WORK.
///
/// ```no_run
/// use tarantool::proc::module_path;
///
/// #[tarantool::proc]
/// fn my_proc() -> i32 {
///     69
/// }
///
/// let path = module_path(my_proc as _).unwrap();
/// let filename = path.file_stem().unwrap();
/// assert_eq!(filename, std::ffi::OsStr::new("libmy_library"));
/// ```
///
/// [`tarantool::proc`]: macro@crate::proc
pub fn module_path(sym: *const ()) -> Option<&'static Path> {
    unsafe {
        let mut info: libc::Dl_info = std::mem::zeroed();
        if libc::dladdr(sym as _, &mut info) == 0 {
            return None;
        }

        if info.dli_fname.is_null() {
            return None;
        }

        let path = std::ffi::CStr::from_ptr(info.dli_fname);
        let path: &std::ffi::OsStr = std::os::unix::ffi::OsStrExt::from_bytes(path.to_bytes());
        Some(Path::new(path))
    }
}

////////////////////////////////////////////////////////////////////////////////
// ReturnMsgpack
////////////////////////////////////////////////////////////////////////////////

/// A wrapper type for returning custom types from stored procedures. Consider
/// using the `custom_ret` attribute parameter instead (see [`tarantool::proc`]
/// docs for examples).
///
/// # using `ReturnMsgpack` directly
///
/// You can either return `ReturnMsgpack` directly:
///
/// ```no_run
/// use tarantool::proc::ReturnMsgpack;
///
/// #[tarantool::proc]
/// fn foo(x: i32) -> ReturnMsgpack<MyStruct> {
///     ReturnMsgpack(MyStruct { x, y: x * 2 })
/// }
///
/// #[derive(serde::Serialize)]
/// struct MyStruct { x: i32, y: i32 }
/// ```
///
/// # implementing `Return` for custom type
///
/// Or you can use it to implement `Return` for your custom type:
///
/// ```no_run
/// use std::os::raw::c_int;
/// use tarantool::{proc::{Return, ReturnMsgpack}, tuple::FunctionCtx};
///
/// #[tarantool::proc]
/// fn foo(x: i32) -> MyStruct {
///     MyStruct { x, y: x * 2 }
/// }
///
/// #[derive(serde::Serialize)]
/// struct MyStruct { x: i32, y: i32 }
///
/// impl Return for MyStruct {
///     fn ret(self, ctx: FunctionCtx) -> c_int {
///         ReturnMsgpack(self).ret(ctx)
///     }
/// }
/// ```
///
/// [`tarantool::proc`]: macro@crate::proc
pub struct ReturnMsgpack<T>(pub T);

impl<T: Serialize> Return for ReturnMsgpack<T> {
    #[inline(always)]
    #[track_caller]
    fn ret(self, ctx: FunctionCtx) -> c_int {
        unwrap_or_report_err!(ctx.return_mp(&self.0))
    }
}

////////////////////////////////////////////////////////////////////////////////
// Return
////////////////////////////////////////////////////////////////////////////////

pub trait Return: Sized {
    fn ret(self, ctx: FunctionCtx) -> c_int;
}

impl Return for Tuple {
    #[inline]
    #[track_caller]
    fn ret(self, ctx: FunctionCtx) -> c_int {
        let res = ctx.return_tuple(&self);
        unwrap_or_report_err!(res)
    }
}

impl<E> Return for Result<Tuple, E>
where
    E: IntoBoxError,
{
    #[inline(always)]
    #[track_caller]
    fn ret(self, ctx: FunctionCtx) -> c_int {
        unwrap_or_report_err!(self.map(|t| t.ret(ctx)))
    }
}

impl Return for TupleBuffer {
    #[inline]
    #[track_caller]
    fn ret(self, ctx: FunctionCtx) -> c_int {
        let res = ctx.return_bytes(self.as_ref());
        unwrap_or_report_err!(res)
    }
}

impl<E> Return for Result<TupleBuffer, E>
where
    E: IntoBoxError,
{
    #[inline(always)]
    #[track_caller]
    fn ret(self, ctx: FunctionCtx) -> c_int {
        unwrap_or_report_err!(self.map(|t| t.ret(ctx)))
    }
}

impl Return for &RawBytes {
    #[inline]
    #[track_caller]
    fn ret(self, ctx: FunctionCtx) -> c_int {
        let res = ctx.return_bytes(self);
        unwrap_or_report_err!(res)
    }
}

impl<E> Return for Result<&RawBytes, E>
where
    E: IntoBoxError,
{
    #[inline(always)]
    #[track_caller]
    fn ret(self, ctx: FunctionCtx) -> c_int {
        unwrap_or_report_err!(self.map(|t| t.ret(ctx)))
    }
}

impl Return for RawByteBuf {
    #[inline]
    #[track_caller]
    fn ret(self, ctx: FunctionCtx) -> c_int {
        let res = ctx.return_bytes(&self);
        unwrap_or_report_err!(res)
    }
}

impl<E> Return for Result<RawByteBuf, E>
where
    E: IntoBoxError,
{
    #[inline(always)]
    #[track_caller]
    fn ret(self, ctx: FunctionCtx) -> c_int {
        unwrap_or_report_err!(self.map(|t| t.ret(ctx)))
    }
}

impl Return for () {
    #[inline(always)]
    fn ret(self, _: FunctionCtx) -> c_int {
        0
    }
}

impl<O, E> Return for Result<O, E>
where
    O: Serialize,
    E: IntoBoxError,
{
    #[inline(always)]
    #[track_caller]
    fn ret(self, ctx: FunctionCtx) -> c_int {
        match self {
            Ok(o) => match ctx.return_mp(&o) {
                Ok(_) => 0,
                Err(e) => {
                    e.set_last_error();
                    -1
                }
            },
            Err(e) => {
                e.set_last_error();
                -1
            }
        }
    }
}

macro_rules! impl_return {
    (impl $([ $( $tp:tt )* ])? for $t:ty) => {
        impl $(< $($tp)* >)? Return for $t
        where
            Self: Serialize,
        {
            #[inline(always)]
            #[track_caller]
            fn ret(self, ctx: FunctionCtx) -> c_int {
                unwrap_or_report_err!(ctx.return_mp(&self))
            }
        }
    };
    ($( $t:ty )+) => {
        $( impl_return!{ impl for $t } )+
    }
}

impl_return! { impl[V]                 for Option<V> }
impl_return! { impl[V]                 for Vec<V> }
impl_return! { impl[V]                 for &'_ [V] }
impl_return! { impl[V, const N: usize] for [V; N] }
impl_return! { impl[K, V]              for std::collections::HashMap<K, V> }
impl_return! { impl[K]                 for std::collections::HashSet<K> }
impl_return! { impl[K, V]              for std::collections::BTreeMap<K, V> }
impl_return! { impl[K]                 for std::collections::BTreeSet<K> }
impl_return! {
    bool
    i8 u8 i16 u16 i32 u32 i64 u64 i128 u128 isize usize
    f32 f64
    String &'_ str
    std::ffi::CString &'_ std::ffi::CStr
}

macro_rules! impl_return_for_tuple {
    () => {};
    ($h:ident $($t:ident)*) => {
        impl<$h, $($t),*> Return for ($h, $($t,)*)
        where
            Self: Serialize,
        {
            #[inline(always)]
            #[track_caller]
            fn ret(self, ctx: FunctionCtx) -> c_int {
                unwrap_or_report_err!(ctx.return_mp(&self))
            }
        }

        impl_return_for_tuple!{$($t)*}
    }
}
impl_return_for_tuple! {A B C D E F G H I J K L M N O P Q}
