use super::{
    database_id, open_error, other_database_id, record, record_with_frame_len, WalService,
    WalServiceConfig,
};
use crate::backend::local_fs::LocalFsBackend;
use crate::backend::Backend;
use crate::config::mode::DurabilityPolicy;
use crate::format::{encode_wal_segment_header, WalSegmentHeader};
use crate::layout::ObjectLayout;
use crate::service::wal::{WalOperation, WalRetentionProof, WalServiceError};
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

    match open_error(result, "wrong segment id should fail closed") {
        WalServiceError::Format {
            operation,
            object: actual,
            source: _,
        } => {
            assert_eq!(operation, WalOperation::Read);
            assert_eq!(actual, object);
        }
        other => panic!("expected WAL header format failure, got {other:?}"),
    }
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

    assert_eq!(
        open_error(result, "wrong database id should fail closed"),
        WalServiceError::DatabaseMismatch {
            object,
            segment_id: 1,
        }
    );
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
    // Pick a frame that fits an empty segment but cannot fit after the valid
    // prefix. That keeps this on the rotation path without tripping the
    // record-size preflight first.
    let rotating_record = record_with_frame_len(2, 900, 0x55);
    let error = reopened
        .append(&rotating_record)
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
        .delete_covered_segments(WalRetentionProof::snapshot_watermark(CommitVersion::MAX))
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
        .delete_covered_segments(WalRetentionProof::flush_watermark(CommitVersion::new(1)))
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

// Fast path (persistent append descriptor): many appends flow through one held
// descriptor without a per-append sync; the close sync must make them all durable
// and recoverable on reopen.
#[test]
fn fast_path_many_appends_recover_after_reopen() {
    let (_dir, backend) = backend();
    let count: usize = 64;
    {
        let mut service = WalService::open(
            &backend,
            database_id(),
            1,
            DurabilityPolicy::Standard,
            // One segment large enough to hold every record, so all appends share
            // a single held descriptor (no rotation).
            WalServiceConfig::new(64 * 1024),
        )
        .expect("open WAL");
        for index in 0..count {
            service
                .append(&record(
                    index as u64 + 1,
                    format!("rec-{index}").into_bytes(),
                ))
                .expect("append through held descriptor");
        }
        service.close().expect("close syncs the held descriptor");
    }

    let reopened = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::new(64 * 1024),
    )
    .expect("reopen WAL");
    let read = reopened.read_all().expect("read all");
    assert_eq!(read.records().len(), count);
    assert_eq!(read.truncation(), None);
}

// Fast path: rotation inside `append` must sync and release the old segment's held
// descriptor before advancing, so records from both the sealed old segment and the
// new active segment survive a reopen.
#[test]
fn fast_path_rotation_records_recover_after_reopen() {
    let (_dir, backend) = backend();
    let large = vec![0x55; 800];
    let first = record(1, large.clone());
    let second = record(2, large);
    {
        let mut service = WalService::open(
            &backend,
            database_id(),
            1,
            DurabilityPolicy::Standard,
            testing_config(),
        )
        .expect("open WAL");
        service.append(&first).expect("first");
        let rotated = service.append(&second).expect("second rotates");
        assert_eq!(rotated.segment_id(), 2);
        assert_eq!(service.active_segment_id(), 2);
        service.close().expect("close");
    }

    // Reopen at the rotated active segment; `read_all` still scans every segment.
    let reopened = WalService::open(
        &backend,
        database_id(),
        2,
        DurabilityPolicy::Standard,
        testing_config(),
    )
    .expect("reopen at rotated segment");
    let read = reopened.read_all().expect("read all");
    assert_eq!(read.records(), &[first, second]);
    assert_eq!(read.truncation(), None);
}

// Fast path: repairing a partial tail rewrites the active object, so the held
// descriptor must be dropped on repair; the next append re-opens against the
// repaired file and lands exactly at the valid prefix end.
#[test]
fn fast_path_repair_then_append_recovers() {
    let (_dir, backend) = backend();
    let first = record(1, b"first".to_vec());
    {
        let mut service = WalService::open(
            &backend,
            database_id(),
            1,
            DurabilityPolicy::Standard,
            testing_config(),
        )
        .expect("open WAL");
        service.append(&first).expect("append");
        service.close().expect("close");
    }
    let object = ObjectLayout::wal_segment(1).expect("segment");
    let valid_end = backend
        .object_metadata(&object)
        .expect("metadata before tail")
        .size_bytes();
    backend
        .append_object(&object, &[0xff, 0xee])
        .expect("partial tail");

    let mut reopened = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        testing_config(),
    )
    .expect("reopen with recoverable tail");
    let read = reopened.read_all().expect("read recoverable tail");
    let truncation = read.truncation().expect("truncation fact").clone();
    drop(read);
    reopened
        .repair_latest_tail(&truncation)
        .expect("repair latest tail");

    let second = record(2, b"after repair".to_vec());
    let append = reopened.append(&second).expect("append after repair");
    assert_eq!(append.start_offset(), valid_end);

    let read = reopened.read_all().expect("read repaired WAL");
    assert_eq!(read.records(), &[first, second]);
    assert_eq!(read.truncation(), None);
}
