use super::*;

use crate::branch::BranchTimestampCoverage;
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
    CommitBatch::new(
        branch(),
        vec![CommitMutation::Put {
            storage_space: engine_space(),
            key: api_key(key),
            value: StorageValue::new(value.to_vec()),
            ttl: None,
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
