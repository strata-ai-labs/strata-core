//! Generated lifecycle scaffold property harness.

#![deny(unsafe_code)]

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_property_harness_runs_scaffold_contract() {
    use proptest::collection::vec;
    use proptest::prelude::any;
    use proptest::test_runner::TestCaseError;
    use proptest::test_runner::{Config, FileFailurePersistence, TestRunner};
    use strata_storage::testkit::check_lifecycle_scaffold_contract;

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
    use strata_storage::testkit::check_lifecycle_recovery_contract;

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
    use strata_storage::testkit::check_lifecycle_bootstrap_contract;

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
    use strata_storage::testkit::check_lifecycle_checkpoint_contract;

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
    use strata_storage::testkit::check_lifecycle_maintenance_contract;

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
    use strata_storage::testkit::check_lifecycle_flush_contract;

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
#[test]
fn lifecycle_property_harness_runs_table_rewrite_contract() {
    use proptest::collection::vec;
    use proptest::prelude::any;
    use proptest::test_runner::TestCaseError;
    use proptest::test_runner::{Config, FileFailurePersistence, TestRunner};
    use strata_storage::testkit::check_lifecycle_table_rewrite_contract;

    let mut runner = TestRunner::new(Config {
        cases: 16,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/lifecycle-table-rewrite.txt",
        ))),
        ..Config::default()
    });

    runner
        .run(&vec(any::<u8>(), 0..=32), |script| {
            let outcome = check_lifecycle_table_rewrite_contract(&script)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            if outcome.cache_compaction_cases() == 0
                || outcome.durable_compaction_cases() == 0
                || outcome.materialization_cases() == 0
                || outcome.pressure_cases() == 0
                || outcome.compaction_output_published_cases() == 0
                || outcome.materialization_output_published_cases() == 0
                || outcome.output_reopened_cases() == 0
                || outcome.install_after_publish_cases() == 0
                || outcome.manifest_after_install_cases() == 0
                || outcome.publish_failed_before_install_cases() == 0
                || outcome.install_failed_after_publish_cases() == 0
                || outcome.manifest_failed_after_install_cases() == 0
                || outcome.orphan_output_recorded_cases() == 0
                || outcome.no_pruning_observed_cases() == 0
            {
                return Err(TestCaseError::fail(
                    "lifecycle table rewrite contract did not exercise all categories",
                ));
            }
            Ok(())
        })
        .expect("generated lifecycle table rewrite property");
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_property_harness_runs_row_pruning_contract() {
    use proptest::collection::vec;
    use proptest::prelude::*;
    use proptest::test_runner::{Config, FileFailurePersistence, TestRunner};
    use strata_storage::testkit::check_lifecycle_row_pruning_contract;

    let mut runner = TestRunner::new(Config {
        cases: 16,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/lifecycle-row-pruning.txt",
        ))),
        ..Config::default()
    });

    runner
        .run(&vec(any::<u8>(), 0..=32), |script| {
            let outcome = check_lifecycle_row_pruning_contract(&script)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            if outcome.proof_rejected_cases() == 0
                || outcome.old_version_dropped_cases() == 0
                || outcome.tombstone_dropped_cases() == 0
                || outcome.tombstone_kept_for_shadowing_cases() == 0
                || outcome.expired_row_dropped_cases() == 0
                || outcome.expired_row_kept_for_as_of_cases() == 0
                || outcome.pinned_view_blocked_cases() == 0
                || outcome.inherited_layer_blocked_cases() == 0
                || outcome.retained_boundary_reported_cases() == 0
                || outcome.recovery_boundary_enforced_cases() == 0
            {
                return Err(TestCaseError::fail(
                    "lifecycle row pruning contract did not exercise all categories",
                ));
            }
            Ok(())
        })
        .expect("generated lifecycle row pruning property");
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_property_harness_runs_budget_contract() {
    use proptest::collection::vec;
    use proptest::prelude::any;
    use proptest::test_runner::TestCaseError;
    use proptest::test_runner::{Config, FileFailurePersistence, TestRunner};
    use strata_storage::testkit::check_lifecycle_budget_contract;

    let mut runner = TestRunner::new(Config {
        cases: 32,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/lifecycle-budget.txt",
        ))),
        ..Config::default()
    });

    runner
        .run(&vec(any::<u8>(), 0..=32), |script| {
            let outcome = check_lifecycle_budget_contract(&script)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            if outcome.budget_accept_cases() == 0
                || outcome.budget_reject_cases() == 0
                || outcome.reservation_release_on_success_cases() == 0
                || outcome.reservation_release_on_failure_cases() == 0
                || outcome.cache_eviction_cases() == 0
                || outcome.reader_reject_cases() == 0
                || outcome.active_reject_cases() == 0
                || outcome.artifact_defer_cases() == 0
                || outcome.maintenance_queue_reject_cases() == 0
                || outcome.low_memory_smoke_cases() == 0
                || outcome.isolation_cases() == 0
            {
                return Err(TestCaseError::fail(
                    "lifecycle budget contract did not exercise all categories",
                ));
            }
            Ok(())
        })
        .expect("generated lifecycle budget property");
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_property_harness_runs_generated_script_contract() {
    use proptest::collection::vec;
    use proptest::prelude::any;
    use proptest::test_runner::TestCaseError;
    use proptest::test_runner::{Config, FileFailurePersistence, TestRunner};
    use strata_storage::testkit::check_lifecycle_generated_script_contract;

    let mut runner = TestRunner::new(Config {
        cases: 16,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/lifecycle-generated.txt",
        ))),
        ..Config::default()
    });

    runner
        .run(&vec(any::<u8>(), 0..=80), |script| {
            let outcome = check_lifecycle_generated_script_contract(&script)
                .map_err(|error| TestCaseError::fail(error.to_string()))?;
            if !generated_script_categories_exercised(&outcome) {
                return Err(TestCaseError::fail(
                    "generated lifecycle script did not exercise all categories",
                ));
            }
            Ok(())
        })
        .expect("generated lifecycle script property");
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_property_harness_requires_input_derived_recovery_routes() {
    let outcome = generated_script_outcome(b"recovery-generated-input");
    let model = outcome.model_facts();

    assert!(outcome.input_open_recovery_close_route_cases() > 0);
    assert!(outcome.recovered_visibility_match_cases() > 0);
    assert!(outcome.lossy_degraded_health_check_cases() > 0);
    assert!(model.visible_version() >= model.checkpoint_watermark());
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_property_harness_requires_input_derived_maintenance_routes() {
    let outcome = generated_script_outcome(b"maintenance-generated-input");
    let model = outcome.model_facts();

    assert!(outcome.input_maintenance_route_cases() > 0);
    assert!(outcome.watermark_monotonic_check_cases() > 0);
    assert!(model.visible_version() >= model.flush_watermark());
    assert!(model.pending_maintenance_tasks() <= 4);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_property_harness_requires_input_derived_retention_routes() {
    let outcome = generated_script_outcome(b"retention-generated-input");
    let model = outcome.model_facts();

    assert!(outcome.input_reclaim_route_cases() > 0);
    assert!(outcome.deletion_subset_check_cases() > 0);
    assert!(model.purged_objects() <= model.reclaim_proofed_objects());
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_property_harness_requires_input_derived_quarantine_routes() {
    let outcome = generated_script_outcome(b"quarantine-generated-input");

    assert!(outcome.input_reclaim_route_cases() > 0);
    assert!(outcome.deletion_subset_check_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_property_harness_requires_input_derived_close_routes() {
    let outcome = generated_script_outcome(b"close-generated-input");

    assert!(outcome.input_open_recovery_close_route_cases() > 0);
    assert!(outcome.close_idempotence_check_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_property_harness_replays_minimized_failure_case() {
    let outcome = generated_script_outcome(&[]);
    let model = outcome.model_facts();

    assert!(outcome.validation_only_rejection_cases() > 0);
    assert!(generated_script_categories_exercised(&outcome));
    assert!(model.validation_rejections() > 0 || outcome.validation_only_rejection_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_property_harness_records_regression_file() {
    let outcome = generated_script_outcome(b"regression-file-route");

    assert!(generated_script_categories_exercised(&outcome));
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_generated_script_exercises_input_derived_open_recovery_and_close() {
    let outcome = generated_script_outcome(rich_generated_script());

    assert!(outcome.input_open_recovery_close_route_cases() > 0);
    assert!(outcome.close_idempotence_check_cases() > 0);
    assert!(outcome.model_facts().closed());
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_generated_script_exercises_input_derived_maintenance_routes() {
    let outcome = generated_script_outcome(rich_generated_script());
    let model = outcome.model_facts();

    assert!(outcome.input_maintenance_route_cases() > 0);
    assert!(outcome.watermark_monotonic_check_cases() > 0);
    assert!(model.pending_maintenance_tasks() <= 4);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_generated_script_exercises_input_derived_reclaim_routes() {
    let outcome = generated_script_outcome(rich_generated_script());
    let model = outcome.model_facts();

    assert!(outcome.input_reclaim_route_cases() > 0);
    assert!(outcome.deletion_subset_check_cases() > 0);
    assert!(model.purged_objects() <= model.reclaim_proofed_objects());
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_generated_script_rejects_validation_only_script_without_side_effect_claim() {
    let outcome = generated_script_outcome(&[0, 1]);
    let model = outcome.model_facts();

    assert!(outcome.validation_only_rejection_cases() > 0);
    assert!(model.validation_rejections() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_generated_script_model_matches_healthy_recovered_visibility() {
    let outcome = generated_script_outcome(&[0, 0, 1, 2]);
    let model = outcome.model_facts();

    assert!(outcome.recovered_visibility_match_cases() > 0);
    assert!(model.checkpoint_watermark() <= model.visible_version());
    assert!(model.flush_watermark() <= model.visible_version());
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_generated_script_deletion_set_is_subset_of_model_proof() {
    let outcome = generated_script_outcome(&[0, 3, 4, 5]);
    let model = outcome.model_facts();

    assert!(outcome.deletion_subset_check_cases() > 0);
    assert_eq!(model.purged_objects(), 1);
    assert!(model.purged_objects() <= model.reclaim_proofed_objects());
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_generated_script_watermarks_are_monotonic() {
    let outcome = generated_script_outcome(&[0, 0, 1, 0, 2]);
    let model = outcome.model_facts();

    assert!(outcome.watermark_monotonic_check_cases() > 0);
    assert!(model.checkpoint_watermark() <= model.visible_version());
    assert!(model.flush_watermark() <= model.visible_version());
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_generated_script_close_is_idempotent_after_success() {
    let outcome = generated_script_outcome(&[0, 8, 8]);
    let model = outcome.model_facts();

    assert!(outcome.close_idempotence_check_cases() > 0);
    assert!(model.closed());
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_generated_script_cache_mode_never_claims_durable_recovery() {
    let outcome = generated_script_outcome(&[0, 0, 0, 8]);
    let model = outcome.model_facts();

    assert!(outcome.cache_no_durable_claim_check_cases() > 0);
    assert_eq!(model.wal_records(), 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_generated_script_lossy_recovery_records_degraded_health() {
    let outcome = generated_script_outcome(&[1, 3, 4]);
    let model = outcome.model_facts();

    assert!(outcome.lossy_degraded_health_check_cases() > 0);
    assert!(model.degraded_health());
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
fn generated_script_outcome(
    script: &[u8],
) -> strata_storage::testkit::LifecycleGeneratedScriptOutcome {
    strata_storage::testkit::check_lifecycle_generated_script_contract(script)
        .expect("generated lifecycle script contract")
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
fn rich_generated_script() -> &'static [u8] {
    &[0, 0, 1, 2, 3, 4, 5, 6, 7, 8, 9]
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
fn generated_script_categories_exercised(
    outcome: &strata_storage::testkit::LifecycleGeneratedScriptOutcome,
) -> bool {
    outcome.input_open_recovery_close_route_cases() > 0
        && outcome.input_maintenance_route_cases() > 0
        && outcome.input_reclaim_route_cases() > 0
        && outcome.validation_only_rejection_cases() > 0
        && outcome.recovered_visibility_match_cases() > 0
        && outcome.deletion_subset_check_cases() > 0
        && outcome.watermark_monotonic_check_cases() > 0
        && outcome.close_idempotence_check_cases() > 0
        && outcome.cache_no_durable_claim_check_cases() > 0
        && outcome.lossy_degraded_health_check_cases() > 0
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
fn all_categories_exercised(outcome: &strata_storage::testkit::LifecycleScaffoldOutcome) -> bool {
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
