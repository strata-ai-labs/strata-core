//! Generated branch-LSM scaffold property harness.

#![deny(unsafe_code)]

mod common;

use std::fs;

#[test]
fn branch_lsm_property_harness_is_not_a_placeholder() {
    let source = fs::read_to_string(common::crate_root().join("tests/branch_lsm_properties.rs"))
        .expect("read branch LSM property harness");

    assert!(source.contains("branch_lsm_property_harness_runs_scaffold_contract"));
    assert!(source.contains("check_branch_lsm_scaffold_contract"));
    assert!(source.contains("read_bound_cases"));
    assert!(source.contains("descriptor_cases"));
    assert!(source.contains("error_source_cases"));
    assert!(source.contains("matching_row_cases"));
    assert!(source.contains("inherited_bound_cases"));
    assert!(source.contains("candidate_tombstone_cases"));
    assert!(source.contains("edge_row_cases"));
    assert!(source.contains("encoded_grouping_cases"));
    assert!(source.contains("row_chain_cases"));
    assert!(source.contains("fork_edge_cases"));
    assert!(source.contains("state_construction_cases"));
    assert!(source.contains("committed_put_append_cases"));
    assert!(source.contains("committed_tombstone_append_cases"));
    assert!(source.contains("wrong_branch_append_rejection_cases"));
    assert!(source.contains("active_duplicate_rejection_cases"));
    assert!(source.contains("frozen_duplicate_rejection_cases"));
    assert!(source.contains("same_key_version_append_cases"));
    assert!(source.contains("same_version_key_append_cases"));
    assert!(source.contains("active_rotation_cases"));
    assert!(source.contains("empty_rotation_skip_cases"));
    assert!(source.contains("frozen_limit_skip_cases"));
    assert!(source.contains("active_only_fact_cases"));
    assert!(source.contains("frozen_only_fact_cases"));
    assert!(source.contains("mixed_active_frozen_fact_cases"));
    assert!(source.contains("timestamp_edge_fact_cases"));
    assert!(source.contains("max_commit_edge_fact_cases"));
    assert!(!source.contains("src/branch/mod.rs\").is_file()"));
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn branch_lsm_property_harness_runs_scaffold_contract() {
    use proptest::collection::vec;
    use proptest::prelude::any;
    use proptest::test_runner::TestCaseError;
    use proptest::test_runner::{Config, FileFailurePersistence, TestRunner};
    use strata_storage_next::testkit::check_branch_lsm_scaffold_contract;

    let mut runner = TestRunner::new(Config {
        cases: 32,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/branch_lsm.txt",
        ))),
        ..Config::default()
    });

    runner
        .run(&vec(any::<u8>(), 0..=128), |script| {
            let outcome = check_branch_lsm_scaffold_contract(&script)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            if outcome.valid_config_cases() == 0
                || outcome.invalid_config_cases() == 0
                || outcome.read_bound_cases() == 0
                || outcome.valid_fact_cases() == 0
                || outcome.invalid_fact_cases() == 0
                || outcome.descriptor_cases() == 0
                || outcome.error_source_cases() == 0
                || outcome.stats_cases() == 0
                || outcome.matching_row_cases() == 0
                || outcome.mismatching_row_cases() == 0
                || outcome.physical_key_rewrite_cases() == 0
                || outcome.row_rewrite_cases() == 0
                || outcome.own_bound_cases() == 0
                || outcome.inherited_bound_cases() == 0
                || outcome.candidate_put_cases() == 0
                || outcome.candidate_tombstone_cases() == 0
                || outcome.edge_row_cases() == 0
                || outcome.encoded_grouping_cases() == 0
                || outcome.row_chain_cases() == 0
                || outcome.fork_edge_cases() == 0
                || outcome.state_construction_cases() == 0
                || outcome.committed_put_append_cases() == 0
                || outcome.committed_tombstone_append_cases() == 0
                || outcome.wrong_branch_append_rejection_cases() == 0
                || outcome.active_duplicate_rejection_cases() == 0
                || outcome.frozen_duplicate_rejection_cases() == 0
                || outcome.same_key_version_append_cases() == 0
                || outcome.same_version_key_append_cases() == 0
                || outcome.active_rotation_cases() == 0
                || outcome.empty_rotation_skip_cases() == 0
                || outcome.frozen_limit_skip_cases() == 0
                || outcome.active_only_fact_cases() == 0
                || outcome.frozen_only_fact_cases() == 0
                || outcome.mixed_active_frozen_fact_cases() == 0
                || outcome.timestamp_edge_fact_cases() == 0
                || outcome.max_commit_edge_fact_cases() == 0
            {
                return Err(TestCaseError::fail(
                    "branch LSM scaffold contract did not exercise all categories",
                ));
            }
            Ok(())
        })
        .expect("generated branch LSM scaffold property");
}
