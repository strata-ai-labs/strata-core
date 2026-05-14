use super::{WalService, WalServiceError};
use crate::backend::{
    Backend, BackendAppend, BackendCapabilities, BackendCapability, BackendError, BackendErrorKind,
    BackendMetadata, BackendRange, BackendResult, PublishDurability, PublishMode, PublishOutcome,
    PublishResult,
};
use crate::format::{
    encode_wal_record, encode_wal_record_envelope, encode_wal_segment_header, WalRecord,
    WalRecordEnvelope, WalSegmentHeader,
};
use crate::layout::ObjectLayout;
use crate::object::{ObjectName, ObjectPrefix};
use std::collections::BTreeMap;
use std::sync::Mutex;
use strata_core_next::{BranchId, CommitVersion, Timestamp};

pub(super) fn database_id() -> [u8; 16] {
    [
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e,
        0x1f,
    ]
}

#[cfg(all(feature = "localfs", unix))]
pub(super) fn other_database_id() -> [u8; 16] {
    other_database_id_for_tests()
}

pub(super) fn other_database_id_for_tests() -> [u8; 16] {
    [
        0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e,
        0x2f,
    ]
}

fn branch_id() -> BranchId {
    BranchId::from_bytes([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15])
}

pub(super) fn alternate_branch_id() -> BranchId {
    BranchId::from_bytes([
        0xf0, 0xf1, 0xf2, 0xf3, 0xf4, 0xf5, 0xf6, 0xf7, 0xf8, 0xf9, 0xfa, 0xfb, 0xfc, 0xfd, 0xfe,
        0xff,
    ])
}

pub(super) fn record(version: u64, payload: impl Into<Vec<u8>>) -> WalRecord {
    record_for_branch(version, branch_id(), payload)
}

pub(super) fn record_for_branch(
    version: u64,
    branch_id: BranchId,
    payload: impl Into<Vec<u8>>,
) -> WalRecord {
    WalRecord::new(
        CommitVersion::new(version),
        branch_id,
        Timestamp::from_micros(1_700_000_000_000_000 + version),
        payload,
    )
}

pub(super) fn required_wal_capabilities() -> [BackendCapability; 7] {
    [
        BackendCapability::ReadObject,
        BackendCapability::ReadRange,
        BackendCapability::ListPrefix,
        BackendCapability::ObjectMetadata,
        BackendCapability::AppendObject,
        BackendCapability::DurablePublish,
        BackendCapability::DurableSync,
    ]
}

pub(super) fn stored_wal_capabilities() -> [BackendCapability; 8] {
    [
        BackendCapability::ReadObject,
        BackendCapability::ReadRange,
        BackendCapability::ListPrefix,
        BackendCapability::ObjectMetadata,
        BackendCapability::AppendObject,
        BackendCapability::DurablePublish,
        BackendCapability::DurableSync,
        BackendCapability::DeleteObject,
    ]
}

pub(super) fn segment_bytes(segment_id: u64, records: &[WalRecord]) -> Vec<u8> {
    let mut bytes = encode_wal_segment_header(&WalSegmentHeader::new(segment_id, database_id()));
    for record in records {
        bytes.extend_from_slice(&record_frame(record));
    }
    bytes
}

pub(super) fn record_frame(record: &WalRecord) -> Vec<u8> {
    let record_bytes = encode_wal_record(record).expect("encode WAL record");
    let envelope = WalRecordEnvelope::new(record_bytes).expect("WAL record envelope");
    encode_wal_record_envelope(&envelope).expect("encode WAL record envelope")
}

pub(super) fn record_frame_len(record: &WalRecord) -> u64 {
    record_frame(record).len() as u64
}

pub(super) fn record_with_frame_len(version: u64, frame_len: u64, byte: u8) -> WalRecord {
    let max_payload_len = usize::try_from(frame_len).expect("frame length fits usize");
    for payload_len in 0..=max_payload_len {
        let record = record(version, vec![byte; payload_len]);
        if record_frame_len(&record) == frame_len {
            return record;
        }
    }
    panic!("no WAL record frame has {frame_len} encoded bytes");
}

pub(super) fn assert_unsupported_capability(error: &WalServiceError, expected: BackendCapability) {
    assert_eq!(
        *error,
        WalServiceError::UnsupportedCapability {
            capability: expected
        }
    );
}

pub(super) fn assert_backend_list_invalid_object(
    error: WalServiceError,
    expected_object: &ObjectName,
) {
    match error {
        WalServiceError::Backend {
            operation,
            object,
            source,
        } => {
            assert_eq!(operation, super::super::WalOperation::List);
            assert_eq!(object, *expected_object);
            assert_eq!(source.kind(), BackendErrorKind::InvalidObjectName);
        }
        other => panic!("expected WAL list backend error, got {other:?}"),
    }
}

pub(super) fn wal_listed_object(component: &str) -> ObjectName {
    ObjectName::new(format!(
        "{}{component}",
        ObjectLayout::wal_prefix().expect("WAL prefix").as_str()
    ))
    .expect("WAL-looking object")
}

pub(super) fn open_error(
    result: Result<WalService<'_>, WalServiceError>,
    message: &str,
) -> WalServiceError {
    match result {
        Ok(_) => panic!("{message}"),
        Err(error) => error,
    }
}

pub(super) fn assert_empty_active_metadata(service: &WalService<'_>, segment_id: u64) {
    let metadata = service.active_metadata();
    assert_eq!(metadata.segment_id(), segment_id);
    assert_eq!(metadata.record_count(), 0);
    assert_eq!(metadata.min_commit_version(), CommitVersion::MAX);
    assert_eq!(metadata.max_commit_version(), CommitVersion::ZERO);
    assert_eq!(metadata.min_timestamp(), Timestamp::MAX);
    assert_eq!(metadata.max_timestamp(), Timestamp::EPOCH);
}

pub(super) fn assert_clean_append_state(service: &WalService<'_>, segment_id: u64) {
    assert_eq!(service.active_segment_id(), segment_id);
    assert_eq!(service.dirty_bytes(), 0);
    assert_eq!(service.dirty_records(), 0);
    assert_empty_active_metadata(service, segment_id);
}

pub(super) struct CapabilityProbeBackend {
    capabilities: BackendCapabilities,
}

impl CapabilityProbeBackend {
    pub(super) fn missing(missing: BackendCapability) -> Self {
        let available = required_wal_capabilities()
            .into_iter()
            .filter(|capability| *capability != missing)
            .collect::<Vec<_>>();
        Self {
            capabilities: BackendCapabilities::from_slice(&available),
        }
    }
}

impl Backend for CapabilityProbeBackend {
    fn capabilities(&self) -> BackendCapabilities {
        self.capabilities
    }

    fn read_object(&self, _name: &ObjectName) -> BackendResult<Vec<u8>> {
        Err(BackendError::new(
            BackendErrorKind::Unknown,
            "capability probe should not read",
        ))
    }

    fn read_range(&self, _name: &ObjectName, _range: BackendRange) -> BackendResult<Vec<u8>> {
        Err(BackendError::new(
            BackendErrorKind::Unknown,
            "capability probe should not read range",
        ))
    }

    fn write_object(&self, _name: &ObjectName, _bytes: &[u8]) -> BackendResult<BackendMetadata> {
        Err(BackendError::new(
            BackendErrorKind::Unknown,
            "capability probe should not write",
        ))
    }

    fn delete_object(&self, _name: &ObjectName) -> BackendResult<()> {
        Err(BackendError::new(
            BackendErrorKind::Unknown,
            "capability probe should not delete",
        ))
    }

    fn list_prefix(&self, _prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
        Err(BackendError::new(
            BackendErrorKind::Unknown,
            "capability probe should not list",
        ))
    }

    fn object_metadata(&self, _name: &ObjectName) -> BackendResult<BackendMetadata> {
        Err(BackendError::new(
            BackendErrorKind::Unknown,
            "capability probe should not read metadata",
        ))
    }
}

#[derive(Default)]
pub(super) struct StoredWalBackend {
    objects: Mutex<BTreeMap<ObjectName, Vec<u8>>>,
    list_order: Mutex<Option<Vec<ObjectName>>>,
    metadata_sizes: Mutex<BTreeMap<ObjectName, u64>>,
    metadata_failures: Mutex<Vec<ObjectName>>,
    delete_failures: Mutex<Vec<ObjectName>>,
    hide_delete_capability: Mutex<bool>,
    publish_count: Mutex<u64>,
}

impl StoredWalBackend {
    pub(super) fn new() -> Self {
        Self::default()
    }

    pub(super) fn with_object(name: &ObjectName, bytes: &[u8]) -> Self {
        let backend = Self::new();
        backend.write_object(name, bytes).expect("seed WAL object");
        backend
    }

    pub(super) fn set_list_order(&self, objects: Vec<ObjectName>) {
        *self.list_order.lock().expect("list order lock") = Some(objects);
    }

    pub(super) fn set_metadata_size(&self, name: &ObjectName, size: u64) {
        self.metadata_sizes
            .lock()
            .expect("metadata sizes lock")
            .insert(name.clone(), size);
    }

    pub(super) fn fail_metadata_for(&self, name: &ObjectName) {
        self.metadata_failures
            .lock()
            .expect("metadata failures lock")
            .push(name.clone());
    }

    pub(super) fn fail_delete_for(&self, name: &ObjectName) {
        self.delete_failures
            .lock()
            .expect("delete failures lock")
            .push(name.clone());
    }

    // Retention tests need to distinguish "delete is unsupported by contract"
    // from "delete exists but fails while pruning one segment."
    pub(super) fn hide_delete_capability(&self) {
        *self
            .hide_delete_capability
            .lock()
            .expect("hide delete capability lock") = true;
    }

    pub(super) fn listed_objects(&self) -> Vec<ObjectName> {
        self.list_prefix(&ObjectLayout::wal_prefix().expect("WAL prefix"))
            .expect("list WAL objects")
    }

    pub(super) fn publish_count(&self) -> u64 {
        *self.publish_count.lock().expect("publish count lock")
    }
}

impl Backend for StoredWalBackend {
    fn capabilities(&self) -> BackendCapabilities {
        // The stored backend can still execute delete_object while this flag is
        // set. That lets tests prove WalService checks advertised capability
        // before invoking deletion.
        if *self
            .hide_delete_capability
            .lock()
            .expect("hide delete capability lock")
        {
            BackendCapabilities::from_slice(&required_wal_capabilities())
        } else {
            BackendCapabilities::from_slice(&stored_wal_capabilities())
        }
    }

    fn read_object(&self, name: &ObjectName) -> BackendResult<Vec<u8>> {
        self.objects
            .lock()
            .expect("stored WAL objects lock")
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
            .expect("stored WAL objects lock")
            .insert(name.clone(), bytes.to_vec());
        Ok(BackendMetadata::new(bytes.len() as u64, None))
    }

    fn delete_object(&self, name: &ObjectName) -> BackendResult<()> {
        let mut failures = self.delete_failures.lock().expect("delete failures lock");
        if let Some(index) = failures.iter().position(|object| object == name) {
            failures.remove(index);
            return Err(BackendError::new(
                BackendErrorKind::Unavailable,
                "injected delete failure",
            ));
        }

        self.objects
            .lock()
            .expect("stored WAL objects lock")
            .remove(name)
            .map(|_| ())
            .ok_or_else(|| BackendError::new(BackendErrorKind::NotFound, "not found"))
    }

    fn list_prefix(&self, prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
        if let Some(objects) = self.list_order.lock().expect("list order lock").as_ref() {
            return Ok(objects
                .iter()
                .filter(|name| name.as_str().starts_with(prefix.as_str()))
                .cloned()
                .collect());
        }

        Ok(self
            .objects
            .lock()
            .expect("stored WAL objects lock")
            .keys()
            .filter(|name| name.as_str().starts_with(prefix.as_str()))
            .cloned()
            .collect())
    }

    fn object_metadata(&self, name: &ObjectName) -> BackendResult<BackendMetadata> {
        // Metadata faults are one-shot so tests can assert the routed error and
        // still inspect the same object afterward.
        let mut failures = self
            .metadata_failures
            .lock()
            .expect("metadata failures lock");
        if let Some(index) = failures.iter().position(|object| object == name) {
            failures.remove(index);
            return Err(BackendError::new(
                BackendErrorKind::Unavailable,
                "injected metadata failure",
            ));
        }
        drop(failures);

        if let Some(size) = self
            .metadata_sizes
            .lock()
            .expect("metadata sizes lock")
            .get(name)
            .copied()
        {
            return Ok(BackendMetadata::new(size, None));
        }

        self.objects
            .lock()
            .expect("stored WAL objects lock")
            .get(name)
            .map(|bytes| BackendMetadata::new(bytes.len() as u64, None))
            .ok_or_else(|| BackendError::new(BackendErrorKind::NotFound, "not found"))
    }

    fn append_object(&self, name: &ObjectName, bytes: &[u8]) -> BackendResult<BackendAppend> {
        let mut objects = self.objects.lock().expect("stored WAL objects lock");
        let stored = objects
            .get_mut(name)
            .ok_or_else(|| BackendError::new(BackendErrorKind::NotFound, "not found"))?;
        let start_offset = stored.len() as u64;
        stored.extend_from_slice(bytes);
        Ok(BackendAppend::new(
            start_offset,
            bytes.len() as u64,
            BackendMetadata::new(stored.len() as u64, None),
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
        let mut objects = self.objects.lock().expect("stored WAL objects lock");
        if mode == PublishMode::Create && objects.contains_key(name) {
            return Err(crate::backend::PublishError::precondition_failed(
                name,
                "object already exists",
            ));
        }
        objects.insert(name.clone(), bytes.to_vec());
        *self.publish_count.lock().expect("publish count lock") += 1;
        Ok(PublishOutcome::new(
            name.clone(),
            BackendMetadata::new(bytes.len() as u64, None),
            PublishDurability::Durable,
        ))
    }
}
