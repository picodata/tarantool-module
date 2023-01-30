//! Internals used by custom test runtime to run tests that require tarantool environment

use tester::{ShouldPanic, TestDesc, TestDescAndFn, TestFn, TestName, TestType};

#[derive(Clone)]
pub struct TestCase {
    pub name: &'static str,
    // TODO: Support functions returning `Result`
    pub f: fn(),
}

/// The recommended way to describe tests in `tarantool` crate
///
/// # Example
/// ```
/// tarantool::tests! {
///     fn my_test() {
///         assert!(true);
///     }
/// }
/// ```
#[macro_export]
macro_rules! tests {
    ($(fn $test:ident () $body:block)+) => {
        $(
            #[::linkme::distributed_slice($crate::test::TARANTOOL_MODULE_TESTS)]
            #[allow(non_upper_case_globals)]
            static $test: $crate::test::TestCase = $crate::test::TestCase {
                name: concat!(module_path!(), "::", ::std::stringify!($test)),
                f: || $body,
            };
        )+
    }
}

#[linkme::distributed_slice]
pub static TARANTOOL_MODULE_TESTS: [TestCase] = [..];

pub fn collect() -> Vec<TestDescAndFn> {
    TARANTOOL_MODULE_TESTS
        .iter()
        .map(|case| TestDescAndFn {
            desc: TestDesc {
                name: TestName::StaticTestName(case.name),
                ignore: false,
                should_panic: ShouldPanic::No,
                allow_fail: false,
                test_type: TestType::IntegrationTest,
            },
            testfn: TestFn::StaticTestFn(case.f),
        })
        .collect()
}

/// The default port where tarantool listens in tests
pub const TARANTOOL_LISTEN: u16 = 3301;
