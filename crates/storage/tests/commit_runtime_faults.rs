//! Commit runtime fault harness entry point.

#![cfg(feature = "fault-injection")]
#![deny(unsafe_code)]

use std::num::NonZeroU64;
use strata_storage::testkit::{
    check_commit_runtime_fault_contract, check_commit_runtime_scaffold_contract, BackendOperation,
    FaultKind, FaultRule, FaultScript,
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

    let generated = check_commit_runtime_fault_contract(&generated_fault_route_seed())
        .expect("generated commit runtime fault script");
    assert!(generated.wal_failures >= 4);
    assert!(generated.post_wal_failures >= 2);
    assert!(generated.clean_wal_failures > 0);
    assert!(generated.uncertain_wal_failures > 0);
    assert!(generated.writer_halted_failures > 0);
    assert!(generated.segment_id_overflow_failures > 0);
    assert!(generated.apply_after_wal_failures > 0);
    assert!(generated.visible_after_apply_failures > 0);
    assert!(generated.replay_successes >= 2);
    assert!(generated.guard_or_quiesce_rejections > 0);
    assert!(generated.branch_lifecycle_transitions > 0);
    assert!(generated.branch_lifecycle_rejections > 0);
}

fn generated_fault_route_seed() -> Vec<u8> {
    let mut seed = Vec::new();
    for [selector, branch, key, value] in [
        [3, 1, 3, 1],
        [3, 1, 4, 2],
        [3, 3, 7, 3],
        [3, 3, 8, 4],
        [3, 1, 5, 5],
        [11, 0, 0, 0],
        [3, 2, 6, 6],
        [11, 0, 0, 0],
        [7, 0, 0, 0],
        [9, 0, 0, 0],
        [8, 0, 0, 0],
        [0, 4, 9, 0x41],
        [12, 4, 0, 0],
        [13, 4, 0, 0],
        [14, 4, 0, 0],
        [14, 4, 0, 0],
    ] {
        seed.extend_from_slice(&[selector, branch, key, value]);
    }
    seed
}
