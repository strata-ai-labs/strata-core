#![deny(unsafe_code)]

mod optional_fallback;

use super::{
    WalSegmentMetadataSidecarError, WalSegmentMetadataSidecarLoad, WalSegmentMetadataSidecarService,
};
#[cfg(all(feature = "localfs", unix))]
use crate::backend::local_fs::LocalFsBackend;
use crate::backend::memory::MemoryBackend;
use crate::backend::{
    Backend, BackendAppend, BackendCapabilities, BackendCapability, BackendError, BackendErrorKind,
    BackendMetadata, BackendRange, DeleteError, DeleteOutcome, DeleteStatus, PublishDurability,
    PublishError, PublishFailureKind, PublishMode, PublishOutcome, PublishResult,
};
use crate::format::{
    decode_segment_metadata, encode_segment_metadata, FormatError, SegmentMetadata,
};
use crate::layout::ObjectLayout;
use crate::object::{ObjectName, ObjectPrefix};
use std::collections::HashMap;
use std::sync::Mutex;
use strata_core_next::{CommitVersion, Timestamp};

type CorruptMutation = fn(Vec<u8>) -> Vec<u8>;
type CorruptCase = (&'static str, CorruptMutation);

struct RecordingBackend {
    objects: Mutex<HashMap<ObjectName, Vec<u8>>>,
    publish_modes: Mutex<Vec<PublishMode>>,
    publish_failure: Mutex<Option<PublishFailureKind>>,
    read_failure: Mutex<Option<BackendError>>,
    delete_failure: Mutex<Option<BackendError>>,
    publish_durability: Mutex<PublishDurability>,
}

impl RecordingBackend {
    fn new() -> Self {
        Self {
            objects: Mutex::new(HashMap::new()),
            publish_modes: Mutex::new(Vec::new()),
            publish_failure: Mutex::new(None),
            read_failure: Mutex::new(None),
            delete_failure: Mutex::new(None),
            publish_durability: Mutex::new(PublishDurability::Durable),
        }
    }

    fn insert(&self, object: ObjectName, bytes: Vec<u8>) {
        self.objects.lock().expect("objects").insert(object, bytes);
    }

    fn read_stored(&self, object: &ObjectName) -> Option<Vec<u8>> {
        self.objects.lock().expect("objects").get(object).cloned()
    }

    fn set_publish_failure(&self, kind: PublishFailureKind) {
        *self.publish_failure.lock().expect("publish failure") = Some(kind);
    }

    fn set_read_failure(&self, source: BackendError) {
        *self.read_failure.lock().expect("read failure") = Some(source);
    }

    fn set_delete_failure(&self, source: BackendError) {
        *self.delete_failure.lock().expect("delete failure") = Some(source);
    }

    fn set_publish_durability(&self, durability: PublishDurability) {
        *self.publish_durability.lock().expect("publish durability") = durability;
    }

    fn publish_modes(&self) -> Vec<PublishMode> {
        self.publish_modes.lock().expect("publish modes").clone()
    }
}

impl Backend for RecordingBackend {
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

    fn read_object(&self, name: &ObjectName) -> Result<Vec<u8>, BackendError> {
        if let Some(source) = self.read_failure.lock().expect("read failure").clone() {
            return Err(source);
        }

        self.objects
            .lock()
            .expect("objects")
            .get(name)
            .cloned()
            .ok_or_else(|| BackendError::new(BackendErrorKind::NotFound, "missing object"))
    }

    fn read_range(
        &self,
        _name: &ObjectName,
        _range: BackendRange,
    ) -> Result<Vec<u8>, BackendError> {
        Err(BackendError::new(
            BackendErrorKind::UnsupportedOperation,
            "read range not used by sidecar tests",
        ))
    }

    fn write_object(
        &self,
        name: &ObjectName,
        bytes: &[u8],
    ) -> Result<BackendMetadata, BackendError> {
        self.objects
            .lock()
            .expect("objects")
            .insert(name.clone(), bytes.to_vec());
        Ok(BackendMetadata::new(bytes.len() as u64, None))
    }

    fn delete_object(&self, name: &ObjectName) -> crate::backend::DeleteResult {
        if let Some(source) = self.delete_failure.lock().expect("delete failure").clone() {
            return crate::backend::failed_delete_result(name, source);
        }

        let removed = self.objects.lock().expect("objects").remove(name).is_some();
        crate::backend::durable_delete_result(name, removed)
    }

    fn list_prefix(&self, prefix: &ObjectPrefix) -> Result<Vec<ObjectName>, BackendError> {
        let mut listed: Vec<_> = self
            .objects
            .lock()
            .expect("objects")
            .keys()
            .filter(|object| object.as_str().starts_with(prefix.as_str()))
            .cloned()
            .collect();
        listed.sort();
        Ok(listed)
    }

    fn object_metadata(&self, name: &ObjectName) -> Result<BackendMetadata, BackendError> {
        let length = self
            .objects
            .lock()
            .expect("objects")
            .get(name)
            .map(Vec::len)
            .ok_or_else(|| BackendError::new(BackendErrorKind::NotFound, "missing object"))?;
        Ok(BackendMetadata::new(length as u64, None))
    }

    fn append_object(
        &self,
        _name: &ObjectName,
        _bytes: &[u8],
    ) -> Result<BackendAppend, BackendError> {
        Err(BackendError::new(
            BackendErrorKind::UnsupportedOperation,
            "append not used by sidecar tests",
        ))
    }

    fn sync_object(&self, _name: &ObjectName) -> crate::backend::BackendResult<()> {
        Ok(())
    }

    fn publish_object(
        &self,
        name: &ObjectName,
        bytes: &[u8],
        mode: PublishMode,
    ) -> PublishResult<PublishOutcome> {
        self.publish_modes.lock().expect("publish modes").push(mode);
        if let Some(kind) = self.publish_failure.lock().expect("publish failure").take() {
            if kind == PublishFailureKind::VisibleDurabilityUnconfirmed {
                self.objects
                    .lock()
                    .expect("objects")
                    .insert(name.clone(), bytes.to_vec());
            }
            return Err(PublishError::new(
                name.clone(),
                kind,
                BackendError::new(BackendErrorKind::Unavailable, "injected publish failure"),
            ));
        }

        self.objects
            .lock()
            .expect("objects")
            .insert(name.clone(), bytes.to_vec());
        Ok(PublishOutcome::new(
            name.clone(),
            BackendMetadata::new(bytes.len() as u64, None),
            *self.publish_durability.lock().expect("publish durability"),
        ))
    }
}

fn metadata(segment_id: u64) -> SegmentMetadata {
    let mut metadata = SegmentMetadata::empty(segment_id);
    metadata.track_record(CommitVersion::new(7), Timestamp::from_micros(1_700));
    metadata.track_record(CommitVersion::new(11), Timestamp::from_micros(1_900));
    metadata
}

fn service_load(backend: &RecordingBackend, segment_id: u64) -> WalSegmentMetadataSidecarLoad {
    WalSegmentMetadataSidecarService::new(backend)
        .load(segment_id)
        .expect("load")
}

fn sidecar_object(segment_id: u64) -> ObjectName {
    ObjectLayout::wal_segment_metadata(segment_id).expect("sidecar object")
}

fn corrupt_magic(mut bytes: Vec<u8>) -> Vec<u8> {
    bytes[0] = b'X';
    bytes
}

fn corrupt_future_version(mut bytes: Vec<u8>) -> Vec<u8> {
    bytes[4..8].copy_from_slice(&2u32.to_le_bytes());
    bytes
}

fn corrupt_pre_v1_version(mut bytes: Vec<u8>) -> Vec<u8> {
    bytes[4..8].copy_from_slice(&0u32.to_le_bytes());
    bytes
}

fn corrupt_crc(mut bytes: Vec<u8>) -> Vec<u8> {
    bytes[20] ^= 0xff;
    bytes
}

fn corrupt_trailing(mut bytes: Vec<u8>) -> Vec<u8> {
    bytes.push(0);
    bytes
}

#[test]
fn sidecar_rejects_zero_segment_id_before_backend_access() {
    let backend = RecordingBackend::new();
    let service = WalSegmentMetadataSidecarService::new(&backend);

    let load_error = service
        .load(0)
        .expect_err("zero segment id must be rejected");
    let publish_error = service
        .publish_replace(&SegmentMetadata::empty(0))
        .expect_err("zero segment id must be rejected before publish");
    let delete_error = service
        .delete(0)
        .expect_err("zero segment id must be rejected before delete");

    assert_eq!(
        load_error,
        WalSegmentMetadataSidecarError::InvalidSegmentId { segment_id: 0 }
    );
    assert_eq!(
        publish_error,
        WalSegmentMetadataSidecarError::InvalidSegmentId { segment_id: 0 }
    );
    assert_eq!(
        delete_error,
        WalSegmentMetadataSidecarError::InvalidSegmentId { segment_id: 0 }
    );
    assert!(backend.objects.lock().expect("objects").is_empty());
    assert!(backend.publish_modes().is_empty());
}

#[test]
fn sidecar_publish_and_load_round_trips_segment_metadata() {
    let backend = RecordingBackend::new();
    let service = WalSegmentMetadataSidecarService::new(&backend);
    let metadata = metadata(9);

    let write = service.publish_replace(&metadata).expect("publish");
    let loaded = service.load(9).expect("load");

    assert_eq!(write.segment_id(), 9);
    assert_eq!(write.object(), &sidecar_object(9));
    assert_eq!(write.byte_count(), 60);
    assert_eq!(write.outcome().metadata().size_bytes(), 60);
    assert!(loaded.is_present());
    if let WalSegmentMetadataSidecarLoad::Present(sidecar) = loaded {
        assert_eq!(sidecar.segment_id(), 9);
        assert_eq!(sidecar.object(), &sidecar_object(9));
        assert_eq!(sidecar.metadata(), &metadata);
    } else {
        unreachable!("present sidecar was asserted above");
    }
}

#[test]
fn sidecar_publish_uses_durable_replace() {
    let backend = RecordingBackend::new();
    let service = WalSegmentMetadataSidecarService::new(&backend);

    service
        .publish_replace(&metadata(3))
        .expect("publish sidecar");

    assert_eq!(backend.publish_modes(), vec![PublishMode::Replace]);
}

#[test]
fn sidecar_publish_rejects_non_durable_publish_outcome() {
    let backend = RecordingBackend::new();
    backend.set_publish_durability(PublishDurability::NonDurable);
    let service = WalSegmentMetadataSidecarService::new(&backend);

    let error = service
        .publish_replace(&metadata(3))
        .expect_err("sidecar publish must report durable outcome");

    assert!(matches!(
        error,
        WalSegmentMetadataSidecarError::InvalidPublishMetadata { object, field }
            if object == sidecar_object(3) && field == "durability"
    ));
}

#[test]
fn memory_backend_rejects_sidecar_durable_publish() {
    let backend = MemoryBackend::new();
    let service = WalSegmentMetadataSidecarService::new(&backend);

    let error = service
        .publish_replace(&metadata(12))
        .expect_err("memory backend cannot durably publish a sidecar");

    assert!(matches!(
        error,
        WalSegmentMetadataSidecarError::Publish { object, source }
            if object == sidecar_object(12)
                && source.kind() == PublishFailureKind::Unsupported
                && source.source_error().kind() == BackendErrorKind::UnsupportedOperation
    ));
    assert_eq!(
        service.load(12).expect("load after failed publish"),
        WalSegmentMetadataSidecarLoad::Missing {
            segment_id: 12,
            object: sidecar_object(12)
        }
    );
}

#[cfg(all(feature = "localfs", unix))]
#[test]
fn local_filesystem_backend_publishes_durable_sidecar_bytes() {
    let dir = tempfile::tempdir().expect("tempdir");
    let backend = LocalFsBackend::new(dir.path());
    let service = WalSegmentMetadataSidecarService::new(&backend);
    let metadata = metadata(14);

    let write = service.publish_replace(&metadata).expect("publish");
    let loaded = service.load(14).expect("load");

    assert_eq!(write.object(), &sidecar_object(14));
    assert_eq!(write.outcome().durability(), PublishDurability::Durable);
    assert_eq!(
        backend.read_object(write.object()).expect("read sidecar"),
        encode_segment_metadata(&metadata)
    );
    assert!(matches!(
        loaded,
        WalSegmentMetadataSidecarLoad::Present(sidecar) if sidecar.metadata() == &metadata
    ));
}

#[test]
fn missing_sidecar_returns_recoverable_missing_fact() {
    let backend = RecordingBackend::new();

    let loaded = service_load(&backend, 4);

    assert_eq!(
        loaded,
        WalSegmentMetadataSidecarLoad::Missing {
            segment_id: 4,
            object: sidecar_object(4)
        }
    );
    assert!(loaded.is_missing());
}

#[test]
fn corrupt_sidecar_bytes_return_recoverable_corrupt_fact() {
    let cases: [CorruptCase; 5] = [
        ("magic", corrupt_magic),
        ("future version", corrupt_future_version),
        ("pre-v1 version", corrupt_pre_v1_version),
        ("crc", corrupt_crc),
        ("trailing", corrupt_trailing),
    ];

    for (name, mutate) in cases {
        let backend = RecordingBackend::new();
        backend.insert(
            sidecar_object(7),
            mutate(encode_segment_metadata(&metadata(7))),
        );

        let loaded = service_load(&backend, 7);

        assert!(loaded.is_corrupt(), "{name} should be corrupt");
        if let WalSegmentMetadataSidecarLoad::Corrupt {
            segment_id, object, ..
        } = loaded
        {
            assert_eq!(segment_id, 7);
            assert_eq!(object, sidecar_object(7));
        }
    }
}

#[test]
fn sidecar_segment_id_mismatch_returns_recoverable_corrupt_fact() {
    let backend = RecordingBackend::new();
    backend.insert(sidecar_object(8), encode_segment_metadata(&metadata(9)));

    let loaded = service_load(&backend, 8);

    assert_eq!(
        loaded,
        WalSegmentMetadataSidecarLoad::Corrupt {
            segment_id: 8,
            object: sidecar_object(8),
            source: FormatError::InvalidValue {
                field: "segment_id"
            }
        }
    );
}

#[test]
fn backend_read_failure_returns_typed_error() {
    let backend = RecordingBackend::new();
    backend.set_read_failure(BackendError::new(
        BackendErrorKind::Unavailable,
        "backend unavailable",
    ));
    let service = WalSegmentMetadataSidecarService::new(&backend);

    let error = service.load(5).expect_err("read failure");

    assert!(matches!(
        error,
        WalSegmentMetadataSidecarError::Read {
            object,
            source
        } if object == sidecar_object(5)
            && source.kind() == BackendErrorKind::Unavailable
    ));
}

#[test]
fn publish_failure_preserves_publish_kind_and_does_not_touch_wal_segment() {
    let kinds = [
        PublishFailureKind::Unsupported,
        PublishFailureKind::PreconditionFailed,
        PublishFailureKind::FailedBeforeVisibility,
        PublishFailureKind::VisibilityUnknown,
        PublishFailureKind::VisibleDurabilityUnconfirmed,
    ];

    for kind in kinds {
        let backend = RecordingBackend::new();
        let wal_object = ObjectLayout::wal_segment(5).expect("wal segment");
        backend.insert(wal_object.clone(), b"wal bytes".to_vec());
        backend.set_publish_failure(kind);
        let service = WalSegmentMetadataSidecarService::new(&backend);

        let error = service
            .publish_replace(&metadata(5))
            .expect_err("publish failure");

        assert!(matches!(
            error,
            WalSegmentMetadataSidecarError::Publish { source, .. }
                if source.kind() == kind
        ));
        assert_eq!(
            backend.read_stored(&wal_object),
            Some(b"wal bytes".to_vec())
        );
    }
}

#[test]
fn delete_failure_is_reported_without_touching_wal_segment() {
    let backend = RecordingBackend::new();
    let wal_object = ObjectLayout::wal_segment(6).expect("wal segment");
    backend.insert(wal_object.clone(), b"wal bytes".to_vec());
    backend.insert(sidecar_object(6), encode_segment_metadata(&metadata(6)));
    backend.set_delete_failure(BackendError::new(
        BackendErrorKind::Unavailable,
        "delete unavailable",
    ));
    let service = WalSegmentMetadataSidecarService::new(&backend);

    let report = service.delete(6).expect("delete report");

    assert_eq!(report.segment_id(), 6);
    assert_eq!(report.object(), &sidecar_object(6));
    assert!(!report.deleted());
    assert_eq!(
        report.failure().map(BackendError::kind),
        Some(BackendErrorKind::Unavailable)
    );
    assert!(report.outcome().is_none());
    assert_eq!(
        report.delete_error().map(DeleteError::object),
        Some(&sidecar_object(6))
    );
    assert_eq!(
        backend.read_stored(&wal_object),
        Some(b"wal bytes".to_vec())
    );
    assert!(matches!(
        decode_segment_metadata(
            &backend
                .read_stored(&sidecar_object(6))
                .expect("sidecar remains")
        ),
        Ok(decoded) if decoded.segment_id() == 6
    ));
}

#[test]
fn delete_missing_sidecar_is_a_noop_fact() {
    let backend = RecordingBackend::new();
    let service = WalSegmentMetadataSidecarService::new(&backend);

    let report = service.delete(10).expect("delete report");

    assert_eq!(report.segment_id(), 10);
    assert_eq!(report.object(), &sidecar_object(10));
    assert!(!report.deleted());
    assert!(report.failure().is_none());
    assert_eq!(
        report.outcome().map(DeleteOutcome::status),
        Some(DeleteStatus::AlreadyMissing)
    );
}

#[test]
fn delete_not_found_sidecar_error_is_a_noop_fact() {
    let backend = RecordingBackend::new();
    backend.set_delete_failure(BackendError::new(
        BackendErrorKind::NotFound,
        "sidecar already absent",
    ));
    let service = WalSegmentMetadataSidecarService::new(&backend);

    let report = service.delete(11).expect("delete report");

    assert_eq!(report.segment_id(), 11);
    assert_eq!(report.object(), &sidecar_object(11));
    assert!(!report.deleted());
    assert!(report.failure().is_none());
    assert_eq!(
        report.outcome().map(DeleteOutcome::status),
        Some(DeleteStatus::AlreadyMissing)
    );
}
