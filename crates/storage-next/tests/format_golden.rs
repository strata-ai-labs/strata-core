//! Durable format golden-vector harness entry point.

#![deny(unsafe_code)]

mod common;

#[test]
fn format_golden_harness_has_storage_format_directory() {
    let dir = common::storage_format_goldens_dir();

    assert!(dir.is_dir());
    for file in [
        "internal-key-ordinary.hex",
        "internal-key-zero-user-byte.hex",
        "manifest-identity.hex",
        "segment-metadata-sidecar.hex",
        "snapshot-container-single-section.hex",
        "snapshot-header-identity.hex",
        "snapshot-section-empty.hex",
        "snapshot-watermark-empty.hex",
        "snapshot-watermark-present.hex",
        "storage-row-put.hex",
        "storage-row-tombstone.hex",
        "wal-record-empty.hex",
        "wal-record-envelope.hex",
        "wal-record-payload.hex",
        "wal-segment-header.hex",
    ] {
        assert!(dir.join(file).is_file(), "missing golden vector {file}");
    }
}
