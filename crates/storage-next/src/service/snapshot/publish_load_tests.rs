use super::{SnapshotPublishRequest, SnapshotService, SnapshotServiceError};
use crate::backend::{
    Backend, BackendCapabilities, BackendCapability, BackendError, BackendErrorKind,
    BackendMetadata, BackendRange, BackendResult, PublishDurability, PublishError,
    PublishFailureKind, PublishMode, PublishOutcome,
};
use crate::format::{
    encode_snapshot_container, FormatError, SnapshotContainer, SnapshotHeader, SnapshotSection,
};
use crate::layout::ObjectLayout;
use crate::object::{ObjectName, ObjectPrefix};
use std::collections::BTreeMap;
use std::sync::Mutex;
use strata_core_next::{CommitVersion, Timestamp};

const DATABASE_ID: [u8; 16] = [0x31; 16];
const SNAPSHOT_ID: u64 = 17;
const SNAPSHOT_WATERMARK: CommitVersion = CommitVersion::new(42);
const SNAPSHOT_CREATED_AT: Timestamp = Timestamp::from_micros(1_900);
const CODEC_ID: &str = "identity";
const SNAPSHOT_VERSION_OFFSET: usize = 4;
const SNAPSHOT_HEADER_SIZE: usize = 64;
const SNAPSHOT_SECTION_HEADER_SIZE: usize = 9;
const SNAPSHOT_FOOTER_SIZE: usize = 4;

#[derive(Debug, Default)]
struct StoredObjectBackend {
    objects: Mutex<BTreeMap<ObjectName, Vec<u8>>>,
    publish_calls: Mutex<u64>,
}

impl StoredObjectBackend {
    fn stored_bytes(&self, object: &ObjectName) -> Vec<u8> {
        self.objects
            .lock()
            .expect("object lock")
            .get(object)
            .expect("stored object")
            .clone()
    }

    fn stored_object_names(&self) -> Vec<ObjectName> {
        self.objects
            .lock()
            .expect("object lock")
            .keys()
            .cloned()
            .collect()
    }

    fn is_empty(&self) -> bool {
        self.objects.lock().expect("object lock").is_empty()
    }

    fn publish_calls(&self) -> u64 {
        *self.publish_calls.lock().expect("publish lock")
    }
}

impl Backend for StoredObjectBackend {
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
        self.objects
            .lock()
            .expect("object lock")
            .get(name)
            .cloned()
            .ok_or_else(|| BackendError::new(BackendErrorKind::NotFound, "not found"))
    }

    fn read_range(&self, name: &ObjectName, range: BackendRange) -> BackendResult<Vec<u8>> {
        let bytes = self.read_object(name)?;
        let start = usize::try_from(range.offset())
            .map_err(|_| BackendError::new(BackendErrorKind::InvalidRange, "range overflow"))?;
        let end =
            usize::try_from(range.end_offset().ok_or_else(|| {
                BackendError::new(BackendErrorKind::InvalidRange, "range overflow")
            })?)
            .map_err(|_| BackendError::new(BackendErrorKind::InvalidRange, "range overflow"))?;
        if start > bytes.len() {
            return Ok(Vec::new());
        }
        Ok(bytes[start..bytes.len().min(end)].to_vec())
    }

    fn write_object(&self, name: &ObjectName, bytes: &[u8]) -> BackendResult<BackendMetadata> {
        self.objects
            .lock()
            .expect("object lock")
            .insert(name.clone(), bytes.to_vec());
        Ok(BackendMetadata::new(bytes.len() as u64, None))
    }

    fn delete_object(&self, name: &ObjectName) -> BackendResult<()> {
        self.objects
            .lock()
            .expect("object lock")
            .remove(name)
            .map(|_| ())
            .ok_or_else(|| BackendError::new(BackendErrorKind::NotFound, "not found"))
    }

    fn list_prefix(&self, prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
        Ok(self
            .objects
            .lock()
            .expect("object lock")
            .keys()
            .filter(|name| name.as_str().starts_with(prefix.as_str()))
            .cloned()
            .collect())
    }

    fn object_metadata(&self, name: &ObjectName) -> BackendResult<BackendMetadata> {
        self.objects
            .lock()
            .expect("object lock")
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
        *self.publish_calls.lock().expect("publish lock") += 1;
        let mut objects = self.objects.lock().expect("object lock");
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

struct MissingCapabilityBackend {
    missing: BackendCapability,
}

impl MissingCapabilityBackend {
    const fn new(missing: BackendCapability) -> Self {
        Self { missing }
    }
}

impl Backend for MissingCapabilityBackend {
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::from_slice(
            &[
                BackendCapability::ReadObject,
                BackendCapability::ReadRange,
                BackendCapability::WriteObject,
                BackendCapability::DeleteObject,
                BackendCapability::ListPrefix,
                BackendCapability::ObjectMetadata,
                BackendCapability::DurablePublish,
                BackendCapability::DurableSync,
            ]
            .into_iter()
            .filter(|capability| *capability != self.missing)
            .collect::<Vec<_>>(),
        )
    }

    fn read_object(&self, _name: &ObjectName) -> BackendResult<Vec<u8>> {
        // The tests using this backend assert service-level capability
        // preflights. Reaching the backend method means the service already
        // crossed the boundary it was supposed to guard.
        panic!("read_object should be preflighted when capability is missing")
    }

    fn read_range(&self, _name: &ObjectName, _range: BackendRange) -> BackendResult<Vec<u8>> {
        panic!("read_range is not used by snapshot publish/load tests")
    }

    fn write_object(&self, _name: &ObjectName, _bytes: &[u8]) -> BackendResult<BackendMetadata> {
        panic!("write_object is not used by snapshot publish/load tests")
    }

    fn delete_object(&self, _name: &ObjectName) -> BackendResult<()> {
        panic!("delete_object is not used by snapshot publish/load tests")
    }

    fn list_prefix(&self, _prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
        panic!("list_prefix is not used by snapshot publish/load tests")
    }

    fn object_metadata(&self, _name: &ObjectName) -> BackendResult<BackendMetadata> {
        panic!("object_metadata is not used by snapshot publish/load tests")
    }

    fn publish_object(
        &self,
        _name: &ObjectName,
        _bytes: &[u8],
        _mode: PublishMode,
    ) -> Result<PublishOutcome, PublishError> {
        // Durable snapshot publication must be refused by ObjectPublisher
        // before a backend without publish/sync capability can observe bytes.
        panic!("publish_object should be preflighted when durability capability is missing")
    }
}

struct ReadFailureBackend {
    source: BackendError,
}

impl Backend for ReadFailureBackend {
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::from_slice(&[
            BackendCapability::ReadObject,
            BackendCapability::ReadRange,
            BackendCapability::WriteObject,
            BackendCapability::DeleteObject,
            BackendCapability::ListPrefix,
            BackendCapability::ObjectMetadata,
        ])
    }

    fn read_object(&self, _name: &ObjectName) -> BackendResult<Vec<u8>> {
        Err(self.source.clone())
    }

    fn read_range(&self, _name: &ObjectName, _range: BackendRange) -> BackendResult<Vec<u8>> {
        Err(BackendError::unsupported(BackendCapability::ReadRange))
    }

    fn write_object(&self, _name: &ObjectName, _bytes: &[u8]) -> BackendResult<BackendMetadata> {
        Err(BackendError::unsupported(BackendCapability::WriteObject))
    }

    fn delete_object(&self, _name: &ObjectName) -> BackendResult<()> {
        Err(BackendError::unsupported(BackendCapability::DeleteObject))
    }

    fn list_prefix(&self, _prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
        Err(BackendError::unsupported(BackendCapability::ListPrefix))
    }

    fn object_metadata(&self, _name: &ObjectName) -> BackendResult<BackendMetadata> {
        Err(BackendError::unsupported(BackendCapability::ObjectMetadata))
    }
}

#[test]
fn snapshot_load_requires_read_capability_before_backend_access() {
    let backend = MissingCapabilityBackend::new(BackendCapability::ReadObject);
    let service = SnapshotService::new(&backend);

    assert_eq!(
        service.load_optional(SNAPSHOT_ID),
        Err(SnapshotServiceError::UnsupportedCapability {
            capability: BackendCapability::ReadObject,
        })
    );
}

#[test]
fn snapshot_publish_rejects_each_missing_durability_capability_before_publish() {
    for missing in [
        BackendCapability::DurablePublish,
        BackendCapability::DurableSync,
    ] {
        let backend = MissingCapabilityBackend::new(missing);
        let service = SnapshotService::new(&backend);
        let object = ObjectLayout::snapshot(SNAPSHOT_ID).expect("snapshot object");

        match service
            .publish_create(request(CODEC_ID, sections()))
            .expect_err("missing durability capability should fail")
        {
            SnapshotServiceError::Publish {
                snapshot_id,
                source,
            } => {
                assert_eq!(snapshot_id, SNAPSHOT_ID);
                assert_eq!(source.kind(), PublishFailureKind::Unsupported);
                assert_eq!(source.object(), &object);
                assert_eq!(
                    source.source_error().kind(),
                    BackendErrorKind::UnsupportedOperation
                );
            }
            other => panic!("expected publish error, got {other:?}"),
        }
    }
}

#[test]
fn snapshot_publish_rejects_invalid_codec_ids_before_backend_access() {
    let cases = [
        (
            String::new(),
            FormatError::InvalidLength { field: "codec_id" },
        ),
        (
            "x".repeat(256),
            FormatError::InvalidLength { field: "codec_id" },
        ),
        (
            "identity\0zstd".to_string(),
            FormatError::InvalidUtf8 { field: "codec_id" },
        ),
    ];

    for (codec_id, expected_source) in cases {
        let backend = StoredObjectBackend::default();
        let service = SnapshotService::new(&backend);
        let object = ObjectLayout::snapshot(SNAPSHOT_ID).expect("snapshot object");

        assert_eq!(
            service.publish_create(request(&codec_id, sections())),
            Err(SnapshotServiceError::Encode {
                object,
                snapshot_id: SNAPSHOT_ID,
                source: expected_source,
            })
        );
        assert!(backend.is_empty());
        assert_eq!(backend.publish_calls(), 0);
    }
}

#[test]
fn snapshot_section_kind_zero_is_unconstructible_before_publish() {
    assert_eq!(
        SnapshotSection::new(0, b"rows".to_vec()),
        Err(FormatError::InvalidValue {
            field: "snapshot_section_kind",
        })
    );
}

#[test]
fn snapshot_publish_returns_layout_metadata_and_round_trippable_sections() {
    let backend = StoredObjectBackend::default();
    let service = SnapshotService::new(&backend);
    let expected_object = ObjectLayout::snapshot(SNAPSHOT_ID).expect("snapshot object");

    let write = service
        .publish_create(request(
            CODEC_ID,
            vec![
                SnapshotSection::new(0x01, b"rows".to_vec()).expect("section"),
                SnapshotSection::new(0x02, b"\x00opaque\xff".to_vec()).expect("section"),
            ],
        ))
        .expect("publish snapshot");
    let stored_bytes = backend.stored_bytes(&expected_object);
    let loaded = service
        .load_required_for_codec(SNAPSHOT_ID, DATABASE_ID, CODEC_ID)
        .expect("load snapshot");
    let (container, outcome) = &write;

    assert_eq!(container, &loaded);
    assert_eq!(outcome.object(), &expected_object);
    assert_eq!(outcome.metadata().size_bytes(), stored_bytes.len() as u64);
    assert_eq!(outcome.durability(), PublishDurability::Durable);
    assert_eq!(container.header().snapshot_id(), SNAPSHOT_ID);
    assert_eq!(
        container.header().watermark_commit_version(),
        SNAPSHOT_WATERMARK
    );
    assert_eq!(container.header().created_at(), SNAPSHOT_CREATED_AT);
    assert_eq!(container.sections().len(), 2);
    assert_eq!(container.sections()[0].payload(), b"rows");
    assert_eq!(container.sections()[1].payload(), b"\x00opaque\xff");
    assert_eq!(backend.stored_object_names(), vec![expected_object]);
    assert_eq!(backend.publish_calls(), 1);
}

#[test]
fn snapshot_load_maps_backend_read_failure_without_decoding() {
    let source = BackendError::new(BackendErrorKind::Unavailable, "read unavailable");
    let backend = ReadFailureBackend {
        source: source.clone(),
    };
    let service = SnapshotService::new(&backend);
    let object = ObjectLayout::snapshot(SNAPSHOT_ID).expect("snapshot object");

    assert_eq!(
        service.load_required(SNAPSHOT_ID),
        Err(SnapshotServiceError::Read {
            object,
            snapshot_id: SNAPSHOT_ID,
            source,
        })
    );
}

#[test]
fn snapshot_load_rejects_version_and_checksum_corruption_at_service_boundary() {
    let cases = [
        (
            corrupt_header_version(99),
            FormatError::FutureFormat {
                format: "snapshot_header",
                version: 99,
                max_supported: 1,
            },
        ),
        (
            corrupt_header_version(2),
            FormatError::PreV1Format {
                format: "snapshot_header",
                version: 2,
            },
        ),
        (
            corrupt_container_crc(),
            FormatError::ChecksumMismatch {
                format: "snapshot_container",
                expected: 0,
                computed: 0,
            },
        ),
    ];

    for (bytes, expected_source) in cases {
        let error = load_error_for_bytes(&bytes);
        match (error, expected_source) {
            (
                SnapshotServiceError::Decode {
                    object: _,
                    snapshot_id: SNAPSHOT_ID,
                    source:
                        FormatError::FutureFormat {
                            format,
                            version,
                            max_supported,
                        },
                },
                FormatError::FutureFormat {
                    format: expected_format,
                    version: expected_version,
                    max_supported: expected_max_supported,
                },
            ) => {
                assert_eq!(format, expected_format);
                assert_eq!(version, expected_version);
                assert_eq!(max_supported, expected_max_supported);
            }
            (
                SnapshotServiceError::Decode {
                    object: _,
                    snapshot_id: SNAPSHOT_ID,
                    source: FormatError::PreV1Format { format, version },
                },
                FormatError::PreV1Format {
                    format: expected_format,
                    version: expected_version,
                },
            ) => {
                assert_eq!(format, expected_format);
                assert_eq!(version, expected_version);
            }
            (
                SnapshotServiceError::Decode {
                    object: _,
                    snapshot_id: SNAPSHOT_ID,
                    source: FormatError::ChecksumMismatch { format, .. },
                },
                FormatError::ChecksumMismatch {
                    format: expected_format,
                    ..
                },
            ) => assert_eq!(format, expected_format),
            (other, expected) => panic!("expected {expected:?}, got {other:?}"),
        }
    }
}

#[test]
fn snapshot_load_rejects_truncated_container_shapes_at_service_boundary() {
    let cases = [
        (valid_snapshot_bytes()[..10].to_vec(), "short header"),
        (valid_snapshot_bytes()[..69].to_vec(), "truncated codec id"),
        (partial_section_header_bytes(), "partial section header"),
        (
            truncated_section_payload_bytes(),
            "truncated section payload",
        ),
        (
            trailing_partial_section_after_complete_section_bytes(),
            "trailing partial section after complete section",
        ),
    ];

    for (bytes, context) in cases {
        assert!(
            matches!(
                load_error_for_bytes(&bytes),
                SnapshotServiceError::Decode {
                    object: _,
                    snapshot_id: SNAPSHOT_ID,
                    source: FormatError::InsufficientBytes { .. }
                        | FormatError::TrailingData { .. }
                }
            ),
            "{context} should return a typed decode error"
        );
    }
}

#[test]
fn snapshot_load_rejects_invalid_section_kind_with_valid_container_crc() {
    let mut bytes = valid_snapshot_bytes();
    let section_start = section_start_offset();
    bytes[section_start] = 0;
    refresh_container_crc(&mut bytes);

    assert!(matches!(
        load_error_for_bytes(&bytes),
        SnapshotServiceError::Decode {
            object: _,
            snapshot_id: SNAPSHOT_ID,
            source: FormatError::InvalidValue {
                field: "snapshot_section_kind"
            },
        }
    ));
}

fn request(codec_id: &str, sections: Vec<SnapshotSection>) -> SnapshotPublishRequest {
    SnapshotPublishRequest::new(
        SNAPSHOT_ID,
        SNAPSHOT_WATERMARK,
        SNAPSHOT_CREATED_AT,
        DATABASE_ID,
        codec_id,
        sections,
    )
}

fn sections() -> Vec<SnapshotSection> {
    vec![SnapshotSection::new(0x01, b"rows".to_vec()).expect("section")]
}

fn container() -> SnapshotContainer {
    SnapshotContainer::new(
        SnapshotHeader::new(
            SNAPSHOT_ID,
            SNAPSHOT_WATERMARK,
            SNAPSHOT_CREATED_AT,
            DATABASE_ID,
            CODEC_ID,
        )
        .expect("snapshot header"),
        sections(),
    )
}

fn valid_snapshot_bytes() -> Vec<u8> {
    encode_snapshot_container(&container()).expect("encode snapshot")
}

fn load_error_for_bytes(bytes: &[u8]) -> SnapshotServiceError {
    let backend = StoredObjectBackend::default();
    let object = ObjectLayout::snapshot(SNAPSHOT_ID).expect("snapshot object");
    backend
        .write_object(&object, bytes)
        .expect("write snapshot");
    SnapshotService::new(&backend)
        .load_required(SNAPSHOT_ID)
        .expect_err("snapshot bytes should be rejected")
}

fn corrupt_header_version(version: u32) -> Vec<u8> {
    let mut bytes = valid_snapshot_bytes();
    // Header version is decoded before the container footer CRC is checked, so
    // this mutation does not refresh the footer. That keeps the test pinned to
    // the version-routing branch rather than the checksum branch.
    bytes[SNAPSHOT_VERSION_OFFSET..SNAPSHOT_VERSION_OFFSET + 4]
        .copy_from_slice(&version.to_le_bytes());
    bytes
}

fn corrupt_container_crc() -> Vec<u8> {
    let mut bytes = valid_snapshot_bytes();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xff;
    bytes
}

fn partial_section_header_bytes() -> Vec<u8> {
    let mut bytes = valid_snapshot_bytes()[..section_start_offset() + 2].to_vec();
    // Refresh the footer after creating the partial section. Without this, the
    // visitor would stop at the container checksum and never prove section
    // boundary validation.
    append_container_crc(&mut bytes);
    bytes
}

fn truncated_section_payload_bytes() -> Vec<u8> {
    let mut bytes = valid_snapshot_bytes()
        [..section_start_offset() + SNAPSHOT_SECTION_HEADER_SIZE + 2]
        .to_vec();
    // Same CRC refresh as the partial-header case: the payload truncation must
    // be the first failure the service observes.
    append_container_crc(&mut bytes);
    bytes
}

fn trailing_partial_section_after_complete_section_bytes() -> Vec<u8> {
    let mut bytes = valid_snapshot_bytes();
    bytes.truncate(bytes.len() - SNAPSHOT_FOOTER_SIZE);
    // Keep the first section complete, then add too few bytes for another
    // section header. The footer is refreshed so the service reaches section
    // boundary validation instead of stopping at the container checksum.
    bytes.extend_from_slice(&[0x02, 0x00, 0x00]);
    append_container_crc(&mut bytes);
    bytes
}

fn section_start_offset() -> usize {
    SNAPSHOT_HEADER_SIZE + CODEC_ID.len()
}

fn append_container_crc(bytes: &mut Vec<u8>) {
    let crc = crc32fast::hash(bytes);
    bytes.extend_from_slice(&crc.to_le_bytes());
}

fn refresh_container_crc(bytes: &mut [u8]) {
    let footer_offset = bytes.len() - SNAPSHOT_FOOTER_SIZE;
    let crc = crc32fast::hash(&bytes[..footer_offset]);
    bytes[footer_offset..].copy_from_slice(&crc.to_le_bytes());
}
