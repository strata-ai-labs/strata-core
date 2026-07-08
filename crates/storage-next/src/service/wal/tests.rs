use super::{
    WalRetentionProof, WalRetentionProofSource, WalService, WalServiceConfig, WalServiceError,
};
use crate::backend::memory::MemoryBackend;
use crate::backend::{
    Backend, BackendAppend, BackendCapabilities, BackendCapability, BackendError, BackendErrorKind,
    BackendMetadata, BackendRange, BackendResult, PublishDurability, PublishMode, PublishOutcome,
    PublishResult,
};
use crate::config::mode::DurabilityPolicy;
use crate::format::{
    encode_wal_segment_header, WalRecord, WalSegmentHeader, WAL_SEGMENT_HEADER_SIZE,
};
use crate::layout::ObjectLayout;
use crate::object::{ObjectName, ObjectPrefix};
use std::sync::Mutex;
use strata_core_next::CommitVersion;
mod support;
use support::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AppendReportFault {
    BackendFailure,
    WrongStartOffset,
    ShortLength,
    WrongMetadataSize,
}

#[test]
fn wal_retention_proof_records_durable_source() {
    let snapshot = WalRetentionProof::snapshot_watermark(CommitVersion::new(7));
    let flush = WalRetentionProof::flush_watermark(CommitVersion::new(11));

    assert_eq!(snapshot.covered_through(), CommitVersion::new(7));
    assert_eq!(
        snapshot.source(),
        WalRetentionProofSource::SnapshotWatermark
    );
    assert_eq!(flush.covered_through(), CommitVersion::new(11));
    assert_eq!(flush.source(), WalRetentionProofSource::FlushWatermark);
}

#[test]
fn wal_repair_uncertain_is_writer_halted_not_append_uncertain() {
    let error = WalServiceError::RepairUncertain { segment_id: 7 };

    assert!(error.is_writer_halted_append_failure());
    assert!(!error.is_durability_uncertain_append_failure());
}

// This backend writes the bytes but lies about the append report. The WAL
// service must reject the report without advancing its own offset and dirty
// facts, because the durable bytes are now ahead of service state.
struct MisreportingAppendBackend {
    object: Mutex<Option<(ObjectName, Vec<u8>)>>,
    fault: AppendReportFault,
}

impl MisreportingAppendBackend {
    fn new(fault: AppendReportFault) -> Self {
        Self {
            object: Mutex::new(None),
            fault,
        }
    }

    fn stored_metadata(&self, name: &ObjectName) -> BackendResult<BackendMetadata> {
        let object = self.object.lock().expect("backend object lock");
        match object.as_ref() {
            Some((stored_name, bytes)) if stored_name == name => {
                Ok(BackendMetadata::new(bytes.len() as u64, None))
            }
            _ => Err(BackendError::new(BackendErrorKind::NotFound, "not found")),
        }
    }
}

impl Backend for MisreportingAppendBackend {
    fn capabilities(&self) -> BackendCapabilities {
        BackendCapabilities::from_slice(&[
            BackendCapability::ReadObject,
            BackendCapability::ReadRange,
            BackendCapability::ListPrefix,
            BackendCapability::ObjectMetadata,
            BackendCapability::AppendObject,
            BackendCapability::DurablePublish,
            BackendCapability::DurableSync,
        ])
    }

    fn read_object(&self, name: &ObjectName) -> BackendResult<Vec<u8>> {
        let object = self.object.lock().expect("backend object lock");
        match object.as_ref() {
            Some((stored_name, bytes)) if stored_name == name => Ok(bytes.clone()),
            _ => Err(BackendError::new(BackendErrorKind::NotFound, "not found")),
        }
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

    fn write_object(&self, _name: &ObjectName, _bytes: &[u8]) -> BackendResult<BackendMetadata> {
        Err(BackendError::unsupported(BackendCapability::WriteObject))
    }

    fn delete_object(&self, name: &ObjectName) -> crate::backend::DeleteResult {
        crate::backend::failed_delete_result(
            name,
            BackendError::unsupported(BackendCapability::DeleteObject),
        )
    }

    fn list_prefix(&self, prefix: &ObjectPrefix) -> BackendResult<Vec<ObjectName>> {
        let object = self.object.lock().expect("backend object lock");
        match object.as_ref() {
            Some((name, _)) if name.as_str().starts_with(prefix.as_str()) => Ok(vec![name.clone()]),
            _ => Ok(Vec::new()),
        }
    }

    fn object_metadata(&self, name: &ObjectName) -> BackendResult<BackendMetadata> {
        self.stored_metadata(name)
    }

    fn append_object(&self, name: &ObjectName, bytes: &[u8]) -> BackendResult<BackendAppend> {
        if self.fault == AppendReportFault::BackendFailure {
            return Err(BackendError::new(
                BackendErrorKind::Unknown,
                "append fault before bytes are accepted",
            ));
        }

        let mut object = self.object.lock().expect("backend object lock");
        let Some((stored_name, stored_bytes)) = object.as_mut() else {
            return Err(BackendError::new(BackendErrorKind::NotFound, "not found"));
        };
        if stored_name != name {
            return Err(BackendError::new(BackendErrorKind::NotFound, "not found"));
        }

        let start_offset = stored_bytes.len() as u64;
        stored_bytes.extend_from_slice(bytes);
        let actual_size = stored_bytes.len() as u64;
        let actual_len = bytes.len() as u64;

        let (start_offset, bytes_written, metadata_size) = match self.fault {
            AppendReportFault::BackendFailure => unreachable!("handled before append"),
            AppendReportFault::WrongStartOffset => {
                (start_offset.saturating_add(1), actual_len, actual_size)
            }
            AppendReportFault::ShortLength => {
                (start_offset, actual_len.saturating_sub(1), actual_size)
            }
            AppendReportFault::WrongMetadataSize => {
                (start_offset, actual_len, actual_size.saturating_sub(1))
            }
        };
        Ok(BackendAppend::new(
            start_offset,
            bytes_written,
            BackendMetadata::new(metadata_size, None),
        ))
    }

    fn sync_object(&self, _name: &ObjectName) -> crate::backend::BackendResult<()> {
        Ok(())
    }

    fn publish_object(
        &self,
        name: &ObjectName,
        bytes: &[u8],
        _mode: PublishMode,
    ) -> PublishResult<PublishOutcome> {
        let mut object = self.object.lock().expect("backend object lock");
        *object = Some((name.clone(), bytes.to_vec()));
        Ok(PublishOutcome::new(
            name.clone(),
            BackendMetadata::new(bytes.len() as u64, None),
            PublishDurability::Durable,
        ))
    }
}

#[test]
fn memory_backend_cannot_open_durable_wal_service() {
    let backend = MemoryBackend::new();

    let result = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    );

    assert_unsupported_capability(
        &open_error(result, "memory backend should not open durable WAL"),
        BackendCapability::AppendObject,
    );
}

#[test]
fn open_rejects_segment_id_zero() {
    let backend = StoredWalBackend::new();

    let result = WalService::open(
        &backend,
        database_id(),
        0,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    );

    assert_eq!(
        open_error(result, "segment id zero should be rejected"),
        WalServiceError::InvalidSegmentId { segment_id: 0 }
    );
    assert_eq!(backend.publish_count(), 0);
}

#[test]
fn open_rejects_segment_size_below_minimum() {
    let backend = StoredWalBackend::new();

    let result = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::new(1023),
    );

    assert_eq!(
        open_error(result, "too-small segment size should be rejected"),
        WalServiceError::InvalidConfig {
            field: "segment_size"
        }
    );
    assert_eq!(backend.publish_count(), 0);
}

#[test]
fn wal_config_makes_identity_codec_boundary_explicit() {
    let config = WalServiceConfig::new(1024);

    assert_eq!(config.segment_size(), 1024);
    assert_eq!(config.codec_id(), "identity");
    assert_eq!(
        WalServiceConfig::default().codec_id(),
        "identity",
        "V1 WAL service defaults to the required identity storage codec"
    );
}

#[test]
fn open_rejects_non_identity_wal_codec_before_backend_access() {
    let backend = StoredWalBackend::new();

    let result = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::with_codec(1024, "zstd"),
    );

    assert_eq!(
        open_error(result, "non-identity WAL codec should be rejected in V1"),
        WalServiceError::InvalidConfig { field: "codec_id" }
    );
    assert_eq!(backend.publish_count(), 0);
}

#[test]
fn open_rejects_each_missing_required_capability() {
    for missing in required_wal_capabilities() {
        let backend = CapabilityProbeBackend::missing(missing);

        let result = WalService::open(
            &backend,
            database_id(),
            1,
            DurabilityPolicy::Standard,
            WalServiceConfig::default(),
        );

        assert_unsupported_capability(
            &open_error(result, "missing capability should be rejected at open"),
            missing,
        );
    }
}

#[test]
fn open_missing_segment_creates_exactly_one_header_object() {
    let backend = StoredWalBackend::new();
    let object = ObjectLayout::wal_segment(1).expect("WAL segment");

    let service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    )
    .expect("open WAL");

    assert_eq!(service.active_segment_id(), 1);
    assert_eq!(backend.publish_count(), 1);
    assert_eq!(backend.listed_objects(), vec![object.clone()]);
    assert_eq!(
        backend.read_object(&object).expect("segment header bytes"),
        encode_wal_segment_header(&WalSegmentHeader::new(1, database_id()))
    );
}

#[test]
fn open_existing_valid_segment_does_not_rewrite_it() {
    let object = ObjectLayout::wal_segment(1).expect("WAL segment");
    let bytes = segment_bytes(1, &[record(7, b"seed".to_vec())]);
    let backend = StoredWalBackend::with_object(&object, &bytes);

    let service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    )
    .expect("open existing WAL");

    assert_eq!(service.active_segment_id(), 1);
    assert_eq!(service.active_metadata().record_count(), 1);
    assert_eq!(backend.publish_count(), 0);
    assert_eq!(backend.read_object(&object).expect("segment bytes"), bytes);
}

#[test]
fn open_existing_short_or_corrupt_headers_fail_closed() {
    let object = ObjectLayout::wal_segment(1).expect("WAL segment");
    let mut corrupt_magic = encode_wal_segment_header(&WalSegmentHeader::new(1, database_id()));
    corrupt_magic[0] ^= 0xff;
    let mut future_version = encode_wal_segment_header(&WalSegmentHeader::new(1, database_id()));
    future_version[4..8].copy_from_slice(&9_u32.to_le_bytes());
    let mut checksum_mismatch = encode_wal_segment_header(&WalSegmentHeader::new(1, database_id()));
    let last = checksum_mismatch.len() - 1;
    checksum_mismatch[last] ^= 0xff;
    let segment_mismatch = encode_wal_segment_header(&WalSegmentHeader::new(2, database_id()));

    for bytes in [
        Vec::new(),
        vec![0; WAL_SEGMENT_HEADER_SIZE - 1],
        corrupt_magic,
        future_version,
        checksum_mismatch,
        segment_mismatch,
    ] {
        let backend = StoredWalBackend::with_object(&object, &bytes);

        let result = WalService::open(
            &backend,
            database_id(),
            1,
            DurabilityPolicy::Standard,
            WalServiceConfig::default(),
        );

        match open_error(result, "bad header should fail closed") {
            WalServiceError::Format {
                operation,
                object: actual,
                source: _,
            } => {
                assert_eq!(operation, super::WalOperation::Read);
                assert_eq!(actual, object);
            }
            other => panic!("expected header format failure, got {other:?}"),
        }
        assert_eq!(backend.publish_count(), 0);
    }
}

#[test]
fn open_existing_header_for_other_database_fails_with_database_mismatch() {
    let object = ObjectLayout::wal_segment(1).expect("WAL segment");
    let backend = StoredWalBackend::with_object(
        &object,
        &encode_wal_segment_header(&WalSegmentHeader::new(1, other_database_id_for_tests())),
    );

    let result = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    );

    assert_eq!(
        open_error(result, "other database header should be rejected"),
        WalServiceError::DatabaseMismatch {
            object,
            segment_id: 1,
        }
    );
    assert_eq!(backend.publish_count(), 0);
}

#[test]
fn open_metadata_failure_returns_typed_open_backend_error() {
    let object = ObjectLayout::wal_segment(1).expect("WAL segment");
    let backend = StoredWalBackend::new();
    backend.fail_metadata_for(&object);

    let result = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    );

    match open_error(result, "metadata failure should fail open") {
        WalServiceError::Backend {
            operation,
            object: actual,
            source,
        } => {
            assert_eq!(operation, super::WalOperation::Open);
            assert_eq!(actual, object);
            assert_eq!(source.kind(), BackendErrorKind::Unavailable);
        }
        other => panic!("expected open metadata backend error, got {other:?}"),
    }
    assert_eq!(backend.publish_count(), 0);
}

fn other_database_id_for_tests() -> [u8; 16] {
    [
        0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e,
        0x2f,
    ]
}

#[test]
fn read_rejects_invalid_wal_object_names_from_listing() {
    let active = ObjectLayout::wal_segment(1).expect("active segment");
    for invalid in [
        wal_listed_object("not-fixed-width"),
        wal_listed_object("0000000000000001/extra"),
        wal_listed_object("000000000000000A"),
        wal_listed_object("zzzzzzzzzzzzzzzz"),
        wal_listed_object("0000000000000000"),
    ] {
        let backend = StoredWalBackend::with_object(&active, &segment_bytes(1, &[]));
        backend.set_list_order(vec![invalid.clone()]);
        let service = WalService::open(
            &backend,
            database_id(),
            1,
            DurabilityPolicy::Standard,
            WalServiceConfig::default(),
        )
        .expect("open WAL");

        let error = service
            .read_all()
            .expect_err("invalid listed WAL object should fail");

        assert_backend_list_invalid_object(error, &invalid);
    }
}

#[test]
fn read_uses_listed_numeric_segments_and_accepts_gaps() {
    let segment_one = ObjectLayout::wal_segment(1).expect("segment one");
    let segment_three = ObjectLayout::wal_segment(3).expect("segment three");
    let first = record(1, b"first".to_vec());
    let third = record(3, b"third".to_vec());
    let backend = StoredWalBackend::new();
    backend
        .write_object(
            &segment_one,
            &segment_bytes(1, std::slice::from_ref(&first)),
        )
        .expect("seed first segment");
    backend
        .write_object(
            &segment_three,
            &segment_bytes(3, std::slice::from_ref(&third)),
        )
        .expect("seed third segment");
    backend.set_list_order(vec![segment_three, segment_one]);
    let service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    )
    .expect("open WAL");

    let read = service.read_all().expect("read listed WAL segments");

    assert_eq!(read.records(), &[first, third]);
    assert_eq!(read.truncation(), None);
}

#[test]
fn read_ignores_adjacent_non_wal_prefix_objects_when_backend_filters_prefix() {
    let first = record(1, b"first".to_vec());
    let segment_one = ObjectLayout::wal_segment(1).expect("segment one");
    let adjacent = ObjectName::new("walx/0000000000000002").expect("adjacent object");
    let backend = StoredWalBackend::new();
    backend
        .write_object(
            &segment_one,
            &segment_bytes(1, std::slice::from_ref(&first)),
        )
        .expect("seed WAL segment");
    backend
        .write_object(&adjacent, b"not WAL")
        .expect("seed adjacent object");
    let service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    )
    .expect("open WAL");

    let read = service.read_all().expect("read WAL only");

    assert_eq!(backend.listed_objects(), vec![segment_one]);
    assert_eq!(read.records(), &[first]);
    assert_eq!(read.truncation(), None);
}

#[test]
fn rotation_after_max_segment_id_returns_typed_overflow() {
    let object = ObjectLayout::wal_segment(u64::MAX).expect("max segment");
    let backend = StoredWalBackend::with_object(
        &object,
        &segment_bytes(u64::MAX, &[record(1, vec![0x55; 800])]),
    );
    let mut service = WalService::open(
        &backend,
        database_id(),
        u64::MAX,
        DurabilityPolicy::Standard,
        WalServiceConfig::new(1024),
    )
    .expect("open max segment WAL");

    let error = service
        .append(&record(2, vec![0x66; 800]))
        .expect_err("rotation after max segment should overflow");

    assert_eq!(
        error,
        WalServiceError::SegmentIdOverflow {
            segment_id: u64::MAX,
        }
    );
    assert_eq!(service.active_segment_id(), u64::MAX);
}

mod append;
mod coalescing;
mod corruption;
mod durability;
mod fault_windows;
#[cfg(all(feature = "localfs", unix))]
mod localfs;
mod read;
mod retention_reopen;
