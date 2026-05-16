//! Durable service fault-window harness entry point.

#![cfg(feature = "fault-injection")]
#![deny(unsafe_code)]

mod common;

#[test]
fn service_fault_window_harness_exercises_l4_service_faults() {
    let outcome = strata_storage_next::testkit::run_service_fault_window_harness()
        .expect("service fault-window harness");

    assert_eq!(outcome.cases_executed(), 3);
    assert!(common::crate_root().join("src/service/mod.rs").is_file());
}
