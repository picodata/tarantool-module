use crate::{
    error::TarantoolErrorCode::ProcC,
    ffi::tarantool::TarantoolProc,
    set_error,
    tuple::{FunctionCtx, RawByteBuf, RawBytes, Tuple, TupleBuffer},
};
use serde::Serialize;
use std::{fmt::Display, os::raw::c_int};

macro_rules! unwrap_or_report_err {
    ($res:expr) => {
        match $res {
            Ok(o) => o,
            Err(e) => {
                set_error!(ProcC, "{}", e);
                -1
            }
        }
    };
}

/// All stored procedures collected by
/// [`collect_proc!`](crate::collect_proc) macro.
pub struct AllProcs;

#[doc(hidden)]
#[::linkme::distributed_slice]
pub static ALL_PROCS: [(&'static str, TarantoolProc)] = [..];

impl AllProcs {
    /// Returns an iterator over all procs names.
    pub fn names() -> impl Iterator<Item = &'static str> {
        ALL_PROCS.iter().map(|v| v.0)
    }
}

#[macro_export]
/// Collects a function to be used as a tarantool stored procedure.
///
/// The objective of this macro is to simplify creating stored
/// procedures (`box.schema.func.create` invoked during the instance
/// initialization). And, since those functions aren't invoked directly,
/// to prevent symbols removal due to the link-time optimization.
///
/// For retrieving collected items refer to [`AllProcs`].
///
macro_rules! collect_proc {
    ($proc:ident) => {
        impl $crate::proc::AllProcs {
            #[allow(dead_code)]
            #[doc(hidden)]
            fn $proc() {
                // Enclosing static variable in a function allows to avoid name clash.
                #[::linkme::distributed_slice($crate::proc::ALL_PROCS)]
                pub static PROC: (&str, $crate::ffi::tarantool::TarantoolProc) =
                    ($crate::stringify_cfunc!($proc), $proc);
            }
        }
    };
}

/// Checks that the given function exists and returns it's name suitable for
/// calling it via tarantool rpc.
///
/// The argument can be a full path to the function.
#[macro_export]
macro_rules! stringify_cfunc {
    ( $($func_name:tt)+ ) => {{
        use $crate::tuple::FunctionArgs;
        use $crate::tuple::FunctionCtx;
        use libc::c_int;

        // Existence check
        const _: unsafe extern "C" fn(FunctionCtx, FunctionArgs) -> c_int = $($func_name)+;

        concat!(".", $crate::stringify_last_token!($($func_name)+))
    }};
}

#[macro_export]
macro_rules! stringify_last_token {
    ($tail:tt) => { std::stringify!($tail) };
    ($head:tt $($tail:tt)+) => { $crate::stringify_last_token!($($tail)+) };
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
/// ```
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
/// ```
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
    fn ret(self, ctx: FunctionCtx) -> c_int {
        let res = ctx.return_tuple(&self);
        unwrap_or_report_err!(res)
    }
}

impl<E> Return for Result<Tuple, E>
where
    E: Display,
{
    #[inline(always)]
    fn ret(self, ctx: FunctionCtx) -> c_int {
        unwrap_or_report_err!(self.map(|t| t.ret(ctx)))
    }
}

impl Return for TupleBuffer {
    #[inline]
    fn ret(self, ctx: FunctionCtx) -> c_int {
        let res = ctx.return_bytes(self.as_ref());
        unwrap_or_report_err!(res)
    }
}

impl<E> Return for Result<TupleBuffer, E>
where
    E: Display,
{
    #[inline(always)]
    fn ret(self, ctx: FunctionCtx) -> c_int {
        unwrap_or_report_err!(self.map(|t| t.ret(ctx)))
    }
}

impl Return for &RawBytes {
    #[inline]
    fn ret(self, ctx: FunctionCtx) -> c_int {
        let res = ctx.return_bytes(self);
        unwrap_or_report_err!(res)
    }
}

impl<E> Return for Result<&RawBytes, E>
where
    E: Display,
{
    #[inline(always)]
    fn ret(self, ctx: FunctionCtx) -> c_int {
        unwrap_or_report_err!(self.map(|t| t.ret(ctx)))
    }
}

impl Return for RawByteBuf {
    #[inline]
    fn ret(self, ctx: FunctionCtx) -> c_int {
        let res = ctx.return_bytes(&self);
        unwrap_or_report_err!(res)
    }
}

impl<E> Return for Result<RawByteBuf, E>
where
    E: Display,
{
    #[inline(always)]
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
    E: Display,
{
    #[inline(always)]
    fn ret(self, ctx: FunctionCtx) -> c_int {
        match self {
            Ok(o) => match ctx.return_mp(&o) {
                Ok(_) => 0,
                Err(e) => {
                    set_error!(ProcC, "{}", e);
                    -1
                }
            },
            Err(e) => {
                set_error!(ProcC, "{}", e);
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
            fn ret(self, ctx: FunctionCtx) -> c_int {
                unwrap_or_report_err!(ctx.return_mp(&self))
            }
        }

        impl_return_for_tuple!{$($t)*}
    }
}
impl_return_for_tuple! {A B C D E F G H I J K L M N O P Q}
