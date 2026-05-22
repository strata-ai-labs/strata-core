//! Generated lifecycle scaffold property harness.

#![deny(unsafe_code)]

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_property_harness_runs_scaffold_contract() {
    use proptest::collection::vec;
    use proptest::prelude::any;
    use proptest::test_runner::TestCaseError;
    use proptest::test_runner::{Config, FileFailurePersistence, TestRunner};
    use strata_storage_next::testkit::check_lifecycle_scaffold_contract;

    let mut runner = TestRunner::new(Config {
        cases: 32,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/lifecycle.txt",
        ))),
        ..Config::default()
    });

    runner
        .run(&vec(any::<u8>(), 0..=63), |script| {
            let outcome = check_lifecycle_scaffold_contract(&script)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            if !all_categories_exercised(&outcome) {
                return Err(TestCaseError::fail(
                    "lifecycle scaffold contract did not exercise all categories",
                ));
            }
            Ok(())
        })
        .expect("generated lifecycle scaffold property");
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
fn all_categories_exercised(
    outcome: &strata_storage_next::testkit::LifecycleScaffoldOutcome,
) -> bool {
    outcome.valid_config_cases() > 0
        && outcome.invalid_config_cases() > 0
        && outcome.lifecycle_state_cases() > 0
        && outcome.storage_mode_cases() > 0
        && outcome.valid_transition_cases() > 0
        && outcome.invalid_transition_cases() > 0
        && outcome.operation_admission_accept_cases() > 0
        && outcome.operation_admission_reject_cases() > 0
        && outcome.close_retry_cases() > 0
        && outcome.closed_idempotence_cases() > 0
        && outcome.failed_state_sticky_cases() > 0
        && outcome.input_derived_state_cases() > 0
        && outcome.open_plan_cases() > 0
        && outcome.open_outcome_cases() > 0
        && outcome.recovery_health_cases() > 0
        && outcome.maintenance_task_cases() > 0
        && outcome.reclaim_fact_cases() > 0
        && outcome.error_display_cases() > 0
        && outcome.error_source_cases() > 0
        && outcome.stats_cases() > 0
        && outcome.source_guard_fixture_cases() > 0
        && outcome.accepted_capability_cases() > 0
        && outcome.rejected_capability_cases() > 0
        && outcome.cache_capability_cases() > 0
        && outcome.durable_standard_capability_cases() > 0
        && outcome.durable_always_capability_cases() > 0
        && outcome.object_candidate_capability_cases() > 0
        && outcome.missing_capability_cases() > 0
        && outcome.object_candidate_conditional_publish_cases() > 0
        && outcome.object_candidate_create_update_cases() > 0
        && outcome.capability_preflight_cases() > 0
        && outcome.input_derived_capability_cases() > 0
        && outcome.cache_open_accepted_cases() > 0
        && outcome.cache_open_rejected_cases() > 0
        && outcome.cache_baseline_cases() > 0
        && outcome.cache_durable_absence_cases() > 0
        && outcome.cache_commit_read_cases() > 0
        && outcome.cache_close_cases() > 0
        && outcome.cache_close_idempotence_cases() > 0
        && outcome.cache_commit_after_close_rejected_cases() > 0
        && outcome.cache_reopen_empty_cases() > 0
        && outcome.input_derived_cache_cases() > 0
        && outcome.durable_assembly_standard_cases() > 0
        && outcome.durable_assembly_always_cases() > 0
        && outcome.durable_assembly_rejected_cases() > 0
        && outcome.durable_manifest_create_cases() > 0
        && outcome.durable_manifest_existing_cases() > 0
        && outcome.durable_writer_lock_failure_cases() > 0
        && outcome.durable_manifest_identity_mismatch_cases() > 0
        && outcome.durable_manifest_create_race_cases() > 0
        && outcome.durable_manifest_publish_fault_cases() > 0
        && outcome.durable_wal_open_failure_cases() > 0
        && outcome.durable_recovering_admission_cases() > 0
        && outcome.durable_no_recovery_side_effect_cases() > 0
        && outcome.input_derived_durable_cases() > 0
}
