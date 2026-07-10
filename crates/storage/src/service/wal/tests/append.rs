use super::*;
use crate::service::wal::WalOperation;

#[cfg(not(target_arch = "wasm32"))]
use proptest::collection::vec;
#[cfg(not(target_arch = "wasm32"))]
use proptest::prelude::{any, Strategy};
#[cfg(not(target_arch = "wasm32"))]
use proptest::prop_assert_eq;
#[cfg(not(target_arch = "wasm32"))]
use proptest::test_runner::{
    Config, FileFailurePersistence, TestCaseError, TestCaseResult, TestRunner,
};

#[test]
fn empty_row_value_record_appends_and_reads() {
    let backend = StoredWalBackend::new();
    let mut service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    )
    .expect("open WAL");
    let record = record(1, Vec::new());
    let frame_len = record_frame_len(&record);

    let append = service
        .append(&record)
        .expect("append empty row-value record");
    let read = service.read_all().expect("read empty row-value record");

    assert_eq!(append.segment_id(), 1);
    assert_eq!(append.start_offset(), WAL_SEGMENT_HEADER_SIZE as u64);
    assert_eq!(append.bytes_written(), frame_len);
    assert_eq!(read.records(), std::slice::from_ref(&record));
    assert_eq!(read.truncation(), None);
}

#[cfg(feature = "perf-trace")]
#[test]
fn append_records_wal_encode_buffer_reuse_counters() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let backend = StoredWalBackend::new();
    let mut service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    )
    .expect("open WAL");
    let first = record(1, b"first".to_vec());
    let second = record(2, b"second".to_vec());

    service.append(&first).expect("append first record");
    service.append(&second).expect("append second record");

    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.commit_wal_records_built(), 0);
    assert_eq!(perf.commit_wal_appends(), 0);
    assert!(perf.commit_wal_record_bytes() > 0);
    assert!(perf.commit_wal_payload_bytes() > 0);
    assert!(perf.commit_wal_row_encode_bytes() > 0);
    assert!(perf.commit_wal_record_bytes() > perf.commit_wal_payload_bytes());
    assert!(perf.commit_wal_payload_bytes() > perf.commit_wal_row_encode_bytes());
    assert!(
        perf.commit_wal_encode_buffer_allocations() + perf.commit_wal_encode_buffer_reuses() >= 8
    );
    assert!(perf.commit_wal_encode_buffer_reuses() > 0);
}

#[test]
fn append_that_exactly_fills_segment_stays_in_current_segment() {
    let segment_size = 1024;
    let record = record_with_frame_len(1, segment_size - WAL_SEGMENT_HEADER_SIZE as u64, 0x41);
    let frame_len = record_frame_len(&record);
    let backend = StoredWalBackend::new();
    let mut service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::new(segment_size),
    )
    .expect("open WAL");

    let append = service.append(&record).expect("append exact-fit record");

    let segment_one = ObjectLayout::wal_segment(1).expect("segment one");
    let segment_two = ObjectLayout::wal_segment(2).expect("segment two");
    assert_eq!(append.segment_id(), 1);
    assert_eq!(append.start_offset(), WAL_SEGMENT_HEADER_SIZE as u64);
    assert_eq!(append.bytes_written(), frame_len);
    assert_eq!(service.active_segment_id(), 1);
    assert_eq!(
        backend
            .object_metadata(&segment_one)
            .expect("segment one metadata")
            .size_bytes(),
        segment_size
    );
    assert!(backend.object_metadata(&segment_two).is_err());
    assert_eq!(
        backend.read_object(&segment_one).expect("segment bytes"),
        segment_bytes(1, std::slice::from_ref(&record))
    );
    let read = service.read_all().expect("read exact-fit record");
    assert_eq!(read.records(), std::slice::from_ref(&record));
    assert_eq!(read.truncation(), None);
}

#[test]
fn append_that_exceeds_remaining_segment_by_one_rotates_first() {
    let segment_size = 1024;
    let second = record(2, b"second".to_vec());
    let second_len = record_frame_len(&second);
    let first = record_with_frame_len(
        1,
        segment_size - WAL_SEGMENT_HEADER_SIZE as u64 - second_len + 1,
        0x42,
    );
    let backend = StoredWalBackend::new();
    let mut service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::new(segment_size),
    )
    .expect("open WAL");
    service.append(&first).expect("append first record");
    let segment_one = ObjectLayout::wal_segment(1).expect("segment one");
    let first_segment_bytes = backend
        .read_object(&segment_one)
        .expect("segment one bytes");

    let append = service
        .append(&second)
        .expect("append second record after rotation");

    let segment_two = ObjectLayout::wal_segment(2).expect("segment two");
    assert_eq!(append.segment_id(), 2);
    assert_eq!(append.start_offset(), WAL_SEGMENT_HEADER_SIZE as u64);
    assert_eq!(append.bytes_written(), second_len);
    assert_eq!(append.dirty_bytes(), second_len);
    assert_eq!(service.active_segment_id(), 2);
    assert_eq!(service.dirty_bytes(), second_len);
    assert_eq!(service.dirty_records(), 1);
    assert_eq!(
        backend
            .read_object(&segment_one)
            .expect("segment one bytes"),
        first_segment_bytes
    );
    assert_eq!(
        backend
            .read_object(&segment_two)
            .expect("segment two bytes"),
        segment_bytes(2, std::slice::from_ref(&second))
    );
    let read = service.read_all().expect("read rotated WAL");
    assert_eq!(read.records(), &[first, second]);
}

#[test]
fn record_larger_than_segment_capacity_is_rejected_before_backend_append() {
    let backend = StoredWalBackend::new();
    let segment_one = ObjectLayout::wal_segment(1).expect("segment one");
    let mut service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::new(1024),
    )
    .expect("open WAL");
    let before = backend.read_object(&segment_one).expect("segment header");
    let record = record(1, vec![0x99; 2_000]);
    let frame_len = record_frame_len(&record);

    let error = service
        .append(&record)
        .expect_err("oversized record should fail before append");

    assert_eq!(
        error,
        WalServiceError::RecordTooLarge {
            bytes: frame_len,
            segment_size: 1024,
        }
    );
    assert_clean_append_state(&service, 1);
    assert_eq!(
        backend
            .read_object(&segment_one)
            .expect("segment after error"),
        before
    );
    assert!(backend
        .object_metadata(&ObjectLayout::wal_segment(2).expect("segment two"))
        .is_err());
}

#[test]
fn active_metadata_mismatch_before_append_rejects_without_writing() {
    let backend = StoredWalBackend::new();
    let segment_one = ObjectLayout::wal_segment(1).expect("segment one");
    let mut service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    )
    .expect("open WAL");
    let before = backend.read_object(&segment_one).expect("segment header");
    backend.set_metadata_size(&segment_one, before.len() as u64 + 1);

    let error = service
        .append(&record(1, b"metadata mismatch".to_vec()))
        .expect_err("metadata mismatch should reject append");

    assert_eq!(
        error,
        WalServiceError::UnexpectedAppendOffset {
            object: segment_one.clone(),
            expected: before.len() as u64,
            actual: before.len() as u64 + 1,
        }
    );
    assert_clean_append_state(&service, 1);
    assert_eq!(
        backend
            .read_object(&segment_one)
            .expect("segment after error"),
        before
    );
}

#[test]
fn active_metadata_mismatch_after_rotation_rejects_before_new_segment_append() {
    let segment_size = 1024;
    let second = record(2, b"second".to_vec());
    let second_len = record_frame_len(&second);
    let first = record_with_frame_len(
        1,
        segment_size - WAL_SEGMENT_HEADER_SIZE as u64 - second_len + 1,
        0x43,
    );
    let backend = StoredWalBackend::new();
    let segment_two = ObjectLayout::wal_segment(2).expect("segment two");
    backend.set_metadata_size(&segment_two, WAL_SEGMENT_HEADER_SIZE as u64 + 1);
    let mut service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::new(segment_size),
    )
    .expect("open WAL");
    service.append(&first).expect("append first record");

    let error = service
        .append(&second)
        .expect_err("metadata mismatch after rotation should reject append");

    assert_eq!(
        error,
        WalServiceError::UnexpectedAppendOffset {
            object: segment_two.clone(),
            expected: WAL_SEGMENT_HEADER_SIZE as u64,
            actual: WAL_SEGMENT_HEADER_SIZE as u64 + 1,
        }
    );
    assert_clean_append_state(&service, 2);
    assert_eq!(
        backend
            .read_object(&segment_two)
            .expect("segment two bytes"),
        encode_wal_segment_header(&WalSegmentHeader::new(2, database_id()))
    );
    let read = service.read_all().expect("read after refused append");
    assert_eq!(read.records(), &[first]);
}

#[test]
fn append_backend_failure_does_not_advance_service_state() {
    let backend = MisreportingAppendBackend::new(AppendReportFault::BackendFailure);
    let mut service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    )
    .expect("open WAL");
    let segment_one = ObjectLayout::wal_segment(1).expect("segment one");
    let before = backend.read_object(&segment_one).expect("segment header");

    let error = service
        .append(&record(1, b"append backend failure".to_vec()))
        .expect_err("backend append failure should be rejected");

    match error {
        WalServiceError::Backend {
            operation,
            object,
            source,
        } => {
            assert_eq!(operation, WalOperation::Append);
            assert_eq!(object, segment_one);
            assert_eq!(source.kind(), BackendErrorKind::Unknown);
        }
        other => panic!("expected backend append error, got {other:?}"),
    }
    assert_clean_append_state(&service, 1);
    assert_eq!(
        backend
            .read_object(&ObjectLayout::wal_segment(1).expect("segment one"))
            .expect("segment after append failure"),
        before
    );
}

#[test]
fn append_rejects_backend_wrong_start_offset_without_advancing_state() {
    let backend = MisreportingAppendBackend::new(AppendReportFault::WrongStartOffset);
    let mut service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    )
    .expect("open WAL");
    let first = record(1, b"wrong start offset".to_vec());

    let error = service
        .append(&first)
        .expect_err("wrong start offset should be rejected");

    let segment_one = ObjectLayout::wal_segment(1).expect("segment one");
    assert_eq!(
        error,
        WalServiceError::UnexpectedAppendOffset {
            object: segment_one.clone(),
            expected: WAL_SEGMENT_HEADER_SIZE as u64,
            actual: WAL_SEGMENT_HEADER_SIZE as u64 + 1,
        }
    );
    assert_clean_append_state(&service, 1);
    assert_eq!(
        backend
            .read_object(&segment_one)
            .expect("segment after bad append report"),
        segment_bytes(1, std::slice::from_ref(&first))
    );
    let retry = service
        .append(&record(2, b"retry".to_vec()))
        .expect_err("state should still expect the header offset");
    match retry {
        WalServiceError::UnexpectedAppendOffset {
            expected, actual, ..
        } => {
            assert_eq!(expected, WAL_SEGMENT_HEADER_SIZE as u64);
            assert!(actual > expected);
        }
        other => panic!("expected retry offset mismatch, got {other:?}"),
    }
}

#[test]
fn append_rejects_backend_short_byte_count_report_without_advancing_state() {
    let backend = MisreportingAppendBackend::new(AppendReportFault::ShortLength);
    let mut service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    )
    .expect("open WAL");
    let first = record(1, b"short append report".to_vec());
    let frame_len = record_frame_len(&first);

    let error = service
        .append(&first)
        .expect_err("short append report should be rejected");

    assert_eq!(
        error,
        WalServiceError::UnexpectedAppendLength {
            object: ObjectLayout::wal_segment(1).expect("segment one"),
            expected: frame_len,
            actual: frame_len - 1,
        }
    );
    assert_clean_append_state(&service, 1);
    let retry = service
        .append(&record(2, b"retry".to_vec()))
        .expect_err("state should still expect the header offset");
    match retry {
        WalServiceError::UnexpectedAppendOffset {
            expected, actual, ..
        } => {
            assert_eq!(expected, WAL_SEGMENT_HEADER_SIZE as u64);
            assert!(actual > expected);
        }
        other => panic!("expected retry offset mismatch, got {other:?}"),
    }
}

#[test]
fn append_rejects_backend_wrong_metadata_size_report_without_advancing_state() {
    let backend = MisreportingAppendBackend::new(AppendReportFault::WrongMetadataSize);
    let mut service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        WalServiceConfig::default(),
    )
    .expect("open WAL");
    let first = record(1, b"wrong size report".to_vec());
    let frame_len = record_frame_len(&first);

    let error = service
        .append(&first)
        .expect_err("wrong metadata size should be rejected");

    assert_eq!(
        error,
        WalServiceError::UnexpectedObjectSize {
            object: ObjectLayout::wal_segment(1).expect("segment one"),
            expected: WAL_SEGMENT_HEADER_SIZE as u64 + frame_len,
            actual: WAL_SEGMENT_HEADER_SIZE as u64 + frame_len - 1,
        }
    );
    assert_clean_append_state(&service, 1);
    let retry = service
        .append(&record(2, b"retry".to_vec()))
        .expect_err("state should still expect the header offset");
    match retry {
        WalServiceError::UnexpectedAppendOffset {
            expected, actual, ..
        } => {
            assert_eq!(expected, WAL_SEGMENT_HEADER_SIZE as u64);
            assert!(actual > expected);
        }
        other => panic!("expected retry offset mismatch, got {other:?}"),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn wal_append_model_strategy() -> impl Strategy<Value = (u64, Vec<(u64, Vec<u8>)>)> {
    // Row-native payload framing adds fixed row overhead around the fuzzed value
    // bytes, so the large arm stays below the minimum 1 KiB segment while still
    // hitting rotation and near-capacity paths.
    let payload = proptest::prop_oneof![
        4 => vec(any::<u8>(), 0..=256),
        1 => vec(any::<u8>(), 500..=740),
    ];
    (1024_u64..=8192, vec((1_u64..=1_000_000, payload), 1..=128))
}

#[cfg(not(target_arch = "wasm32"))]
fn fail_case(message: impl Into<String>) -> TestCaseError {
    TestCaseError::fail(message.into())
}

#[cfg(not(target_arch = "wasm32"))]
fn run_wal_append_model((segment_size, inputs): (u64, Vec<(u64, Vec<u8>)>)) -> TestCaseResult {
    let backend = StoredWalBackend::new();
    let config = WalServiceConfig::new(segment_size);
    let mut service = WalService::open(
        &backend,
        database_id(),
        1,
        DurabilityPolicy::Standard,
        config,
    )
    .map_err(|error| fail_case(format!("open WAL: {error:?}")))?;
    let mut expected_records = Vec::new();
    let mut active_records: Vec<WalRecord> = Vec::new();
    let mut active_segment_id = 1_u64;
    let mut active_size = WAL_SEGMENT_HEADER_SIZE as u64;
    let mut dirty_bytes = 0_u64;
    let mut dirty_records = 0_u64;

    for (version, payload) in inputs {
        let record = record(version, payload);
        let frame_len = record_frame_len(&record);
        if active_size + frame_len > segment_size {
            active_segment_id += 1;
            active_size = WAL_SEGMENT_HEADER_SIZE as u64;
            dirty_bytes = 0;
            dirty_records = 0;
            active_records.clear();
        }

        let expected_offset = active_size;
        let append = service
            .append(&record)
            .map_err(|error| fail_case(format!("append record: {error:?}")))?;
        prop_assert_eq!(append.segment_id(), active_segment_id);
        prop_assert_eq!(append.start_offset(), expected_offset);
        prop_assert_eq!(append.bytes_written(), frame_len);
        prop_assert_eq!(append.forced_durable(), false);

        active_size += frame_len;
        dirty_bytes += frame_len;
        dirty_records += 1;
        expected_records.push(record.clone());
        active_records.push(record);

        prop_assert_eq!(append.dirty_bytes(), dirty_bytes);
        prop_assert_eq!(service.active_segment_id(), active_segment_id);
        prop_assert_eq!(service.dirty_bytes(), dirty_bytes);
        prop_assert_eq!(service.dirty_records(), dirty_records);
        assert_active_metadata_matches_records(&service, active_segment_id, &active_records)?;
    }

    let reopened = WalService::open(
        &backend,
        database_id(),
        active_segment_id,
        DurabilityPolicy::Standard,
        config,
    )
    .map_err(|error| fail_case(format!("reopen WAL: {error:?}")))?;
    let read = reopened
        .read_all()
        .map_err(|error| fail_case(format!("read WAL: {error:?}")))?;
    prop_assert_eq!(read.records(), expected_records.as_slice());
    prop_assert_eq!(read.truncation(), None);

    for object in backend.listed_objects() {
        let size = backend
            .object_metadata(&object)
            .map_err(|error| fail_case(format!("metadata for {object}: {error:?}")))?
            .size_bytes();
        if size > segment_size {
            return Err(fail_case(format!(
                "{object} has {size} bytes, above configured segment size {segment_size}"
            )));
        }
    }

    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn assert_active_metadata_matches_records(
    service: &WalService<'_>,
    segment_id: u64,
    records: &[WalRecord],
) -> TestCaseResult {
    let metadata = service.active_metadata();
    prop_assert_eq!(metadata.segment_id(), segment_id);
    prop_assert_eq!(metadata.record_count(), records.len() as u64);
    let min_version = records
        .iter()
        .map(WalRecord::commit_version)
        .min()
        .expect("active segment has records");
    let max_version = records
        .iter()
        .map(WalRecord::commit_version)
        .max()
        .expect("active segment has records");
    let min_timestamp = records
        .iter()
        .map(WalRecord::commit_timestamp)
        .min()
        .expect("active segment has records");
    let max_timestamp = records
        .iter()
        .map(WalRecord::commit_timestamp)
        .max()
        .expect("active segment has records");
    prop_assert_eq!(metadata.min_commit_version(), min_version);
    prop_assert_eq!(metadata.max_commit_version(), max_version);
    prop_assert_eq!(metadata.min_timestamp(), min_timestamp);
    prop_assert_eq!(metadata.max_timestamp(), max_timestamp);
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn wal_append_model_matches_service_facts() {
    let mut runner = TestRunner::new(Config {
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "proptest-regressions/wal_append_model.txt",
        ))),
        ..Config::default()
    });

    runner
        .run(&wal_append_model_strategy(), run_wal_append_model)
        .expect("WAL append model should match service facts");
}
