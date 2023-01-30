//! Internals used by custom test runtime to run tests that require tarantool environment

use tester::{ShouldPanic, TestDesc, TestDescAndFn, TestFn, TestName, TestType};

#[derive(Clone)]
pub struct TestCase {
    pub name: &'static str,
    pub f: fn(),
}

/// The recommended way to describe tests in `tlua` crate
///
/// # Example
/// ```
/// tlua::tests! {
///     fn my_test() {
///         assert!(true);
///     }
/// }
/// ```
#[macro_export]
macro_rules! tests {
    ($(fn $test:ident () $body:block)+) => {
        $(
            #[::linkme::distributed_slice($crate::test::TLUA_TESTS)]
            #[allow(non_upper_case_globals)]
            static $test: $crate::test::TestCase = $crate::test::TestCase {
                name: concat!(module_path!(), "::", ::std::stringify!($test)),
                f: || $body,
            };
        )+
    }
}

// It is not possible to name it `TESTS` as in `tarantool` lib
// as there happens a name clash and tests are executed twice.
//
// This seems to be an undocumented side effect of `linkme`.
#[::linkme::distributed_slice]
pub static TLUA_TESTS: [TestCase] = [..];

pub fn collect() -> Vec<TestDescAndFn> {
    TLUA_TESTS
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
