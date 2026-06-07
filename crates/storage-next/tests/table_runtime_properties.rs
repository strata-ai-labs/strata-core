//! Generated table-runtime property harness.

#![deny(unsafe_code)]

mod common;

use std::fs;

#[test]
fn table_runtime_property_harness_is_not_a_placeholder() {
    let source = fs::read_to_string(common::crate_root().join("tests/table_runtime_properties.rs"))
        .expect("read table runtime property harness");

    assert!(source.contains("table_runtime_property_harness_runs_scaffold_contract"));
    assert!(source.contains("check_table_runtime_scaffold_contract"));
    assert!(source.contains("immutable_table_reader_cases"));
    assert!(source.contains("object_backed_table_reader_cases"));
    assert!(source.contains("table_block_cache_cases"));
    assert!(source.contains("table_bloom_filter_cases"));
    assert!(source.contains("table_compaction_cases"));
    assert!(source.contains("table_perf_trace_cases"));
    assert!(!source.contains("src/table/mod.rs\").is_file()"));
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn table_runtime_property_harness_runs_scaffold_contract() {
    use proptest::collection::vec;
    use proptest::prelude::any;
    use proptest::test_runner::TestCaseError;
    use proptest::test_runner::{Config, FileFailurePersistence, TestRunner};
    use strata_storage_next::testkit::check_table_runtime_scaffold_contract;

    let mut runner = TestRunner::new(Config {
        cases: 32,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/table_runtime.txt",
        ))),
        ..Config::default()
    });

    runner
        .run(&vec(any::<u8>(), 0..=256), |script| {
            let outcome = check_table_runtime_scaffold_contract(&script)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            if outcome.valid_config_cases() == 0
                || outcome.invalid_config_cases() == 0
                || outcome.valid_fact_cases() == 0
                || outcome.invalid_fact_cases() == 0
                || outcome.row_key_adapter_cases() == 0
                || outcome.invalid_row_key_sequence_cases() == 0
                || outcome.key_bound_cases() == 0
                || outcome.size_accounting_cases() == 0
                || outcome.mutable_frozen_table_cases() == 0
                || outcome.raw_cursor_cases() == 0
                || outcome.immutable_builder_artifact_cases() == 0
                || outcome.immutable_table_reader_cases() == 0
                || outcome.object_backed_table_reader_cases() == 0
                || outcome.table_block_cache_cases() == 0
                || outcome.table_bloom_filter_cases() == 0
                || outcome.table_compaction_cases() == 0
                || outcome.error_source_cases() == 0
            {
                return Err(TestCaseError::fail(
                    "table runtime scaffold contract did not exercise all categories",
                ));
            }
            #[cfg(feature = "perf-trace")]
            if outcome.table_perf_trace_cases() == 0 {
                return Err(TestCaseError::fail(
                    "table runtime scaffold contract did not exercise table perf-trace perf-trace cases",
                ));
            }
            Ok(())
        })
        .expect("generated table runtime scaffold property");
}
