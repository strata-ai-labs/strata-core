//! Mutable and frozen in-memory tables.

use super::{
    MemoryTableCursor, TableInternalKeyBytes, TableKeyBounds, TablePhysicalKeyBytes, TableRow,
    TableRuntimeError, TableRuntimeResult,
};
use crate::observability::perf_trace;
use crate::row::StorageRow;
use crate::row::{InternalKey, PhysicalKey};
use std::collections::BTreeMap;
use std::sync::Arc;
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

#[derive(Clone, Debug, Default, Eq, PartialEq)]
struct TableMemoryState {
    rows: BTreeMap<TableInternalKeyBytes, TableRow>,
    approximate_size_bytes: usize,
    min_commit: Option<CommitVersion>,
    max_commit: Option<CommitVersion>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct MutableTable {
    inner: TableMemoryState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MutableTableAppendBaseline {
    approximate_size_bytes: usize,
    min_commit: Option<CommitVersion>,
    max_commit: Option<CommitVersion>,
}

impl MutableTable {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn clone_for_read_view(&self) -> Self {
        self.clone()
    }

    pub(crate) fn insert_row(&mut self, row: StorageRow) -> TableRuntimeResult<()> {
        self.insert_table_row(TableRow::new(row))
    }

    pub(crate) fn insert_table_row(&mut self, row: TableRow) -> TableRuntimeResult<()> {
        let key = row.key().clone();
        perf_trace::record_mutable_insert_duplicate_check();
        if self.inner.rows.contains_key(&key) {
            return Err(TableRuntimeError::DuplicateInternalKey {
                key: key.as_slice().to_vec(),
            });
        }

        let row_size = row.approximate_size_bytes();
        let commit_version = row.commit_version();
        let inner = &mut self.inner;
        inner.rows.insert(key, row);
        inner.approximate_size_bytes = inner.approximate_size_bytes.saturating_add(row_size);
        update_commit_range(&mut inner.min_commit, &mut inner.max_commit, commit_version);
        Ok(())
    }

    pub(crate) fn append_baseline(&self) -> MutableTableAppendBaseline {
        MutableTableAppendBaseline {
            approximate_size_bytes: self.inner.approximate_size_bytes,
            min_commit: self.inner.min_commit,
            max_commit: self.inner.max_commit,
        }
    }

    pub(crate) fn rollback_appended_rows(
        &mut self,
        baseline: MutableTableAppendBaseline,
        inserted_keys: &[TableInternalKeyBytes],
    ) {
        let inner = &mut self.inner;
        for key in inserted_keys {
            inner.rows.remove(key);
        }
        inner.approximate_size_bytes = baseline.approximate_size_bytes;
        inner.min_commit = baseline.min_commit;
        inner.max_commit = baseline.max_commit;
    }

    pub(crate) fn len(&self) -> usize {
        self.inner.rows.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.inner.rows.is_empty()
    }

    pub(crate) fn approximate_size_bytes(&self) -> usize {
        self.inner.approximate_size_bytes
    }

    pub(crate) fn facts(&self) -> TableMemoryFacts {
        memory_facts(
            &self.inner.rows,
            self.inner.approximate_size_bytes,
            self.inner.min_commit,
            self.inner.max_commit,
        )
    }

    pub(crate) fn first_key(&self) -> Option<&TableInternalKeyBytes> {
        self.inner.rows.first_key_value().map(|(key, _)| key)
    }

    pub(crate) fn last_key(&self) -> Option<&TableInternalKeyBytes> {
        self.inner.rows.last_key_value().map(|(key, _)| key)
    }

    pub(crate) fn get(&self, key: &TableInternalKeyBytes) -> Option<&TableRow> {
        self.inner.rows.get(key)
    }

    pub(crate) fn iter(&self) -> impl DoubleEndedIterator<Item = &TableRow> {
        self.inner.rows.values()
    }

    pub(crate) fn cursor(&self) -> MemoryTableCursor<'_> {
        MemoryTableCursor::from_mutable(self)
    }

    pub(super) fn row_map(&self) -> &BTreeMap<TableInternalKeyBytes, TableRow> {
        &self.inner.rows
    }

    pub(crate) fn rows_in_bounds<'a>(
        &'a self,
        bounds: &'a TableKeyBounds,
    ) -> impl Iterator<Item = &'a TableRow> + 'a {
        self.inner
            .rows
            .values()
            .filter(move |row| bounds.contains_key(row.key()))
    }

    pub(crate) fn rows_with_physical_prefix<'a>(
        &'a self,
        prefix: &'a TablePhysicalKeyBytes,
    ) -> impl Iterator<Item = &'a TableRow> + 'a {
        self.inner
            .rows
            .values()
            .filter(move |row| prefix.is_prefix_of(row.key()))
    }

    pub(crate) fn seek_physical_key(
        &self,
        key: &PhysicalKey,
        max_commit_version: Option<CommitVersion>,
        max_commit_timestamp: Option<Timestamp>,
    ) -> (Option<&TableRow>, usize) {
        seek_physical_key_in_rows(
            &self.inner.rows,
            key,
            max_commit_version,
            max_commit_timestamp,
        )
    }

    pub(crate) fn physical_key_rows(&self, key: &PhysicalKey) -> (Vec<TableRow>, usize) {
        physical_key_rows_in_rows(&self.inner.rows, key)
    }

    #[cfg(feature = "perf-trace")]
    pub(crate) fn perf_seek_physical_key_unbounded(
        &self,
        key: &PhysicalKey,
    ) -> (Option<&TableRow>, usize) {
        self.seek_physical_key(key, None, None)
    }

    pub(crate) fn freeze(self) -> FrozenTable {
        FrozenTable {
            inner: Arc::new(self.inner),
        }
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct FrozenTable {
    inner: Arc<TableMemoryState>,
}

impl FrozenTable {
    pub(crate) fn len(&self) -> usize {
        self.inner.rows.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.inner.rows.is_empty()
    }

    pub(crate) fn approximate_size_bytes(&self) -> usize {
        self.inner.approximate_size_bytes
    }

    pub(crate) fn facts(&self) -> TableMemoryFacts {
        memory_facts(
            &self.inner.rows,
            self.inner.approximate_size_bytes,
            self.inner.min_commit,
            self.inner.max_commit,
        )
    }

    pub(crate) fn first_key(&self) -> Option<&TableInternalKeyBytes> {
        self.inner.rows.first_key_value().map(|(key, _)| key)
    }

    pub(crate) fn last_key(&self) -> Option<&TableInternalKeyBytes> {
        self.inner.rows.last_key_value().map(|(key, _)| key)
    }

    pub(crate) fn get(&self, key: &TableInternalKeyBytes) -> Option<&TableRow> {
        self.inner.rows.get(key)
    }

    pub(crate) fn iter(&self) -> impl DoubleEndedIterator<Item = &TableRow> {
        self.inner.rows.values()
    }

    pub(crate) fn cursor(&self) -> MemoryTableCursor<'_> {
        MemoryTableCursor::from_frozen(self)
    }

    pub(super) fn row_map(&self) -> &BTreeMap<TableInternalKeyBytes, TableRow> {
        &self.inner.rows
    }

    pub(crate) fn rows_in_bounds<'a>(
        &'a self,
        bounds: &'a TableKeyBounds,
    ) -> impl Iterator<Item = &'a TableRow> + 'a {
        self.inner
            .rows
            .values()
            .filter(move |row| bounds.contains_key(row.key()))
    }

    pub(crate) fn rows_with_physical_prefix<'a>(
        &'a self,
        prefix: &'a TablePhysicalKeyBytes,
    ) -> impl Iterator<Item = &'a TableRow> + 'a {
        self.inner
            .rows
            .values()
            .filter(move |row| prefix.is_prefix_of(row.key()))
    }

    pub(crate) fn seek_physical_key(
        &self,
        key: &PhysicalKey,
        max_commit_version: Option<CommitVersion>,
        max_commit_timestamp: Option<Timestamp>,
    ) -> (Option<&TableRow>, usize) {
        seek_physical_key_in_rows(
            &self.inner.rows,
            key,
            max_commit_version,
            max_commit_timestamp,
        )
    }

    pub(crate) fn physical_key_rows(&self, key: &PhysicalKey) -> (Vec<TableRow>, usize) {
        physical_key_rows_in_rows(&self.inner.rows, key)
    }

    #[cfg(feature = "perf-trace")]
    pub(crate) fn perf_seek_physical_key_unbounded(
        &self,
        key: &PhysicalKey,
    ) -> (Option<&TableRow>, usize) {
        self.seek_physical_key(key, None, None)
    }
}

fn seek_physical_key_in_rows<'a>(
    rows: &'a BTreeMap<TableInternalKeyBytes, TableRow>,
    key: &PhysicalKey,
    max_commit_version: Option<CommitVersion>,
    max_commit_timestamp: Option<Timestamp>,
) -> (Option<&'a TableRow>, usize) {
    perf_trace::record_table_seek();
    let prefix = TablePhysicalKeyBytes::from_physical_key(key);
    let seek_version = max_commit_version.unwrap_or(CommitVersion::MAX);
    let seek_key =
        TableInternalKeyBytes::from_internal_key(&InternalKey::new(key.clone(), seek_version));
    let mut visited = 0usize;
    for (_, row) in rows.range(seek_key..) {
        visited = visited.saturating_add(1);
        if !prefix.is_prefix_of(row.key()) {
            break;
        }
        if row_matches_point_bound(row, max_commit_version, max_commit_timestamp) {
            return (Some(row), visited);
        }
    }
    (None, visited)
}

fn physical_key_rows_in_rows(
    rows: &BTreeMap<TableInternalKeyBytes, TableRow>,
    key: &PhysicalKey,
) -> (Vec<TableRow>, usize) {
    let prefix = TablePhysicalKeyBytes::from_physical_key(key);
    let seek_key = TableInternalKeyBytes::from_internal_key(&InternalKey::new(
        key.clone(),
        CommitVersion::MAX,
    ));
    let mut matching = Vec::new();
    for (_, row) in rows.range(seek_key..) {
        if !prefix.is_prefix_of(row.key()) {
            break;
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

fn update_commit_range(
    min_commit: &mut Option<CommitVersion>,
    max_commit: &mut Option<CommitVersion>,
    commit_version: CommitVersion,
) {
    *min_commit = Some(min_commit.map_or(commit_version, |min| min.min(commit_version)));
    *max_commit = Some(max_commit.map_or(commit_version, |max| max.max(commit_version)));
}

fn memory_facts(
    rows: &BTreeMap<TableInternalKeyBytes, TableRow>,
    approximate_size_bytes: usize,
    min_commit: Option<CommitVersion>,
    max_commit: Option<CommitVersion>,
) -> TableMemoryFacts {
    TableMemoryFacts {
        row_count: rows.len(),
        approximate_size_bytes,
        first_key: rows
            .first_key_value()
            .map(|(key, _)| key.as_slice().to_vec()),
        last_key: rows
            .last_key_value()
            .map(|(key, _)| key.as_slice().to_vec()),
        min_commit,
        max_commit,
    }
}
