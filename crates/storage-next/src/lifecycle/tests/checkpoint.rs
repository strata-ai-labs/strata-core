use super::*;
use crate::backend::{
    Backend, BackendAppend, BackendCapabilities, BackendError, BackendErrorKind, BackendMetadata,
    BackendRange, BackendResult, BackendWriterGuard, PublishDurability, PublishError,
    PublishFailureKind, PublishMode, PublishOutcome, PublishResult,
    DURABLE_LOCAL_MODE_REQUIREMENTS,
};
use crate::branch::{BranchLocalState, BranchMaterializationRequest, BranchRuntimeConfig};
use crate::commit::{
    CommitBatch, CommitBatchOptions, CommitBranchGeneration, CommitBranchGenerationGuard,
    CommitConflictValidationMode, CommitDuplicateKeyPolicy, CommitDurabilityMode, CommitExpiry,
    CommitManualTimestampSource, CommitMutation, CommitOrigin, CommitRetentionHint,
    CommitRuntimeConfig, CommitTimelineEntry, CommitTimelineRows, CommitTimestampPolicy,
    CommitValidationFacts,
};
use crate::format::decode_snapshot_container;
use crate::layout::ObjectLayout;
use crate::object::{ObjectName, ObjectPrefix};
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use crate::service::{DatabaseManifestService, WalRetentionProof, WalRetentionProofSource};
use std::cell::Cell;
use std::collections::BTreeMap;
use std::error::Error;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

mod remaining;
mod shared;

use shared::*;

const DATABASE_ID: [u8; 16] = [0x7d; 16];

#[test]
fn checkpoint_request_rejects_zero_snapshot_id() {
    assert_eq!(
        LifecycleCheckpointRequest::new(branch_id(0x10), 0, Timestamp::from_micros(1)),
        Err(LifecycleError::InvalidConfig {
            field: "checkpoint_snapshot_id",
            reason: "checkpoint snapshot id must be nonzero",
        })
    );
}

#[test]
fn checkpoint_task_rejects_wrong_maintenance_scope() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x0f);
    let shell = assemble_shell(branch, &backend).expect("shell");
    let task = maintenance_task_for_test(31, MaintenanceTaskRequest::wal_truncation());

    let error = checkpoint_request_from_maintenance_task(
        &task,
        branch,
        shell.services().manifest(),
        Timestamp::from_micros(1),
    )
    .expect_err("wrong task rejects");

    assert_eq!(
        error.code(),
        "failed_precondition.lifecycle.maintenance_task"
    );
    assert_eq!(backend.event_count(), 0);
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
fn checkpoint_rows_include_tombstones_and_timeline_rows() {
    let branch = branch_id(0x1b);
    let mut state = BranchLocalState::empty(branch);
    let deleted = StorageRow::tombstone(
        physical_key(branch, b"deleted"),
        CommitVersion::new(1),
        Timestamp::from_micros(100),
    );
    let timeline = CommitTimelineRows::from_entry(
        CommitTimelineEntry::new(branch, CommitVersion::new(2), Timestamp::from_micros(200))
            .expect("timeline entry"),
    )
    .expect("timeline rows")
    .into_rows();

    state
        .append_committed_row(deleted.clone())
        .expect("append deleted row");
    state
        .append_committed_row(timeline[0].clone())
        .expect("append timeline row");
    state
        .append_committed_row(timeline[1].clone())
        .expect("append reverse timeline row");

    let rows = state
        .checkpoint_rows(CommitVersion::new(2))
        .expect("checkpoint rows");

    assert!(rows.iter().any(StorageRow::is_tombstone));
    assert!(rows.contains(&timeline[0]));
    assert!(rows.contains(&timeline[1]));
    assert!(rows
        .iter()
        .all(|row| row.physical_key().branch_id() == branch));
    assert!(rows
        .iter()
        .map(StorageRow::commit_timestamp)
        .any(|timestamp| timestamp == Timestamp::from_micros(200)));
}

#[test]
fn checkpoint_watermark_uses_visible_version_not_allocated_version() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x1c);
    let shell = assemble_shell(branch, &backend).expect("shell");
    let mut state = BranchLocalState::empty(branch);
    let visible = put_row(branch, 1, b"visible-bound", b"value");
    let hidden = put_row(branch, 2, b"hidden-bound", b"value");
    state
        .append_committed_row(visible)
        .expect("append visible row");
    state
        .append_committed_row(hidden)
        .expect("append hidden row");
    let request =
        LifecycleCheckpointRequest::new(branch, 1, Timestamp::from_micros(20)).expect("request");

    let outcome = checkpoint_durable_branch(
        &state,
        shell.services(),
        shell.guard_set(),
        || CommitVersion::new(1),
        &request,
    )
    .expect("checkpoint");

    assert_eq!(outcome.checkpoint_watermark(), Some(CommitVersion::new(1)));
    assert_eq!(outcome.row_count(), 1);
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
fn checkpoint_snapshot_publish_failure_releases_quiesce_and_keeps_recovery_facts() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x1d);
    let shell = assemble_shell(branch, &backend).expect("shell");
    let mut state = BranchLocalState::empty(branch);
    state
        .append_committed_row(put_row(branch, 1, b"snapshot-fail", b"value"))
        .expect("append row");
    backend.fail_snapshot_publish();
    let request =
        LifecycleCheckpointRequest::new(branch, 1, Timestamp::from_micros(21)).expect("request");

    let error = checkpoint_durable_branch(
        &state,
        shell.services(),
        shell.guard_set(),
        || CommitVersion::new(1),
        &request,
    )
    .expect_err("snapshot publish failure");

    assert_eq!(error.code(), "failed_precondition.lifecycle.service");
    assert!(error.source().is_some());
    assert!(!shell.guard_set().is_quiescing().expect("quiesce state"));
    let manifest = DatabaseManifestService::new(&backend)
        .load_required()
        .expect("current database record");
    assert_eq!(manifest.snapshot_id(), None);
    assert_eq!(manifest.snapshot_watermark(), None);
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
fn checkpoint_publishes_snapshot_between_database_record_updates() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x1e);
    let mut runtime = open_runtime(branch, &backend);
    runtime
        .execute_durable_commit(
            durable_batch(branch, b"ordering-key", b"value"),
            generation_guard(),
        )
        .expect("commit");
    let request =
        LifecycleCheckpointRequest::new(branch, 1, Timestamp::from_micros(22)).expect("request");

    let outcome = runtime.checkpoint(&request).expect("checkpoint");

    assert_eq!(outcome.status(), LifecycleCheckpointStatus::Completed);
    assert_eq!(outcome.active_wal_segment(), Some(1));
    assert_eq!(
        backend.checkpoint_events(),
        vec![
            CheckpointBackendEvent::DatabaseRecordReplace,
            CheckpointBackendEvent::SnapshotCreate,
            CheckpointBackendEvent::DatabaseRecordReplace,
        ]
    );
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
fn checkpoint_manifest_publish_failure_reports_partial_snapshot() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x1f);
    let mut runtime = open_runtime(branch, &backend);
    runtime
        .execute_durable_commit(
            durable_batch(branch, b"partial-key", b"value"),
            generation_guard(),
        )
        .expect("commit");
    backend.fail_manifest_replacement_on_call(2);
    let request =
        LifecycleCheckpointRequest::new(branch, 1, Timestamp::from_micros(23)).expect("request");

    let outcome = runtime.checkpoint(&request).expect("partial outcome");

    assert_eq!(
        outcome.status(),
        LifecycleCheckpointStatus::SnapshotPublishedManifestNotUpdated
    );
    assert_eq!(outcome.snapshot_id(), Some(1));
    assert!(outcome.snapshot_object().is_some());
    assert_eq!(
        outcome.failure().expect("orphan fact").code(),
        "unknown.lifecycle.checkpoint_snapshot"
    );
    assert_eq!(
        outcome
            .maintenance_outcome()
            .source_error()
            .expect("source error")
            .code(),
        "unknown.lifecycle.checkpoint_snapshot"
    );
    assert!(outcome.recovery_health().is_some());
    let manifest = DatabaseManifestService::new(&backend)
        .load_required()
        .expect("current database record");
    assert_eq!(manifest.snapshot_id(), None);
    assert_eq!(manifest.snapshot_watermark(), None);
}

#[test]
fn recovery_ignores_unreferenced_snapshot_after_manifest_failure() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x3d);
    let key = physical_key(branch, b"orphan-key");
    let mut runtime = open_runtime(branch, &backend);
    runtime
        .execute_durable_commit(
            durable_batch(branch, b"orphan-key", b"value"),
            generation_guard(),
        )
        .expect("commit");
    backend.fail_manifest_replacement_on_call(2);
    let request =
        LifecycleCheckpointRequest::new(branch, 1, Timestamp::from_micros(29)).expect("request");

    let outcome = runtime.checkpoint(&request).expect("partial outcome");
    let orphan = outcome.snapshot_object().expect("snapshot object").clone();
    drop(runtime);
    let reopened = open_runtime(branch, &backend);

    assert!(backend.read_object(&orphan).is_ok());
    assert_eq!(
        DatabaseManifestService::new(&backend)
            .load_required()
            .expect("manifest")
            .snapshot_id(),
        None
    );
    assert_eq!(
        reopened
            .read_view()
            .expect("view")
            .latest(&key)
            .expect("read")
            .expect("visible")
            .row()
            .value(),
        b"value"
    );
}

#[test]
fn checkpoint_manifest_uncertainty_reports_uncertain_status() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x20);
    let mut runtime = open_runtime(branch, &backend);
    runtime
        .execute_durable_commit(
            durable_batch(branch, b"uncertain-key", b"value"),
            generation_guard(),
        )
        .expect("commit");
    backend.uncertain_manifest_replacement_on_call(2);
    let request =
        LifecycleCheckpointRequest::new(branch, 1, Timestamp::from_micros(24)).expect("request");

    let outcome = runtime.checkpoint(&request).expect("uncertain outcome");

    assert_eq!(
        outcome.status(),
        LifecycleCheckpointStatus::SnapshotVisibilityUncertain
    );
    assert_eq!(
        outcome.maintenance_outcome().status(),
        MaintenanceOutcomeStatus::Failed
    );
}

#[test]
fn checkpoint_existing_snapshot_id_collision_fails_closed() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x21);
    let mut runtime = open_runtime(branch, &backend);
    runtime
        .execute_durable_commit(
            durable_batch(branch, b"collision-key", b"value"),
            generation_guard(),
        )
        .expect("commit");
    let request =
        LifecycleCheckpointRequest::new(branch, 1, Timestamp::from_micros(25)).expect("request");
    runtime.checkpoint(&request).expect("first checkpoint");

    let error = runtime
        .checkpoint(&request)
        .expect_err("second checkpoint rejects");

    assert_eq!(
        error.code(),
        "failed_precondition.lifecycle.checkpoint_publication"
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
fn checkpoint_with_truncation_skips_delete_when_deferred() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x22);
    let shell = assemble_shell(branch, &backend).expect("shell");
    let request = LifecycleCheckpointRequest::new(branch, 1, Timestamp::from_micros(26))
        .expect("request")
        .with_wal_truncation_after_checkpoint(true);

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
    assert_eq!(outcome.wal_truncation(), None);
    assert_eq!(backend.list_calls(), 0);
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

    assert_eq!(outcome.status(), LifecycleCheckpointStatus::Completed);
    assert_eq!(outcome.snapshot_id(), Some(1));
    assert!(outcome.snapshot_object().is_some());
    assert!(outcome.wal_truncation().is_none());
    assert!(outcome.recovery_health().is_some());
    assert_eq!(
        outcome.maintenance_outcome().status(),
        MaintenanceOutcomeStatus::Completed
    );
    assert!(!outcome.maintenance_outcome().retryable());
}

#[test]
fn checkpoint_recovery_restores_rows_without_covered_log_records() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x23);
    let mut runtime = open_runtime(branch, &backend);
    let key = physical_key(branch, b"recover-from-checkpoint");
    runtime
        .execute_durable_commit(
            durable_batch(branch, b"recover-from-checkpoint", b"value"),
            generation_guard(),
        )
        .expect("commit");
    let request = LifecycleCheckpointRequest::new(branch, 1, Timestamp::from_micros(27))
        .expect("request")
        .with_wal_truncation_after_checkpoint(true);
    runtime.checkpoint(&request).expect("checkpoint");
    drop(runtime);

    let reopened = open_runtime(branch, &backend);
    let visible = reopened
        .read_view()
        .expect("read view")
        .latest(&key)
        .expect("read latest")
        .expect("visible row");

    assert_eq!(visible.row().value(), b"value");
    assert_eq!(reopened.visible_version(), CommitVersion::new(1));
}

#[test]
fn checkpoint_recovery_restores_tombstone_and_timeline_rows() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x24);
    let mut runtime = open_runtime(branch, &backend);
    let key = physical_key(branch, b"recover-deleted");
    let batch = CommitBatch::mutating(
        branch,
        vec![CommitMutation::delete(key.clone())],
        CommitValidationFacts::empty(),
        CommitBatchOptions::new(
            CommitDurabilityMode::Standard,
            CommitConflictValidationMode::Validate,
            CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::RuntimeGenerated,
            CommitOrigin::StorageRuntime,
        ),
    );
    runtime
        .execute_durable_commit(batch, generation_guard())
        .expect("commit");
    let request =
        LifecycleCheckpointRequest::new(branch, 1, Timestamp::from_micros(28)).expect("request");
    runtime.checkpoint(&request).expect("checkpoint");
    drop(runtime);

    let reopened = open_runtime(branch, &backend);
    let history = reopened
        .read_view()
        .expect("read view")
        .history(&key, crate::branch::BranchHistoryOptions::all())
        .expect("history");

    assert!(history.first().is_some_and(|row| row.row().is_tombstone()));
    assert_eq!(reopened.visible_version(), CommitVersion::new(1));
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
        Err(LifecycleError::WalRetentionProofIncomplete { .. })
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
fn flush_watermark_rejects_bounds_and_preserves_branch_state() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x25);
    let shell = assemble_shell(branch, &backend).expect("shell");
    shell
        .services()
        .manifest()
        .persist_snapshot_facts(4, CommitVersion::new(6))
        .expect("snapshot facts");
    let before = shell.branch_state().facts();

    let above_checkpoint = persist_flush_watermark(
        shell.services().manifest(),
        CommitVersion::new(8),
        LifecycleFlushWatermarkRequest::new(
            CommitVersion::new(7),
            LifecycleFlushWatermarkProof::CheckpointCovered {
                snapshot_watermark: CommitVersion::new(6),
            },
        )
        .expect("covered request"),
    )
    .expect_err("above checkpoint rejects");
    let above_visible = persist_flush_watermark(
        shell.services().manifest(),
        CommitVersion::new(5),
        LifecycleFlushWatermarkRequest::new(
            CommitVersion::new(7),
            LifecycleFlushWatermarkProof::CheckpointCovered {
                snapshot_watermark: CommitVersion::new(7),
            },
        )
        .expect("covered request"),
    )
    .expect_err("above visible rejects");
    let already_not_persisted = persist_flush_watermark(
        shell.services().manifest(),
        CommitVersion::new(5),
        LifecycleFlushWatermarkRequest::new(
            CommitVersion::new(5),
            LifecycleFlushWatermarkProof::AlreadyPersisted,
        )
        .expect("already request"),
    )
    .expect_err("not already persisted");

    assert_eq!(
        above_checkpoint.code(),
        "failed_precondition.lifecycle.wal_retention"
    );
    assert_eq!(
        above_visible.code(),
        "failed_precondition.lifecycle.wal_retention"
    );
    assert_eq!(
        already_not_persisted.code(),
        "failed_precondition.lifecycle.wal_retention"
    );
    assert_eq!(shell.branch_state().facts(), before);
}

#[test]
fn flush_watermark_persist_failure_preserves_source_chain() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x26);
    let shell = assemble_shell(branch, &backend).expect("shell");
    shell
        .services()
        .manifest()
        .persist_snapshot_facts(5, CommitVersion::new(7))
        .expect("snapshot facts");
    backend.fail_manifest_replacement_on_call(2);

    let error = persist_flush_watermark(
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
    .expect_err("persist failure");

    assert_eq!(error.code(), "failed_precondition.lifecycle.service");
    assert!(error.source().is_some());
    let manifest = DatabaseManifestService::new(&backend)
        .load_required()
        .expect("current database record");
    assert_eq!(manifest.flushed_through_commit_id(), None);
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
fn wal_truncation_from_checkpoint_and_flush_proofs_are_typed() {
    let snapshot = LifecycleWalTruncationRequest::new(WalRetentionProof::snapshot_watermark(
        CommitVersion::new(3),
    ))
    .expect("snapshot proof");
    let flush = LifecycleWalTruncationRequest::new(WalRetentionProof::flush_watermark(
        CommitVersion::new(4),
    ))
    .expect("flush proof");

    assert_eq!(snapshot.proof().covered_through(), CommitVersion::new(3));
    assert_eq!(
        snapshot.proof().source(),
        WalRetentionProofSource::SnapshotWatermark
    );
    assert_eq!(flush.proof().covered_through(), CommitVersion::new(4));
    assert_eq!(
        flush.proof().source(),
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
fn duplicate_checkpoint_tasks_coalesce_by_checkpoint_scope() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x27);
    let mut runtime = open_runtime(branch, &backend);

    let first = runtime
        .enqueue_maintenance(MaintenanceTaskRequest::checkpoint())
        .expect("first enqueue");
    let second = runtime
        .enqueue_maintenance(MaintenanceTaskRequest::checkpoint())
        .expect("second enqueue");

    assert_eq!(first.status(), MaintenanceEnqueueStatus::Enqueued);
    assert_eq!(second.status(), MaintenanceEnqueueStatus::Coalesced);
    assert_eq!(runtime.maintenance_status().stats().coalesced(), 1);
}

#[test]
fn queued_checkpoint_task_failure_adds_health_debt() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x28);
    let mut runtime = open_runtime(branch, &backend);
    runtime
        .execute_durable_commit(
            durable_batch(branch, b"queued-failure", b"value"),
            generation_guard(),
        )
        .expect("commit");
    backend.fail_manifest_replacement_on_call(2);
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::checkpoint())
        .expect("enqueue");

    let maintenance = runtime
        .run_next_checkpoint_maintenance()
        .expect("run")
        .expect("maintenance");

    assert_eq!(maintenance.status(), MaintenanceOutcomeStatus::Failed);
    assert!(maintenance.recovery_health().is_some());
    assert!(maintenance.retryable());
    assert_eq!(runtime.maintenance_status().stats().failed(), 1);
}

#[test]
fn queued_checkpoint_retry_advances_after_orphaned_snapshot() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x29);
    let mut runtime = open_runtime(branch, &backend);
    runtime
        .execute_durable_commit(
            durable_batch(branch, b"queued-orphan", b"value"),
            generation_guard(),
        )
        .expect("commit");
    backend.fail_manifest_replacement_on_call(2);
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::checkpoint())
        .expect("enqueue first");

    let first = runtime
        .run_next_checkpoint_maintenance()
        .expect("run first")
        .expect("first maintenance");
    assert_eq!(first.status(), MaintenanceOutcomeStatus::Failed);

    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::checkpoint())
        .expect("enqueue second");
    let second = runtime
        .run_next_checkpoint_maintenance()
        .expect("run second")
        .expect("second maintenance");

    assert_eq!(second.status(), MaintenanceOutcomeStatus::Completed);
    assert_eq!(
        DatabaseManifestService::new(&backend)
            .load_required()
            .expect("database record")
            .snapshot_id(),
        Some(2)
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
fn duplicate_wal_truncation_tasks_coalesce_by_retention_scope() {
    let backend = CheckpointTestBackend::new();
    let branch = branch_id(0x29);
    let mut runtime = open_runtime(branch, &backend);

    let first = runtime
        .enqueue_maintenance(MaintenanceTaskRequest::wal_truncation())
        .expect("first enqueue");
    let second = runtime
        .enqueue_maintenance(MaintenanceTaskRequest::wal_truncation())
        .expect("second enqueue");

    assert_eq!(first.status(), MaintenanceEnqueueStatus::Enqueued);
    assert_eq!(second.status(), MaintenanceEnqueueStatus::Coalesced);
    assert_eq!(runtime.maintenance_status().stats().coalesced(), 1);
}

#[test]
fn wal_truncation_request_rejects_zero_proof() {
    assert_eq!(
        LifecycleWalTruncationRequest::new(WalRetentionProof::snapshot_watermark(
            CommitVersion::ZERO,
        )),
        Err(LifecycleError::WalRetentionProofIncomplete {
            reason: "WAL retention proof must be nonzero",
        })
    );
}
