//! Model-backed API read contract.

use std::collections::BTreeMap;

use strata_core_next::{BranchId, CommitVersion, Timestamp};

use crate::api::{
    CommitBatch, CommitMutation, CommitOptions, CommitSummary, HistoryReadRequest,
    PointReadRequest, PrefixScanReadRequest, ReadBound, ReadLimit, ScanRange, ScanReadRequest,
    StorageApiErrorClass, StorageKey, StorageOpenOptions, StorageReadRow, StorageRuntime,
    StorageSpaceId, StorageValue, TimestampLookupRequest, VersionLookupRequest,
};
use crate::testkit::TestkitError;

const DEFAULT_BRANCH_ID: BranchId = BranchId::from_bytes([0x01; BranchId::BYTE_LEN]);
const ENGINE_STORAGE_SPACE: u8 = 0x20;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StorageApiReadModelOutcome {
    scripts: usize,
    puts: usize,
    deletes: usize,
    point_reads: usize,
    history_reads: usize,
    prefix_scans: usize,
    range_scans: usize,
    timestamp_lookups: usize,
    retained_history_misses: usize,
}

impl StorageApiReadModelOutcome {
    pub const fn scripts(self) -> usize {
        self.scripts
    }

    pub const fn puts(self) -> usize {
        self.puts
    }

    pub const fn deletes(self) -> usize {
        self.deletes
    }

    pub const fn point_reads(self) -> usize {
        self.point_reads
    }

    pub const fn history_reads(self) -> usize {
        self.history_reads
    }

    pub const fn prefix_scans(self) -> usize {
        self.prefix_scans
    }

    pub const fn range_scans(self) -> usize {
        self.range_scans
    }

    pub const fn timestamp_lookups(self) -> usize {
        self.timestamp_lookups
    }

    pub const fn retained_history_misses(self) -> usize {
        self.retained_history_misses
    }
}

type ModelRow = (Vec<u8>, Option<Vec<u8>>, CommitVersion, Timestamp);

pub fn check_storage_api_read_model_contract(
    script: &[u8],
) -> Result<StorageApiReadModelOutcome, TestkitError> {
    let mut runtime = StorageRuntime::open(StorageOpenOptions::default())
        .map_err(testkit_error)?
        .into_runtime();
    let mut model = BTreeMap::<Vec<u8>, Vec<ModelRow>>::new();
    let mut outcome = StorageApiReadModelOutcome {
        scripts: 1,
        ..StorageApiReadModelOutcome::default()
    };
    let script = if script.is_empty() {
        &[0_u8][..]
    } else {
        script
    };

    for (index, chunk) in script.chunks(4).take(24).enumerate() {
        apply_script_chunk(&mut runtime, &mut model, &mut outcome, index, chunk)?;
    }

    if outcome.deletes == 0 {
        apply_forced_delete(&mut runtime, &mut model, &mut outcome, script[0])?;
    }

    assert_retained_history_misses(&runtime, &mut outcome)?;

    Ok(outcome)
}

fn apply_script_chunk(
    runtime: &mut StorageRuntime<'_>,
    model: &mut BTreeMap<Vec<u8>, Vec<ModelRow>>,
    outcome: &mut StorageApiReadModelOutcome,
    index: usize,
    chunk: &[u8],
) -> Result<(), TestkitError> {
    let key = vec![b'k', b'0' + chunk.get(1).copied().unwrap_or(0) % 4];
    let commit_timestamp = Timestamp::from_micros(10 + u64::try_from(index).unwrap() * 10);
    let value =
        (index == 0 || chunk[0] % 4 != 0).then(|| vec![b'v', chunk.get(2).copied().unwrap_or(0)]);
    let summary = commit_model_row(
        runtime,
        model,
        outcome,
        key.clone(),
        value,
        commit_timestamp,
    )?;
    assert_model_surfaces(runtime, model, outcome, &key, summary)
}

fn apply_forced_delete(
    runtime: &mut StorageRuntime<'_>,
    model: &mut BTreeMap<Vec<u8>, Vec<ModelRow>>,
    outcome: &mut StorageApiReadModelOutcome,
    selector: u8,
) -> Result<(), TestkitError> {
    let key = vec![b'k', b'0' + selector % 4];
    let commit_timestamp =
        Timestamp::from_micros(10 + u64::try_from(outcome.puts + outcome.deletes).unwrap() * 10);
    let summary = commit_model_row(runtime, model, outcome, key.clone(), None, commit_timestamp)?;
    assert_model_surfaces(runtime, model, outcome, &key, summary)
}

fn commit_model_row(
    runtime: &mut StorageRuntime<'_>,
    model: &mut BTreeMap<Vec<u8>, Vec<ModelRow>>,
    outcome: &mut StorageApiReadModelOutcome,
    key: Vec<u8>,
    value: Option<Vec<u8>>,
    commit_timestamp: Timestamp,
) -> Result<CommitSummary, TestkitError> {
    let summary = if let Some(value) = &value {
        outcome.puts += 1;
        runtime
            .commit_for_test(&put_batch(&key, value)?, commit_timestamp)
            .map_err(testkit_error)?
    } else {
        outcome.deletes += 1;
        runtime
            .commit_for_test(&delete_batch(&key)?, commit_timestamp)
            .map_err(testkit_error)?
    };
    model.entry(key.clone()).or_default().push((
        key,
        value,
        summary.commit_version(),
        summary.commit_timestamp(),
    ));
    Ok(summary)
}

fn assert_model_surfaces(
    runtime: &StorageRuntime<'_>,
    model: &BTreeMap<Vec<u8>, Vec<ModelRow>>,
    outcome: &mut StorageApiReadModelOutcome,
    key: &[u8],
    summary: CommitSummary,
) -> Result<(), TestkitError> {
    assert_point_matches_model(runtime, model, key, ReadBound::Latest)?;
    outcome.point_reads += 1;
    assert_point_matches_model(
        runtime,
        model,
        key,
        ReadBound::AtVersion(summary.commit_version()),
    )?;
    outcome.point_reads += 1;
    assert_point_matches_model(
        runtime,
        model,
        key,
        ReadBound::AtTimestamp(summary.commit_timestamp()),
    )?;
    outcome.point_reads += 1;
    assert_history_matches_model(runtime, model, key)?;
    outcome.history_reads += 1;
    assert_prefix_scan_matches_model(runtime, model, ReadBound::Latest)?;
    outcome.prefix_scans += 1;
    assert_range_scan_matches_model(runtime, model, ReadBound::Latest)?;
    outcome.range_scans += 1;
    assert_timeline_matches_summary(runtime, summary)?;
    outcome.timestamp_lookups += 2;
    Ok(())
}

fn assert_timeline_matches_summary(
    runtime: &StorageRuntime<'_>,
    summary: CommitSummary,
) -> Result<(), TestkitError> {
    let timestamp_lookup = runtime
        .lookup_version_at_or_before_timestamp(TimestampLookupRequest::new(
            DEFAULT_BRANCH_ID,
            summary.commit_timestamp(),
        ))
        .map_err(testkit_error)?;
    if timestamp_lookup.matched_version() != summary.commit_version() {
        return Err(TestkitError::new(
            "timestamp lookup disagrees with generated model",
        ));
    }

    let version_lookup = runtime
        .lookup_timestamp_for_version(VersionLookupRequest::new(
            DEFAULT_BRANCH_ID,
            summary.commit_version(),
        ))
        .map_err(testkit_error)?;
    if version_lookup.timestamp() != summary.commit_timestamp() {
        return Err(TestkitError::new(
            "version lookup disagrees with generated model",
        ));
    }
    Ok(())
}

fn assert_retained_history_misses(
    runtime: &StorageRuntime<'_>,
    outcome: &mut StorageApiReadModelOutcome,
) -> Result<(), TestkitError> {
    let before_retained = runtime
        .read_point(&point_request(
            b"k0",
            ReadBound::AtVersion(CommitVersion::ZERO),
        )?)
        .expect_err("zero version should be outside retained history");
    if before_retained.class() != StorageApiErrorClass::HistoryUnavailable {
        return Err(TestkitError::new(
            "zero-version read returned the wrong error class",
        ));
    }
    outcome.retained_history_misses += 1;

    let before_timestamp = runtime
        .lookup_version_at_or_before_timestamp(TimestampLookupRequest::new(
            DEFAULT_BRANCH_ID,
            Timestamp::from_micros(0),
        ))
        .expect_err("timestamp before retained history should fail");
    if before_timestamp.class() != StorageApiErrorClass::HistoryUnavailable {
        return Err(TestkitError::new(
            "before-retained timestamp lookup returned the wrong error class",
        ));
    }
    outcome.retained_history_misses += 1;
    Ok(())
}

fn put_batch(key: &[u8], value: &[u8]) -> Result<CommitBatch, TestkitError> {
    CommitBatch::new(
        DEFAULT_BRANCH_ID,
        vec![CommitMutation::Put {
            storage_space: engine_space()?,
            key: api_key(key)?,
            value: StorageValue::new(value.to_vec()),
            ttl: None,
        }],
        CommitOptions::default().require_conflict_check(false),
    )
    .map_err(testkit_error)
}

fn delete_batch(key: &[u8]) -> Result<CommitBatch, TestkitError> {
    CommitBatch::new(
        DEFAULT_BRANCH_ID,
        vec![CommitMutation::Delete {
            storage_space: engine_space()?,
            key: api_key(key)?,
        }],
        CommitOptions::default().require_conflict_check(false),
    )
    .map_err(testkit_error)
}

fn point_request(key: &[u8], bound: ReadBound) -> Result<PointReadRequest, TestkitError> {
    Ok(PointReadRequest::new(
        DEFAULT_BRANCH_ID,
        engine_space()?,
        api_key(key)?,
        bound,
    ))
}

fn assert_point_matches_model(
    runtime: &StorageRuntime<'_>,
    model: &BTreeMap<Vec<u8>, Vec<ModelRow>>,
    key: &[u8],
    bound: ReadBound,
) -> Result<(), TestkitError> {
    let outcome = runtime
        .read_point(&point_request(key, bound)?)
        .map_err(testkit_error)?;
    let expected = model_visible_row(model.get(key), bound);
    assert_api_row_matches_model(outcome.row(), expected)
}

fn assert_history_matches_model(
    runtime: &StorageRuntime<'_>,
    model: &BTreeMap<Vec<u8>, Vec<ModelRow>>,
    key: &[u8],
) -> Result<(), TestkitError> {
    let limit = ReadLimit::new(3).map_err(testkit_error)?;
    let outcome = runtime
        .read_history(
            &HistoryReadRequest::new(DEFAULT_BRANCH_ID, engine_space()?, api_key(key)?)
                .limit(limit),
        )
        .map_err(testkit_error)?;
    let expected = model
        .get(key)
        .into_iter()
        .flat_map(|rows| rows.iter().rev().take(limit.get()));
    for (actual, expected) in outcome.rows().iter().zip(expected.clone()) {
        assert_storage_row_matches_model(actual, expected)?;
    }
    if outcome.rows().len() != expected.count() {
        return Err(TestkitError::new("history row count disagrees with model"));
    }
    Ok(())
}

fn assert_prefix_scan_matches_model(
    runtime: &StorageRuntime<'_>,
    model: &BTreeMap<Vec<u8>, Vec<ModelRow>>,
    bound: ReadBound,
) -> Result<(), TestkitError> {
    let outcome = runtime
        .scan_prefix(&PrefixScanReadRequest::new(
            DEFAULT_BRANCH_ID,
            engine_space()?,
            api_key(b"k")?,
            bound,
            None,
        ))
        .map_err(testkit_error)?;
    let expected = model
        .values()
        .filter_map(|rows| model_visible_row(Some(rows), bound))
        .collect::<Vec<_>>();
    assert_scan_matches_model(outcome.rows(), &expected)
}

fn assert_range_scan_matches_model(
    runtime: &StorageRuntime<'_>,
    model: &BTreeMap<Vec<u8>, Vec<ModelRow>>,
    bound: ReadBound,
) -> Result<(), TestkitError> {
    let range =
        ScanRange::new(Some(api_key(b"k0")?), Some(api_key(b"kz")?)).map_err(testkit_error)?;
    let outcome = runtime
        .scan_range(&ScanReadRequest::new(
            DEFAULT_BRANCH_ID,
            engine_space()?,
            range,
            bound,
            None,
        ))
        .map_err(testkit_error)?;
    let expected = model
        .range(b"k0".to_vec()..b"kz".to_vec())
        .filter_map(|(_, rows)| model_visible_row(Some(rows), bound))
        .collect::<Vec<_>>();
    assert_scan_matches_model(outcome.rows(), &expected)
}

fn assert_scan_matches_model(
    actual: &[StorageReadRow],
    expected: &[&ModelRow],
) -> Result<(), TestkitError> {
    for (actual, expected) in actual.iter().zip(expected) {
        assert_storage_row_matches_model(actual, expected)?;
    }
    if actual.len() != expected.len() {
        return Err(TestkitError::new("scan row count disagrees with model"));
    }
    Ok(())
}

fn model_visible_row(rows: Option<&Vec<ModelRow>>, bound: ReadBound) -> Option<&ModelRow> {
    rows?.iter().rev().find(|row| match bound {
        ReadBound::Latest => true,
        ReadBound::AtVersion(version) => model_row_version(row) <= version,
        ReadBound::AtTimestamp(timestamp) => model_row_timestamp(row) <= timestamp,
    })
}

fn assert_api_row_matches_model(
    actual: Option<&StorageReadRow>,
    expected: Option<&ModelRow>,
) -> Result<(), TestkitError> {
    match (actual, expected) {
        (None, None) => Ok(()),
        (Some(actual), Some(expected)) => assert_storage_row_matches_model(actual, expected),
        _ => Err(TestkitError::new("point row presence disagrees with model")),
    }
}

fn assert_storage_row_matches_model(
    actual: &StorageReadRow,
    expected: &ModelRow,
) -> Result<(), TestkitError> {
    if actual.key().as_bytes() != model_row_key(expected)
        || actual.commit_version() != model_row_version(expected)
        || actual.commit_timestamp() != model_row_timestamp(expected)
        || actual.value().map(StorageValue::as_bytes) != model_row_value(expected)
        || actual.is_tombstone() != model_row_value(expected).is_none()
    {
        return Err(TestkitError::new("row facts disagree with model"));
    }
    Ok(())
}

fn model_row_key(row: &ModelRow) -> &[u8] {
    &row.0
}

fn model_row_value(row: &ModelRow) -> Option<&[u8]> {
    row.1.as_deref()
}

fn model_row_version(row: &ModelRow) -> CommitVersion {
    row.2
}

fn model_row_timestamp(row: &ModelRow) -> Timestamp {
    row.3
}

fn engine_space() -> Result<StorageSpaceId, TestkitError> {
    StorageSpaceId::new(vec![ENGINE_STORAGE_SPACE]).map_err(testkit_error)
}

fn api_key(bytes: &[u8]) -> Result<StorageKey, TestkitError> {
    StorageKey::new(bytes.to_vec()).map_err(testkit_error)
}

fn testkit_error(error: impl std::fmt::Display) -> TestkitError {
    TestkitError::new(error.to_string())
}
