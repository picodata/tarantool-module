#![allow(unknown_lints)]
#![allow(clippy::approx_constant)]
#![allow(clippy::disallowed_names)]
#![allow(clippy::bool_assert_comparison)]
#![allow(clippy::upper_case_acronyms)]
#![allow(clippy::needless_borrow)]
#![allow(clippy::needless_borrows_for_generic_args)]
#![allow(clippy::redundant_pattern_matching)]
#![allow(clippy::useless_vec)]
#![allow(clippy::get_first)]
#![allow(clippy::unused_unit)]
use std::io;

use serde::Deserialize;
use tester::{
    run_tests_console, ColorConfig, Options, OutputFormat, RunIgnored, ShouldPanic, TestDesc,
    TestDescAndFn, TestFn, TestName, TestOpts, TestType,
};

use tarantool::error::Error;
use tarantool::index::IndexType;
use tarantool::space::{Field, FieldType, Space};

mod access_control;
mod auth;
mod r#box;
mod coio;
mod common;
mod datetime;
mod define_str_enum;
mod enums;
mod fiber;
mod latch;
mod net_box;
mod proc;
mod session;
mod sql;
mod test_attr;
mod tlua;
mod transaction;
mod tuple;
mod tuple_picodata;
mod uuid;

macro_rules! tests {
    (@should_panic should_panic) => { Some(ShouldPanic::Yes) };
    (@should_panic $( $_:tt )?)  => { None };
    ($([should_panic_if: $should_panic_if:expr])? $( $( #[ $attr:tt ] )? $func_name:path,)*) => {{
        #[allow(unused_mut, unused_variables)]
        let mut should_panic = ShouldPanic::No;
        $(
            if $should_panic_if {
                should_panic = ShouldPanic::Yes
            }
        )?
        vec![
            $(TestDescAndFn{
                desc: TestDesc{
                    name: TestName::StaticTestName(stringify!($func_name)),
                    ignore: false,
                    should_panic: tests!(@should_panic $($attr)?).unwrap_or(should_panic),
                    allow_fail: false,
                    test_type: TestType::IntegrationTest
                },
                testfn: TestFn::StaticTestFn($func_name)
            },)*
        ]
    }}
}

#[derive(Clone, Deserialize)]
struct TestConfig {
    filter: Option<String>,
}

fn create_test_spaces() -> Result<(), Error> {
    // space.test_s1
    let test_s1 = Space::builder("test_s1")
        .field(Field::unsigned("id"))
        .field(Field::string("text"))
        .create()?;

    // space.test_s1.index.primary
    test_s1
        .index_builder("primary")
        .index_type(IndexType::Tree)
        .part(1)
        .create()?;

    // space.test_s2
    let test_s2 = Space::builder("test_s2")
        .format([
            ("id", FieldType::Unsigned),
            ("key", FieldType::String),
            ("value", FieldType::String),
            ("a", FieldType::Integer),
            ("b", FieldType::Integer),
        ])
        .create()?;

    // space.test_s2.index.primary
    test_s2
        .index_builder("primary")
        .index_type(IndexType::Tree)
        .part(1)
        .create()?;

    // space.test_s2.index.idx_1
    test_s2
        .index_builder("idx_1")
        .index_type(IndexType::Hash)
        .part(2)
        .create()?;

    // space.test_s2.index.idx_2
    test_s2
        .index_builder("idx_2")
        .index_type(IndexType::Tree)
        .parts(["id", "a", "b"])
        .create()?;

    // space.test_s2.index.idx_3
    test_s2
        .index_builder("idx_3")
        .index_type(IndexType::Tree)
        .unique(false)
        .part("a")
        .create()?;

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

    // space.with_array
    let with_array = Space::builder("with_array")
        .field(Field::unsigned("id"))
        .field(Field::array("array"))
        .create()?;

    // space.with_array.index.pk
    with_array.index_builder("pk").part("id").create()?;

    with_array.insert(&(1, vec![1, 2, 3]))?;
    with_array.insert(&(2, ("foo", ("bar", [69, 420]), 3.14)))?;

    Ok(())
}

fn run_tests(cfg: TestConfig) -> Result<bool, io::Error> {
    run_tests_console(
        &TestOpts {
            list: false,
            filter: cfg.filter,
            filter_exact: false,
            force_run_in_process: false,
            exclude_should_panic: false,
            run_ignored: RunIgnored::No,
            run_tests: true,
            bench_benchmarks: false,
            logfile: None,
            nocapture: false,
            color: ColorConfig::AutoColor,
            format: OutputFormat::Pretty,
            test_threads: Some(1),
            skip: vec![],
            time_options: None,
            options: Options::new(),
        },
        {
            let mut tests = tarantool::test::collect_tester();

            println!("internal tests count: {}", tests.len());
            for test in &tests {
                println!("{}", test.desc.name);
            }

            tests.append(&mut tarantool::tlua::test::collect());
            tests.append(&mut tests![
                define_str_enum::basic,
                define_str_enum::coerce_from_str,
                define_str_enum::deserialize_from_owned,
                tlua::lua_functions::basic,
                tlua::lua_functions::two_functions_at_the_same_time,
                tlua::lua_functions::args,
                tlua::lua_functions::args_in_order,
                tlua::lua_functions::syntax_error,
                tlua::lua_functions::execution_error,
                tlua::lua_functions::check_types,
                tlua::lua_functions::call_and_read_table,
                tlua::lua_functions::table_as_args,
                tlua::lua_functions::table_method_call,
                tlua::lua_functions::lua_function_returns_function,
                tlua::lua_functions::error_location,
                tlua::lua_functions::either_or,
                tlua::lua_functions::multiple_return_values,
                tlua::lua_functions::multiple_return_values_fail,
                tlua::lua_functions::execute_from_reader_errors_if_cant_read,
                tlua::lua_functions::from_function_call_error,
                tlua::lua_functions::non_string_error,
                tlua::lua_functions::push_function,
                tlua::lua_functions::push_iter_no_err,
                tlua::lua_functions::eval_with,
                tlua::lua_tables::iterable,
                tlua::lua_tables::iterable_multipletimes,
                tlua::lua_tables::get_set,
                tlua::lua_tables::get_nil,
                tlua::lua_tables::table_over_table,
                tlua::lua_tables::get_or_create_metatable,
                tlua::lua_tables::complex_anonymous_table_metatable,
                tlua::lua_tables::empty_array,
                tlua::lua_tables::by_value,
                tlua::lua_tables::registry,
                tlua::lua_tables::registry_metatable,
                #[should_panic]
                tlua::lua_tables::table_iter_stack_invariance,
                tlua::lua_tables::iter_table_of_tables,
                tlua::functions_write::simple_function,
                tlua::functions_write::one_argument,
                tlua::functions_write::two_arguments,
                tlua::functions_write::wrong_arguments_types,
                tlua::functions_write::return_result,
                tlua::functions_write::closures,
                tlua::functions_write::closures_lifetime,
                tlua::functions_write::closures_extern_access,
                tlua::functions_write::closures_drop_env,
                tlua::functions_write::global_data,
                tlua::functions_write::push_callback_by_ref,
                tlua::functions_write::closures_must_be_static,
                tlua::functions_write::pcall,
                tlua::functions_write::error,
                tlua::functions_write::optional_params,
                tlua::functions_write::lua_function_as_argument,
                tlua::any::read_numbers,
                tlua::any::read_hashable_numbers,
                tlua::any::read_strings,
                tlua::any::read_hashable_strings,
                tlua::any::read_booleans,
                tlua::any::read_hashable_booleans,
                tlua::any::read_tables,
                tlua::any::read_hashable_tables,
                tlua::any::push_numbers,
                tlua::any::push_hashable_numbers,
                tlua::any::push_strings,
                tlua::any::push_hashable_strings,
                tlua::any::push_booleans,
                tlua::any::push_hashable_booleans,
                tlua::any::push_nil,
                tlua::any::push_hashable_nil,
                tlua::any::non_utf_8_string,
                tlua::misc::print,
                tlua::misc::json,
                tlua::misc::dump_stack,
                tlua::misc::dump_stack_raw,
                tlua::misc::error_during_push_tuple,
                tlua::misc::hash,
                tlua::object::callable_builtin,
                tlua::object::callable_ffi,
                tlua::object::callable_meta,
                tlua::object::indexable_builtin,
                tlua::object::indexable_ffi,
                tlua::object::indexable_meta,
                tlua::object::cannot_get_mutltiple_values,
                tlua::object::indexable_rw_builtin,
                tlua::object::indexable_rw_meta,
                tlua::object::anything_to_msgpack,
                tlua::userdata::readwrite,
                tlua::userdata::destructor_called,
                tlua::userdata::type_check,
                tlua::userdata::metatables,
                tlua::userdata::multiple_userdata,
                tlua::rust_tables::push_array,
                tlua::rust_tables::push_vec,
                tlua::rust_tables::push_hashmap,
                tlua::rust_tables::push_hashset,
                tlua::rust_tables::globals_table,
                tlua::rust_tables::read_array,
                tlua::rust_tables::read_array_partial,
                tlua::rust_tables::read_vec,
                tlua::rust_tables::read_hashmap,
                tlua::rust_tables::read_wrong_type_fail,
                tlua::rust_tables::derive_struct_push,
                tlua::rust_tables::derive_struct_lua_read,
                tlua::rust_tables::derive_enum_push,
                tlua::rust_tables::derive_push_into,
                tlua::rust_tables::derive_enum_lua_read,
                tlua::rust_tables::derive_generic_struct_push,
                tlua::rust_tables::derive_generic_struct_lua_read,
                tlua::rust_tables::derive_generic_enum_push,
                tlua::rust_tables::derive_generic_enum_lua_read,
                tlua::rust_tables::derive_generic_push_into,
                tlua::rust_tables::enum_variants_order_matters,
                tlua::rust_tables::struct_of_enums_vs_enum_of_structs,
                tlua::rust_tables::derive_unit_structs_lua_read,
                tlua::rust_tables::derive_unit_structs_push,
                tlua::rust_tables::push_custom_iter,
                tlua::rust_tables::error_during_push_iter,
                tlua::rust_tables::push_custom_collection,
                tlua::rust_tables::table_from_iter,
                tlua::rust_tables::push_struct_of_nones,
                tlua::rust_tables::derive_tuple_structs,
                tlua::values::read_i32s,
                tlua::values::write_i32s,
                tlua::values::int64,
                tlua::values::cdata_numbers,
                tlua::values::push_cdata,
                tlua::values::cdata_on_stack,
            ]);
            tests.append(&mut tests![
                [should_panic_if: cfg!(debug_assertions)]
                tlua::values::as_cdata_wrong_size,
            ]);
            tests.append(&mut tests![
                tlua::values::readwrite_floats,
                tlua::values::readwrite_bools,
                tlua::values::readwrite_strings,
                tlua::values::i32_to_string,
                tlua::values::string_to_i32,
                tlua::values::string_on_lua,
                tlua::values::push_opt,
                tlua::values::read_nil,
                tlua::values::typename,
                tlua::values::tuple_as_table,
                fiber::old::fiber_new,
                fiber::old::fiber_new_with_attr,
                fiber::old::fiber_arg,
                fiber::old::fiber_cancel,
                fiber::old::fiber_wake,
                fiber::old::fiber_cond_signal,
                fiber::old::fiber_cond_broadcast,
                fiber::old::fiber_cond_timeout,
                fiber::immediate,
                fiber::immediate_with_attrs,
                fiber::multiple_immediate,
                fiber::unit_immediate,
                fiber::unit_immediate_with_attrs,
                fiber::multiple_unit_immediate,
                fiber::deferred,
                fiber::deferred_with_attrs,
                fiber::multiple_deferred,
                fiber::unit_deferred,
                fiber::unit_deferred_with_attrs,
                fiber::multiple_unit_deferred,
                fiber::deferred_doesnt_yield,
                fiber::immediate_yields,
                fiber::start_error,
                fiber::require_error,
                #[should_panic]
                fiber::start_dont_join,
                #[should_panic]
                fiber::start_proc_dont_join,
                #[should_panic]
                fiber::defer_dont_join,
                #[should_panic]
                fiber::defer_proc_dont_join,
                fiber::immediate_with_cond,
                fiber::deferred_with_cond,
                fiber::lifetime,
                fiber::r#yield,
                fiber::yield_canceled,
            ]);

            tests.append(&mut tests![
                [should_panic_if: unsafe { !tarantool::ffi::has_fiber_set_ctx() }]
                fiber::deferred_ffi,
            ]);

            tests.append(&mut tests![
                [should_panic_if: !tarantool::ffi::has_fiber_channel()]
                fiber::lua_thread,

                fiber::channel::send_self,
                fiber::channel::send_full,
                fiber::channel::recv_empty,
                fiber::channel::drop_sender,
                fiber::channel::dont_drop_msg,
                fiber::channel::unbuffered,
                fiber::channel::one_v_two,
                fiber::channel::two_v_one,
                fiber::channel::drop_msgs,
                fiber::channel::circle,
                fiber::channel::as_mutex,
                fiber::channel::iter,
                fiber::channel::into_iter,
                fiber::channel::try_iter,
                fiber::channel::demo,
                fiber::channel::drop_rx,
                fiber::channel::into_clones,
                fiber::channel::cannot_send_ref,

                fiber::mutex::advanced,
            ]);

            tests.append(&mut tests![
                fiber::mutex::simple,
                fiber::mutex::try_lock,
                fiber::mutex::debug,
                r#box::space_get_by_name,
                r#box::space_get_by_name_cached,
                r#box::space_cache_invalidated,
                r#box::space_get_system,
                r#box::index_get_by_name,
                r#box::index_get_by_name_cached,
                r#box::index_cache_invalidated,
                r#box::insert,
                r#box::replace,
                r#box::delete,
                r#box::update,
                r#box::update_macro,
                r#box::update_index_macro,
                r#box::update_ops,
                r#box::upsert,
                r#box::upsert_macro,
                r#box::truncate,
                r#box::get,
                r#box::select,
                r#box::select_composite_key,
                r#box::len,
                r#box::random,
                r#box::min_max,
                r#box::count,
                r#box::extract_key,
                r#box::sequence_get_by_name,
                r#box::sequence_iterate,
                r#box::sequence_set,
                r#box::space_create_opt_default,
                r#box::space_create_opt_if_not_exists,
                r#box::space_create_id_increment,
                r#box::space_create_opt_user,
                r#box::space_create_opt_id,
                r#box::space_create_is_sync,
                r#box::space_meta,
                r#box::space_drop,
                r#box::index_create_drop,
                r#box::index_parts,
                tuple::tuple_new_from_struct,
                tuple::new_tuple_from_flatten_struct,
                tuple::tuple_field_count,
                tuple::tuple_size,
                tuple::tuple_decode,
                tuple::tuple_clone,
                tuple::tuple_iterator,
                tuple::tuple_iterator_seek_rewind,
                tuple::tuple_get_format,
                tuple::tuple_get_field,
                tuple::raw_bytes,
            ]);
            tests.append(&mut tests![
                [should_panic_if: !tarantool::ffi::has_fully_temporary_spaces()]
                r#box::fully_temporary_space,
            ]);
            tests.append(&mut tests![
                [should_panic_if: !tarantool::ffi::has_tuple_field_by_path()]
                tuple::tuple_get_field_path,
            ]);
            tests.append(&mut tests![
                tuple::tuple_compare,
                tuple::tuple_compare_with_key,
                tuple::to_and_from_lua,
                tuple::tuple_debug_fmt,
                tuple::tuple_buffer_from_vec_fail,
                coio::coio_accept,
                coio::coio_read_write,
                coio::coio_call,
                coio::coio_channel,
                coio::channel_rx_closed,
                coio::channel_tx_closed,
                transaction::transaction_commit,
                transaction::transaction_rollback,
                latch::latch_lock,
                latch::latch_try_lock,
                net_box::immediate_close,
                net_box::ping,
                net_box::ping_timeout,
                net_box::ping_concurrent,
                net_box::call,
                net_box::call_async,
                net_box::call_async_error,
                net_box::call_async_disconnected,
                net_box::call_timeout,
                net_box::call_async_timeout,
                net_box::call_async_wait_disconnected,
                net_box::eval,
                net_box::eval_async,
                net_box::async_common_cond,
                net_box::connection_error,
                net_box::is_connected,
                net_box::schema_sync,
                net_box::select,
                net_box::get,
                net_box::insert,
                net_box::replace,
                net_box::update,
                net_box::upsert,
                net_box::delete,
                net_box::cancel_recv,
                net_box::triggers_connect,
                net_box::triggers_reject,
                net_box::triggers_schema_sync,
                net_box::execute,
                proc::simple,
                proc::return_tuple,
                proc::return_raw_bytes,
                proc::with_error,
                proc::packed,
                proc::debug,
                proc::tarantool_reimport,
                proc::custom_ret,
                proc::inject,
                proc::inject_with_packed,
                uuid::to_tuple,
                uuid::from_tuple,
                uuid::to_lua,
                uuid::from_lua,
                enums::space_engine_type,
                enums::space_field_type,
                enums::index_type,
                enums::index_field_type,
                enums::rtree_index_distance_type,
                test_attr::with_custom_section,
            ]);
            tests.append(&mut tests![
                [should_panic_if: !tarantool::ffi::has_datetime()]
                datetime::to_tuple,
                datetime::from_tuple,
                datetime::to_lua,
                datetime::from_lua,
            ]);

            #[cfg(feature = "picodata")]
            {
                tests.append(&mut tests![
                    proc::return_port,
                    sql::prepared_source_query,
                    sql::prepared_invalid_query,
                    sql::prepared_no_params,
                    sql::prepared_large_query,
                    sql::prepared_with_unnamed_params,
                    sql::prepared_with_named_params,
                    sql::prepared_invalid_params,
                    sql::port_c,
                    tuple_picodata::tuple_hash,
                ])
            }
            tests
        },
    )
}

#[tarantool::proc]
pub fn entry(cfg: TestConfig) -> Result<(), Error> {
    create_test_spaces()?;

    let ok = run_tests(cfg)?;
    if !ok {
        return Err(Error::other("test failure"));
    }

    Ok(())
}
