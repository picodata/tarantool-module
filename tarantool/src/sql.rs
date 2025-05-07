#![cfg(any(feature = "picodata", doc))]

use crate::error::TarantoolError;
use crate::ffi;
use crate::ffi::sql::{ObufWrapper, PortC};
use serde::Serialize;
use std::borrow::Cow;
use std::io::Read;
use std::os::raw::c_char;
use std::str;

const MP_EMPTY_ARRAY: &[u8] = &[0x90];

/// Returns the hash, used as the statement ID, generated from the SQL query text.
pub fn calculate_hash(sql: &str) -> u32 {
    unsafe { ffi::sql::sql_stmt_calculate_id(sql.as_ptr() as *const c_char, sql.len()) }
}

/// Executes an SQL query without storing the prepared statement in the instance
/// cache and returns a wrapper around the raw msgpack bytes.
pub fn prepare_and_execute_raw<IN>(
    query: &str,
    bind_params: &IN,
    vdbe_max_steps: u64,
) -> crate::Result<impl Read>
where
    IN: Serialize,
{
    let mut buf = ObufWrapper::new(1024);
    let mut param_data = Cow::from(MP_EMPTY_ARRAY);
    if std::mem::size_of::<IN>() != 0 {
        param_data = Cow::from(rmp_serde::to_vec(bind_params)?);
        debug_assert!(crate::msgpack::skip_value(&mut std::io::Cursor::new(&param_data)).is_ok());
    }
    let param_ptr = param_data.as_ptr() as *const u8;
    let execute_result = unsafe {
        ffi::sql::sql_prepare_and_execute_ext(
            query.as_ptr() as *const u8,
            query.len() as i32,
            param_ptr,
            vdbe_max_steps,
            buf.obuf(),
        )
    };
    if execute_result < 0 {
        return Err(TarantoolError::last().into());
    }
    Ok(buf)
}

pub fn sql_execute_into_port<IN>(
    query: &str,
    bind_params: &IN,
    vdbe_max_steps: u64,
    port: &mut PortC,
) -> crate::Result<()>
where
    IN: Serialize,
{
    let mut param_data = Cow::from(MP_EMPTY_ARRAY);
    if std::mem::size_of::<IN>() != 0 {
        param_data = Cow::from(rmp_serde::to_vec(bind_params)?);
        debug_assert!(crate::msgpack::skip_value(&mut std::io::Cursor::new(&param_data)).is_ok());
    }
    let param_ptr = param_data.as_ptr() as *const u8;
    let execute_result = unsafe {
        ffi::sql::sql_execute_into_port(
            query.as_ptr() as *const u8,
            query.len() as i32,
            param_ptr,
            vdbe_max_steps,
            port.as_mut_ptr(),
        )
    };
    if execute_result < 0 {
        return Err(TarantoolError::last().into());
    }
    Ok(())
}

/// Creates new SQL prepared statement and stores it in the session.
/// query - SQL query.
///
/// Keep in mind that a prepared statement is stored in the instance cache as
/// long as its reference counter is non-zero. The counter increases only when
/// a new statement is added to a session. Repeatedly calling prepare on an
/// already existing statement within the same session does not increase the
/// instance cache counter. However, calling prepare on the statement in a
/// different session without the statement does increase the counter.
pub fn prepare(query: String) -> crate::Result<Statement> {
    let mut stmt_id: u32 = 0;
    let mut session_id: u64 = 0;

    if unsafe {
        ffi::sql::sql_prepare_ext(
            query.as_ptr(),
            query.len() as u32,
            &mut stmt_id as *mut u32,
            &mut session_id as *mut u64,
        )
    } < 0
    {
        return Err(TarantoolError::last().into());
    }

    Ok(Statement {
        query,
        stmt_id,
        session_id,
    })
}

/// Removes SQL prepared statement from the session.
///
/// The statement is removed from the session, and its reference counter in
/// the instance cache is decremented. If the counter reaches zero, the
/// statement is removed from the instance cache.
pub fn unprepare(stmt: Statement) -> crate::Result<()> {
    if unsafe { ffi::sql::sql_unprepare_ext(stmt.id(), stmt.session_id()) } < 0 {
        return Err(TarantoolError::last().into());
    }
    Ok(())
}

/// SQL prepared statement.
#[derive(Default, Debug)]
pub struct Statement {
    query: String,
    stmt_id: u32,
    session_id: u64,
}

impl Statement {
    /// Returns original query.
    pub fn source(&self) -> &str {
        self.query.as_str()
    }

    /// Returns the statement ID generated from the SQL query text.
    pub fn id(&self) -> u32 {
        self.stmt_id
    }

    /// Returns the session ID.
    pub fn session_id(&self) -> u64 {
        self.session_id
    }

    /// Executes prepared statement and returns a wrapper over the raw msgpack bytes.
    pub fn execute_raw<IN>(&self, bind_params: &IN, vdbe_max_steps: u64) -> crate::Result<impl Read>
    where
        IN: Serialize,
    {
        let mut buf = ObufWrapper::new(1024);
        let mut param_data = Cow::from(MP_EMPTY_ARRAY);
        if std::mem::size_of::<IN>() != 0 {
            param_data = Cow::from(rmp_serde::to_vec(bind_params)?);
            debug_assert!(
                crate::msgpack::skip_value(&mut std::io::Cursor::new(&param_data)).is_ok()
            );
        }
        let param_ptr = param_data.as_ptr() as *const u8;
        let execute_result = unsafe {
            ffi::sql::sql_execute_prepared_ext(self.id(), param_ptr, vdbe_max_steps, buf.obuf())
        };

        if execute_result < 0 {
            return Err(TarantoolError::last().into());
        }
        Ok(buf)
    }

    pub fn execute_into_port<IN>(
        &self,
        bind_params: &IN,
        vdbe_max_steps: u64,
        port: &mut PortC,
    ) -> crate::Result<()>
    where
        IN: Serialize,
    {
        let mut param_data = Cow::from(MP_EMPTY_ARRAY);
        if std::mem::size_of::<IN>() != 0 {
            param_data = Cow::from(rmp_serde::to_vec(bind_params)?);
            debug_assert!(
                crate::msgpack::skip_value(&mut std::io::Cursor::new(&param_data)).is_ok()
            );
        }
        let param_ptr = param_data.as_ptr() as *const u8;
        let execute_result = unsafe {
            ffi::sql::stmt_execute_into_port(
                self.id(),
                param_ptr,
                vdbe_max_steps,
                port.as_mut_ptr(),
            )
        };

        if execute_result < 0 {
            return Err(TarantoolError::last().into());
        }
        Ok(())
    }
}
