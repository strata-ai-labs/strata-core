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
#[test]
fn lifecycle_property_harness_runs_recovery_contract() {
    use proptest::collection::vec;
    use proptest::prelude::any;
    use proptest::test_runner::TestCaseError;
    use proptest::test_runner::{Config, FileFailurePersistence, TestRunner};
    use strata_storage_next::testkit::check_lifecycle_recovery_contract;

    let mut runner = TestRunner::new(Config {
        cases: 16,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/lifecycle-recovery.txt",
        ))),
        ..Config::default()
    });

    runner
        .run(&vec(any::<u8>(), 0..=16), |script| {
            let outcome = check_lifecycle_recovery_contract(&script)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            if outcome.empty_recovery_cases() == 0
                || outcome.checkpoint_recovery_cases() == 0
                || outcome.log_tail_cases() == 0
                || outcome.strict_failure_cases() == 0
                || outcome.lossy_degradation_cases() == 0
                || outcome.input_derived_empty_cases() == 0
                || outcome.input_derived_checkpoint_cases() == 0
                || outcome.input_derived_log_tail_cases() == 0
                || outcome.input_derived_strict_failure_cases() == 0
                || outcome.input_derived_lossy_degradation_cases() == 0
            {
                return Err(TestCaseError::fail(
                    "lifecycle recovery contract did not exercise all categories",
                ));
            }
            Ok(())
        })
        .expect("generated lifecycle recovery property");
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_property_harness_runs_bootstrap_contract() {
    use proptest::collection::vec;
    use proptest::prelude::any;
    use proptest::test_runner::TestCaseError;
    use proptest::test_runner::{Config, FileFailurePersistence, TestRunner};
    use strata_storage_next::testkit::check_lifecycle_bootstrap_contract;

    let mut runner = TestRunner::new(Config {
        cases: 16,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/lifecycle-bootstrap.txt",
        ))),
        ..Config::default()
    });

    runner
        .run(&vec(any::<u8>(), 0..=32), |script| {
            let outcome = check_lifecycle_bootstrap_contract(&script)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            if outcome.empty_bootstrap_cases() == 0
                || outcome.checkpoint_bootstrap_cases() == 0
                || outcome.wal_replay_bootstrap_cases() == 0
                || outcome.degraded_bootstrap_cases() == 0
                || outcome.replay_rejection_cases() == 0
                || outcome.input_derived_empty_bootstrap_cases() == 0
                || outcome.input_derived_checkpoint_bootstrap_cases() == 0
                || outcome.input_derived_wal_replay_bootstrap_cases() == 0
                || outcome.input_derived_degraded_bootstrap_cases() == 0
                || outcome.input_derived_replay_rejection_cases() == 0
            {
                return Err(TestCaseError::fail(
                    "lifecycle bootstrap contract did not exercise all categories",
                ));
            }
            Ok(())
        })
        .expect("generated lifecycle bootstrap property");
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_property_harness_runs_checkpoint_contract() {
    use proptest::collection::vec;
    use proptest::prelude::any;
    use proptest::test_runner::TestCaseError;
    use proptest::test_runner::{Config, FileFailurePersistence, TestRunner};
    use strata_storage_next::testkit::check_lifecycle_checkpoint_contract;

    let mut runner = TestRunner::new(Config {
        cases: 32,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/lifecycle-checkpoint.txt",
        ))),
        ..Config::default()
    });

    runner
        .run(&vec(any::<u8>(), 0..=32), |script| {
            let outcome = check_lifecycle_checkpoint_contract(&script)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            if outcome.accepted_request_cases() == 0
                || outcome.deferred_request_cases() == 0
                || outcome.active_row_cases() == 0
                || outcome.frozen_row_cases() == 0
                || outcome.owned_row_cases() == 0
                || outcome.tombstone_row_cases() == 0
                || outcome.timeline_row_cases() == 0
                || outcome.flush_accept_cases() == 0
                || outcome.flush_reject_cases() == 0
                || outcome.flush_noop_cases() == 0
                || outcome.retention_accept_cases() == 0
                || outcome.retention_reject_cases() == 0
                || outcome.service_failure_cases() == 0
                || outcome.partial_window_cases() == 0
                || outcome.delete_failure_cases() == 0
                || outcome.checkpoint_truncation_round_trip_cases() == 0
                || outcome.cache_rejection_cases() == 0
            {
                return Err(TestCaseError::fail(
                    "lifecycle checkpoint contract did not exercise all categories",
                ));
            }
            Ok(())
        })
        .expect("generated lifecycle checkpoint property");
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_property_harness_runs_maintenance_contract() {
    use proptest::collection::vec;
    use proptest::prelude::any;
    use proptest::test_runner::TestCaseError;
    use proptest::test_runner::{Config, FileFailurePersistence, TestRunner};
    use strata_storage_next::testkit::check_lifecycle_maintenance_contract;

    let mut runner = TestRunner::new(Config {
        cases: 32,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/lifecycle-maintenance.txt",
        ))),
        ..Config::default()
    });

    runner
        .run(&vec(any::<u8>(), 0..=32), |script| {
            let outcome = check_lifecycle_maintenance_contract(&script)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            if outcome.input_enqueue_cases() == 0
                || outcome.input_coalesce_cases() == 0
                || outcome.input_run_cases() == 0
                || outcome.input_cancel_cases() == 0
                || outcome.input_drain_cases() == 0
                || outcome.input_fault_cases() == 0
                || outcome.input_queue_full_cases() == 0
                || outcome.input_admission_rejection_cases() == 0
                || outcome.input_model_step_cases() == 0
            {
                return Err(TestCaseError::fail(
                    "lifecycle maintenance contract did not exercise all categories",
                ));
            }
            Ok(())
        })
        .expect("generated lifecycle maintenance property");
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_property_harness_runs_flush_contract() {
    use proptest::collection::vec;
    use proptest::prelude::any;
    use proptest::test_runner::TestCaseError;
    use proptest::test_runner::{Config, FileFailurePersistence, TestRunner};
    use strata_storage_next::testkit::check_lifecycle_flush_contract;

    let mut runner = TestRunner::new(Config {
        cases: 16,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/lifecycle-flush.txt",
        ))),
        ..Config::default()
    });

    runner
        .run(&vec(any::<u8>(), 0..=32), |script| {
            let outcome = check_lifecycle_flush_contract(&script)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            if outcome.cache_success_cases() == 0
                || outcome.durable_success_cases() == 0
                || outcome.deferred_cases() == 0
                || outcome.publish_failure_cases() == 0
                || outcome.reopen_failure_cases() == 0
                || outcome.retry_cases() == 0
                || outcome.read_parity_cases() == 0
            {
                return Err(TestCaseError::fail(
                    "lifecycle flush contract did not exercise all categories",
                ));
            }
            Ok(())
        })
        .expect("generated lifecycle flush property");
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
