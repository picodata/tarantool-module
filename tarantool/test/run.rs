use serde::{Deserialize, Serialize};

use std::env;
use std::process::{Command, Stdio};

#[derive(Serialize, Deserialize)]
struct Metadata {
    workspace_root: String,
}

#[derive(Serialize, Deserialize)]
struct TestNames {
    test_names: Vec<String>,
}

const NAMES_BEGIN: &str = "TEST_NAMES_BEGIN";
const NAMES_END: &str = "TEST_NAMES_END";
const PASSED: &str = "\x1b[0;32mok\x1b[0m";
const FAILED: &str = "\x1b[0;31mFAILED\x1b[0m";

fn main() {
    let filter = env::args().nth(1);
    let tarantool_exec =
        env::var("TARANTOOL_EXECUTABLE").unwrap_or_else(|_| "tarantool".to_owned());
    let metadata = Command::new("cargo")
        .arg("metadata")
        .arg("--format-version=1")
        .output()
        .expect("failed to get cargo metadata output");
    let metadata: Metadata =
        serde_json::from_slice(&metadata.stdout).expect("failed to parse cargo metadata output");
    let output = Command::new(tarantool_exec.clone())
        .arg(format!("{}/tests/run_tests.lua", metadata.workspace_root))
        .arg("--print")
        .output()
        .expect("Failed to get test names")
        .stdout;
    let output = String::from_utf8(output).expect("Failed to parse test names as utf8");
    let test_names =
        find_test_names(&output).expect("Failed to find test names in tarantool outptut");
    let tests_len = test_names.len();
    let test_names: TestNames =
        serde_json::from_str(test_names).expect("Failed to parse test names");
    let tests = test_names.test_names.into_iter().filter(|test| {
        if let Some(filter) = &filter {
            test.contains(filter)
        } else {
            true
        }
    });
    let mut failures = vec![];
    let mut passed: usize = 0;
    let num_cpus = num_cpus::get();
    let (tx, rx) = std::sync::mpsc::channel();
    let pool = rayon::ThreadPoolBuilder::new().num_threads(dbg!(num_cpus)).build().unwrap();
    enum TestResult {
        Passed,
        Failed(Vec<u8>),
        Error(std::io::Error),
    }
    for test in tests {
        let tx = tx.clone();
        let tarantool_exec = tarantool_exec.clone();
        let workspace_root = metadata.workspace_root.clone();
        pool.spawn(move || {
            let output = Command::new(tarantool_exec)
                .arg(format!("{}/tests/run_tests.lua", workspace_root))
                .arg(test.clone())
                .stdout(Stdio::null())
                .stderr(Stdio::piped())
                .output();
            let output = tarantool::unwrap_ok_or!{output,
                Err(e) => {
                    tx.send((test, TestResult::Error(e))).unwrap();
                    return;
                }
            };
            if output.status.success() {
                tx.send((test, TestResult::Passed)).unwrap();
            } else {
                tx.send((test, TestResult::Failed(output.stderr))).unwrap();
            }
        });
    }
    drop(tx);

    for (test, result) in rx {
        print!("test {} ... ", test);
        match result {
            TestResult::Passed => {
                println!("{}", PASSED);
                passed += 1;
            }
            TestResult::Failed(output) => {
                println!("{}", FAILED);
                failures.push((test, output))
            }
            TestResult::Error(err) => {
                panic!("Failed to run tarantool for test {}: {}", test, err)
            }
        }
    }
    println!();
    let failures_len = failures.len();
    for (test, output) in failures {
        println!("test {} failed", test);
        println!("STDERR:");
        println!("{}", String::from_utf8_lossy(&output));
    }
    let test_result = if failures_len == 0 { PASSED } else { FAILED };
    println!();
    println!(
        "test result: {}. {} passed; {} failed; {} filtered out",
        test_result,
        passed,
        failures_len,
        tests_len - passed - failures_len
    );
    if failures_len != 0 {
        std::process::exit(1);
    }
}

fn find_test_names(s: &str) -> Option<&str> {
    let start = s.find(NAMES_BEGIN)? + NAMES_BEGIN.len();
    let end = s.find(NAMES_END)?;
    Some(&s[start..end])
}
