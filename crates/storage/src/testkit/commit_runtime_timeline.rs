//! Generated commit timeline substrate contract helpers.

use crate::commit::{
    CommitBatch, CommitBatchOptions, CommitExpiry, CommitMutation, CommitRetentionHint,
    CommitRuntimeConfig, CommitRuntimeError, CommitTimelineEntry, CommitTimelineFact,
    CommitTimelineMiss, CommitTimelineRowKind, CommitTimelineRows, CommitTimelineView,
    CommitValidationFacts, COMMIT_TIMELINE_SPACE,
};
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use strata_core::{BranchId, CommitVersion, Timestamp};

use super::TestkitError;

pub(crate) struct CommitRuntimeTimelineContract {
    pub(crate) valid_entries: usize,
    pub(crate) zero_version_rejections: usize,
    pub(crate) timestamp_index_keys: usize,
    pub(crate) version_index_keys: usize,
    pub(crate) row_pairs: usize,
    pub(crate) shared_commit_facts: usize,
    pub(crate) timestamp_index_decodes: usize,
    pub(crate) version_index_decodes: usize,
    pub(crate) malformed_prefix_rejections: usize,
    pub(crate) malformed_key_length_rejections: usize,
    pub(crate) value_length_rejections: usize,
    pub(crate) key_value_mismatch_rejections: usize,
    pub(crate) timestamp_lookup_exact_matches: usize,
    pub(crate) timestamp_lookup_between_matches: usize,
    pub(crate) duplicate_timestamp_tiebreaks: usize,
    pub(crate) version_timestamp_lookups: usize,
    pub(crate) branch_isolations: usize,
    pub(crate) row_order_independence: usize,
    pub(crate) bounds_reports: usize,
    pub(crate) caller_rejections: usize,
}

pub(crate) fn check_commit_runtime_timeline_contract(
    script: &[u8],
) -> Result<CommitRuntimeTimelineContract, TestkitError> {
    let branch = branch_id(script_byte(script, 54));
    let base_timestamp = 1 + u64::from(script_byte(script, 55));
    let entry = timeline_entry(branch, 1, base_timestamp)?;
    let rows = CommitTimelineRows::from_entry(entry).map_err(testkit_error)?;

    Ok(CommitRuntimeTimelineContract {
        valid_entries: check_valid_entry(entry)?,
        zero_version_rejections: check_zero_version_rejection(branch)?,
        timestamp_index_keys: check_timestamp_index_key(rows.timestamp_to_version(), entry)?,
        version_index_keys: check_version_index_key(rows.version_to_timestamp(), entry)?,
        row_pairs: check_row_pair_count()?,
        shared_commit_facts: check_shared_commit_facts(&rows, entry)?,
        timestamp_index_decodes: check_timestamp_decode(rows.timestamp_to_version(), entry)?,
        version_index_decodes: check_version_decode(rows.version_to_timestamp(), entry)?,
        malformed_prefix_rejections: check_malformed_prefix_rejection(entry)?,
        malformed_key_length_rejections: check_malformed_key_length_rejection(entry)?,
        value_length_rejections: check_value_length_rejection(rows.timestamp_to_version(), entry)?,
        key_value_mismatch_rejections: check_key_value_mismatch(
            rows.timestamp_to_version(),
            entry,
        )?,
        timestamp_lookup_exact_matches: check_timestamp_lookup_exact(branch, base_timestamp)?,
        timestamp_lookup_between_matches: check_timestamp_lookup_between(branch, base_timestamp)?,
        duplicate_timestamp_tiebreaks: check_duplicate_timestamp_tiebreak(branch, base_timestamp)?,
        version_timestamp_lookups: check_version_lookup(branch, base_timestamp)?,
        branch_isolations: check_branch_isolation(branch, base_timestamp)?,
        row_order_independence: check_row_order_independence(branch, base_timestamp)?,
        bounds_reports: check_bounds(branch, base_timestamp)?,
        caller_rejections: check_caller_rejections(branch)?,
    })
}

fn check_valid_entry(entry: CommitTimelineEntry) -> Result<usize, TestkitError> {
    if entry.commit_version() == CommitVersion::ZERO {
        return Err(TestkitError::new("valid timeline entry used zero version"));
    }
    Ok(1)
}

fn check_zero_version_rejection(branch: BranchId) -> Result<usize, TestkitError> {
    match CommitTimelineEntry::new(branch, CommitVersion::ZERO, Timestamp::EPOCH) {
        Err(CommitRuntimeError::InvalidTimelineFact { .. }) => Ok(1),
        other => Err(TestkitError::new(format!(
            "zero timeline version was not rejected as invalid timeline fact: {other:?}"
        ))),
    }
}

fn check_timestamp_index_key(
    row: &StorageRow,
    entry: CommitTimelineEntry,
) -> Result<usize, TestkitError> {
    let key = row.physical_key();
    if key.branch_id() != entry.branch_id()
        || key.space() != COMMIT_TIMELINE_SPACE
        || key.storage_space_id() != StorageSpaceId::COMMIT_TIMELINE
        || !key.user_key().starts_with(b"ts-v1\0")
    {
        return Err(TestkitError::new(
            "timestamp timeline key did not preserve storage facts",
        ));
    }
    Ok(1)
}

fn check_version_index_key(
    row: &StorageRow,
    entry: CommitTimelineEntry,
) -> Result<usize, TestkitError> {
    let key = row.physical_key();
    if key.branch_id() != entry.branch_id()
        || key.space() != COMMIT_TIMELINE_SPACE
        || key.storage_space_id() != StorageSpaceId::COMMIT_TIMELINE
        || !key.user_key().starts_with(b"ver-v1\0")
    {
        return Err(TestkitError::new(
            "version timeline key did not preserve storage facts",
        ));
    }
    Ok(1)
}

fn check_row_pair_count() -> Result<usize, TestkitError> {
    if CommitTimelineRows::timeline_row_count() != 2 {
        return Err(TestkitError::new("timeline row count was not exactly two"));
    }
    Ok(1)
}

fn check_shared_commit_facts(
    rows: &CommitTimelineRows,
    entry: CommitTimelineEntry,
) -> Result<usize, TestkitError> {
    for row in rows.rows() {
        if row.commit_version() != entry.commit_version()
            || row.commit_timestamp() != entry.commit_timestamp()
            || row.expires_at() != Timestamp::EPOCH
            || row.is_tombstone()
        {
            return Err(TestkitError::new(
                "timeline row did not preserve entry commit facts",
            ));
        }
    }
    Ok(1)
}

fn check_timestamp_decode(
    row: &StorageRow,
    entry: CommitTimelineEntry,
) -> Result<usize, TestkitError> {
    let fact = CommitTimelineFact::validate(row).map_err(testkit_error)?;
    if fact.entry() != entry || fact.kind() != CommitTimelineRowKind::TimestampToVersion {
        return Err(TestkitError::new(
            "timestamp timeline fact decoded incorrectly",
        ));
    }
    Ok(1)
}

fn check_version_decode(
    row: &StorageRow,
    entry: CommitTimelineEntry,
) -> Result<usize, TestkitError> {
    let fact = CommitTimelineFact::validate(row).map_err(testkit_error)?;
    if fact.entry() != entry || fact.kind() != CommitTimelineRowKind::VersionToTimestamp {
        return Err(TestkitError::new(
            "version timeline fact decoded incorrectly",
        ));
    }
    Ok(1)
}

fn check_malformed_prefix_rejection(entry: CommitTimelineEntry) -> Result<usize, TestkitError> {
    let row = StorageRow::put(
        storage_owned_key(entry.branch_id(), b"bad-prefix".to_vec()),
        entry.commit_version(),
        entry.commit_timestamp(),
        Timestamp::EPOCH,
        entry.commit_version().as_u64().to_be_bytes(),
    );
    expect_invalid_timeline_fact(CommitTimelineFact::validate(&row))?;
    Ok(1)
}

fn check_malformed_key_length_rejection(entry: CommitTimelineEntry) -> Result<usize, TestkitError> {
    let mut timestamp_key = b"ts-v1\0".to_vec();
    timestamp_key.extend_from_slice(&entry.commit_timestamp().as_micros().to_be_bytes());
    let timestamp_row = StorageRow::put(
        storage_owned_key(entry.branch_id(), timestamp_key),
        entry.commit_version(),
        entry.commit_timestamp(),
        Timestamp::EPOCH,
        entry.commit_version().as_u64().to_be_bytes(),
    );
    expect_invalid_timeline_fact(CommitTimelineFact::validate(&timestamp_row))?;

    let mut version_key = b"ver-v1\0".to_vec();
    version_key.extend_from_slice(&entry.commit_version().as_u64().to_be_bytes());
    version_key.push(0);
    let version_row = StorageRow::put(
        storage_owned_key(entry.branch_id(), version_key),
        entry.commit_version(),
        entry.commit_timestamp(),
        Timestamp::EPOCH,
        entry.commit_timestamp().as_micros().to_be_bytes(),
    );
    expect_invalid_timeline_fact(CommitTimelineFact::validate(&version_row))?;
    Ok(2)
}

fn check_value_length_rejection(
    timestamp_row: &StorageRow,
    entry: CommitTimelineEntry,
) -> Result<usize, TestkitError> {
    let row = StorageRow::put(
        timestamp_row.physical_key().clone(),
        entry.commit_version(),
        entry.commit_timestamp(),
        Timestamp::EPOCH,
        [1, 2, 3],
    );
    expect_invalid_timeline_fact(CommitTimelineFact::validate(&row))?;
    Ok(1)
}

fn check_key_value_mismatch(
    timestamp_row: &StorageRow,
    entry: CommitTimelineEntry,
) -> Result<usize, TestkitError> {
    let row = StorageRow::put(
        timestamp_row.physical_key().clone(),
        entry.commit_version(),
        entry.commit_timestamp(),
        Timestamp::EPOCH,
        entry
            .commit_version()
            .as_u64()
            .saturating_add(1)
            .to_be_bytes(),
    );
    expect_invalid_timeline_fact(CommitTimelineFact::validate(&row))?;
    Ok(1)
}

fn check_timestamp_lookup_exact(branch: BranchId, base: u64) -> Result<usize, TestkitError> {
    let view = timeline_view(branch, base)?;
    let lookup = view.version_at_or_before(Timestamp::from_micros(base + 10));
    expect_lookup(
        lookup,
        Some(3),
        Some(base + 10),
        CommitTimelineMiss::Matched,
    )?;
    Ok(1)
}

fn check_timestamp_lookup_between(branch: BranchId, base: u64) -> Result<usize, TestkitError> {
    let view = timeline_view(branch, base)?;
    let lookup = view.version_at_or_before(Timestamp::from_micros(base + 15));
    expect_lookup(
        lookup,
        Some(3),
        Some(base + 10),
        CommitTimelineMiss::Matched,
    )?;
    Ok(1)
}

fn check_duplicate_timestamp_tiebreak(branch: BranchId, base: u64) -> Result<usize, TestkitError> {
    let view = timeline_view(branch, base)?;
    let lookup = view.version_at_or_before(Timestamp::from_micros(base + 10));
    expect_lookup(
        lookup,
        Some(3),
        Some(base + 10),
        CommitTimelineMiss::Matched,
    )?;
    Ok(1)
}

fn check_version_lookup(branch: BranchId, base: u64) -> Result<usize, TestkitError> {
    let view = timeline_view(branch, base)?;
    if view.timestamp_for_version(CommitVersion::new(2)) != Some(Timestamp::from_micros(base + 10))
        || view.timestamp_for_version(CommitVersion::new(99)).is_some()
    {
        return Err(TestkitError::new("timeline version lookup was incorrect"));
    }
    Ok(1)
}

fn check_branch_isolation(branch: BranchId, base: u64) -> Result<usize, TestkitError> {
    let other = branch_id(branch.as_bytes()[0].wrapping_add(1));
    let mut rows = timeline_rows(branch, base)?;
    rows.extend(timeline_rows(other, base + 100)?);

    let view = CommitTimelineView::from_rows(branch, rows.iter()).map_err(testkit_error)?;
    if view.branch_id() != branch || view.entries().len() != 4 {
        return Err(TestkitError::new("timeline view leaked branch rows"));
    }
    Ok(1)
}

fn check_row_order_independence(branch: BranchId, base: u64) -> Result<usize, TestkitError> {
    let mut rows = timeline_rows(branch, base)?;
    rows.reverse();
    let view = CommitTimelineView::from_rows(branch, rows.iter()).map_err(testkit_error)?;
    let lookup = view.version_at_or_before(Timestamp::from_micros(base + 10));
    expect_lookup(
        lookup,
        Some(3),
        Some(base + 10),
        CommitTimelineMiss::Matched,
    )?;
    Ok(1)
}

fn check_bounds(branch: BranchId, base: u64) -> Result<usize, TestkitError> {
    let view = timeline_view(branch, base)?;
    let bounds = view.bounds();
    if bounds.min_timestamp() != Some(Timestamp::from_micros(base))
        || bounds.max_timestamp() != Some(Timestamp::from_micros(base + 30))
        || bounds.min_version() != Some(CommitVersion::new(1))
        || bounds.max_version() != Some(CommitVersion::new(4))
    {
        return Err(TestkitError::new("timeline bounds were incorrect"));
    }
    Ok(1)
}

fn check_caller_rejections(branch: BranchId) -> Result<usize, TestkitError> {
    let put = CommitBatch::mutating(
        branch,
        vec![CommitMutation::put(
            storage_owned_key(branch, b"timeline-put".to_vec()),
            b"timeline",
            CommitExpiry::None,
            CommitRetentionHint::Append,
        )],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    )
    .validate(&CommitRuntimeConfig::default());
    let delete = CommitBatch::mutating(
        branch,
        vec![CommitMutation::delete(storage_owned_key(
            branch,
            b"timeline-delete".to_vec(),
        ))],
        CommitValidationFacts::empty(),
        CommitBatchOptions::default(),
    )
    .validate(&CommitRuntimeConfig::default());

    expect_storage_owned_rejection(put)?;
    expect_storage_owned_rejection(delete)?;
    Ok(2)
}

fn timeline_view(branch: BranchId, base: u64) -> Result<CommitTimelineView, TestkitError> {
    let rows = timeline_rows(branch, base)?;
    CommitTimelineView::from_rows(branch, rows.iter()).map_err(testkit_error)
}

fn timeline_rows(branch: BranchId, base: u64) -> Result<Vec<StorageRow>, TestkitError> {
    let entries = [
        timeline_entry(branch, 1, base)?,
        timeline_entry(branch, 2, base + 10)?,
        timeline_entry(branch, 3, base + 10)?,
        timeline_entry(branch, 4, base + 30)?,
    ];
    Ok(entries
        .into_iter()
        .flat_map(|entry| {
            CommitTimelineRows::from_entry(entry)
                .expect("testkit timeline entry should encode")
                .into_rows()
        })
        .collect())
}

fn expect_lookup(
    lookup: crate::commit::CommitTimelineLookup,
    expected_version: Option<u64>,
    expected_timestamp: Option<u64>,
    expected_miss: CommitTimelineMiss,
) -> Result<(), TestkitError> {
    if lookup.matched_version() != expected_version.map(CommitVersion::new)
        || lookup.matched_timestamp() != expected_timestamp.map(Timestamp::from_micros)
        || lookup.miss() != expected_miss
    {
        return Err(TestkitError::new("timeline lookup did not match model"));
    }
    Ok(())
}

fn timeline_entry(
    branch: BranchId,
    version: u64,
    timestamp: u64,
) -> Result<CommitTimelineEntry, TestkitError> {
    CommitTimelineEntry::new(
        branch,
        CommitVersion::new(version),
        Timestamp::from_micros(timestamp),
    )
    .map_err(testkit_error)
}

fn storage_owned_key(branch_id: BranchId, user_key: Vec<u8>) -> PhysicalKey {
    PhysicalKey::new(
        branch_id,
        COMMIT_TIMELINE_SPACE,
        StorageSpaceId::COMMIT_TIMELINE,
        user_key,
    )
    .expect("storage-owned physical key")
}

fn branch_id(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}

fn script_byte(script: &[u8], index: usize) -> u8 {
    script.get(index).copied().unwrap_or(0)
}

fn expect_invalid_timeline_fact<T>(
    result: Result<T, CommitRuntimeError>,
) -> Result<(), TestkitError> {
    match result {
        Err(CommitRuntimeError::InvalidTimelineFact { .. }) => Ok(()),
        Err(error) => Err(TestkitError::new(format!(
            "expected invalid timeline fact, got {error}"
        ))),
        Ok(_) => Err(TestkitError::new(
            "expected invalid timeline fact, got success",
        )),
    }
}

fn expect_storage_owned_rejection<T>(
    result: Result<T, CommitRuntimeError>,
) -> Result<(), TestkitError> {
    match result {
        Err(CommitRuntimeError::StorageOwnedMutationSpace {
            space_id: StorageSpaceId::COMMIT_TIMELINE,
        }) => Ok(()),
        Err(error) => Err(TestkitError::new(format!(
            "expected storage-owned timeline rejection, got {error}"
        ))),
        Ok(_) => Err(TestkitError::new(
            "expected storage-owned timeline rejection, got success",
        )),
    }
}

#[expect(clippy::needless_pass_by_value, reason = "used directly with map_err")]
fn testkit_error(error: CommitRuntimeError) -> TestkitError {
    TestkitError::new(error.to_string())
}
