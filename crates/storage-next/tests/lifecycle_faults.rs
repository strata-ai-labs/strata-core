//! Lifecycle fault-window integration checks.

#![deny(unsafe_code)]

#[cfg(all(
    feature = "testkit",
    feature = "fault-injection",
    not(target_arch = "wasm32")
))]
fn fault_outcome(script: &[u8]) -> strata_storage_next::testkit::LifecycleFaultContractOutcome {
    strata_storage_next::testkit::check_lifecycle_fault_contract(script)
        .expect("lifecycle fault contract")
}

#[cfg(all(
    feature = "testkit",
    feature = "fault-injection",
    not(target_arch = "wasm32")
))]
#[test]
fn fault_capability_mismatch_happens_before_durable_side_effects() {
    assert!(fault_outcome(b"fault-capability").capability_preflight_cases() > 0);
}

#[cfg(all(
    feature = "testkit",
    feature = "fault-injection",
    not(target_arch = "wasm32")
))]
#[test]
fn fault_writer_guard_acquired_then_manifest_create_fails_releases_or_reports_guard() {
    assert!(fault_outcome(b"fault-writer-guard").writer_guard_manifest_create_cases() > 0);
}

#[cfg(all(
    feature = "testkit",
    feature = "fault-injection",
    not(target_arch = "wasm32")
))]
#[test]
fn fault_manifest_create_visible_but_publish_uncertain_records_health_debt() {
    assert!(fault_outcome(b"fault-manifest-publish").manifest_publish_uncertain_cases() > 0);
}

#[cfg(all(
    feature = "testkit",
    feature = "fault-injection",
    not(target_arch = "wasm32")
))]
#[test]
fn fault_snapshot_published_manifest_update_fails_records_orphan_snapshot() {
    assert!(fault_outcome(b"fault-snapshot-orphan").snapshot_orphan_cases() > 0);
}

#[cfg(all(
    feature = "testkit",
    feature = "fault-injection",
    not(target_arch = "wasm32")
))]
#[test]
fn fault_manifest_updated_wal_truncation_fails_keeps_checkpoint_success() {
    assert!(fault_outcome(b"fault-wal-truncation").checkpoint_truncation_debt_cases() > 0);
}

#[cfg(all(
    feature = "testkit",
    feature = "fault-injection",
    not(target_arch = "wasm32")
))]
#[test]
fn fault_partial_wal_tail_strict_fails_before_repair() {
    assert!(fault_outcome(b"fault-partial-log-strict").partial_log_strict_cases() > 0);
}

#[cfg(all(
    feature = "testkit",
    feature = "fault-injection",
    not(target_arch = "wasm32")
))]
#[test]
fn fault_partial_wal_tail_lossy_repairs_and_degrades_health() {
    assert!(fault_outcome(b"fault-partial-log-lossy").partial_log_lossy_cases() > 0);
}

#[cfg(all(
    feature = "testkit",
    feature = "fault-injection",
    not(target_arch = "wasm32")
))]
#[test]
fn fault_corrupt_wal_returns_typed_recovery_error() {
    assert!(fault_outcome(b"fault-corrupt-log").corrupt_log_typed_cases() > 0);
}

#[cfg(all(
    feature = "testkit",
    feature = "fault-injection",
    not(target_arch = "wasm32")
))]
#[test]
fn fault_replay_failure_transitions_bootstrap_to_failed() {
    assert!(fault_outcome(b"fault-replay-failed").replay_failed_state_cases() > 0);
}

#[cfg(all(
    feature = "testkit",
    feature = "fault-injection",
    not(target_arch = "wasm32")
))]
#[test]
fn fault_replay_visible_publication_failure_records_durable_not_visible() {
    assert!(fault_outcome(b"fault-replay-visible").replay_visible_debt_cases() > 0);
}

#[cfg(all(
    feature = "testkit",
    feature = "fault-injection",
    not(target_arch = "wasm32")
))]
#[test]
fn fault_flush_table_published_branch_install_fails_reports_orphan_table() {
    assert!(fault_outcome(b"fault-flush-orphan").flush_orphan_table_cases() > 0);
}

#[cfg(all(
    feature = "testkit",
    feature = "fault-injection",
    not(target_arch = "wasm32")
))]
#[test]
fn fault_table_rewrite_branch_swap_failure_preserves_reads() {
    assert!(fault_outcome(b"fault-rewrite-preserved").rewrite_preserved_read_cases() > 0);
}

#[cfg(all(
    feature = "testkit",
    feature = "fault-injection",
    not(target_arch = "wasm32")
))]
#[test]
fn fault_incomplete_retention_proof_blocks_delete_before_backend_access() {
    assert!(fault_outcome(b"fault-retention-blocked").retention_blocked_delete_cases() > 0);
}

#[cfg(all(
    feature = "testkit",
    feature = "fault-injection",
    not(target_arch = "wasm32")
))]
#[test]
fn fault_quarantine_inventory_publish_failure_blocks_purge() {
    assert!(
        fault_outcome(b"fault-quarantine-publish").quarantine_publish_blocked_purge_cases() > 0
    );
}

#[cfg(all(
    feature = "testkit",
    feature = "fault-injection",
    not(target_arch = "wasm32")
))]
#[test]
fn fault_purge_delete_success_inventory_update_failure_preserves_debt() {
    assert!(fault_outcome(b"fault-purge-delete").purge_delete_debt_cases() > 0);
}

#[cfg(all(
    feature = "testkit",
    feature = "fault-injection",
    not(target_arch = "wasm32")
))]
#[test]
fn fault_close_quiesce_timeout_is_retryable() {
    assert!(fault_outcome(b"fault-close-quiesce").close_quiesce_timeout_cases() > 0);
}

#[cfg(all(
    feature = "testkit",
    feature = "fault-injection",
    not(target_arch = "wasm32")
))]
#[test]
fn fault_close_wal_sync_failure_preserves_source_chain() {
    assert!(fault_outcome(b"fault-close-wal-sync").close_log_sync_source_cases() > 0);
}

#[cfg(all(
    feature = "testkit",
    feature = "fault-injection",
    not(target_arch = "wasm32")
))]
#[test]
fn fault_close_manifest_sync_failure_preserves_final_fact_debt() {
    assert!(fault_outcome(b"fault-close-manifest").close_manifest_sync_debt_cases() > 0);
}

#[cfg(all(
    feature = "testkit",
    feature = "fault-injection",
    not(target_arch = "wasm32")
))]
#[test]
fn fault_writer_guard_release_failure_is_typed_when_backend_reports_it() {
    assert!(fault_outcome(b"fault-close-guard-release").writer_guard_release_typed_cases() > 0);
}

#[cfg(not(all(
    feature = "testkit",
    feature = "fault-injection",
    not(target_arch = "wasm32")
)))]
#[test]
fn lifecycle_fault_contract_requires_testkit_fault_injection() {}
