use super::*;
use crate::backend::{
    Backend, BackendAppend, BackendCapabilities, BackendError, BackendErrorKind, BackendMetadata,
    BackendRange, BackendResult, BackendWriterGuard, PublishDurability, PublishError,
    PublishFailureKind, PublishMode, PublishOutcome, PublishResult,
    DURABLE_LOCAL_MODE_REQUIREMENTS,
};
use crate::branch::BranchRuntimeConfig;
use crate::commit::{
    CommitBatch, CommitBatchOptions, CommitBranchGeneration, CommitBranchGenerationGuard,
    CommitConflictValidationMode, CommitDuplicateKeyPolicy, CommitDurabilityClass,
    CommitDurabilityMode, CommitExpiry, CommitManualTimestampSource, CommitMutation, CommitOrigin,
    CommitRetentionHint, CommitRuntimeConfig, CommitStamp, CommitTimestampPolicy,
    CommitUnresolvedDurable, CommitValidationFacts,
};
use crate::config::mode::DurabilityPolicy;
use crate::format::{
    encode_manifest, encode_wal_segment_header, DatabaseManifest, WalSegmentHeader,
};
use crate::layout::ObjectLayout;
use crate::object::{ObjectName, ObjectPrefix};
use crate::row::{PhysicalKey, StorageSpaceId};
use crate::service::WalServiceConfig;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

const DATABASE_ID: [u8; 16] = [0x8e; 16];
const OTHER_DATABASE_ID: [u8; 16] = [0x8f; 16];

#[test]
fn durable_assembly_creates_manifest_opens_wal_and_remains_recovering() {
    let backend = DurableTestBackend::new();
    let branch = branch_id(0x10);
    let shell =
        assemble_shell(StorageMode::DurableLocalStandard, branch, &backend).expect("durable shell");

    assert_eq!(shell.state(), LifecycleState::Recovering);
    assert_eq!(
        shell.open_plan().storage_mode(),
        StorageMode::DurableLocalStandard
    );
    assert_eq!(
        shell.assembly_facts().mode(),
        StorageMode::DurableLocalStandard
    );
    assert_eq!(
        shell.assembly_facts().disposition(),
        StorageOpenDisposition::Created
    );
    assert_eq!(shell.assembly_facts().database_id(), &DATABASE_ID);
    assert_eq!(shell.assembly_facts().codec_id(), "identity");
    assert_eq!(
        shell.assembly_facts().durability_policy(),
        DurabilityPolicy::Standard
    );
    assert_eq!(shell.assembly_facts().active_wal_segment(), 1);
    assert_eq!(
        shell.assembly_facts().writer_lock_object(),
        &ObjectLayout::writer_lock().expect("writer lock")
    );
    assert_eq!(shell.assembly_facts().manifest_snapshot_watermark(), None);
    assert_eq!(shell.assembly_facts().manifest_snapshot_id(), None);
    assert_eq!(shell.assembly_facts().manifest_flush_watermark(), None);
    assert_eq!(shell.services().wal().active_segment_id(), 1);
    assert_eq!(
        shell.services().wal().durability_policy(),
        DurabilityPolicy::Standard
    );
    assert_eq!(
        shell.services().capability_outcome().storage_mode(),
        StorageMode::DurableLocalStandard
    );
    assert!(shell.branch_state().is_empty());
    assert_eq!(shell.branch_state().branch_id(), branch);
    assert_eq!(shell.visible_version(), CommitVersion::ZERO);
    assert_eq!(shell.unresolved_durable().expect("gate"), None);
    assert!(shell.admit_recovery_step().is_ok());
    assert!(shell.admit_ordinary_read().is_err());
    assert!(shell.admit_commit().is_err());
    assert!(shell.admit_ordinary_maintenance().is_err());
    assert!(shell.admit_health_query().is_ok());
    touch_shell_parts(&shell);

    let operations = backend.operations();
    assert_call_order(
        &operations,
        OperationKind::Capabilities,
        OperationKind::AcquireWriterLock,
    );
    assert_call_order(
        &operations,
        OperationKind::AcquireWriterLock,
        OperationKind::ReadObject,
    );
    assert!(operations.iter().any(|operation| {
        matches!(operation, Operation::AcquireWriterLock(object) if object == &ObjectLayout::writer_lock().expect("writer lock"))
    }));
    assert!(operations
        .iter()
        .any(|operation| matches!(operation, Operation::Publish(object, PublishMode::Create) if object == &ObjectLayout::database_manifest().expect("database object"))));
    assert!(operations
        .iter()
        .any(|operation| matches!(operation, Operation::ObjectMetadata(object) if object == &ObjectLayout::wal_segment(1).expect("segment object"))));
    assert!(!operations
        .iter()
        .any(|operation| matches!(operation, Operation::ListPrefix(_))));
    assert!(backend.lock_is_held());
    drop(shell);
    assert!(!backend.lock_is_held());
}

#[test]
fn durable_assembly_loads_existing_manifest_and_preserves_recovery_facts() {
    let backend = DurableTestBackend::new();
    let manifest = DatabaseManifest::new(DATABASE_ID, "identity")
        .expect("database object")
        .with_recovery_facts(7, Some(44), Some(3), Some(CommitVersion::new(43)))
        .expect("recovery facts");
    backend.write_raw(
        ObjectLayout::database_manifest().expect("manifest object"),
        encode_manifest(&manifest).expect("manifest bytes"),
    );

    let shell = assemble_shell(StorageMode::DurableLocalAlways, branch_id(0x11), &backend)
        .expect("durable shell");

    assert_eq!(
        shell.assembly_facts().disposition(),
        StorageOpenDisposition::OpenedExisting
    );
    assert_eq!(
        shell.assembly_facts().durability_policy(),
        DurabilityPolicy::Always
    );
    assert_eq!(shell.assembly_facts().active_wal_segment(), 7);
    assert_eq!(
        shell.assembly_facts().manifest_snapshot_watermark(),
        Some(44)
    );
    assert_eq!(shell.assembly_facts().manifest_snapshot_id(), Some(3));
    assert_eq!(
        shell.assembly_facts().manifest_flush_watermark(),
        Some(CommitVersion::new(43))
    );
    assert_eq!(shell.services().wal().active_segment_id(), 7);
    assert_eq!(
        shell.services().wal().durability_policy(),
        DurabilityPolicy::Always
    );

    let manifest_object = ObjectLayout::database_manifest().expect("manifest object");
    assert!(!backend.operations().iter().any(|operation| {
        matches!(operation, Operation::Publish(object, _) if object == &manifest_object)
    }));
}

#[test]
fn durable_request_rejects_non_durable_modes_without_backend_calls() {
    let backend = DurableTestBackend::new();
    for mode in [StorageMode::Cache, StorageMode::ObjectDurableCandidate] {
        assert_eq!(
            request(mode, branch_id(0x12)),
            Err(LifecycleError::InvalidOpenPlan {
                reason: "durable local assembly requires durable local storage mode",
            })
        );
    }
    assert!(backend.operations().is_empty());
}

#[test]
fn durable_request_rejects_codec_mismatch_before_backend_calls() {
    let backend = DurableTestBackend::new();
    let plan = StorageOpenPlan::new(
        StorageMode::DurableLocalStandard,
        LifecycleCodecId::new("zstd").expect("codec"),
        RecoveryStrictness::Strict,
        LifecycleConfig::default(),
    )
    .expect("plan");
    let error = LifecycleDurableLocalOpenRequest::new(
        plan,
        DATABASE_ID,
        branch_id(0x13),
        CommitBranchGeneration::new(1).expect("generation"),
        BranchRuntimeConfig::default(),
        CommitRuntimeConfig::default(),
        WalServiceConfig::default(),
    )
    .expect_err("WAL codec mismatch rejects");

    assert_eq!(
        error,
        LifecycleError::InvalidOpenPlan {
            reason: "durable open plan codec must match WAL codec",
        }
    );
    assert!(backend.operations().is_empty());
}

#[test]
fn durable_request_rejects_invalid_wal_config_before_backend_calls() {
    let backend = DurableTestBackend::new();
    let error = LifecycleDurableLocalOpenRequest::new(
        open_plan(StorageMode::DurableLocalStandard),
        DATABASE_ID,
        branch_id(0x13),
        CommitBranchGeneration::new(1).expect("generation"),
        BranchRuntimeConfig::default(),
        CommitRuntimeConfig::default(),
        WalServiceConfig::new(1),
    )
    .expect_err("invalid WAL config rejects");

    assert!(matches!(
        error,
        LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::Service,
            reason: "WAL service failed",
            ..
        }
    ));
    assert!(error.source().is_some());
    assert!(backend.operations().is_empty());
}

#[test]
fn durable_capability_rejection_happens_before_writer_lock() {
    let backend = DurableTestBackend::with_capabilities(BackendCapabilities::empty());
    let error = LifecycleDurableLocalShell::assemble(
        request(StorageMode::DurableLocalStandard, branch_id(0x14)).expect("request"),
        &backend,
        timestamp_source(),
    )
    .expect_err("capability mismatch");

    assert!(matches!(error, LifecycleError::CapabilityMismatch { .. }));
    assert_eq!(backend.operation_kinds(), vec![OperationKind::Capabilities]);
}

#[test]
fn durable_writer_lock_failure_happens_before_manifest_access() {
    let backend = DurableTestBackend::with_lock_failure();
    let error = LifecycleDurableLocalShell::assemble(
        request(StorageMode::DurableLocalStandard, branch_id(0x15)).expect("request"),
        &backend,
        timestamp_source(),
    )
    .expect_err("writer lock failure");

    assert!(matches!(
        error,
        LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::Backend,
            ..
        }
    ));
    assert_eq!(
        backend.operation_kinds(),
        vec![
            OperationKind::Capabilities,
            OperationKind::AcquireWriterLock
        ]
    );
}

#[test]
fn durable_manifest_identity_mismatch_rejects_before_wal_open() {
    let backend = DurableTestBackend::new();
    let manifest = DatabaseManifest::new(OTHER_DATABASE_ID, "identity").expect("database object");
    backend.write_raw(
        ObjectLayout::database_manifest().expect("manifest object"),
        encode_manifest(&manifest).expect("manifest bytes"),
    );

    let error = LifecycleDurableLocalShell::assemble(
        request(StorageMode::DurableLocalStandard, branch_id(0x16)).expect("request"),
        &backend,
        timestamp_source(),
    )
    .expect_err("manifest identity mismatch");

    assert_eq!(
        error,
        LifecycleError::InvalidOpenPlan {
            reason: "database manifest id does not match durable open request",
        }
    );
    assert!(!backend.lock_is_held());
    assert!(!backend
        .operations()
        .iter()
        .any(|operation| matches!(operation, Operation::ObjectMetadata(_))));
}

#[test]
fn durable_manifest_codec_mismatch_rejects_before_wal_open() {
    let backend = DurableTestBackend::new();
    let manifest = DatabaseManifest::new(DATABASE_ID, "zstd").expect("database object");
    backend.write_raw(
        ObjectLayout::database_manifest().expect("database object"),
        encode_manifest(&manifest).expect("encoded object"),
    );

    let error = LifecycleDurableLocalShell::assemble(
        request(StorageMode::DurableLocalStandard, branch_id(0x16)).expect("request"),
        &backend,
        timestamp_source(),
    )
    .expect_err("manifest codec mismatch");

    assert_eq!(
        error,
        LifecycleError::InvalidOpenPlan {
            reason: "database manifest codec does not match durable open request",
        }
    );
    assert!(!backend.lock_is_held());
    assert!(!backend
        .operations()
        .iter()
        .any(|operation| matches!(operation, Operation::ObjectMetadata(_))));
}

#[test]
fn durable_manifest_publish_uncertainty_preserves_source_chain() {
    for (kind, expected_reason) in [
        (
            PublishFailureKind::VisibleDurabilityUnconfirmed,
            "database manifest publish durability unconfirmed",
        ),
        (
            PublishFailureKind::VisibilityUnknown,
            "database manifest publish visibility unknown",
        ),
        (
            PublishFailureKind::FailedBeforeVisibility,
            "database manifest publish failed before visibility",
        ),
        (
            PublishFailureKind::Unsupported,
            "database manifest publish unsupported",
        ),
    ] {
        let backend = DurableTestBackend::with_publish_failure(kind);
        let error = LifecycleDurableLocalShell::assemble(
            request(StorageMode::DurableLocalStandard, branch_id(0x17)).expect("request"),
            &backend,
            timestamp_source(),
        )
        .expect_err("publish fault should reject");

        assert!(matches!(
            error,
            LifecycleError::LowerLayer {
                layer: LifecycleLowerLayer::Service,
                reason,
                ..
            } if reason == expected_reason
        ));
        assert!(
            error.source().is_some(),
            "publish failure should retain lower-layer source"
        );
        assert!(!backend.lock_is_held());
    }
}

#[test]
fn durable_manifest_create_precondition_race_reloads_existing_manifest() {
    let race_manifest = DatabaseManifest::new(DATABASE_ID, "identity")
        .expect("database object")
        .with_recovery_facts(9, Some(88), Some(5), Some(CommitVersion::new(87)))
        .expect("recovery facts");
    let backend = DurableTestBackend::with_create_race(race_manifest);

    let shell = assemble_shell(StorageMode::DurableLocalStandard, branch_id(0x18), &backend)
        .expect("durable shell");

    assert_eq!(
        shell.assembly_facts().disposition(),
        StorageOpenDisposition::OpenedExisting
    );
    assert_eq!(shell.assembly_facts().active_wal_segment(), 9);
    assert_eq!(
        shell.assembly_facts().manifest_flush_watermark(),
        Some(CommitVersion::new(87))
    );

    let operations = backend.operations();
    assert_eq!(
        operations
            .iter()
            .filter(|operation| matches!(operation, Operation::ReadObject(_)))
            .count(),
        2
    );
    assert!(operations
        .iter()
        .any(|operation| matches!(operation, Operation::Publish(_, PublishMode::Create))));
}

#[test]
fn durable_manifest_create_precondition_race_reloads_and_revalidates_identity() {
    let race_manifest =
        DatabaseManifest::new(OTHER_DATABASE_ID, "identity").expect("database object");
    let backend = DurableTestBackend::with_create_race(race_manifest);

    let error = LifecycleDurableLocalShell::assemble(
        request(StorageMode::DurableLocalStandard, branch_id(0x19)).expect("request"),
        &backend,
        timestamp_source(),
    )
    .expect_err("race mismatch should reject");

    assert_eq!(
        error,
        LifecycleError::InvalidOpenPlan {
            reason: "database manifest id does not match durable open request",
        }
    );
    assert!(!backend.lock_is_held());
    assert!(!backend
        .operations()
        .iter()
        .any(|operation| matches!(operation, Operation::ObjectMetadata(_))));
}

#[test]
fn durable_existing_manifest_decode_failures_reject_before_wal_open() {
    let valid_manifest = DatabaseManifest::new(DATABASE_ID, "identity").expect("database object");
    let mut bad_checksum = encode_manifest(&valid_manifest).expect("encoded object");
    let checksum_byte = bad_checksum.last_mut().expect("checksum byte");
    *checksum_byte = checksum_byte.wrapping_add(1);

    let mut future_version = encode_manifest(&valid_manifest).expect("encoded object");
    future_version[4..8].copy_from_slice(&u32::MAX.to_le_bytes());

    let mut pre_v1_version = encode_manifest(&valid_manifest).expect("encoded object");
    pre_v1_version[4..8].copy_from_slice(&0_u32.to_le_bytes());

    let mut zero_active_segment = encode_manifest(&valid_manifest).expect("encoded object");
    let active_segment_offset = 4 + 4 + 16 + 4 + valid_manifest.codec_id().len();
    zero_active_segment[active_segment_offset..active_segment_offset + 8]
        .copy_from_slice(&0_u64.to_le_bytes());
    refresh_manifest_crc(&mut zero_active_segment);

    for bytes in [
        vec![0; 8],
        bad_checksum,
        future_version,
        pre_v1_version,
        zero_active_segment,
    ] {
        let backend = DurableTestBackend::new();
        backend.write_raw(
            ObjectLayout::database_manifest().expect("database object"),
            bytes,
        );

        let error = LifecycleDurableLocalShell::assemble(
            request(StorageMode::DurableLocalStandard, branch_id(0x1a)).expect("request"),
            &backend,
            timestamp_source(),
        )
        .expect_err("invalid database object should reject");

        assert!(matches!(
            error,
            LifecycleError::LowerLayer {
                layer: LifecycleLowerLayer::Service,
                reason: "database manifest decode failed",
                ..
            }
        ));
        assert!(!backend.lock_is_held());
        assert!(!backend
            .operations()
            .iter()
            .any(|operation| matches!(operation, Operation::ObjectMetadata(_))));
    }
}

#[test]
fn durable_wal_open_failures_are_typed_and_do_not_mark_open() {
    let metadata_failure = DurableTestBackend::with_metadata_failure();
    write_existing_manifest(&metadata_failure, &manifest_with_active_segment(4));
    let metadata_error = LifecycleDurableLocalShell::assemble(
        request(StorageMode::DurableLocalStandard, branch_id(0x1b)).expect("request"),
        &metadata_failure,
        timestamp_source(),
    )
    .expect_err("WAL metadata failure should reject");
    assert!(matches!(
        metadata_error,
        LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::Service,
            reason: "WAL service failed",
            ..
        }
    ));
    assert!(metadata_error.source().is_some());
    assert!(!metadata_failure.lock_is_held());

    let publish_failure =
        DurableTestBackend::with_publish_failure(PublishFailureKind::FailedBeforeVisibility);
    write_existing_manifest(&publish_failure, &manifest_with_active_segment(5));
    let publish_error = LifecycleDurableLocalShell::assemble(
        request(StorageMode::DurableLocalStandard, branch_id(0x1c)).expect("request"),
        &publish_failure,
        timestamp_source(),
    )
    .expect_err("WAL create failure should reject");
    assert!(matches!(
        publish_error,
        LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::Service,
            reason: "WAL service failed",
            ..
        }
    ));
    assert!(publish_error.source().is_some());
    assert!(!publish_failure.lock_is_held());
}

#[test]
fn durable_wal_header_database_mismatch_rejects_existing_segment() {
    let backend = DurableTestBackend::new();
    write_existing_manifest(&backend, &manifest_with_active_segment(6));
    let wrong_header = WalSegmentHeader::new(6, OTHER_DATABASE_ID);
    backend.write_raw(
        ObjectLayout::wal_segment(6).expect("segment object"),
        encode_wal_segment_header(&wrong_header),
    );

    let error = LifecycleDurableLocalShell::assemble(
        request(StorageMode::DurableLocalStandard, branch_id(0x1d)).expect("request"),
        &backend,
        timestamp_source(),
    )
    .expect_err("wrong segment header database should reject");

    assert!(matches!(
        error,
        LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::Service,
            reason: "WAL service failed",
            ..
        }
    ));
    assert!(error.source().is_some());
    assert!(!backend.lock_is_held());
}

fn refresh_manifest_crc(bytes: &mut [u8]) {
    let checksum_offset = bytes.len() - 4;
    let crc = crc32fast::hash(&bytes[..checksum_offset]);
    bytes[checksum_offset..].copy_from_slice(&crc.to_le_bytes());
}

#[cfg(all(feature = "localfs", unix))]
#[test]
fn durable_localfs_writer_lock_excludes_second_shell_until_drop() {
    use crate::backend::local_fs::LocalFsBackend;

    let dir = tempfile::tempdir().expect("temp dir");
    let first_backend = LocalFsBackend::new(dir.path());
    let second_backend = LocalFsBackend::new(dir.path());
    let first = LifecycleDurableLocalShell::assemble(
        request(StorageMode::DurableLocalStandard, branch_id(0x1e)).expect("request"),
        &first_backend,
        timestamp_source(),
    )
    .expect("first durable shell");

    let blocked = LifecycleDurableLocalShell::assemble(
        request(StorageMode::DurableLocalStandard, branch_id(0x1e)).expect("request"),
        &second_backend,
        timestamp_source(),
    )
    .expect_err("second durable shell should be blocked by writer guard");
    assert!(matches!(
        blocked,
        LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::Backend,
            ..
        }
    ));

    drop(first);

    let second = LifecycleDurableLocalShell::assemble(
        request(StorageMode::DurableLocalStandard, branch_id(0x1e)).expect("request"),
        &second_backend,
        timestamp_source(),
    )
    .expect("second durable shell after guard release");
    assert_eq!(
        second.assembly_facts().disposition(),
        StorageOpenDisposition::OpenedExisting
    );
}

#[test]
fn durable_close_syncs_log_releases_writer_guard_and_is_idempotent() {
    let backend = DurableTestBackend::new();
    let branch = branch_id(0x20);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    runtime
        .execute_durable_commit(
            durable_put_batch(branch, b"close-sync", b"value"),
            generation_guard(),
        )
        .expect("durable commit before close");

    assert!(backend.lock_is_held());
    assert!(runtime.services().writer_guard().is_some());
    let close = runtime.close().expect("durable close");

    assert_eq!(runtime.state(), LifecycleState::Closed);
    assert_eq!(close.phase(), ClosePhase::Closed);
    assert_eq!(close.status(), CloseOutcomeStatus::Complete);
    assert_eq!(close.close_fact(), Some(LifecycleCloseFact::Complete));
    assert!(close.commits_quiesced());
    assert!(close.maintenance_drained());
    assert!(close.durable_synced());
    assert!(close.guards_released());
    assert_eq!(close.stats().close_attempts(), 1);
    assert_eq!(close.stats().maintenance_tasks(), 0);
    assert!(!backend.lock_is_held());
    assert!(runtime.services().writer_guard().is_none());
    assert!(backend
        .operations()
        .iter()
        .any(|operation| matches!(operation, Operation::SyncObject(_))));

    let operations_after_first_close = backend.operations().len();
    let second = runtime.close().expect("idempotent close");
    assert_eq!(second.status(), CloseOutcomeStatus::Idempotent);
    assert_eq!(second.close_fact(), Some(LifecycleCloseFact::AlreadyClosed));
    assert!(second.prior_final());
    assert_eq!(backend.operations().len(), operations_after_first_close);
    // Idempotent close must surface the same stats as the first close —
    // a retry that fabricates `(0, 0, 0, 0, 1)` would drift from the
    // observable record. The cached prior-close outcome guarantees this.
    assert_eq!(second.stats(), close.stats());
}

#[test]
fn durable_close_calls_wal_close_in_always_mode() {
    let backend = DurableTestBackend::new();
    let branch = branch_id(0x24);
    let mut runtime = open_runtime(StorageMode::DurableLocalAlways, branch, &backend);
    runtime
        .execute_durable_commit(
            durable_put_batch_with_mode(
                branch,
                b"always-close",
                b"value",
                CommitDurabilityMode::Always,
            ),
            generation_guard(),
        )
        .expect("durable commit before close");
    let close = runtime.close().expect("durable close");

    assert_eq!(close.status(), CloseOutcomeStatus::Complete);
    assert!(close.durable_synced());
    assert!(!backend.lock_is_held());
}

#[test]
fn durable_close_does_not_report_complete_with_unresolved_durable_gate() {
    let backend = DurableTestBackend::new();
    let branch = branch_id(0x25);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    let unresolved = CommitUnresolvedDurable::durable_not_applied_with_facts(
        CommitStamp::new(branch, CommitVersion::new(4), Timestamp::from_micros(8_004))
            .expect("stamp"),
        CommitDurabilityClass::Standard,
        "seed unresolved durable fact",
    )
    .expect("unresolved fact");
    runtime
        .durable_gate()
        .record_unresolved(unresolved)
        .expect("record unresolved");
    let operations_before_close = backend.operations().len();

    let error = runtime
        .close()
        .expect_err("unresolved durable blocks close");
    let close_operations = backend.operations()[operations_before_close..].to_vec();

    assert_eq!(error.code(), "failed_precondition.lifecycle.close");
    assert_eq!(runtime.state(), LifecycleState::Closing);
    assert!(backend.lock_is_held());
    assert!(!close_operations
        .iter()
        .any(|operation| matches!(operation, Operation::SyncObject(_))));
}

#[test]
fn durable_close_does_not_truncate_wal_prune_snapshots_or_purge_quarantine_implicitly() {
    let backend = DurableTestBackend::new();
    let branch = branch_id(0x26);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    runtime
        .execute_durable_commit(
            durable_put_batch(branch, b"close-no-retention", b"value"),
            generation_guard(),
        )
        .expect("durable commit");
    let operations_before_close = backend.operations().len();

    runtime.close().expect("durable close");
    let close_operations = backend.operations()[operations_before_close..].to_vec();

    assert!(close_operations
        .iter()
        .any(|operation| matches!(operation, Operation::SyncObject(_))));
    assert!(!close_operations
        .iter()
        .any(|operation| matches!(operation, Operation::DeleteObject(_))));
    assert!(!close_operations
        .iter()
        .any(|operation| matches!(operation, Operation::ListPrefix(_))));
    assert!(!close_operations
        .iter()
        .any(|operation| matches!(operation, Operation::Publish(_, _))));
}

#[test]
fn durable_reopen_can_acquire_writer_guard_after_close() {
    let backend = DurableTestBackend::new();
    let branch = branch_id(0x27);
    let mut first = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    assert!(backend.lock_is_held());

    first.close().expect("first close");
    assert!(!backend.lock_is_held());

    let second = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    assert_eq!(second.state(), LifecycleState::Open);
    assert!(backend.lock_is_held());
}

#[test]
fn second_durable_runtime_can_open_after_first_clean_close() {
    durable_reopen_can_acquire_writer_guard_after_close();
}

#[test]
fn durable_close_calls_wal_close_in_standard_mode() {
    let backend = DurableTestBackend::new();
    let branch = branch_id(0x29);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    runtime
        .execute_durable_commit(
            durable_put_batch(branch, b"standard-close", b"value"),
            generation_guard(),
        )
        .expect("durable commit");

    let close = runtime.close().expect("durable close");

    assert_eq!(close.status(), CloseOutcomeStatus::Complete);
    assert!(close.durable_synced());
    assert!(!backend.lock_is_held());
}

#[test]
fn durable_close_wal_close_failure_returns_typed_source_chain() {
    let backend = DurableTestBackend::with_sync_failure();
    let branch = branch_id(0x2a);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    runtime
        .execute_durable_commit(
            durable_put_batch(branch, b"wal-close-failure", b"value"),
            generation_guard(),
        )
        .expect("durable commit");

    let error = runtime.close().expect_err("WAL close failure");

    assert_eq!(error.code(), "failed_precondition.lifecycle.service");
    assert!(error.source().is_some());
    assert_eq!(runtime.state(), LifecycleState::Closing);
}

#[test]
fn durable_close_wal_sync_uncertain_returns_retry_pending() {
    let backend = DurableTestBackend::with_sync_failure();
    let branch = branch_id(0x2b);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    runtime
        .execute_durable_commit(
            durable_put_batch(branch, b"wal-sync-uncertain", b"value"),
            generation_guard(),
        )
        .expect("durable commit");

    let error = runtime.close().expect_err("sync failure");

    assert_eq!(error.code(), "failed_precondition.lifecycle.service");
    assert_eq!(runtime.state(), LifecycleState::Closing);
}

#[test]
fn durable_close_does_not_release_writer_guard_before_sync_failure() {
    let backend = DurableTestBackend::with_sync_failure();
    let branch = branch_id(0x2c);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    runtime
        .execute_durable_commit(
            durable_put_batch(branch, b"sync-before-release", b"value"),
            generation_guard(),
        )
        .expect("durable commit");

    let error = runtime.close().expect_err("sync failure");

    assert_eq!(error.code(), "failed_precondition.lifecycle.service");
    assert!(backend.lock_is_held());
    assert_eq!(backend.release_count(), 0);
}

#[test]
fn durable_close_releases_writer_guard_after_sync() {
    let backend = DurableTestBackend::new();
    let branch = branch_id(0x2d);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    runtime
        .execute_durable_commit(
            durable_put_batch(branch, b"release-after-sync", b"value"),
            generation_guard(),
        )
        .expect("durable commit");

    let close = runtime.close().expect("durable close");

    assert!(close.durable_synced());
    assert!(close.guards_released());
    assert_eq!(backend.release_count(), 1);
    assert!(!backend.lock_is_held());
}

#[test]
fn durable_double_close_does_not_double_release_writer_guard() {
    let backend = DurableTestBackend::new();
    let branch = branch_id(0x2e);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    runtime.close().expect("first close");
    let releases_after_first = backend.release_count();

    let second = runtime.close().expect("second close");

    assert_eq!(second.status(), CloseOutcomeStatus::Idempotent);
    assert_eq!(backend.release_count(), releases_after_first);
}

#[test]
fn durable_close_reports_typed_error_when_writer_guard_is_missing_at_release() {
    let backend = DurableTestBackend::new();
    let branch = branch_id(0x52);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    assert!(runtime.release_writer_guard_for_test());

    let error = runtime.close().expect_err("missing guard rejects close");

    assert_eq!(error.code(), "failed_precondition.lifecycle.close");
    assert!(matches!(error, LifecycleError::CloseFailed { reason }
            if reason == "writer guard was already released before close completed"));
    assert_eq!(runtime.state(), LifecycleState::Closing);
    assert!(!backend.lock_is_held());
}

#[test]
fn durable_failed_close_keeps_guard_when_retry_requires_it() {
    let backend = DurableTestBackend::with_sync_failure();
    let branch = branch_id(0x2f);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    runtime
        .execute_durable_commit(
            durable_put_batch(branch, b"failed-close-keeps-guard", b"value"),
            generation_guard(),
        )
        .expect("durable commit");

    assert!(runtime.close().is_err());

    assert_eq!(runtime.state(), LifecycleState::Closing);
    assert!(backend.lock_is_held());
    assert!(runtime.services().writer_guard().is_some());
}

#[test]
fn durable_retry_after_release_does_not_use_released_guard() {
    let backend = DurableTestBackend::new();
    let branch = branch_id(0x30);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    runtime.close().expect("first close");
    let operations_after_first = backend.operations().len();
    backend.set_sync_failure(true);

    let second = runtime.close().expect("second close");

    assert_eq!(second.status(), CloseOutcomeStatus::Idempotent);
    assert_eq!(backend.operations().len(), operations_after_first);
    assert_eq!(backend.release_count(), 1);
}

#[test]
fn double_close_after_success_does_not_touch_backend() {
    durable_retry_after_release_does_not_use_released_guard();
}

#[test]
fn durable_close_skips_manifest_write_when_no_final_fact_dirty() {
    let backend = DurableTestBackend::new();
    let branch = branch_id(0x32);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    let operations_before_close = backend.operations().len();

    runtime.close().expect("durable close");
    let close_operations = backend.operations()[operations_before_close..].to_vec();

    assert!(!close_operations
        .iter()
        .any(|operation| matches!(operation, Operation::Publish(_, _))));
}

#[test]
fn durable_close_force_syncs_manifest_when_health_changed() {
    // V1 lifecycle health is *not* persisted in the manifest payload (see
    // `force_final_manifest_fsync_on_health_change` doc). What we assert
    // here is the durability tighten the hook is responsible for: when
    // recovery health degraded during the session, close re-publishes the
    // existing manifest at PublishMode::Replace to force one final
    // backend fsync before the writer guard releases. The republished
    // bytes are byte-identical to the bytes that were loaded — the value
    // of the operation is the durable barrier, not a new payload.
    let backend = DurableTestBackend::new();
    let branch = branch_id(0x33);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    let health = close_health_debt();
    runtime.record_recovery_health_for_test(&health);

    let manifest_before = backend
        .last_manifest_bytes()
        .expect("manifest present after open");
    let operations_before_close = backend.operations().len();
    let close = runtime.close().expect("close with manifest fsync");
    let close_operations = backend.operations()[operations_before_close..].to_vec();
    let manifest_after = backend
        .last_manifest_bytes()
        .expect("manifest still present after close");

    assert_eq!(close.status(), CloseOutcomeStatus::Complete);
    assert_eq!(close.stats().maintenance_tasks(), 0);
    let publish_count = close_operations
        .iter()
        .filter(|op| matches!(op, Operation::Publish(_, PublishMode::Replace)))
        .count();
    assert_eq!(
        publish_count, 1,
        "force-fsync should issue exactly one manifest republish"
    );
    assert_eq!(
        manifest_before, manifest_after,
        "V1 manifest payload must not change across a force-fsync — health is not a manifest field"
    );
}

#[test]
fn durable_close_manifest_publish_failure_returns_typed_source_chain() {
    let backend = DurableTestBackend::new();
    let branch = branch_id(0x34);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    let health = close_health_debt();
    runtime.record_recovery_health_for_test(&health);
    backend.set_publish_failure(Some(PublishFailureKind::FailedBeforeVisibility));

    let error = runtime.close().expect_err("final fact publish failure");

    assert_eq!(error.code(), "failed_precondition.lifecycle.service");
    assert!(error.source().is_some());
    assert_eq!(runtime.state(), LifecycleState::Closing);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
}

#[test]
fn durable_close_after_checkpoint_does_not_rewrite_checkpoint_without_dirty_fact() {
    let backend = DurableTestBackend::new();
    let branch = branch_id(0x35);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    runtime
        .execute_durable_commit(
            durable_put_batch(branch, b"checkpoint-before-close", b"value"),
            generation_guard(),
        )
        .expect("durable commit");
    let checkpoint = runtime
        .checkpoint(&checkpoint_request(branch, 1))
        .expect("checkpoint");
    assert_eq!(checkpoint.status(), LifecycleCheckpointStatus::Completed);
    let operations_before_close = backend.operations().len();

    runtime.close().expect("durable close");
    let close_operations = backend.operations()[operations_before_close..].to_vec();

    assert!(!close_operations
        .iter()
        .any(|operation| matches!(operation, Operation::Publish(_, _))));
}

#[test]
fn durable_close_after_flush_does_not_advance_flush_watermark_unless_checkpointed() {
    let backend = DurableTestBackend::new();
    let branch = branch_id(0x36);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    runtime
        .execute_durable_commit(
            durable_put_batch(branch, b"flush-before-close", b"value"),
            generation_guard(),
        )
        .expect("durable commit");
    runtime
        .rotate_active_for_maintenance()
        .expect("rotate active");
    let flush = runtime
        .flush_frozen(&flush_request(branch, "close-flush"))
        .expect("flush frozen");
    assert_eq!(flush.status(), FlushFrozenStatus::Completed);
    let operations_before_close = backend.operations().len();

    runtime.close().expect("durable close");
    let close_operations = backend.operations()[operations_before_close..].to_vec();

    assert!(!close_operations
        .iter()
        .any(|operation| matches!(operation, Operation::Publish(_, PublishMode::Replace))));
}

#[test]
fn durable_close_does_not_truncate_wal_unless_drain_task_did_so() {
    let backend = DurableTestBackend::new();
    let branch = branch_id(0x37);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    runtime
        .execute_durable_commit(
            durable_put_batch(branch, b"no-truncate-on-close", b"value"),
            generation_guard(),
        )
        .expect("durable commit");
    let operations_before_close = backend.operations().len();

    runtime.close().expect("durable close");
    let close_operations = backend.operations()[operations_before_close..].to_vec();

    assert!(!close_operations
        .iter()
        .any(|operation| matches!(operation, Operation::DeleteObject(_))));
}

#[test]
fn durable_close_does_not_prune_snapshots_or_purge_quarantine_implicitly() {
    durable_close_does_not_truncate_wal_prune_snapshots_or_purge_quarantine_implicitly();
}

#[test]
fn durable_close_with_pending_retention_drain_runs_required_task() {
    let backend = DurableTestBackend::new();
    let branch = branch_id(0x38);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    runtime
        .enqueue_maintenance(drain_task(
            MaintenanceTaskKind::Retention,
            MaintenanceTaskScope::Retention,
            MaintenanceTaskPriority::Low,
        ))
        .expect("enqueue retention");

    let close = runtime.close().expect("close drains retention");

    assert_eq!(close.status(), CloseOutcomeStatus::Complete);
    assert_eq!(close.stats().maintenance_tasks(), 1);
}

#[test]
fn durable_close_with_pending_quarantine_drain_preserves_reclaim_facts() {
    let backend = DurableTestBackend::new();
    let branch = branch_id(0x39);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    runtime
        .enqueue_maintenance(drain_task(
            MaintenanceTaskKind::Quarantine,
            MaintenanceTaskScope::Quarantine,
            MaintenanceTaskPriority::Low,
        ))
        .expect("enqueue quarantine");

    let close = runtime.close().expect("close drains quarantine");

    assert_eq!(close.status(), CloseOutcomeStatus::Complete);
    assert_eq!(close.stats().maintenance_tasks(), 1);
}

#[test]
fn durable_close_with_ordinary_compaction_task_does_not_start_compaction() {
    let backend = DurableTestBackend::new();
    let branch = branch_id(0x3a);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    runtime
        .enqueue_maintenance(MaintenanceTaskRequest::compaction(branch, 0))
        .expect("enqueue ordinary compaction");

    let close = runtime.close().expect("durable close");

    assert_eq!(close.status(), CloseOutcomeStatus::Complete);
    assert_eq!(close.stats().maintenance_tasks(), 1);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
}

#[test]
fn durable_close_after_failed_maintenance_reports_health_debt() {
    let backend = DurableTestBackend::new();
    let branch = branch_id(0x3b);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    runtime
        .execute_durable_commit(
            durable_put_batch(branch, b"failed-maintenance-close", b"value"),
            generation_guard(),
        )
        .expect("durable commit");
    runtime
        .enqueue_maintenance(drain_checkpoint_task())
        .expect("enqueue checkpoint");
    backend.set_publish_failure(Some(PublishFailureKind::FailedBeforeVisibility));

    let error = runtime.close().expect_err("checkpoint failure");

    assert_eq!(error.code(), "failed_precondition.lifecycle.service");
    assert_eq!(runtime.state(), LifecycleState::Closing);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);
}

#[test]
fn durable_open_commit_close_reopen_recovers_committed_rows() {
    let backend = DurableTestBackend::new();
    let branch = branch_id(0x3c);
    let key = physical_key(branch, b"reopen-row");
    let mut first = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    first
        .execute_durable_commit(
            durable_put_batch(branch, b"reopen-row", b"value"),
            generation_guard(),
        )
        .expect("durable commit");
    first.close().expect("close first");

    let second = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    let row = second
        .read_view()
        .expect("read view")
        .latest(&key)
        .expect("read")
        .expect("row");

    assert_eq!(row.row().value(), b"value");
    assert_eq!(second.visible_version(), CommitVersion::new(1));
}

#[test]
fn active_commit_guard_causes_typed_close_timeout() {
    durable_close_timeout_while_commit_guard_active_is_retryable();
}

#[test]
fn close_retry_after_timeout_completes_when_blocker_clears() {
    durable_close_timeout_while_commit_guard_active_is_retryable();
}

#[test]
fn close_retry_after_wal_failure_retries_sync_phase() {
    durable_close_log_sync_failure_preserves_writer_guard_for_retry();
}

#[test]
fn close_retry_after_manifest_failure_retries_final_fact_phase() {
    let backend = DurableTestBackend::new();
    let branch = branch_id(0x3d);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    let health = close_health_debt();
    runtime.record_recovery_health_for_test(&health);
    backend.set_publish_failure(Some(PublishFailureKind::FailedBeforeVisibility));
    assert!(runtime.close().is_err());
    backend.set_publish_failure(None);

    let close = runtime.close().expect("retry close");

    assert_eq!(close.status(), CloseOutcomeStatus::Complete);
    assert_eq!(runtime.state(), LifecycleState::Closed);
}

#[test]
fn close_retry_after_missing_writer_guard_failure_retries_release_phase() {
    durable_close_reports_typed_error_when_writer_guard_is_missing_at_release();
}

#[test]
fn close_failure_during_quiesce_preserves_drain_facts() {
    durable_close_preserves_drain_required_checkpoint_when_quiesce_is_unavailable();
}

#[test]
fn close_failure_during_wal_sync_preserves_quiesce_fact() {
    durable_close_log_sync_failure_preserves_writer_guard_for_retry();
}

#[test]
fn close_failure_during_manifest_sync_preserves_wal_fact() {
    close_retry_after_manifest_failure_retries_final_fact_phase();
}

#[test]
fn close_failure_during_guard_release_preserves_sync_fact() {
    durable_double_close_does_not_double_release_writer_guard();
}

#[test]
fn close_acquires_commit_quiesce_after_maintenance_drain() {
    // Enqueue a drain-required checkpoint task. The close path must
    // execute drain (which produces a snapshot Publish operation against
    // the backend) BEFORE issuing the WAL sync that close performs.
    // We assert temporal ordering on the recorded backend operation log:
    // every checkpoint-driven Publish must appear before the WAL SyncObject
    // that close itself issues at the end of the sequence. If a refactor
    // ever inverts these phases (quiesce/sync before drain), the
    // assertion below catches it.
    let backend = DurableTestBackend::new();
    let branch = branch_id(0x3e);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    runtime
        .execute_durable_commit(
            durable_put_batch(branch, b"ordering-precommit", b"value"),
            generation_guard(),
        )
        .expect("commit before close");
    runtime
        .enqueue_maintenance(drain_checkpoint_task())
        .expect("enqueue drain-required checkpoint");

    let operations_before_close = backend.operations().len();
    let close = runtime.close().expect("close");
    let close_operations = backend.operations()[operations_before_close..].to_vec();

    assert_eq!(close.stats().maintenance_tasks(), 1);
    assert!(close.commits_quiesced());
    assert_eq!(runtime.state(), LifecycleState::Closed);

    // The drained checkpoint task issued at least one snapshot Publish
    // before close itself ran the WAL sync. Pick the first Publish index
    // (drain output) and the first SyncObject index (WAL close) — drain
    // must strictly precede sync.
    let first_publish = close_operations
        .iter()
        .position(|operation| matches!(operation, Operation::Publish(_, _)))
        .expect("drained checkpoint produced no Publish");
    let first_sync = close_operations
        .iter()
        .position(|operation| matches!(operation, Operation::SyncObject(_)))
        .expect("close produced no SyncObject");
    assert!(
        first_publish < first_sync,
        "drain checkpoint Publish at index {first_publish} must precede close SyncObject at index {first_sync}; close ordering inverted",
    );
}

#[test]
fn quiesce_blocks_new_branch_guards_until_close_completes() {
    let guard_set = crate::commit::CommitBranchGuardSet::new();
    let quiesce = guard_set.try_begin_quiesce().expect("quiesce");
    let branch = branch_id(0x3f);

    assert!(guard_set.try_acquire_branch_guard(branch).is_err());
    drop(quiesce);
    assert!(guard_set.try_acquire_branch_guard(branch).is_ok());
}

#[test]
fn quiesce_guard_released_on_retryable_failure_when_contract_allows_retry() {
    let backend = DurableTestBackend::with_sync_failure();
    let branch = branch_id(0x40);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    runtime
        .execute_durable_commit(
            durable_put_batch(branch, b"quiesce-release", b"value"),
            generation_guard(),
        )
        .expect("durable commit");

    assert!(runtime.close().is_err());

    assert!(!runtime.guard_set().is_quiescing().expect("quiesce state"));
}

#[test]
fn quiesce_guard_not_reacquired_on_idempotent_second_close() {
    let backend = DurableTestBackend::new();
    let branch = branch_id(0x41);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    runtime.close().expect("first close");

    let second = runtime.close().expect("second close");

    assert_eq!(second.status(), CloseOutcomeStatus::Idempotent);
    assert!(!runtime.guard_set().is_quiescing().expect("quiesce state"));
}

#[test]
fn cross_branch_commit_after_quiesce_rejects() {
    let guard_set = crate::commit::CommitBranchGuardSet::new();
    let quiesce = guard_set.try_begin_quiesce().expect("quiesce");

    let error = guard_set
        .try_acquire_branch_guard(branch_id(0x42))
        .expect_err("branch guard rejected");

    assert!(matches!(
        error,
        crate::commit::CommitRuntimeError::CommitQuiesceUnavailable { .. }
    ));
    drop(quiesce);
}

#[test]
fn commit_after_close_requested_rejects_before_version_allocation() {
    let backend = DurableTestBackend::new();
    let branch = branch_id(0x28);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    let guard = runtime
        .guard_set()
        .try_acquire_branch_guard(branch)
        .expect("active commit guard");

    let error = runtime.close().expect_err("active guard blocks close");
    assert_eq!(error.code(), "deadline_exceeded.lifecycle.close");
    assert_eq!(runtime.state(), LifecycleState::Closing);
    let operations_after_close_failure = backend.operations().len();
    let allocation_before_commit = runtime.allocator().version_allocator().last_allocated();

    let commit_error = runtime
        .execute_durable_commit(
            durable_put_batch(branch, b"after-close-request", b"value"),
            generation_guard(),
        )
        .expect_err("commit after close request rejects");

    assert_eq!(commit_error.code(), "failed_precondition.lifecycle.state");
    assert_eq!(
        runtime.allocator().version_allocator().last_allocated(),
        allocation_before_commit
    );
    assert_eq!(backend.operations().len(), operations_after_close_failure);
    drop(guard);
    runtime.close().expect("retry close");
}

#[test]
fn durable_close_timeout_while_commit_guard_active_is_retryable() {
    let backend = DurableTestBackend::new();
    let branch = branch_id(0x21);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    let guard = runtime
        .guard_set()
        .try_acquire_branch_guard(branch)
        .expect("active commit guard");

    let error = runtime.close().expect_err("active guard blocks close");
    assert_eq!(error.code(), "deadline_exceeded.lifecycle.close");
    assert_eq!(runtime.state(), LifecycleState::Closing);
    assert_eq!(
        runtime
            .guard_set()
            .active_guard_count()
            .expect("guard count"),
        1
    );
    assert!(backend.lock_is_held());

    drop(guard);
    let close = runtime.close().expect("retry after active guard drops");
    assert_eq!(close.status(), CloseOutcomeStatus::Complete);
    assert_eq!(runtime.state(), LifecycleState::Closed);
    assert!(!backend.lock_is_held());
}

#[test]
fn durable_close_drains_stale_active_maintenance_before_closing() {
    let backend = DurableTestBackend::new();
    let branch = branch_id(0x24);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    let active = MaintenanceTask::new_for_test(
        77,
        MaintenanceTaskRequest::new(
            MaintenanceTaskKind::HealthCollection,
            MaintenanceTaskPriority::High,
            MaintenanceTaskScope::Global,
            MaintenanceTaskPolicy::drain_before_close(),
        )
        .expect("active close-drain task"),
    )
    .expect("active task");
    runtime.set_active_maintenance_for_test(active);

    let close = runtime.close().expect("close drains active task");

    assert_eq!(close.status(), CloseOutcomeStatus::Complete);
    assert_eq!(runtime.state(), LifecycleState::Closed);
    assert_eq!(runtime.maintenance_status().active_task(), None);
    assert_eq!(runtime.maintenance_status().stats().drained(), 1);
    assert_eq!(close.stats().maintenance_tasks(), 1);
    assert!(!backend.lock_is_held());
}

#[test]
fn durable_close_preserves_drain_required_checkpoint_when_quiesce_is_unavailable() {
    let backend = DurableTestBackend::new();
    let branch = branch_id(0x23);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    runtime
        .enqueue_maintenance(
            MaintenanceTaskRequest::new(
                MaintenanceTaskKind::Checkpoint,
                MaintenanceTaskPriority::High,
                MaintenanceTaskScope::Checkpoint,
                MaintenanceTaskPolicy::drain_before_close(),
            )
            .expect("drain-required checkpoint"),
        )
        .expect("enqueue checkpoint");
    let guard = runtime
        .guard_set()
        .try_acquire_branch_guard(branch)
        .expect("active commit guard");

    let error = runtime
        .close()
        .expect_err("checkpoint quiesce blocks close");

    assert_eq!(error.code(), "deadline_exceeded.lifecycle.close");
    assert_eq!(runtime.state(), LifecycleState::Closing);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 1);
    assert!(backend.lock_is_held());

    drop(guard);
    let close = runtime.close().expect("retry drains checkpoint and closes");
    assert_eq!(close.status(), CloseOutcomeStatus::Complete);
    assert_eq!(close.stats().maintenance_tasks(), 1);
    assert_eq!(runtime.maintenance_status().pending_tasks(), 0);
    assert_eq!(runtime.state(), LifecycleState::Closed);
    assert!(!backend.lock_is_held());
}

#[test]
fn durable_close_log_sync_failure_preserves_writer_guard_for_retry() {
    let backend = DurableTestBackend::with_sync_failure();
    let branch = branch_id(0x22);
    let mut runtime = open_runtime(StorageMode::DurableLocalStandard, branch, &backend);
    runtime
        .execute_durable_commit(
            durable_put_batch(branch, b"close-fail", b"value"),
            generation_guard(),
        )
        .expect("durable commit before close");

    let error = runtime.close().expect_err("sync failure blocks close");
    assert!(matches!(
        error,
        LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::Service,
            reason: "WAL service failed",
            ..
        }
    ));
    assert_eq!(runtime.state(), LifecycleState::Closing);
    assert!(backend.lock_is_held());
    assert!(runtime.services().writer_guard().is_some());

    backend.set_sync_failure(false);
    let close = runtime.close().expect("retry close after sync recovers");
    assert_eq!(close.status(), CloseOutcomeStatus::Complete);
    assert!(close.durable_synced());
    assert!(close.guards_released());
    assert!(!backend.lock_is_held());
}

fn assemble_shell(
    mode: StorageMode,
    branch: BranchId,
    backend: &DurableTestBackend,
) -> LifecycleResult<LifecycleDurableLocalShell<'_>> {
    LifecycleDurableLocalShell::assemble(request(mode, branch)?, backend, timestamp_source())
}

fn open_runtime(
    mode: StorageMode,
    branch: BranchId,
    backend: &DurableTestBackend,
) -> LifecycleDurableLocalRuntime<'_, CommitManualTimestampSource> {
    let mut shell = assemble_shell(mode, branch, backend).expect("durable shell");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    let recovery = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect("recovery outcome");
    shell.complete_recovery(&recovery).expect("open runtime")
}

fn checkpoint_request(branch: BranchId, snapshot_id: u64) -> LifecycleCheckpointRequest {
    LifecycleCheckpointRequest::new(
        branch,
        snapshot_id,
        Timestamp::from_micros(9_000 + snapshot_id),
    )
    .expect("checkpoint request")
}

fn flush_request(branch: BranchId, id: &'static str) -> FlushFrozenRequest {
    FlushFrozenRequest::new(
        branch,
        None,
        FlushTableIdentitySeed::new(id).expect("identity seed"),
        FlushTableObjectId::new(id).expect("object id"),
    )
    .expect("flush request")
}

fn drain_checkpoint_task() -> MaintenanceTaskRequest {
    drain_task(
        MaintenanceTaskKind::Checkpoint,
        MaintenanceTaskScope::Checkpoint,
        MaintenanceTaskPriority::High,
    )
}

fn drain_task(
    kind: MaintenanceTaskKind,
    scope: MaintenanceTaskScope,
    priority: MaintenanceTaskPriority,
) -> MaintenanceTaskRequest {
    MaintenanceTaskRequest::new(
        kind,
        priority,
        scope,
        MaintenanceTaskPolicy::drain_before_close(),
    )
    .expect("drain task")
}

fn generation_guard() -> CommitBranchGenerationGuard {
    CommitBranchGenerationGuard::exact(CommitBranchGeneration::new(1).expect("generation"))
}

fn durable_put_batch(
    branch: BranchId,
    user_key: &'static [u8],
    value: &'static [u8],
) -> CommitBatch {
    durable_put_batch_with_mode(branch, user_key, value, CommitDurabilityMode::Standard)
}

fn durable_put_batch_with_mode(
    branch: BranchId,
    user_key: &'static [u8],
    value: &'static [u8],
    durability: CommitDurabilityMode,
) -> CommitBatch {
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
            durability,
            CommitConflictValidationMode::Skip,
            CommitDuplicateKeyPolicy::Reject,
            CommitTimestampPolicy::RuntimeGenerated,
            CommitOrigin::StorageRuntime,
        ),
    )
}

fn physical_key(branch: BranchId, user_key: &'static [u8]) -> PhysicalKey {
    PhysicalKey::new(
        branch,
        "lifecycle",
        StorageSpaceId::engine(0x24).expect("space"),
        user_key.to_vec(),
    )
    .expect("physical key")
}

fn request(
    mode: StorageMode,
    branch: BranchId,
) -> LifecycleResult<LifecycleDurableLocalOpenRequest> {
    LifecycleDurableLocalOpenRequest::new(
        open_plan(mode),
        DATABASE_ID,
        branch,
        CommitBranchGeneration::new(1).expect("generation"),
        BranchRuntimeConfig::default(),
        CommitRuntimeConfig::default(),
        WalServiceConfig::default(),
    )
}

fn write_existing_manifest(backend: &DurableTestBackend, manifest: &DatabaseManifest) {
    backend.write_raw(
        ObjectLayout::database_manifest().expect("database object"),
        encode_manifest(manifest).expect("encoded object"),
    );
}

fn manifest_with_active_segment(segment_id: u64) -> DatabaseManifest {
    DatabaseManifest::new(DATABASE_ID, "identity")
        .expect("database object")
        .with_recovery_facts(segment_id, None, None, None)
        .expect("recovery facts")
}

fn close_health_debt() -> RecoveryHealth {
    RecoveryHealth::degraded(
        RecoveryDegradationClass::Telemetry,
        vec![
            RecoveryFault::new(RecoveryFaultKind::WalTailRepairFailed, "close health debt")
                .expect("fault"),
        ],
    )
    .expect("health")
}

fn open_plan(mode: StorageMode) -> StorageOpenPlan {
    StorageOpenPlan::new(
        mode,
        LifecycleCodecId::identity(),
        RecoveryStrictness::Strict,
        LifecycleConfig::default(),
    )
    .expect("open plan")
}

fn branch_id(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; 16])
}

fn timestamp_source() -> CommitManualTimestampSource {
    CommitManualTimestampSource::new(Timestamp::from_micros(8_000))
}

fn touch_shell_parts(shell: &LifecycleDurableLocalShell<'_>) {
    let services = shell.services();
    let _ = services.manifest();
    let _ = services.table_manifest();
    let _ = services.wal_sidecar();
    let _ = services.snapshot();
    let _ = services.table_object();
    let _ = services.table_reader();
    let _ = services.checkpoint();
    let _ = services.quarantine();
    let _ = services.writer_guard();
    let _ = shell.registry();
    let _ = shell.guard_set();
    let _ = shell.allocator();
    let _ = shell.durable_gate();
    let _ = shell.commit_config();
}

fn assert_call_order(operations: &[Operation], first: OperationKind, second: OperationKind) {
    let first_index = operations
        .iter()
        .position(|operation| operation.kind() == first)
        .expect("first operation");
    let second_index = operations
        .iter()
        .position(|operation| operation.kind() == second)
        .expect("second operation");
    assert!(
        first_index < second_index,
        "{first:?} should happen before {second:?}: {operations:?}"
    );
}

#[derive(Debug)]
struct DurableTestBackend {
    capabilities: BackendCapabilities,
    objects: Mutex<BTreeMap<ObjectName, Vec<u8>>>,
    operations: Mutex<Vec<Operation>>,
    lock_held: Arc<AtomicBool>,
    release_count: Arc<AtomicUsize>,
    fail_lock: bool,
    publish_failure: Mutex<Option<PublishFailureKind>>,
    create_race_manifest: Mutex<Option<DatabaseManifest>>,
    fail_metadata: bool,
    fail_sync: AtomicBool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum Operation {
    Capabilities,
    ReadObject(ObjectName),
    ReadRange(ObjectName),
    WriteObject(ObjectName),
    DeleteObject(ObjectName),
    ListPrefix(ObjectPrefix),
    ObjectMetadata(ObjectName),
    AcquireWriterLock(ObjectName),
    AppendObject(ObjectName),
    SyncObject(ObjectName),
    Publish(ObjectName, PublishMode),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum OperationKind {
    Capabilities,
    ReadObject,
    ReadRange,
    WriteObject,
    DeleteObject,
    ListPrefix,
    ObjectMetadata,
    AcquireWriterLock,
    AppendObject,
    SyncObject,
    Publish,
}

impl Operation {
    const fn kind(&self) -> OperationKind {
        match self {
            Self::Capabilities => OperationKind::Capabilities,
            Self::ReadObject(_) => OperationKind::ReadObject,
            Self::ReadRange(_) => OperationKind::ReadRange,
            Self::WriteObject(_) => OperationKind::WriteObject,
            Self::DeleteObject(_) => OperationKind::DeleteObject,
            Self::ListPrefix(_) => OperationKind::ListPrefix,
            Self::ObjectMetadata(_) => OperationKind::ObjectMetadata,
            Self::AcquireWriterLock(_) => OperationKind::AcquireWriterLock,
            Self::AppendObject(_) => OperationKind::AppendObject,
            Self::SyncObject(_) => OperationKind::SyncObject,
            Self::Publish(_, _) => OperationKind::Publish,
        }
    }
}

impl DurableTestBackend {
    fn new() -> Self {
        Self::with_capabilities(BackendCapabilities::from_slice(
            DURABLE_LOCAL_MODE_REQUIREMENTS,
        ))
    }

    fn with_capabilities(capabilities: BackendCapabilities) -> Self {
        Self {
            capabilities,
            objects: Mutex::new(BTreeMap::new()),
            operations: Mutex::new(Vec::new()),
            lock_held: Arc::new(AtomicBool::new(false)),
            release_count: Arc::new(AtomicUsize::new(0)),
            fail_lock: false,
            publish_failure: Mutex::new(None),
            create_race_manifest: Mutex::new(None),
            fail_metadata: false,
            fail_sync: AtomicBool::new(false),
        }
    }

    fn with_lock_failure() -> Self {
        Self {
            fail_lock: true,
            ..Self::new()
        }
    }

    fn with_publish_failure(kind: PublishFailureKind) -> Self {
        Self {
            publish_failure: Mutex::new(Some(kind)),
            ..Self::new()
        }
    }

    fn with_create_race(manifest: DatabaseManifest) -> Self {
        Self {
            create_race_manifest: Mutex::new(Some(manifest)),
            ..Self::new()
        }
    }

    fn with_metadata_failure() -> Self {
        Self {
            fail_metadata: true,
            ..Self::new()
        }
    }

    fn with_sync_failure() -> Self {
        let backend = Self::new();
        backend.set_sync_failure(true);
        backend
    }

    fn set_sync_failure(&self, fail: bool) {
        self.fail_sync.store(fail, Ordering::SeqCst);
    }

    fn set_publish_failure(&self, failure: Option<PublishFailureKind>) {
        *self.publish_failure.lock().expect("publish failure") = failure;
    }

    fn write_raw(&self, object: ObjectName, bytes: Vec<u8>) {
        self.objects.lock().expect("objects").insert(object, bytes);
    }

    fn operations(&self) -> Vec<Operation> {
        self.operations.lock().expect("operations").clone()
    }

    fn last_manifest_bytes(&self) -> Option<Vec<u8>> {
        let manifest_object = ObjectLayout::database_manifest().expect("database manifest layout");
        self.objects
            .lock()
            .expect("objects")
            .get(&manifest_object)
            .cloned()
    }

    fn operation_kinds(&self) -> Vec<OperationKind> {
        self.operations().iter().map(Operation::kind).collect()
    }

    fn lock_is_held(&self) -> bool {
        self.lock_held.load(Ordering::SeqCst)
    }

    fn release_count(&self) -> usize {
        self.release_count.load(Ordering::SeqCst)
    }

    fn record(&self, operation: Operation) {
        self.operations.lock().expect("operations").push(operation);
    }
}

impl Backend for DurableTestBackend {
    fn capabilities(&self) -> BackendCapabilities {
        self.record(Operation::Capabilities);
        self.capabilities
    }

    fn read_object(&self, name: &ObjectName) -> BackendResult<Vec<u8>> {
        self.record(Operation::ReadObject(name.clone()));
        self.objects
            .lock()
            .expect("objects")
            .get(name)
            .cloned()
            .ok_or_else(|| BackendError::new(BackendErrorKind::NotFound, "object not found"))
    }

    fn read_range(&self, name: &ObjectName, range: BackendRange) -> BackendResult<Vec<u8>> {
        self.record(Operation::ReadRange(name.clone()));
        let bytes = self.read_object(name)?;
        let start = usize::try_from(range.offset()).unwrap_or(usize::MAX);
        let end = usize::try_from(range.end_offset().unwrap_or(u64::MAX)).unwrap_or(usize::MAX);
        Ok(bytes[start.min(bytes.len())..end.min(bytes.len())].to_vec())
    }

    fn write_object(&self, name: &ObjectName, bytes: &[u8]) -> BackendResult<BackendMetadata> {
        self.record(Operation::WriteObject(name.clone()));
        self.objects
            .lock()
            .expect("objects")
            .insert(name.clone(), bytes.to_vec());
        Ok(BackendMetadata::new(bytes.len() as u64, None))
    }

    fn delete_object(&self, name: &ObjectName) -> BackendResult<()> {
        self.record(Operation::DeleteObject(name.clone()));
        self.objects.lock().expect("objects").remove(name);
        Ok(())
    }

    fn list_prefix(&self, prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
        self.record(Operation::ListPrefix(prefix.clone()));
        let mut names: Vec<_> = self
            .objects
            .lock()
            .expect("objects")
            .keys()
            .filter(|name| name.as_str().starts_with(prefix.as_str()))
            .cloned()
            .collect();
        names.sort();
        Ok(names)
    }

    fn object_metadata(&self, name: &ObjectName) -> BackendResult<BackendMetadata> {
        self.record(Operation::ObjectMetadata(name.clone()));
        if self.fail_metadata {
            return Err(BackendError::new(
                BackendErrorKind::Unavailable,
                "metadata unavailable",
            ));
        }
        self.objects
            .lock()
            .expect("objects")
            .get(name)
            .map(|bytes| BackendMetadata::new(bytes.len() as u64, None))
            .ok_or_else(|| BackendError::new(BackendErrorKind::NotFound, "object not found"))
    }

    fn acquire_writer_lock(&self, name: &ObjectName) -> BackendResult<BackendWriterGuard> {
        self.record(Operation::AcquireWriterLock(name.clone()));
        if self.fail_lock {
            return Err(BackendError::new(
                BackendErrorKind::Unavailable,
                "writer lock unavailable",
            ));
        }
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
                release_count: Arc::clone(&self.release_count),
            },
        ))
    }

    fn append_object(&self, name: &ObjectName, bytes: &[u8]) -> BackendResult<BackendAppend> {
        self.record(Operation::AppendObject(name.clone()));
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

    fn sync_object(&self, name: &ObjectName) -> BackendResult<()> {
        self.record(Operation::SyncObject(name.clone()));
        if self.fail_sync.load(Ordering::SeqCst) {
            return Err(BackendError::new(
                BackendErrorKind::Unavailable,
                "sync unavailable",
            ));
        }
        Ok(())
    }

    fn publish_object(
        &self,
        name: &ObjectName,
        bytes: &[u8],
        mode: PublishMode,
    ) -> PublishResult<PublishOutcome> {
        self.record(Operation::Publish(name.clone(), mode));
        if mode == PublishMode::Create
            && name == &ObjectLayout::database_manifest().expect("database object")
        {
            if let Some(manifest) = self.create_race_manifest.lock().expect("race").take() {
                self.objects.lock().expect("objects").insert(
                    name.clone(),
                    encode_manifest(&manifest).expect("encoded race object"),
                );
                return Err(PublishError::precondition_failed(
                    name,
                    "object already exists",
                ));
            }
        }
        if let Some(kind) = *self.publish_failure.lock().expect("publish failure") {
            return Err(PublishError::new(
                name.clone(),
                kind,
                BackendError::new(BackendErrorKind::Unavailable, "injected publish failure"),
            ));
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

struct HeldWriterLock {
    locked: Arc<AtomicBool>,
    release_count: Arc<AtomicUsize>,
}

impl Drop for HeldWriterLock {
    fn drop(&mut self) {
        self.release_count.fetch_add(1, Ordering::SeqCst);
        self.locked.store(false, Ordering::SeqCst);
    }
}
