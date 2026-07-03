//! Mutable and frozen in-memory tables.

use super::{
    MemoryTableCursor, TableInternalKeyBytes, TableKeyBounds, TablePhysicalKeyBytes,
    TablePreparedPointLookup, TableRow, TableRuntimeError, TableRuntimeResult,
};
use crate::observability::perf_trace;
use crate::row::StorageRow;
use crate::row::{InternalKey, PhysicalKey};
use std::collections::BTreeMap;
use std::ops::Bound::{Included, Unbounded};
use std::sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard};
use strata_core_next::{CommitVersion, Timestamp};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableMemoryFacts {
    row_count: usize,
    approximate_size_bytes: usize,
    first_key: Option<Vec<u8>>,
    last_key: Option<Vec<u8>>,
    min_commit: Option<CommitVersion>,
    max_commit: Option<CommitVersion>,
}

impl TableMemoryFacts {
    pub(crate) const fn row_count(&self) -> usize {
        self.row_count
    }

    pub(crate) const fn approximate_size_bytes(&self) -> usize {
        self.approximate_size_bytes
    }

    pub(crate) fn first_key(&self) -> Option<&[u8]> {
        self.first_key.as_deref()
    }

    pub(crate) fn last_key(&self) -> Option<&[u8]> {
        self.last_key.as_deref()
    }

    pub(crate) const fn min_commit(&self) -> Option<CommitVersion> {
        self.min_commit
    }

    pub(crate) const fn max_commit(&self) -> Option<CommitVersion> {
        self.max_commit
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct TableMemoryEntry {
    pub(super) row: Arc<TableRow>,
    pub(super) sequence: u64,
}

#[derive(Debug, Default, Eq, PartialEq)]
pub(super) struct TableMemoryData {
    pub(super) rows: BTreeMap<TableInternalKeyBytes, TableMemoryEntry>,
    approximate_size_bytes: usize,
    min_commit: Option<CommitVersion>,
    max_commit: Option<CommitVersion>,
    next_sequence: u64,
}

#[derive(Debug, Default)]
struct TableMemoryState {
    data: RwLock<TableMemoryData>,
}

#[derive(Clone, Debug)]
pub(crate) struct MutableTable {
    inner: Arc<TableMemoryState>,
    sequence_upper_bound: Option<u64>,
    captured_facts: Option<TableMemoryFacts>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MutableTableAppendBaseline {
    approximate_size_bytes: usize,
    min_commit: Option<CommitVersion>,
    max_commit: Option<CommitVersion>,
    next_sequence: u64,
}

impl Default for MutableTable {
    fn default() -> Self {
        Self {
            inner: Arc::new(TableMemoryState::default()),
            sequence_upper_bound: None,
            captured_facts: None,
        }
    }
}

impl PartialEq for MutableTable {
    fn eq(&self, other: &Self) -> bool {
        self.facts() == other.facts() && self.visible_rows() == other.visible_rows()
    }
}

impl Eq for MutableTable {}

impl MutableTable {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn clone_for_read_view(&self) -> Self {
        let (sequence_upper_bound, facts) = self.capture_view_state();
        Self {
            inner: Arc::clone(&self.inner),
            sequence_upper_bound: Some(sequence_upper_bound),
            captured_facts: Some(facts),
        }
    }

    pub(crate) fn insert_row(&mut self, row: StorageRow) -> TableRuntimeResult<()> {
        self.insert_table_row(TableRow::new(row))
    }

    pub(crate) fn insert_table_row(&mut self, row: TableRow) -> TableRuntimeResult<()> {
        debug_assert!(
            self.sequence_upper_bound.is_none(),
            "cannot insert into a pinned memory-table view"
        );
        let key = row.key().clone();
        perf_trace::record_mutable_insert_duplicate_check();
        let mut data = self.write_data();
        if data.rows.contains_key(&key) {
            return Err(TableRuntimeError::DuplicateInternalKey {
                key: key.as_slice().to_vec(),
            });
        }

        let row_size = row.approximate_size_bytes();
        let commit_version = row.commit_version();
        let sequence = data.next_sequence;
        data.next_sequence = data.next_sequence.saturating_add(1);
        data.rows.insert(
            key,
            TableMemoryEntry {
                row: Arc::new(row),
                sequence,
            },
        );
        data.approximate_size_bytes = data.approximate_size_bytes.saturating_add(row_size);
        data.min_commit = Some(
            data.min_commit
                .map_or(commit_version, |min| min.min(commit_version)),
        );
        data.max_commit = Some(
            data.max_commit
                .map_or(commit_version, |max| max.max(commit_version)),
        );
        Ok(())
    }

    pub(crate) fn append_baseline(&self) -> MutableTableAppendBaseline {
        let data = self.read_data();
        MutableTableAppendBaseline {
            approximate_size_bytes: data.approximate_size_bytes,
            min_commit: data.min_commit,
            max_commit: data.max_commit,
            next_sequence: data.next_sequence,
        }
    }

    pub(crate) fn rollback_appended_rows(
        &mut self,
        baseline: MutableTableAppendBaseline,
        inserted_keys: &[TableInternalKeyBytes],
    ) {
        let mut data = self.write_data();
        for key in inserted_keys {
            data.rows.remove(key);
        }
        data.approximate_size_bytes = baseline.approximate_size_bytes;
        data.min_commit = baseline.min_commit;
        data.max_commit = baseline.max_commit;
        data.next_sequence = baseline.next_sequence;
    }

    pub(crate) fn len(&self) -> usize {
        self.facts().row_count()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(crate) fn approximate_size_bytes(&self) -> usize {
        self.facts().approximate_size_bytes()
    }

    pub(crate) fn facts(&self) -> TableMemoryFacts {
        if let Some(facts) = &self.captured_facts {
            return facts.clone();
        }
        memory_facts(&self.read_data())
    }

    pub(crate) fn first_key(&self) -> Option<TableInternalKeyBytes> {
        self.facts().first_key().map(canonical_fact_key)
    }

    pub(crate) fn last_key(&self) -> Option<TableInternalKeyBytes> {
        self.facts().last_key().map(canonical_fact_key)
    }

    pub(crate) fn get(&self, key: &TableInternalKeyBytes) -> Option<Arc<TableRow>> {
        let data = self.read_data();
        data.rows
            .get(key)
            .filter(|entry| self.entry_visible(entry))
            .map(|entry| Arc::clone(&entry.row))
    }

    pub(crate) fn iter(&self) -> impl DoubleEndedIterator<Item = Arc<TableRow>> {
        self.visible_rows().into_iter()
    }

    pub(crate) fn cursor(&self) -> MemoryTableCursor<'_> {
        MemoryTableCursor::from_mutable(self)
    }

    pub(super) fn read_data(&self) -> RwLockReadGuard<'_, TableMemoryData> {
        self.inner
            .data
            .read()
            .expect("table memory read lock poisoned")
    }

    pub(super) const fn sequence_upper_bound(&self) -> Option<u64> {
        self.sequence_upper_bound
    }

    pub(crate) fn rows_in_bounds(
        &self,
        bounds: &TableKeyBounds,
    ) -> impl DoubleEndedIterator<Item = Arc<TableRow>> {
        let rows = self
            .visible_rows()
            .into_iter()
            .filter(|row| bounds.contains_key(row.key()))
            .collect::<Vec<_>>();
        rows.into_iter()
    }

    pub(crate) fn rows_with_physical_prefix(
        &self,
        prefix: &TablePhysicalKeyBytes,
    ) -> impl DoubleEndedIterator<Item = Arc<TableRow>> {
        let rows = self
            .visible_rows()
            .into_iter()
            .filter(|row| prefix.is_prefix_of(row.key()))
            .collect::<Vec<_>>();
        rows.into_iter()
    }

    pub(crate) fn seek_prepared_point(
        &self,
        lookup: &TablePreparedPointLookup,
    ) -> (Option<Arc<TableRow>>, usize) {
        seek_physical_key_in_rows(&self.read_data(), self.sequence_upper_bound, lookup)
    }

    pub(crate) fn physical_key_rows(&self, key: &PhysicalKey) -> (Vec<TableRow>, usize) {
        physical_key_rows_in_rows(&self.read_data(), self.sequence_upper_bound, key)
    }

    #[cfg(feature = "perf-trace")]
    pub(crate) fn perf_seek_physical_key_unbounded(
        &self,
        key: &PhysicalKey,
    ) -> (Option<Arc<TableRow>>, usize) {
        let lookup = TablePreparedPointLookup::new(key, None, None);
        self.seek_prepared_point(&lookup)
    }

    pub(crate) fn freeze(self) -> FrozenTable {
        let (sequence_upper_bound, facts) = self.capture_view_state();
        FrozenTable {
            inner: self.inner,
            sequence_upper_bound,
            facts,
        }
    }

    fn capture_view_state(&self) -> (u64, TableMemoryFacts) {
        if let (Some(sequence_upper_bound), Some(facts)) =
            (self.sequence_upper_bound, &self.captured_facts)
        {
            return (sequence_upper_bound, facts.clone());
        }
        let data = self.read_data();
        (data.next_sequence, memory_facts(&data))
    }

    fn write_data(&self) -> RwLockWriteGuard<'_, TableMemoryData> {
        self.inner
            .data
            .write()
            .expect("table memory write lock poisoned")
    }

    fn entry_visible(&self, entry: &TableMemoryEntry) -> bool {
        entry_visible(entry, self.sequence_upper_bound)
    }

    fn visible_rows(&self) -> Vec<Arc<TableRow>> {
        let data = self.read_data();
        data.rows
            .values()
            .filter(|entry| self.entry_visible(entry))
            .map(|entry| Arc::clone(&entry.row))
            .collect()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct FrozenTable {
    inner: Arc<TableMemoryState>,
    sequence_upper_bound: u64,
    facts: TableMemoryFacts,
}

impl Default for FrozenTable {
    fn default() -> Self {
        MutableTable::new().freeze()
    }
}

impl PartialEq for FrozenTable {
    fn eq(&self, other: &Self) -> bool {
        self.facts == other.facts && self.visible_rows() == other.visible_rows()
    }
}

impl Eq for FrozenTable {}

impl FrozenTable {
    pub(crate) fn len(&self) -> usize {
        self.facts.row_count()
    }

    /// Strong-count of the backing memtable `Arc`, for the BS2.4b snapshot-lifetime probe: a held
    /// snapshot keeps a frozen table alive after the runtime retires it (flush), and it dies by
    /// `Arc` drop once the snapshot is dropped (`RocksDB` `Version::Unref`).
    #[cfg(any(test, feature = "testkit"))]
    pub(crate) fn memory_state_strong_count(&self) -> usize {
        Arc::strong_count(&self.inner)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(crate) fn approximate_size_bytes(&self) -> usize {
        self.facts.approximate_size_bytes()
    }

    pub(crate) fn facts(&self) -> TableMemoryFacts {
        self.facts.clone()
    }

    pub(crate) fn first_key(&self) -> Option<TableInternalKeyBytes> {
        self.facts.first_key().map(canonical_fact_key)
    }

    pub(crate) fn last_key(&self) -> Option<TableInternalKeyBytes> {
        self.facts.last_key().map(canonical_fact_key)
    }

    pub(crate) fn get(&self, key: &TableInternalKeyBytes) -> Option<Arc<TableRow>> {
        let data = self.read_data();
        data.rows
            .get(key)
            .filter(|entry| self.entry_visible(entry))
            .map(|entry| Arc::clone(&entry.row))
    }

    pub(crate) fn iter(&self) -> impl DoubleEndedIterator<Item = Arc<TableRow>> {
        self.visible_rows().into_iter()
    }

    pub(crate) fn cursor(&self) -> MemoryTableCursor<'_> {
        MemoryTableCursor::from_frozen(self)
    }

    pub(super) fn read_data(&self) -> RwLockReadGuard<'_, TableMemoryData> {
        self.inner
            .data
            .read()
            .expect("table memory read lock poisoned")
    }

    pub(super) const fn sequence_upper_bound(&self) -> u64 {
        self.sequence_upper_bound
    }

    pub(crate) fn rows_in_bounds(
        &self,
        bounds: &TableKeyBounds,
    ) -> impl DoubleEndedIterator<Item = Arc<TableRow>> {
        let rows = self
            .visible_rows()
            .into_iter()
            .filter(|row| bounds.contains_key(row.key()))
            .collect::<Vec<_>>();
        rows.into_iter()
    }

    pub(crate) fn rows_with_physical_prefix(
        &self,
        prefix: &TablePhysicalKeyBytes,
    ) -> impl DoubleEndedIterator<Item = Arc<TableRow>> {
        let rows = self
            .visible_rows()
            .into_iter()
            .filter(|row| prefix.is_prefix_of(row.key()))
            .collect::<Vec<_>>();
        rows.into_iter()
    }

    pub(crate) fn seek_prepared_point(
        &self,
        lookup: &TablePreparedPointLookup,
    ) -> (Option<Arc<TableRow>>, usize) {
        seek_physical_key_in_rows(&self.read_data(), Some(self.sequence_upper_bound), lookup)
    }

    pub(crate) fn physical_key_rows(&self, key: &PhysicalKey) -> (Vec<TableRow>, usize) {
        physical_key_rows_in_rows(&self.read_data(), Some(self.sequence_upper_bound), key)
    }

    #[cfg(feature = "perf-trace")]
    pub(crate) fn perf_seek_physical_key_unbounded(
        &self,
        key: &PhysicalKey,
    ) -> (Option<Arc<TableRow>>, usize) {
        let lookup = TablePreparedPointLookup::new(key, None, None);
        self.seek_prepared_point(&lookup)
    }

    fn entry_visible(&self, entry: &TableMemoryEntry) -> bool {
        entry_visible(entry, Some(self.sequence_upper_bound))
    }

    fn visible_rows(&self) -> Vec<Arc<TableRow>> {
        let data = self.read_data();
        data.rows
            .values()
            .filter(|entry| self.entry_visible(entry))
            .map(|entry| Arc::clone(&entry.row))
            .collect()
    }
}

pub(super) fn entry_visible(entry: &TableMemoryEntry, sequence_upper_bound: Option<u64>) -> bool {
    sequence_upper_bound.is_none_or(|upper_bound| entry.sequence < upper_bound)
}

fn seek_physical_key_in_rows(
    data: &TableMemoryData,
    sequence_upper_bound: Option<u64>,
    lookup: &TablePreparedPointLookup,
) -> (Option<Arc<TableRow>>, usize) {
    perf_trace::record_table_seek();
    lookup.record_table_seek_use();
    let prefix = lookup.physical_key();
    let seek_key = lookup.seek_key();
    let mut visited = 0usize;
    for (_, entry) in data.rows.range((Included(seek_key), Unbounded)) {
        let row = entry.row.as_ref();
        if !prefix.is_prefix_of(row.key()) {
            break;
        }
        if !entry_visible(entry, sequence_upper_bound) {
            continue;
        }
        visited = visited.saturating_add(1);
        if row_matches_point_bound(
            row,
            lookup.max_commit_version(),
            lookup.max_commit_timestamp(),
        ) {
            return (Some(Arc::clone(&entry.row)), visited);
        }
    }
    (None, visited)
}

fn physical_key_rows_in_rows(
    data: &TableMemoryData,
    sequence_upper_bound: Option<u64>,
    key: &PhysicalKey,
) -> (Vec<TableRow>, usize) {
    let prefix = TablePhysicalKeyBytes::from_physical_key(key);
    let seek_key = TableInternalKeyBytes::from_internal_key(&InternalKey::new(
        key.clone(),
        CommitVersion::MAX,
    ));
    let mut matching = Vec::new();
    for (_, entry) in data.rows.range(seek_key..) {
        let row = entry.row.as_ref();
        if !prefix.is_prefix_of(row.key()) {
            break;
        }
        if !entry_visible(entry, sequence_upper_bound) {
            continue;
        }
        matching.push(row.clone());
    }
    let visited = matching.len();
    (matching, visited)
}

fn row_matches_point_bound(
    row: &TableRow,
    max_commit_version: Option<CommitVersion>,
    max_commit_timestamp: Option<Timestamp>,
) -> bool {
    max_commit_version.is_none_or(|version| row.commit_version().as_u64() <= version.as_u64())
        && max_commit_timestamp.is_none_or(|timestamp| row.commit_timestamp() <= timestamp)
}

fn memory_facts(data: &TableMemoryData) -> TableMemoryFacts {
    TableMemoryFacts {
        row_count: data.rows.len(),
        approximate_size_bytes: data.approximate_size_bytes,
        first_key: data
            .rows
            .first_key_value()
            .map(|(key, _)| key.as_slice().to_vec()),
        last_key: data
            .rows
            .last_key_value()
            .map(|(key, _)| key.as_slice().to_vec()),
        min_commit: data.min_commit,
        max_commit: data.max_commit,
    }
}

fn canonical_fact_key(bytes: &[u8]) -> TableInternalKeyBytes {
    TableInternalKeyBytes::from_canonical_bytes(bytes.to_vec())
        .expect("memory table facts store canonical internal keys")
}
