//! Generated immutable-table format harness.

#![deny(unsafe_code)]

mod common;

use std::fs;

#[test]
fn table_property_harness_is_not_a_placeholder() {
    let source = fs::read_to_string(common::crate_root().join("tests/table_properties.rs"))
        .expect("read table property harness");

    assert!(source.contains("table_property_harness_runs_generated_artifacts"));
    assert!(source.contains("check_table_format_model_script"));
    assert!(!source.contains("src/table/mod.rs\").is_file()"));
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn table_property_harness_runs_generated_artifacts() {
    use proptest::collection::vec;
    use proptest::prelude::any;
    use proptest::test_runner::TestCaseError;
    use proptest::test_runner::{Config, FileFailurePersistence, TestRunner};
    use strata_storage::testkit::check_table_format_model_script;

    let mut runner = TestRunner::new(Config {
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/table_format.txt",
        ))),
        ..Config::default()
    });

    runner
        .run(&vec(any::<u8>(), 0..=1024), |script| {
            check_table_format_model_script(&script)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            Ok(())
        })
        .expect("generated table format property");
}
