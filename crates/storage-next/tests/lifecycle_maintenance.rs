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
fn lifecycle_quarantine_integration() {
    let outcome =
        strata_storage_next::testkit::check_lifecycle_quarantine_contract(b"quarantine-flow")
            .expect("quarantine contract");

    assert!(outcome.complete_safe_proof_cases() > 0);
    assert!(outcome.staged_object_cases() > 0);
    assert!(outcome.already_quarantined_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_close_contract_covers_shutdown_categories() {
    let outcome =
        strata_storage_next::testkit::check_lifecycle_close_contract(b"close-shutdown-categories")
            .expect("close contract");

    assert!(outcome.close_requested_cases() > 0);
    assert!(outcome.cache_close_completed_cases() > 0);
    assert!(outcome.durable_close_completed_cases() > 0);
    assert!(outcome.idempotent_close_cases() > 0);
    assert!(outcome.retryable_timeout_cases() > 0);
    assert!(outcome.drain_required_completed_cases() > 0);
    assert!(outcome.cancelable_task_canceled_cases() > 0);
    assert!(outcome.ordinary_task_not_started_after_close_cases() > 0);
    assert!(outcome.commit_quiesce_acquired_cases() > 0);
    assert!(outcome.commit_quiesce_blocked_cases() > 0);
    assert!(outcome.wal_sync_failure_cases() > 0);
    assert!(outcome.manifest_sync_failure_cases() > 0);
    assert!(outcome.guard_release_observed_cases() > 0);
    assert!(outcome.source_chain_preserved_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_purge_integration() {
    let outcome = strata_storage_next::testkit::check_lifecycle_quarantine_contract(b"purge-flow")
        .expect("quarantine contract");

    assert!(outcome.purged_object_cases() > 0);
    assert!(outcome.purge_delete_failure_cases() > 0);
    assert!(outcome.stale_purge_proof_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_repair_reconciliation_integration() {
    let outcome = strata_storage_next::testkit::check_lifecycle_quarantine_contract(b"repair-flow")
        .expect("quarantine contract");

    assert!(outcome.corrupt_inventory_repair_cases() > 0);
    assert!(outcome.unlisted_object_repair_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_reclaim_blocks_unsafe_recovery_integration() {
    let outcome =
        strata_storage_next::testkit::check_lifecycle_quarantine_contract(b"blocked-reclaim")
            .expect("quarantine contract");

    assert!(outcome.blocked_recovery_cases() > 0);
    assert!(outcome.referenced_candidate_cases() > 0);
    assert!(outcome.incomplete_proof_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_cache_reclaim_unsupported_integration() {
    let outcome =
        strata_storage_next::testkit::check_lifecycle_quarantine_contract(b"cache-reclaim")
            .expect("quarantine contract");

    assert!(outcome.cache_deferred_cases() >= 3);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_quarantine_then_purge_round_trip() {
    let outcome = strata_storage_next::testkit::check_lifecycle_quarantine_contract(b"round-trip")
        .expect("quarantine contract");

    assert!(outcome.staged_object_cases() > 0);
    assert!(outcome.purged_object_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_quarantine_publish_failure_surfaces_health_debt() {
    let outcome =
        strata_storage_next::testkit::check_lifecycle_quarantine_contract(b"publish-faults")
            .expect("quarantine contract");

    assert!(outcome.inventory_publish_failure_cases() > 0);
    assert!(outcome.quarantine_publish_failure_cases() > 0);
    assert!(outcome.source_delete_failure_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_quarantine_generated_bytes_influence_routes() {
    let staging = strata_storage_next::testkit::check_lifecycle_quarantine_contract(&[
        0, 1, 2, 3, 4, 0, 1, 8, 1, 0, 0, 0, 0, 0,
    ])
    .expect("staging contract");
    let purge = strata_storage_next::testkit::check_lifecycle_quarantine_contract(&[
        0, 1, 2, 3, 4, 1, 1, 8, 1, 0, 0, 0, 0, 0,
    ])
    .expect("purge contract");

    assert!(staging.input_derived_route_cases() > 0);
    assert!(purge.input_derived_route_cases() > 0);
    assert_ne!(staging.staged_object_cases(), purge.staged_object_cases());
    assert_ne!(staging.purged_object_cases(), purge.purged_object_cases());
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

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_generated_integration_runs_default_mode_script() {
    let outcome =
        strata_storage_next::testkit::check_lifecycle_generated_script_contract(b"default-script")
            .expect("generated lifecycle contract");

    assert!(outcome.input_open_recovery_close_route_cases() > 0);
    assert!(outcome.cache_no_durable_claim_check_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_generated_integration_runs_durable_mode_script() {
    let outcome =
        strata_storage_next::testkit::check_lifecycle_generated_script_contract(b"durable-script")
            .expect("generated lifecycle contract");

    assert!(outcome.recovered_visibility_match_cases() > 0);
    assert!(outcome.watermark_monotonic_check_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_generated_integration_runs_reclaim_close_script() {
    let outcome = strata_storage_next::testkit::check_lifecycle_generated_script_contract(
        b"reclaim-close-script",
    )
    .expect("generated lifecycle contract");

    assert!(outcome.input_reclaim_route_cases() > 0);
    assert!(outcome.close_idempotence_check_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_fault_integration_covers_all_phase_families() {
    let capability =
        strata_storage_next::testkit::check_lifecycle_fault_contract(b"fault-capability")
            .expect("capability fault lifecycle contract");
    let flush = strata_storage_next::testkit::check_lifecycle_fault_contract(b"fault-flush-orphan")
        .expect("flush fault lifecycle contract");
    let close =
        strata_storage_next::testkit::check_lifecycle_fault_contract(b"fault-close-wal-sync")
            .expect("close fault lifecycle contract");

    assert!(capability.capability_preflight_cases() > 0);
    assert!(flush.flush_orphan_table_cases() > 0);
    assert!(close.close_log_sync_source_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn lifecycle_crash_integration_reports_case_counts() {
    let replay = strata_storage_next::testkit::check_lifecycle_crash_contract(b"crash-wal-append")
        .expect("replay crash lifecycle contract");
    let snapshot =
        strata_storage_next::testkit::check_lifecycle_crash_contract(b"crash-orphan-snapshot")
            .expect("snapshot crash lifecycle contract");
    let close = strata_storage_next::testkit::check_lifecycle_crash_contract(b"crash-close-reopen")
        .expect("close crash lifecycle contract");

    assert!(replay.log_append_replay_cases() > 0);
    assert!(snapshot.orphan_snapshot_ignored_cases() > 0);
    assert!(close.close_reopen_consistent_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn generated_maintenance_model_matches_enqueue_coalesce_run_cancel_drain() {
    let outcome = strata_storage_next::testkit::check_lifecycle_maintenance_contract(b"model");

    let outcome = outcome.expect("maintenance contract");
    assert!(outcome.input_model_step_cases() > 0);
    assert!(outcome.input_drain_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn generated_maintenance_queue_full_and_admission_rejections_are_typed() {
    let outcome = strata_storage_next::testkit::check_lifecycle_maintenance_contract(b"queue");

    let outcome = outcome.expect("maintenance contract");
    assert!(outcome.input_queue_full_cases() > 0);
    assert!(outcome.input_admission_rejection_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn generated_flush_preserves_read_parity_and_candidate_watermark() {
    let outcome = strata_storage_next::testkit::check_lifecycle_flush_contract(b"flush-parity")
        .expect("flush contract");

    assert!(outcome.read_parity_cases() > 0);
    assert!(outcome.retry_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn generated_flush_publication_failure_keeps_branch_state_safe() {
    let outcome = strata_storage_next::testkit::check_lifecycle_flush_contract(b"flush-failure")
        .expect("flush contract");

    assert!(outcome.publish_failure_cases() > 0);
    assert!(outcome.reopen_failure_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn generated_checkpoint_preserves_row_visibility_and_tail_replay() {
    let outcome =
        strata_storage_next::testkit::check_lifecycle_checkpoint_contract(b"checkpoint-tail")
            .expect("checkpoint contract");

    assert!(outcome.checkpoint_truncation_round_trip_cases() > 0);
    assert!(outcome.timeline_row_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn generated_checkpoint_truncation_never_removes_uncovered_wal_records() {
    let outcome =
        strata_storage_next::testkit::check_lifecycle_generated_script_contract(b"truncation")
            .expect("generated lifecycle contract");

    assert!(outcome.watermark_monotonic_check_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn generated_table_rewrite_preserves_reads_after_compaction() {
    let outcome =
        strata_storage_next::testkit::check_lifecycle_table_rewrite_contract(b"compaction")
            .expect("table rewrite contract");

    assert!(outcome.cache_compaction_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn generated_materialization_preserves_child_precedence() {
    let outcome =
        strata_storage_next::testkit::check_lifecycle_table_rewrite_contract(b"materialization")
            .expect("table rewrite contract");

    assert!(outcome.materialization_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn generated_storage_pressure_suggestions_are_model_consistent() {
    let outcome = strata_storage_next::testkit::check_lifecycle_table_rewrite_contract(b"pressure")
        .expect("table rewrite contract");

    assert!(outcome.pressure_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn generated_retention_proof_blocks_unsafe_recovery_health() {
    let outcome = strata_storage_next::testkit::check_lifecycle_retention_contract(b"blocked")
        .expect("retention contract");

    assert!(outcome.blocked_recovery_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn generated_retention_never_deletes_reachable_tables_or_live_snapshots() {
    let outcome =
        strata_storage_next::testkit::check_lifecycle_generated_script_contract(b"safe-delete")
            .expect("generated lifecycle contract");

    assert!(outcome.deletion_subset_check_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn generated_snapshot_pruning_retains_live_and_newest_snapshots() {
    let outcome = strata_storage_next::testkit::check_lifecycle_retention_contract(b"snapshots")
        .expect("retention contract");

    assert!(outcome.snapshot_protected_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn generated_quarantine_happens_before_purge() {
    let outcome =
        strata_storage_next::testkit::check_lifecycle_quarantine_contract(b"quarantine-purge")
            .expect("quarantine contract");

    assert!(outcome.staged_object_cases() > 0);
    assert!(outcome.purged_object_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn generated_purge_requires_fresh_inventory_proof() {
    let outcome = strata_storage_next::testkit::check_lifecycle_quarantine_contract(b"stale-proof")
        .expect("quarantine contract");

    assert!(outcome.stale_purge_proof_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn generated_repair_reports_inconclusive_without_mutating_state() {
    let outcome = strata_storage_next::testkit::check_lifecycle_quarantine_contract(b"repair")
        .expect("quarantine contract");

    assert!(outcome.unlisted_object_repair_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn generated_close_blocks_new_commits_and_ordinary_maintenance() {
    let outcome = strata_storage_next::testkit::check_lifecycle_close_contract(b"close-block")
        .expect("close contract");

    assert!(outcome.ordinary_task_not_started_after_close_cases() > 0);
    assert!(outcome.commit_quiesce_blocked_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn generated_close_retry_and_double_close_match_model() {
    let outcome = strata_storage_next::testkit::check_lifecycle_close_contract(b"close-retry")
        .expect("close contract");

    assert!(outcome.idempotent_close_cases() > 0);
    assert!(outcome.retryable_timeout_cases() > 0);
}

#[cfg(all(feature = "testkit", not(target_arch = "wasm32")))]
#[test]
fn generated_close_faults_preserve_health_debt() {
    let wal = strata_storage_next::testkit::check_lifecycle_fault_contract(b"fault-close-wal-sync")
        .expect("wal fault contract");
    let manifest =
        strata_storage_next::testkit::check_lifecycle_fault_contract(b"fault-close-manifest")
            .expect("manifest fault contract");

    assert!(wal.close_log_sync_source_cases() > 0);
    assert!(manifest.close_manifest_sync_debt_cases() > 0);
}

#[cfg(not(all(feature = "testkit", not(target_arch = "wasm32"))))]
#[test]
fn lifecycle_maintenance_contract_requires_testkit() {}
