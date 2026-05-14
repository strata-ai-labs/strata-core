use super::*;
use crate::backend::Backend;
use crate::format::FormatError;
use crate::service::wal::WalOperation;
use strata_core_next::CommitVersion;

fn wal_segment(segment_id: u64) -> ObjectName {
    ObjectLayout::wal_segment(segment_id).expect("WAL segment")
}

fn read_records(service: &WalService<'_>) -> Vec<WalRecord> {
    service.read_all().expect("read WAL").records().to_vec()
}

fn seed_segment(backend: &StoredWalBackend, segment_id: u64, records: &[WalRecord]) -> ObjectName {
    let object = wal_segment(segment_id);
    backend
        .write_object(&object, &segment_bytes(segment_id, records))
        .expect("seed WAL segment");
    object
}

fn assert_segment_missing(backend: &StoredWalBackend, object: &ObjectName) {
    assert_eq!(
        backend
            .object_metadata(object)
            .expect_err("segment should be deleted")
            .kind(),
        BackendErrorKind::NotFound
    );
}

fn assert_segment_present(backend: &StoredWalBackend, object: &ObjectName) {
    backend.object_metadata(object).expect("segment is present");
}

#[test]
fn retention_deletes_only_covered_old_segments_and_sorts_report() {
    let backend = StoredWalBackend::new();
    let segment_one = seed_segment(&backend, 1, &[record(1, b"covered".to_vec())]);
    let segment_two = seed_segment(
        &backend,
        2,
        &[
            record(2, b"partly covered".to_vec()),
            record(5, b"above watermark".to_vec()),
        ],
    );
    let segment_three = seed_segment(&backend, 3, &[record(3, b"active".to_vec())]);
    let segment_four = seed_segment(&backend, 4, &[record(4, b"newer".to_vec())]);
    let manifest = ObjectLayout::database_manifest().expect("database manifest");
    backend
        .write_object(&manifest, b"not a WAL segment")
        .expect("seed non-WAL object");
    // The backend order is deliberately hostile: retention reports must be
    // ordered by parsed segment id, and non-WAL objects must stay outside the
    // WAL retention decision.
    backend.set_list_order(vec![
        segment_four.clone(),
        segment_two.clone(),
        manifest.clone(),
        segment_three.clone(),
        segment_one.clone(),
    ]);
    let service = WalService::open(
        &backend,
        database_id(),
        3,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    )
    .expect("open WAL");

    let report = service
        .delete_covered_segments(CommitVersion::new(3))
        .expect("delete covered WAL segments");

    assert_eq!(report.deleted_segments(), &[1]);
    assert_eq!(report.protected_segments(), &[2, 3, 4]);
    assert_eq!(report.failed_segments(), &[]);
    assert_segment_missing(&backend, &segment_one);
    assert_segment_present(&backend, &segment_two);
    assert_segment_present(&backend, &segment_three);
    assert_segment_present(&backend, &segment_four);
    assert_eq!(
        backend.read_object(&manifest).expect("manifest bytes"),
        b"not a WAL segment"
    );
}

#[test]
fn retention_records_delete_failure_without_hiding_other_results() {
    let backend = StoredWalBackend::new();
    let segment_one = seed_segment(&backend, 1, &[record(1, b"failed delete".to_vec())]);
    let segment_two = seed_segment(&backend, 2, &[record(2, b"deleted".to_vec())]);
    let segment_three = seed_segment(&backend, 3, &[record(3, b"active".to_vec())]);
    backend.fail_delete_for(&segment_one);
    backend.set_list_order(vec![
        segment_three.clone(),
        segment_two.clone(),
        segment_one.clone(),
    ]);
    let service = WalService::open(
        &backend,
        database_id(),
        3,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    )
    .expect("open WAL");

    let report = service
        .delete_covered_segments(CommitVersion::new(2))
        .expect("delete covered WAL segments");

    assert_eq!(report.deleted_segments(), &[2]);
    assert_eq!(report.protected_segments(), &[3]);
    assert_eq!(report.failed_segments(), &[1]);
    assert_segment_present(&backend, &segment_one);
    assert_segment_missing(&backend, &segment_two);
    assert_segment_present(&backend, &segment_three);
}

#[test]
fn retention_requires_delete_capability_before_listing_or_deleting() {
    let backend = StoredWalBackend::new();
    let segment_one = seed_segment(&backend, 1, &[record(1, b"covered".to_vec())]);
    // The backend can mechanically delete objects; only the advertised
    // capability is hidden. This proves retention checks the backend contract
    // before list/delete work, not just that delete later fails.
    backend.hide_delete_capability();
    let service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    )
    .expect("open WAL without delete capability");

    let error = service
        .delete_covered_segments(CommitVersion::MAX)
        .expect_err("retention should require delete capability");

    assert_unsupported_capability(&error, BackendCapability::DeleteObject);
    assert_segment_present(&backend, &segment_one);
}

#[test]
fn retention_reports_no_covered_segments_without_deleting() {
    let backend = StoredWalBackend::new();
    let segment_one = seed_segment(&backend, 1, &[record(10, b"too new".to_vec())]);
    let segment_two = seed_segment(&backend, 2, &[record(11, b"active".to_vec())]);
    let service = WalService::open(
        &backend,
        database_id(),
        2,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    )
    .expect("open WAL");

    let report = service
        .delete_covered_segments(CommitVersion::new(1))
        .expect("delete covered WAL segments");

    assert_eq!(report.deleted_segments(), &[]);
    assert_eq!(report.protected_segments(), &[1, 2]);
    assert_eq!(report.failed_segments(), &[]);
    assert_segment_present(&backend, &segment_one);
    assert_segment_present(&backend, &segment_two);
}

#[test]
fn reopen_after_standard_close_rebuilds_records_and_active_metadata() {
    let backend = StoredWalBackend::new();
    let first = record(4, b"before close".to_vec());
    let second = record(7, b"after close".to_vec());
    {
        let mut service = WalService::open(
            &backend,
            database_id(),
            1,
            DurabilityPolicy::Standard,
            WalServiceConfig::default(),
        )
        .expect("open WAL");
        service.append(&first).expect("append first");
        service.append(&second).expect("append second");
        service.close().expect("close WAL");
    }

    let reopened = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    )
    .expect("reopen WAL");

    assert_eq!(read_records(&reopened), vec![first.clone(), second.clone()]);
    assert_eq!(reopened.dirty_bytes(), 0);
    assert_eq!(reopened.dirty_records(), 0);
    assert_eq!(reopened.active_metadata().record_count(), 2);
    assert_eq!(
        reopened.active_metadata().min_commit_version(),
        first.commit_version()
    );
    assert_eq!(
        reopened.active_metadata().max_commit_version(),
        second.commit_version()
    );
    assert_eq!(
        reopened.active_metadata().min_timestamp(),
        first.commit_timestamp()
    );
    assert_eq!(
        reopened.active_metadata().max_timestamp(),
        second.commit_timestamp()
    );
}

#[test]
fn reopen_after_dirty_standard_append_reads_visible_record() {
    let backend = StoredWalBackend::new();
    let first = record(1, b"dirty standard".to_vec());
    {
        let mut service = WalService::open(
            &backend,
            database_id(),
            1,
            DurabilityPolicy::Standard,
            WalServiceConfig::default(),
        )
        .expect("open WAL");
        service
            .append(&first)
            .expect("append dirty standard record");
    }

    let reopened = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    )
    .expect("reopen WAL");

    assert_eq!(read_records(&reopened), vec![first]);
    assert_eq!(reopened.dirty_bytes(), 0);
    assert_eq!(reopened.dirty_records(), 0);
    assert_eq!(reopened.active_metadata().record_count(), 1);
}

#[test]
fn reopen_after_always_append_reads_visible_clean_record() {
    let backend = StoredWalBackend::new();
    let first = record(1, b"always".to_vec());
    {
        let mut service = WalService::open(
            &backend,
            database_id(),
            1,
            DurabilityPolicy::Always,
            WalServiceConfig::default(),
        )
        .expect("open WAL");
        let append = service.append(&first).expect("append always record");
        assert!(append.forced_durable());
        assert_eq!(service.dirty_bytes(), 0);
        assert_eq!(service.dirty_records(), 0);
    }

    let reopened = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Always,
        WalServiceConfig::default(),
    )
    .expect("reopen WAL");

    assert_eq!(read_records(&reopened), vec![first]);
    assert_eq!(reopened.dirty_bytes(), 0);
    assert_eq!(reopened.dirty_records(), 0);
}

#[test]
fn reopen_after_rotation_reads_all_segments_and_rebuilds_active_metadata() {
    let backend = StoredWalBackend::new();
    let first = record(1, vec![0x11; 800]);
    let second = record(2, vec![0x22; 800]);
    {
        let mut service = WalService::open(
            &backend,
            database_id(),
            1,
            DurabilityPolicy::Standard,
            WalServiceConfig::new(1024),
        )
        .expect("open WAL");
        service.append(&first).expect("append first");
        let append = service.append(&second).expect("append second");
        assert_eq!(append.segment_id(), 2);
        assert_eq!(service.active_segment_id(), 2);
    }

    let reopened = WalService::open(
        &backend,
        database_id(),
        2,
        DurabilityPolicy::Standard,
        WalServiceConfig::new(1024),
    )
    .expect("reopen rotated WAL");

    assert_eq!(read_records(&reopened), vec![first, second.clone()]);
    assert_eq!(reopened.active_segment_id(), 2);
    assert_eq!(reopened.active_metadata().record_count(), 1);
    assert_eq!(
        reopened.active_metadata().min_commit_version(),
        second.commit_version()
    );
    assert_eq!(
        reopened.active_metadata().max_commit_version(),
        second.commit_version()
    );
}

#[test]
fn reopen_after_latest_partial_tail_reports_truncation_and_refuses_append() {
    let backend = StoredWalBackend::new();
    let segment_one = wal_segment(1);
    let first = record(1, b"partial latest".to_vec());
    {
        let mut service = WalService::open(
            &backend,
            database_id(),
            1,
            DurabilityPolicy::Standard,
            WalServiceConfig::new(1024),
        )
        .expect("open WAL");
        service.append(&first).expect("append first");
    }
    let valid_end = backend
        .object_metadata(&segment_one)
        .expect("segment metadata before partial")
        .size_bytes();
    backend
        .append_object(&segment_one, &[0xff])
        .expect("append partial tail");
    let object_size = valid_end + 1;

    let mut reopened = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::new(1024),
    )
    .expect("reopen latest partial WAL");
    let read = reopened.read_all().expect("read recoverable latest tail");
    assert_eq!(read.records(), std::slice::from_ref(&first));
    let truncation = read.truncation().expect("truncation fact");
    assert_eq!(truncation.segment_id(), 1);
    assert_eq!(truncation.valid_end_offset(), valid_end);
    assert_eq!(truncation.object_size(), object_size);

    // A latest partial tail is recoverable for reads, but appending before the
    // lifecycle layer truncates or repairs it would write after uncertain bytes.
    let same_segment_error = reopened
        .append(&record(2, b"same segment".to_vec()))
        .expect_err("same-segment append should be refused before repair");
    assert_eq!(
        same_segment_error,
        WalServiceError::UnexpectedAppendOffset {
            object: segment_one.clone(),
            expected: valid_end,
            actual: object_size,
        }
    );

    let rotating_record = record(3, vec![0x33; 900]);
    // This assertion keeps the rotation branch honest. Without it, the test can
    // silently collapse into a second same-segment append-refusal case.
    assert!(
        valid_end + record_frame_len(&rotating_record) > 1024,
        "test record must force rotation after the valid prefix"
    );
    let rotation_error = reopened
        .append(&rotating_record)
        .expect_err("rotation append should be refused before repair");
    assert_eq!(
        rotation_error,
        WalServiceError::UnexpectedAppendOffset {
            object: segment_one,
            expected: valid_end,
            actual: object_size,
        }
    );
}

#[test]
fn reopen_after_non_latest_partial_tail_fails_strict_read() {
    let backend = StoredWalBackend::new();
    let segment_one = wal_segment(1);
    let segment_two = seed_segment(&backend, 2, &[]);
    let first = record(1, b"non-latest partial".to_vec());
    backend
        .write_object(
            &segment_one,
            &segment_bytes(1, std::slice::from_ref(&first)),
        )
        .expect("seed first segment");
    backend
        .append_object(&segment_one, &[0xff])
        .expect("append partial tail");
    assert_segment_present(&backend, &segment_two);
    // Only the latest segment may expose a recoverable tail. A partial tail in
    // any older segment means the WAL no longer has a trustworthy prefix.
    let service = WalService::open(
        &backend,
        database_id(),
        2,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    )
    .expect("open newer segment");

    let error = service
        .read_all()
        .expect_err("non-latest partial tail should be corruption");

    match error {
        WalServiceError::Format {
            operation,
            object,
            source,
        } => {
            assert_eq!(operation, WalOperation::Read);
            assert_eq!(object, segment_one);
            assert!(matches!(source, FormatError::InsufficientBytes { .. }));
        }
        other => panic!("expected WAL format error, got {other:?}"),
    }
}

#[test]
fn reopen_after_corrupt_header_fails_strict_open() {
    let segment_one = wal_segment(1);
    let mut bytes = segment_bytes(1, &[]);
    bytes[0] ^= 0xff;
    let backend = StoredWalBackend::with_object(&segment_one, &bytes);

    let result = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    );

    match open_error(result, "corrupt header should fail open") {
        WalServiceError::Format {
            operation,
            object,
            source: _,
        } => {
            assert_eq!(operation, WalOperation::Read);
            assert_eq!(object, segment_one);
        }
        other => panic!("expected WAL format error, got {other:?}"),
    }
}
