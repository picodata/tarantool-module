#![cfg(any(feature = "picodata", doc))]

use crate::error::TarantoolError;
use crate::ffi;
use crate::ffi::sql::{Bind, ObufWrapper, Port, PortSql, SqlStatement};
use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashMap;
use std::ffi::CStr;
use std::io::Read;
use std::os::raw::c_char;
use std::str;

fn decode_params<IN>(bind_params: &IN) -> crate::Result<(*const Bind, u32)>
where
    IN: Serialize,
{
    let mut bind_ptr = std::ptr::null::<Bind>();
    if std::mem::size_of::<IN>() == 0 {
        return Ok((bind_ptr, 0));
    }
    let params = rmp_serde::to_vec(bind_params)?;
    let bind_cnt = unsafe {
        ffi::sql::sql_bind_list_decode(
            params.as_ptr() as *const c_char,
            &mut bind_ptr as *mut *const Bind,
        )
    };
    if bind_cnt < 0 {
        return Err(TarantoolError::last().into());
    }

    Ok((bind_ptr, bind_cnt as u32))
}

/// Executes SQL query without storing prepared statement in the instance cache
/// and returns a wrapper over the raw msgpack bytes.
pub fn prepare_and_execute_raw<IN>(
    query: &str,
    bind_params: &IN,
    vdbe_max_steps: u64,
) -> crate::Result<impl Read>
where
    IN: Serialize,
{
    let mut port = Port::zeroed();

    let (bind_ptr, bind_cnt) = decode_params(bind_params)?;
    let execute_result = unsafe {
        ffi::sql::sql_prepare_and_execute_ext(
            query.as_ptr() as *const c_char,
            query.len() as i32,
            bind_ptr,
            bind_cnt,
            vdbe_max_steps,
            &port as *const Port,
        )
    };

    if execute_result < 0 {
        // Tarantool has already called `port_destroy()` and has possibly
        // trashed `vtab` pointer. We need to reset it to avoid UB.
        port.vtab = std::ptr::null();
        return Err(TarantoolError::last().into());
    }

    let buf = ObufWrapper::new(1024);

    unsafe {
        ((*port.vtab).dump_msgpack)(&port as *const Port, buf.obuf());
    };
    Ok(buf)
}

/// Create new SQL prepared statement.
/// query - SQL query.
pub fn prepare(query: &str) -> crate::Result<Statement> {
    let port = Port::zeroed();

    if unsafe {
        ffi::sql::sql_prepare(
            query.as_ptr() as *const c_char,
            query.len() as u32,
            &port as *const Port,
        )
    } < 0
    {
        return Err(TarantoolError::last().into());
    }

    let sql_port = &port as *const Port as *const PortSql;
    let stmt = unsafe { (*sql_port).sql_stmt };
    let stmt_id =
        unsafe { ffi::sql::sql_stmt_calculate_id(query.as_ptr() as *const c_char, query.len()) };

    Ok(Statement {
        inner: stmt,
        id: stmt_id,
    })
}

/// SQL prepared statement.
pub struct Statement {
    inner: *const SqlStatement,
    id: u32,
}

impl Statement {
    /// Returns original query.
    pub fn source(&self) -> Result<&str, std::str::Utf8Error> {
        unsafe {
            let query = ffi::sql::sql_stmt_query_str(self.inner);
            CStr::from_ptr(query)
        }
        .to_str()
    }

    /// Returns internal Tarantool id of the prepared statement.
    pub fn id(&self) -> u32 {
        self.id
    }

    /// Executes prepared statement and returns a wrapper over the raw msgpack bytes.
    pub fn execute_raw<IN>(&self, bind_params: &IN, vdbe_max_steps: u64) -> crate::Result<impl Read>
    where
        IN: Serialize,
    {
        let mut port = Port::zeroed();
        let (bind_ptr, bind_cnt) = decode_params(bind_params)?;
        let execute_result = unsafe {
            ffi::sql::sql_execute_prepared_ext(
                self.id,
                bind_ptr,
                bind_cnt,
                vdbe_max_steps,
                &port as *const Port,
            )
        };

        if execute_result < 0 {
            // Tarantool has already called `port_destroy()` and has possibly
            // trashed `vtab` pointer. We need to reset it to avoid UB.
            port.vtab = std::ptr::null();
            return Err(TarantoolError::last().into());
        }

        let buf = ObufWrapper::new(1024);

        unsafe {
            ((*port.vtab).dump_msgpack)(&port as *const Port, buf.obuf());
        };
        Ok(buf)
    }

    /// Executes a *returning data* prepared statement with binding variables.
    ///
    /// Example:
    /// ```no_run
    /// #[cfg(feature = "picodata")]
    /// {
    ///     use tarantool::sql;
    ///
    ///     let stmt = sql::prepare("SELECT * FROM S WHERE ID > ?").unwrap();
    ///     let result: Vec<(u8, String)> = stmt.execute(&(100,), 0).unwrap();
    ///     println!("SQL query result: {:?}", result);
    /// }
    /// ```
    pub fn execute<IN, OUT>(&self, bind_params: &IN, vdbe_max_steps: u64) -> crate::Result<OUT>
    where
        IN: Serialize,
        OUT: DeserializeOwned,
    {
        let buf = self.execute_raw(bind_params, vdbe_max_steps)?;
        let mut map = rmp_serde::decode::from_read::<_, HashMap<u32, rmpv::Value>>(buf)?;
        let data = map.remove(&ffi::sql::IPROTO_DATA).ok_or_else(|| {
            rmp_serde::decode::Error::Syntax("Invalid execution result format".to_string())
        })?;
        let values = rmpv::ext::from_value::<OUT>(data)
            .map_err(|e| rmp_serde::decode::Error::Syntax(e.to_string()))?;

        Ok(values)
    }
}

impl Default for Statement {
    fn default() -> Self {
        Statement {
            inner: std::ptr::null(),
            id: 0,
        }
    }
}

impl Drop for Statement {
    fn drop(&mut self) {
        unsafe {
            ffi::sql::sql_unprepare(self.id);
        }
    }
}
