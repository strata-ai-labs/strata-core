use super::{
    metadata, sidecar_object, RecordingBackend, WalSegmentMetadataSidecarError,
    WalSegmentMetadataSidecarLoad, WalSegmentMetadataSidecarService,
};
use crate::backend::{Backend, BackendError, BackendErrorKind, PublishFailureKind};
use crate::format::{decode_manifest, encode_segment_metadata, FormatError};
use crate::layout::ObjectLayout;
use crate::service::DatabaseManifestService;
use strata_core::CommitVersion;

const DATABASE_ID: [u8; 16] = [0x9d; 16];
const CODEC_ID: &str = "identity";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CorruptSource {
    InvalidMagic,
    FutureFormat,
    PreV1Format,
    ChecksumMismatch,
    TrailingData,
}

#[test]
fn sidecar_object_name_uses_metadata_namespace() {
    assert_eq!(
        sidecar_object(0x2a),
        ObjectLayout::wal_segment_metadata(0x2a).expect("metadata object")
    );
}

#[test]
fn zero_segment_id_is_rejected_without_backend_access() {
    let backend = RecordingBackend::new();
    backend.set_read_failure(BackendError::new(
        BackendErrorKind::Unavailable,
        "backend must not be read",
    ));
    let service = WalSegmentMetadataSidecarService::new(&backend);

    assert_eq!(
        service.load(0),
        Err(WalSegmentMetadataSidecarError::InvalidSegmentId { segment_id: 0 })
    );
    assert!(backend.publish_modes().is_empty());
}

#[test]
fn corrupt_sidecar_fallback_preserves_decode_reason() {
    let cases = [
        (
            CorruptSource::InvalidMagic,
            corrupt_magic(encode_segment_metadata(&metadata(12))),
        ),
        (
            CorruptSource::FutureFormat,
            corrupt_future_version(encode_segment_metadata(&metadata(12))),
        ),
        (
            CorruptSource::PreV1Format,
            corrupt_pre_v1_version(encode_segment_metadata(&metadata(12))),
        ),
        (
            CorruptSource::ChecksumMismatch,
            corrupt_crc(encode_segment_metadata(&metadata(12))),
        ),
        (
            CorruptSource::TrailingData,
            corrupt_trailing(encode_segment_metadata(&metadata(12))),
        ),
    ];

    for (expected, bytes) in cases {
        let backend = RecordingBackend::new();
        backend.insert(sidecar_object(12), bytes);
        let service = WalSegmentMetadataSidecarService::new(&backend);

        let loaded = service.load(12).expect("corrupt sidecar is recoverable");

        assert!(matches!(
            loaded,
            WalSegmentMetadataSidecarLoad::Corrupt {
                segment_id: 12,
                ref object,
                ref source,
            } if object == &sidecar_object(12) && corrupt_source_matches(source, expected)
        ));
    }
}

#[test]
fn missing_or_corrupt_sidecar_does_not_mutate_authoritative_wal() {
    let wal_object = ObjectLayout::wal_segment(12).expect("wal segment object");
    let wal_bytes = b"authoritative wal bytes".to_vec();

    let missing_backend = RecordingBackend::new();
    missing_backend.insert(wal_object.clone(), wal_bytes.clone());
    let missing = WalSegmentMetadataSidecarService::new(&missing_backend)
        .load(12)
        .expect("missing sidecar is recoverable");
    assert!(missing.is_missing());
    assert_eq!(
        missing_backend.read_stored(&wal_object),
        Some(wal_bytes.clone())
    );

    let corrupt_backend = RecordingBackend::new();
    corrupt_backend.insert(wal_object.clone(), wal_bytes.clone());
    corrupt_backend.insert(
        sidecar_object(12),
        corrupt_crc(encode_segment_metadata(&metadata(12))),
    );
    let corrupt = WalSegmentMetadataSidecarService::new(&corrupt_backend)
        .load(12)
        .expect("corrupt sidecar is recoverable");
    assert!(corrupt.is_corrupt());
    assert_eq!(corrupt_backend.read_stored(&wal_object), Some(wal_bytes));
}

#[test]
fn sidecar_publish_uncertainty_does_not_mutate_authoritative_recovery_facts() {
    let cases = [
        (PublishFailureKind::VisibilityUnknown, false),
        (PublishFailureKind::VisibleDurabilityUnconfirmed, true),
    ];

    for (kind, sidecar_visible) in cases {
        let backend = RecordingBackend::new();
        let wal_object = ObjectLayout::wal_segment(13).expect("wal segment object");
        let wal_bytes = b"stable wal bytes".to_vec();
        backend.insert(wal_object.clone(), wal_bytes.clone());
        DatabaseManifestService::new(&backend)
            .create_initial(DATABASE_ID, CODEC_ID)
            .expect("create manifest");
        DatabaseManifestService::new(&backend)
            .persist_snapshot_facts(5, CommitVersion::new(9))
            .expect("seed snapshot facts");
        let manifest_object = ObjectLayout::database_manifest().expect("manifest object");
        let manifest_before = backend
            .read_object(&manifest_object)
            .expect("manifest bytes before sidecar failure");
        backend.set_publish_failure(kind);
        let service = WalSegmentMetadataSidecarService::new(&backend);

        let error = service
            .publish_replace(&metadata(13))
            .expect_err("sidecar publish uncertainty remains a sidecar error");

        assert!(matches!(
            error,
            WalSegmentMetadataSidecarError::Publish { ref source, .. }
                if source.kind() == kind
        ));
        assert_eq!(backend.read_stored(&wal_object), Some(wal_bytes));
        assert_eq!(
            backend
                .read_object(&manifest_object)
                .expect("manifest bytes after sidecar failure"),
            manifest_before
        );
        let manifest = decode_manifest(&manifest_before).expect("manifest remains decodable");
        assert_eq!(manifest.snapshot_id(), Some(5));
        assert_eq!(manifest.snapshot_watermark(), Some(9));

        let loaded = WalSegmentMetadataSidecarService::new(&backend)
            .load(13)
            .expect("sidecar publish uncertainty remains recoverable");
        if sidecar_visible {
            assert!(matches!(
                loaded,
                WalSegmentMetadataSidecarLoad::Present(sidecar)
                    if sidecar.metadata() == &metadata(13)
            ));
        } else {
            assert_eq!(
                loaded,
                WalSegmentMetadataSidecarLoad::Missing {
                    segment_id: 13,
                    object: sidecar_object(13)
                }
            );
        }
    }
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

fn corrupt_source_matches(source: &FormatError, expected: CorruptSource) -> bool {
    matches!(
        (source, expected),
        (
            FormatError::InvalidMagic {
                format: "segment_metadata",
            },
            CorruptSource::InvalidMagic,
        ) | (
            FormatError::FutureFormat {
                format: "segment_metadata",
                version: 2,
                max_supported: 1,
            },
            CorruptSource::FutureFormat,
        ) | (
            FormatError::PreV1Format {
                format: "segment_metadata",
                version: 0,
            },
            CorruptSource::PreV1Format,
        ) | (
            FormatError::ChecksumMismatch {
                format: "segment_metadata",
                ..
            },
            CorruptSource::ChecksumMismatch,
        ) | (
            FormatError::TrailingData {
                format: "segment_metadata",
                remaining: 1,
            },
            CorruptSource::TrailingData,
        )
    )
}
