//! Box: indices
//!
//! The `index` submodule provides access for index definitions and index keys.
//! They provide an API for ordered iteration over tuples.
//! This API is a direct binding to corresponding methods of index objects of type `box.index` in the storage engine.
//!
//! See also:
//! - [Indexes](https://www.tarantool.io/en/doc/latest/book/box/data_model/#indexes)
//! - [Lua reference: Submodule box.index](https://www.tarantool.io/en/doc/latest/reference/reference_lua/box_index/)
use std::os::raw::c_char;
use std::ptr::null_mut;
use std::mem::MaybeUninit;

use num_traits::ToPrimitive;

use crate::error::{Error, TarantoolError};
use crate::ffi::tarantool as ffi;
use crate::tuple::{AsTuple, Tuple, TupleBuffer};
use crate::tuple_from_box_api;

/// An index is a group of key values and pointers.
#[derive(Debug, Clone)]
pub struct Index {
    space_id: u32,
    index_id: u32,
}

/// Controls how to iterate over tuples in an index.
/// Different index types support different iterator types.
/// For example, one can start iteration from a particular value
/// (request key) and then retrieve all tuples where keys are
/// greater or equal (= `GE`) to this key.
///
/// If iterator type is not supported by the selected index type,
/// iterator constructor must fail with `ER_UNSUPPORTED`. To be
/// selectable for primary key, an index must support at least
/// `Eq` and `GE` types.
///
/// `None` value of request key corresponds to the first or last
/// key in the index, depending on iteration direction.
/// (first key for `GE` and `GT` types, and last key for `LE` and `LT`).
/// Therefore, to iterate over all tuples in an index, one can
/// use `GE` or `LE` iteration types with start key equal to `None`.
/// For `EQ`, the key must not be `None`.
#[repr(i32)]
#[derive(Debug, Copy, Clone, ToPrimitive)]
pub enum IteratorType {
    /// key == x ASC order
    Eq = 0,

    /// key == x DESC order
    Req = 1,

    /// all tuples
    All = 2,

    /// key <  x
    LT = 3,

    /// key <= x
    LE = 4,

    /// key >= x
    GE = 5,

    /// key >  x
    GT = 6,

    /// all bits from x are set in key
    BitsAllSet = 7,

    /// at least one x's bit is set
    BitsAnySet = 8,

    /// all bits are not set
    BitsAllNotSet = 9,

    /// key overlaps x
    Overlaps = 10,

    /// tuples in distance ascending order from specified point
    Neighbor = 11,
}

/// List of options for new or updated index.
///
/// For details see [space_object:create_index - options](https://www.tarantool.io/en/doc/latest/reference/reference_lua/box_space/create_index/).
#[derive(Serialize)]
pub struct IndexOptions {
    pub index_type: Option<IndexType>,
    pub id: Option<u32>,
    pub unique: Option<bool>,
    pub if_not_exists: Option<bool>,
    pub parts: Option<Vec<IndexPart>>,
    pub dimension: Option<u32>,
    pub distance: Option<RtreeIndexDistanceType>,
    pub bloom_fpr: Option<f32>,
    pub page_size: Option<u32>,
    pub range_size: Option<u32>,
    pub run_count_per_level: Option<u32>,
    pub run_size_ratio: Option<f32>,
    pub sequence: Option<IndexSequenceOption>,
    pub func: Option<String>,
    // Only for Tarantool >= 2.6
    // pub hint: Option<bool>,
}

impl Default for IndexOptions {
    fn default() -> Self {
        IndexOptions {
            index_type: Some(IndexType::Tree),
            id: None,
            unique: Some(true),
            if_not_exists: Some(false),
            parts: Some(vec![IndexPart {
                field_index: 1,
                field_type: IndexFieldType::Unsigned,
                collation: None,
                is_nullable: None,
                path: None,
            }]),
            dimension: Some(2),
            distance: Some(RtreeIndexDistanceType::Euclid),
            bloom_fpr: Some(0.05),
            page_size: Some(8 * 1024),
            range_size: None,
            run_count_per_level: Some(2),
            run_size_ratio: Some(3.5),
            sequence: None,
            func: None,
            // Only for Tarantool >= 2.6
            // hint: Some(true),
        }
    }
}

/// Sequence option for new or updated index.
///
/// For details see [specifying a sequence in create_index](https://www.tarantool.io/en/doc/latest/reference/reference_lua/box_schema_sequence/create_index/#box-schema-sequence-create-index).
#[derive(Serialize)]
pub enum IndexSequenceOption {
    SeqId {
        seq_id: u32,
        field_index: Option<u32>,
    },
    SeqName {
        seq_name: String,
        field_index: Option<u32>,
    },
    True,
    Empty,
}

/// Type of index.
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all="lowercase")]
pub enum IndexType {
    Hash,
    Tree,
    Bitset,
    Rtree,
}

/// Type of index part.
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all="lowercase")]
pub enum IndexFieldType {
    Unsigned,
    String,
    Integer,
    Number,
    Double,
    Decimal,
    Boolean,
    Varbinary,
    Uuid,
    Array,
    Scalar,
}

/// Index part.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct IndexPart {
    pub field_index: u32,
    pub field_type: IndexFieldType,
    pub collation: Option<String>,
    pub is_nullable: Option<bool>,
    pub path: Option<String>,
}

impl IndexPart {
    pub fn new(fi: u32, ft: IndexFieldType) -> Self {
        return IndexPart {
            field_index: fi,
            field_type: ft,
            collation: None,
            is_nullable: None,
            path: None,
        };
    }
}

/// Type of distance for retree index.
#[derive(Copy, Clone, Debug, Serialize)]
pub enum RtreeIndexDistanceType {
    Euclid,
    Manhattan,
}

impl Index {
    pub(crate) fn new(space_id: u32, index_id: u32) -> Self {
        Index { space_id, index_id }
    }

    // Drops index.
    #[cfg(feature = "schema")]
    pub fn drop(&self) -> Result<(), Error> {
        crate::schema::index::drop_index(self.space_id, self.index_id)
    }

    /// Get a tuple from index by the key.
    ///
    /// Please note that this function works much more faster than [select](#method.select)
    ///
    /// - `key` - encoded key in MsgPack Array format (`[part1, part2, ...]`).
    ///
    /// Returns a tuple or `None` if index is empty
    pub fn get<K>(&self, key: &K) -> Result<Option<Tuple>, Error>
    where
        K: AsTuple,
    {
        let key_buf = key.serialize_as_tuple().unwrap();
        let key_buf_ptr = key_buf.as_ptr() as *const c_char;
        tuple_from_box_api!(
            ffi::box_index_get[
                self.space_id,
                self.index_id,
                key_buf_ptr,
                key_buf_ptr.offset(key_buf.len() as isize),
                @out
            ]
        )
    }

    /// Allocate and initialize iterator for index.
    ///
    /// This is an alternative to [space.select()](../space/struct.Space.html#method.select) which goes via a particular
    /// index and can make use of additional parameter that specify the iterator type.
    ///
    /// - `type` - iterator type
    /// - `key` - encoded key in MsgPack Array format (`[part1, part2, ...]`).
    pub fn select<K>(&self, iterator_type: IteratorType, key: &K) -> Result<IndexIterator, Error>
    where
        K: AsTuple,
    {
        let key_buf = key.serialize_as_tuple().unwrap();
        let key_buf_ptr = key_buf.as_ptr() as *const c_char;

        let ptr = unsafe {
            ffi::box_index_iterator(
                self.space_id,
                self.index_id,
                iterator_type.to_i32().unwrap(),
                key_buf_ptr,
                key_buf_ptr.offset(key_buf.len() as isize),
            )
        };

        if ptr.is_null() {
            return Err(TarantoolError::last().into());
        }

        Ok(IndexIterator {
            ptr,
            _key_data: key_buf,
        })
    }

    /// Delete a tuple identified by a key.
    ///
    /// Same as [space.delete()](../space/struct.Space.html#method.delete), but key is searched in this index instead
    /// of in the primary-key index. This index ought to be unique.
    ///
    /// - `key` - encoded key in MsgPack Array format (`[part1, part2, ...]`).
    ///
    /// Returns the deleted tuple
    pub fn delete<K>(&mut self, key: &K) -> Result<Option<Tuple>, Error>
    where
        K: AsTuple,
    {
        let key_buf = key.serialize_as_tuple().unwrap();
        let key_buf_ptr = key_buf.as_ptr() as *const c_char;
        tuple_from_box_api!(
            ffi::box_delete[
                self.space_id,
                self.index_id,
                key_buf_ptr,
                key_buf_ptr.offset(key_buf.len() as isize),
                @out
            ]
        )
    }

    /// Update a tuple.
    ///
    /// Same as [space.update()](../space/struct.Space.html#method.update), but key is searched in this index instead
    /// of primary key. This index ought to be unique.
    ///
    /// - `key` - encoded key in MsgPack Array format (`[part1, part2, ...]`).
    /// - `ops` - encoded operations in MsgPack array format, e.g. `[['=', field_id, value], ['!', 2, 'xxx']]`
    ///
    /// Returns a new tuple.
    ///
    /// See also: [index.upsert()](#method.upsert)
    pub fn update<K, Op>(&mut self, key: &K, ops: &Vec<Op>) -> Result<Option<Tuple>, Error>
    where
        K: AsTuple,
        Op: AsTuple,
    {
        let key_buf = key.serialize_as_tuple().unwrap();
        let key_buf_ptr = key_buf.as_ptr() as *const c_char;
        let ops_buf = ops.serialize_as_tuple().unwrap();
        let ops_buf_ptr = ops_buf.as_ptr() as *const c_char;
        tuple_from_box_api!(
            ffi::box_update[
                self.space_id,
                self.index_id,
                key_buf_ptr,
                key_buf_ptr.offset(key_buf.len() as isize),
                ops_buf_ptr,
                ops_buf_ptr.offset(ops_buf.len() as isize),
                0,
                @out
            ]
        )
    }

    /// Execute an UPSERT request.
    ///
    /// Will try to insert tuple. Update if already exists.
    ///
    /// - `value` - encoded tuple in MsgPack Array format (`[field1, field2, ...]`)
    /// - `ops` - encoded operations in MsgPack array format, e.g. `[['=', field_id, value], ['!', 2, 'xxx']]`
    ///
    /// Returns a new tuple.
    ///
    /// See also: [index.update()](#method.update)
    pub fn upsert<T, Op>(&mut self, value: &T, ops: &Vec<Op>) -> Result<Option<Tuple>, Error>
    where
        T: AsTuple,
        Op: AsTuple,
    {
        let value_buf = value.serialize_as_tuple().unwrap();
        let value_buf_ptr = value_buf.as_ptr() as *const c_char;
        let ops_buf = ops.serialize_as_tuple().unwrap();
        let ops_buf_ptr = ops_buf.as_ptr() as *const c_char;
        tuple_from_box_api!(
            ffi::box_upsert[
                self.space_id,
                self.index_id,
                value_buf_ptr,
                value_buf_ptr.offset(value_buf.len() as isize),
                ops_buf_ptr,
                ops_buf_ptr.offset(ops_buf.len() as isize),
                0,
                @out
            ]
        )
    }

    /// Return the number of elements in the index.
    pub fn len(&self) -> Result<usize, Error> {
        let result = unsafe { ffi::box_index_len(self.space_id, self.index_id) };

        if result < 0 {
            Err(TarantoolError::last().into())
        } else {
            Ok(result as usize)
        }
    }

    /// Return the number of bytes used in memory by the index.
    pub fn bsize(&self) -> Result<usize, Error> {
        let result = unsafe { ffi::box_index_bsize(self.space_id, self.index_id) };

        if result < 0 {
            Err(TarantoolError::last().into())
        } else {
            Ok(result as usize)
        }
    }

    /// Return a random tuple from the index (useful for statistical analysis).
    ///
    /// - `rnd` - random seed
    pub fn random(&self, seed: u32) -> Result<Option<Tuple>, Error> {
        tuple_from_box_api!(
            ffi::box_index_random[
                self.space_id,
                self.index_id,
                seed,
                @out
            ]
        )
    }

    /// Return a first (minimal) tuple matched the provided key.
    ///
    /// - `key` - encoded key in MsgPack Array format (`[part1, part2, ...]`).
    ///
    /// Returns a tuple or `None` if index is empty
    pub fn min<K>(&self, key: &K) -> Result<Option<Tuple>, Error>
    where
        K: AsTuple,
    {
        let key_buf = key.serialize_as_tuple().unwrap();
        let key_buf_ptr = key_buf.as_ptr() as *const c_char;
        tuple_from_box_api!(
            ffi::box_index_min[
                self.space_id,
                self.index_id,
                key_buf_ptr,
                key_buf_ptr.offset(key_buf.len() as isize),
                @out
            ]
        )
    }

    /// Return a last (maximal) tuple matched the provided key.
    ///
    /// - `key` - encoded key in MsgPack Array format (`[part1, part2, ...]`).
    ///
    /// Returns a tuple or `None` if index is empty
    pub fn max<K>(&self, key: &K) -> Result<Option<Tuple>, Error>
    where
        K: AsTuple,
    {
        let key_buf = key.serialize_as_tuple().unwrap();
        let key_buf_ptr = key_buf.as_ptr() as *const c_char;
        tuple_from_box_api!(
            ffi::box_index_max[
                self.space_id,
                self.index_id,
                key_buf_ptr,
                key_buf_ptr.offset(key_buf.len() as isize),
                @out
            ]
        )
    }

    /// Count the number of tuple matched the provided key.
    ///
    /// - `type` - iterator type
    /// - `key` - encoded key in MsgPack Array format (`[part1, part2, ...]`).
    pub fn count<K>(&self, iterator_type: IteratorType, key: &K) -> Result<usize, Error>
    where
        K: AsTuple,
    {
        let key_buf = key.serialize_as_tuple().unwrap();
        let key_buf_ptr = key_buf.as_ptr() as *const c_char;

        let result = unsafe {
            ffi::box_index_count(
                self.space_id,
                self.index_id,
                iterator_type.to_i32().unwrap(),
                key_buf_ptr,
                key_buf_ptr.offset(key_buf.len() as isize),
            )
        };

        if result < 0 {
            Err(TarantoolError::last().into())
        } else {
            Ok(result as usize)
        }
    }

    /// Extract key from tuple according to key definition of given
    /// index. Returned buffer is allocated on `box_txn_alloc()` with
    /// this key.
    ///
    /// - `tuple` - tuple from which need to extract key.
    pub fn extract_key(&self, tuple: Tuple) -> Tuple {
        unsafe {
            let mut result_size = MaybeUninit::uninit();
            let result_ptr = ffi::box_tuple_extract_key(
                tuple.into_ptr(),
                self.space_id,
                self.index_id,
                result_size.as_mut_ptr(),
            );
            Tuple::from_raw_data(result_ptr, result_size.assume_init())
        }
    }
}

/// Index iterator. Can be used with `for` statement.
pub struct IndexIterator {
    ptr: *mut ffi::BoxIterator,
    _key_data: TupleBuffer,
}

impl Iterator for IndexIterator {
    type Item = Tuple;

    fn next(&mut self) -> Option<Self::Item> {
        let mut result_ptr = null_mut();
        if unsafe { ffi::box_iterator_next(self.ptr, &mut result_ptr) } < 0 {
            return None;
        }
        Tuple::try_from_ptr(result_ptr)
    }
}

impl Drop for IndexIterator {
    fn drop(&mut self) {
        unsafe { ffi::box_iterator_free(self.ptr) };
    }
}
