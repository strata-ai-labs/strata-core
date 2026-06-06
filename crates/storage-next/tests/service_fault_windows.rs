//! Durable service fault-window harness entry point.

#![cfg(feature = "fault-injection")]
#![deny(unsafe_code)]

mod common;

#[test]
fn service_fault_window_harness_exercises_l4_service_faults() {
    let outcome = strata_storage_next::testkit::run_service_fault_window_harness()
        .expect("service fault-window harness");

    // The harness now drives 19 distinct fault routes covering every
    // service-fault family enumerated by `LifecycleFaultContractOutcome`.
    // Each route is asserted by its own per-route counter in the
    // outcome; the aggregate `cases_executed` is the family count.
    assert_eq!(
        outcome.cases_executed(),
        strata_storage_next::testkit::ServiceFaultWindowHarnessOutcome::EXPECTED_CASES
    );
    for (case, count) in outcome.case_counts() {
        assert!(count > 0, "{case} service fault route did not execute");
    }
    assert!(common::crate_root().join("src/service/mod.rs").is_file());
}
