use super::*;
use crate::backend::{
    Backend, BackendAppend, BackendCapabilities, BackendError, BackendErrorKind, BackendMetadata,
    BackendRange, BackendResult, BackendWriterGuard, PublishDurability, PublishError,
    PublishFailureKind, PublishMode, PublishOutcome, PublishResult,
    DURABLE_LOCAL_MODE_REQUIREMENTS,
};
use crate::branch::BranchRuntimeConfig;
use crate::commit::{CommitBranchGeneration, CommitManualTimestampSource, CommitRuntimeConfig};
use crate::config::mode::DurabilityPolicy;
use crate::format::{
    encode_manifest, encode_wal_segment_header, DatabaseManifest, WalSegmentHeader,
};
use crate::layout::ObjectLayout;
use crate::object::{ObjectName, ObjectPrefix};
use crate::service::WalServiceConfig;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, Ordering};
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

fn assemble_shell(
    mode: StorageMode,
    branch: BranchId,
    backend: &DurableTestBackend,
) -> LifecycleResult<LifecycleDurableLocalShell<'_>> {
    LifecycleDurableLocalShell::assemble(request(mode, branch)?, backend, timestamp_source())
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
    fail_lock: bool,
    publish_failure: Option<PublishFailureKind>,
    create_race_manifest: Mutex<Option<DatabaseManifest>>,
    fail_metadata: bool,
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
            fail_lock: false,
            publish_failure: None,
            create_race_manifest: Mutex::new(None),
            fail_metadata: false,
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
            publish_failure: Some(kind),
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

    fn write_raw(&self, object: ObjectName, bytes: Vec<u8>) {
        self.objects.lock().expect("objects").insert(object, bytes);
    }

    fn operations(&self) -> Vec<Operation> {
        self.operations.lock().expect("operations").clone()
    }

    fn operation_kinds(&self) -> Vec<OperationKind> {
        self.operations().iter().map(Operation::kind).collect()
    }

    fn lock_is_held(&self) -> bool {
        self.lock_held.load(Ordering::SeqCst)
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
        if let Some(kind) = self.publish_failure {
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
}

impl Drop for HeldWriterLock {
    fn drop(&mut self) {
        self.locked.store(false, Ordering::SeqCst);
    }
}
