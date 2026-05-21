//! Commit runtime fault harness entry point.

#![cfg(feature = "fault-injection")]
#![deny(unsafe_code)]

mod common;

use std::num::NonZeroU64;
use strata_storage_next::testkit::{
    check_commit_runtime_scaffold_contract, BackendOperation, FaultKind, FaultRule, FaultScript,
};

#[test]
fn commit_runtime_fault_harness_exercises_protocol_boundaries() {
    let call = NonZeroU64::new(1).expect("non-zero call number");
    let script = FaultScript::new([FaultRule::new(
        BackendOperation::ReadObject,
        call,
        FaultKind::Unavailable,
    )]);

    assert_ne!(script, FaultScript::empty());
    assert!(common::crate_root().join("src/commit/mod.rs").is_file());

    let outcome = check_commit_runtime_scaffold_contract(b"commit-runtime-fault-boundaries-v1")
        .expect("commit runtime fault contract");
    assert!(outcome.cache_branch_admission_rejection_cases() > 0);
    assert!(outcome.cache_conflict_rejection_cases() > 0);
    assert!(outcome.cache_applied_above_visible_rejection_cases() > 0);
    assert!(outcome.durable_clean_wal_failure_cases() > 0);
    assert!(outcome.durable_uncertain_wal_failure_cases() > 0);
    assert!(outcome.durable_guard_release_after_failure_cases() > 0);
    assert!(outcome.durable_not_applied_gate_cases() > 0);
    assert!(outcome.durable_applied_not_visible_gate_cases() > 0);
    assert!(outcome.durable_unresolved_gate_block_cases() > 0);
    assert!(outcome.durable_unresolved_gate_cache_block_cases() > 0);
    assert!(outcome.durable_clean_wal_no_gate_cases() > 0);
    assert!(outcome.durable_uncertain_wal_no_gate_cases() > 0);
}
