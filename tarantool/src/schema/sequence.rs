use crate::error::Error;
use crate::schema;
use crate::space::{Space, SystemSpace};
use crate::tuple::Tuple;

/// Drop existing sequence.
///
/// - `seq_id` - ID of existing space.
pub fn drop_sequence(seq_id: u32) -> Result<(), Error> {
    schema::revoke_object_privileges("sequence", seq_id)?;

    let sys_sequence: Space = SystemSpace::Sequence.into();
    sys_sequence.delete(&Tuple::encode_rmp(&(seq_id,))?)?;

    let sys_sequence_data: Space = SystemSpace::SequenceData.into();
    sys_sequence_data.delete(&Tuple::encode_rmp(&(seq_id,))?)?;

    Ok(())
}
