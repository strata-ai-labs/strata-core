use super::{SnapshotPublishRequest, SnapshotService, SnapshotServiceError};
#[cfg(all(feature = "localfs", unix))]
use crate::backend::local_fs::LocalFsBackend;
use crate::backend::{
    Backend, BackendCapabilities, BackendCapability, BackendError, BackendErrorKind,
    BackendMetadata, BackendRange, BackendResult, PublishError, PublishFailureKind, PublishMode,
    PublishOutcome,
};
use crate::format::SnapshotSection;
use crate::layout::ObjectLayout;
use crate::object::{ObjectName, ObjectPrefix};
use strata_core_next::{CommitVersion, Timestamp};

const DATABASE_ID: [u8; 16] = [0x47; 16];
const SNAPSHOT_ID: u64 = 23;
const SNAPSHOT_WATERMARK: CommitVersion = CommitVersion::new(51);
const SNAPSHOT_CREATED_AT: Timestamp = Timestamp::from_micros(2_300);
const CODEC_ID: &str = "identity";

struct SyntheticPublishBackend {
    kind: PublishFailureKind,
}

impl Backend for SyntheticPublishBackend {
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::from_slice(&[
            BackendCapability::ReadObject,
            BackendCapability::ReadRange,
            BackendCapability::WriteObject,
            BackendCapability::DeleteObject,
            BackendCapability::ListPrefix,
            BackendCapability::ObjectMetadata,
            BackendCapability::DurablePublish,
            BackendCapability::DurableSync,
        ])
    }

    fn read_object(&self, _name: &ObjectName) -> BackendResult<Vec<u8>> {
        Err(BackendError::new(BackendErrorKind::NotFound, "not found"))
    }

    fn read_range(&self, _name: &ObjectName, _range: BackendRange) -> BackendResult<Vec<u8>> {
        Err(BackendError::new(BackendErrorKind::NotFound, "not found"))
    }

    fn write_object(&self, _name: &ObjectName, _bytes: &[u8]) -> BackendResult<BackendMetadata> {
        Ok(BackendMetadata::new(0, None))
    }

    fn delete_object(&self, _name: &ObjectName) -> BackendResult<()> {
        Ok(())
    }

    fn list_prefix(&self, _prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
        Ok(Vec::new())
    }

    fn object_metadata(&self, _name: &ObjectName) -> BackendResult<BackendMetadata> {
        Err(BackendError::new(BackendErrorKind::NotFound, "not found"))
    }

    fn publish_object(
        &self,
        name: &ObjectName,
        _bytes: &[u8],
        _mode: PublishMode,
    ) -> Result<PublishOutcome, PublishError> {
        Err(PublishError::new(
            name.clone(),
            self.kind,
            BackendError::new(
                BackendErrorKind::Unavailable,
                "synthetic publish failure before visibility",
            ),
        ))
    }
}

#[cfg(all(feature = "localfs", unix))]
#[test]
fn snapshot_publish_temporary_write_fault_is_before_visibility() {
    let dir = tempfile::tempdir().expect("tempdir");
    let backend = LocalFsBackend::new(dir.path());
    backend
        .inject_temporary_write_publish_fault()
        .expect("inject publish fault");

    let error = SnapshotService::new(&backend)
        .publish_create(request(SNAPSHOT_ID))
        .expect_err("temporary write fault should stop snapshot publication");

    assert_snapshot_publish_error(error, PublishFailureKind::FailedBeforeVisibility);
    assert_snapshot_absent(&backend, SNAPSHOT_ID);
}

#[cfg(all(feature = "localfs", unix))]
#[test]
fn snapshot_publish_temporary_sync_fault_is_before_visibility() {
    let dir = tempfile::tempdir().expect("tempdir");
    let backend = LocalFsBackend::new(dir.path());
    backend
        .inject_temporary_sync_publish_fault()
        .expect("inject publish fault");

    let error = SnapshotService::new(&backend)
        .publish_create(request(SNAPSHOT_ID))
        .expect_err("temporary sync fault should stop snapshot publication");

    assert_snapshot_publish_error(error, PublishFailureKind::FailedBeforeVisibility);
    assert_snapshot_absent(&backend, SNAPSHOT_ID);
}

#[cfg(all(feature = "localfs", unix))]
#[test]
fn snapshot_publish_create_precondition_preserves_existing_snapshot_bytes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let backend = LocalFsBackend::new(dir.path());
    let service = SnapshotService::new(&backend);
    let object = ObjectLayout::snapshot(SNAPSHOT_ID).expect("snapshot object");
    service
        .publish_create(request(SNAPSHOT_ID))
        .expect("initial snapshot publish");
    let original = backend
        .read_object(&object)
        .expect("read original snapshot");

    let error = service
        .publish_create(request_with_payload(SNAPSHOT_ID, b"different"))
        .expect_err("create precondition should reject duplicate snapshot");

    assert_snapshot_publish_error(error, PublishFailureKind::PreconditionFailed);
    assert_eq!(
        backend
            .read_object(&object)
            .expect("snapshot bytes remain readable"),
        original
    );
}

#[test]
fn snapshot_publish_visibility_unknown_is_preserved_at_service_boundary() {
    // Local filesystem create faults are known before visibility in the test
    // hook. This backend models a backend that cannot prove create visibility,
    // so the snapshot service must preserve the ambiguous publish class.
    let backend = SyntheticPublishBackend {
        kind: PublishFailureKind::VisibilityUnknown,
    };

    let error = SnapshotService::new(&backend)
        .publish_create(request(SNAPSHOT_ID))
        .expect_err("visibility uncertainty should remain typed");

    assert_snapshot_publish_error(error, PublishFailureKind::VisibilityUnknown);
}

#[cfg(all(feature = "localfs", unix))]
#[test]
fn snapshot_publish_parent_sync_fault_leaves_snapshot_visible_but_unconfirmed() {
    let dir = tempfile::tempdir().expect("tempdir");
    let backend = LocalFsBackend::new(dir.path());
    backend
        .inject_parent_sync_publish_fault()
        .expect("inject publish fault");

    let error = SnapshotService::new(&backend)
        .publish_create(request(SNAPSHOT_ID))
        .expect_err("parent sync fault should leave durability uncertain");

    assert_snapshot_publish_error(error, PublishFailureKind::VisibleDurabilityUnconfirmed);
    // Parent-directory sync happens after final object visibility. Even though
    // durability is unconfirmed, recovery must be able to inspect the snapshot.
    let loaded = SnapshotService::new(&backend)
        .load_required_for_codec(SNAPSHOT_ID, DATABASE_ID, CODEC_ID)
        .expect("snapshot is visible after parent sync failure");
    assert_eq!(loaded.header().snapshot_id(), SNAPSHOT_ID);
    assert_eq!(loaded.sections()[0].payload(), b"rows");
}

fn assert_snapshot_publish_error(error: SnapshotServiceError, expected: PublishFailureKind) {
    let object = ObjectLayout::snapshot(SNAPSHOT_ID).expect("snapshot object");
    match error {
        SnapshotServiceError::Publish {
            snapshot_id,
            source,
        } => {
            assert_eq!(snapshot_id, SNAPSHOT_ID);
            assert_eq!(source.kind(), expected);
            assert_eq!(source.object(), &object);
        }
        other => panic!("expected snapshot publish error, got {other:?}"),
    }
}

#[cfg(all(feature = "localfs", unix))]
fn assert_snapshot_absent(backend: &dyn Backend, snapshot_id: u64) {
    let object = ObjectLayout::snapshot(snapshot_id).expect("snapshot object");
    assert_eq!(
        backend
            .read_object(&object)
            .expect_err("snapshot object should be absent")
            .kind(),
        BackendErrorKind::NotFound
    );
}

fn request(snapshot_id: u64) -> SnapshotPublishRequest {
    request_with_payload(snapshot_id, b"rows")
}

fn request_with_payload(snapshot_id: u64, payload: &'static [u8]) -> SnapshotPublishRequest {
    SnapshotPublishRequest::new(
        snapshot_id,
        SNAPSHOT_WATERMARK,
        SNAPSHOT_CREATED_AT,
        DATABASE_ID,
        CODEC_ID,
        vec![SnapshotSection::new(0x01, payload.to_vec()).expect("section")],
    )
}
