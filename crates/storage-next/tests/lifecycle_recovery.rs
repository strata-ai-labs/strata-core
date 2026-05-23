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
    let outcome = strata_storage_next::testkit::check_lifecycle_recovery_contract(b"l8f-recovery")
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
}

#[cfg(feature = "testkit")]
#[test]
fn lifecycle_bootstrap_contract_exercises_commit_bootstrap_paths() {
    let outcome =
        strata_storage_next::testkit::check_lifecycle_bootstrap_contract(b"l8g-bootstrap")
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
