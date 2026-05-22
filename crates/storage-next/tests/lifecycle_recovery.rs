//! Lifecycle and recovery harness entry point.

#![deny(unsafe_code)]

mod common;

#[test]
fn lifecycle_recovery_harness_has_lifecycle_module() {
    assert!(common::crate_root().join("src/lifecycle/mod.rs").is_file());
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
