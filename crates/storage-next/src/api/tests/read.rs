use super::*;

use std::time::Duration;

use crate::branch::read::BranchTimestampCoverage;
use crate::commit::COMMIT_TIMELINE_SPACE;
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId as RowStorageSpaceId};

fn open_runtime() -> StorageRuntime<'static> {
    StorageRuntime::open(StorageOpenOptions::default())
        .expect("open cache runtime")
        .into_runtime()
}

fn branch() -> BranchId {
    StorageRuntime::default_branch_id_for_test()
}

fn other_branch() -> BranchId {
    branch_id(0x44)
}

fn engine_space() -> StorageSpaceId {
    StorageSpaceId::new(vec![0x20]).expect("engine storage space")
}

fn api_key(bytes: &[u8]) -> StorageKey {
    StorageKey::new(bytes.to_vec()).expect("valid API key")
}

fn put_batch(key: &[u8], value: &[u8]) -> CommitBatch {
    put_batch_with_ttl(key, value, None)
}

fn put_batch_with_ttl(key: &[u8], value: &[u8], ttl: Option<Duration>) -> CommitBatch {
    CommitBatch::new(
        branch(),
        vec![CommitMutation::Put {
            storage_space: engine_space(),
            key: api_key(key),
            value: StorageValue::new(value.to_vec()),
            ttl,
        }],
        CommitOptions::default().require_conflict_check(false),
    )
    .expect("valid put batch")
}

fn delete_batch(key: &[u8]) -> CommitBatch {
    CommitBatch::new(
        branch(),
        vec![CommitMutation::Delete {
            storage_space: engine_space(),
            key: api_key(key),
        }],
        CommitOptions::default().require_conflict_check(false),
    )
    .expect("valid delete batch")
}

fn commit_put(
    runtime: &mut StorageRuntime<'static>,
    key: &[u8],
    value: &[u8],
    ts: u64,
) -> CommitSummary {
    runtime
        .commit_for_test(&put_batch(key, value), Timestamp::from_micros(ts))
        .expect("commit put")
}

fn commit_put_with_ttl(
    runtime: &mut StorageRuntime<'static>,
    key: &[u8],
    value: &[u8],
    ts: u64,
    ttl: Duration,
) -> CommitSummary {
    runtime
        .commit_for_test(
            &put_batch_with_ttl(key, value, Some(ttl)),
            Timestamp::from_micros(ts),
        )
        .expect("commit put with ttl")
}

fn commit_delete(runtime: &mut StorageRuntime<'static>, key: &[u8], ts: u64) -> CommitSummary {
    runtime
        .commit_for_test(&delete_batch(key), Timestamp::from_micros(ts))
        .expect("commit delete")
}

fn point_request(key: &[u8], bound: ReadBound) -> PointReadRequest {
    PointReadRequest::new(branch(), engine_space(), api_key(key), bound)
}

fn point_request_for(branch_id: BranchId, key: &[u8], bound: ReadBound) -> PointReadRequest {
    PointReadRequest::new(branch_id, engine_space(), api_key(key), bound)
}

fn read_value(row: &StorageReadRow) -> &[u8] {
    row.value().expect("put row").as_bytes()
}

#[test]
fn read_latest_returns_newest_visible_value() {
    let mut runtime = open_runtime();
    commit_put(&mut runtime, b"alpha", b"old", 10);
    let latest = commit_put(&mut runtime, b"alpha", b"new", 20);

    let outcome = runtime
        .read_point(&point_request(b"alpha", ReadBound::Latest))
        .expect("read latest");
    let row = outcome.row().expect("row present");
    assert_eq!(read_value(row), b"new");
    assert_eq!(row.commit_version(), latest.commit_version());
    assert_eq!(row.commit_timestamp(), latest.commit_timestamp());
    assert!(!row.is_tombstone());
}

#[test]
fn read_latest_returns_none_for_absent_key() {
    let mut runtime = open_runtime();
    commit_put(&mut runtime, b"alpha", b"value", 10);

    let outcome = runtime
        .read_point(&point_request(b"missing", ReadBound::Latest))
        .expect("read absent");
    assert!(outcome.row().is_none());
}

#[test]
fn read_latest_returns_tombstone_fact_for_visible_delete() {
    let mut runtime = open_runtime();
    commit_put(&mut runtime, b"alpha", b"value", 10);
    let deleted = commit_delete(&mut runtime, b"alpha", 20);

    let outcome = runtime
        .read_point(&point_request(b"alpha", ReadBound::Latest))
        .expect("read tombstone");
    let row = outcome.row().expect("tombstone fact");
    assert!(row.is_tombstone());
    assert!(row.value().is_none());
    assert_eq!(row.commit_version(), deleted.commit_version());
}

#[test]
fn read_at_version_returns_exact_retained_value() {
    let mut runtime = open_runtime();
    let first = commit_put(&mut runtime, b"alpha", b"old", 10);
    commit_put(&mut runtime, b"alpha", b"new", 20);

    let outcome = runtime
        .read_point(&point_request(
            b"alpha",
            ReadBound::AtVersion(first.commit_version()),
        ))
        .expect("read version");
    assert_eq!(read_value(outcome.row().expect("row present")), b"old");
}

#[test]
fn read_at_version_uses_latest_at_or_before_version() {
    let mut runtime = open_runtime();
    let first = commit_put(&mut runtime, b"alpha", b"old", 10);
    let second = commit_put(&mut runtime, b"alpha", b"new", 20);
    commit_put(&mut runtime, b"beta", b"separate", 30);

    let outcome = runtime
        .read_point(&point_request(
            b"alpha",
            ReadBound::AtVersion(second.commit_version()),
        ))
        .expect("read version");
    let row = outcome.row().expect("row present");
    assert_eq!(read_value(row), b"new");
    assert!(row.commit_version() > first.commit_version());
}

#[test]
fn read_at_version_rejects_unretained_history() {
    let mut runtime = open_runtime();
    commit_put(&mut runtime, b"alpha", b"value", 10);

    let error = runtime
        .read_point(&point_request(
            b"alpha",
            ReadBound::AtVersion(CommitVersion::ZERO),
        ))
        .expect_err("zero version is before retained timeline");
    assert_eq!(error.class(), StorageApiErrorClass::HistoryUnavailable);
}

#[test]
fn read_at_version_rejects_unrecorded_future_version() {
    let mut runtime = open_runtime();
    commit_put(&mut runtime, b"alpha", b"value", 10);

    let error = runtime
        .read_point(&point_request(
            b"alpha",
            ReadBound::AtVersion(CommitVersion::new(99)),
        ))
        .expect_err("unrecorded version is not a retained frontier");
    assert_eq!(error.class(), StorageApiErrorClass::HistoryUnavailable);
}

#[test]
fn read_at_timestamp_resolves_to_commit_version() {
    let mut runtime = open_runtime();
    let first = commit_put(&mut runtime, b"alpha", b"old", 10);
    commit_put(&mut runtime, b"alpha", b"new", 30);

    let outcome = runtime
        .read_point(&point_request(
            b"alpha",
            ReadBound::AtTimestamp(Timestamp::from_micros(20)),
        ))
        .expect("read timestamp");
    let row = outcome.row().expect("row present");
    assert_eq!(read_value(row), b"old");
    assert_eq!(row.commit_version(), first.commit_version());
}

#[test]
fn read_at_timestamp_after_latest_rejects() {
    let mut runtime = open_runtime();
    commit_put(&mut runtime, b"alpha", b"value", 10);

    let error = runtime
        .read_point(&point_request(
            b"alpha",
            ReadBound::AtTimestamp(Timestamp::from_micros(20)),
        ))
        .expect_err("after-latest timestamp read must not clamp to current");
    assert_eq!(error.class(), StorageApiErrorClass::HistoryUnavailable);
}

#[test]
fn read_at_timestamp_rejects_insufficient_history() {
    let mut runtime = open_runtime();
    commit_put(&mut runtime, b"alpha", b"value", 50);
    runtime
        .set_timestamp_coverage_for_test(
            branch(),
            BranchTimestampCoverage::complete_since(Timestamp::from_micros(40)),
        )
        .expect("set coverage");

    let error = runtime
        .read_point(&point_request(
            b"alpha",
            ReadBound::AtTimestamp(Timestamp::from_micros(10)),
        ))
        .expect_err("timestamp before retained history");
    assert_eq!(error.class(), StorageApiErrorClass::HistoryUnavailable);
}

#[test]
fn read_after_close_rejects_closed_runtime() {
    let mut runtime = open_runtime();
    commit_put(&mut runtime, b"alpha", b"value", 10);
    runtime.close().expect("close runtime");

    let error = runtime
        .read_point(&point_request(b"alpha", ReadBound::Latest))
        .expect_err("closed runtime rejected");
    assert_eq!(error.class(), StorageApiErrorClass::FailedPrecondition);
}

#[test]
fn read_unknown_branch_rejects() {
    let mut runtime = open_runtime();
    commit_put(&mut runtime, b"alpha", b"value", 10);

    let error = runtime
        .read_point(&point_request_for(
            other_branch(),
            b"alpha",
            ReadBound::Latest,
        ))
        .expect_err("unknown branch rejected");
    assert_eq!(error.class(), StorageApiErrorClass::NotFound);
}

#[test]
fn read_at_version_applies_ttl_at_selected_frontier() {
    let mut runtime = open_runtime();
    let first = commit_put_with_ttl(
        &mut runtime,
        b"alpha",
        b"value",
        10,
        Duration::from_micros(5),
    );
    let second = commit_put(&mut runtime, b"beta", b"other", 20);

    let before_expiry = runtime
        .read_point(&point_request(
            b"alpha",
            ReadBound::AtVersion(first.commit_version()),
        ))
        .expect("read before expiry");
    assert_eq!(
        read_value(before_expiry.row().expect("row before expiry")),
        b"value"
    );

    let after_expiry = runtime
        .read_point(&point_request(
            b"alpha",
            ReadBound::AtVersion(second.commit_version()),
        ))
        .expect("read after expiry");
    assert!(after_expiry.row().is_none());
}

#[test]
fn read_at_timestamp_applies_ttl_at_matched_commit_frontier() {
    let mut runtime = open_runtime();
    commit_put_with_ttl(
        &mut runtime,
        b"alpha",
        b"value",
        10,
        Duration::from_micros(12),
    );
    commit_put(&mut runtime, b"beta", b"beta", 20);
    commit_put(&mut runtime, b"gamma", b"gamma", 30);

    let outcome = runtime
        .read_point(&point_request(
            b"alpha",
            ReadBound::AtTimestamp(Timestamp::from_micros(25)),
        ))
        .expect("timestamp read between commits");
    assert_eq!(
        read_value(outcome.row().expect("ttl is evaluated at matched commit")),
        b"value"
    );
}

#[test]
fn scan_at_version_applies_ttl_at_selected_frontier() {
    let mut runtime = open_runtime();
    commit_put_with_ttl(&mut runtime, b"item-a", b"a", 10, Duration::from_micros(5));
    let second = commit_put(&mut runtime, b"item-b", b"b", 20);

    let scan = runtime
        .scan_prefix(&PrefixScanReadRequest::new(
            branch(),
            engine_space(),
            api_key(b"item-"),
            ReadBound::AtVersion(second.commit_version()),
            None,
        ))
        .expect("scan after ttl expiry");
    assert_eq!(scan.rows().len(), 1);
    assert_eq!(scan.rows()[0].key().as_bytes(), b"item-b");
}

#[test]
fn history_returns_newest_first() {
    let mut runtime = open_runtime();
    commit_put(&mut runtime, b"alpha", b"one", 10);
    commit_put(&mut runtime, b"alpha", b"two", 20);
    commit_put(&mut runtime, b"alpha", b"three", 30);

    let history = runtime
        .read_history(&HistoryReadRequest::new(
            branch(),
            engine_space(),
            api_key(b"alpha"),
        ))
        .expect("history");
    let values: Vec<&[u8]> = history.rows().iter().map(read_value).collect();
    assert_eq!(
        values,
        vec![b"three".as_slice(), b"two".as_slice(), b"one".as_slice()]
    );
}

#[test]
fn history_limit_is_enforced() {
    let mut runtime = open_runtime();
    commit_put(&mut runtime, b"alpha", b"one", 10);
    commit_put(&mut runtime, b"alpha", b"two", 20);

    let history = runtime
        .read_history(
            &HistoryReadRequest::new(branch(), engine_space(), api_key(b"alpha"))
                .limit(ReadLimit::new(1).expect("valid limit")),
        )
        .expect("history");
    assert_eq!(history.rows().len(), 1);
    assert_eq!(read_value(&history.rows()[0]), b"two");
}

#[test]
fn history_before_version_excludes_newer_versions() {
    let mut runtime = open_runtime();
    let first = commit_put(&mut runtime, b"alpha", b"one", 10);
    let second = commit_put(&mut runtime, b"alpha", b"two", 20);
    commit_put(&mut runtime, b"alpha", b"three", 30);

    let history = runtime
        .read_history(
            &HistoryReadRequest::new(branch(), engine_space(), api_key(b"alpha"))
                .before_version(second.commit_version()),
        )
        .expect("history");
    assert_eq!(history.rows().len(), 1);
    assert_eq!(history.rows()[0].commit_version(), first.commit_version());
}

#[test]
fn history_preserves_tombstone_entries() {
    let mut runtime = open_runtime();
    commit_put(&mut runtime, b"alpha", b"one", 10);
    let deleted = commit_delete(&mut runtime, b"alpha", 20);

    let history = runtime
        .read_history(&HistoryReadRequest::new(
            branch(),
            engine_space(),
            api_key(b"alpha"),
        ))
        .expect("history");
    assert!(history.rows()[0].is_tombstone());
    assert_eq!(history.rows()[0].commit_version(), deleted.commit_version());
}

#[test]
fn history_pruned_versions_return_retention_error() {
    let mut runtime = open_runtime();
    commit_put(&mut runtime, b"alpha", b"one", 10);

    let error = runtime
        .read_history(
            &HistoryReadRequest::new(branch(), engine_space(), api_key(b"alpha"))
                .before_version(CommitVersion::ZERO),
        )
        .expect_err("unretained history rejected");
    assert_eq!(error.class(), StorageApiErrorClass::HistoryUnavailable);
}

#[test]
fn history_empty_key_returns_empty_history() {
    let mut runtime = open_runtime();
    commit_put(&mut runtime, b"alpha", b"one", 10);

    let history = runtime
        .read_history(&HistoryReadRequest::new(
            branch(),
            engine_space(),
            api_key(b"missing"),
        ))
        .expect("history");
    assert!(history.rows().is_empty());
}

#[test]
fn prefix_scan_returns_sorted_keys() {
    let mut runtime = open_runtime();
    commit_put(&mut runtime, b"item-c", b"c", 10);
    commit_put(&mut runtime, b"item-a", b"a", 20);
    commit_put(&mut runtime, b"other", b"x", 30);
    commit_put(&mut runtime, b"item-b", b"b", 40);

    let scan = runtime
        .scan_prefix(&PrefixScanReadRequest::new(
            branch(),
            engine_space(),
            api_key(b"item-"),
            ReadBound::Latest,
            None,
        ))
        .expect("prefix scan");
    let keys: Vec<&[u8]> = scan.rows().iter().map(|row| row.key().as_bytes()).collect();
    assert_eq!(
        keys,
        vec![
            b"item-a".as_slice(),
            b"item-b".as_slice(),
            b"item-c".as_slice()
        ]
    );
}

#[test]
fn prefix_scan_applies_version_bound() {
    let mut runtime = open_runtime();
    let first = commit_put(&mut runtime, b"item-a", b"old", 10);
    commit_put(&mut runtime, b"item-a", b"new", 20);

    let scan = runtime
        .scan_prefix(&PrefixScanReadRequest::new(
            branch(),
            engine_space(),
            api_key(b"item-"),
            ReadBound::AtVersion(first.commit_version()),
            None,
        ))
        .expect("prefix scan");
    assert_eq!(scan.rows().len(), 1);
    assert_eq!(read_value(&scan.rows()[0]), b"old");
}

#[test]
fn prefix_scan_applies_timestamp_bound() {
    let mut runtime = open_runtime();
    commit_put(&mut runtime, b"item-a", b"old", 10);
    commit_put(&mut runtime, b"item-a", b"new", 30);

    let scan = runtime
        .scan_prefix(&PrefixScanReadRequest::new(
            branch(),
            engine_space(),
            api_key(b"item-"),
            ReadBound::AtTimestamp(Timestamp::from_micros(20)),
            None,
        ))
        .expect("prefix scan");
    assert_eq!(read_value(&scan.rows()[0]), b"old");
}

#[test]
fn prefix_scan_limit_is_stable() {
    let mut runtime = open_runtime();
    commit_put(&mut runtime, b"item-b", b"b", 10);
    commit_put(&mut runtime, b"item-a", b"a", 20);
    commit_put(&mut runtime, b"item-c", b"c", 30);

    let scan = runtime
        .scan_prefix(&PrefixScanReadRequest::new(
            branch(),
            engine_space(),
            api_key(b"item-"),
            ReadBound::Latest,
            Some(ReadLimit::new(2).expect("valid limit")),
        ))
        .expect("prefix scan");
    let keys: Vec<&[u8]> = scan.rows().iter().map(|row| row.key().as_bytes()).collect();
    assert_eq!(keys, vec![b"item-a".as_slice(), b"item-b".as_slice()]);
}

#[test]
fn range_scan_respects_start_and_end() {
    let mut runtime = open_runtime();
    for name in [
        b"a".as_slice(),
        b"b".as_slice(),
        b"c".as_slice(),
        b"d".as_slice(),
    ] {
        commit_put(&mut runtime, name, name, 10 + u64::from(name[0]));
    }

    let scan = runtime
        .scan_range(&ScanReadRequest::new(
            branch(),
            engine_space(),
            ScanRange::new(Some(api_key(b"b")), Some(api_key(b"d"))).expect("valid range"),
            ReadBound::Latest,
            None,
        ))
        .expect("range scan");
    let keys: Vec<&[u8]> = scan.rows().iter().map(|row| row.key().as_bytes()).collect();
    assert_eq!(keys, vec![b"b".as_slice(), b"c".as_slice()]);
}

#[test]
fn range_scan_empty_range_returns_empty() {
    let mut runtime = open_runtime();
    commit_put(&mut runtime, b"a", b"a", 10);

    let scan = runtime
        .scan_range(&ScanReadRequest::new(
            branch(),
            engine_space(),
            ScanRange::new(Some(api_key(b"x")), Some(api_key(b"z"))).expect("valid range"),
            ReadBound::Latest,
            None,
        ))
        .expect("range scan");
    assert!(scan.rows().is_empty());
}

#[test]
fn range_scan_tombstone_visibility_matches_point_read() {
    let mut runtime = open_runtime();
    commit_put(&mut runtime, b"b", b"value", 10);
    commit_delete(&mut runtime, b"b", 20);

    let point = runtime
        .read_point(&point_request(b"b", ReadBound::Latest))
        .expect("point read");
    let scan = runtime
        .scan_range(&ScanReadRequest::new(
            branch(),
            engine_space(),
            ScanRange::new(Some(api_key(b"a")), Some(api_key(b"c"))).expect("valid range"),
            ReadBound::Latest,
            None,
        ))
        .expect("range scan");
    assert!(point.row().expect("point tombstone").is_tombstone());
    assert!(scan.rows()[0].is_tombstone());
}

#[test]
fn scan_inherited_rows_match_point_reads() {
    let mut runtime = open_runtime();
    commit_put(&mut runtime, b"item-a", b"a", 10);
    commit_put(&mut runtime, b"item-b", b"b", 20);
    runtime
        .flush_default_branch_for_test()
        .expect("flush parent branch");
    let child = other_branch();
    runtime
        .fork_default_branch_for_test(child)
        .expect("fork branch");

    let point = runtime
        .read_point(&point_request_for(child, b"item-a", ReadBound::Latest))
        .expect("point read");
    let scan = runtime
        .scan_prefix(&PrefixScanReadRequest::new(
            child,
            engine_space(),
            api_key(b"item-"),
            ReadBound::Latest,
            None,
        ))
        .expect("prefix scan");
    assert_eq!(read_value(point.row().expect("inherited point")), b"a");
    assert_eq!(scan.rows().len(), 2);
    assert_eq!(read_value(&scan.rows()[0]), b"a");
}

#[test]
fn timestamp_lookup_returns_newest_commit_at_or_before_timestamp() {
    let mut runtime = open_runtime();
    let first = commit_put(&mut runtime, b"a", b"a", 10);
    commit_put(&mut runtime, b"b", b"b", 30);

    let lookup = runtime
        .lookup_version_at_or_before_timestamp(TimestampLookupRequest::new(
            branch(),
            Timestamp::from_micros(20),
        ))
        .expect("timeline lookup");
    assert_eq!(lookup.matched_version(), first.commit_version());
    assert_eq!(lookup.matched_timestamp(), first.commit_timestamp());
}

#[test]
fn timestamp_lookup_equal_timestamps_uses_greatest_version() {
    let mut runtime = open_runtime();
    commit_put(&mut runtime, b"a", b"a", 10);
    let second = commit_put(&mut runtime, b"b", b"b", 10);

    let lookup = runtime
        .lookup_version_at_or_before_timestamp(TimestampLookupRequest::new(
            branch(),
            Timestamp::from_micros(10),
        ))
        .expect("timeline lookup");
    assert_eq!(lookup.matched_version(), second.commit_version());
}

#[test]
fn timestamp_lookup_before_retained_range_rejects() {
    let mut runtime = open_runtime();
    commit_put(&mut runtime, b"a", b"a", 50);

    let error = runtime
        .lookup_version_at_or_before_timestamp(TimestampLookupRequest::new(
            branch(),
            Timestamp::from_micros(10),
        ))
        .expect_err("timestamp before retained timeline");
    assert_eq!(error.class(), StorageApiErrorClass::HistoryUnavailable);
}

#[test]
fn timestamp_lookup_after_latest_returns_matched_with_miss_flag() {
    let mut runtime = open_runtime();
    let latest = commit_put(&mut runtime, b"a", b"a", 50);

    let lookup = runtime
        .lookup_version_at_or_before_timestamp(TimestampLookupRequest::new(
            branch(),
            Timestamp::from_micros(60),
        ))
        .expect("after-latest timeline lookup");
    assert_eq!(lookup.matched_version(), latest.commit_version());
    assert_eq!(lookup.matched_timestamp(), latest.commit_timestamp());
    assert_eq!(
        lookup.miss(),
        Some(TimestampLookupMiss::AfterLatestRetained)
    );
}

#[test]
fn version_lookup_returns_commit_timestamp() {
    let mut runtime = open_runtime();
    let commit = commit_put(&mut runtime, b"a", b"a", 50);

    let lookup = runtime
        .lookup_timestamp_for_version(VersionLookupRequest::new(branch(), commit.commit_version()))
        .expect("version lookup");
    assert_eq!(lookup.timestamp(), commit.commit_timestamp());
}

#[test]
fn version_lookup_unretained_version_rejects() {
    let mut runtime = open_runtime();
    commit_put(&mut runtime, b"a", b"a", 50);

    let error = runtime
        .lookup_timestamp_for_version(VersionLookupRequest::new(branch(), CommitVersion::ZERO))
        .expect_err("version outside retained timeline");
    assert_eq!(error.class(), StorageApiErrorClass::HistoryUnavailable);
}

#[test]
fn timeline_bounds_report_retained_range() {
    let mut runtime = open_runtime();
    let first = commit_put(&mut runtime, b"a", b"a", 10);
    let second = commit_put(&mut runtime, b"b", b"b", 30);

    let bounds = runtime
        .timeline_bounds(TimelineBoundsRequest::new(branch()))
        .expect("timeline bounds");
    assert_eq!(bounds.min_timestamp(), Some(first.commit_timestamp()));
    assert_eq!(bounds.max_timestamp(), Some(second.commit_timestamp()));
    assert_eq!(bounds.min_version(), Some(first.commit_version()));
    assert_eq!(bounds.max_version(), Some(second.commit_version()));
}

#[test]
fn timeline_corruption_maps_to_diagnostic_error() {
    let mut runtime = open_runtime();
    commit_put(&mut runtime, b"a", b"a", 10);
    let bad_key = PhysicalKey::new(
        branch(),
        COMMIT_TIMELINE_SPACE,
        RowStorageSpaceId::COMMIT_TIMELINE,
        b"ts-v1\0short".to_vec(),
    )
    .expect("timeline key");
    runtime
        .append_raw_row_for_test(StorageRow::put(
            bad_key,
            CommitVersion::new(99),
            Timestamp::from_micros(99),
            Timestamp::EPOCH,
            99_u64.to_be_bytes(),
        ))
        .expect("append corrupt timeline row");

    let error = runtime
        .timeline_bounds(TimelineBoundsRequest::new(branch()))
        .expect_err("timeline corruption rejected");
    assert_eq!(error.class(), StorageApiErrorClass::Internal);
    assert!(error.source().is_some());
}

#[test]
fn timeline_tombstone_corruption_maps_to_diagnostic_error() {
    let mut runtime = open_runtime();
    commit_put(&mut runtime, b"a", b"a", 10);
    let mut user_key = b"ver-v1\0".to_vec();
    user_key.extend_from_slice(&99_u64.to_be_bytes());
    let bad_key = PhysicalKey::new(
        branch(),
        COMMIT_TIMELINE_SPACE,
        RowStorageSpaceId::COMMIT_TIMELINE,
        user_key,
    )
    .expect("timeline key");
    runtime
        .append_raw_row_for_test(StorageRow::tombstone(
            bad_key,
            CommitVersion::new(99),
            Timestamp::from_micros(99),
        ))
        .expect("append corrupt timeline tombstone");

    let error = runtime
        .timeline_bounds(TimelineBoundsRequest::new(branch()))
        .expect_err("timeline tombstone rejected");
    assert_eq!(error.class(), StorageApiErrorClass::Internal);
    assert!(error.source().is_some());
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn generated_read_contract_matches_model_for_mutations_and_reads() {
    use std::collections::BTreeMap;

    use proptest::collection::vec;
    use proptest::prelude::any;
    use proptest::test_runner::{Config, TestCaseError, TestRunner};

    let mut runner = TestRunner::new(Config {
        cases: 48,
        ..Config::default()
    });
    runner
        .run(&vec(any::<u8>(), 1..=96), |script| {
            let mut runtime = open_runtime();
            let mut model = BTreeMap::<Vec<u8>, Vec<ModelRow>>::new();

            for (index, chunk) in script.chunks(4).take(24).enumerate() {
                let key = vec![b'k', b'0' + chunk.get(1).copied().unwrap_or(0) % 4];
                let timestamp = Timestamp::from_micros(10 + u64::try_from(index).unwrap() * 10);
                let value =
                    (chunk[0] % 4 != 0).then(|| vec![b'v', chunk.get(2).copied().unwrap_or(0)]);
                let summary = if let Some(value) = &value {
                    runtime
                        .commit_for_test(&put_batch(&key, value), timestamp)
                        .map_err(|error| TestCaseError::fail(error.to_string()))?
                } else {
                    runtime
                        .commit_for_test(&delete_batch(&key), timestamp)
                        .map_err(|error| TestCaseError::fail(error.to_string()))?
                };
                model.entry(key.clone()).or_default().push(ModelRow {
                    key: key.clone(),
                    value,
                    commit_version: summary.commit_version(),
                    commit_timestamp: summary.commit_timestamp(),
                });

                assert_point_matches_model(&runtime, &model, &key, ReadBound::Latest)?;
                assert_point_matches_model(
                    &runtime,
                    &model,
                    &key,
                    ReadBound::AtVersion(summary.commit_version()),
                )?;
                assert_point_matches_model(
                    &runtime,
                    &model,
                    &key,
                    ReadBound::AtTimestamp(summary.commit_timestamp()),
                )?;
                assert_history_matches_model(&runtime, &model, &key)?;
                assert_prefix_scan_matches_model(&runtime, &model, ReadBound::Latest)?;
                assert_prefix_scan_matches_model(
                    &runtime,
                    &model,
                    ReadBound::AtVersion(summary.commit_version()),
                )?;
            }
            Ok(())
        })
        .expect("generated API read model");
}

#[cfg(not(target_arch = "wasm32"))]
#[derive(Clone, Debug, Eq, PartialEq)]
struct ModelRow {
    key: Vec<u8>,
    value: Option<Vec<u8>>,
    commit_version: CommitVersion,
    commit_timestamp: Timestamp,
}

#[cfg(not(target_arch = "wasm32"))]
fn assert_point_matches_model(
    runtime: &StorageRuntime<'static>,
    model: &std::collections::BTreeMap<Vec<u8>, Vec<ModelRow>>,
    key: &[u8],
    bound: ReadBound,
) -> Result<(), proptest::test_runner::TestCaseError> {
    let outcome = runtime
        .read_point(&point_request(key, bound))
        .map_err(|error| proptest::test_runner::TestCaseError::fail(error.to_string()))?;
    let expected = model_visible_row(model.get(key), bound);
    assert_api_row_matches_model(outcome.row(), expected)
}

#[cfg(not(target_arch = "wasm32"))]
fn assert_history_matches_model(
    runtime: &StorageRuntime<'static>,
    model: &std::collections::BTreeMap<Vec<u8>, Vec<ModelRow>>,
    key: &[u8],
) -> Result<(), proptest::test_runner::TestCaseError> {
    let limit = ReadLimit::new(3)
        .map_err(|error| proptest::test_runner::TestCaseError::fail(error.to_string()))?;
    let outcome = runtime
        .read_history(&HistoryReadRequest::new(branch(), engine_space(), api_key(key)).limit(limit))
        .map_err(|error| proptest::test_runner::TestCaseError::fail(error.to_string()))?;
    let expected = model
        .get(key)
        .into_iter()
        .flat_map(|rows| rows.iter().rev().take(limit.get()));
    for (actual, expected) in outcome.rows().iter().zip(expected.clone()) {
        assert_storage_row_matches_model(actual, expected)?;
    }
    if outcome.rows().len() != expected.count() {
        return Err(proptest::test_runner::TestCaseError::fail(
            "history row count disagrees with model",
        ));
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn assert_prefix_scan_matches_model(
    runtime: &StorageRuntime<'static>,
    model: &std::collections::BTreeMap<Vec<u8>, Vec<ModelRow>>,
    bound: ReadBound,
) -> Result<(), proptest::test_runner::TestCaseError> {
    let outcome = runtime
        .scan_prefix(&PrefixScanReadRequest::new(
            branch(),
            engine_space(),
            api_key(b"k"),
            bound,
            None,
        ))
        .map_err(|error| proptest::test_runner::TestCaseError::fail(error.to_string()))?;
    let expected = model
        .values()
        .filter_map(|rows| model_visible_row(Some(rows), bound))
        .collect::<Vec<_>>();
    for (actual, expected) in outcome.rows().iter().zip(&expected) {
        assert_storage_row_matches_model(actual, expected)?;
    }
    if outcome.rows().len() != expected.len() {
        return Err(proptest::test_runner::TestCaseError::fail(
            "scan row count disagrees with model",
        ));
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn model_visible_row(rows: Option<&Vec<ModelRow>>, bound: ReadBound) -> Option<&ModelRow> {
    rows?.iter().rev().find(|row| match bound {
        ReadBound::Latest => true,
        ReadBound::AtVersion(version) => row.commit_version <= version,
        ReadBound::AtTimestamp(timestamp) => row.commit_timestamp <= timestamp,
    })
}

#[cfg(not(target_arch = "wasm32"))]
fn assert_api_row_matches_model(
    actual: Option<&StorageReadRow>,
    expected: Option<&ModelRow>,
) -> Result<(), proptest::test_runner::TestCaseError> {
    match (actual, expected) {
        (None, None) => Ok(()),
        (Some(actual), Some(expected)) => assert_storage_row_matches_model(actual, expected),
        _ => Err(proptest::test_runner::TestCaseError::fail(
            "point row presence disagrees with model",
        )),
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn assert_storage_row_matches_model(
    actual: &StorageReadRow,
    expected: &ModelRow,
) -> Result<(), proptest::test_runner::TestCaseError> {
    if actual.key().as_bytes() != expected.key
        || actual.commit_version() != expected.commit_version
        || actual.commit_timestamp() != expected.commit_timestamp
        || actual.value().map(StorageValue::as_bytes) != expected.value.as_deref()
        || actual.is_tombstone() != expected.value.is_none()
    {
        return Err(proptest::test_runner::TestCaseError::fail(
            "row facts disagree with model",
        ));
    }
    Ok(())
}
