//! Lifecycle and recovery harness entry point.

#![deny(unsafe_code)]

mod common;

#[test]
fn lifecycle_recovery_harness_has_lifecycle_module() {
    assert!(common::crate_root().join("src/lifecycle/mod.rs").is_file());
}

#[test]
fn lifecycle_recovery_harness_has_dedicated_bootstrap_module() {
    let root = common::crate_root();
    let bootstrap = std::fs::read_to_string(root.join("src/lifecycle/durable/bootstrap.rs"))
        .expect("bootstrap module");
    assert!(bootstrap.contains("complete_recovery"));
    assert!(bootstrap.contains("CommitReplayRuntime"));
    assert!(bootstrap.contains("catch_up_to_recovered_version"));
    assert!(bootstrap.contains("catch_up_visible_after_replay"));

    let durable =
        std::fs::read_to_string(root.join("src/lifecycle/durable.rs")).expect("durable module");
    assert!(!durable.contains("CommitReplayRuntime"));
    assert!(!durable.contains("catch_up_to_recovered_version"));
    assert!(!durable.contains("catch_up_visible_after_replay"));
}

#[cfg(feature = "testkit")]
#[test]
fn lifecycle_recovery_contract_exercises_storage_recovery_paths() {
    let outcome = strata_storage_next::testkit::check_lifecycle_recovery_contract(b"recovery-seed")
        .expect("lifecycle recovery contract");

    assert!(outcome.empty_recovery_cases() > 0);
    assert!(outcome.checkpoint_recovery_cases() > 0);
    assert!(outcome.log_tail_cases() > 0);
    assert!(outcome.strict_failure_cases() > 0);
    assert!(outcome.lossy_degradation_cases() > 0);
    assert!(outcome.input_derived_empty_cases() > 0);
    assert!(outcome.input_derived_checkpoint_cases() > 0);
    assert!(outcome.input_derived_log_tail_cases() > 0);
    assert!(outcome.input_derived_strict_failure_cases() > 0);
    assert!(outcome.input_derived_lossy_degradation_cases() > 0);
    assert!(outcome.table_manifest_published_cases() > 0);
    assert!(outcome.table_manifest_recovered_cases() > 0);
    assert!(outcome.table_manifest_corrupt_cases() > 0);
    assert!(outcome.table_object_missing_cases() > 0);
    assert!(outcome.table_object_mismatch_cases() > 0);
    assert!(outcome.orphan_ignored_cases() > 0);
}

#[cfg(feature = "testkit")]
#[test]
fn lifecycle_bootstrap_contract_exercises_commit_bootstrap_paths() {
    let outcome =
        strata_storage_next::testkit::check_lifecycle_bootstrap_contract(b"bootstrap-seed")
            .expect("lifecycle bootstrap contract");

    assert!(outcome.empty_bootstrap_cases() > 0);
    assert!(outcome.checkpoint_bootstrap_cases() > 0);
    assert!(outcome.wal_replay_bootstrap_cases() > 0);
    assert!(outcome.degraded_bootstrap_cases() > 0);
    assert!(outcome.replay_rejection_cases() > 0);
    assert!(outcome.input_derived_empty_bootstrap_cases() > 0);
    assert!(outcome.input_derived_checkpoint_bootstrap_cases() > 0);
    assert!(outcome.input_derived_wal_replay_bootstrap_cases() > 0);
    assert!(outcome.input_derived_degraded_bootstrap_cases() > 0);
    assert!(outcome.input_derived_replay_rejection_cases() > 0);
}

#[cfg(feature = "testkit")]
#[test]
fn generated_recovery_empty_checkpoint_tail_and_lossy_routes_are_input_driven() {
    let outcome = strata_storage_next::testkit::check_lifecycle_generated_script_contract(
        b"recovery-input-routes",
    )
    .expect("generated lifecycle contract");

    assert!(outcome.input_open_recovery_close_route_cases() > 0);
    assert!(outcome.lossy_degraded_health_check_cases() > 0);
}

#[cfg(feature = "testkit")]
#[test]
fn generated_recovery_corrupt_manifest_snapshot_wal_and_table_are_typed() {
    let corrupt_log =
        strata_storage_next::testkit::check_lifecycle_fault_contract(b"fault-corrupt-log")
            .expect("corrupt-log fault contract");
    let partial_log =
        strata_storage_next::testkit::check_lifecycle_fault_contract(b"fault-partial-log-strict")
            .expect("partial-log fault contract");

    assert!(corrupt_log.corrupt_log_typed_cases() > 0);
    assert!(partial_log.partial_log_strict_cases() > 0);
}

#[cfg(feature = "testkit")]
#[test]
fn generated_bootstrap_catches_allocator_timestamp_and_visible_facts() {
    let outcome =
        strata_storage_next::testkit::check_lifecycle_bootstrap_contract(b"bootstrap-facts")
            .expect("bootstrap contract");

    assert!(outcome.checkpoint_bootstrap_cases() > 0);
    assert!(outcome.wal_replay_bootstrap_cases() > 0);
}

#[cfg(feature = "testkit")]
#[test]
fn generated_bootstrap_rejects_timeline_mismatch() {
    let outcome =
        strata_storage_next::testkit::check_lifecycle_bootstrap_contract(b"timeline-mismatch")
            .expect("bootstrap contract");

    assert!(outcome.replay_rejection_cases() > 0);
}

#[cfg(feature = "testkit")]
#[test]
fn generated_bootstrap_reconciles_unresolved_durable_gate() {
    let replay_failed =
        strata_storage_next::testkit::check_lifecycle_fault_contract(b"fault-replay-failed")
            .expect("replay-failed fault contract");
    let replay_visible =
        strata_storage_next::testkit::check_lifecycle_fault_contract(b"fault-replay-visible")
            .expect("replay-visible fault contract");

    assert!(replay_failed.replay_failed_state_cases() > 0);
    assert!(replay_visible.replay_visible_debt_cases() > 0);
}

#[cfg(feature = "testkit")]
#[test]
fn generated_recovery_health_matches_fault_family_model() {
    let outcome =
        strata_storage_next::testkit::check_lifecycle_generated_script_contract(b"health-model")
            .expect("generated lifecycle contract");

    assert!(outcome.recovered_visibility_match_cases() > 0);
    assert!(outcome.lossy_degraded_health_check_cases() > 0);
}
