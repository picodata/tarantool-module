use std::ffi::CStr;
use std::io;
use std::os::raw::{c_int, c_schar};

use serde::Deserialize;
use tester::{
    run_tests_console, ColorConfig, Options, OutputFormat, RunIgnored, ShouldPanic, TestDesc,
    TestDescAndFn, TestFn, TestName, TestOpts, TestType,
};

use tarantool::error::Error;
use tarantool::ffi::lua as ffi_lua;
use tarantool::index::{IndexFieldType, IndexOptions, IndexPart, IndexType};
use tarantool::space::{Space, SpaceCreateOptions, SpaceFieldFormat, SpaceFieldType};

mod bench_bulk_insert;
mod common;
mod test_box;
mod test_coio;
mod test_error;
mod test_fiber;
mod test_latch;
mod test_log;
mod test_net_box;
mod test_raft;
mod test_session;
mod test_transaction;
mod test_tuple;

macro_rules! tests {
    ($($func_name:expr,)*) => {
        vec![
            $(TestDescAndFn{
                desc: TestDesc{
                    name: TestName::StaticTestName(stringify!($func_name)),
                    ignore: false,
                    should_panic: ShouldPanic::No,
                    allow_fail: false,
                    test_type: TestType::IntegrationTest
                },
                testfn: TestFn::StaticTestFn($func_name)
            },)*
        ]
    }
}

#[derive(Clone, Copy, Default, Deserialize)]
struct TestConfig {
    bench: bool,
}

fn create_test_spaces() -> Result<(), Error> {
    // space.test_s1
    let mut test_s1_opts = SpaceCreateOptions::default();
    test_s1_opts.format = Some(vec![
        SpaceFieldFormat::new("id", SpaceFieldType::Unsigned),
        SpaceFieldFormat::new("text", SpaceFieldType::String),
    ]);
    let mut test_s1 = match Space::create("test_s1", &test_s1_opts) {
        Ok(s) => s,
        Err(e) => return Err(e),
    };

    // space.test_s1.index.primary
    let mut test_s1_idx_primary = IndexOptions::default();
    test_s1_idx_primary.index_type = Some(IndexType::Tree);
    test_s1_idx_primary.parts = Some(vec![IndexPart::new(1, IndexFieldType::Unsigned)]);
    test_s1.create_index("primary", &test_s1_idx_primary)?;

    // space.test_s2
    let mut test_s2_opts = SpaceCreateOptions::default();
    test_s2_opts.format = Some(vec![
        SpaceFieldFormat::new("id", SpaceFieldType::Unsigned),
        SpaceFieldFormat::new("key", SpaceFieldType::String),
        SpaceFieldFormat::new("value", SpaceFieldType::String),
        SpaceFieldFormat::new("a", SpaceFieldType::Integer),
        SpaceFieldFormat::new("b", SpaceFieldType::Integer),
    ]);
    let mut test_s2 = match Space::create("test_s2", &test_s1_opts) {
        Ok(s) => s,
        Err(e) => return Err(e),
    };

    // space.test_s2.index.primary
    let mut test_s2_idx_primary = IndexOptions::default();
    test_s2_idx_primary.index_type = Some(IndexType::Tree);
    test_s2_idx_primary.parts = Some(vec![IndexPart::new(1, IndexFieldType::Unsigned)]);
    test_s2.create_index("primary", &test_s2_idx_primary)?;

    // space.test_s2.index.idx_1
    let mut test_s2_idx_sec_1 = IndexOptions::default();
    test_s2_idx_sec_1.index_type = Some(IndexType::Hash);
    test_s2_idx_sec_1.parts = Some(vec![IndexPart::new(2, IndexFieldType::String)]);
    test_s2.create_index("idx_1", &test_s2_idx_sec_1)?;

    // space.test_s2.index.idx_2
    let mut test_s2_idx_sec_2 = IndexOptions::default();
    test_s2_idx_sec_2.index_type = Some(IndexType::Tree);
    test_s2_idx_sec_2.parts = Some(vec![
        IndexPart::new(1, IndexFieldType::Unsigned),
        IndexPart::new(4, IndexFieldType::Integer),
        IndexPart::new(5, IndexFieldType::Integer),
    ]);
    test_s2.create_index("idx_2", &test_s2_idx_sec_2)?;

    // space.test_s2.index.idx_3
    let mut test_s2_idx_sec_3 = IndexOptions::default();
    test_s2_idx_sec_3.index_type = Some(IndexType::Tree);
    test_s2_idx_sec_3.unique = Some(false);
    test_s2_idx_sec_3.parts = Some(vec![IndexPart::new(4, IndexFieldType::Integer)]);
    test_s2.create_index("idx_3", &test_s2_idx_sec_3)?;

    // Insert test data into space.test_s2
    for i in 1..21 {
        let rec = common::S2Record {
            id: i,
            key: format!("key_{}", i),
            value: format!("value_{}", i),
            a: (i as i32) % 5,
            b: (i as f32 / 5.0).floor() as i32,
        };
        test_s2.insert(&rec)?;
    }

    Ok(())
}

fn drop_test_spaces() -> Result<(), Error> {
    let space_names = vec!["test_s1".to_string(), "test_s2".to_string()];

    for s in space_names.iter() {
        if let Some(space) = Space::find(s) {
            space.drop()?;
        }
    }

    Ok(())
}

fn run_tests(cfg: TestConfig) -> Result<bool, io::Error> {
    run_tests_console(
        &TestOpts {
            list: false,
            filter: None,
            filter_exact: false,
            force_run_in_process: false,
            exclude_should_panic: false,
            run_ignored: RunIgnored::No,
            run_tests: true,
            bench_benchmarks: cfg.bench,
            logfile: None,
            nocapture: false,
            color: ColorConfig::AutoColor,
            format: OutputFormat::Pretty,
            test_threads: Some(1),
            skip: vec![],
            time_options: None,
            options: Options::new(),
        },
        if cfg.bench {
            vec![TestDescAndFn {
                desc: TestDesc {
                    name: TestName::StaticTestName("bench_bulk_insert"),
                    ignore: false,
                    should_panic: ShouldPanic::No,
                    allow_fail: false,
                    test_type: TestType::Unknown,
                },
                testfn: TestFn::DynBenchFn(Box::new(bench_bulk_insert::BulkInsertBenchmark {
                    test_size: 64,
                    num_fibers: 256,
                    num_rows: 1000,
                })),
            }]
        } else {
            tests![
                test_fiber::test_fiber_new,
                test_fiber::test_fiber_new_with_attr,
                test_fiber::test_fiber_arg,
                test_fiber::test_fiber_cancel,
                test_fiber::test_fiber_wake,
                test_fiber::test_fiber_cond_signal,
                test_fiber::test_fiber_cond_broadcast,
                test_fiber::test_fiber_cond_timeout,
                test_box::test_space_get_by_name,
                test_box::test_space_get_system,
                test_box::test_index_get_by_name,
                test_box::test_box_insert,
                test_box::test_box_replace,
                test_box::test_box_delete,
                test_box::test_box_update,
                test_box::test_box_upsert,
                test_box::test_box_truncate,
                test_box::test_box_get,
                test_box::test_box_select,
                test_box::test_box_select_composite_key,
                test_box::test_box_len,
                test_box::test_box_random,
                test_box::test_box_min_max,
                test_box::test_box_count,
                test_box::test_box_extract_key,
                test_box::test_box_sequence_get_by_name,
                test_box::test_box_sequence_iterate,
                test_box::test_box_sequence_set,
                test_box::test_space_create_opt_default,
                test_box::test_space_create_opt_if_not_exists,
                test_box::test_space_create_id_increment,
                test_box::test_space_create_opt_user,
                test_box::test_space_create_opt_id,
                test_box::test_space_drop,
                test_box::test_index_create_drop,
                test_tuple::test_tuple_new_from_struct,
                test_tuple::test_tuple_field_count,
                test_tuple::test_tuple_size,
                test_tuple::test_tuple_into_struct,
                test_tuple::test_tuple_clone,
                test_tuple::test_tuple_iterator,
                test_tuple::test_tuple_iterator_seek_rewind,
                test_tuple::test_tuple_get_format,
                test_tuple::test_tuple_get_field,
                test_tuple::test_tuple_compare,
                test_tuple::test_tuple_compare_with_key,
                test_error::test_error_last,
                test_coio::test_coio_accept,
                test_coio::test_coio_read_write,
                test_coio::test_coio_call,
                test_transaction::test_transaction_commit,
                test_transaction::test_transaction_rollback,
                test_log::test_log,
                test_latch::test_latch_lock,
                test_latch::test_latch_try_lock,
                test_net_box::test_immediate_close,
                test_net_box::test_ping,
                test_net_box::test_ping_timeout,
                test_net_box::test_ping_concurrent,
                test_net_box::test_call,
                test_net_box::test_call_timeout,
                test_net_box::test_eval,
                test_net_box::test_connection_error,
                test_net_box::test_is_connected,
                test_net_box::test_schema_sync,
                test_net_box::test_select,
                test_net_box::test_get,
                test_net_box::test_insert,
                test_net_box::test_replace,
                test_net_box::test_update,
                test_net_box::test_upsert,
                test_net_box::test_delete,
                test_net_box::test_cancel_recv,
                test_net_box::test_triggers_connect,
                test_net_box::test_triggers_reject,
                test_net_box::test_triggers_schema_sync,
                test_session::test_uid,
                test_session::test_euid,
                test_raft::test_bootstrap_solo,
                test_raft::test_bootstrap_2n,
            ]
        },
    )
}

pub extern "C" fn start(l: *mut ffi_lua::lua_State) -> c_int {
    let cfg_src = unsafe { ffi_lua::lua_tostring(l, 1) };
    let cfg = if !cfg_src.is_null() {
        let cfg_src = unsafe { CStr::from_ptr(cfg_src) }.to_str().unwrap();
        serde_json::from_str::<TestConfig>(cfg_src).unwrap()
    } else {
        TestConfig::default()
    };

    if let Err(e) = create_test_spaces() {
        unsafe { ffi_lua::luaL_error(l, e.to_string().as_ptr() as *const c_schar) };
        return 0;
    }

    let is_success = match run_tests(cfg) {
        Ok(success) => success,
        Err(e) => {
            // Clenaup without handling error to avoid code mess.
            drop_test_spaces();
            unsafe { ffi_lua::luaL_error(l, e.to_string().as_ptr() as *const c_schar) };
            return 0;
        }
    };

    if let Err(e) = drop_test_spaces() {
        unsafe { ffi_lua::luaL_error(l, e.to_string().as_ptr() as *const c_schar) };
        return 0;
    }

    unsafe { ffi_lua::lua_pushinteger(l, (!is_success) as isize) };
    1
}

#[no_mangle]
pub extern "C" fn luaopen_libtarantool_module_test_runner(l: *mut ffi_lua::lua_State) -> c_int {
    unsafe { ffi_lua::lua_pushcfunction(l, Some(start)) };
    1
}
