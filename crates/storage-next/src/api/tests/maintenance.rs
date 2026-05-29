use super::*;

fn open_runtime() -> StorageRuntime<'static> {
    StorageRuntime::open(StorageOpenOptions::default())
        .expect("open cache runtime")
        .into_runtime()
}

fn branch() -> BranchId {
    StorageRuntime::default_branch_id_for_test()
}

fn engine_space() -> StorageSpaceId {
    StorageSpaceId::new(vec![0x20]).expect("engine storage space")
}

fn api_key(bytes: &[u8]) -> StorageKey {
    StorageKey::new(bytes.to_vec()).expect("valid key")
}

fn put_batch(key: &[u8], value: &[u8]) -> CommitBatch {
    CommitBatch::new(
        branch(),
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
