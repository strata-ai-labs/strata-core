use super::*;
use crate::backend::{
    Backend, BackendAppend, BackendCapabilities, BackendError, BackendErrorKind, BackendMetadata,
    BackendRange, BackendResult, BackendWriterGuard, PublishDurability, PublishError, PublishMode,
    PublishOutcome, PublishResult, DURABLE_LOCAL_MODE_REQUIREMENTS,
};
use crate::branch::BranchRuntimeConfig;
use crate::commit::{CommitBranchGeneration, CommitManualTimestampSource, CommitRuntimeConfig};
use crate::format::{encode_manifest, DatabaseManifest, WalCommitPayload, WalRecord};
use crate::layout::ObjectLayout;
use crate::object::{ObjectName, ObjectPrefix};
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use crate::service::{
    SnapshotPublishRequest, SnapshotService, TableObjectService, WalServiceConfig,
};
use crate::table::{ImmutableTableBuilder, TableBuilderConfig, TableIdentity, TableRow};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

const DATABASE_ID: [u8; 16] = [0x9a; 16];

#[test]
fn recovery_empty_database_returns_healthy_package_without_replay() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x31);
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");

    let outcome = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect("recovery outcome");

    assert_eq!(outcome.health(), &RecoveryHealth::Healthy);
    assert_eq!(outcome.checkpoint().snapshot_id(), None);
    assert_eq!(outcome.checkpoint().trusted_watermark(), None);
    assert_eq!(outcome.checkpoint().section_count(), 0);
    assert_eq!(outcome.checkpoint().row_count(), 0);
    assert!(outcome.checkpoint().install_outcome().is_none());
    assert_eq!(outcome.wal().replay_start(), CommitVersion::ZERO);
    assert!(outcome.wal().records().is_empty());
    assert!(outcome.wal().truncation().is_none());
    assert!(outcome.wal().repair().is_none());
    assert!(outcome.quarantine().object().is_some());
    assert!(!outcome.quarantine().is_present());
    assert_eq!(outcome.quarantine().byte_count(), 0);
    assert_eq!(outcome.quarantine().entry_count(), 0);
    assert_eq!(outcome.tables().validated_count(), 0);
    assert_eq!(shell.state(), LifecycleState::Recovering);
    assert_eq!(shell.visible_version(), CommitVersion::ZERO);
    assert!(shell.branch_state().is_empty());
    assert!(shell.admit_ordinary_read().is_err());
    assert!(shell.admit_commit().is_err());
}

#[test]
fn recovery_loads_checkpoint_installs_rows_and_packages_only_wal_tail() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x32);
    let checkpoint_row = put_row(branch, 3, b"checkpoint", b"checkpoint-value");
    publish_snapshot(
        &backend,
        5,
        CommitVersion::new(3),
        std::slice::from_ref(&checkpoint_row),
    );
    write_manifest(
        &backend,
        &DatabaseManifest::new(DATABASE_ID, "identity")
            .expect("database root")
            .with_recovery_facts(1, Some(3), Some(5), Some(CommitVersion::new(2)))
            .expect("database root facts"),
    );
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let skipped = wal_record(branch, 2, b"skipped", b"old");
    let replayed = wal_record(branch, 4, b"tail", b"tail-value");
    shell
        .services_mut()
        .wal_mut()
        .append(&skipped)
        .expect("append skipped record");
    shell
        .services_mut()
        .wal_mut()
        .append(&replayed)
        .expect("append replayed record");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");

    let outcome = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect("recovery outcome");

    assert_eq!(outcome.health(), &RecoveryHealth::Healthy);
    assert_eq!(outcome.checkpoint().snapshot_id(), Some(5));
    assert_eq!(
        outcome.checkpoint().trusted_watermark(),
        Some(CommitVersion::new(3))
    );
    assert_eq!(outcome.checkpoint().section_count(), 1);
    assert_eq!(outcome.checkpoint().row_count(), 1);
    assert!(outcome.checkpoint().install_outcome().is_some());
    assert_eq!(outcome.wal().replay_start(), CommitVersion::new(3));
    assert_eq!(outcome.wal().records(), std::slice::from_ref(&replayed));
    assert!(outcome.wal().truncation().is_none());
    assert!(outcome.wal().repair().is_none());

    let view = shell.branch_state().capture_read_view().expect("read view");
    let visible = view
        .latest(checkpoint_row.physical_key())
        .expect("latest")
        .expect("visible checkpoint row");
    assert_eq!(visible.row(), &checkpoint_row);
    assert_eq!(shell.visible_version(), CommitVersion::ZERO);
}

#[test]
fn recovery_does_not_install_checkpoint_when_later_wal_read_fails() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x37);
    let checkpoint_row = put_row(branch, 3, b"checkpoint", b"checkpoint-value");
    publish_snapshot(
        &backend,
        5,
        CommitVersion::new(3),
        std::slice::from_ref(&checkpoint_row),
    );
    write_manifest(
        &backend,
        &DatabaseManifest::new(DATABASE_ID, "identity")
            .expect("database root")
            .with_recovery_facts(1, Some(3), Some(5), None)
            .expect("database root facts"),
    );
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    backend.fail_read_object(ObjectLayout::wal_segment(1).expect("active log object"));
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");

    let error = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect_err("log read failure rejects");

    assert!(matches!(
        error,
        LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::Service,
            reason: "WAL recovery read failed",
            ..
        }
    ));
    assert!(error.source().is_some());
    assert!(shell.branch_state().is_empty());
}

#[test]
fn recovery_repairs_latest_partial_log_tail_and_preserves_valid_records() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x40);
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let record = wal_record(branch, 2, b"valid", b"value");
    shell
        .services_mut()
        .wal_mut()
        .append(&record)
        .expect("append valid record");
    backend.append_raw(
        ObjectLayout::wal_segment(1).expect("active log object"),
        b"partial",
    );
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");

    let outcome = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect("partial tail recovery");

    assert_eq!(outcome.wal().records(), std::slice::from_ref(&record));
    assert!(outcome.wal().truncation().is_some());
    assert!(outcome.wal().repair().is_some());
    assert_eq!(outcome.health(), &RecoveryHealth::Healthy);
}

#[test]
fn recovery_rejects_checkpoint_row_newer_than_snapshot_watermark() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x38);
    let checkpoint_row = put_row(branch, 4, b"too-new", b"value");
    publish_snapshot(
        &backend,
        6,
        CommitVersion::new(3),
        std::slice::from_ref(&checkpoint_row),
    );
    write_manifest(
        &backend,
        &DatabaseManifest::new(DATABASE_ID, "identity")
            .expect("database root")
            .with_recovery_facts(1, Some(3), Some(6), None)
            .expect("database root facts"),
    );
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");

    assert_eq!(
        LifecycleRecoveryRuntime::new(&mut shell).recover(&request),
        Err(LifecycleError::RecoveryFailed {
            reason: "checkpoint row commit version exceeds snapshot watermark",
        })
    );
    assert!(shell.branch_state().is_empty());
}

#[test]
fn recovery_rejects_checkpoint_rows_for_unopened_branch() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x39);
    let other_branch_row = put_row(branch_id(0x3a), 3, b"other", b"value");
    publish_snapshot(
        &backend,
        7,
        CommitVersion::new(3),
        std::slice::from_ref(&other_branch_row),
    );
    write_manifest(
        &backend,
        &DatabaseManifest::new(DATABASE_ID, "identity")
            .expect("database root")
            .with_recovery_facts(1, Some(3), Some(7), None)
            .expect("database root facts"),
    );
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");

    assert_eq!(
        LifecycleRecoveryRuntime::new(&mut shell).recover(&request),
        Err(LifecycleError::RecoveryFailed {
            reason: "checkpoint contains rows for unopened branch",
        })
    );
    assert!(shell.branch_state().is_empty());
}

#[test]
fn recovery_rejects_flush_watermark_without_recovered_table_state() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x3b);
    write_manifest(
        &backend,
        &DatabaseManifest::new(DATABASE_ID, "identity")
            .expect("database root")
            .with_recovery_facts(1, None, None, Some(CommitVersion::new(4)))
            .expect("database root facts"),
    );
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");

    assert_eq!(
        LifecycleRecoveryRuntime::new(&mut shell).recover(&request),
        Err(LifecycleError::RecoveryFailed {
            reason: "manifest flush watermark requires recovered flushed table state",
        })
    );
    assert!(shell.branch_state().is_empty());
}

#[test]
fn recovery_records_validated_table_identity_and_facts() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x42);
    let table_identity = TableIdentity::new("recovery-table-valid").expect("table identity");
    let table_bytes = table_bytes(
        table_identity.clone(),
        std::slice::from_ref(&put_row(branch, 2, b"table-row", b"value")),
    );
    let write = TableObjectService::new(&backend)
        .publish_create("branch", 0, "table0002", &table_bytes)
        .expect("publish table");
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let request = LifecycleRecoveryRequest::from_open_plan(shell.open_plan())
        .expect("recovery request")
        .with_table_objects(vec![LifecycleRecoveryTableObject::new(
            table_identity.clone(),
            write.facts().clone(),
        )]);

    let outcome = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect("table validation recovery");

    assert_eq!(outcome.tables().validated_count(), 1);
    assert_eq!(outcome.tables().validated()[0].identity(), &table_identity);
    assert_eq!(outcome.tables().validated()[0].facts(), write.facts());
}

#[test]
fn recovery_rejects_missing_referenced_table_object() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x3d);
    let table_identity = TableIdentity::new("recovery-table").expect("table identity");
    let table_bytes = table_bytes(
        table_identity.clone(),
        std::slice::from_ref(&put_row(branch, 2, b"table-row", b"value")),
    );
    let write = TableObjectService::new(&backend)
        .publish_create("branch", 0, "table0001", &table_bytes)
        .expect("publish table");
    backend
        .delete_object(write.facts().object())
        .expect("remove referenced table");
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let request = LifecycleRecoveryRequest::from_open_plan(shell.open_plan())
        .expect("recovery request")
        .with_table_objects(vec![LifecycleRecoveryTableObject::new(
            table_identity,
            write.facts().clone(),
        )]);

    let error = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect_err("missing table rejects");

    assert!(matches!(
        error,
        LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::TableRuntime,
            reason: "table object recovery validation failed",
            ..
        }
    ));
    assert!(error.source().is_some());
    assert!(shell.branch_state().is_empty());
}

#[test]
fn recovery_validates_tables_before_wal_tail_repair() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x43);
    let table_identity = TableIdentity::new("recovery-table-before-wal").expect("table identity");
    let table_bytes = table_bytes(
        table_identity.clone(),
        std::slice::from_ref(&put_row(branch, 2, b"table-row", b"value")),
    );
    let write = TableObjectService::new(&backend)
        .publish_create("branch", 0, "table0003", &table_bytes)
        .expect("publish table");
    backend
        .delete_object(write.facts().object())
        .expect("remove referenced table");
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let record = wal_record(branch, 3, b"valid", b"value");
    shell
        .services_mut()
        .wal_mut()
        .append(&record)
        .expect("append valid record");
    let wal_object = ObjectLayout::wal_segment(1).expect("active log object");
    backend.append_raw(wal_object.clone(), b"partial");
    let before = backend.object_bytes(&wal_object).expect("wal bytes before");
    let request = LifecycleRecoveryRequest::from_open_plan(shell.open_plan())
        .expect("recovery request")
        .with_table_objects(vec![LifecycleRecoveryTableObject::new(
            table_identity,
            write.facts().clone(),
        )]);

    let error = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect_err("missing table rejects before WAL repair");

    assert!(matches!(
        error,
        LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::TableRuntime,
            reason: "table object recovery validation failed",
            ..
        }
    ));
    assert_eq!(
        backend.object_bytes(&wal_object).expect("wal bytes after"),
        before,
        "WAL tail must not be repaired after table validation already failed",
    );
}

#[test]
fn recovery_degrades_quarantine_inventory_mismatch_only_when_explicitly_lossy() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x3e);
    backend.write_raw(
        ObjectLayout::quarantine_manifest(&branch.to_string()).expect("inventory object"),
        b"not inventory".to_vec(),
    );
    let mut strict_shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("strict shell");
    let strict_request =
        LifecycleRecoveryRequest::from_open_plan(strict_shell.open_plan()).expect("strict request");
    let strict_error = LifecycleRecoveryRuntime::new(&mut strict_shell)
        .recover(&strict_request)
        .expect_err("strict inventory mismatch rejects");
    assert!(matches!(
        strict_error,
        LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::Service,
            reason: "quarantine inventory recovery failed",
            ..
        }
    ));
    drop(strict_shell);

    let mut lossy_shell = assemble_shell(lossy_open_plan(), branch, &backend).expect("lossy shell");
    let lossy_request =
        LifecycleRecoveryRequest::from_open_plan(lossy_shell.open_plan()).expect("lossy request");
    let outcome = LifecycleRecoveryRuntime::new(&mut lossy_shell)
        .recover(&lossy_request)
        .expect("lossy inventory mismatch degrades");

    assert!(matches!(
        outcome.health(),
        RecoveryHealth::Degraded {
            class: RecoveryDegradationClass::PolicyDowngrade,
            faults
        } if faults.iter().any(
            |fault| fault.kind() == RecoveryFaultKind::QuarantineInventoryMismatch
        )
    ));
    assert!(outcome.quarantine().object().is_none());
    assert!(!outcome.quarantine().is_present());
    assert_eq!(outcome.quarantine().byte_count(), 0);
    assert_eq!(outcome.quarantine().entry_count(), 0);
}

#[test]
fn recovery_rejects_missing_snapshot_in_strict_mode() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x33);
    write_manifest(
        &backend,
        &DatabaseManifest::new(DATABASE_ID, "identity")
            .expect("database root")
            .with_recovery_facts(1, Some(9), Some(6), None)
            .expect("database root facts"),
    );
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");

    let error = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect_err("strict missing snapshot rejects");

    assert!(matches!(
        error,
        LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::Service,
            reason: "manifest-listed snapshot is missing",
            ..
        }
    ));
    assert!(error.source().is_some());
    assert!(shell.branch_state().is_empty());
}

#[test]
fn recovery_allows_explicit_lossy_missing_snapshot_without_trusting_watermark() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x34);
    write_manifest(
        &backend,
        &DatabaseManifest::new(DATABASE_ID, "identity")
            .expect("database root")
            .with_recovery_facts(1, Some(9), Some(7), None)
            .expect("database root facts"),
    );
    let mut shell = assemble_shell(lossy_open_plan(), branch, &backend).expect("durable shell");
    let replayed = wal_record(branch, 5, b"lossy-tail", b"tail-value");
    shell
        .services_mut()
        .wal_mut()
        .append(&replayed)
        .expect("append replayed record");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    assert_eq!(
        request.strictness(),
        RecoveryStrictness::AllowExplicitLossyFallback
    );
    assert!(request.max_faults() > 0);
    assert!(request.max_snapshot_sections() > 0);

    let outcome = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect("lossy recovery outcome");

    assert!(matches!(
        outcome.health(),
        RecoveryHealth::Degraded {
            class: RecoveryDegradationClass::PolicyDowngrade,
            faults
        } if faults.iter().any(|fault| fault.kind() == RecoveryFaultKind::MissingSnapshotObject)
    ));
    assert_eq!(outcome.checkpoint().snapshot_id(), Some(7));
    assert_eq!(outcome.checkpoint().trusted_watermark(), None);
    assert_eq!(outcome.wal().replay_start(), CommitVersion::ZERO);
    assert_eq!(outcome.wal().records(), &[replayed]);
}

#[test]
fn recovery_request_rejects_lossy_when_open_plan_is_strict() {
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x35);
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let request = LifecycleRecoveryRequest::new(
        RecoveryStrictness::AllowExplicitLossyFallback,
        1,
        1,
        "lossy-request",
    )
    .expect("request");

    assert_eq!(
        LifecycleRecoveryRuntime::new(&mut shell).recover(&request),
        Err(LifecycleError::InvalidOpenPlan {
            reason: "lossy recovery request requires lossy open plan",
        })
    );
}

#[test]
fn recovery_request_validates_limits_and_checkpoint_identity() {
    assert_eq!(
        LifecycleRecoveryRequest::new(RecoveryStrictness::Strict, 0, 1, "valid"),
        Err(LifecycleError::InvalidConfig {
            field: "max_faults",
            reason: "must be nonzero",
        })
    );
    assert_eq!(
        LifecycleRecoveryRequest::new(RecoveryStrictness::Strict, 1, 0, "valid"),
        Err(LifecycleError::InvalidConfig {
            field: "max_snapshot_sections",
            reason: "must be nonzero",
        })
    );
    assert!(matches!(
        LifecycleRecoveryRequest::new(RecoveryStrictness::Strict, 1, 1, ""),
        Err(LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::TableRuntime,
            reason: "invalid recovery table identity",
            ..
        })
    ));
}

#[test]
fn checkpoint_row_section_round_trips_and_rejects_trailing_bytes() {
    let row = put_row(branch_id(0x36), 11, b"section", b"value");
    let section =
        encode_checkpoint_row_section(std::slice::from_ref(&row)).expect("row checkpoint section");
    assert_eq!(section.section_kind(), SNAPSHOT_ROW_SECTION_KIND);
    let container = crate::format::SnapshotContainer::new(
        crate::format::SnapshotHeader::new(
            8,
            CommitVersion::new(11),
            Timestamp::from_micros(1_100),
            DATABASE_ID,
            "identity",
        )
        .expect("header"),
        vec![section.clone()],
    );
    assert_eq!(container.sections()[0], section);

    let mut trailing = section.payload().to_vec();
    trailing.push(0);
    let invalid = crate::format::SnapshotSection::new(SNAPSHOT_ROW_SECTION_KIND, trailing)
        .expect("invalid row section shape");
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x36);
    let snapshot = crate::format::SnapshotContainer::new(
        crate::format::SnapshotHeader::new(
            8,
            CommitVersion::new(11),
            Timestamp::from_micros(1_100),
            DATABASE_ID,
            "identity",
        )
        .expect("header"),
        vec![invalid],
    );
    let snapshot_bytes = crate::format::encode_snapshot_container(&snapshot).expect("snapshot");
    backend.write_raw(ObjectLayout::snapshot(8).expect("snapshot"), snapshot_bytes);
    write_manifest(
        &backend,
        &DatabaseManifest::new(DATABASE_ID, "identity")
            .expect("database root")
            .with_recovery_facts(1, Some(11), Some(8), None)
            .expect("database root facts"),
    );
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");
    let error = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect_err("trailing row section rejects");
    assert!(matches!(
        error,
        LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::Format,
            reason: "snapshot section decode failed",
            ..
        }
    ));
    assert!(error.source().is_some());
}

#[test]
fn checkpoint_row_section_rejects_declared_rows_without_length_prefixes() {
    let mut payload = Vec::new();
    payload.extend_from_slice(b"STRR");
    payload.extend_from_slice(&1_u32.to_le_bytes());
    payload.extend_from_slice(&u32::MAX.to_le_bytes());
    let invalid = crate::format::SnapshotSection::new(SNAPSHOT_ROW_SECTION_KIND, payload)
        .expect("invalid row section shape");
    let backend = RecoveryTestBackend::new();
    let branch = branch_id(0x3c);
    let snapshot = crate::format::SnapshotContainer::new(
        crate::format::SnapshotHeader::new(
            9,
            CommitVersion::new(11),
            Timestamp::from_micros(1_100),
            DATABASE_ID,
            "identity",
        )
        .expect("header"),
        vec![invalid],
    );
    let snapshot_bytes = crate::format::encode_snapshot_container(&snapshot).expect("snapshot");
    backend.write_raw(
        ObjectLayout::snapshot(9).expect("snapshot object"),
        snapshot_bytes,
    );
    write_manifest(
        &backend,
        &DatabaseManifest::new(DATABASE_ID, "identity")
            .expect("database root")
            .with_recovery_facts(1, Some(11), Some(9), None)
            .expect("database root facts"),
    );
    let mut shell = assemble_shell(open_plan(RecoveryStrictness::Strict), branch, &backend)
        .expect("durable shell");
    let request =
        LifecycleRecoveryRequest::from_open_plan(shell.open_plan()).expect("recovery request");

    let error = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&request)
        .expect_err("impossible row count rejects");

    assert!(matches!(
        error,
        LifecycleError::LowerLayer {
            layer: LifecycleLowerLayer::Format,
            reason: "snapshot section decode failed",
            ..
        }
    ));
    assert!(error.source().is_some());
    assert!(shell.branch_state().is_empty());
}

fn assemble_shell(
    plan: StorageOpenPlan,
    branch: BranchId,
    backend: &RecoveryTestBackend,
) -> LifecycleResult<LifecycleDurableLocalShell<'_>> {
    LifecycleDurableLocalShell::assemble(
        LifecycleDurableLocalOpenRequest::new(
            plan,
            DATABASE_ID,
            branch,
            CommitBranchGeneration::new(1).expect("generation"),
            BranchRuntimeConfig::default(),
            CommitRuntimeConfig::default(),
            WalServiceConfig::default(),
        )?,
        backend,
        timestamp_source(),
    )
}

fn open_plan(recovery_policy: RecoveryStrictness) -> StorageOpenPlan {
    StorageOpenPlan::new(
        StorageMode::DurableLocalStandard,
        LifecycleCodecId::identity(),
        recovery_policy,
        LifecycleConfig::default(),
    )
    .expect("open plan")
}

fn lossy_open_plan() -> StorageOpenPlan {
    let config = LifecycleConfig::new(
        1024,
        16,
        LifecycleCloseTimeoutPolicy::ReturnTypedTimeout,
        LifecycleLossyRecoveryPolicy::ExplicitlyAllowed,
    )
    .expect("lossy lifecycle config");
    StorageOpenPlan::new(
        StorageMode::DurableLocalStandard,
        LifecycleCodecId::identity(),
        RecoveryStrictness::AllowExplicitLossyFallback,
        config,
    )
    .expect("lossy open plan")
}

fn publish_snapshot(
    backend: &RecoveryTestBackend,
    snapshot_id: u64,
    watermark: CommitVersion,
    rows: &[StorageRow],
) {
    SnapshotService::new(backend)
        .publish_create(SnapshotPublishRequest::new(
            snapshot_id,
            watermark,
            Timestamp::from_micros(7_000),
            DATABASE_ID,
            "identity",
            vec![encode_checkpoint_row_section(rows).expect("row section")],
        ))
        .expect("publish snapshot");
}

fn write_manifest(backend: &RecoveryTestBackend, manifest: &DatabaseManifest) {
    backend.write_raw(
        ObjectLayout::database_manifest().expect("database root object"),
        encode_manifest(manifest).expect("database root bytes"),
    );
}

fn wal_record(
    branch: BranchId,
    version: u64,
    user_key: &'static [u8],
    value: &'static [u8],
) -> WalRecord {
    let commit_version = CommitVersion::new(version);
    let timestamp = Timestamp::from_micros(version * 100);
    let row = StorageRow::put(
        physical_key(branch, user_key),
        commit_version,
        timestamp,
        Timestamp::EPOCH,
        value.to_vec(),
    );
    let payload = WalCommitPayload::new(vec![row]).expect("payload");
    WalRecord::new(commit_version, branch, timestamp, payload).expect("record")
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

fn table_bytes(identity: TableIdentity, rows: &[StorageRow]) -> Vec<u8> {
    let rows = rows.iter().cloned().map(TableRow::new).collect::<Vec<_>>();
    ImmutableTableBuilder::new(TableBuilderConfig::default())
        .expect("table builder")
        .build_from_rows(identity, &rows)
        .expect("build table")
        .into_bytes()
}

fn physical_key(branch: BranchId, user_key: &'static [u8]) -> PhysicalKey {
    PhysicalKey::new(
        branch,
        "lifecycle",
        StorageSpaceId::engine(0x20).expect("space"),
        user_key.to_vec(),
    )
    .expect("physical key")
}

fn branch_id(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; 16])
}

fn timestamp_source() -> CommitManualTimestampSource {
    CommitManualTimestampSource::new(Timestamp::from_micros(8_000))
}

#[derive(Debug)]
struct RecoveryTestBackend {
    capabilities: BackendCapabilities,
    objects: Mutex<BTreeMap<ObjectName, Vec<u8>>>,
    fail_reads: Mutex<BTreeSet<ObjectName>>,
    lock_held: Arc<AtomicBool>,
}

impl RecoveryTestBackend {
    fn new() -> Self {
        Self {
            capabilities: BackendCapabilities::from_slice(DURABLE_LOCAL_MODE_REQUIREMENTS),
            objects: Mutex::new(BTreeMap::new()),
            fail_reads: Mutex::new(BTreeSet::new()),
            lock_held: Arc::new(AtomicBool::new(false)),
        }
    }

    fn write_raw(&self, object: ObjectName, bytes: Vec<u8>) {
        self.objects.lock().expect("objects").insert(object, bytes);
    }

    fn append_raw(&self, object: ObjectName, bytes: &[u8]) {
        self.objects
            .lock()
            .expect("objects")
            .entry(object)
            .or_default()
            .extend_from_slice(bytes);
    }

    fn object_bytes(&self, object: &ObjectName) -> Option<Vec<u8>> {
        self.objects.lock().expect("objects").get(object).cloned()
    }

    fn fail_read_object(&self, object: ObjectName) {
        self.fail_reads
            .lock()
            .expect("read failures")
            .insert(object);
    }
}

impl Backend for RecoveryTestBackend {
    fn capabilities(&self) -> BackendCapabilities {
        self.capabilities
    }

    fn read_object(&self, name: &ObjectName) -> BackendResult<Vec<u8>> {
        if self
            .fail_reads
            .lock()
            .expect("read failures")
            .contains(name)
        {
            return Err(BackendError::new(
                BackendErrorKind::Unavailable,
                "injected read failure",
            ));
        }
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
}

impl Drop for HeldWriterLock {
    fn drop(&mut self) {
        self.locked.store(false, Ordering::SeqCst);
    }
}
