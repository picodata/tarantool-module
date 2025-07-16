use crate::error::{BoxError, TarantoolErrorCode};

use std::ffi::CStr;
use std::os::raw::c_char;
use std::ptr::NonNull;

use libloading::os::unix::Library;

////////////////////////////////////////////////////////////////////////////////
// c_str!
////////////////////////////////////////////////////////////////////////////////

/// Returns a [`std::ffi::CStr`] constructed from the provided literal with a
/// nul byte appended to the end. Use this when you need a `&CStr` from a
/// string literal.
///
/// # Example
/// ```rust
/// # use tarantool::c_str;
///
/// let c_str = c_str!("hello");
/// assert_eq!(c_str.to_bytes(), b"hello");
/// ```
///
/// This macro will also check at compile time if the string literal contains
/// any interior nul bytes.
/// ```compile_fail
/// # use tarantool::c_str;
/// let this_doesnt_compile = c_str!("interior\0nul\0byte");
/// ```
#[macro_export]
macro_rules! c_str {
    ($s:expr) => {{
        #[allow(unused_unsafe)]
        const RESULT: &'static ::std::ffi::CStr = unsafe {
            ::std::ffi::CStr::from_bytes_with_nul_unchecked(::std::concat!($s, "\0").as_bytes())
        };
        RESULT
    }};
}

////////////////////////////////////////////////////////////////////////////////
// c_ptr!
////////////////////////////////////////////////////////////////////////////////

/// Returns a `*const std::ffi::c_char` constructed from the provided literal
/// with a nul byte appended to the end. Use this to pass static c-strings when
/// working with ffi.
///
/// # Example
/// ```rust
/// # use tarantool::c_ptr;
/// extern "C" {
///     fn strlen(s: *const std::ffi::c_char) -> usize;
/// }
///
/// let count = unsafe { strlen(c_ptr!("foo bar")) };
/// assert_eq!(count, 7);
/// ```
///
/// Same as [`c_str!`], this macro will check at compile time if the string
/// literal contains any interior nul bytes.
///
/// [`c_str!`]: crate::c_str
#[macro_export]
macro_rules! c_ptr {
    ($s:expr) => {
        $crate::c_str!($s).as_ptr()
    };
}

////////////////////////////////////////////////////////////////////////////////
// offset_of!
////////////////////////////////////////////////////////////////////////////////

/// Returns an offset of the struct or tuple member in bytes.
///
/// Returns a constant which can be used at compile time.
///
/// # Example
/// ```rust
/// # use tarantool::offset_of;
/// #[repr(C)]
/// struct MyStruct { a: u8, b: u8 }
/// assert_eq!(offset_of!(MyStruct, a), 0);
/// assert_eq!(offset_of!(MyStruct, b), 1);
///
/// // Also works with tuples:
/// assert_eq!(offset_of!((i32, i32), 0), 0);
/// assert_eq!(offset_of!((i32, i32), 1), 4);
/// ```
#[macro_export]
macro_rules! offset_of {
    ($type:ty, $field:tt) => {{
        const RESULT: usize = unsafe {
            let dummy = ::core::mem::MaybeUninit::<$type>::uninit();
            let dummy_ptr = dummy.as_ptr();
            let field_ptr = ::std::ptr::addr_of!((*dummy_ptr).$field);

            let field_ptr = field_ptr.cast::<u8>();
            let dummy_ptr = dummy_ptr.cast::<u8>();
            field_ptr.offset_from(dummy_ptr) as usize
        };
        RESULT
    }};
}

////////////////////////////////////////////////////////////////////////////////
// static_assert!
////////////////////////////////////////////////////////////////////////////////

#[macro_export]
macro_rules! static_assert {
    ($e:expr $(,)?) => {
        const _: () = assert!($e);
    };
    ($e:expr, $msg:expr $(,)?) => {
        const _: () = assert!($e, $msg);
    };
}

////////////////////////////////////////////////////////////////////////////////
// define_dlsym_reloc!
////////////////////////////////////////////////////////////////////////////////

#[macro_export]
macro_rules! define_dlsym_reloc {
    (
        $(
            $(#[$meta:meta])*
            pub $(($where:tt))? fn $sym:ident ( $( $args:ident: $types:ty ),* $(,)? ) $( -> $ret:ty )?;
        )+
    ) => {
        $(
            $(#[$meta])*
            #[inline(always)]
            pub $(($where))? unsafe fn $sym($($args: $types),*) $(-> $ret)? {
                return RELOC_FN($($args),*);

                type SymType = unsafe fn($($args: $types),*) $(-> $ret)?;
                static mut RELOC_FN: SymType = init;

                unsafe fn init($($args: $types),*) $(-> $ret)? {
                    let sym_name = $crate::c_str!(::std::stringify!($sym));
                    let impl_fn: SymType = $crate::ffi::helper::get_any_symbol(sym_name)
                        .unwrap();
                    RELOC_FN = impl_fn;
                    RELOC_FN($($args),*)
                }
            }
        )+
    };
}

/// Find a symbol using the `tnt_internal_symbol` api.
///
/// This function performs a slow search over all the exported internal
/// tarantool symbols, so don't use it everytime you want to call a given
/// function.
#[inline]
pub unsafe fn tnt_internal_symbol<T>(name: &CStr) -> Option<T> {
    if std::mem::size_of::<T>() != std::mem::size_of::<*mut ()>() {
        return None;
    }
    let ptr = (RELOC_FN?)(name.as_ptr())?;
    return Some(std::mem::transmute_copy(&ptr));

    type SymType = unsafe fn(*const c_char) -> Option<NonNull<()>>;
    static mut RELOC_FN: Option<SymType> = Some(init);

    unsafe fn init(name: *const c_char) -> Option<NonNull<()>> {
        let current_library = Library::this();
        let internal_symbol = c_str!("tnt_internal_symbol").to_bytes();
        match current_library.get(internal_symbol) {
            Ok(sym) => {
                RELOC_FN = Some(*sym);
                (RELOC_FN.unwrap())(name)
            }
            Err(_) => {
                RELOC_FN = None;
                None
            }
        }
    }
}

/// Check if symbol can be found in the current executable using dlsym.
#[inline]
pub unsafe fn has_dyn_symbol(name: &CStr) -> bool {
    get_dyn_symbol::<*const ()>(name).is_ok()
}

/// Find a sybmol in the current executable using dlsym.
#[inline]
pub unsafe fn get_dyn_symbol<T: Copy>(name: &CStr) -> Result<T, BoxError> {
    let current_library = Library::this();
    let symbol_name = name.to_bytes_with_nul();
    let symbol_pointer = current_library.get::<T>(symbol_name).map_err(|e| {
        let code = TarantoolErrorCode::NoSuchProc;
        let message = format!("symbol '{name:?}' not found: {e}");
        BoxError::new(code, message)
    })?;
    Ok(*symbol_pointer)
}

/// Find a symbol either using the `tnt_internal_symbol` api or using dlsym as a
/// fallback.
#[inline]
pub unsafe fn get_any_symbol<T: Copy>(name: &CStr) -> Result<T, BoxError> {
    if let Some(sym) = tnt_internal_symbol(name) {
        return Ok(sym);
    }
    let current_library = Library::this();
    let symbol_name = name.to_bytes_with_nul();
    let symbol_pointer = current_library.get::<T>(symbol_name).map_err(|e| {
        let code = TarantoolErrorCode::NoSuchProc;
        let message = format!("symbol '{name:?}' not found: {e}");
        BoxError::new(code, message)
    })?;
    Ok(*symbol_pointer)
}
