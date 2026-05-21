//! Generated commit-runtime scaffold property harness.

#![deny(unsafe_code)]

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn commit_runtime_property_harness_runs_scaffold_contract() {
    use proptest::collection::vec;
    use proptest::prelude::any;
    use proptest::test_runner::TestCaseError;
    use proptest::test_runner::{Config, FileFailurePersistence, TestRunner};
    use strata_storage_next::testkit::check_commit_runtime_scaffold_contract;

    let mut runner = TestRunner::new(Config {
        cases: 32,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/commit_runtime.txt",
        ))),
        ..Config::default()
    });

    runner
        .run(&vec(any::<u8>(), 0..=79), |script| {
            let outcome = check_commit_runtime_scaffold_contract(&script)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            if !all_categories_exercised(&outcome) {
                return Err(TestCaseError::fail(
                    "commit runtime scaffold contract did not exercise all categories",
                ));
            }
            Ok(())
        })
        .expect("generated commit runtime scaffold property");
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
fn all_categories_exercised(
    outcome: &strata_storage_next::testkit::CommitRuntimeScaffoldOutcome,
) -> bool {
    core_categories_exercised(outcome)
        && outcome_categories_exercised(outcome)
        && guard_categories_exercised(outcome)
        && conflict_categories_exercised(outcome)
        && timeline_categories_exercised(outcome)
        && cache_categories_exercised(outcome)
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
fn core_categories_exercised(
    outcome: &strata_storage_next::testkit::CommitRuntimeScaffoldOutcome,
) -> bool {
    outcome.valid_config_cases() > 0
        && outcome.invalid_config_cases() > 0
        && outcome.phase_fact_cases() > 0
        && outcome.visibility_fact_cases() > 0
        && outcome.invalid_visibility_fact_cases() > 0
        && outcome.error_display_cases() > 0
        && outcome.error_source_cases() > 0
        && outcome.stats_cases() > 0
        && outcome.source_guard_fixture_cases() > 0
        && outcome.valid_batch_cases() > 0
        && outcome.invalid_batch_cases() > 0
        && outcome.duplicate_mutation_cases() > 0
        && outcome.branch_mismatch_cases() > 0
        && outcome.storage_owned_space_cases() > 0
        && outcome.invalid_fact_cases() > 0
        && outcome.stamping_cases() > 0
        && outcome.expiry_rejection_cases() > 0
        && outcome.stamping_rejection_cases() > 0
        && outcome.version_allocation_cases() > 0
        && outcome.version_catch_up_cases() > 0
        && outcome.version_overflow_cases() > 0
        && outcome.generated_timestamp_cases() > 0
        && outcome.clamped_timestamp_cases() > 0
        && outcome.explicit_timestamp_cases() > 0
        && outcome.invalid_explicit_timestamp_cases() > 0
        && outcome.timestamp_source_failure_cases() > 0
        && outcome.read_only_no_allocation_cases() > 0
        && outcome.no_transaction_id_check_cases() > 0
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
fn outcome_categories_exercised(
    outcome: &strata_storage_next::testkit::CommitRuntimeScaffoldOutcome,
) -> bool {
    outcome.read_only_outcome_cases() > 0
        && outcome.read_only_disabled_rejection_cases() > 0
        && outcome.visible_tracker_initialization_cases() > 0
        && outcome.visible_tracker_monotonic_publish_cases() > 0
        && outcome.visible_tracker_regression_rejection_cases() > 0
        && outcome.outcome_invalid_visibility_fact_cases() > 0
        && outcome.outcome_constructor_rejection_cases() > 0
        && outcome.mutation_count_fact_cases() > 0
        && outcome.cross_branch_read_only_fact_cases() > 0
        && outcome.read_only_outcome_no_allocation_cases() > 0
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
fn guard_categories_exercised(
    outcome: &strata_storage_next::testkit::CommitRuntimeScaffoldOutcome,
) -> bool {
    outcome.branch_registration_success_cases() > 0
        && outcome.duplicate_registration_rejection_cases() > 0
        && outcome.missing_branch_rejection_cases() > 0
        && outcome.deleting_branch_rejection_cases() > 0
        && outcome.generation_exact_match_cases() > 0
        && outcome.generation_mismatch_cases() > 0
        && outcome.generation_not_supplied_cases() > 0
        && outcome.stale_generation_after_recreate_cases() > 0
        && outcome.same_branch_guard_contention_cases() > 0
        && outcome.different_branch_simultaneous_guard_cases() > 0
        && outcome.quiesce_start_success_cases() > 0
        && outcome.quiesce_rejected_with_active_guard_cases() > 0
        && outcome.mutating_guard_rejected_during_quiesce_cases() > 0
        && outcome.read_only_allowed_during_quiesce_cases() > 0
        && outcome.guard_release_and_reacquire_cases() > 0
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
fn conflict_categories_exercised(
    outcome: &strata_storage_next::testkit::CommitRuntimeScaffoldOutcome,
) -> bool {
    outcome.read_present_match_cases() > 0
        && outcome.read_present_mismatch_cases() > 0
        && outcome.read_present_becomes_missing_cases() > 0
        && outcome.read_missing_match_cases() > 0
        && outcome.read_missing_becomes_present_cases() > 0
        && outcome.cas_present_match_cases() > 0
        && outcome.cas_present_mismatch_cases() > 0
        && outcome.cas_present_becomes_missing_cases() > 0
        && outcome.cas_missing_match_cases() > 0
        && outcome.cas_missing_becomes_present_cases() > 0
        && outcome.combined_read_before_cas_cases() > 0
        && outcome.blind_put_no_conflict_cases() > 0
        && outcome.blind_delete_no_conflict_cases() > 0
        && outcome.skip_mode_no_read_cases() > 0
        && outcome.lower_layer_read_failure_cases() > 0
        && outcome.conflict_error_vocabulary_cases() > 0
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
fn timeline_categories_exercised(
    outcome: &strata_storage_next::testkit::CommitRuntimeScaffoldOutcome,
) -> bool {
    outcome.valid_timeline_entry_cases() > 0
        && outcome.zero_timeline_version_rejection_cases() > 0
        && outcome.timestamp_index_key_cases() > 0
        && outcome.version_index_key_cases() > 0
        && outcome.timeline_row_pair_cases() > 0
        && outcome.timeline_shared_commit_fact_cases() > 0
        && outcome.timestamp_index_decode_cases() > 0
        && outcome.version_index_decode_cases() > 0
        && outcome.malformed_timeline_prefix_rejection_cases() > 0
        && outcome.malformed_timeline_key_length_rejection_cases() > 0
        && outcome.timeline_value_length_rejection_cases() > 0
        && outcome.timeline_key_value_mismatch_rejection_cases() > 0
        && outcome.timestamp_lookup_exact_match_cases() > 0
        && outcome.timestamp_lookup_between_match_cases() > 0
        && outcome.duplicate_timestamp_tiebreak_cases() > 0
        && outcome.version_timestamp_lookup_cases() > 0
        && outcome.timeline_branch_isolation_cases() > 0
        && outcome.timeline_row_order_independence_cases() > 0
        && outcome.timeline_bounds_report_cases() > 0
        && outcome.timeline_caller_rejection_cases() > 0
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
fn cache_categories_exercised(
    outcome: &strata_storage_next::testkit::CommitRuntimeScaffoldOutcome,
) -> bool {
    outcome.cache_put_commit_cases() > 0
        && outcome.cache_delete_commit_cases() > 0
        && outcome.cache_mixed_commit_cases() > 0
        && outcome.cache_one_version_per_batch_cases() > 0
        && outcome.cache_one_timestamp_per_batch_cases() > 0
        && outcome.cache_timeline_rows_installed_cases() > 0
        && outcome.cache_visible_publication_cases() > 0
        && outcome.cache_not_durable_outcome_cases() > 0
        && outcome.cache_branch_admission_rejection_cases() > 0
        && outcome.cache_conflict_rejection_cases() > 0
        && outcome.cache_non_cache_rejection_cases() > 0
        && outcome.cache_apply_failure_atomicity_cases() > 0
        && outcome.cache_version_gap_after_failure_cases() > 0
        && outcome.cache_applied_above_visible_rejection_cases() > 0
        && outcome.cache_visible_allocator_mismatch_rejection_cases() > 0
        && outcome.cache_guard_release_after_failure_cases() > 0
}
