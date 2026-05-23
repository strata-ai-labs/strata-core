use super::*;
use crate::backend::{
    Backend, BackendAppend, BackendCapabilities, BackendError, BackendErrorKind, BackendMetadata,
    BackendRange, BackendResult, BackendWriterGuard, PublishDurability, PublishError, PublishMode,
    PublishOutcome, PublishResult, DURABLE_LOCAL_MODE_REQUIREMENTS,
};
use crate::branch::{BranchLocalState, BranchRuntimeConfig};
use crate::commit::{
    CommitBatch, CommitBatchOptions, CommitBranchGeneration, CommitBranchGenerationGuard,
    CommitConflictValidationMode, CommitDuplicateKeyPolicy, CommitDurabilityMode, CommitExpiry,
    CommitManualTimestampSource, CommitMutation, CommitOrigin, CommitRetentionHint,
    CommitRuntimeConfig, CommitTimestampPolicy, CommitValidationFacts,
};
use crate::format::decode_snapshot_container;
use crate::layout::ObjectLayout;
use crate::object::{ObjectName, ObjectPrefix};
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use crate::service::{DatabaseManifestService, WalRetentionProof, WalRetentionProofSource};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

const DATABASE_ID: [u8; 16] = [0x7d; 16];

#[test]
fn checkpoint_request_rejects_zero_snapshot_id() {
    assert_eq!(
        LifecycleCheckpointRequest::new(branch_id(0x10), 0, Timestamp::from_micros(1)),
        Err(LifecycleError::MaintenanceFailed {
            reason: "checkpoint snapshot id must be nonzero",
        })
    );
}

#[test]
fn checkpoint_rows_include_owned_frozen_active_and_exclude_newer_rows() {
    let branch = branch_id(0x11);
    let mut state = BranchLocalState::empty(branch);
    let owned = put_row(branch, 1, b"owned", b"owned-value");
    let frozen = put_row(branch, 2, b"frozen", b"frozen-value");
    let active = put_row(branch, 3, b"active", b"active-value");
    let hidden = put_row(branch, 4, b"hidden", b"hidden-value");

    state
        .append_committed_row(owned.clone())
        .expect("append owned candidate");
    state.rotate_active();
    flush_cache_branch(&mut state, &flush_request(branch)).expect("flush owned");
    state
        .append_committed_row(frozen.clone())
        .expect("append frozen candidate");
    state.rotate_active();
    state
        .append_committed_row(active.clone())
        .expect("append active candidate");
    state
        .append_committed_row(hidden)
        .expect("append hidden candidate");

    let rows = state
        .checkpoint_rows(CommitVersion::new(3))
        .expect("checkpoint rows");

    assert_eq!(rows.len(), 3);
    assert!(rows.contains(&owned));
    assert!(rows.contains(&frozen));
    assert!(rows.contains(&active));
    assert!(rows
        .windows(2)
        .all(
            |window| crate::table::TableInternalKeyBytes::from_row(&window[0])
                < crate::table::TableInternalKeyBytes::from_row(&window[1])
        ));
}

#[test]
fn checkpoint_reads_visible_version_after_commit_quiesce() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x12);
    let shell = assemble_shell(branch, &backend).expect("shell");
    let mut state = BranchLocalState::empty(branch);
    state
        .append_committed_row(put_row(branch, 1, b"visible", b"value"))
        .expect("append row");
    let observed_quiesce = Cell::new(false);
    let request =
        LifecycleCheckpointRequest::new(branch, 1, Timestamp::from_micros(10)).expect("request");

    let outcome = checkpoint_durable_branch(
        &state,
        shell.services(),
        shell.guard_set(),
        || {
            observed_quiesce.set(shell.guard_set().is_quiescing().expect("quiesce state"));
            CommitVersion::new(1)
        },
        &request,
    )
    .expect("checkpoint");

    assert!(observed_quiesce.get());
    assert_eq!(outcome.status(), LifecycleCheckpointStatus::Completed);
    assert_eq!(outcome.checkpoint_watermark(), Some(CommitVersion::new(1)));
    assert_eq!(outcome.row_count(), 1);
}

#[test]
fn checkpoint_defers_when_branch_has_no_rows_under_visible_watermark() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x13);
    let shell = assemble_shell(branch, &backend).expect("shell");
    let request =
        LifecycleCheckpointRequest::new(branch, 1, Timestamp::from_micros(10)).expect("request");

    let outcome = checkpoint_durable_branch(
        shell.branch_state(),
        shell.services(),
        shell.guard_set(),
        || CommitVersion::new(1),
        &request,
    )
    .expect("checkpoint");

    assert_eq!(
        outcome.status(),
        LifecycleCheckpointStatus::DeferredNoVisibleRows
    );
    assert!(outcome.snapshot_id().is_none());
    assert!(backend.snapshot_objects().is_empty());
}

#[test]
fn checkpoint_publishes_snapshot_and_flush_watermark_after_commit() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x14);
    let mut runtime = open_runtime(branch, &backend);
    runtime
        .execute_durable_commit(
            durable_batch(branch, b"checkpoint-key", b"value"),
            generation_guard(),
        )
        .expect("commit");
    let request = LifecycleCheckpointRequest::new(branch, 1, Timestamp::from_micros(11))
        .expect("request")
        .with_flush_watermark_after_checkpoint(true);

    let outcome = runtime.checkpoint(&request).expect("checkpoint");

    assert_eq!(outcome.status(), LifecycleCheckpointStatus::Completed);
    assert_eq!(outcome.checkpoint_watermark(), Some(CommitVersion::new(1)));
    assert_eq!(outcome.snapshot_id(), Some(1));
    assert_eq!(outcome.row_count(), 3);
    assert!(outcome.snapshot_object().is_some());
    assert_eq!(
        outcome.flush_watermark().expect("flush outcome").status(),
        LifecycleFlushWatermarkStatus::Persisted
    );
    let manifest = DatabaseManifestService::new(&backend)
        .load_required()
        .expect("current database record");
    assert_eq!(manifest.snapshot_id(), Some(1));
    assert_eq!(manifest.snapshot_watermark(), Some(1));
    assert_eq!(
        manifest.flushed_through_commit_id(),
        Some(CommitVersion::new(1))
    );
}

#[test]
fn checkpoint_reports_flush_watermark_failure_without_losing_snapshot_facts() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x15);
    let mut runtime = open_runtime(branch, &backend);
    runtime
        .execute_durable_commit(
            durable_batch(branch, b"flush-failure-key", b"value"),
            generation_guard(),
        )
        .expect("commit");
    backend.fail_manifest_replacement_on_call(3);
    let request = LifecycleCheckpointRequest::new(branch, 1, Timestamp::from_micros(12))
        .expect("request")
        .with_flush_watermark_after_checkpoint(true);

    let outcome = runtime.checkpoint(&request).expect("checkpoint outcome");

    assert_eq!(
        outcome.status(),
        LifecycleCheckpointStatus::FlushWatermarkFailed
    );
    assert_eq!(outcome.snapshot_id(), Some(1));
    assert!(outcome.snapshot_object().is_some());
    assert!(outcome.flush_watermark().is_none());
    assert!(outcome.recovery_health().is_some());
    assert_eq!(
        outcome.maintenance_outcome().status(),
        MaintenanceOutcomeStatus::Failed
    );
    assert!(outcome.maintenance_outcome().retryable());
}

#[test]
fn checkpoint_reports_wal_truncation_failure_without_losing_snapshot_facts() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x16);
    let mut runtime = open_runtime(branch, &backend);
    runtime
        .execute_durable_commit(
            durable_batch(branch, b"truncation-failure-key", b"value"),
            generation_guard(),
        )
        .expect("commit");
    backend.fail_wal_listing();
    let request = LifecycleCheckpointRequest::new(branch, 1, Timestamp::from_micros(13))
        .expect("request")
        .with_wal_truncation_after_checkpoint(true);

    let outcome = runtime.checkpoint(&request).expect("checkpoint outcome");

    assert_eq!(
        outcome.status(),
        LifecycleCheckpointStatus::WalTruncationFailed
    );
    assert_eq!(outcome.snapshot_id(), Some(1));
    assert!(outcome.snapshot_object().is_some());
    assert!(outcome.wal_truncation().is_none());
    assert!(outcome.recovery_health().is_some());
    assert_eq!(
        outcome.maintenance_outcome().status(),
        MaintenanceOutcomeStatus::Failed
    );
    assert!(outcome.maintenance_outcome().retryable());
}

#[test]
fn flush_watermark_proofs_are_conservative_and_monotonic() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x17);
    let shell = assemble_shell(branch, &backend).expect("shell");
    shell
        .services()
        .manifest()
        .persist_snapshot_facts(3, CommitVersion::new(7))
        .expect("snapshot facts");

    let table_only = LifecycleFlushWatermarkRequest::new(
        CommitVersion::new(5),
        LifecycleFlushWatermarkProof::TableObjectsOnly {
            flushed_through: CommitVersion::new(5),
        },
    )
    .expect("table-only request");
    assert!(matches!(
        persist_flush_watermark(
            shell.services().manifest(),
            CommitVersion::new(7),
            table_only,
        ),
        Err(LifecycleError::RetentionBlocked { .. })
    ));

    let persisted = persist_flush_watermark(
        shell.services().manifest(),
        CommitVersion::new(7),
        LifecycleFlushWatermarkRequest::new(
            CommitVersion::new(5),
            LifecycleFlushWatermarkProof::CheckpointCovered {
                snapshot_watermark: CommitVersion::new(7),
            },
        )
        .expect("covered request"),
    )
    .expect("persisted");
    assert_eq!(persisted.status(), LifecycleFlushWatermarkStatus::Persisted);
    assert_eq!(persisted.persisted_watermark(), Some(CommitVersion::new(5)));

    let already = persist_flush_watermark(
        shell.services().manifest(),
        CommitVersion::new(7),
        LifecycleFlushWatermarkRequest::new(
            CommitVersion::new(5),
            LifecycleFlushWatermarkProof::AlreadyPersisted,
        )
        .expect("already request"),
    )
    .expect("already persisted");
    assert_eq!(
        already.status(),
        LifecycleFlushWatermarkStatus::AlreadyPersisted
    );
    assert_eq!(already.candidate(), CommitVersion::new(5));
}

#[test]
fn wal_truncation_task_uses_strongest_manifest_retention_proof() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x18);
    let shell = assemble_shell(branch, &backend).expect("shell");
    shell
        .services()
        .manifest()
        .persist_snapshot_facts(2, CommitVersion::new(4))
        .expect("snapshot facts");
    shell
        .services()
        .manifest()
        .persist_flush_watermark(CommitVersion::new(6))
        .expect("flush facts");
    let task = maintenance_task_for_test(1, MaintenanceTaskRequest::wal_truncation());

    let request = wal_truncation_request_from_maintenance_task(&task, shell.services().manifest())
        .expect("request")
        .expect("proof");

    assert_eq!(request.proof().covered_through(), CommitVersion::new(6));
    assert_eq!(
        request.proof().source(),
        WalRetentionProofSource::FlushWatermark
    );
}

#[test]
fn queued_checkpoint_task_runs_through_maintenance_executor() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x19);
    let mut runtime = open_runtime(branch, &backend);
    runtime
        .execute_durable_commit(
            durable_batch(branch, b"queued-checkpoint", b"value"),
            generation_guard(),
        )
        .expect("commit");
    let enqueue = runtime
        .enqueue_maintenance(MaintenanceTaskRequest::checkpoint())
        .expect("enqueue");

    let maintenance = runtime
        .run_next_checkpoint_maintenance()
        .expect("run")
        .expect("maintenance");

    assert_eq!(maintenance.task_id(), Some(enqueue.task_id()));
    assert_eq!(maintenance.task_kind(), MaintenanceTaskKind::Checkpoint);
    assert_eq!(maintenance.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(runtime.maintenance_status().stats().completed(), 1);
    assert_eq!(
        DatabaseManifestService::new(&backend)
            .load_required()
            .expect("current database record")
            .snapshot_id(),
        Some(1)
    );
    assert_eq!(
        backend.snapshot_created_at(),
        vec![Timestamp::from_micros(9_000)]
    );
}

#[test]
fn queued_wal_truncation_task_defers_without_retention_proof() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x1a);
    let mut runtime = open_runtime(branch, &backend);
    let enqueue = runtime
        .enqueue_maintenance(MaintenanceTaskRequest::wal_truncation())
        .expect("enqueue");

    let maintenance = runtime
        .run_next_wal_truncation_maintenance()
        .expect("run")
        .expect("maintenance");

    assert_eq!(maintenance.task_id(), Some(enqueue.task_id()));
    assert_eq!(maintenance.task_kind(), MaintenanceTaskKind::WalTruncation);
    assert_eq!(maintenance.status(), MaintenanceOutcomeStatus::Deferred);
    assert_eq!(runtime.maintenance_status().stats().deferred(), 1);
}

#[test]
fn wal_truncation_request_rejects_zero_proof() {
    assert_eq!(
        LifecycleWalTruncationRequest::new(WalRetentionProof::snapshot_watermark(
            CommitVersion::ZERO,
        )),
        Err(LifecycleError::MaintenanceFailed {
            reason: "WAL retention proof must be nonzero",
        })
    );
}

fn open_runtime(
    branch: BranchId,
    backend: &CheckpointTestBackend,
) -> LifecycleDurableLocalRuntime<'_, CommitManualTimestampSource> {
    let mut shell = assemble_shell(branch, backend).expect("shell");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    let outcome = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect("recovery outcome");
    shell.complete_recovery(&outcome).expect("open runtime")
}

fn assemble_shell(
    branch: BranchId,
    backend: &CheckpointTestBackend,
) -> LifecycleResult<LifecycleDurableLocalShell<'_>> {
    LifecycleDurableLocalShell::assemble(
        LifecycleDurableLocalOpenRequest::new(
            StorageOpenPlan::new(
                StorageMode::DurableLocalStandard,
                LifecycleCodecId::identity(),
                RecoveryStrictness::Strict,
                LifecycleConfig::default(),
            )
            .expect("open plan"),
            DATABASE_ID,
            branch,
            CommitBranchGeneration::new(1).expect("generation"),
            BranchRuntimeConfig::default(),
            CommitRuntimeConfig::default(),
            crate::service::WalServiceConfig::default(),
        )?,
        backend,
        CommitManualTimestampSource::new(Timestamp::from_micros(9_000)),
    )
}

fn durable_batch(branch: BranchId, user_key: &'static [u8], value: &'static [u8]) -> CommitBatch {
    CommitBatch::mutating(
        branch,
        vec![CommitMutation::put(
            physical_key(branch, user_key),
            value.to_vec(),
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::empty(),
        CommitBatchOptions::new(
            CommitDurabilityMode::Standard,
            CommitConflictValidationMode::Validate,
            CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::RuntimeGenerated,
            CommitOrigin::StorageRuntime,
        ),
    )
}

fn generation_guard() -> CommitBranchGenerationGuard {
    CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation"))
}

fn flush_request(branch: BranchId) -> FlushFrozenRequest {
    FlushFrozenRequest::new(
        branch,
        None,
        FlushTableIdentitySeed::new(format!("checkpoint-flush-{branch}")).expect("seed"),
        FlushTableObjectId::new(format!("checkpoint-object-{branch}")).expect("object"),
    )
    .expect("flush request")
}

fn put_row(
    branch: BranchId,
    version: u64,
    user_key: &'static [u8],
    value: &'static [u8],
) -> StorageRow {
    StorageRow::put(
        physical_key(branch, user_key),
        CommitVersion::new(version),
        Timestamp::from_micros(version * 100),
        Timestamp::EPOCH,
        value.to_vec(),
    )
}

fn physical_key(branch: BranchId, user_key: &'static [u8]) -> PhysicalKey {
    PhysicalKey::new(
        branch,
        "checkpoint",
        StorageSpaceId::engine(0x30).expect("space"),
        user_key.to_vec(),
    )
    .expect("physical key")
}

fn branch_id(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; 16])
}

fn maintenance_task_for_test(id: u64, request: MaintenanceTaskRequest) -> MaintenanceTask {
    MaintenanceTask::new_for_test(id, request).expect("maintenance task")
}

#[derive(Debug)]
struct CheckpointTestBackend {
    objects: Mutex<BTreeMap<ObjectName, Vec<u8>>>,
    fail_list: AtomicBool,
    fail_manifest_replace_call: AtomicUsize,
    manifest_replace_calls: AtomicUsize,
    lock_held: Arc<AtomicBool>,
}

impl CheckpointTestBackend {
    fn new() -> Self {
        Self {
            objects: Mutex::new(BTreeMap::new()),
            fail_list: AtomicBool::new(false),
            fail_manifest_replace_call: AtomicUsize::new(0),
            manifest_replace_calls: AtomicUsize::new(0),
            lock_held: Arc::new(AtomicBool::new(false)),
        }
    }

    fn fail_wal_listing(&self) {
        self.fail_list.store(true, Ordering::SeqCst);
    }

    fn fail_manifest_replacement_on_call(&self, call: usize) {
        self.fail_manifest_replace_call
            .store(call, Ordering::SeqCst);
    }

    fn snapshot_objects(&self) -> Vec<ObjectName> {
        let objects = self.objects.lock().expect("objects");
        let mut snapshots = objects
            .keys()
            .filter(|name| decode_snapshot_object(name, objects.get(*name).expect("bytes")))
            .cloned()
            .collect::<Vec<_>>();
        snapshots.sort();
        snapshots
    }

    fn snapshot_created_at(&self) -> Vec<Timestamp> {
        let objects = self.objects.lock().expect("objects");
        let mut timestamps = objects
            .values()
            .filter_map(|bytes| {
                decode_snapshot_container(bytes)
                    .ok()
                    .map(|container| container.header().created_at())
            })
            .collect::<Vec<_>>();
        timestamps.sort();
        timestamps
    }
}

impl Backend for CheckpointTestBackend {
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::from_slice(DURABLE_LOCAL_MODE_REQUIREMENTS)
    }

    fn read_object(&self, name: &ObjectName) -> BackendResult<Vec<u8>> {
        self.objects
            .lock()
            .expect("objects")
            .get(name)
            .cloned()
            .ok_or_else(|| BackendError::new(BackendErrorKind::NotFound, "object not found"))
    }

    fn read_range(&self, name: &ObjectName, range: BackendRange) -> BackendResult<Vec<u8>> {
        let bytes = self.read_object(name)?;
        let start = usize::try_from(range.offset()).unwrap_or(usize::MAX);
        let end = usize::try_from(range.end_offset().unwrap_or(u64::MAX)).unwrap_or(usize::MAX);
        Ok(bytes[start.min(bytes.len())..end.min(bytes.len())].to_vec())
    }

    fn write_object(&self, name: &ObjectName, bytes: &[u8]) -> BackendResult<BackendMetadata> {
        self.objects
            .lock()
            .expect("objects")
            .insert(name.clone(), bytes.to_vec());
        Ok(BackendMetadata::new(bytes.len() as u64, None))
    }

    fn delete_object(&self, name: &ObjectName) -> BackendResult<()> {
        self.objects.lock().expect("objects").remove(name);
        Ok(())
    }

    fn list_prefix(&self, prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
        if self.fail_list.load(Ordering::SeqCst) {
            return Err(BackendError::new(
                BackendErrorKind::Unavailable,
                "injected list failure",
            ));
        }
        let mut names = self
            .objects
            .lock()
            .expect("objects")
            .keys()
            .filter(|name| name.as_str().starts_with(prefix.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        names.sort();
        Ok(names)
    }

    fn object_metadata(&self, name: &ObjectName) -> BackendResult<BackendMetadata> {
        self.objects
            .lock()
            .expect("objects")
            .get(name)
            .map(|bytes| BackendMetadata::new(bytes.len() as u64, None))
            .ok_or_else(|| BackendError::new(BackendErrorKind::NotFound, "object not found"))
    }

    fn acquire_writer_lock(&self, name: &ObjectName) -> BackendResult<BackendWriterGuard> {
        if self.lock_held.swap(true, Ordering::SeqCst) {
            return Err(BackendError::new(
                BackendErrorKind::Unavailable,
                "writer lock already held",
            ));
        }
        Ok(BackendWriterGuard::new(
            name.clone(),
            HeldWriterLock {
                locked: Arc::clone(&self.lock_held),
            },
        ))
    }

    fn append_object(&self, name: &ObjectName, bytes: &[u8]) -> BackendResult<BackendAppend> {
        let mut objects = self.objects.lock().expect("objects");
        let object = objects.entry(name.clone()).or_default();
        let start_offset = object.len() as u64;
        object.extend_from_slice(bytes);
        Ok(BackendAppend::new(
            start_offset,
            bytes.len() as u64,
            BackendMetadata::new(object.len() as u64, None),
        ))
    }

    fn sync_object(&self, _name: &ObjectName) -> BackendResult<()> {
        Ok(())
    }

    fn publish_object(
        &self,
        name: &ObjectName,
        bytes: &[u8],
        mode: PublishMode,
    ) -> PublishResult<PublishOutcome> {
        if mode == PublishMode::Replace
            && *name == ObjectLayout::database_manifest().expect("current database object")
        {
            let call = self
                .manifest_replace_calls
                .fetch_add(1, Ordering::SeqCst)
                .saturating_add(1);
            if self.fail_manifest_replace_call.load(Ordering::SeqCst) == call {
                return Err(PublishError::precondition_failed(
                    name,
                    "injected current record replace failure",
                ));
            }
        }
        let mut objects = self.objects.lock().expect("objects");
        if mode == PublishMode::Create && objects.contains_key(name) {
            return Err(PublishError::precondition_failed(
                name,
                "object already exists",
            ));
        }
        objects.insert(name.clone(), bytes.to_vec());
        Ok(PublishOutcome::new(
            name.clone(),
            BackendMetadata::new(bytes.len() as u64, None),
            PublishDurability::Durable,
        ))
    }
}

fn decode_snapshot_object(_name: &ObjectName, bytes: &[u8]) -> bool {
    decode_snapshot_container(bytes).is_ok()
}

struct HeldWriterLock {
    locked: Arc<AtomicBool>,
}

impl Drop for HeldWriterLock {
    fn drop(&mut self) {
        self.locked.store(false, Ordering::SeqCst);
    }
}
