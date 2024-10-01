//! Box: sequences
use crate::error::{Error, TarantoolError};
use crate::ffi::tarantool as ffi;
use crate::space::{Space, SystemSpace};

/// A sequence is a generator of ordered integer values.
pub struct Sequence {
    seq_id: u32,
}

impl Sequence {
    /// Find sequence by name.
    pub fn find(name: &str) -> Result<Option<Self>, Error> {
        let space: Space = SystemSpace::Sequence.into();
        let name_idx = space.index("name").unwrap();

        Ok(match name_idx.get(&(name,))? {
            None => None,
            Some(row_tuple) => Some(Sequence {
                seq_id: row_tuple.field(0)?.unwrap(),
            }),
        })
    }

    /// Get sequence id.
    pub fn id(&self) -> u32 {
        self.seq_id
    }

    #[allow(clippy::should_implement_trait)]
    /// Generate the next value and return it.
    ///
    /// The generation algorithm is simple:
    /// - If this is the first time, then return the `start` value.
    /// - If the previous value plus the `increment` value is less than the `minimum` value or greater than the
    /// `maximum` value, that is "overflow", so either raise an error (if `cycle = false`) or return the `maximum` value
    /// (if `cycle = true` and `step < 0`) or return the `minimum` value (if `cycle = true` and `step > 0`).
    ///
    /// If there was no error, then save the returned result, it is now the "previous value".
    pub fn next(&mut self) -> Result<i64, Error> {
        let mut result: i64 = 0;
        if unsafe { ffi::box_sequence_next(self.seq_id, &mut result) } < 0 {
            Err(TarantoolError::last().into())
        } else {
            Ok(result)
        }
    }

    /// Set the "previous value" to `new_value`.
    ///
    /// This function requires a "write" privilege on the sequence.
    pub fn set(&mut self, new_value: i64) -> Result<(), Error> {
        if unsafe { ffi::box_sequence_set(self.seq_id, new_value) } < 0 {
            Err(TarantoolError::last().into())
        } else {
            Ok(())
        }
    }

    /// Set the sequence back to its original state.
    ///
    /// The effect is that a subsequent [next](#method.next) will return the start value.
    /// This function requires a "write" privilege on the sequence.
    pub fn reset(&mut self) -> Result<(), Error> {
        if unsafe { ffi::box_sequence_reset(self.seq_id) } < 0 {
            Err(TarantoolError::last().into())
        } else {
            Ok(())
        }
    }
}
