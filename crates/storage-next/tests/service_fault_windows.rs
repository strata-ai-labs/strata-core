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
    assert_eq!(outcome.cases_executed(), 19);
    // Spot-check a few of the per-route counters to confirm individual
    // injections actually fired, not just the aggregate.
    assert!(outcome.capability_preflight_cases() > 0);
    assert!(outcome.snapshot_orphan_cases() > 0);
    assert!(outcome.flush_orphan_table_cases() > 0);
    assert!(outcome.quarantine_publish_blocked_purge_cases() > 0);
    assert!(outcome.close_log_sync_source_cases() > 0);
    assert!(common::crate_root().join("src/service/mod.rs").is_file());
}
