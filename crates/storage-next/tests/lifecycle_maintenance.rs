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

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_executor_enqueue_and_coalesce_integration() {
    let outcome = strata_storage_next::testkit::check_lifecycle_maintenance_contract(
        b"maintenance-enqueue-integration",
    )
    .expect("maintenance contract");

    assert!(outcome.input_enqueue_cases() > 0);
    assert!(outcome.input_coalesce_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_executor_drain_cancel_and_fault_integration() {
    let outcome = strata_storage_next::testkit::check_lifecycle_maintenance_contract(
        b"maintenance-drain-fault-integration",
    )
    .expect("maintenance contract");

    assert!(outcome.input_cancel_cases() > 0);
    assert!(outcome.input_drain_cases() > 0);
    assert!(outcome.input_fault_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_maintenance_contract_covers_flush_categories() {
    let outcome =
        strata_storage_next::testkit::check_lifecycle_flush_contract(b"flush-contract-seed")
            .expect("flush contract");

    assert!(outcome.cache_success_cases() > 0);
    assert!(outcome.durable_success_cases() > 0);
    assert!(outcome.deferred_cases() > 0);
    assert!(outcome.publish_failure_cases() > 0);
    assert!(outcome.reopen_failure_cases() > 0);
    assert!(outcome.retry_cases() > 0);
    assert!(outcome.read_parity_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_flush_cache_and_durable_integration() {
    let outcome =
        strata_storage_next::testkit::check_lifecycle_flush_contract(b"flush-success-integration")
            .expect("flush contract");

    assert!(outcome.cache_success_cases() > 0);
    assert!(outcome.durable_success_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_flush_failure_integration() {
    let outcome =
        strata_storage_next::testkit::check_lifecycle_flush_contract(b"flush-failure-integration")
            .expect("flush contract");

    assert!(outcome.publish_failure_cases() > 0);
    assert!(outcome.reopen_failure_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_flush_retry_and_read_parity_integration() {
    let outcome =
        strata_storage_next::testkit::check_lifecycle_flush_contract(b"flush-retry-integration")
            .expect("flush contract");

    assert!(outcome.retry_cases() > 0);
    assert!(outcome.read_parity_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_checkpoint_publication_integration() {
    let outcome = strata_storage_next::testkit::check_lifecycle_checkpoint_contract(
        b"checkpoint-publication-integration",
    )
    .expect("checkpoint contract");

    assert!(outcome.accepted_request_cases() > 0);
    assert!(outcome.active_row_cases() > 0);
    assert!(outcome.frozen_row_cases() > 0);
    assert!(outcome.owned_row_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_checkpoint_failure_and_retention_integration() {
    let outcome = strata_storage_next::testkit::check_lifecycle_checkpoint_contract(
        b"checkpoint-failure-retention-integration",
    )
    .expect("checkpoint contract");

    assert!(outcome.partial_window_cases() > 0);
    assert!(outcome.delete_failure_cases() > 0);
    assert!(outcome.retention_accept_cases() > 0);
    assert!(outcome.retention_reject_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_table_rewrite_compaction_integration() {
    let outcome = strata_storage_next::testkit::check_lifecycle_table_rewrite_contract(
        b"table-rewrite-compaction-integration",
    )
    .expect("table rewrite contract");

    assert!(outcome.cache_compaction_cases() > 0);
    assert!(outcome.durable_compaction_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_table_rewrite_materialization_integration() {
    let outcome = strata_storage_next::testkit::check_lifecycle_table_rewrite_contract(
        b"table-rewrite-materialization-integration",
    )
    .expect("table rewrite contract");

    assert!(outcome.materialization_cases() > 0);
    assert!(outcome.pressure_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_retention_proof_integration() {
    let outcome =
        strata_storage_next::testkit::check_lifecycle_retention_contract(b"retention-proof");

    let outcome = outcome.expect("retention contract");
    assert!(outcome.complete_proof_cases() > 0);
    assert!(outcome.incomplete_proof_cases() > 0);
    assert!(outcome.blocked_recovery_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_snapshot_pruning_integration() {
    let outcome =
        strata_storage_next::testkit::check_lifecycle_retention_contract(b"snapshot-pruning");

    let outcome = outcome.expect("retention contract");
    assert!(outcome.snapshot_pruned_cases() > 0);
    assert!(outcome.snapshot_protected_cases() > 0);
    assert!(outcome.snapshot_delete_failure_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_table_retention_delegation_integration() {
    let outcome =
        strata_storage_next::testkit::check_lifecycle_retention_contract(b"table-delegation");

    let outcome = outcome.expect("retention contract");
    assert!(outcome.table_retained_cases() > 0);
    assert!(outcome.table_quarantine_candidate_cases() > 0);
    assert!(outcome.wal_delegated_cases() > 0);
    assert!(outcome.cache_deferred_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_retention_blocks_unsafe_recovery_integration() {
    let outcome =
        strata_storage_next::testkit::check_lifecycle_retention_contract(b"retention-blocked");

    let outcome = outcome.expect("retention contract");
    assert!(outcome.blocked_recovery_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_table_retention_delegates_to_quarantine_integration() {
    let outcome =
        strata_storage_next::testkit::check_lifecycle_retention_contract(b"table-quarantine");

    let outcome = outcome.expect("retention contract");
    assert!(outcome.table_quarantine_candidate_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_snapshot_pruning_delete_failure_integration() {
    let outcome = strata_storage_next::testkit::check_lifecycle_retention_contract(
        b"snapshot-delete-failure",
    );

    let outcome = outcome.expect("retention contract");
    assert!(outcome.snapshot_delete_failure_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_retention_generated_bytes_influence_decision_counts() {
    let low_retain = strata_storage_next::testkit::check_lifecycle_retention_contract(&[0, 0])
        .expect("low retain contract");
    let high_retain = strata_storage_next::testkit::check_lifecycle_retention_contract(&[0, 2])
        .expect("high retain contract");

    assert_ne!(
        low_retain.snapshot_pruned_cases(),
        high_retain.snapshot_pruned_cases()
    );
}

#[cfg(not(all(feature = "testkit", not(target_arch = "wasm32"))))]
#[test]
fn lifecycle_maintenance_contract_requires_testkit() {}
