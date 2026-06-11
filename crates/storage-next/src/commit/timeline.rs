//! Storage-owned commit timeline row helpers.

use super::{CommitRuntimeError, CommitRuntimeResult, CommitStamp};
use crate::observability::perf_trace;
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use std::collections::hash_map::Entry;
use std::collections::HashMap;
use strata_core_next::{BranchId, CommitVersion, Timestamp};

pub(crate) const COMMIT_TIMELINE_SPACE: &str = "timeline";
const TIMESTAMP_INDEX_PREFIX: &[u8] = b"ts-v1\0";
const VERSION_INDEX_PREFIX: &[u8] = b"ver-v1\0";
const ENCODED_U64_BYTES: usize = 8;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CommitTimelineEntry {
    branch_id: BranchId,
    commit_version: CommitVersion,
    commit_timestamp: Timestamp,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CommitTimelineRows {
    entry: CommitTimelineEntry,
    timestamp_to_version: StorageRow,
    version_to_timestamp: StorageRow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommitTimelineRowKind {
    TimestampToVersion,
    VersionToTimestamp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CommitTimelineFact {
    entry: CommitTimelineEntry,
    kind: CommitTimelineRowKind,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct CommitTimelineBounds {
    // These are independent loose bounds over the retained timeline rows. They
    // do not claim that max_version occurred at max_timestamp, or that retained
    // history is complete between the extrema.
    min_timestamp: Option<Timestamp>,
    max_timestamp: Option<Timestamp>,
    min_version: Option<CommitVersion>,
    max_version: Option<CommitVersion>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CommitTimelineView {
    branch_id: BranchId,
    entries: Vec<CommitTimelineEntry>,
    entries_by_version: Vec<CommitTimelineEntry>,
    bounds: CommitTimelineBounds,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CommitTimelineLookup {
    query_timestamp: Timestamp,
    matched_version: Option<CommitVersion>,
    matched_timestamp: Option<Timestamp>,
    miss: CommitTimelineMiss,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommitTimelineMiss {
    Matched,
    BeforeRetainedHistory,
    AfterLatestRetained,
    Empty,
}

impl CommitTimelineEntry {
    pub(crate) fn new(
        branch_id: BranchId,
        commit_version: CommitVersion,
        commit_timestamp: Timestamp,
    ) -> CommitRuntimeResult<Self> {
        if commit_version == CommitVersion::ZERO {
            return Err(CommitRuntimeError::InvalidTimelineFact {
                reason: "commit version must be nonzero",
            });
        }
        Ok(Self {
            branch_id,
            commit_version,
            commit_timestamp,
        })
    }

    pub(crate) fn from_stamp(stamp: CommitStamp) -> CommitRuntimeResult<Self> {
        Self::new(
            stamp.branch_id(),
            stamp.commit_version(),
            stamp.commit_timestamp(),
        )
    }

    pub(crate) const fn branch_id(self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn commit_version(self) -> CommitVersion {
        self.commit_version
    }

    pub(crate) const fn commit_timestamp(self) -> Timestamp {
        self.commit_timestamp
    }
}

impl CommitTimelineRows {
    pub(crate) fn from_entry(entry: CommitTimelineEntry) -> CommitRuntimeResult<Self> {
        let timestamp_to_version = StorageRow::put(
            timestamp_index_key(entry)?,
            entry.commit_version(),
            entry.commit_timestamp(),
            Timestamp::EPOCH,
            encode_u64(entry.commit_version().as_u64()),
        );
        let version_to_timestamp = StorageRow::put(
            version_index_key(entry)?,
            entry.commit_version(),
            entry.commit_timestamp(),
            Timestamp::EPOCH,
            encode_u64(entry.commit_timestamp().as_micros()),
        );

        Ok(Self {
            entry,
            timestamp_to_version,
            version_to_timestamp,
        })
    }

    pub(crate) const fn entry(&self) -> CommitTimelineEntry {
        self.entry
    }

    pub(crate) const fn timestamp_to_version(&self) -> &StorageRow {
        &self.timestamp_to_version
    }

    pub(crate) const fn version_to_timestamp(&self) -> &StorageRow {
        &self.version_to_timestamp
    }

    pub(crate) fn rows(&self) -> [&StorageRow; 2] {
        [&self.timestamp_to_version, &self.version_to_timestamp]
    }

    pub(crate) fn into_rows(self) -> [StorageRow; 2] {
        [self.timestamp_to_version, self.version_to_timestamp]
    }

    pub(crate) const fn timeline_row_count() -> usize {
        2
    }
}

impl CommitTimelineFact {
    pub(crate) fn validate(row: &StorageRow) -> CommitRuntimeResult<Self> {
        require_timeline_row_shape(row)?;
        let user_key = row.physical_key().user_key();

        if let Some(rest) = user_key.strip_prefix(TIMESTAMP_INDEX_PREFIX) {
            return decode_timestamp_index_fact(row, rest);
        }
        if let Some(rest) = user_key.strip_prefix(VERSION_INDEX_PREFIX) {
            return decode_version_index_fact(row, rest);
        }
        Err(CommitRuntimeError::InvalidTimelineFact {
            reason: "timeline row has unknown key prefix",
        })
    }

    pub(crate) const fn entry(self) -> CommitTimelineEntry {
        self.entry
    }

    pub(crate) const fn kind(self) -> CommitTimelineRowKind {
        self.kind
    }
}

impl CommitTimelineBounds {
    pub(crate) const fn empty() -> Self {
        Self {
            min_timestamp: None,
            max_timestamp: None,
            min_version: None,
            max_version: None,
        }
    }

    pub(crate) const fn min_timestamp(self) -> Option<Timestamp> {
        self.min_timestamp
    }

    pub(crate) const fn max_timestamp(self) -> Option<Timestamp> {
        self.max_timestamp
    }

    pub(crate) const fn min_version(self) -> Option<CommitVersion> {
        self.min_version
    }

    pub(crate) const fn max_version(self) -> Option<CommitVersion> {
        self.max_version
    }
}

impl CommitTimelineView {
    pub(crate) fn from_rows<'a>(
        branch_id: BranchId,
        rows: impl IntoIterator<Item = &'a StorageRow>,
    ) -> CommitRuntimeResult<Self> {
        let mut timestamp_facts = Vec::new();
        let mut version_facts = Vec::new();
        let mut rows_scanned = 0usize;

        for row in rows {
            rows_scanned = rows_scanned.saturating_add(1);
            if row.physical_key().branch_id() != branch_id || !is_timeline_candidate(row) {
                continue;
            }
            let fact = CommitTimelineFact::validate(row)?;
            match fact.kind() {
                CommitTimelineRowKind::TimestampToVersion => timestamp_facts.push(fact.entry()),
                CommitTimelineRowKind::VersionToTimestamp => version_facts.push(fact.entry()),
            }
        }
        perf_trace::record_commit_timeline_view_rows(rows_scanned);
        perf_trace::record_commit_timeline_view_facts(timestamp_facts.len(), version_facts.len());

        let (entries, entries_by_version) =
            reconcile_timeline_facts(&timestamp_facts, &version_facts)?;
        let bounds = timeline_bounds(&entries);
        Ok(Self {
            branch_id,
            entries,
            entries_by_version,
            bounds,
        })
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) fn entries(&self) -> &[CommitTimelineEntry] {
        &self.entries
    }

    pub(crate) const fn bounds(&self) -> CommitTimelineBounds {
        self.bounds
    }

    pub(crate) fn version_at_or_before(&self, query_timestamp: Timestamp) -> CommitTimelineLookup {
        let retained_entries = self.entries.len();
        let lookup_probes = binary_search_probe_bound(retained_entries);
        let Some(first) = self.entries.first() else {
            perf_trace::record_commit_timeline_lookup(0);
            return CommitTimelineLookup::empty(query_timestamp);
        };
        if query_timestamp < first.commit_timestamp() {
            perf_trace::record_commit_timeline_lookup(lookup_probes);
            return CommitTimelineLookup {
                query_timestamp,
                matched_version: None,
                matched_timestamp: None,
                miss: CommitTimelineMiss::BeforeRetainedHistory,
            };
        }

        let latest = self
            .entries
            .last()
            .expect("non-empty entries have a latest entry");
        let upper_bound = self
            .entries
            .partition_point(|entry| entry.commit_timestamp() <= query_timestamp);
        let matched = self
            .entries
            .get(upper_bound.saturating_sub(1))
            .expect("query at or after earliest timestamp must match an entry");
        let miss = if query_timestamp > latest.commit_timestamp() {
            CommitTimelineMiss::AfterLatestRetained
        } else {
            CommitTimelineMiss::Matched
        };
        perf_trace::record_commit_timeline_lookup(lookup_probes);

        CommitTimelineLookup {
            query_timestamp,
            matched_version: Some(matched.commit_version()),
            matched_timestamp: Some(matched.commit_timestamp()),
            miss,
        }
    }

    pub(crate) fn timestamp_for_version(&self, version: CommitVersion) -> Option<Timestamp> {
        self.entries_by_version
            .binary_search_by_key(&version, |entry| entry.commit_version())
            .ok()
            .and_then(|index| self.entries_by_version.get(index))
            .map(|entry| entry.commit_timestamp())
    }
}

impl CommitTimelineLookup {
    const fn empty(query_timestamp: Timestamp) -> Self {
        Self {
            query_timestamp,
            matched_version: None,
            matched_timestamp: None,
            miss: CommitTimelineMiss::Empty,
        }
    }

    pub(crate) const fn query_timestamp(self) -> Timestamp {
        self.query_timestamp
    }

    pub(crate) const fn matched_version(self) -> Option<CommitVersion> {
        self.matched_version
    }

    pub(crate) const fn matched_timestamp(self) -> Option<Timestamp> {
        self.matched_timestamp
    }

    pub(crate) const fn miss(self) -> CommitTimelineMiss {
        self.miss
    }
}

fn timestamp_index_key(entry: CommitTimelineEntry) -> CommitRuntimeResult<PhysicalKey> {
    PhysicalKey::new(
        entry.branch_id(),
        COMMIT_TIMELINE_SPACE,
        StorageSpaceId::COMMIT_TIMELINE,
        timestamp_index_user_key(entry),
    )
    .map_err(|_| CommitRuntimeError::InvalidTimelineFact {
        reason: "timeline physical key is invalid",
    })
}

fn version_index_key(entry: CommitTimelineEntry) -> CommitRuntimeResult<PhysicalKey> {
    PhysicalKey::new(
        entry.branch_id(),
        COMMIT_TIMELINE_SPACE,
        StorageSpaceId::COMMIT_TIMELINE,
        version_index_user_key(entry),
    )
    .map_err(|_| CommitRuntimeError::InvalidTimelineFact {
        reason: "timeline physical key is invalid",
    })
}

fn timestamp_index_user_key(entry: CommitTimelineEntry) -> Vec<u8> {
    let mut key = Vec::with_capacity(TIMESTAMP_INDEX_PREFIX.len() + (ENCODED_U64_BYTES * 2));
    key.extend_from_slice(TIMESTAMP_INDEX_PREFIX);
    key.extend_from_slice(&encode_u64(entry.commit_timestamp().as_micros()));
    key.extend_from_slice(&encode_u64(entry.commit_version().as_u64()));
    key
}

fn version_index_user_key(entry: CommitTimelineEntry) -> Vec<u8> {
    let mut key = Vec::with_capacity(VERSION_INDEX_PREFIX.len() + ENCODED_U64_BYTES);
    key.extend_from_slice(VERSION_INDEX_PREFIX);
    key.extend_from_slice(&encode_u64(entry.commit_version().as_u64()));
    key
}

fn require_timeline_row_shape(row: &StorageRow) -> CommitRuntimeResult<()> {
    if row.physical_key().storage_space_id() != StorageSpaceId::COMMIT_TIMELINE {
        return Err(CommitRuntimeError::InvalidTimelineFact {
            reason: "timeline row uses the wrong storage space",
        });
    }
    if row.physical_key().space() != COMMIT_TIMELINE_SPACE {
        return Err(CommitRuntimeError::InvalidTimelineFact {
            reason: "timeline row uses the wrong physical key space",
        });
    }
    if row.is_tombstone() {
        return Err(CommitRuntimeError::InvalidTimelineFact {
            reason: "timeline row must be a put row",
        });
    }
    if row.expires_at() != Timestamp::EPOCH {
        return Err(CommitRuntimeError::InvalidTimelineFact {
            reason: "timeline row must not expire",
        });
    }
    if row.commit_version() == CommitVersion::ZERO {
        return Err(CommitRuntimeError::InvalidTimelineFact {
            reason: "timeline row version must be nonzero",
        });
    }
    Ok(())
}

fn decode_timestamp_index_fact(
    row: &StorageRow,
    key_rest: &[u8],
) -> CommitRuntimeResult<CommitTimelineFact> {
    if key_rest.len() != ENCODED_U64_BYTES * 2 {
        return Err(CommitRuntimeError::InvalidTimelineFact {
            reason: "timestamp timeline key has invalid length",
        });
    }
    let timestamp = Timestamp::from_micros(decode_u64(&key_rest[..ENCODED_U64_BYTES])?);
    let key_version = CommitVersion::new(decode_u64(&key_rest[ENCODED_U64_BYTES..])?);
    let value_version = CommitVersion::new(decode_exact_value(row.value())?);
    if key_version != value_version {
        return Err(CommitRuntimeError::InvalidTimelineFact {
            reason: "timestamp timeline key and value disagree",
        });
    }
    if row.commit_version() != key_version {
        return Err(CommitRuntimeError::InvalidTimelineFact {
            reason: "timestamp timeline row version disagrees with key",
        });
    }
    if row.commit_timestamp() != timestamp {
        return Err(CommitRuntimeError::InvalidTimelineFact {
            reason: "timestamp timeline row timestamp disagrees with key",
        });
    }
    let entry = CommitTimelineEntry::new(row.physical_key().branch_id(), key_version, timestamp)?;
    Ok(CommitTimelineFact {
        entry,
        kind: CommitTimelineRowKind::TimestampToVersion,
    })
}

fn decode_version_index_fact(
    row: &StorageRow,
    key_rest: &[u8],
) -> CommitRuntimeResult<CommitTimelineFact> {
    if key_rest.len() != ENCODED_U64_BYTES {
        return Err(CommitRuntimeError::InvalidTimelineFact {
            reason: "version timeline key has invalid length",
        });
    }
    let key_version = CommitVersion::new(decode_u64(key_rest)?);
    let timestamp = Timestamp::from_micros(decode_exact_value(row.value())?);
    if row.commit_version() != key_version {
        return Err(CommitRuntimeError::InvalidTimelineFact {
            reason: "version timeline row version disagrees with key",
        });
    }
    if row.commit_timestamp() != timestamp {
        return Err(CommitRuntimeError::InvalidTimelineFact {
            reason: "version timeline row timestamp disagrees with value",
        });
    }
    let entry = CommitTimelineEntry::new(row.physical_key().branch_id(), key_version, timestamp)?;
    Ok(CommitTimelineFact {
        entry,
        kind: CommitTimelineRowKind::VersionToTimestamp,
    })
}

fn is_timeline_candidate(row: &StorageRow) -> bool {
    row.physical_key().storage_space_id() == StorageSpaceId::COMMIT_TIMELINE
}

fn insert_unique_timeline_entry(
    entries: &mut HashMap<CommitVersion, CommitTimelineEntry>,
    entry: CommitTimelineEntry,
    conflict_reason: &'static str,
) -> CommitRuntimeResult<()> {
    match entries.entry(entry.commit_version()) {
        Entry::Vacant(slot) => {
            slot.insert(entry);
            Ok(())
        }
        Entry::Occupied(slot) if *slot.get() == entry => Ok(()),
        Entry::Occupied(_) => Err(CommitRuntimeError::TimelineConflict {
            reason: conflict_reason,
        }),
    }
}

fn reconcile_timeline_facts(
    timestamp_facts: &[CommitTimelineEntry],
    version_facts: &[CommitTimelineEntry],
) -> CommitRuntimeResult<(Vec<CommitTimelineEntry>, Vec<CommitTimelineEntry>)> {
    let mut timestamp_by_version = HashMap::with_capacity(timestamp_facts.len());
    let mut version_by_version = HashMap::with_capacity(version_facts.len());
    let mut entry_checks = 0usize;

    for timestamp_entry in timestamp_facts {
        entry_checks = entry_checks.saturating_add(1);
        if let Err(error) = insert_unique_timeline_entry(
            &mut timestamp_by_version,
            *timestamp_entry,
            "conflicting duplicate timestamp timeline fact",
        ) {
            record_timeline_reconcile(timestamp_facts, version_facts, entry_checks);
            return Err(error);
        }
    }
    for version_entry in version_facts {
        entry_checks = entry_checks.saturating_add(1);
        if let Err(error) = insert_unique_timeline_entry(
            &mut version_by_version,
            *version_entry,
            "conflicting duplicate version timeline fact",
        ) {
            record_timeline_reconcile(timestamp_facts, version_facts, entry_checks);
            return Err(error);
        }
    }

    for timestamp_entry in timestamp_by_version.values() {
        entry_checks = entry_checks.saturating_add(1);
        let Some(version_entry) = version_by_version.get(&timestamp_entry.commit_version()) else {
            record_timeline_reconcile(timestamp_facts, version_facts, entry_checks);
            return Err(CommitRuntimeError::InvalidTimelineFact {
                reason: "timestamp timeline fact is missing version index",
            });
        };
        if version_entry != timestamp_entry {
            record_timeline_reconcile(timestamp_facts, version_facts, entry_checks);
            return Err(CommitRuntimeError::TimelineConflict {
                reason: "timeline indexes disagree for a commit version",
            });
        }
    }

    for version_entry in version_by_version.values() {
        entry_checks = entry_checks.saturating_add(1);
        if !timestamp_by_version.contains_key(&version_entry.commit_version()) {
            record_timeline_reconcile(timestamp_facts, version_facts, entry_checks);
            return Err(CommitRuntimeError::InvalidTimelineFact {
                reason: "version timeline fact is missing timestamp index",
            });
        }
    }
    record_timeline_reconcile(timestamp_facts, version_facts, entry_checks);

    let mut entries = timestamp_by_version
        .into_values()
        .collect::<Vec<CommitTimelineEntry>>();
    entries.sort_by_key(|entry| (entry.commit_timestamp(), entry.commit_version()));
    let mut entries_by_version = entries.clone();
    entries_by_version.sort_by_key(|entry| entry.commit_version());
    Ok((entries, entries_by_version))
}

fn record_timeline_reconcile(
    timestamp_facts: &[CommitTimelineEntry],
    version_facts: &[CommitTimelineEntry],
    entry_checks: usize,
) {
    perf_trace::record_commit_timeline_reconcile(
        timestamp_facts.len(),
        version_facts.len(),
        entry_checks,
    );
}

fn binary_search_probe_bound(retained_entries: usize) -> usize {
    if retained_entries == 0 {
        return 0;
    }
    usize::BITS as usize - retained_entries.leading_zeros() as usize
}

fn timeline_bounds(entries: &[CommitTimelineEntry]) -> CommitTimelineBounds {
    if entries.is_empty() {
        return CommitTimelineBounds::empty();
    }
    let min_timestamp = entries.iter().map(|entry| entry.commit_timestamp()).min();
    let max_timestamp = entries.iter().map(|entry| entry.commit_timestamp()).max();
    let min_version = entries.iter().map(|entry| entry.commit_version()).min();
    let max_version = entries.iter().map(|entry| entry.commit_version()).max();
    CommitTimelineBounds {
        min_timestamp,
        max_timestamp,
        min_version,
        max_version,
    }
}

const fn encode_u64(value: u64) -> [u8; ENCODED_U64_BYTES] {
    value.to_be_bytes()
}

fn decode_exact_value(bytes: &[u8]) -> CommitRuntimeResult<u64> {
    if bytes.len() != ENCODED_U64_BYTES {
        return Err(CommitRuntimeError::InvalidTimelineFact {
            reason: "timeline value has invalid length",
        });
    }
    decode_u64(bytes)
}

fn decode_u64(bytes: &[u8]) -> CommitRuntimeResult<u64> {
    let array = <[u8; ENCODED_U64_BYTES]>::try_from(bytes).map_err(|_| {
        CommitRuntimeError::InvalidTimelineFact {
            reason: "timeline integer has invalid length",
        }
    })?;
    Ok(u64::from_be_bytes(array))
}
