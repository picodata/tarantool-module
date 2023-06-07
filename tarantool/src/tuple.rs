//! Tuples
//!
//! The `tuple` submodule provides read-only access for the tuple userdata type.
//! It allows, for a single tuple: selective retrieval of the field contents, retrieval of information about size,
//! iteration over all the fields, and conversion from/to rust structures
//!
//! See also:
//! - [Tuples](https://www.tarantool.io/en/doc/2.2/book/box/data_model/#tuples)
//! - [Lua reference: Submodule box.tuple](https://www.tarantool.io/en/doc/2.2/reference/reference_lua/box_tuple/)
//! - [C API reference: Module tuple](https://www.tarantool.io/en/doc/2.2/dev_guide/reference_capi/tuple/)
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::BTreeMap;
use std::convert::TryFrom;
use std::ffi::{CStr, CString};
use std::fmt::{self, Debug, Formatter};
use std::io::Write;
use std::ops::{Deref, Range};
use std::os::raw::{c_char, c_int};
use std::ptr::{null, NonNull};

use rmp::Marker;
use serde::Serialize;
use tarantool_proc::impl_tuple_encode;

use crate::error::{self, Error, Result, TarantoolError};
use crate::ffi::tarantool as ffi;
use crate::index;
use crate::tlua;
use crate::util::NumOrStr;

pub use rmp;
pub use tarantool_proc::Encode;

/// Tuple
pub struct Tuple {
    ptr: NonNull<ffi::BoxTuple>,
}

impl Debug for Tuple {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        if let Ok(v) = self.decode::<rmpv::Value>() {
            f.debug_tuple("Tuple").field(&v).finish()
        } else {
            // Probably will never happen but better safe than sorry
            f.debug_tuple("Tuple").field(&self.as_buffer()).finish()
        }
    }
}

impl Tuple {
    /// Create a new tuple from `value` implementing [`ToTupleBuffer`].
    #[inline]
    pub fn new<T>(value: &T) -> Result<Self>
    where
        T: ToTupleBuffer + ?Sized,
    {
        Ok(Self::from(&value.to_tuple_buffer()?))
    }

    /// Creates new tuple from `value`.
    ///
    /// This function will serialize structure instance `value` of type `T` into tuple internal representation
    ///
    /// See also: [AsTuple](trait.AsTuple.html)
    #[deprecated = "Use `Tuple::new` instead."]
    #[inline]
    pub fn from_struct<T>(value: &T) -> Result<Self>
    where
        T: ToTupleBuffer + ?Sized,
    {
        Self::new(value)
    }

    /// # Safety
    /// `data` must point to a buffer containing `len` bytes representing a
    /// valid messagepack array
    pub unsafe fn from_raw_data(data: *mut c_char, len: u32) -> Self {
        let format = TupleFormat::default();
        let tuple_ptr = ffi::box_tuple_new(format.inner, data as _, data.add(len as _) as _);

        Self::from_ptr(NonNull::new_unchecked(tuple_ptr))
    }

    /// # Safety
    /// `data` must represent a valid messagepack array
    pub unsafe fn from_slice(data: &[u8]) -> Self {
        let format = TupleFormat::default();
        let Range { start, end } = data.as_ptr_range();
        let tuple_ptr = ffi::box_tuple_new(format.inner, start as _, end as _);

        Self::from_ptr(NonNull::new_unchecked(tuple_ptr))
    }

    pub fn try_from_slice(data: &[u8]) -> Result<Self> {
        let data = validate_msgpack(data)?;
        unsafe { Ok(Self::from_slice(data)) }
    }

    pub fn from_ptr(mut ptr: NonNull<ffi::BoxTuple>) -> Self {
        unsafe { ffi::box_tuple_ref(ptr.as_mut()) };
        Tuple { ptr }
    }

    pub fn try_from_ptr(ptr: *mut ffi::BoxTuple) -> Option<Self> {
        NonNull::new(ptr).map(Self::from_ptr)
    }

    /// Return the number of fields in tuple (the size of MsgPack Array).
    pub fn len(&self) -> u32 {
        unsafe { ffi::box_tuple_field_count(self.ptr.as_ptr()) }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Will return the number of bytes in the tuple.
    ///
    /// With both the memtx storage engine and the vinyl storage engine the default maximum is one megabyte
    /// (`memtx_max_tuple_size` or `vinyl_max_tuple_size`). Every field has one or more "length" bytes preceding the
    /// actual contents, so `bsize()` returns a value which is slightly greater than the sum of the lengths of the
    /// contents.
    ///
    /// The value does not include the size of "struct tuple"
    /// (for the current size of this structure look in the tuple.h file in Tarantool’s source code).
    pub fn bsize(&self) -> usize {
        unsafe { self.ptr.as_ref().bsize() }
    }

    /// Return the associated format.
    pub fn format(&self) -> TupleFormat {
        TupleFormat {
            inner: unsafe { ffi::box_tuple_format(self.ptr.as_ptr()) },
        }
    }

    /// Allocate and initialize a new `Tuple` iterator. The `Tuple` iterator
    /// allow to iterate over fields at root level of MsgPack array.
    ///
    /// Example:
    /// ```no_run
    /// # fn foo<T: serde::de::DeserializeOwned>(tuple: tarantool::tuple::Tuple) {
    /// let mut it = tuple.iter().unwrap();
    ///
    /// while let Some(field) = it.next::<T>().unwrap() {
    ///     // process data
    /// }
    ///
    /// // rewind iterator to first position
    /// it.rewind();
    /// assert!(it.position() == 0);
    ///
    /// // rewind iterator to first position
    /// let field = it.seek::<T>(3).unwrap();
    /// assert!(it.position() == 4);
    /// }
    /// ```
    pub fn iter(&self) -> Result<TupleIterator> {
        let inner = unsafe { ffi::box_tuple_iterator(self.ptr.as_ptr()) };
        if inner.is_null() {
            Err(TarantoolError::last().into())
        } else {
            Ok(TupleIterator { inner })
        }
    }

    /// Deserialize a tuple field specified by zero-based array index.
    ///
    /// - `fieldno` - zero-based index in MsgPack array.
    ///
    /// Returns:
    /// - `Ok(None)` if `fieldno >= self.len()`
    /// - `Err(e)` if deserialization failed
    /// - `Ok(Some(field value))` otherwise
    ///
    /// See also [`Tuple::try_get`], [`Tuple::get`].
    pub fn field<'a, T>(&'a self, fieldno: u32) -> Result<Option<T>>
    where
        T: Decode<'a>,
    {
        unsafe {
            let field_ptr = ffi::box_tuple_field(self.ptr.as_ptr(), fieldno);
            field_value_from_ptr(field_ptr as _)
        }
    }

    /// Deserialize a tuple field specified by an index implementing
    /// [`TupleIndex`] trait.
    ///
    /// Currently 2 types of indexes are supported:
    /// - `u32` - zero-based index in MsgPack array (See also [`Tuple::field`])
    /// - `&str` - JSON path for tuples with non default formats
    ///
    /// **NOTE**: getting tuple fields by JSON paths is not supported in all
    /// tarantool versions. Use [`tarantool::ffi::has_tuple_field_by_path`] to
    /// check whether it's supported in your case.
    /// If `has_tuple_field_by_path` returns `false` this function will always
    /// return `Err`.
    ///
    /// Returns:
    /// - `Ok(None)` if index wasn't found
    /// - `Err(e)` if deserialization failed (or api not supported)
    /// - `Ok(Some(field value))` otherwise
    ///
    /// See also [`Tuple::get`].
    ///
    /// [`tarantool::ffi::has_tuple_field_by_path`]:
    /// crate::ffi::has_tuple_field_by_path
    #[inline(always)]
    pub fn try_get<'a, I, T>(&'a self, key: I) -> Result<Option<T>>
    where
        I: TupleIndex,
        T: Decode<'a>,
    {
        key.get_field(self)
    }

    /// Deserialize a tuple field specified by an index implementing
    /// [`TupleIndex`] trait.
    ///
    /// Currently 2 types of indexes are supported:
    /// - `u32` - zero-based index in MsgPack array (See also [`Tuple::field`])
    /// - `&str` - JSON path for tuples with non default formats
    ///
    /// **NOTE**: getting tuple fields by JSON paths is not supported in all
    /// tarantool versions. Use [`tarantool::ffi::has_tuple_field_by_path`] to
    /// check whether it's supported in your case.
    /// If `has_tuple_field_by_path` returns `false` this function will always
    /// **panic**.
    ///
    /// Returns:
    /// - `None` if index wasn't found
    /// - **panics** if deserialization failed (or api not supported)
    /// - `Some(field value)` otherwise
    ///
    /// See also [`Tuple::get`].
    ///
    /// [`tarantool::ffi::has_tuple_field_by_path`]:
    /// crate::ffi::has_tuple_field_by_path
    #[inline(always)]
    #[track_caller]
    pub fn get<'a, I, T>(&'a self, key: I) -> Option<T>
    where
        I: TupleIndex,
        T: Decode<'a>,
    {
        self.try_get(key).expect("Error during getting tuple field")
    }

    /// Decode tuple contents as `T`.
    ///
    /// **NOTE**: Because [`Tuple`] implements [`DecodeOwned`], you can do
    /// something like this
    /// ```no_run
    /// use tarantool::tuple::{Decode, Tuple};
    /// let tuple: Tuple;
    /// # tuple = Tuple::new(&[1, 2, 3]).unwrap();
    /// let deep_copy: Tuple = tuple.decode().unwrap();
    /// let inc_ref_count: Tuple = tuple.clone();
    /// ```
    /// "Decoding" a `Tuple` into a `Tuple` will basically perform a **deep
    /// copy** of its contents, while `tuple.clone()` will just increase tuple's
    /// reference count. There's probably no use case for deep copying the
    /// tuple, because there's actully no way to move data out of it, so keep
    /// this in mind.
    #[inline]
    pub fn decode<T>(&self) -> Result<T>
    where
        T: DecodeOwned,
    {
        let raw_data = self.as_buffer();
        Decode::decode(&raw_data)
    }

    /// Deserializes tuple contents into structure of type `T`
    #[deprecated = "Use `Tuple::decode` instead"]
    pub fn as_struct<T>(&self) -> Result<T>
    where
        T: DecodeOwned,
    {
        self.decode()
    }

    #[inline]
    pub(crate) fn as_buffer(&self) -> Vec<u8> {
        let size = self.bsize();
        let mut buf = Vec::with_capacity(size);

        unsafe {
            let actual_size = ffi::box_tuple_to_buf(self.ptr.as_ptr(), buf.as_ptr() as _, size);
            buf.set_len(actual_size as usize);
        }

        buf
    }

    /// Deserializes tuple contents into structure of type `T`
    #[deprecated = "Use `Tuple::decode` instead"]
    pub fn into_struct<T>(self) -> Result<T>
    where
        T: DecodeOwned,
    {
        self.decode()
    }

    pub(crate) fn into_ptr(self) -> *mut ffi::BoxTuple {
        self.ptr.as_ptr()
    }
}

////////////////////////////////////////////////////////////////////////////////
/// TupleIndex
////////////////////////////////////////////////////////////////////////////////

/// Types implementing this trait can be used as arguments for the
/// [`Tuple::get`] method.
///
/// This is a helper trait, so you don't want to use it directly.
pub trait TupleIndex {
    fn get_field<'a, T>(self, tuple: &'a Tuple) -> Result<Option<T>>
    where
        T: Decode<'a>;
}

impl TupleIndex for u32 {
    #[inline(always)]
    fn get_field<'a, T>(self, tuple: &'a Tuple) -> Result<Option<T>>
    where
        T: Decode<'a>,
    {
        tuple.field(self)
    }
}

impl TupleIndex for &str {
    #[inline(always)]
    fn get_field<'a, T>(self, tuple: &'a Tuple) -> Result<Option<T>>
    where
        T: Decode<'a>,
    {
        use once_cell::sync::Lazy;
        use std::io::{Error as IOError, ErrorKind};
        static API: Lazy<std::result::Result<Api, dlopen::Error>> = Lazy::new(|| unsafe {
            let c_str = std::ffi::CStr::from_bytes_with_nul_unchecked;
            let lib = dlopen::symbor::Library::open_self()?;
            let err = match lib.symbol_cstr(c_str(ffi::TUPLE_FIELD_BY_PATH_NEW_API.as_bytes())) {
                Ok(api) => return Ok(Api::New(*api)),
                Err(e) => e,
            };
            if let Ok(api) = lib.symbol_cstr(c_str(ffi::TUPLE_FIELD_BY_PATH_OLD_API.as_bytes())) {
                return Ok(Api::Old(*api));
            }
            Err(err)
        });

        return match API.as_ref() {
            Ok(Api::New(api)) => unsafe {
                let field_ptr = api(tuple.ptr.as_ptr(), self.as_ptr() as _, self.len() as _, 1);
                field_value_from_ptr(field_ptr as _)
            },
            Ok(Api::Old(api)) => unsafe {
                let data_offset = tuple.ptr.as_ref().data_offset() as _;
                let data = tuple.ptr.as_ptr().cast::<c_char>().add(data_offset);
                let field_ptr = api(
                    tuple.format().inner,
                    data,
                    data as _,
                    self.as_ptr() as _,
                    self.len() as _,
                    tlua::util::hash(self),
                );
                field_value_from_ptr(field_ptr as _)
            },
            Err(e) => Err(Error::IO(IOError::new(ErrorKind::Unsupported, e))),
        };

        enum Api {
            /// Before 2.10 private api `tuple_field_raw_by_full_path`
            Old(
                extern "C" fn(
                    format: *const ffi::BoxTupleFormat,
                    tuple: *const c_char,
                    field_map: *const u32,
                    path: *const c_char,
                    path_len: u32,
                    path_hash: u32,
                ) -> *const c_char,
            ),
            /// After 2.10 public api `box_tuple_field_by_path`
            New(
                extern "C" fn(
                    tuple: *const ffi::BoxTuple,
                    path: *const c_char,
                    path_len: u32,
                    index_base: i32,
                ) -> *const c_char,
            ),
        }
    }
}

impl From<&TupleBuffer> for Tuple {
    fn from(buf: &TupleBuffer) -> Self {
        unsafe { Self::from_raw_data(buf.as_ptr() as _, buf.len() as _) }
    }
}

impl Drop for Tuple {
    fn drop(&mut self) {
        unsafe { ffi::box_tuple_unref(self.ptr.as_ptr()) };
    }
}

impl Clone for Tuple {
    fn clone(&self) -> Self {
        unsafe { ffi::box_tuple_ref(self.ptr.as_ptr()) };
        Tuple { ptr: self.ptr }
    }
}

////////////////////////////////////////////////////////////////////////////////
/// ToTupleBuffer
////////////////////////////////////////////////////////////////////////////////

/// Types implementing this trait can be converted to tarantool tuple (msgpack
/// array).
pub trait ToTupleBuffer {
    fn to_tuple_buffer(&self) -> Result<TupleBuffer> {
        let mut buf = Vec::with_capacity(128);
        self.write_tuple_data(&mut buf)?;
        TupleBuffer::try_from_vec(buf)
    }

    fn write_tuple_data(&self, w: &mut impl Write) -> Result<()>;
}

impl ToTupleBuffer for Tuple {
    #[inline]
    fn to_tuple_buffer(&self) -> Result<TupleBuffer> {
        Ok(TupleBuffer::from(self))
    }

    #[inline]
    fn write_tuple_data(&self, w: &mut impl Write) -> Result<()> {
        w.write_all(&self.as_buffer()).map_err(Into::into)
    }
}

#[allow(deprecated)]
impl<T> ToTupleBuffer for T
where
    T: ?Sized,
    T: AsTuple,
{
    #[inline]
    fn write_tuple_data(&self, w: &mut impl Write) -> Result<()> {
        self.serialize_to(w)
    }
}

////////////////////////////////////////////////////////////////////////////////
/// AsTuple
////////////////////////////////////////////////////////////////////////////////

#[deprecated = "This is a legacy trait which will be removed in future. \
Implement `Encode` for custom types instead. \
Use `ToTupleBuffer` if you need the tuple data instead."]
/// Must be implemented for types, which will be used with box access methods as data
pub trait AsTuple: Serialize {
    /// Describes how object can be converted to [Tuple](struct.Tuple.html).
    ///
    /// Has default implementation, but can be overloaded for special cases
    #[inline]
    fn serialize_as_tuple(&self) -> Result<TupleBuffer> {
        TupleBuffer::try_from(AsTuple::serialize(self)?)
    }

    #[inline]
    fn serialize(&self) -> Result<Vec<u8>> {
        // TODO(gmoshkin): tuple is required to be a message pack array only on
        // the top layer, but `to_vec` serializes all of the nested structs as
        // arrays, which is very bad. We should implement a custom serializer,
        // which does the correct thing
        let mut vec = Vec::with_capacity(128);
        self.serialize_to(&mut vec)?;
        Ok(vec)
    }

    fn serialize_to(&self, w: &mut impl Write) -> Result<()> {
        rmp_serde::encode::write(w, self).map_err(Into::into)
    }
}

////////////////////////////////////////////////////////////////////////////////
/// Encode
////////////////////////////////////////////////////////////////////////////////

/// Types implementing this trait can be serialized into a valid tarantool tuple
/// (msgpack array).
// TODO: remove this trait when `specialization` feature is stabilized
// https://github.com/rust-lang/rust/issues/31844
pub trait Encode: Serialize {
    fn encode(&self, w: &mut impl Write) -> Result<()> {
        rmp_serde::encode::write(w, self).map_err(Into::into)
    }
}

impl<'a, T> Encode for &'a T
where
    T: Encode,
{
    fn encode(&self, w: &mut impl Write) -> Result<()> {
        T::encode(*self, w)
    }
}

impl Encode for () {
    fn encode(&self, w: &mut impl Write) -> Result<()> {
        rmp_serde::encode::write(w, &Vec::<()>::new()).map_err(Into::into)
    }
}

impl<T> Encode for [T] where T: Serialize {}
impl<T> Encode for Vec<T> where T: Serialize {}

macro_rules! impl_array {
    ($($n:literal)+) => {
        $(
            #[allow(clippy::zero_prefixed_literal)]
            impl<T> Encode for [T; $n] where T: Serialize {}
        )+
    }
}

impl_array! {
    00 01 02 03 04 05 06 07 08 09 10 11 12 13 14 15
    16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32
}

macro_rules! impl_tuple {
    () => {};
    ($h:ident $($t:ident)*) => {
        impl<$h, $($t),*> Encode for ($h, $($t),*)
        where
            $h: Serialize,
            $($t: Serialize,)*
        {}

        impl_tuple! { $($t)* }
    }
}

impl_tuple! { A B C D E F G H I J K L M N O P }

/// A general purpose trait for msgpack serialization.
///
/// Writes `self` to writer supplied in `w`.
/// `struct_as_map` sets if struct fields should be serialized as tuple (`false`) or as map (`true`).
///
/// E.g. given `let foo = Foo { a: 1, b: true}`
/// `struct_as_map: false` - `foo` is identical to `(1, true)` during serialization.
/// `struct_as_map: true` - `foo` is identical to `HashMap<String, _>` with
/// keys `"a"` and `"b"` and values `1`, `true` accordingly.
///
/// It should replace `Encode` after `raw` attribute is finished
/// `_` prefix is used for disambiguation and is temporary.
///
/// # Example
/// ```
/// use tarantool_proc::Encode;
/// use tarantool::tuple::_Encode;
/// // For most use cases this trait can be derived
/// #[derive(Encode)]
/// struct Foo;
///
/// let mut buffer = vec![];
/// Foo.encode(&mut buffer, false).unwrap();
/// ```
// TODO: Remove `_` prefix and use this trait instead of previous, replace derive `Serialize` to derive `Encode`
pub trait _Encode {
    fn encode(&self, w: &mut impl Write, struct_as_map: bool) -> Result<()>;
}

impl _Encode for () {
    fn encode(&self, w: &mut impl Write, _named: bool) -> Result<()> {
        rmp::encode::write_nil(w)?;
        Ok(())
    }
}

impl<T> _Encode for [T]
where
    T: _Encode,
{
    fn encode(&self, w: &mut impl Write, named: bool) -> Result<()> {
        rmp::encode::write_array_len(w, self.len() as u32)?;
        for v in self.iter() {
            v.encode(w, named)?;
        }
        Ok(())
    }
}

impl<T> _Encode for Vec<T>
where
    T: _Encode,
{
    fn encode(&self, w: &mut impl Write, named: bool) -> Result<()> {
        self[..].as_ref().encode(w, named)
    }
}

impl<'a, T> _Encode for Cow<'a, T>
where
    T: _Encode + ToOwned + ?Sized,
{
    fn encode(&self, w: &mut impl Write, named: bool) -> Result<()> {
        self.deref().encode(w, named)
    }
}

impl _Encode for String {
    fn encode(&self, w: &mut impl Write, named: bool) -> Result<()> {
        self.as_str().encode(w, named)
    }
}

impl _Encode for str {
    fn encode(&self, w: &mut impl Write, _named: bool) -> Result<()> {
        rmp::encode::write_str(w, self).map_err(Into::into)
    }
}

impl<K, V> _Encode for BTreeMap<K, V>
where
    K: _Encode,
    V: _Encode,
{
    fn encode(&self, w: &mut impl Write, named: bool) -> Result<()> {
        rmp::encode::write_map_len(w, self.len() as u32)?;
        for (k, v) in self.iter() {
            k.encode(w, named)?;
            v.encode(w, named)?;
        }
        Ok(())
    }
}

impl _Encode for char {
    fn encode(&self, w: &mut impl Write, named: bool) -> Result<()> {
        self.to_string().encode(w, named)
    }
}

macro_rules! impl_simple_encode {
    ($(($t:ty, $f:tt, $conv:ty))+) => {
        $(
            impl _Encode for $t{
                fn encode(&self, w: &mut impl Write, _named: bool) -> Result<()> {
                    rmp::encode::$f(w, *self as $conv)?;
                    Ok(())
                }
            }
        )+
    }
}

impl_simple_encode! {
    (u8, write_uint, u64)
    (u16, write_uint, u64)
    (u32, write_uint, u64)
    (u64, write_uint, u64)
    (usize, write_uint, u64)
    (i8, write_sint, i64)
    (i16, write_sint, i64)
    (i32, write_sint, i64)
    (i64, write_sint, i64)
    (isize, write_sint, i64)
    (f32, write_f32, f32)
    (f64, write_f64, f64)
    (bool, write_bool, bool)
    (&str, write_str, &str)
}

macro_rules! _impl_array {
    ($($n:literal)+) => {
        $(
            #[allow(clippy::zero_prefixed_literal)]
            impl<T> _Encode for [T; $n] where T: _Encode {
                fn encode(&self, w: &mut impl Write, named: bool) -> Result<()> {
                    rmp::encode::write_array_len(w, $n)?;
                    for item in self {
                        item.encode(w, named)?;
                    }
                    Ok(())
                }
            }
        )+
    }
}

_impl_array! {
    00 01 02 03 04 05 06 07 08 09 10 11 12 13 14 15
    16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32
}

impl_tuple_encode!();

impl _Encode for serde_json::Value {
    fn encode(&self, w: &mut impl Write, named: bool) -> Result<()> {
        let bytes = if named {
            rmp_serde::to_vec_named(self)?
        } else {
            rmp_serde::to_vec(self)?
        };
        w.write_all(bytes.as_slice())?;
        Ok(())
    }
}

impl _Encode for serde_json::Map<String, serde_json::Value> {
    fn encode(&self, w: &mut impl Write, named: bool) -> Result<()> {
        let bytes = if named {
            rmp_serde::to_vec_named(self)?
        } else {
            rmp_serde::to_vec(self)?
        };
        w.write_all(bytes.as_slice())?;
        Ok(())
    }
}

#[allow(deprecated)]
impl<T> AsTuple for T
where
    T: ?Sized,
    T: Encode,
{
    fn serialize_to(&self, w: &mut impl Write) -> Result<()> {
        self.encode(w)
    }
}

////////////////////////////////////////////////////////////////////////////////
/// TupleBuffer
////////////////////////////////////////////////////////////////////////////////

/// Buffer containing tuple contents (MsgPack array)
#[derive(Clone, PartialEq, Eq)]
pub struct TupleBuffer(
    // TODO(gmoshkin): previously TupleBuffer would use tarantool's transaction
    // scoped memory allocator, but it would do so in a confusingly inefficient
    // and error prone manner (redundant copies and use after free).
    //
    // This doesn't mean however that there's no point in using box_txn_alloc,
    // but at this time I don't see an easy way to leave it within the current
    // state of TupleBuffer.
    //
    // There might be a use for box_txn_alloc from within
    // transaction::start_transaction, but a well thought through api is needed.
    //
    // TODO(gmoshkin): use smallvec::SmallVec instead
    Vec<u8>,
);

impl TupleBuffer {
    /// Get raw pointer to buffer.
    #[inline]
    pub fn as_ptr(&self) -> *const u8 {
        self.0.as_ptr()
    }

    /// Return the number of bytes used in memory by the tuple.
    #[inline]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// # Safety
    /// `buf` must be a valid message pack array
    #[track_caller]
    #[inline]
    pub unsafe fn from_vec_unchecked(buf: Vec<u8>) -> Self {
        Self(buf)
    }

    #[inline]
    pub fn try_from_vec(data: Vec<u8>) -> Result<Self> {
        let data = validate_msgpack(data)?;
        unsafe { Ok(Self::from_vec_unchecked(data)) }
    }
}

impl AsRef<[u8]> for TupleBuffer {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl From<TupleBuffer> for Vec<u8> {
    #[inline]
    fn from(b: TupleBuffer) -> Self {
        b.0
    }
}

impl TryFrom<Vec<u8>> for TupleBuffer {
    type Error = Error;

    #[inline]
    fn try_from(data: Vec<u8>) -> Result<Self> {
        Self::try_from_vec(data)
    }
}

impl From<Tuple> for TupleBuffer {
    #[inline]
    fn from(t: Tuple) -> Self {
        Self(t.as_buffer())
    }
}

impl From<&Tuple> for TupleBuffer {
    #[inline]
    fn from(t: &Tuple) -> Self {
        Self(t.as_buffer())
    }
}

impl Debug for TupleBuffer {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        if let Ok(v) = rmpv::Value::decode(&self.0) {
            f.debug_tuple("TupleBuffer").field(&v).finish()
        } else {
            f.debug_tuple("TupleBuffer").field(&self.0).finish()
        }
    }
}

impl ToTupleBuffer for TupleBuffer {
    #[inline]
    fn to_tuple_buffer(&self) -> Result<TupleBuffer> {
        Ok(self.clone())
    }

    #[inline]
    fn write_tuple_data(&self, w: &mut impl Write) -> Result<()> {
        w.write_all(self.as_ref()).map_err(Into::into)
    }
}

impl serde_bytes::Serialize for TupleBuffer {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serde_bytes::Serialize::serialize(&self.0, serializer)
    }
}

impl<'de> serde_bytes::Deserialize<'de> for TupleBuffer {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let tmp: Vec<u8> = serde_bytes::Deserialize::deserialize(deserializer)?;
        Self::try_from(tmp).map_err(serde::de::Error::custom)
    }
}

////////////////////////////////////////////////////////////////////////////////
/// TupleFormat
////////////////////////////////////////////////////////////////////////////////

/// Tuple format
///
/// Each Tuple has associated format (class). Default format is used to
/// create tuples which are not attach to any particular space.
pub struct TupleFormat {
    inner: *mut ffi::BoxTupleFormat,
}

impl Default for TupleFormat {
    fn default() -> Self {
        TupleFormat {
            inner: unsafe { ffi::box_tuple_format_default() },
        }
    }
}

impl Debug for TupleFormat {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        if self.inner == Self::default().inner {
            f.write_str("TupleFormat::default()")
        } else {
            f.debug_tuple("TupleFormat").field(&self.inner).finish()
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
/// TupleIterator
////////////////////////////////////////////////////////////////////////////////

/// Tuple iterator
pub struct TupleIterator {
    inner: *mut ffi::BoxTupleIterator,
}

impl Debug for TupleIterator {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_struct("TupleIterator")
            .field("position", &self.position())
            .finish()
    }
}

impl TupleIterator {
    /// Return zero-based next position in iterator.
    ///
    /// That is, this function return the field id of field that will be
    /// returned by the next call to `box_tuple_next(it)`. Returned value is zero
    /// after initialization or rewind and `box_tuple_field_count(Tuple)`
    /// after the end of iteration.
    pub fn position(&self) -> u32 {
        unsafe { ffi::box_tuple_position(self.inner) }
    }

    /// Rewind iterator to the initial position.
    pub fn rewind(&mut self) {
        unsafe { ffi::box_tuple_rewind(self.inner) }
    }

    /// Seek the Tuple iterator.
    ///
    /// Requested fieldno returned by next call to `box_tuple_next(it)`.
    ///
    /// - `fieldno` - zero-based position in MsgPack array.
    ///
    /// After call:
    /// - `box_tuple_position(it) == fieldno` if returned value is not `None`
    /// - `box_tuple_position(it) == box_tuple_field_count(Tuple)` if returned value is `None`.
    pub fn seek<'t, T>(&'t mut self, fieldno: u32) -> Result<Option<T>>
    where
        T: Decode<'t>,
    {
        unsafe { field_value_from_ptr(ffi::box_tuple_seek(self.inner, fieldno) as _) }
    }

    /// Return the next Tuple field from Tuple iterator.
    ///
    /// Returns:
    /// - `None` if `i >= box_tuple_field_count(Tuple)` or if field has a non primitive type
    /// - field value otherwise
    ///
    /// After call:
    /// - `box_tuple_position(it) == fieldno` if returned value is not `None`
    /// - `box_tuple_position(it) == box_tuple_field_count(Tuple)` if returned value is `None`.
    #[allow(clippy::should_implement_trait)]
    pub fn next<'t, T>(&'t mut self) -> Result<Option<T>>
    where
        T: Decode<'t>,
    {
        unsafe { field_value_from_ptr(ffi::box_tuple_next(self.inner) as _) }
    }

    pub fn update(&mut self) {}
}

impl Drop for TupleIterator {
    fn drop(&mut self) {
        unsafe { ffi::box_tuple_iterator_free(self.inner) }
    }
}

impl TupleIterator {}

////////////////////////////////////////////////////////////////////////////////
// FieldType
////////////////////////////////////////////////////////////////////////////////

crate::define_str_enum! {
    pub enum FieldType {
        Any       = "any",
        Unsigned  = "unsigned",
        String    = "string",
        Number    = "number",
        Double    = "double",
        Integer   = "integer",
        Boolean   = "boolean",
        Varbinary = "varbinary",
        Scalar    = "scalar",
        Decimal   = "decimal",
        Uuid      = "uuid",
        Datetime  = "datetime",
        Array     = "array",
        Map       = "map",
    }
}

impl Default for FieldType {
    fn default() -> Self {
        Self::Any
    }
}

impl From<index::FieldType> for FieldType {
    #[rustfmt::skip]
    fn from(t: index::FieldType) -> Self {
        match t {
            // "any" type is not supported as index part,
            // that's the only reason we need 2 enums.
            index::FieldType::Unsigned  => Self::Unsigned,
            index::FieldType::String    => Self::String,
            index::FieldType::Number    => Self::Number,
            index::FieldType::Double    => Self::Double,
            index::FieldType::Integer   => Self::Integer,
            index::FieldType::Boolean   => Self::Boolean,
            index::FieldType::Varbinary => Self::Varbinary,
            index::FieldType::Scalar    => Self::Scalar,
            index::FieldType::Decimal   => Self::Decimal,
            index::FieldType::Uuid      => Self::Uuid,
            index::FieldType::Datetime  => Self::Datetime,
            index::FieldType::Array     => Self::Array,
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
// KeyDef
////////////////////////////////////////////////////////////////////////////////

#[derive(Debug)]
pub struct KeyDef {
    inner: NonNull<ffi::BoxKeyDef>,
}

#[derive(Default, Debug, PartialEq, Eq, Hash)]
pub struct KeyDefPart<'a> {
    pub field_no: u32,
    pub field_type: FieldType,
    pub collation: Option<Cow<'a, CStr>>,
    pub is_nullable: bool,
    pub path: Option<Cow<'a, CStr>>,
}

impl<'a> KeyDefPart<'a> {
    fn as_tt(&self) -> ffi::box_key_part_def_t {
        let flags = if self.is_nullable {
            ffi::BoxKeyDefPartFlag::IS_NULLABLE.bits()
        } else {
            0
        };
        ffi::box_key_part_def_t {
            meat: ffi::BoxKeyDefPart {
                fieldno: self.field_no,
                field_type: self.field_type.as_cstr().as_ptr(),
                flags,
                collation: self
                    .collation
                    .as_deref()
                    .map(CStr::as_ptr)
                    .unwrap_or(null()),
                path: self.path.as_deref().map(CStr::as_ptr).unwrap_or(null()),
            },
        }
    }

    pub fn try_from_index_part(p: &'a index::Part) -> Option<Self> {
        let field_no = match p.field {
            NumOrStr::Num(field_no) => field_no,
            NumOrStr::Str(_) => return None,
        };

        let collation = p.collation.as_deref().map(|s| {
            CString::new(s)
                .expect("it's your fault if you put '\0' in collation")
                .into()
        });
        let path = p.path.as_deref().map(|s| {
            CString::new(s)
                .expect("it's your fault if you put '\0' in collation")
                .into()
        });
        Some(Self {
            field_no,
            field_type: p.r#type.map(From::from).unwrap_or(FieldType::Any),
            is_nullable: p.is_nullable.unwrap_or(false),
            collation,
            path,
        })
    }
}

impl KeyDef {
    /// Create key definition with key fields with passed typed on passed positions.
    /// May be used for tuple format creation and/or tuple comparison.
    ///
    /// - `items` - array with key field identifiers and key field types (see [FieldType](struct.FieldType.html))
    #[inline]
    pub fn new<'a>(parts: impl IntoIterator<Item = &'a KeyDefPart<'a>>) -> Result<Self> {
        let mut tt_parts = parts.into_iter().map(KeyDefPart::as_tt).collect::<Vec<_>>();
        let ptr = unsafe { ffi::box_key_def_new_v2(tt_parts.as_mut_ptr(), tt_parts.len() as _) };
        let inner = NonNull::new(ptr).ok_or_else(TarantoolError::last)?;
        Ok(KeyDef { inner })
    }

    /// Compare tuples using the key definition.
    ///
    /// - `tuple_a` - first tuple
    /// - `tuple_b` - second tuple
    ///
    /// Returns:
    /// - `Ordering::Equal`   if `key_fields(tuple_a) == key_fields(tuple_b)`
    /// - `Ordering::Less`    if `key_fields(tuple_a) < key_fields(tuple_b)`
    /// - `Ordering::Greater` if `key_fields(tuple_a) > key_fields(tuple_b)`
    pub fn compare(&self, tuple_a: &Tuple, tuple_b: &Tuple) -> Ordering {
        unsafe {
            ffi::box_tuple_compare(
                tuple_a.ptr.as_ptr(),
                tuple_b.ptr.as_ptr(),
                self.inner.as_ptr(),
            )
            .cmp(&0)
        }
    }

    /// Compare tuple with key using the key definition.
    ///
    /// - `tuple` - tuple
    /// - `key` - key with MessagePack array header
    ///
    /// Returns:
    /// - `Ordering::Equal`   if `key_fields(tuple) == parts(key)`
    /// - `Ordering::Less`    if `key_fields(tuple) < parts(key)`
    /// - `Ordering::Greater` if `key_fields(tuple) > parts(key)`
    pub fn compare_with_key<K>(&self, tuple: &Tuple, key: &K) -> Ordering
    where
        K: ToTupleBuffer + ?Sized,
    {
        let key_buf = key.to_tuple_buffer().unwrap();
        let key_buf_ptr = key_buf.as_ptr() as _;
        unsafe {
            ffi::box_tuple_compare_with_key(tuple.ptr.as_ptr(), key_buf_ptr, self.inner.as_ptr())
                .cmp(&0)
        }
    }
}

impl Drop for KeyDef {
    fn drop(&mut self) {
        unsafe { ffi::box_key_def_delete(self.inner.as_ptr()) }
    }
}

unsafe fn field_value_from_ptr<'de, T>(field_ptr: *mut u8) -> Result<Option<T>>
where
    T: Decode<'de>,
{
    if field_ptr.is_null() {
        return Ok(None);
    }

    // Theoretically this is an exploit point, which would allow reading up to
    // 2gigs of memory in case `value_ptr` happens to point to memory which
    // isn't a field of a tuple, but is a valid messagepack value
    let max_len = u32::MAX >> 1;
    let rough_slice = std::slice::from_raw_parts(field_ptr, max_len as _);
    let mut cursor = std::io::Cursor::new(rough_slice);
    let start = cursor.position() as usize;
    // There's overhead for iterating over the whole msgpack value, but this is
    // necessary.
    crate::msgpack::skip_value(&mut cursor)?;
    let value_range = start..(cursor.position() as usize);
    let rough_slice = cursor.into_inner();
    let value_slice = &rough_slice[value_range];
    Ok(Some(T::decode(value_slice)?))
}

////////////////////////////////////////////////////////////////////////////////
/// FunctionCtx
////////////////////////////////////////////////////////////////////////////////

#[repr(C)]
#[derive(Debug)]
pub struct FunctionCtx {
    inner: *mut ffi::BoxFunctionCtx,
}

impl FunctionCtx {
    /// Return a Tuple from stored procedure.
    ///
    /// Returned Tuple is automatically reference counted by Tarantool.
    ///
    /// - `tuple` - a Tuple to return
    #[inline]
    pub fn return_tuple(&self, tuple: &Tuple) -> Result<c_int> {
        let result = unsafe { ffi::box_return_tuple(self.inner, tuple.ptr.as_ptr()) };
        if result < 0 {
            Err(TarantoolError::last().into())
        } else {
            Ok(result)
        }
    }

    /// Return a value encoded as MessagePack from a stored procedure.
    ///
    /// MessagePack is not validated, for the sake of speed. It is
    /// expected to be a single encoded object. An attempt to encode
    /// and return multiple objects without wrapping them into an
    /// `MP_ARRAY` or `MP_MAP` is undefined behaviour.
    ///
    /// - `value` - value to be encoded to MessagePack
    #[inline]
    pub fn return_mp<T>(&self, value: &T) -> Result<c_int>
    where
        T: Serialize + ?Sized,
    {
        let buf = rmp_serde::to_vec_named(value)?;
        self.return_bytes(&buf)
    }

    /// Return raw bytes representing a MessagePack value from a stored
    /// procedure.
    ///
    /// MessagePack is not validated, for the sake of speed. It is
    /// expected to be a single encoded object. An attempt to encode
    /// and return multiple objects without wrapping them into an
    /// `MP_ARRAY` or `MP_MAP` is undefined behaviour.
    ///
    /// - `bytes` - raw msgpack bytes to be returned
    #[inline]
    pub fn return_bytes(&self, bytes: &[u8]) -> Result<c_int> {
        let Range { start, end } = bytes.as_ptr_range();
        let result = unsafe { ffi::box_return_mp(self.inner, start as _, end as _) };

        if result < 0 {
            Err(TarantoolError::last().into())
        } else {
            Ok(result)
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
/// FunctionArgs
////////////////////////////////////////////////////////////////////////////////

#[repr(C)]
pub struct FunctionArgs {
    pub start: *const u8,
    pub end: *const u8,
}

impl Debug for FunctionArgs {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.debug_tuple("FunctionArgs")
            .field(&Tuple::from(self))
            .finish()
    }
}

impl From<FunctionArgs> for Tuple {
    fn from(args: FunctionArgs) -> Tuple {
        Tuple::from(&args)
    }
}

impl From<&FunctionArgs> for Tuple {
    fn from(args: &FunctionArgs) -> Tuple {
        unsafe { Tuple::from_raw_data(args.start as _, args.end.offset_from(args.start) as _) }
    }
}

impl FunctionArgs {
    /// Decode the msgpack value represented by the function args.
    #[inline(always)]
    pub fn decode<'a, T>(&'a self) -> Result<T>
    where
        T: Decode<'a>,
    {
        let slice = unsafe {
            std::slice::from_raw_parts(self.start, self.end.offset_from(self.start) as _)
        };
        T::decode(slice)
    }

    /// Deserialize a tuple reprsented by the function args as `T`.
    #[inline(always)]
    #[deprecated = "Use `FunctionArgs::decode` instead."]
    pub fn as_struct<T>(&self) -> Result<T>
    where
        T: DecodeOwned,
    {
        Tuple::from(self).decode()
    }
}

/// Push MessagePack data into a session data channel - socket,
/// console or whatever is behind the session. Note, that
/// successful push does not guarantee delivery in case it was sent
/// into the network. Just like with `write()`/`send()` system calls.
pub fn session_push<T>(value: &T) -> Result<()>
where
    T: ToTupleBuffer + ?Sized,
{
    let buf = value.to_tuple_buffer().unwrap();
    let buf_ptr = buf.as_ptr() as *const c_char;
    if unsafe { ffi::box_session_push(buf_ptr, buf_ptr.add(buf.len())) } < 0 {
        Err(TarantoolError::last().into())
    } else {
        Ok(())
    }
}

#[inline(always)]
fn validate_msgpack<T>(data: T) -> Result<T>
where
    T: AsRef<[u8]> + Into<Vec<u8>>,
{
    let mut slice = data.as_ref();
    let m = rmp::decode::read_marker(&mut slice)?;
    if !matches!(m, Marker::FixArray(_) | Marker::Array16 | Marker::Array32) {
        return Err(error::Encode::InvalidMP(data.into()).into());
    }
    Ok(data)
}

impl<L> tlua::Push<L> for Tuple
where
    L: tlua::AsLua,
{
    type Err = tlua::Void;

    fn push_to_lua(&self, lua: L) -> tlua::PushResult<L, Self> {
        unsafe {
            ffi::luaT_pushtuple(tlua::AsLua::as_lua(&lua), self.ptr.as_ptr());
            Ok(tlua::PushGuard::new(lua, 1))
        }
    }
}

impl<L> tlua::PushOne<L> for Tuple where L: tlua::AsLua {}

impl<L> tlua::PushInto<L> for Tuple
where
    L: tlua::AsLua,
{
    type Err = tlua::Void;

    fn push_into_lua(self, lua: L) -> tlua::PushResult<L, Self> {
        unsafe {
            ffi::luaT_pushtuple(tlua::AsLua::as_lua(&lua), self.ptr.as_ptr());
            Ok(tlua::PushGuard::new(lua, 1))
        }
    }
}

impl<L> tlua::PushOneInto<L> for Tuple where L: tlua::AsLua {}

impl<L> tlua::LuaRead<L> for Tuple
where
    L: tlua::AsLua,
{
    fn lua_read_at_position(lua: L, index: std::num::NonZeroI32) -> tlua::ReadResult<Self, L> {
        let lua_ptr = tlua::AsLua::as_lua(&lua);
        let mut ptr = unsafe { ffi::luaT_istuple(lua_ptr, index.get()) };
        if ptr.is_null() {
            let format = TupleFormat::default();
            ptr = unsafe { ffi::luaT_tuple_new(lua_ptr, index.get(), format.inner) };
        }
        Self::try_from_ptr(ptr).ok_or_else(|| {
            let e = tlua::WrongType::info("reading tarantool tuple")
                .expected_type::<Self>()
                .actual_single_lua(&lua, index);
            (lua, e)
        })
    }
}

impl<L> tlua::LuaRead<L> for TupleBuffer
where
    L: tlua::AsLua,
{
    fn lua_read_at_position(lua: L, index: std::num::NonZeroI32) -> tlua::ReadResult<Self, L> {
        unsafe {
            let svp = ffi::box_region_used();
            let lua_ptr = tlua::AsLua::as_lua(&lua);
            let ptr = ffi::luaT_istuple(lua_ptr, index.get());
            if let Some(tuple) = Tuple::try_from_ptr(ptr) {
                return Ok(Self::from(tuple));
            }
            let mut len = 0;
            let data = ffi::luaT_tuple_encode(lua_ptr, index.get(), &mut len);
            if data.is_null() {
                let e = tlua::WrongType::info("converting Lua value to tarantool tuple")
                    .expected("msgpack array")
                    .actual(format!("error: {}", TarantoolError::last().message()));
                return Err((lua, e));
            }
            let data = std::slice::from_raw_parts(data, len);
            let data = Vec::from(data);
            ffi::box_region_truncate(svp);
            Ok(Self::from_vec_unchecked(data))
        }
    }
}

////////////////////////////////////////////////////////////////////////////////
/// Decode
////////////////////////////////////////////////////////////////////////////////

/// Types implementing this trait can be decoded from msgpack.
///
/// [`Tuple`] also implements [`Decode`] with an implementation which just
/// copies the bytes as is (and validates them).
pub trait Decode<'de>: Sized {
    fn decode(data: &'de [u8]) -> Result<Self>;
}

impl<'de, T> Decode<'de> for T
where
    T: serde::Deserialize<'de>,
{
    fn decode(data: &'de [u8]) -> Result<Self> {
        Ok(rmp_serde::from_slice(data)?)
    }
}

impl Decode<'_> for Tuple {
    fn decode(data: &[u8]) -> Result<Self> {
        Self::try_from_slice(data)
    }
}

/// Types implementing this trait can be decoded from msgpack by value.
///
/// `DecodeOwned` is to [`Decode`] what [`DeserializeOwned`] is to
/// [`Deserialize`].
///
/// [`Deserialize`]: serde::Deserialize
/// [`DeserializeOwned`]: serde::de::DeserializeOwned
pub trait DecodeOwned: for<'de> Decode<'de> {}
impl<T> DecodeOwned for T where T: for<'de> Decode<'de> {}

////////////////////////////////////////////////////////////////////////////////
/// RawBytes
////////////////////////////////////////////////////////////////////////////////

/// A wrapper type for reading raw bytes from a tuple.
///
/// Can be used to read a field of a tuple as raw bytes:
/// ```no_run
/// use tarantool::{tuple::Tuple, tuple::RawBytes};
/// let tuple = Tuple::new(&(1, (2, 3, 4), 5)).unwrap();
/// let second_field: &RawBytes = tuple.get(1).unwrap();
/// assert_eq!(&**second_field, &[0x93, 2, 3, 4]);
/// ```
///
/// This type also implements [`ToTupleBuffer`] such that `to_tuple_buffer`
/// returns `Ok` only if the underlying bytes represent a valid tuple (msgpack
/// array).
#[derive(Debug)]
#[repr(transparent)]
pub struct RawBytes(pub [u8]);

impl RawBytes {
    /// Convert a slice of bytes `data` into a `&RawBytes`.
    #[inline(always)]
    pub fn new(data: &[u8]) -> &Self {
        // SAFETY: this is safe, because `RawBytes` has `#[repr(transparent)]`
        unsafe { &*(data as *const [u8] as *const RawBytes) }
    }
}

impl<'a> From<&'a [u8]> for &'a RawBytes {
    #[inline(always)]
    fn from(data: &'a [u8]) -> Self {
        RawBytes::new(data)
    }
}

impl<'de> Decode<'de> for &'de RawBytes {
    #[inline(always)]
    fn decode(data: &'de [u8]) -> Result<Self> {
        // TODO: only read msgpack bytes
        Ok(RawBytes::new(data))
    }
}

impl ToTupleBuffer for RawBytes {
    #[inline(always)]
    fn write_tuple_data(&self, w: &mut impl Write) -> Result<()> {
        let data = &**self;
        validate_msgpack(data)?;
        w.write_all(data).map_err(Into::into)
    }
}

impl std::ops::Deref for RawBytes {
    type Target = [u8];
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

////////////////////////////////////////////////////////////////////////////////
/// RawByteBuf
////////////////////////////////////////////////////////////////////////////////

/// A wrapper type for reading raw bytes from a tuple.
///
/// The difference between [`TupleBuffer`] and `RawByteBuf` is that the former
/// can only contain a valid tarantool tuple (msgpack array), while the latter
/// can contain any sequence of bytes.
///
/// This type also implements [`ToTupleBuffer`] such that `to_tuple_buffer`
/// returns `Ok` only if the underlying bytes represent a valid tuple.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct RawByteBuf(pub Vec<u8>);

impl serde_bytes::Serialize for RawByteBuf {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serde_bytes::Serialize::serialize(&self.0, serializer)
    }
}

impl<'de> serde_bytes::Deserialize<'de> for RawByteBuf {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        serde_bytes::Deserialize::deserialize(deserializer).map(Self)
    }
}

impl From<Vec<u8>> for RawByteBuf {
    #[inline(always)]
    fn from(b: Vec<u8>) -> Self {
        Self(b)
    }
}

impl Decode<'_> for RawByteBuf {
    #[inline(always)]
    fn decode(data: &[u8]) -> Result<Self> {
        // TODO: only read msgpack bytes
        Ok(Self(data.into()))
    }
}

impl ToTupleBuffer for RawByteBuf {
    #[inline(always)]
    fn write_tuple_data(&self, w: &mut impl Write) -> Result<()> {
        let data = self.as_slice();
        validate_msgpack(data)?;
        w.write_all(data).map_err(Into::into)
    }
}

impl std::ops::Deref for RawByteBuf {
    type Target = Vec<u8>;
    #[inline(always)]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::ops::DerefMut for RawByteBuf {
    #[inline(always)]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

#[cfg(feature = "picodata")]
mod picodata {
    use crate::tuple::{Tuple, TupleFormat};
    use crate::Result;
    use std::ffi::CStr;
    use std::io::{Cursor, Write};
    use std::marker::PhantomData;
    use std::os::raw::c_char;

    impl Tuple {
        /// Returns messagepack encoded tuple with named fields (messagepack map).
        ///
        /// Returned map has only numeric keys if tuple has default tuple format (see [TupleFormat](struct.TupleFormat.html)),
        /// for example when tuple dont belongs to any space. If tuple has greater fields than named
        /// fields in tuple format - then additional fields are  presents in the map with numeric keys.
        ///
        /// This function is useful if there is no information about tuple fields in program runtime.
        pub fn as_named_buffer(&self) -> Result<Vec<u8>> {
            let format = self.format();
            let buff = self.as_buffer();

            let field_count = self.len();
            let mut named_buffer = Vec::with_capacity(buff.len());

            let mut cursor = Cursor::new(&buff);

            rmp::encode::write_map_len(&mut named_buffer, field_count)?;
            rmp::decode::read_array_len(&mut cursor)?;
            format.names().try_for_each(|field_name| -> Result<()> {
                let value_start = cursor.position() as usize;
                crate::msgpack::skip_value(&mut cursor)?;
                let value_end = cursor.position() as usize;

                rmp::encode::write_str(&mut named_buffer, field_name)?;
                Ok(named_buffer.write_all(&buff[value_start..value_end])?)
            })?;

            for i in 0..field_count - format.name_count() {
                let value_start = cursor.position() as usize;
                crate::msgpack::skip_value(&mut cursor)?;
                let value_end = cursor.position() as usize;

                rmp::encode::write_u32(&mut named_buffer, i)?;
                named_buffer.write_all(&buff[value_start..value_end])?;
            }

            Ok(named_buffer)
        }
    }

    impl TupleFormat {
        /// Return tuple field names count.
        pub fn name_count(&self) -> u32 {
            unsafe { (*(*self.inner).dict).name_count }
        }

        /// Return tuple field names.
        pub fn names(&self) -> impl Iterator<Item = &str> {
            let ptr = unsafe { (*(*self.inner).dict).names };
            NameIterator {
                ptr,
                len: self.name_count() as usize,
                pos: 0,
                _p: PhantomData,
            }
        }
    }

    pub(crate) struct NameIterator<'a> {
        ptr: *const *const c_char,
        len: usize,
        pos: usize,
        _p: PhantomData<&'a ()>,
    }

    impl<'a> Iterator for NameIterator<'a> {
        type Item = &'a str;

        #[track_caller]
        fn next(&mut self) -> Option<Self::Item> {
            if self.pos >= self.len {
                return None;
            }

            unsafe {
                let str_ptr = self.ptr.add(self.pos);
                self.pos += 1;

                Some(
                    CStr::from_ptr(*str_ptr)
                        .to_str()
                        .expect("invalid utf-8 string"),
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::BTreeMap, io::Cursor};

    use super::_Encode;
    use rmp::decode::Bytes;
    use serde::Deserialize;
    use tarantool_proc::Encode;

    #[track_caller]
    fn assert_map(bytes: &[u8]) {
        let marker = rmp::decode::read_marker(&mut Bytes::new(bytes)).unwrap();
        assert!(matches!(
            dbg!(marker),
            rmp::Marker::Map16 | rmp::Marker::Map32 | rmp::Marker::FixMap(_)
        ))
    }

    #[track_caller]
    fn assert_array(bytes: &[u8]) {
        let marker = rmp::decode::read_marker(&mut Bytes::new(bytes)).unwrap();
        assert!(matches!(
            dbg!(marker),
            rmp::Marker::Array16 | rmp::Marker::Array32 | rmp::Marker::FixArray(_)
        ))
    }

    #[test]
    fn encode_struct() {
        #[derive(Clone, Encode, Deserialize, PartialEq, Debug)]
        #[encode(tarantool = "crate")]
        struct Test1 {
            b: u32,
        }
        #[derive(Clone, Encode, Deserialize, PartialEq, Debug)]
        #[encode(tarantool = "crate")]
        struct Test {
            a: usize,
            b: String,
            c: Test1,
        }
        let mut bytes = vec![];
        let mut bytes_named = vec![];
        let original = Test {
            a: 1,
            b: "abc".to_owned(),
            c: Test1 { b: 0 },
        };
        original.encode(&mut bytes, false).unwrap();
        assert_array(&bytes);
        original.encode(&mut bytes_named, true).unwrap();
        assert_map(&bytes_named);
        let decoded: Test = rmp_serde::from_slice(bytes.as_slice()).unwrap();
        assert_eq!(original, decoded);
        let decoded_named: Test = rmp_serde::from_slice(bytes_named.as_slice()).unwrap();
        assert_eq!(original, decoded_named);
    }

    #[test]
    fn encode_tuple_struct() {
        #[derive(Clone, Encode, Deserialize, PartialEq, Debug)]
        #[encode(tarantool = "crate")]
        struct Test(u32, bool);
        let mut bytes = vec![];
        let mut bytes_named = vec![];
        let original = Test(0, true);
        original.encode(&mut bytes, false).unwrap();
        assert_array(&bytes);
        original.encode(&mut bytes_named, true).unwrap();
        assert_eq!(
            bytes, bytes_named,
            "tuple structs are always encoded as arrays"
        );
        let decoded: Test = rmp_serde::from_slice(bytes.as_slice()).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn encode_unit_struct() {
        #[derive(Clone, Encode, Deserialize, PartialEq, Debug)]
        #[encode(tarantool = "crate")]
        struct Test;
        let mut bytes = vec![];
        let original = Test;
        original.encode(&mut bytes, false).unwrap();
        let decoded: Test = rmp_serde::from_slice(bytes.as_slice()).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn encode_enum() {
        #[derive(Clone, Encode, Deserialize, PartialEq, Debug)]
        #[encode(tarantool = "crate")]
        enum Foo {
            BarUnit,
            BarTuple1(bool),
            BarTupleN((), (), ()),
            BarStruct1 { bar: bool },
            BarStructN { bar1: (), bar2: (), bar3: () },
        }
        let mut bytes = vec![];
        let original = Foo::BarUnit;
        original.encode(&mut bytes, false).unwrap();
        let decoded: Foo = rmp_serde::from_slice(bytes.as_slice()).unwrap();
        assert_eq!(original, decoded);

        let mut bytes = vec![];
        let original = Foo::BarTuple1(true);
        original.encode(&mut bytes, false).unwrap();
        let decoded: Foo = rmp_serde::from_slice(bytes.as_slice()).unwrap();
        assert_eq!(original, decoded);

        let mut bytes = vec![];
        let original = Foo::BarTupleN((), (), ());
        original.encode(&mut bytes, false).unwrap();
        let decoded: Foo = rmp_serde::from_slice(bytes.as_slice()).unwrap();
        assert_eq!(original, decoded);

        let mut bytes = vec![];
        let mut bytes_named = vec![];
        let original = Foo::BarStruct1 { bar: false };
        original.encode(&mut bytes, false).unwrap();
        original.encode(&mut bytes_named, true).unwrap();
        let decoded: Foo = rmp_serde::from_slice(bytes.as_slice()).unwrap();
        assert_eq!(original, decoded);
        let decoded_named: Foo = rmp_serde::from_slice(bytes.as_slice()).unwrap();
        assert_eq!(original, decoded_named);

        let mut bytes = vec![];
        let mut bytes_named = vec![];
        let original = Foo::BarStructN {
            bar1: (),
            bar2: (),
            bar3: (),
        };
        original.encode(&mut bytes, false).unwrap();
        original.encode(&mut bytes_named, true).unwrap();
        let decoded: Foo = rmp_serde::from_slice(bytes.as_slice()).unwrap();
        assert_eq!(original, decoded);
        let decoded_named: Foo = rmp_serde::from_slice(bytes.as_slice()).unwrap();
        assert_eq!(original, decoded_named);
    }

    #[test]
    fn encode_named_with_raw_ident() {
        #[derive(Clone, Encode, Deserialize, PartialEq, Debug)]
        #[encode(tarantool = "crate")]
        struct Test {
            r#fn: u32,
        }
        let mut bytes = vec![];
        let original = Test { r#fn: 1 };
        original.encode(&mut bytes, true).unwrap();
        let mut bytes = Cursor::new(bytes);
        let marker = rmp::decode::read_marker(&mut bytes).unwrap();
        assert!(matches!(marker, rmp::Marker::FixMap(1)));
        let mut key_bytes = vec![0; 10];
        let key = rmp::decode::read_str(&mut bytes, key_bytes.as_mut_slice()).unwrap();
        assert_eq!(key, "fn");
    }

    #[test]
    fn encode_vec() {
        let mut bytes = vec![];
        let original = vec![1u32];
        original.encode(&mut bytes, false).unwrap();
        let decoded: Vec<u32> = rmp_serde::from_slice(bytes.as_slice()).unwrap();
        assert_eq!(original, decoded);

        let mut bytes = vec![];
        let original = vec![(), (), (), (), ()];
        original.encode(&mut bytes, false).unwrap();
        let decoded: Vec<()> = rmp_serde::from_slice(bytes.as_slice()).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn encode_map() {
        let mut bytes = vec![];
        let mut original = BTreeMap::new();
        original.insert(1, "abc".to_string());
        original.insert(2, "def".to_string());
        original.encode(&mut bytes, false).unwrap();
        let decoded: BTreeMap<u32, String> = rmp_serde::from_slice(bytes.as_slice()).unwrap();
        assert_eq!(original, decoded);
    }
}

#[cfg(feature = "internal_test")]
mod test {
    use super::*;

    #[crate::test(tarantool = "crate")]
    fn tuple_buffer_from_lua() {
        let svp = unsafe { ffi::box_region_used() };

        let lua = crate::lua_state();
        let t: TupleBuffer = lua
            .eval("return { 3, 'foo', { true, box.NIL, false } }")
            .unwrap();

        #[derive(::serde::Deserialize, PartialEq, Eq, Debug)]
        struct S {
            i: i32,
            s: String,
            t: [Option<bool>; 3],
        }

        let s = S::decode(t.as_ref()).unwrap();
        assert_eq!(
            s,
            S {
                i: 3,
                s: "foo".into(),
                t: [Some(true), None, Some(false)]
            }
        );

        let res = lua.eval::<TupleBuffer>("return 1, 2, 3");
        assert_eq!(
            res.unwrap_err().to_string(),
            "failed converting Lua value to tarantool tuple: msgpack array expected, got error: A tuple or a table expected, got number
    while reading value(s) returned by Lua: tarantool::tuple::TupleBuffer expected, got (number, number, number)"
        );

        let res = lua.eval::<TupleBuffer>("return { 1, 2, foo = 'bar' }");
        assert_eq!(
            res.unwrap_err().to_string(),
            "failed converting Lua value to tarantool tuple: msgpack array expected, got error: Tuple/Key must be MsgPack array
    while reading value(s) returned by Lua: tarantool::tuple::TupleBuffer expected, got table"
        );

        let res = lua.eval::<TupleBuffer>(
            "ffi = require 'ffi';
            local cdata = ffi.new('struct { int x; int y; }', { x = -1, y = 2 })
            return { 1, cdata }",
        );
        assert_eq!(
            res.unwrap_err().to_string(),
            "failed converting Lua value to tarantool tuple: msgpack array expected, got error: unsupported Lua type 'cdata'
    while reading value(s) returned by Lua: tarantool::tuple::TupleBuffer expected, got table"
        );

        assert_eq!(svp, unsafe { ffi::box_region_used() });
    }
}
