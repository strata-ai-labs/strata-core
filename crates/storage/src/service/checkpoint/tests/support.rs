use super::super::CheckpointRequest;
use crate::backend::{
    Backend, BackendCapabilities, BackendCapability, BackendError, BackendErrorKind,
    BackendMetadata, BackendRange, BackendResult, PublishDurability, PublishError,
    PublishFailureKind, PublishMode, PublishOutcome,
};
use crate::format::{decode_manifest, DatabaseManifest, SnapshotSection};
use crate::layout::{ObjectFamily, ObjectLayout};
use crate::object::{ObjectName, ObjectPrefix};
use crate::service::DatabaseManifestService;
use std::collections::BTreeMap;
use std::sync::Mutex;
use strata_core::{CommitVersion, Timestamp};

pub(super) const DATABASE_ID: [u8; 16] = [0x73; 16];
pub(super) const OTHER_DATABASE_ID: [u8; 16] = [0x29; 16];
pub(super) const CODEC_ID: &str = "identity";
pub(super) const OTHER_CODEC_ID: &str = "other";
pub(super) const ACTIVE_WAL_SEGMENT: u64 = 9;
pub(super) const SNAPSHOT_ID: u64 = 17;
pub(super) const SNAPSHOT_WATERMARK: u64 = 44;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum PublishPoint {
    InitialManifest,
    ActiveWalManifest,
    Snapshot,
    SnapshotFactsManifest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct PublishEvent {
    pub(super) point: PublishPoint,
    pub(super) mode: PublishMode,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct InjectedPublishFailure {
    point: PublishPoint,
    kind: PublishFailureKind,
}

#[derive(Debug, Default)]
pub(super) struct RecordingBackend {
    objects: Mutex<BTreeMap<ObjectName, Vec<u8>>>,
    events: Mutex<Vec<PublishEvent>>,
    failure: Mutex<Option<InjectedPublishFailure>>,
    reads: Mutex<u64>,
}

impl RecordingBackend {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn fail_next_publish(&self, point: PublishPoint, kind: PublishFailureKind) {
        *self.failure.lock().expect("failure lock") = Some(InjectedPublishFailure { point, kind });
    }

    pub(super) fn clear_events(&self) {
        self.events.lock().expect("events lock").clear();
    }

    pub(super) fn clear_read_count(&self) {
        *self.reads.lock().expect("reads lock") = 0;
    }

    pub(super) fn events(&self) -> Vec<PublishEvent> {
        self.events.lock().expect("events lock").clone()
    }

    pub(super) fn read_count(&self) -> u64 {
        *self.reads.lock().expect("reads lock")
    }

    pub(super) fn read_database_manifest(&self) -> DatabaseManifest {
        let object = ObjectLayout::database_manifest().expect("database manifest object");
        let bytes = self.read_object(&object).expect("read database manifest");
        decode_manifest(&bytes).expect("decode database manifest")
    }

    pub(super) fn snapshot_bytes(&self, snapshot_id: u64) -> Option<Vec<u8>> {
        let object = ObjectLayout::snapshot(snapshot_id).expect("snapshot object");
        self.read_object(&object).ok()
    }

    fn take_failure_for(&self, point: PublishPoint) -> Option<PublishFailureKind> {
        let mut failure = self.failure.lock().expect("failure lock");
        match *failure {
            Some(injected) if injected.point == point => {
                *failure = None;
                Some(injected.kind)
            }
            _ => None,
        }
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

    fn read_object(&self, name: &ObjectName) -> BackendResult<Vec<u8>> {
        *self.reads.lock().expect("reads lock") += 1;
        self.objects
            .lock()
            .expect("objects lock")
            .get(name)
            .cloned()
            .ok_or_else(|| BackendError::new(BackendErrorKind::NotFound, "not found"))
    }

    fn read_range(&self, name: &ObjectName, range: BackendRange) -> BackendResult<Vec<u8>> {
        let bytes = self.read_object(name)?;
        let start = usize::try_from(range.offset())
            .map_err(|_| BackendError::new(BackendErrorKind::InvalidRange, "range overflow"))?;
        let end = range
            .end_offset()
            .ok_or_else(|| BackendError::new(BackendErrorKind::InvalidRange, "range overflow"))?;
        let end = usize::try_from(end)
            .map_err(|_| BackendError::new(BackendErrorKind::InvalidRange, "range overflow"))?;
        if start > bytes.len() {
            return Ok(Vec::new());
        }
        Ok(bytes[start..bytes.len().min(end)].to_vec())
    }

    fn write_object(&self, name: &ObjectName, bytes: &[u8]) -> BackendResult<BackendMetadata> {
        self.objects
            .lock()
            .expect("objects lock")
            .insert(name.clone(), bytes.to_vec());
        Ok(BackendMetadata::new(bytes.len() as u64, None))
    }

    fn delete_object(&self, name: &ObjectName) -> crate::backend::DeleteResult {
        let removed = self
            .objects
            .lock()
            .expect("objects lock")
            .remove(name)
            .is_some();
        crate::backend::durable_delete_result(name, removed)
    }

    fn list_prefix(&self, prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
        Ok(self
            .objects
            .lock()
            .expect("objects lock")
            .keys()
            .filter(|name| name.as_str().starts_with(prefix.as_str()))
            .cloned()
            .collect())
    }

    fn object_metadata(&self, name: &ObjectName) -> BackendResult<BackendMetadata> {
        self.objects
            .lock()
            .expect("objects lock")
            .get(name)
            .map(|bytes| BackendMetadata::new(bytes.len() as u64, None))
            .ok_or_else(|| BackendError::new(BackendErrorKind::NotFound, "not found"))
    }

    fn publish_object(
        &self,
        name: &ObjectName,
        bytes: &[u8],
        mode: PublishMode,
    ) -> Result<PublishOutcome, PublishError> {
        let mut objects = self.objects.lock().expect("objects lock");
        let point = publish_point(name, bytes, &objects);
        self.events
            .lock()
            .expect("events lock")
            .push(PublishEvent { point, mode });

        if mode == PublishMode::Create && objects.contains_key(name) {
            return Err(PublishError::precondition_failed(
                name,
                "object already exists",
            ));
        }

        if let Some(kind) = self.take_failure_for(point) {
            if kind == PublishFailureKind::VisibleDurabilityUnconfirmed {
                objects.insert(name.clone(), bytes.to_vec());
            }
            return Err(PublishError::new(
                name.clone(),
                kind,
                BackendError::new(BackendErrorKind::Unavailable, "injected publish failure"),
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

fn publish_point(
    name: &ObjectName,
    bytes: &[u8],
    objects: &BTreeMap<ObjectName, Vec<u8>>,
) -> PublishPoint {
    if ObjectFamily::from_object_name(name) == Some(ObjectFamily::Snapshots) {
        return PublishPoint::Snapshot;
    }

    let database_manifest_object =
        ObjectLayout::database_manifest().expect("database manifest object");
    if name == &database_manifest_object {
        let current_manifest = objects
            .get(name)
            .and_then(|current| decode_manifest(current).ok());
        return match (current_manifest, decode_manifest(bytes)) {
            (Some(current), Ok(next))
                if current.snapshot_id() != next.snapshot_id()
                    || current.snapshot_watermark() != next.snapshot_watermark() =>
            {
                PublishPoint::SnapshotFactsManifest
            }
            (Some(current), Ok(next))
                if current.active_wal_segment() != next.active_wal_segment() =>
            {
                PublishPoint::ActiveWalManifest
            }
            _ => PublishPoint::InitialManifest,
        };
    }

    PublishPoint::InitialManifest
}

fn section(payload: &'static [u8]) -> SnapshotSection {
    SnapshotSection::new(0x01, payload.to_vec()).expect("snapshot section")
}

pub(super) fn request() -> CheckpointRequest {
    request_with_facts(
        ACTIVE_WAL_SEGMENT,
        SNAPSHOT_ID,
        CommitVersion::new(SNAPSHOT_WATERMARK),
        CODEC_ID,
        DATABASE_ID,
    )
}

pub(super) fn request_with_facts(
    active_wal_segment: u64,
    snapshot_id: u64,
    snapshot_watermark: CommitVersion,
    codec_id: &str,
    database_id: [u8; 16],
) -> CheckpointRequest {
    CheckpointRequest::new(
        database_id,
        codec_id,
        active_wal_segment,
        snapshot_id,
        snapshot_watermark,
        Timestamp::from_micros(1_700),
        vec![section(b"rows"), section(b"index")],
    )
}

pub(super) fn seeded_backend(codec_id: &str) -> RecordingBackend {
    let backend = RecordingBackend::new();
    DatabaseManifestService::new(&backend)
        .create_initial(DATABASE_ID, codec_id)
        .expect("create database manifest");
    backend.clear_events();
    backend.clear_read_count();
    backend
}

pub(super) fn assert_no_snapshot_publish(backend: &RecordingBackend) {
    assert!(backend
        .events()
        .iter()
        .all(|event| event.point != PublishPoint::Snapshot));
    assert!(backend.snapshot_bytes(SNAPSHOT_ID).is_none());
}

pub(super) fn assert_manifest_has_no_snapshot_facts(backend: &RecordingBackend) {
    let manifest = backend.read_database_manifest();
    assert_eq!(manifest.snapshot_id(), None);
    assert_eq!(manifest.snapshot_watermark(), None);
}
