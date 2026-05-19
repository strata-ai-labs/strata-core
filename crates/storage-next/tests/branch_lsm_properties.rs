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
    assert!(source.contains("read_view_capture_cases"));
    assert!(source.contains("pinned_append_isolation_cases"));
    assert!(source.contains("pinned_rotation_isolation_cases"));
    assert!(source.contains("latest_point_read_cases"));
    assert!(source.contains("version_bounded_point_read_cases"));
    assert!(source.contains("tombstone_shadow_read_cases"));
    assert!(source.contains("history_read_cases"));
    assert!(source.contains("history_tombstone_cases"));
    assert!(source.contains("history_limit_cases"));
    assert!(source.contains("prefix_scan_cases"));
    assert!(source.contains("range_scan_cases"));
    assert!(source.contains("scan_tombstone_suppression_cases"));
    assert!(source.contains("active_frozen_merge_read_cases"));
    assert!(source.contains("wrong_branch_read_rejection_cases"));
    assert!(source.contains("timestamp_bound_deferral_cases"));
    assert!(source.contains("immutable_descriptor_cases"));
    assert!(source.contains("immutable_l0_install_cases"));
    assert!(source.contains("immutable_l1_install_cases"));
    assert!(source.contains("invalid_immutable_install_rejection_cases"));
    assert!(source.contains("immutable_l1_overlap_rejection_cases"));
    assert!(source.contains("frozen_replacement_cases"));
    assert!(source.contains("pinned_immutable_install_isolation_cases"));
    assert!(source.contains("immutable_latest_read_cases"));
    assert!(source.contains("immutable_version_bounded_read_cases"));
    assert!(source.contains("immutable_history_cases"));
    assert!(source.contains("immutable_prefix_scan_cases"));
    assert!(source.contains("immutable_range_scan_cases"));
    assert!(source.contains("immutable_tombstone_shadow_cases"));
    assert!(source.contains("active_frozen_immutable_merge_read_cases"));
    assert!(source.contains("immutable_source_attribution_cases"));
    assert!(source.contains("inherited_fork_capture_cases"));
    assert!(source.contains("inherited_layer_validation_cases"));
    assert!(source.contains("inherited_latest_read_cases"));
    assert!(source.contains("inherited_version_bounded_read_cases"));
    assert!(source.contains("inherited_history_read_cases"));
    assert!(source.contains("inherited_prefix_scan_cases"));
    assert!(source.contains("inherited_range_scan_cases"));
    assert!(source.contains("inherited_key_rewrite_cases"));
    assert!(source.contains("inherited_child_put_shadow_cases"));
    assert!(source.contains("inherited_child_tombstone_shadow_cases"));
    assert!(source.contains("inherited_post_fork_invisibility_cases"));
    assert!(source.contains("inherited_chained_ancestry_cases"));
    assert!(source.contains("invalid_inherited_layer_rejection_cases"));
    assert!(source.contains("pinned_inherited_view_isolation_cases"));
    assert!(!source.contains("src/branch/mod.rs\").is_file()"));
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
fn branch_lsm_outcome_exercised_all_categories(
    outcome: &strata_storage_next::testkit::BranchLsmScaffoldOutcome,
) -> bool {
    !(outcome.valid_config_cases() == 0
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
        || outcome.read_view_capture_cases() == 0
        || outcome.pinned_append_isolation_cases() == 0
        || outcome.pinned_rotation_isolation_cases() == 0
        || outcome.latest_point_read_cases() == 0
        || outcome.version_bounded_point_read_cases() == 0
        || outcome.tombstone_shadow_read_cases() == 0
        || outcome.history_read_cases() == 0
        || outcome.history_tombstone_cases() == 0
        || outcome.history_limit_cases() == 0
        || outcome.prefix_scan_cases() == 0
        || outcome.range_scan_cases() == 0
        || outcome.scan_tombstone_suppression_cases() == 0
        || outcome.active_frozen_merge_read_cases() == 0
        || outcome.wrong_branch_read_rejection_cases() == 0
        || outcome.timestamp_bound_deferral_cases() == 0
        || outcome.immutable_descriptor_cases() == 0
        || outcome.immutable_l0_install_cases() == 0
        || outcome.immutable_l1_install_cases() == 0
        || outcome.invalid_immutable_install_rejection_cases() == 0
        || outcome.immutable_l1_overlap_rejection_cases() == 0
        || outcome.frozen_replacement_cases() == 0
        || outcome.pinned_immutable_install_isolation_cases() == 0
        || outcome.immutable_latest_read_cases() == 0
        || outcome.immutable_version_bounded_read_cases() == 0
        || outcome.immutable_history_cases() == 0
        || outcome.immutable_prefix_scan_cases() == 0
        || outcome.immutable_range_scan_cases() == 0
        || outcome.immutable_tombstone_shadow_cases() == 0
        || outcome.active_frozen_immutable_merge_read_cases() == 0
        || outcome.immutable_source_attribution_cases() == 0
        || outcome.inherited_fork_capture_cases() == 0
        || outcome.inherited_layer_validation_cases() == 0
        || outcome.inherited_latest_read_cases() == 0
        || outcome.inherited_version_bounded_read_cases() == 0
        || outcome.inherited_history_read_cases() == 0
        || outcome.inherited_prefix_scan_cases() == 0
        || outcome.inherited_range_scan_cases() == 0
        || outcome.inherited_key_rewrite_cases() == 0
        || outcome.inherited_child_put_shadow_cases() == 0
        || outcome.inherited_child_tombstone_shadow_cases() == 0
        || outcome.inherited_post_fork_invisibility_cases() == 0
        || outcome.inherited_chained_ancestry_cases() == 0
        || outcome.invalid_inherited_layer_rejection_cases() == 0
        || outcome.pinned_inherited_view_isolation_cases() == 0)
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
            if !branch_lsm_outcome_exercised_all_categories(&outcome) {
                return Err(TestCaseError::fail(
                    "branch LSM scaffold contract did not exercise all categories",
                ));
            }
            Ok(())
        })
        .expect("generated branch LSM scaffold property");
}
