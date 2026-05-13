use super::{WalService, WalServiceConfig, WalServiceError};
use crate::backend::memory::MemoryBackend;
use crate::backend::{
    Backend, BackendAppend, BackendCapabilities, BackendCapability, BackendError, BackendErrorKind,
    BackendMetadata, BackendRange, BackendResult, PublishDurability, PublishMode, PublishOutcome,
    PublishResult,
};
use crate::config::mode::DurabilityPolicy;
use crate::format::WalRecord;
use crate::object::{ObjectName, ObjectPrefix};
use std::sync::Mutex;
use strata_core_next::{BranchId, CommitVersion, Timestamp};

fn database_id() -> [u8; 16] {
    [
        0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e,
        0x1f,
    ]
}

#[cfg(all(feature = "localfs", unix))]
fn other_database_id() -> [u8; 16] {
    [
        0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x27, 0x28, 0x29, 0x2a, 0x2b, 0x2c, 0x2d, 0x2e,
        0x2f,
    ]
}

fn branch_id() -> BranchId {
    BranchId::from_bytes([0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15])
}

fn record(version: u64, payload: impl Into<Vec<u8>>) -> WalRecord {
    WalRecord::new(
        CommitVersion::new(version),
        branch_id(),
        Timestamp::from_micros(1_700_000_000_000_000 + version),
        payload,
    )
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AppendReportFault {
    ShortLength,
    WrongMetadataSize,
}

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

    fn delete_object(&self, _name: &ObjectName) -> BackendResult<()> {
        Err(BackendError::unsupported(BackendCapability::DeleteObject))
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

        let (bytes_written, metadata_size) = match self.fault {
            AppendReportFault::ShortLength => (actual_len.saturating_sub(1), actual_size),
            AppendReportFault::WrongMetadataSize => (actual_len, actual_size.saturating_sub(1)),
        };
        Ok(BackendAppend::new(
            start_offset,
            bytes_written,
            BackendMetadata::new(metadata_size, None),
        ))
    }

    fn sync_object(&self, _name: &ObjectName) -> BackendResult<()> {
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

    assert!(matches!(
        result,
        Err(WalServiceError::UnsupportedCapability { .. })
    ));
}

#[test]
fn append_rejects_backend_short_byte_count_report() {
    let backend = MisreportingAppendBackend::new(AppendReportFault::ShortLength);
    let mut service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    )
    .expect("open WAL");

    let error = service
        .append(&record(1, b"short append report".to_vec()))
        .expect_err("short append report should be rejected");

    assert!(matches!(
        error,
        WalServiceError::UnexpectedAppendLength { .. }
    ));
}

#[test]
fn append_rejects_backend_wrong_metadata_size_report() {
    let backend = MisreportingAppendBackend::new(AppendReportFault::WrongMetadataSize);
    let mut service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    )
    .expect("open WAL");

    let error = service
        .append(&record(1, b"wrong size report".to_vec()))
        .expect_err("wrong metadata size should be rejected");

    assert!(matches!(
        error,
        WalServiceError::UnexpectedObjectSize { .. }
    ));
}

#[cfg(all(feature = "localfs", unix))]
mod localfs {
    use super::{database_id, other_database_id, record, WalService, WalServiceConfig};
    use crate::backend::local_fs::LocalFsBackend;
    use crate::backend::Backend;
    use crate::config::mode::DurabilityPolicy;
    use crate::format::{encode_wal_segment_header, WalSegmentHeader};
    use crate::layout::ObjectLayout;
    use crate::service::wal::WalServiceError;
    use strata_core_next::CommitVersion;

    fn backend() -> (tempfile::TempDir, LocalFsBackend) {
        let dir = tempfile::tempdir().expect("tempdir");
        let backend = LocalFsBackend::new(dir.path());
        (dir, backend)
    }

    fn testing_config() -> WalServiceConfig {
        WalServiceConfig::new(1024)
    }

    #[test]
    fn open_missing_segment_creates_v1_header() {
        let (_dir, backend) = backend();

        let service = WalService::open(
            &backend,
            database_id(),
            1,
            DurabilityPolicy::Standard,
            testing_config(),
        )
        .expect("open WAL");

        let object = ObjectLayout::wal_segment(1).expect("wal segment");
        let bytes = backend.read_object(&object).expect("segment bytes");
        assert_eq!(service.active_segment_id(), 1);
        assert_eq!(
            bytes,
            encode_wal_segment_header(&WalSegmentHeader::new(1, database_id()))
        );
    }

    #[test]
    fn append_and_read_round_trips_records() {
        let (_dir, backend) = backend();
        let mut service = WalService::open(
            &backend,
            database_id(),
            1,
            DurabilityPolicy::Standard,
            testing_config(),
        )
        .expect("open WAL");
        let first = record(1, b"first".to_vec());
        let second = record(2, b"second".to_vec());

        let append = service.append(&first).expect("append first");
        service.append(&second).expect("append second");
        let read = service.read_all().expect("read WAL");

        assert_eq!(append.segment_id(), 1);
        assert_eq!(append.start_offset(), 36);
        assert!(append.bytes_written() > 0);
        assert!(!append.forced_durable());
        assert!(append.dirty_bytes() > 0);
        assert_eq!(service.active_metadata().record_count(), 2);
        assert_eq!(read.records(), &[first, second.clone()]);
        assert_eq!(read.truncation(), None);

        let after_first = service
            .read_after_commit_version(CommitVersion::new(1))
            .expect("read after watermark");
        assert_eq!(after_first.records(), &[second]);
    }

    #[test]
    fn open_existing_segment_rebuilds_active_metadata() {
        let (_dir, backend) = backend();
        let first = record(4, b"first".to_vec());
        let second = record(6, b"second".to_vec());
        {
            let mut service = WalService::open(
                &backend,
                database_id(),
                1,
                DurabilityPolicy::Standard,
                testing_config(),
            )
            .expect("open WAL");
            service.append(&first).expect("append first");
            service.append(&second).expect("append second");
            service.close().expect("close");
        }

        let service = WalService::open(
            &backend,
            database_id(),
            1,
            DurabilityPolicy::Standard,
            testing_config(),
        )
        .expect("reopen WAL");
        let metadata = service.active_metadata();

        assert_eq!(metadata.segment_id(), 1);
        assert_eq!(metadata.record_count(), 2);
        assert_eq!(metadata.min_commit_version(), CommitVersion::new(4));
        assert_eq!(metadata.max_commit_version(), CommitVersion::new(6));
        assert_eq!(metadata.min_timestamp(), first.commit_timestamp());
        assert_eq!(metadata.max_timestamp(), second.commit_timestamp());
    }

    #[test]
    fn always_policy_forces_durability_per_append() {
        let (_dir, backend) = backend();
        let mut service = WalService::open(
            &backend,
            database_id(),
            1,
            DurabilityPolicy::Always,
            testing_config(),
        )
        .expect("open WAL");

        let append = service
            .append(&record(1, b"always".to_vec()))
            .expect("append");

        assert!(append.forced_durable());
        assert_eq!(append.dirty_bytes(), 0);
        assert_eq!(service.dirty_bytes(), 0);
        assert_eq!(service.dirty_records(), 0);
    }

    #[test]
    fn standard_policy_force_durable_clears_dirty_facts() {
        let (_dir, backend) = backend();
        let mut service = WalService::open(
            &backend,
            database_id(),
            1,
            DurabilityPolicy::Standard,
            testing_config(),
        )
        .expect("open WAL");

        service
            .append(&record(1, b"standard".to_vec()))
            .expect("append");
        assert!(service.dirty_bytes() > 0);
        assert_eq!(service.dirty_records(), 1);

        service.force_durable().expect("force durable");

        assert_eq!(service.dirty_bytes(), 0);
        assert_eq!(service.dirty_records(), 0);

        service
            .append(&record(2, b"close".to_vec()))
            .expect("append before close");
        service.close().expect("close sync");
        assert_eq!(service.dirty_bytes(), 0);
    }

    #[test]
    fn append_rotates_before_exceeding_segment_size() {
        let (_dir, backend) = backend();
        let mut service = WalService::open(
            &backend,
            database_id(),
            1,
            DurabilityPolicy::Standard,
            testing_config(),
        )
        .expect("open WAL");
        let large = vec![0x55; 800];

        service.append(&record(1, large.clone())).expect("first");
        let second = service.append(&record(2, large)).expect("second");

        assert_eq!(second.segment_id(), 2);
        assert_eq!(service.active_segment_id(), 2);
        assert!(backend
            .read_object(&ObjectLayout::wal_segment(1).expect("first segment"))
            .is_ok());
        assert!(backend
            .read_object(&ObjectLayout::wal_segment(2).expect("second segment"))
            .is_ok());
    }

    #[test]
    fn open_rejects_wrong_segment_id_header() {
        let (_dir, backend) = backend();
        let object = ObjectLayout::wal_segment(1).expect("segment");
        backend
            .write_object(
                &object,
                &encode_wal_segment_header(&WalSegmentHeader::new(2, database_id())),
            )
            .expect("seed bad header");

        let result = WalService::open(
            &backend,
            database_id(),
            1,
            DurabilityPolicy::Standard,
            testing_config(),
        );

        assert!(matches!(result, Err(WalServiceError::Format { .. })));
    }

    #[test]
    fn open_rejects_wrong_database_id_header() {
        let (_dir, backend) = backend();
        let object = ObjectLayout::wal_segment(1).expect("segment");
        backend
            .write_object(
                &object,
                &encode_wal_segment_header(&WalSegmentHeader::new(1, other_database_id())),
            )
            .expect("seed bad header");

        let result = WalService::open(
            &backend,
            database_id(),
            1,
            DurabilityPolicy::Standard,
            testing_config(),
        );

        assert!(matches!(
            result,
            Err(WalServiceError::DatabaseMismatch { .. })
        ));
    }

    #[test]
    fn latest_segment_partial_tail_returns_truncation_fact() {
        let (_dir, backend) = backend();
        let mut service = WalService::open(
            &backend,
            database_id(),
            1,
            DurabilityPolicy::Standard,
            testing_config(),
        )
        .expect("open WAL");
        let first = record(1, b"first".to_vec());
        service.append(&first).expect("append");
        let object = ObjectLayout::wal_segment(1).expect("segment");
        backend
            .append_object(&object, &[0xff])
            .expect("partial tail");

        let read = service.read_all().expect("partial latest is recoverable");

        assert_eq!(read.records(), &[first]);
        let truncation = read.truncation().expect("truncation fact");
        assert_eq!(truncation.segment_id(), 1);
        assert_eq!(truncation.object_size(), truncation.valid_end_offset() + 1);
    }

    #[test]
    fn latest_segment_partial_tail_prevents_blind_append_after_reopen() {
        let (_dir, backend) = backend();
        let mut service = WalService::open(
            &backend,
            database_id(),
            1,
            DurabilityPolicy::Standard,
            testing_config(),
        )
        .expect("open WAL");
        let first = record(1, b"first".to_vec());
        service.append(&first).expect("append");
        let object = ObjectLayout::wal_segment(1).expect("segment");
        backend
            .append_object(&object, &[0xff])
            .expect("partial tail");
        let size_with_partial_tail = backend
            .object_metadata(&object)
            .expect("metadata")
            .size_bytes();

        let mut reopened = WalService::open(
            &backend,
            database_id(),
            1,
            DurabilityPolicy::Standard,
            testing_config(),
        )
        .expect("reopen WAL with recoverable tail");
        let error = reopened
            .append(&record(2, b"must not append".to_vec()))
            .expect_err("partial tail must be repaired before append");

        assert_eq!(reopened.active_metadata().record_count(), 1);
        assert!(matches!(
            error,
            WalServiceError::UnexpectedAppendOffset { .. }
        ));
        assert_eq!(
            backend
                .object_metadata(&object)
                .expect("metadata after refused append")
                .size_bytes(),
            size_with_partial_tail
        );
    }

    #[test]
    fn latest_segment_partial_tail_prevents_rotation_after_reopen() {
        let (_dir, backend) = backend();
        let mut service = WalService::open(
            &backend,
            database_id(),
            1,
            DurabilityPolicy::Standard,
            testing_config(),
        )
        .expect("open WAL");
        service
            .append(&record(1, b"first".to_vec()))
            .expect("append");
        let first_segment = ObjectLayout::wal_segment(1).expect("first segment");
        backend
            .append_object(&first_segment, &[0xff])
            .expect("partial tail");
        let size_with_partial_tail = backend
            .object_metadata(&first_segment)
            .expect("metadata")
            .size_bytes();

        let mut reopened = WalService::open(
            &backend,
            database_id(),
            1,
            DurabilityPolicy::Standard,
            testing_config(),
        )
        .expect("reopen WAL with recoverable tail");
        let error = reopened
            .append(&record(2, vec![0x55; 900]))
            .expect_err("partial tail must be repaired before rotation");

        assert!(matches!(
            error,
            WalServiceError::UnexpectedAppendOffset { .. }
        ));
        assert_eq!(reopened.active_segment_id(), 1);
        assert_eq!(
            backend
                .object_metadata(&first_segment)
                .expect("metadata after refused rotation")
                .size_bytes(),
            size_with_partial_tail
        );
        assert!(backend
            .read_object(&ObjectLayout::wal_segment(2).expect("second segment"))
            .is_err());
    }

    #[test]
    fn non_latest_partial_tail_is_corruption() {
        let (_dir, backend) = backend();
        let mut service = WalService::open(
            &backend,
            database_id(),
            1,
            DurabilityPolicy::Standard,
            testing_config(),
        )
        .expect("open WAL");
        let large = vec![0x55; 800];
        service.append(&record(1, large.clone())).expect("first");
        service.append(&record(2, large)).expect("second rotates");
        let first_object = ObjectLayout::wal_segment(1).expect("first segment");
        backend
            .append_object(&first_object, &[0xff])
            .expect("partial old tail");

        let error = service
            .read_all()
            .expect_err("partial non-latest segment is corruption");

        assert!(matches!(error, WalServiceError::Format { .. }));
    }

    #[test]
    fn mid_segment_corruption_fails_strict_read() {
        let (_dir, backend) = backend();
        let mut service = WalService::open(
            &backend,
            database_id(),
            1,
            DurabilityPolicy::Standard,
            testing_config(),
        )
        .expect("open WAL");
        service
            .append(&record(1, b"corrupt".to_vec()))
            .expect("append");
        let object = ObjectLayout::wal_segment(1).expect("segment");
        let mut bytes = backend.read_object(&object).expect("read");
        bytes[40] ^= 0xff;
        backend
            .write_object(&object, &bytes)
            .expect("replace corrupt");

        let error = service.read_all().expect_err("corruption");

        assert!(matches!(error, WalServiceError::Format { .. }));
    }

    #[test]
    fn active_segment_is_protected_from_deletion() {
        let (_dir, backend) = backend();
        let mut service = WalService::open(
            &backend,
            database_id(),
            1,
            DurabilityPolicy::Standard,
            testing_config(),
        )
        .expect("open WAL");
        service
            .append(&record(1, b"active".to_vec()))
            .expect("append");

        let report = service
            .delete_covered_segments(CommitVersion::MAX)
            .expect("delete covered");

        assert_eq!(report.deleted_segments(), &[]);
        assert_eq!(report.protected_segments(), &[1]);
        assert_eq!(report.failed_segments(), &[]);
        assert!(backend
            .read_object(&ObjectLayout::wal_segment(1).expect("segment"))
            .is_ok());
    }

    #[test]
    fn covered_old_segments_are_deleted_after_rotation() {
        let (_dir, backend) = backend();
        let mut service = WalService::open(
            &backend,
            database_id(),
            1,
            DurabilityPolicy::Standard,
            testing_config(),
        )
        .expect("open WAL");
        let large = vec![0x55; 800];
        service.append(&record(1, large.clone())).expect("first");
        service.append(&record(2, large)).expect("second rotates");

        let report = service
            .delete_covered_segments(CommitVersion::new(1))
            .expect("delete covered");

        assert_eq!(report.deleted_segments(), &[1]);
        assert_eq!(report.protected_segments(), &[2]);
        assert!(backend
            .read_object(&ObjectLayout::wal_segment(1).expect("first segment"))
            .is_err());
        assert!(backend
            .read_object(&ObjectLayout::wal_segment(2).expect("second segment"))
            .is_ok());
    }
}
