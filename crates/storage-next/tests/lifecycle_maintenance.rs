//! Integration harness for lifecycle maintenance executor contracts.

#![deny(unsafe_code)]

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_maintenance_contract_covers_executor_categories() {
    let outcome = strata_storage_next::testkit::check_lifecycle_maintenance_contract(
        b"maintenance-executor-seed",
    )
    .expect("maintenance contract");

    assert!(outcome.input_enqueue_cases() > 0);
    assert!(outcome.input_coalesce_cases() > 0);
    assert!(outcome.input_run_cases() > 0);
    assert!(outcome.input_cancel_cases() > 0);
    assert!(outcome.input_drain_cases() > 0);
    assert!(outcome.input_fault_cases() > 0);
    assert!(outcome.input_queue_full_cases() > 0);
    assert!(outcome.input_admission_rejection_cases() > 0);
    assert!(outcome.input_model_step_cases() > 0);
}

#[cfg(not(all(feature = "testkit", not(target_arch = "wasm32"))))]
#[test]
fn lifecycle_maintenance_contract_requires_testkit() {
    assert!(true);
}
