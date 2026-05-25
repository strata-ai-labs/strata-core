mod support;

use self::support::*;
use super::{
    CheckpointManifestOperation, CheckpointService, CheckpointServiceError,
    DatabaseManifestService, ManifestRole, ObjectPublisher, QuarantineGate, QuarantinePurgeRequest,
    QuarantineReconciliationKind, QuarantineService, QuarantineServiceError, SnapshotService,
    SnapshotServiceError, TableManifestService, TableObjectService, WalSegmentMetadataSidecarError,
    WalSegmentMetadataSidecarService, WalService, WalServiceConfig, WalServiceError,
};
use crate::backend::{
    Backend, BackendCapabilities, BackendCapability, PublishDurability, PublishMode,
    CACHE_MODE_REQUIREMENTS,
};
use crate::config::mode::{DurabilityPolicy, StorageModeRequest};
use crate::format::{DatabaseManifest, SegmentMetadata};
use crate::layout::ObjectLayout;
use strata_core_next::Timestamp;

#[test]
fn cache_mode_capability_requirements_exclude_durable_and_metadata_primitives() {
    let browser_like_capabilities = BackendCapabilities::from_slice(&[
        BackendCapability::ReadObject,
        BackendCapability::ReadRange,
        BackendCapability::WriteObject,
        BackendCapability::DeleteObject,
        BackendCapability::ListPrefix,
    ]);

    StorageModeRequest::cache()
        .validate_backend(browser_like_capabilities)
        .expect("cache mode should accept browser-like non-durable storage");

    for capability in [
        BackendCapability::ObjectMetadata,
        BackendCapability::AppendObject,
        BackendCapability::DurablePublish,
        BackendCapability::DurableSync,
        BackendCapability::SingleWriterLock,
    ] {
        assert!(
            !CACHE_MODE_REQUIREMENTS.contains(&capability),
            "cache mode unexpectedly requires {capability}"
        );
    }
}

#[test]
fn cache_mode_non_durable_publish_writes_only_the_requested_cache_object() {
    let backend = ObservingCacheBackend::new();
    let before = durable_snapshot(&backend);
    backend.clear_operations();
    let object = cache_object();

    let outcome = ObjectPublisher::new(&backend)
        .publish_non_durable_replace(&object, b"cache bytes")
        .expect("non-durable cache publish");

    assert_eq!(outcome.object(), &object);
    assert_eq!(outcome.durability(), PublishDurability::NonDurable);
    assert_eq!(backend.read_seeded(&object), b"cache bytes");
    assert_eq!(
        backend.operations(),
        vec![Operation::Publish(object, PublishMode::NonDurableReplace)]
    );
    assert_durable_snapshot_unchanged(&backend, &before);
}

#[test]
fn cache_backend_rejects_durable_manifest_and_table_publication_before_mutation() {
    let backend = ObservingCacheBackend::new();
    let before = durable_snapshot(&backend);
    backend.clear_operations();
    let manifest_object = ObjectLayout::database_manifest().expect("database manifest object");
    let table_manifest_object =
        ObjectLayout::branch_table_manifest(&table_branch()).expect("table manifest object");
    let table_object =
        ObjectLayout::table_object(&table_branch(), 0, "table0001").expect("table object");

    let database_error = DatabaseManifestService::new(&backend)
        .create_initial(DATABASE_ID, CODEC_ID)
        .expect_err("cache backend cannot publish database manifest durably");
    assert_manifest_publish_unsupported(database_error, ManifestRole::Database, &manifest_object);

    let replacement_manifest =
        DatabaseManifest::new(DATABASE_ID, CODEC_ID).expect("database metadata");
    let database_replace_error = DatabaseManifestService::new(&backend)
        .publish_current(&replacement_manifest)
        .expect_err("cache backend cannot replace database manifest durably");
    assert_manifest_publish_unsupported(
        database_replace_error,
        ManifestRole::Database,
        &manifest_object,
    );

    let table_create_error = TableManifestService::new(&backend)
        .publish_create(&table_branch(), b"table manifest")
        .expect_err("cache backend cannot create table manifest durably");
    assert_manifest_publish_unsupported(
        table_create_error,
        ManifestRole::Table,
        &table_manifest_object,
    );

    let table_replace_error = TableManifestService::new(&backend)
        .publish_replace(&table_branch(), b"table manifest")
        .expect_err("cache backend cannot replace table manifest durably");
    assert_manifest_publish_unsupported(
        table_replace_error,
        ManifestRole::Table,
        &table_manifest_object,
    );

    let table_object_error = TableObjectService::new(&backend)
        .publish_create(&table_branch(), 0, "table0001", &valid_table_object_bytes())
        .expect_err("cache backend cannot publish table object durably");
    assert_table_object_publish_unsupported(table_object_error, &table_object);

    assert!(backend.operations().is_empty());
    assert_durable_snapshot_unchanged(&backend, &before);
}

#[test]
fn cache_backend_rejects_wal_and_sidecar_durable_paths_before_mutation() {
    let backend = ObservingCacheBackend::new();
    let before = durable_snapshot(&backend);
    backend.clear_operations();

    let Err(wal_error) = WalService::open(
        &backend,
        DATABASE_ID,
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::new(1024),
    ) else {
        panic!("cache backend cannot open durable WAL");
    };
    assert_eq!(
        wal_error,
        WalServiceError::UnsupportedCapability {
            capability: BackendCapability::AppendObject,
        }
    );
    assert!(backend.operations().is_empty());

    let wal_object = ObjectLayout::wal_segment(1).expect("WAL segment object");
    let wal_record_bytes =
        crate::format::encode_wal_record(&wal_record(1)).expect("encode WAL record");
    let append_error = backend
        .append_object(&wal_object, &wal_record_bytes)
        .expect_err("cache backend cannot append WAL records");
    assert_eq!(
        append_error.kind(),
        crate::backend::BackendErrorKind::UnsupportedOperation
    );
    assert_eq!(backend.operations(), vec![Operation::Append(wal_object)]);
    assert_durable_snapshot_unchanged(&backend, &before);

    // Direct append is a backend capability probe, not a cache service path.
    // Clear it so the sidecar/cache-path assertion below still forbids append.
    backend.clear_operations();

    let sidecar_metadata = SegmentMetadata::empty(1);
    let sidecar_error = WalSegmentMetadataSidecarService::new(&backend)
        .publish_replace(&sidecar_metadata)
        .expect_err("cache backend cannot publish WAL sidecar durably");
    let sidecar_object = ObjectLayout::wal_segment_metadata(1).expect("WAL sidecar object");
    match sidecar_error {
        WalSegmentMetadataSidecarError::Publish { object, source } => {
            assert_eq!(object, sidecar_object);
            assert_publish_unsupported(&source, &object, BackendCapability::DurablePublish);
        }
        other => panic!("expected sidecar publish error, got {other:?}"),
    }

    let sidecar_load = WalSegmentMetadataSidecarService::new(&backend)
        .load(1)
        .expect("explicit cache-side sidecar load may inspect");
    assert!(sidecar_load.is_missing());

    assert_no_mutating_operations(&backend);
    assert_durable_snapshot_unchanged(&backend, &before);
}

#[test]
fn cache_backend_rejects_snapshot_and_checkpoint_durable_paths_without_recovery_facts() {
    let backend = ObservingCacheBackend::new();
    let before_empty = durable_snapshot(&backend);
    backend.clear_operations();

    let snapshot_error = SnapshotService::new(&backend)
        .publish_create(snapshot_request())
        .expect_err("cache backend cannot publish snapshot durably");
    match snapshot_error {
        SnapshotServiceError::Publish {
            snapshot_id,
            source,
        } => {
            assert_eq!(snapshot_id, 7);
            let object = ObjectLayout::snapshot(7).expect("snapshot object");
            assert_publish_unsupported(&source, &object, BackendCapability::DurablePublish);
        }
        other => panic!("expected snapshot publish error, got {other:?}"),
    }
    assert!(backend.operations().is_empty());
    assert_durable_snapshot_unchanged(&backend, &before_empty);

    let manifest_object = ObjectLayout::database_manifest().expect("database manifest object");
    backend.seed(&manifest_object, &valid_manifest_bytes());
    let before_checkpoint = durable_snapshot(&backend);
    backend.clear_operations();
    let manifest_before = backend.read_seeded(&manifest_object);

    let checkpoint_error = CheckpointService::new(&backend)
        .checkpoint(checkpoint_request())
        .expect_err("cache backend cannot checkpoint durably");

    match checkpoint_error {
        CheckpointServiceError::Manifest { operation, source } => {
            assert_eq!(
                operation,
                CheckpointManifestOperation::PersistActiveWalSegment
            );
            assert_manifest_publish_unsupported(*source, ManifestRole::Database, &manifest_object);
        }
        other => panic!("expected active WAL manifest publish error, got {other:?}"),
    }
    assert_eq!(backend.read_seeded(&manifest_object), manifest_before);
    assert_durable_snapshot_unchanged(&backend, &before_checkpoint);
    assert_no_mutating_operations(&backend);
}

#[test]
fn cache_backend_rejects_quarantine_mutation_and_purge_but_reconcile_is_read_only() {
    let backend = ObservingCacheBackend::new();
    let branch_id = branch_id();
    let (source_object, quarantine_object, inventory_object, inventory_bytes) =
        quarantine_inventory_fixture();
    backend.seed(&source_object, b"table bytes");
    let before_quarantine = durable_snapshot(&backend);
    backend.clear_operations();

    let request = super::QuarantineObjectRequest::new(
        branch_id,
        DATABASE_ID,
        CODEC_ID,
        "table0002",
        source_object.clone(),
        Timestamp::from_micros(2_100_000),
        QuarantineGate::Safe,
    );
    let quarantine_error = QuarantineService::new(&backend)
        .quarantine_object(&request)
        .expect_err("cache backend cannot quarantine durably");
    assert_eq!(
        quarantine_error,
        QuarantineServiceError::UnsupportedCapability {
            capability: BackendCapability::DurablePublish,
        }
    );
    assert!(backend.operations().is_empty());
    assert_durable_snapshot_unchanged(&backend, &before_quarantine);

    backend.seed(&inventory_object, &inventory_bytes);
    backend.seed(&quarantine_object, b"table");
    let before_purge = durable_snapshot(&backend);
    backend.clear_operations();

    let quarantine_service = QuarantineService::new(&backend);
    let inventory_token = quarantine_service
        .load_inventory(branch_id, DATABASE_ID, CODEC_ID)
        .expect("load inventory token")
        .token();
    let purge_error = quarantine_service
        .purge_quarantine(QuarantinePurgeRequest::new(
            branch_id,
            DATABASE_ID,
            CODEC_ID,
            QuarantineGate::Safe,
            Some(inventory_token),
        ))
        .expect_err("cache backend cannot purge durably");
    assert_eq!(
        purge_error,
        QuarantineServiceError::UnsupportedCapability {
            capability: BackendCapability::DurablePublish,
        }
    );
    assert_durable_snapshot_unchanged(&backend, &before_purge);

    let reconcile = QuarantineService::new(&backend)
        .reconcile_branch_quarantine(branch_id, DATABASE_ID, CODEC_ID)
        .expect("reconcile is allowed as explicit read-only inspection");
    assert_eq!(
        reconcile.kind(),
        QuarantineReconciliationKind::CleanInventory
    );
    assert_no_mutating_operations(&backend);
    assert_durable_snapshot_unchanged(&backend, &before_purge);
}

#[test]
fn cache_backend_creates_no_locks_and_ignores_stale_temporary_artifacts() {
    let backend = ObservingCacheBackend::new();
    let lock_object = ObjectLayout::writer_lock().expect("writer lock object");
    let temp_object =
        ObjectLayout::temporary_object("stale-publish", "payload").expect("temp object");
    backend.seed(&temp_object, b"stale durable publish bytes");
    let before = durable_snapshot(&backend);
    backend.clear_operations();

    let lock_error = backend
        .acquire_writer_lock(&lock_object)
        .expect_err("cache backend cannot acquire durable writer lock");
    assert_eq!(
        lock_error.kind(),
        crate::backend::BackendErrorKind::UnsupportedOperation
    );

    let object = cache_object();
    ObjectPublisher::new(&backend)
        .publish_non_durable_replace(&object, b"cache bytes")
        .expect("non-durable publish ignores stale durable temp debris");

    assert_eq!(
        backend.read_seeded(&temp_object),
        b"stale durable publish bytes"
    );
    assert_eq!(
        backend.operations(),
        vec![
            Operation::Lock(lock_object),
            Operation::Publish(object, PublishMode::NonDurableReplace),
        ]
    );
    assert_durable_snapshot_unchanged(&backend, &before);
}
