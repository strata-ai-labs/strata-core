use super::*;

fn open_runtime() -> StorageRuntime<'static> {
    StorageRuntime::open(StorageOpenOptions::default())
        .expect("open cache runtime")
        .into_runtime()
}

#[cfg(feature = "localfs")]
fn open_durable_runtime_with_options(
    name: &str,
    options: StorageOpenOptions,
) -> StorageRuntime<'static> {
    let backend = StorageBackend::local_fs(temp_dir_for_api_test(name));
    StorageRuntime::open_with_backend(options, Box::leak(Box::new(backend)))
        .expect("open durable runtime")
        .into_runtime()
}

#[cfg(feature = "localfs")]
fn open_durable_runtime_with_backend(
    name: &str,
    options: StorageOpenOptions,
) -> (&'static StorageBackend, StorageRuntime<'static>) {
    let backend = Box::leak(Box::new(StorageBackend::local_fs(temp_dir_for_api_test(
        name,
    ))));
    let runtime = StorageRuntime::open_with_backend(options, backend)
        .expect("open durable runtime")
        .into_runtime();
    (backend, runtime)
}

fn branch() -> BranchId {
    StorageRuntime::default_branch_id_for_test()
}

fn branch_with(byte: u8) -> BranchId {
    branch_id(byte)
}

fn engine_space() -> StorageSpaceId {
    StorageSpaceId::new(vec![0x20]).expect("engine storage space")
}

fn api_key(bytes: &[u8]) -> StorageKey {
    StorageKey::new(bytes.to_vec()).expect("valid key")
}

fn put_batch(key: &[u8], value: &[u8]) -> CommitBatch {
    put_batch_for(branch(), key, value)
}

fn put_batch_for(branch_id: BranchId, key: &[u8], value: &[u8]) -> CommitBatch {
    CommitBatch::new(
        branch_id,
        vec![CommitMutation::Put {
            storage_space: engine_space(),
            key: api_key(key),
            value: StorageValue::new(value.to_vec()),
            ttl: None,
        }],
        CommitOptions::default(),
    )
    .expect("valid put batch")
}

fn fork_branch(runtime: &mut StorageRuntime<'_>, child: BranchId) {
    runtime
        .branch(&BranchRequest::new(
            child,
            BranchAction::ForkCurrent { source: branch() },
            Some(BranchGeneration::new(1)),
        ))
        .expect("fork branch");
}

#[cfg(feature = "localfs")]
fn stage_quarantine_object(
    backend: &StorageBackend,
    branch_id: BranchId,
    table_id: &str,
    bytes: &[u8],
) {
    let source = crate::layout::ObjectLayout::table_object(&branch_id.to_string(), 0, table_id)
        .expect("source table object");
    backend
        .as_backend()
        .write_object(&source, bytes)
        .expect("write source table");
    let request = crate::lifecycle::LifecycleQuarantineRequest::from_source_object(
        branch_id,
        [0x53; 16],
        crate::lifecycle::LifecycleCodecId::identity(),
        source,
        Timestamp::from_micros(42),
        crate::lifecycle::LifecycleQuarantineProof::safe(crate::lifecycle::RecoveryHealth::Healthy),
    )
    .expect("quarantine request");
    let outcome = crate::lifecycle::quarantine_object(
        &crate::service::QuarantineService::new(backend.as_backend()),
        &request,
    );
    assert_eq!(
        outcome.status(),
        crate::lifecycle::LifecycleQuarantineStatus::QuarantinedSourceDeleted
    );
    assert!(outcome.inventory_object().is_some());
    assert!(outcome.quarantine_object().is_some());
}

#[test]
fn maintenance_request_snapshot_pruning_is_constructible() {
    let request =
        MaintenanceRequest::new(MaintenanceTask::SnapshotPruning, MaintenanceScope::Global);

    assert_eq!(request.task(), MaintenanceTask::SnapshotPruning);
    assert_eq!(request.scope(), MaintenanceScope::Global);
}

#[test]
fn api_maintenance_status_reports_empty_queue() {
    let runtime = open_runtime();

    let status = runtime.maintenance_status().expect("maintenance status");

    assert_eq!(status.pending_tasks(), 0);
    assert_eq!(status.active_task(), None);
    assert_eq!(status.enqueued(), 0);
}

#[test]
fn api_checkpoint_cache_mode_returns_deferred() {
    let mut runtime = open_runtime();
    let request = MaintenanceRequest::new(MaintenanceTask::Checkpoint, MaintenanceScope::Global);

    let outcome = runtime.maintenance(&request).expect("checkpoint outcome");

    assert_eq!(outcome.task(), MaintenanceTask::Checkpoint);
    assert_eq!(outcome.status(), MaintenanceSummaryStatus::Deferred);
    assert_eq!(
        outcome.reason_class(),
        Some(MaintenanceReasonClass::Deferred)
    );
    assert_eq!(outcome.affected_objects(), 0);
}

#[test]
#[cfg(feature = "localfs")]
fn api_checkpoint_returns_watermark_facts() {
    let mut runtime = open_durable_runtime_with_options(
        "maintenance-checkpoint-facts",
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
    );
    let commit = runtime
        .commit(&put_batch(b"checkpoint", b"value"))
        .expect("commit before checkpoint");
    let request = MaintenanceRequest::new(MaintenanceTask::Checkpoint, MaintenanceScope::Global);

    let outcome = runtime.maintenance(&request).expect("checkpoint outcome");

    assert_eq!(outcome.task(), MaintenanceTask::Checkpoint);
    assert_eq!(outcome.status(), MaintenanceSummaryStatus::Completed);
    assert_eq!(
        outcome.checkpoint_watermark(),
        Some(commit.commit_version())
    );
    assert!(outcome.snapshot_id().is_some());
    assert!(outcome.rows_processed() > 0);
    assert!(!outcome.wal_truncated());
    assert_eq!(outcome.source_error_code(), None);
}

#[test]
fn api_checkpoint_after_close_rejects() {
    let mut runtime = open_runtime();
    runtime.close().expect("close runtime");
    let request = MaintenanceRequest::new(MaintenanceTask::Checkpoint, MaintenanceScope::Global);

    let error = runtime
        .maintenance(&request)
        .expect_err("closed runtime rejects checkpoint");

    assert_eq!(error.class(), StorageApiErrorClass::FailedPrecondition);
    assert_eq!(error.code(), "failed_precondition.storage_api.state");
}

#[test]
fn api_flush_returns_publication_facts() {
    let mut runtime = open_runtime();
    runtime
        .commit(&put_batch(b"key", b"value"))
        .expect("commit");
    let request =
        MaintenanceRequest::new(MaintenanceTask::Flush, MaintenanceScope::Branch(branch()));

    let outcome = runtime.maintenance(&request).expect("flush outcome");

    assert_eq!(outcome.task(), MaintenanceTask::Flush);
    assert_eq!(outcome.status(), MaintenanceSummaryStatus::Completed);
    assert!(outcome.rows_processed() > 0);
    assert!(!outcome.wal_truncated());
}

#[test]
fn api_flush_does_not_claim_wal_truncation_without_proof() {
    let mut runtime = open_runtime();
    runtime
        .commit(&put_batch(b"no-proof", b"value"))
        .expect("commit");
    let request =
        MaintenanceRequest::new(MaintenanceTask::Flush, MaintenanceScope::Branch(branch()));

    let outcome = runtime.maintenance(&request).expect("flush outcome");

    assert_eq!(outcome.status(), MaintenanceSummaryStatus::Completed);
    assert!(!outcome.wal_truncated());
    assert!(outcome.checkpoint_watermark().is_none());
}

#[test]
fn api_flush_global_scope_rejects() {
    let mut runtime = open_runtime();
    let request = MaintenanceRequest::new(MaintenanceTask::Flush, MaintenanceScope::Global);

    let error = runtime
        .maintenance(&request)
        .expect_err("global flush scope rejects");

    assert_eq!(error.class(), StorageApiErrorClass::InvalidArgument);
    assert_eq!(error.code(), "invalid_argument.storage_api.argument");
}

#[test]
fn api_flush_failure_preserves_orphan_facts() {
    let object = "tables/00000000-0000-0000-0000-000000000001/l0/orphan".to_string();
    let lower = crate::lifecycle::MaintenanceOutcome::new(
        crate::lifecycle::MaintenanceTaskKind::Flush,
        crate::lifecycle::MaintenanceOutcomeStatus::Failed,
    )
    .with_affected_object_names(vec![object.clone()])
    .with_effects(1, 0, true)
    .with_source_error(
        crate::lifecycle::LifecycleError::flush_publication_orphaned_with(
            Some(object.clone()),
            "flush published an object that was not installed",
            SourceError,
        ),
    );
    let request =
        MaintenanceRequest::new(MaintenanceTask::Flush, MaintenanceScope::Branch(branch()));

    let outcome = map_maintenance_outcome_for_test(request, &lower);

    assert_eq!(outcome.status(), MaintenanceSummaryStatus::Failed);
    assert_eq!(outcome.affected_objects(), 1);
    assert_eq!(outcome.affected_object_names(), &[object]);
    assert_eq!(
        outcome.source_error_code(),
        Some("ambiguous_commit.storage_api.durable_uncertain")
    );
    assert_eq!(
        outcome.source_error_class(),
        Some(StorageApiErrorClass::AmbiguousCommit)
    );
    assert!(outcome.retryable());
}

#[test]
fn api_compaction_returns_rewrite_facts() {
    let mut runtime = open_runtime();
    let request =
        MaintenanceRequest::new(MaintenanceTask::Compact, MaintenanceScope::Branch(branch()));

    let outcome = runtime.maintenance(&request).expect("compaction outcome");

    assert_eq!(outcome.task(), MaintenanceTask::Compact);
    assert_eq!(outcome.scope(), MaintenanceScope::Branch(branch()));
    assert_eq!(outcome.status(), MaintenanceSummaryStatus::Deferred);
    assert_eq!(
        outcome.reason_class(),
        Some(MaintenanceReasonClass::Deferred)
    );
}

#[test]
fn api_compaction_reports_checkpoint_debt() {
    let lower = crate::lifecycle::MaintenanceOutcome::new(
        crate::lifecycle::MaintenanceTaskKind::Compaction,
        crate::lifecycle::MaintenanceOutcomeStatus::Completed,
    )
    .with_checkpoint_required(true)
    .with_affected_object_names(vec!["tables/default/l1/rewrite".to_string()])
    .with_effects(1, 4096, false);
    let request =
        MaintenanceRequest::new(MaintenanceTask::Compact, MaintenanceScope::Branch(branch()));

    let outcome = map_maintenance_outcome_for_test(request, &lower);

    assert_eq!(outcome.status(), MaintenanceSummaryStatus::Completed);
    assert!(outcome.checkpoint_required());
    assert_eq!(outcome.affected_objects(), 1);
    assert_eq!(outcome.bytes_reclaimed(), 4096);
}

#[test]
fn api_materialization_returns_rewrite_facts() {
    let lower = crate::lifecycle::MaintenanceOutcome::new(
        crate::lifecycle::MaintenanceTaskKind::Materialization,
        crate::lifecycle::MaintenanceOutcomeStatus::Completed,
    )
    .with_affected_object_names(vec!["tables/default/l0/materialized".to_string()])
    .with_effects(1, 2048, false)
    .with_state_changes(1)
    .with_checkpoint_required(true);
    let request = MaintenanceRequest::new(
        MaintenanceTask::Materialize,
        MaintenanceScope::Branch(branch()),
    );

    let outcome = map_maintenance_outcome_for_test(request, &lower);

    assert_eq!(outcome.task(), MaintenanceTask::Materialize);
    assert_eq!(outcome.scope(), MaintenanceScope::Branch(branch()));
    assert_eq!(outcome.status(), MaintenanceSummaryStatus::Completed);
    assert_eq!(outcome.affected_objects(), 1);
    assert_eq!(outcome.bytes_reclaimed(), 2048);
    assert!(outcome.state_changes() > 0);
    assert!(outcome.checkpoint_required());
}

#[test]
fn api_materialization_uses_stable_intent() {
    let mut runtime = open_runtime();
    runtime
        .commit(&put_batch(b"stable", b"value"))
        .expect("commit parent");
    let child = branch_with(0x73);
    fork_branch(&mut runtime, child);
    let request = MaintenanceRequest::new(
        MaintenanceTask::Materialize,
        MaintenanceScope::Branch(child),
    );

    runtime
        .enqueue_maintenance(&request)
        .expect("enqueue materialization");
    let drain = runtime.drain_maintenance().expect("drain materialization");

    assert_eq!(drain.drained_tasks(), 1);
    assert_eq!(drain.outcomes()[0].task(), MaintenanceTask::Materialize);
    assert_eq!(drain.outcomes()[0].scope(), MaintenanceScope::Branch(child));
    assert_ne!(
        drain.outcomes()[0].status(),
        MaintenanceSummaryStatus::Failed
    );
}

#[test]
fn api_materialization_direct_call_runs_requested_task_not_prior_queue_entry() {
    let mut runtime = open_runtime();
    runtime
        .commit(&put_batch(b"direct-stable", b"value"))
        .expect("commit parent");
    let first_child = branch_with(0x75);
    let second_child = branch_with(0x76);
    fork_branch(&mut runtime, first_child);
    fork_branch(&mut runtime, second_child);
    let queued = MaintenanceRequest::new(
        MaintenanceTask::Materialize,
        MaintenanceScope::Branch(first_child),
    );
    let direct = MaintenanceRequest::new(
        MaintenanceTask::Materialize,
        MaintenanceScope::Branch(second_child),
    );

    runtime
        .enqueue_maintenance(&queued)
        .expect("enqueue first materialization");
    let direct_outcome = runtime
        .maintenance(&direct)
        .expect("run direct materialization");
    let drain = runtime.drain_maintenance().expect("drain remaining task");

    assert_eq!(
        direct_outcome.scope(),
        MaintenanceScope::Branch(second_child)
    );
    assert_eq!(drain.drained_tasks(), 1);
    assert_eq!(
        drain.outcomes()[0].scope(),
        MaintenanceScope::Branch(first_child)
    );
}

#[test]
fn api_rewrite_cache_mode_does_not_call_durable_services() {
    let mut runtime = open_runtime();
    let request =
        MaintenanceRequest::new(MaintenanceTask::Compact, MaintenanceScope::Branch(branch()));

    let outcome = runtime.maintenance(&request).expect("cache compaction");

    // Cache mode has no durable service handles; Deferred is the public
    // boundary signal that the compact request did not enter a durable path.
    assert_eq!(outcome.status(), MaintenanceSummaryStatus::Deferred);
    assert_eq!(runtime.state(), StorageRuntimeState::Open);
}

#[test]
fn api_wal_growth_policy_status_reports_no_durable_action_for_cache() {
    let mut runtime = open_runtime();
    let request = MaintenanceRequest::new(MaintenanceTask::WalGrowth, MaintenanceScope::Global);

    let outcome = runtime.maintenance(&request).expect("wal growth outcome");
    let growth = outcome.wal_growth().expect("wal growth facts");

    assert_eq!(outcome.status(), MaintenanceSummaryStatus::Completed);
    assert_eq!(growth.status(), MaintenanceWalGrowthStatus::NoDurableAction);
    assert!(!growth.checkpoint_enqueued());
}

#[test]
fn api_wal_growth_policy_status_reports_not_needed() {
    #[cfg(feature = "localfs")]
    let mut runtime = open_durable_runtime_with_options(
        "maintenance-wal-growth-below-threshold",
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard)
            .with_wal_growth_policy(StorageWalGrowthPolicy::thresholds(
                u64::MAX,
                usize::MAX,
                u64::MAX,
            )),
    );
    #[cfg(not(feature = "localfs"))]
    let mut runtime = open_runtime();
    let request = MaintenanceRequest::new(MaintenanceTask::WalGrowth, MaintenanceScope::Global);

    let outcome = runtime.maintenance(&request).expect("WAL growth outcome");
    let growth = outcome.wal_growth().expect("WAL growth summary");

    #[cfg(feature = "localfs")]
    assert_eq!(growth.status(), MaintenanceWalGrowthStatus::BelowThreshold);
    #[cfg(not(feature = "localfs"))]
    assert_eq!(growth.status(), MaintenanceWalGrowthStatus::NoDurableAction);
    assert!(!growth.checkpoint_enqueued());
}

#[test]
#[cfg(feature = "localfs")]
fn api_wal_growth_policy_status_reports_disabled() {
    let mut runtime = open_durable_runtime_with_options(
        "maintenance-wal-growth-disabled",
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard)
            .with_wal_growth_policy(StorageWalGrowthPolicy::Disabled),
    );
    let request = MaintenanceRequest::new(MaintenanceTask::WalGrowth, MaintenanceScope::Global);

    let outcome = runtime.maintenance(&request).expect("WAL growth outcome");
    let growth = outcome.wal_growth().expect("WAL growth summary");

    assert_eq!(growth.status(), MaintenanceWalGrowthStatus::Disabled);
    assert!(!growth.checkpoint_enqueued());
}

#[test]
#[cfg(feature = "localfs")]
fn api_wal_growth_policy_status_reports_checkpoint_due() {
    let options = StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard)
        .with_wal_growth_policy(StorageWalGrowthPolicy::thresholds(u64::MAX, usize::MAX, 1));
    let mut runtime = open_durable_runtime_with_options("maintenance-wal-growth-due", options);
    runtime
        .commit(&put_batch(b"growth-a", b"value"))
        .expect("first commit");
    runtime
        .commit(&put_batch(b"growth-b", b"value"))
        .expect("second commit");
    let request = MaintenanceRequest::new(MaintenanceTask::WalGrowth, MaintenanceScope::Global);

    let outcome = runtime.maintenance(&request).expect("WAL growth outcome");
    let growth = outcome.wal_growth().expect("WAL growth summary");

    assert_eq!(outcome.task(), MaintenanceTask::WalGrowth);
    assert!(matches!(
        growth.status(),
        MaintenanceWalGrowthStatus::CheckpointEnqueued
            | MaintenanceWalGrowthStatus::CheckpointCoalesced
    ));
    assert_eq!(
        growth.trigger(),
        Some(MaintenanceWalGrowthTrigger::CommitsSinceCheckpoint)
    );
    assert!(growth.checkpoint_enqueued());
    assert_eq!(growth.commits_since_checkpoint(), 2);
    assert_eq!(
        runtime
            .maintenance_status()
            .expect("status")
            .pending_tasks(),
        1
    );
}

#[test]
fn api_wal_growth_trigger_runs_supported_path() {
    let mut runtime = open_runtime();
    let request = MaintenanceRequest::new(MaintenanceTask::WalGrowth, MaintenanceScope::Global);

    let outcome = runtime.maintenance(&request).expect("WAL growth outcome");

    assert_eq!(outcome.task(), MaintenanceTask::WalGrowth);
    assert!(outcome.wal_growth().is_some());
}

#[test]
fn api_wal_growth_enqueue_rejects_direct_queueing() {
    let mut runtime = open_runtime();
    let request = MaintenanceRequest::new(MaintenanceTask::WalGrowth, MaintenanceScope::Global);

    let error = runtime
        .enqueue_maintenance(&request)
        .expect_err("WAL growth is policy-evaluated directly");

    assert_eq!(error.class(), StorageApiErrorClass::InvalidArgument);
}

#[test]
fn api_maintenance_enqueue_and_drain_are_deterministic() {
    let mut runtime = open_runtime();
    runtime
        .commit(&put_batch(b"queue", b"value"))
        .expect("commit");
    let request =
        MaintenanceRequest::new(MaintenanceTask::Flush, MaintenanceScope::Branch(branch()));

    let queued = runtime
        .enqueue_maintenance(&request)
        .expect("enqueue flush");
    assert_eq!(queued.pending_tasks(), 1);
    assert_eq!(queued.enqueued(), 1);

    let drain = runtime.drain_maintenance().expect("drain maintenance");

    assert_eq!(drain.drained_tasks(), 1);
    assert_eq!(drain.outcomes().len(), 1);
    assert_eq!(drain.outcomes()[0].task(), MaintenanceTask::Flush);
    assert_eq!(drain.queue().pending_tasks(), 0);
}

#[test]
fn api_maintenance_queue_status_reports_pending_only() {
    let mut runtime = open_runtime();
    runtime
        .commit(&put_batch(b"status", b"value"))
        .expect("commit");
    let request =
        MaintenanceRequest::new(MaintenanceTask::Flush, MaintenanceScope::Branch(branch()));

    let queued = runtime
        .enqueue_maintenance(&request)
        .expect("enqueue flush");
    let status = runtime.maintenance_status().expect("status");

    assert_eq!(queued.pending_tasks(), 1);
    assert_eq!(status.pending_tasks(), 1);
    // Active task ids are transient inside run_task and are not observable from
    // this synchronous API without a dedicated hook.
    assert_eq!(status.active_task(), None);
}

#[test]
fn api_cache_durable_only_enqueue_rejects_without_stranding_tasks() {
    let mut runtime = open_runtime();
    let cases = [
        MaintenanceRequest::new(MaintenanceTask::Checkpoint, MaintenanceScope::Global),
        MaintenanceRequest::new(MaintenanceTask::Retain, MaintenanceScope::Global),
        MaintenanceRequest::new(MaintenanceTask::SnapshotPruning, MaintenanceScope::Global),
        MaintenanceRequest::new(MaintenanceTask::Reclaim, MaintenanceScope::Branch(branch())),
        MaintenanceRequest::new(MaintenanceTask::Quarantine, MaintenanceScope::Global),
        MaintenanceRequest::new(MaintenanceTask::Purge, MaintenanceScope::Branch(branch())),
        MaintenanceRequest::new(MaintenanceTask::Repair, MaintenanceScope::Global),
    ];

    for request in cases {
        let error = runtime
            .enqueue_maintenance(&request)
            .expect_err("cache durable-only enqueue rejects");

        assert_eq!(error.class(), StorageApiErrorClass::FailedPrecondition);
        assert_eq!(
            runtime
                .maintenance_status()
                .expect("maintenance status")
                .pending_tasks(),
            0
        );
    }
}

#[test]
fn api_maintenance_drain_is_deterministic() {
    let mut runtime = open_runtime();
    runtime
        .commit(&put_batch(b"first", b"value"))
        .expect("first commit");
    let flush = MaintenanceRequest::new(MaintenanceTask::Flush, MaintenanceScope::Branch(branch()));
    let compact =
        MaintenanceRequest::new(MaintenanceTask::Compact, MaintenanceScope::Branch(branch()));

    runtime
        .enqueue_maintenance(&compact)
        .expect("enqueue compact");
    runtime.enqueue_maintenance(&flush).expect("enqueue flush");
    let drain = runtime.drain_maintenance().expect("drain");

    assert_eq!(drain.outcomes()[0].task(), MaintenanceTask::Flush);
    assert_eq!(drain.queue().pending_tasks(), 0);
}

#[test]
fn api_maintenance_drain_preserves_branch_scope() {
    let mut runtime = open_runtime();
    runtime
        .commit(&put_batch(b"parent-scope", b"value"))
        .expect("parent commit");
    let child = branch_with(0x74);
    fork_branch(&mut runtime, child);
    runtime
        .commit(&put_batch_for(child, b"child-scope", b"value"))
        .expect("child commit");
    let request = MaintenanceRequest::new(MaintenanceTask::Flush, MaintenanceScope::Branch(child));

    runtime
        .enqueue_maintenance(&request)
        .expect("enqueue child flush");
    let drain = runtime.drain_maintenance().expect("drain child flush");

    assert_eq!(drain.drained_tasks(), 1);
    assert_eq!(drain.outcomes()[0].scope(), MaintenanceScope::Branch(child));
    assert_ne!(
        drain.outcomes()[0].status(),
        MaintenanceSummaryStatus::Failed
    );
}

#[test]
#[cfg(feature = "localfs")]
fn api_repair_branch_scope_round_trips_through_drain() {
    let (backend, mut runtime) = open_durable_runtime_with_backend(
        "maintenance-repair-branch-scope",
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
    );
    let child = branch_with(0x77);
    runtime
        .commit(&put_batch(b"repair-parent", b"value"))
        .expect("seed parent history");
    fork_branch(&mut runtime, child);
    let orphan =
        crate::layout::ObjectLayout::quarantine_object(&child.to_string(), "api-branch-orphan")
            .expect("orphan quarantine object");
    backend
        .as_backend()
        .write_object(&orphan, b"orphan")
        .expect("write orphan quarantine object");
    let request = MaintenanceRequest::new(MaintenanceTask::Repair, MaintenanceScope::Branch(child));

    runtime
        .enqueue_maintenance(&request)
        .expect("enqueue branch repair");
    let drain = runtime.drain_maintenance().expect("drain branch repair");

    assert_eq!(drain.drained_tasks(), 1);
    assert_eq!(drain.outcomes()[0].task(), MaintenanceTask::Repair);
    assert_eq!(drain.outcomes()[0].scope(), MaintenanceScope::Branch(child));
    assert!(drain.outcomes()[0].recovery_health().is_some());
}

#[test]
fn api_maintenance_after_close_rejects() {
    let mut runtime = open_runtime();
    runtime.close().expect("close");
    let request =
        MaintenanceRequest::new(MaintenanceTask::Flush, MaintenanceScope::Branch(branch()));

    let error = runtime
        .maintenance(&request)
        .expect_err("closed runtime rejects maintenance");

    assert_eq!(error.class(), StorageApiErrorClass::FailedPrecondition);
    assert_eq!(error.code(), "failed_precondition.storage_api.state");
}

#[test]
fn api_retention_without_current_proof_rejects() {
    let proof = crate::lifecycle::LifecycleRetentionProof::new(
        crate::lifecycle::LifecycleRetentionProofStatus::Incomplete,
        crate::lifecycle::RecoveryHealth::Healthy,
        None,
        None,
        None,
        Some("missing current proof"),
    );
    let lower = crate::lifecycle::LifecycleRetentionOutcome::from_decisions(proof, Vec::new(), 0)
        .expect("incomplete proof outcome")
        .maintenance_outcome();
    let request = MaintenanceRequest::new(MaintenanceTask::Retain, MaintenanceScope::Global);

    let outcome = map_maintenance_outcome_for_test(request, &lower);

    assert_eq!(outcome.task(), MaintenanceTask::Retain);
    assert_eq!(outcome.status(), MaintenanceSummaryStatus::Deferred);
    assert_eq!(outcome.reason(), Some("retention proof is incomplete"));
    assert_eq!(
        outcome.recovery_health(),
        Some(RecoveryHealthSummary::Degraded)
    );
}

#[test]
fn api_retention_table_objects_deferred_when_unsupported() {
    let mut runtime = open_runtime();
    let request =
        MaintenanceRequest::new(MaintenanceTask::Reclaim, MaintenanceScope::Branch(branch()));

    let outcome = runtime.maintenance(&request).expect("reclaim outcome");

    assert_eq!(outcome.task(), MaintenanceTask::Reclaim);
    assert_eq!(outcome.status(), MaintenanceSummaryStatus::Deferred);
}

#[test]
fn api_retention_noop_does_not_report_successful_table_deletion() {
    let lower = crate::lifecycle::MaintenanceOutcome::new(
        crate::lifecycle::MaintenanceTaskKind::Retention,
        crate::lifecycle::MaintenanceOutcomeStatus::Deferred,
    )
    .with_reason("retention scope not supported by generic path");
    let request =
        MaintenanceRequest::new(MaintenanceTask::Reclaim, MaintenanceScope::Branch(branch()));

    let outcome = map_maintenance_outcome_for_test(request, &lower);

    assert_eq!(outcome.status(), MaintenanceSummaryStatus::Deferred);
    assert_eq!(outcome.affected_objects(), 0);
    assert_eq!(outcome.bytes_reclaimed(), 0);
    assert_eq!(outcome.state_changes(), 0);
}

#[test]
fn api_checkpoint_truncation_debt_does_not_fail_checkpoint_summary() {
    let lower = crate::lifecycle::MaintenanceOutcome::new(
        crate::lifecycle::MaintenanceTaskKind::Checkpoint,
        crate::lifecycle::MaintenanceOutcomeStatus::Completed,
    )
    .with_reason("WAL truncation follow-up has health debt")
    .with_source_error(
        crate::lifecycle::LifecycleError::CheckpointSnapshotOrphaned {
            object: Some("snapshots/0000000000000001".to_string()),
            reason: "snapshot published before manifest update",
        },
    );
    let request = MaintenanceRequest::new(MaintenanceTask::Checkpoint, MaintenanceScope::Global);

    let outcome = map_maintenance_outcome_for_test(request, &lower);

    assert_eq!(outcome.task(), MaintenanceTask::Checkpoint);
    assert_eq!(outcome.status(), MaintenanceSummaryStatus::Completed);
    assert_eq!(outcome.reason_class(), None);
    assert_eq!(
        outcome.source_error_code(),
        Some("failed_precondition.storage_api.maintenance")
    );
}

#[test]
fn api_snapshot_pruning_preserves_required_snapshot() {
    #[cfg(feature = "localfs")]
    let mut runtime = open_durable_runtime_with_options(
        "maintenance-snapshot-pruning-preserves-required",
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
    );
    #[cfg(not(feature = "localfs"))]
    let mut runtime = open_runtime();
    #[cfg(feature = "localfs")]
    {
        runtime
            .commit(&put_batch(b"snapshot-prune", b"value"))
            .expect("commit before checkpoint");
        runtime
            .maintenance(&MaintenanceRequest::new(
                MaintenanceTask::Checkpoint,
                MaintenanceScope::Global,
            ))
            .expect("checkpoint before pruning");
    }
    let request =
        MaintenanceRequest::new(MaintenanceTask::SnapshotPruning, MaintenanceScope::Global);

    let outcome = runtime
        .maintenance(&request)
        .expect("snapshot pruning outcome");

    assert_eq!(outcome.task(), MaintenanceTask::SnapshotPruning);
    #[cfg(feature = "localfs")]
    {
        assert_eq!(outcome.status(), MaintenanceSummaryStatus::Completed);
        assert_eq!(outcome.affected_objects(), 1);
        assert_eq!(outcome.bytes_reclaimed(), 0);
        assert_eq!(outcome.state_changes(), 0);
    }
    #[cfg(not(feature = "localfs"))]
    {
        assert_eq!(outcome.status(), MaintenanceSummaryStatus::Deferred);
        assert_eq!(
            outcome.reason(),
            Some("cache runtime does not support durable snapshot pruning maintenance")
        );
    }
}

#[test]
fn api_quarantine_via_queue_defers_without_explicit_request() {
    let mut runtime = open_runtime();
    let request = MaintenanceRequest::new(MaintenanceTask::Quarantine, MaintenanceScope::Global);

    let outcome = runtime.maintenance(&request).expect("quarantine outcome");

    assert_eq!(outcome.task(), MaintenanceTask::Quarantine);
    assert_eq!(outcome.status(), MaintenanceSummaryStatus::Deferred);
    assert_eq!(
        outcome.reason(),
        Some("cache runtime does not support durable quarantine maintenance")
    );
    assert_eq!(
        outcome.reason_class(),
        Some(MaintenanceReasonClass::Deferred)
    );
    assert_eq!(outcome.affected_objects(), 0);
}

#[test]
fn api_purge_requires_fresh_proof() {
    #[cfg(feature = "localfs")]
    {
        let (backend, mut runtime) = open_durable_runtime_with_backend(
            "maintenance-purge-fresh-proof",
            StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        );
        stage_quarantine_object(backend, branch(), "api-purge-source", b"purge-me");
        let request =
            MaintenanceRequest::new(MaintenanceTask::Purge, MaintenanceScope::Branch(branch()));

        let outcome = runtime.maintenance(&request).expect("purge outcome");

        assert_eq!(outcome.task(), MaintenanceTask::Purge);
        assert_eq!(outcome.status(), MaintenanceSummaryStatus::Completed);
        assert!(outcome.bytes_reclaimed() >= b"purge-me".len() as u64);
        assert!(outcome.affected_objects() >= 2);
    }

    #[cfg(not(feature = "localfs"))]
    {
        let mut runtime = open_runtime();
        let request =
            MaintenanceRequest::new(MaintenanceTask::Purge, MaintenanceScope::Branch(branch()));

        let outcome = runtime.maintenance(&request).expect("purge outcome");

        assert_eq!(outcome.status(), MaintenanceSummaryStatus::Deferred);
        assert_eq!(
            outcome.reason_class(),
            Some(MaintenanceReasonClass::Deferred)
        );
    }
}

#[test]
fn api_repair_reports_reconciliation_facts() {
    #[cfg(feature = "localfs")]
    let (backend, mut runtime) = open_durable_runtime_with_backend(
        "maintenance-repair-reconciliation-facts",
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
    );
    #[cfg(feature = "localfs")]
    {
        let orphan = crate::layout::ObjectLayout::quarantine_object(
            &branch().to_string(),
            "api-repair-orphan",
        )
        .expect("orphan quarantine object");
        backend
            .as_backend()
            .write_object(&orphan, b"orphan")
            .expect("write orphan quarantine object");
    }
    #[cfg(not(feature = "localfs"))]
    let mut runtime = open_runtime();
    let request = MaintenanceRequest::new(MaintenanceTask::Repair, MaintenanceScope::Global);

    let outcome = runtime.maintenance(&request).expect("repair outcome");

    assert_eq!(outcome.task(), MaintenanceTask::Repair);
    #[cfg(feature = "localfs")]
    {
        assert_eq!(outcome.status(), MaintenanceSummaryStatus::Completed);
        assert!(outcome.recovery_health().is_some());
        assert!(outcome.affected_objects() > 0);
    }
    #[cfg(not(feature = "localfs"))]
    {
        assert_eq!(outcome.status(), MaintenanceSummaryStatus::Deferred);
        assert_eq!(
            outcome.reason(),
            Some("cache runtime does not support quarantine repair maintenance")
        );
    }
}

#[test]
fn api_reclaim_degraded_health_blocks_when_required() {
    let health = crate::lifecycle::RecoveryHealth::degraded(
        crate::lifecycle::RecoveryDegradationClass::PolicyDowngrade,
        vec![crate::lifecycle::RecoveryFault::new(
            crate::lifecycle::RecoveryFaultKind::QuarantineInventoryMismatch,
            "current recovery health blocks reclaim",
        )
        .expect("fault")],
    )
    .expect("degraded health");
    let proof = crate::lifecycle::LifecycleRetentionProof::new(
        crate::lifecycle::LifecycleRetentionProofStatus::BlockedByRecoveryHealth,
        health,
        None,
        Some(CommitVersion::new(1)),
        Some(CommitVersion::new(1)),
        None,
    );
    let lower = crate::lifecycle::LifecycleRetentionOutcome::from_decisions(proof, Vec::new(), 0)
        .expect("blocked proof outcome")
        .maintenance_outcome();
    let request =
        MaintenanceRequest::new(MaintenanceTask::Reclaim, MaintenanceScope::Branch(branch()));

    let outcome = map_maintenance_outcome_for_test(request, &lower);

    assert_eq!(outcome.task(), MaintenanceTask::Reclaim);
    assert_eq!(outcome.status(), MaintenanceSummaryStatus::Deferred);
    assert_eq!(outcome.reason(), Some("recovery health blocks retention"));
    assert_eq!(
        outcome.recovery_health(),
        Some(RecoveryHealthSummary::Degraded)
    );
}

#[test]
fn api_rewrite_unknown_branch_rejects() {
    let mut runtime = open_runtime();
    let unknown = BranchId::from_bytes([0x55; BranchId::BYTE_LEN]);
    let request =
        MaintenanceRequest::new(MaintenanceTask::Compact, MaintenanceScope::Branch(unknown));

    let error = runtime
        .maintenance(&request)
        .expect_err("unknown branch rejects rewrite maintenance");

    assert_eq!(error.class(), StorageApiErrorClass::NotFound);
}
