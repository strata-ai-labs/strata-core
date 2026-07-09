//! Immutable table reader.

use std::fmt;
use std::sync::{Arc, OnceLock};

use super::cache::CacheInsert;
use super::key::table_internal_physical_key_bytes;
use super::{
    BoundedTableCursor, TableBlockAddress, TableBlockCache, TableBlockCacheKey,
    TableBlockCacheKind, TableBloomFilter, TableBloomProbe, TableCacheTableId, TableCommitRange,
    TableIdentity, TableInternalKeyBytes, TableKeyBounds, TableKeyRange,
    TableMaterializationPolicy, TablePhysicalKeyBytes, TablePreparedPointLookup, TableReaderConfig,
    TableReaderEagerFilterMode, TableRow, TableRuntimeError, TableRuntimeFacts, TableRuntimeResult,
};
use crate::format::seek_immutable_table_data_block_point;
use crate::format::{
    build_immutable_table_data_block_entry_offsets, decode_immutable_table_data_block_trusted,
    seek_immutable_table_data_block_point_indexed, FormatError,
};
use crate::format::{
    decode_filter_frame, decode_immutable_table, decode_immutable_table_data_block,
    decode_immutable_table_metadata, decode_table_footer_metadata, decode_table_header,
    ImmutableTableMetadata, TableDataBlockPointSeek, TableFilterFrame, TableIndexEntry,
    MAX_TABLE_FOOTER_SIZE, MAX_TABLE_HEADER_SIZE,
};
use crate::observability::perf_trace;
use crate::row::{InternalKey, PhysicalKey};
use sha2::{Digest, Sha256};
use strata_core_next::{CommitVersion, Timestamp};

use super::facts::table_facts_from_decoded;

const DEFAULT_EAGER_FILTER_BITS_PER_KEY: usize = 10;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BytesTableSource {
    bytes: Vec<u8>,
    exact_content_digest: Option<[u8; 32]>,
}

impl BytesTableSource {
    pub(crate) fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            bytes: bytes.into(),
            exact_content_digest: None,
        }
    }

    pub(crate) fn with_exact_content_digest(mut self) -> Self {
        self.exact_content_digest = Some(table_content_digest(&self.bytes));
        self
    }
}

impl TableByteSource for BytesTableSource {
    fn byte_count(&self) -> u64 {
        u64::try_from(self.bytes.len()).expect("usize fits in u64")
    }

    fn read_at(&self, offset: u64, len: usize) -> TableRuntimeResult<Vec<u8>> {
        let start = usize::try_from(offset).map_err(|_| TableRuntimeError::InvalidRange {
            field: "byte_offset",
        })?;
        let end = start
            .checked_add(len)
            .ok_or(TableRuntimeError::InvalidRange {
                field: "byte_range",
            })?;
        if end > self.bytes.len() {
            return Err(TableRuntimeError::source_read(
                "byte range exceeds source length",
            ));
        }
        Ok(self.bytes[start..end].to_vec())
    }

    fn exact_content_digest(&self) -> TableRuntimeResult<Option<[u8; 32]>> {
        Ok(self.exact_content_digest)
    }
}

pub(crate) trait TableByteSource {
    fn byte_count(&self) -> u64;
    fn read_at(&self, offset: u64, len: usize) -> TableRuntimeResult<Vec<u8>>;

    fn exact_content_digest(&self) -> TableRuntimeResult<Option<[u8; 32]>> {
        Ok(None)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableReaderFilter {
    state: TableReaderFilterState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TableContentFingerprint {
    byte_count: u64,
    table_crc32: u32,
    metadata_crc32: u32,
    content_sha256: Option<[u8; 32]>,
}

impl TableContentFingerprint {
    fn matches_exact_content(self, other: Self) -> bool {
        self.byte_count == other.byte_count
            && self.table_crc32 == other.table_crc32
            && self.metadata_crc32 == other.metadata_crc32
            && matches!(
                (self.content_sha256, other.content_sha256),
                (Some(left), Some(right)) if left == right
            )
    }

    /// BS4.4a-i: content equality without requiring a SHA-256 on both sides. Byte count + both CRC32s
    /// pin byte-identical content; the SHA-256 is an extra check only when both sides carry it. Unlike
    /// `matches_exact_content`, this treats an eager reader (SHA present) and a lazy reader (SHA absent)
    /// over the same table as equal — matching the pre-BS4.4a row-compare behavior.
    fn matches_content(self, other: Self) -> bool {
        self.byte_count == other.byte_count
            && self.table_crc32 == other.table_crc32
            && self.metadata_crc32 == other.metadata_crc32
            && match (self.content_sha256, other.content_sha256) {
                (Some(left), Some(right)) => left == right,
                _ => true,
            }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum TableReaderFilterState {
    Unavailable,
    // `Arc`, not `Box`: the bloom is immutable once built, so cloning a reader (and thus a
    // `BranchOwnedTable`) shares the filter by refcount instead of deep-copying its bits.
    Bloom(Arc<TableReaderBloomFilterState>),
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TableReaderBloomFilterState {
    facts: TableRuntimeFacts,
    fingerprint: TableContentFingerprint,
    filter: TableBloomFilter,
}

impl TableReaderFilter {
    pub(crate) const fn unavailable() -> Self {
        Self {
            state: TableReaderFilterState::Unavailable,
        }
    }

    pub(crate) fn from_table_bytes(
        identity: TableIdentity,
        bytes: &[u8],
        bits_per_key: usize,
    ) -> TableRuntimeResult<Self> {
        let decoded = decode_immutable_table(bytes)
            .map_err(|source| TableRuntimeError::DecodeFormat { source })?;
        let rows = decoded
            .rows()
            .iter()
            .cloned()
            .map(TableRow::new)
            .collect::<Vec<_>>();
        super::validate_strictly_sorted_unique_rows(&rows)?;
        let facts = table_facts_from_decoded(identity, bytes, &decoded)?;
        let fingerprint = table_content_fingerprint_from_bytes(bytes)?;
        build_filter_from_decoded_rows(facts, fingerprint, &rows, bits_per_key)
    }

    fn from_validated_rows(
        facts: TableRuntimeFacts,
        fingerprint: TableContentFingerprint,
        rows: &[TableRow],
        bits_per_key: usize,
    ) -> TableRuntimeResult<Self> {
        validate_filter_rows_match_facts(&facts, rows)?;
        let filter = TableBloomFilter::build(
            rows.iter().map(|row| row.key().physical_key_bytes()),
            bits_per_key,
        )?;
        Ok(Self {
            state: TableReaderFilterState::Bloom(Arc::new(TableReaderBloomFilterState {
                facts,
                fingerprint,
                filter,
            })),
        })
    }

    /// BS4.2b: build a reader filter from a table's persisted filter frame, without scanning rows.
    /// The frame's own CRC (verified during `decode_filter_frame`) plus the `from_frame_parts` bounds
    /// are the integrity gate. The state carries the reader's own `facts`/`fingerprint`; the reader
    /// attaches this directly, bypassing the `matches_table` gate (which guards *externally supplied*
    /// filters and, for a lazy reader whose fingerprint carries no content hash, would reject it).
    fn from_persisted_frame(
        facts: TableRuntimeFacts,
        fingerprint: TableContentFingerprint,
        frame: TableFilterFrame,
    ) -> TableRuntimeResult<Self> {
        // A persisted frame only exists for a non-empty table, so an empty filter (`key_count == 0`)
        // is malformed — and, since `might_contain` short-circuits an empty filter to
        // `DefinitelyAbsent`, would false-absent every live row (silent data loss). Reject it rather
        // than trust it; a well-formed filter over a non-empty table always has `key_count >= 1`.
        if frame.key_count == 0 {
            return Err(TableRuntimeError::InvalidRange {
                field: "table_filter_key_count",
            });
        }
        let filter = TableBloomFilter::from_frame_parts(
            frame.probes,
            frame.key_count,
            frame.bit_count,
            frame.bits,
        )?;
        Ok(Self {
            state: TableReaderFilterState::Bloom(Arc::new(TableReaderBloomFilterState {
                facts,
                fingerprint,
                filter,
            })),
        })
    }

    /// Allocation address of the shared bloom state, or `None` when unavailable. Test-only: lets a
    /// clone-sharing test assert the filter is shared by `Arc` refcount rather than deep-copied.
    #[cfg(test)]
    pub(crate) fn bloom_alloc_addr(&self) -> Option<usize> {
        match &self.state {
            TableReaderFilterState::Unavailable => None,
            TableReaderFilterState::Bloom(state) => Some(Arc::as_ptr(state) as usize),
        }
    }

    /// The loaded bloom's shape `(probes, bit_count, key_count)`, or `None` when unavailable. Test-only:
    /// lets a test prove the *persisted* filter was loaded (its shape differs from a rescan's).
    #[cfg(test)]
    pub(crate) fn bloom_shape(&self) -> Option<(u8, usize, usize)> {
        match &self.state {
            TableReaderFilterState::Unavailable => None,
            TableReaderFilterState::Bloom(state) => Some((
                state.filter.probes(),
                state.filter.bit_count(),
                state.filter.key_count(),
            )),
        }
    }

    pub(crate) fn probe_physical_key(&self, key: &TablePhysicalKeyBytes) -> TableBloomProbe {
        match &self.state {
            TableReaderFilterState::Unavailable => TableBloomProbe::Unavailable,
            TableReaderFilterState::Bloom(state) => state.filter.might_contain(key.as_slice()),
        }
    }

    pub(crate) const fn is_available(&self) -> bool {
        matches!(self.state, TableReaderFilterState::Bloom(_))
    }

    fn matches_table(
        &self,
        facts: &TableRuntimeFacts,
        fingerprint: TableContentFingerprint,
    ) -> bool {
        match &self.state {
            TableReaderFilterState::Unavailable => true,
            TableReaderFilterState::Bloom(state) => {
                state.facts == *facts && state.fingerprint.matches_exact_content(fingerprint)
            }
        }
    }
}

impl<T: TableByteSource + ?Sized> TableByteSource for &T {
    fn byte_count(&self) -> u64 {
        (**self).byte_count()
    }

    fn read_at(&self, offset: u64, len: usize) -> TableRuntimeResult<Vec<u8>> {
        (**self).read_at(offset, len)
    }

    fn exact_content_digest(&self) -> TableRuntimeResult<Option<[u8; 32]>> {
        (**self).exact_content_digest()
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ImmutableTableReader<'a> {
    config: TableReaderConfig,
    facts: TableRuntimeFacts,
    fingerprint: TableContentFingerprint,
    runtime_facts: TableReaderRuntimeFacts,
    rows: TableReaderRows<'a>,
}

impl PartialEq for ImmutableTableReader<'_> {
    fn eq(&self, other: &Self) -> bool {
        // BS4.4a-i: compare by content fingerprint instead of materializing both readers' rows. Sealed
        // tables are content-immutable, so equal config + facts + fingerprint (byte count + CRC32s)
        // implies byte-identical content. `matches_content` is lazy-safe and preserves the old
        // row-compare behavior: an eager and a lazy reader of the same table still compare equal.
        self.config == other.config
            && self.facts == other.facts
            && self.fingerprint.matches_content(other.fingerprint)
    }
}

impl Eq for ImmutableTableReader<'_> {}

#[derive(Clone, Debug)]
enum TableReaderRows<'a> {
    Eager(EagerTableRows),
    Lazy(Box<LazyTableRows<'a>>),
}

#[derive(Clone, Debug)]
struct EagerTableRows {
    rows: Arc<[TableRow]>,
    filter: TableReaderFilter,
}

impl EagerTableRows {
    fn new(rows: Arc<[TableRow]>, filter: TableReaderFilter) -> Self {
        Self { rows, filter }
    }

    fn from_vec(
        facts: TableRuntimeFacts,
        fingerprint: TableContentFingerprint,
        rows: Vec<TableRow>,
        config: TableReaderConfig,
    ) -> TableRuntimeResult<Self> {
        let filter = build_eager_filter(facts, fingerprint, &rows, config)?;
        Ok(Self::new(Arc::from(rows.into_boxed_slice()), filter))
    }

    fn from_vec_with_filter(rows: Vec<TableRow>, filter: TableReaderFilter) -> Self {
        Self::new(Arc::from(rows.into_boxed_slice()), filter)
    }

    fn rows(&self) -> &[TableRow] {
        &self.rows
    }

    fn with_filter(&mut self, filter: TableReaderFilter) -> bool {
        let available = filter.is_available();
        self.filter = filter;
        available
    }
}

pub(crate) enum TablePointLookupRow<'a> {
    Borrowed(&'a TableRow),
    Owned(TableRow),
}

impl TablePointLookupRow<'_> {
    pub(crate) const fn as_table_row(&self) -> &TableRow {
        match self {
            Self::Borrowed(row) => row,
            Self::Owned(row) => row,
        }
    }

    pub(crate) fn into_owned(self) -> TableRow {
        match self {
            Self::Borrowed(row) => row.clone(),
            Self::Owned(row) => row,
        }
    }
}

impl<'a> TableReaderRows<'a> {
    fn try_rows(&self) -> TableRuntimeResult<&[TableRow]> {
        match self {
            Self::Eager(rows) => Ok(rows.rows()),
            Self::Lazy(rows) => rows.try_rows(),
        }
    }

    fn materialize_for_oracle(&self) -> TableRuntimeResult<Vec<TableRow>> {
        match self {
            Self::Eager(rows) => Ok(rows.rows().to_vec()),
            Self::Lazy(rows) => rows.materialize_for_oracle(),
        }
    }

    fn into_materialized(
        self,
        facts: TableRuntimeFacts,
        fingerprint: TableContentFingerprint,
    ) -> TableRuntimeResult<(EagerTableRows, bool)> {
        match self {
            Self::Eager(rows) => Ok((rows, false)),
            Self::Lazy(rows) => rows
                .into_materialized_eager(facts, fingerprint)
                .map(|rows| (rows, true)),
        }
    }

    fn try_get_exact(&self, key: &TableInternalKeyBytes) -> TableRuntimeResult<Option<TableRow>> {
        match self {
            Self::Eager(rows) => Ok(find_exact_in_rows(rows.rows(), key)),
            Self::Lazy(rows) => rows.try_get_exact(key),
        }
    }

    fn try_seek_prepared_point(
        &self,
        lookup: &TablePreparedPointLookup,
    ) -> TableRuntimeResult<(Option<TableRow>, usize)> {
        let (row, visited) = self.try_seek_prepared_point_candidate(lookup)?;
        Ok((row.map(TablePointLookupRow::into_owned), visited))
    }

    fn try_seek_prepared_point_candidate(
        &self,
        lookup: &TablePreparedPointLookup,
    ) -> TableRuntimeResult<(Option<TablePointLookupRow<'_>>, usize)> {
        match self {
            Self::Eager(rows) => {
                let (row, visited) =
                    seek_prepared_point_in_slice(rows.rows(), lookup, Some(&rows.filter));
                Ok((row.map(TablePointLookupRow::Borrowed), visited))
            }
            Self::Lazy(rows) => rows.try_seek_prepared_point_candidate(lookup),
        }
    }

    fn try_physical_key_rows(
        &self,
        key: &PhysicalKey,
    ) -> TableRuntimeResult<(Vec<TableRow>, usize)> {
        match self {
            Self::Eager(rows) => Ok(physical_key_rows_in_slice(rows.rows(), key)),
            Self::Lazy(rows) => rows.try_physical_key_rows(key),
        }
    }

    fn cursor<'reader>(
        &'reader self,
        bounds_hint: Option<TableKeyBounds>,
        fill_cache: bool,
    ) -> ImmutableTableCursor<'reader, 'a> {
        match self {
            Self::Eager(rows) => ImmutableTableCursor::eager(&rows.rows),
            Self::Lazy(rows) => rows.cursor(bounds_hint, fill_cache),
        }
    }

    fn with_block_cache(&mut self, table: TableCacheTableId, cache: Arc<TableBlockCache>) -> bool {
        match self {
            Self::Eager(_) => false,
            Self::Lazy(rows) => {
                let enabled = cache.enabled();
                rows.with_block_cache(table, cache);
                enabled
            }
        }
    }

    fn with_filter(&mut self, filter: TableReaderFilter) -> bool {
        match self {
            Self::Eager(rows) => rows.with_filter(filter),
            Self::Lazy(rows) => rows.with_filter(filter),
        }
    }

    #[cfg(test)]
    fn filter(&self) -> Option<&TableReaderFilter> {
        match self {
            Self::Eager(rows) => Some(&rows.filter),
            Self::Lazy(rows) => rows.state.filter.as_ref(),
        }
    }
}

#[derive(Clone, Debug)]
struct LazyTableRows<'a> {
    state: LazyTableState<'a>,
    rows: OnceLock<TableRuntimeResult<Vec<TableRow>>>,
}

impl<'a> LazyTableRows<'a> {
    fn new(
        source: SharedTableSource<'a>,
        metadata: ImmutableTableMetadata,
        materialization_policy: TableMaterializationPolicy,
    ) -> Self {
        Self {
            state: LazyTableState {
                source,
                metadata: Arc::new(metadata),
                cache: None,
                filter: None,
                materialization_policy,
                fill_cache: true,
            },
            rows: OnceLock::new(),
        }
    }

    fn try_rows(&self) -> TableRuntimeResult<&[TableRow]> {
        let rows = self
            .rows
            .get_or_init(|| read_and_validate_rows(&self.state));
        match rows {
            Ok(rows) => Ok(rows.as_slice()),
            Err(error) => Err(error.clone()),
        }
    }

    fn materialize_for_oracle(&self) -> TableRuntimeResult<Vec<TableRow>> {
        // Oracle hatch: reuse the row cache if a legitimate read already populated it; otherwise do a
        // throwaway full read that bypasses the `DenyRuntime` guard and the lazy-materialization
        // counter (this read is not a durable consumer forcing materialization — it is a debug oracle
        // cross-checking a facts-based fast path). It populates neither the row cache (`OnceLock`) nor
        // the block cache (the read is forced no-fill), so it cannot warm a table a cold-read path
        // expects to stay unmaterialized. It still validates, so the oracle sees exactly the rows a
        // permitted full read would yield.
        if let Some(cached) = self.rows.get() {
            return cached.clone();
        }
        let mut state = self.state.clone();
        state.fill_cache = false;
        let rows = read_rows_from_metadata(&state)?;
        super::validate_strictly_sorted_unique_rows(&rows)?;
        Ok(rows)
    }

    fn try_get_exact(&self, key: &TableInternalKeyBytes) -> TableRuntimeResult<Option<TableRow>> {
        if let Some(rows) = self.rows.get() {
            return match rows {
                Ok(rows) => Ok(find_exact_in_rows(rows, key)),
                Err(error) => Err(error.clone()),
            };
        }
        let Some(block_index) = find_index_entry_for_key(&self.state.metadata, key) else {
            return Ok(None);
        };
        // Exact-row probes serve verification-class consumers — they always
        // re-verify cached frames.
        let rows = self
            .state
            .read_data_block_rows(block_index, TableReadTrust::AlwaysVerify)?;
        Ok(find_exact_in_rows(&rows, key))
    }

    fn try_seek_prepared_point_candidate(
        &self,
        lookup: &TablePreparedPointLookup,
    ) -> TableRuntimeResult<(Option<TablePointLookupRow<'_>>, usize)> {
        if let Some(rows) = self.rows.get() {
            return match rows {
                Ok(rows) => {
                    let (row, visited) =
                        seek_prepared_point_in_slice(rows, lookup, self.state.filter.as_ref());
                    Ok((row.map(TablePointLookupRow::Borrowed), visited))
                }
                Err(error) => Err(error.clone()),
            };
        }
        let (row, visited) = self.state.seek_prepared_point(lookup)?;
        Ok((row.map(TablePointLookupRow::Owned), visited))
    }

    fn try_physical_key_rows(
        &self,
        key: &PhysicalKey,
    ) -> TableRuntimeResult<(Vec<TableRow>, usize)> {
        if let Some(rows) = self.rows.get() {
            return match rows {
                Ok(rows) => Ok(physical_key_rows_in_slice(rows, key)),
                Err(error) => Err(error.clone()),
            };
        }
        self.state.physical_key_rows(key)
    }

    fn into_materialized(self) -> TableRuntimeResult<Vec<TableRow>> {
        match self.rows.into_inner() {
            Some(rows) => rows,
            None => read_and_validate_rows(&self.state),
        }
    }

    fn into_materialized_eager(
        self,
        facts: TableRuntimeFacts,
        fingerprint: TableContentFingerprint,
    ) -> TableRuntimeResult<EagerTableRows> {
        let filter = self.state.filter.clone();
        let rows = self.into_materialized()?;
        if let Some(filter) = filter {
            Ok(EagerTableRows::from_vec_with_filter(rows, filter))
        } else {
            EagerTableRows::from_vec(facts, fingerprint, rows, TableReaderConfig::default())
        }
    }

    fn cursor<'reader>(
        &'reader self,
        bounds_hint: Option<TableKeyBounds>,
        fill_cache: bool,
    ) -> ImmutableTableCursor<'reader, 'a> {
        match self.rows.get() {
            Some(Ok(rows)) => ImmutableTableCursor::eager(rows),
            Some(Err(error)) => ImmutableTableCursor::failed(error.clone()),
            None => {
                let mut state = self.state.clone();
                state.fill_cache = fill_cache;
                ImmutableTableCursor::lazy(state, bounds_hint)
            }
        }
    }

    fn with_block_cache(&mut self, table: TableCacheTableId, cache: Arc<TableBlockCache>) {
        self.state.cache = Some(LazyTableBlockCache { table, cache });
    }

    fn with_filter(&mut self, filter: TableReaderFilter) -> bool {
        let available = filter.is_available();
        self.state.filter = Some(filter);
        available
    }
}

#[derive(Clone, Debug)]
struct LazyTableState<'a> {
    source: SharedTableSource<'a>,
    // BS4.4g: `Arc`-shared so cloning a lazy reader — per cursor open (`reader.rs` cursor path) and
    // per install copy-on-write (`Arc::make_mut` on `BranchLayout`) — is a pointer bump rather than a
    // deep clone of the index-block `Vec`, the dominant per-table metadata cost. Accessors deref
    // through the `Arc` unchanged. (The filter's bloom is already `Arc`-shared internally.)
    metadata: Arc<ImmutableTableMetadata>,
    cache: Option<LazyTableBlockCache>,
    filter: Option<TableReaderFilter>,
    materialization_policy: TableMaterializationPolicy,
    // BS4.4g: when false, a data-block miss reads from the source but does NOT insert into the block
    // cache — set on read-once compaction/materialization merge-input cursors so streaming a large
    // table through a merge cannot evict the working set of the point-read cache. Default true (a
    // normal reader/cursor fills the cache); a cache hit is still served regardless.
    fill_cache: bool,
}

impl LazyTableState<'_> {
    fn seek_prepared_point(
        &self,
        lookup: &TablePreparedPointLookup,
    ) -> TableRuntimeResult<(Option<TableRow>, usize)> {
        perf_trace::record_table_seek();
        lookup.record_table_seek_use();
        let prefix = lookup.physical_key();
        let target_physical_key = prefix.as_slice();
        if !self.contains_physical_key(target_physical_key) {
            perf_trace::record_table_point_rows_visited(0);
            return Ok((None, 0));
        }
        if self.probe_physical_filter(prefix) == TableBloomProbe::DefinitelyAbsent {
            perf_trace::record_table_point_rows_visited(0);
            return Ok((None, 0));
        }

        let seek_key = lookup.seek_key();
        let entries = self.metadata.index().entries();
        let Some(mut block_index) = first_candidate_block_for_key(entries, seek_key) else {
            perf_trace::record_table_point_rows_visited(0);
            return Ok((None, 0));
        };

        let mut visited = 0usize;
        while let Some(entry) = entries.get(block_index) {
            match compare_index_entry_physical_range(entry, target_physical_key) {
                PhysicalRangeOrdering::Before => {
                    block_index = block_index.saturating_add(1);
                    continue;
                }
                PhysicalRangeOrdering::After => {
                    perf_trace::record_table_point_rows_visited(visited);
                    return Ok((None, visited));
                }
                PhysicalRangeOrdering::MayContain => {}
            }

            let seek = self.seek_data_block_point(entry, block_index, lookup)?;
            visited = visited.saturating_add(seek.rows_visited());
            let continue_to_next_block = seek.continue_to_next_block();
            if let Some(row) = seek.row() {
                perf_trace::record_table_point_rows_visited(visited);
                return Ok((Some(TableRow::new(row)), visited));
            }

            if !continue_to_next_block {
                perf_trace::record_table_point_rows_visited(visited);
                return Ok((None, visited));
            }
            block_index = block_index.saturating_add(1);
        }

        perf_trace::record_table_point_rows_visited(visited);
        Ok((None, visited))
    }

    fn physical_key_rows(&self, key: &PhysicalKey) -> TableRuntimeResult<(Vec<TableRow>, usize)> {
        perf_trace::record_table_seek();
        let prefix = TablePhysicalKeyBytes::from_physical_key(key);
        let target_physical_key = prefix.as_slice();
        if !self.contains_physical_key(target_physical_key) {
            perf_trace::record_table_point_rows_visited(0);
            return Ok((Vec::new(), 0));
        }
        if self.probe_physical_filter(&prefix) == TableBloomProbe::DefinitelyAbsent {
            perf_trace::record_table_point_rows_visited(0);
            return Ok((Vec::new(), 0));
        }

        let seek_key = TableInternalKeyBytes::from_internal_key(&InternalKey::new(
            key.clone(),
            CommitVersion::MAX,
        ));
        let entries = self.metadata.index().entries();
        let Some(mut block_index) = first_candidate_block_for_key(entries, &seek_key) else {
            perf_trace::record_table_point_rows_visited(0);
            return Ok((Vec::new(), 0));
        };
        let first_candidate_block_index = block_index;

        let mut rows_out = Vec::new();
        while let Some(entry) = entries.get(block_index) {
            match compare_index_entry_physical_range(entry, target_physical_key) {
                PhysicalRangeOrdering::Before => {
                    block_index = block_index.saturating_add(1);
                    continue;
                }
                PhysicalRangeOrdering::After => break,
                PhysicalRangeOrdering::MayContain => {}
            }

            // History reads are an interactive read surface — cache hits are
            // trusted like point seeks.
            let rows = self.read_data_block_rows(block_index, TableReadTrust::TrustCache)?;
            let start = if block_index == first_candidate_block_index {
                candidate_row_index_for_seek_key(&rows, &seek_key)
            } else {
                0
            };
            if start >= rows.len() {
                block_index = block_index.saturating_add(1);
                continue;
            }

            for row in &rows[start..] {
                match row.key().physical_key_bytes().cmp(target_physical_key) {
                    std::cmp::Ordering::Less => {}
                    std::cmp::Ordering::Greater => {
                        let visited = rows_out.len();
                        perf_trace::record_table_point_rows_visited(visited);
                        return Ok((rows_out, visited));
                    }
                    std::cmp::Ordering::Equal => {
                        rows_out.push(row.clone());
                    }
                }
            }

            block_index = block_index.saturating_add(1);
        }

        let visited = rows_out.len();
        perf_trace::record_table_point_rows_visited(visited);
        Ok((rows_out, visited))
    }

    fn contains_physical_key(&self, physical_key: &[u8]) -> bool {
        let properties = self.metadata.properties();
        let min_key = table_internal_physical_key_bytes(properties.min_key_bytes());
        let max_key = table_internal_physical_key_bytes(properties.max_key_bytes());
        min_key <= physical_key && physical_key <= max_key
    }

    fn probe_physical_filter(&self, key: &TablePhysicalKeyBytes) -> TableBloomProbe {
        let probe = self
            .filter
            .as_ref()
            .map_or(TableBloomProbe::Unavailable, |filter| {
                filter.probe_physical_key(key)
            });
        // W2.2: the lazy point path's filter probes share the eager-probe
        // counters — together they are "reader point filter probes".
        match probe {
            TableBloomProbe::DefinitelyAbsent => {
                perf_trace::record_table_eager_filter_negative_probe();
            }
            TableBloomProbe::MaybePresent => {
                perf_trace::record_table_eager_filter_positive_probe();
            }
            TableBloomProbe::Unavailable => {
                perf_trace::record_table_eager_filter_unavailable_probe();
            }
        }
        probe
    }

    fn read_data_block_rows(
        &self,
        block_index: usize,
        trust: TableReadTrust,
    ) -> TableRuntimeResult<Vec<TableRow>> {
        let entry = self.metadata.index().entries().get(block_index).ok_or(
            TableRuntimeError::InvalidRange {
                field: "data_block_index",
            },
        )?;
        let frame = self.read_data_block_frame(entry, block_index)?;
        // W2.6 (B1): only TrustCache consumers skip re-verification on hits;
        // materialization, compaction merge inputs, and the oracle re-verify
        // (see `TableReadTrust`).
        let block = if frame.trusted && trust == TableReadTrust::TrustCache {
            perf_trace::record_table_trusted_block_seek();
            decode_immutable_table_data_block_trusted(entry, frame.bytes.as_ref())
        } else {
            perf_trace::record_table_checked_block_seek();
            decode_immutable_table_data_block(entry, frame.bytes.as_ref())
        }
        .map_err(|source| TableRuntimeError::DecodeFormat { source })?;
        perf_trace::record_table_data_blocks_decoded(1, block.row_count());
        if let Some((cache, key)) = frame.cache_insert {
            cache.insert(key, Arc::clone(&frame.bytes))?;
        }
        Ok(block.rows().cloned().map(TableRow::new).collect())
    }

    fn seek_data_block_point(
        &self,
        entry: &TableIndexEntry,
        block_index: usize,
        lookup: &TablePreparedPointLookup,
    ) -> TableRuntimeResult<TableDataBlockPointSeek> {
        let frame = self.read_data_block_frame(entry, block_index)?;
        // W2.6 (B1): cache hits skip the CRC pass and the payload copy — the
        // frame was verified at admission. Fresh source reads stay checked
        // (and only then insert), so corrupt frames never enter the cache.
        // W2.3 (B3): trusted hits additionally bisect via the derived
        // entry-offset accelerator instead of walking the block linearly.
        let seek = if frame.trusted {
            perf_trace::record_table_trusted_block_seek();
            self.seek_trusted_block_point(entry, block_index, &frame, lookup)
        } else {
            perf_trace::record_table_checked_block_seek();
            let seek = seek_immutable_table_data_block_point(
                entry,
                frame.bytes.as_ref(),
                lookup.seek_key().as_slice(),
                lookup.physical_key().as_slice(),
                lookup.max_commit_version(),
                lookup.max_commit_timestamp(),
            );
            // The frame just passed the full validating walk; derive its
            // accelerator in one cheap extra pass so the FIRST hit is already
            // indexed. Best-effort and fill-gated like the frame insert.
            if seek.is_ok() && self.fill_cache {
                self.insert_accelerator_best_effort(entry, block_index, frame.bytes.as_ref());
            }
            seek
        }
        .map_err(|source| TableRuntimeError::DecodeFormat { source })?;
        perf_trace::record_table_lazy_point_block_scan(
            seek.entries_scanned(),
            seek.candidate_rows_decoded(),
            usize::try_from(entry.row_count())
                .unwrap_or(usize::MAX)
                .saturating_sub(seek.candidate_rows_decoded()),
        );
        if let Some((cache, key)) = frame.cache_insert {
            cache.insert(key, Arc::clone(&frame.bytes))?;
        }
        Ok(seek)
    }

    /// W2.3 (B3): the trusted-hit seek — indexed via the cached accelerator
    /// when present, building it (one cheap pass over the verified payload's
    /// length prefixes) when absent. A malformed accelerator is rebuilt from
    /// the payload and the seek retried — self-healing, never a wrong answer
    /// (the indexed seek bounds-checks every probe).
    fn seek_trusted_block_point(
        &self,
        entry: &TableIndexEntry,
        block_index: usize,
        frame: &DataBlockFrame,
        lookup: &TablePreparedPointLookup,
    ) -> Result<TableDataBlockPointSeek, FormatError> {
        let indexed = |offsets: &[u32]| {
            seek_immutable_table_data_block_point_indexed(
                entry,
                frame.bytes.as_ref(),
                offsets,
                lookup.seek_key().as_slice(),
                lookup.physical_key().as_slice(),
                lookup.max_commit_version(),
                lookup.max_commit_timestamp(),
            )
        };
        let cached_offsets = self.cache.as_ref().and_then(|cache| {
            let key = accelerator_key_for_entry(&cache.table, entry, block_index).ok()?;
            let bytes = cache.cache.get_quiet(&key)?;
            decode_entry_offsets(&bytes)
        });
        if let Some(offsets) = cached_offsets {
            perf_trace::record_table_indexed_block_seek();
            match indexed(&offsets) {
                Ok(seek) => return Ok(seek),
                Err(_) => {
                    // Malformed accelerator (e.g. in-memory corruption):
                    // rebuild from the verified payload below.
                    perf_trace::record_table_accelerator_rebuild();
                }
            }
        }
        let offsets = build_immutable_table_data_block_entry_offsets(frame.bytes.as_ref())?;
        perf_trace::record_table_accelerator_build();
        if self.fill_cache {
            if let Some(cache) = &self.cache {
                if let Ok(key) = accelerator_key_for_entry(&cache.table, entry, block_index) {
                    // Rationale: accelerator admission is best-effort — a full
                    // shard costs a rebuild on the next hit, nothing more.
                    let _ = cache.cache.insert(key, encode_entry_offsets(&offsets));
                }
            }
        }
        perf_trace::record_table_indexed_block_seek();
        indexed(&offsets)
    }

    /// W2.3 (B3): derive + admit the accelerator for a block that just passed
    /// the checked validating walk (miss path). Best-effort by design.
    fn insert_accelerator_best_effort(
        &self,
        entry: &TableIndexEntry,
        block_index: usize,
        frame_bytes: &[u8],
    ) {
        let Some(cache) = &self.cache else {
            return;
        };
        let Ok(key) = accelerator_key_for_entry(&cache.table, entry, block_index) else {
            return;
        };
        let Ok(offsets) = build_immutable_table_data_block_entry_offsets(frame_bytes) else {
            return;
        };
        perf_trace::record_table_accelerator_build();
        // Rationale: best-effort admission; a skipped insert costs one cheap
        // rebuild on the first trusted hit.
        let _ = cache.cache.insert(key, encode_entry_offsets(&offsets));
    }

    fn read_data_block_frame(
        &self,
        entry: &TableIndexEntry,
        block_index: usize,
    ) -> TableRuntimeResult<DataBlockFrame> {
        let cache_key = self
            .cache
            .as_ref()
            .map(|cache| cache_key_for_entry(&cache.table, entry, block_index))
            .transpose()?;
        if let (Some(cache), Some(key)) = (&self.cache, &cache_key) {
            if let Some(bytes) = cache.cache.get(key) {
                return Ok(DataBlockFrame {
                    bytes,
                    cache_insert: None,
                    trusted: true,
                });
            }
        }

        let frame_len = usize::try_from(entry.block_frame_len()).map_err(|_| {
            TableRuntimeError::InvalidRange {
                field: "block_frame_len",
            }
        })?;
        let frame_bytes = read_exact_source(
            &self.source,
            entry.block_offset(),
            frame_len,
            "short table data block read",
        )?;
        perf_trace::record_table_data_block_read(frame_bytes.len());
        let bytes = Arc::<[u8]>::from(frame_bytes);
        // BS4.4g: a no-fill cursor reads the missed block but does not populate the cache.
        let cache_insert = if self.fill_cache {
            self.cache
                .as_ref()
                .zip(cache_key)
                .map(|(cache, key)| (Arc::clone(&cache.cache), key))
        } else {
            None
        };
        Ok(DataBlockFrame {
            bytes,
            cache_insert,
            trusted: false,
        })
    }
}

#[derive(Clone, Debug)]
struct LazyTableBlockCache {
    table: TableCacheTableId,
    cache: Arc<TableBlockCache>,
}

#[derive(Clone, Debug)]
struct DataBlockFrame {
    bytes: Arc<[u8]>,
    cache_insert: Option<(Arc<TableBlockCache>, TableBlockCacheKey)>,
    // W2.6 (B1): true ONLY for a block-cache hit — every frame in the cache
    // passed the checked decode at admission (demand path decodes before
    // insert; the warm path verifies before insert). NOT derivable from
    // `cache_insert` (a no-fill miss also carries None).
    trusted: bool,
}

/// W2.6 (B1): whether a consumer may skip re-verification on cache hits.
/// Consumers whose rows feed newly built tables or independent cross-checks
/// (materialization, merge inputs, the oracle) always verify, so an
/// in-memory-corrupted cached frame can never be laundered into a new table
/// with a fresh valid CRC.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TableReadTrust {
    TrustCache,
    AlwaysVerify,
}

/// C2: outcome of one bounded `warm_data_blocks_from_source` call.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TableWarmFromSourceReport {
    /// Blocks verified and admitted this call.
    pub(crate) admitted: usize,
    /// Blocks skipped because they were already cached.
    pub(crate) skipped_present: usize,
    /// No-evict admissions skipped on a full shard.
    pub(crate) skipped_full: usize,
    /// Source bytes read (verification rejects included — the frame was read).
    pub(crate) bytes_read: u64,
    /// Resume point: `Some(i)` when the byte bound stopped the walk before
    /// block `i`; `None` when the table is fully covered.
    pub(crate) next_block: Option<usize>,
    /// Length of the trailing run of consecutive full-shard skips, for the
    /// caller's saturation stop (a long streak means every shard the walk is
    /// hashing into is full, so further passes only burn IO).
    pub(crate) trailing_skipped_full: usize,
}

#[derive(Clone)]
struct SharedTableSource<'a> {
    inner: Arc<dyn TableByteSource + Send + Sync + 'a>,
}

impl<'a> SharedTableSource<'a> {
    fn new<S: TableByteSource + Send + Sync + 'a>(source: S) -> Self {
        Self {
            inner: Arc::new(source),
        }
    }
}

impl fmt::Debug for SharedTableSource<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SharedTableSource")
            .finish_non_exhaustive()
    }
}

impl TableByteSource for SharedTableSource<'_> {
    fn byte_count(&self) -> u64 {
        self.inner.byte_count()
    }

    fn read_at(&self, offset: u64, len: usize) -> TableRuntimeResult<Vec<u8>> {
        self.inner.read_at(offset, len)
    }

    fn exact_content_digest(&self) -> TableRuntimeResult<Option<[u8; 32]>> {
        self.inner.exact_content_digest()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TableReaderOpenMode {
    EagerBytes,
    EagerSource,
    LazySource,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TableReaderRuntimeFacts {
    open_mode: TableReaderOpenMode,
    flags: u8,
    data_blocks_loaded: u32,
    rows_materialized: u64,
}

const RUNTIME_FACT_METADATA_LOADED: u8 = 1 << 0;
const RUNTIME_FACT_INDEX_LOADED: u8 = 1 << 1;
const RUNTIME_FACT_FILTER_AVAILABLE: u8 = 1 << 2;
const RUNTIME_FACT_CACHE_ENABLED: u8 = 1 << 3;

impl TableReaderRuntimeFacts {
    const fn eager(
        open_mode: TableReaderOpenMode,
        data_blocks_loaded: u32,
        rows_materialized: u64,
    ) -> Self {
        Self {
            open_mode,
            flags: RUNTIME_FACT_METADATA_LOADED | RUNTIME_FACT_INDEX_LOADED,
            data_blocks_loaded,
            rows_materialized,
        }
    }

    const fn lazy(open_mode: TableReaderOpenMode) -> Self {
        Self {
            open_mode,
            flags: RUNTIME_FACT_METADATA_LOADED | RUNTIME_FACT_INDEX_LOADED,
            data_blocks_loaded: 0,
            rows_materialized: 0,
        }
    }

    const fn with_cache_enabled(mut self, enabled: bool) -> Self {
        if enabled {
            self.flags |= RUNTIME_FACT_CACHE_ENABLED;
        } else {
            self.flags &= !RUNTIME_FACT_CACHE_ENABLED;
        }
        self
    }

    const fn with_filter_available(mut self, available: bool) -> Self {
        if available {
            self.flags |= RUNTIME_FACT_FILTER_AVAILABLE;
        } else {
            self.flags &= !RUNTIME_FACT_FILTER_AVAILABLE;
        }
        self
    }

    pub(crate) const fn open_mode(self) -> TableReaderOpenMode {
        self.open_mode
    }

    pub(crate) const fn metadata_loaded(self) -> bool {
        self.flags & RUNTIME_FACT_METADATA_LOADED != 0
    }

    pub(crate) const fn index_loaded(self) -> bool {
        self.flags & RUNTIME_FACT_INDEX_LOADED != 0
    }

    pub(crate) const fn data_blocks_loaded(self) -> u32 {
        self.data_blocks_loaded
    }

    pub(crate) const fn rows_materialized(self) -> u64 {
        self.rows_materialized
    }

    pub(crate) const fn filter_available(self) -> bool {
        self.flags & RUNTIME_FACT_FILTER_AVAILABLE != 0
    }

    pub(crate) const fn cache_enabled(self) -> bool {
        self.flags & RUNTIME_FACT_CACHE_ENABLED != 0
    }
}

impl<'a> ImmutableTableReader<'a> {
    #[expect(
        clippy::needless_pass_by_value,
        reason = "owned Vec preserves the eager-open API symmetry with open_source and lets future implementations cache the buffer without changing call sites"
    )]
    pub(crate) fn open_bytes(
        identity: TableIdentity,
        bytes: Vec<u8>,
        config: TableReaderConfig,
    ) -> TableRuntimeResult<Self> {
        require_validate_on_open(config);
        perf_trace::record_table_reader_open();
        let (facts, fingerprint, rows, persisted_filter) = decode_reader_rows(identity, &bytes)?;
        // BS4.2b: a persisted filter always wins — load it (no scan) when present; otherwise fall back
        // to the row scan, which still honors `eager_filter_mode`.
        let rows = match persisted_filter {
            Some(frame) => {
                let filter =
                    TableReaderFilter::from_persisted_frame(facts.clone(), fingerprint, frame)?;
                EagerTableRows::from_vec_with_filter(rows, filter)
            }
            None => EagerTableRows::from_vec(facts.clone(), fingerprint, rows, config)?,
        };
        let runtime_facts = TableReaderRuntimeFacts::eager(
            TableReaderOpenMode::EagerBytes,
            facts.data_block_count(),
            facts.row_count(),
        )
        .with_filter_available(rows.filter.is_available());
        Ok(Self {
            config,
            facts,
            fingerprint,
            runtime_facts,
            rows: TableReaderRows::Eager(rows),
        })
    }

    pub(crate) fn open_source<S: TableByteSource + Send + Sync + 'a>(
        identity: TableIdentity,
        source: S,
        config: TableReaderConfig,
    ) -> TableRuntimeResult<Self> {
        require_validate_on_open(config);
        perf_trace::record_table_reader_open();
        let source = SharedTableSource::new(source);
        let (metadata, fingerprint) = read_table_metadata(&source)?;
        let facts = table_facts_from_metadata(identity, &metadata)?;
        // BS4.2b: load the persisted filter frame (a small targeted range read) before `source` moves
        // into the lazy state, so the lazy reader can short-circuit misses without reading data blocks.
        let filter = if metadata.has_filter() {
            let filter_len = usize::try_from(metadata.filter_block_frame_len()).map_err(|_| {
                TableRuntimeError::InvalidRange {
                    field: "filter_block_frame_len",
                }
            })?;
            let frame_bytes = read_exact_source(
                &source,
                metadata.filter_block_offset(),
                filter_len,
                "short table filter read",
            )?;
            let (frame, consumed) = decode_filter_frame(&frame_bytes)
                .map_err(|source| TableRuntimeError::DecodeFormat { source })?;
            if consumed != frame_bytes.len() {
                return Err(TableRuntimeError::InvalidRange {
                    field: "filter_block_frame_len",
                });
            }
            Some(TableReaderFilter::from_persisted_frame(
                facts.clone(),
                fingerprint,
                frame,
            )?)
        } else {
            None
        };
        // Attach the frame-loaded filter directly (not via `with_table_filter`): its
        // `facts`/`fingerprint` are already the reader's own, and the lazy fingerprint carries no
        // `content_sha256` (no full-content read), so the `matches_table` exact-content gate — meant
        // for externally supplied filters — would spuriously reject it. The frame's CRC (verified in
        // `decode_filter_frame`) is the real integrity guarantee.
        let mut lazy = LazyTableRows::new(source, metadata, config.materialization_policy());
        let filter_available = match filter {
            Some(filter) => lazy.with_filter(filter),
            None => false,
        };
        let runtime_facts = TableReaderRuntimeFacts::lazy(TableReaderOpenMode::LazySource)
            .with_filter_available(filter_available);
        Ok(Self {
            config,
            facts,
            fingerprint,
            runtime_facts,
            rows: TableReaderRows::Lazy(Box::new(lazy)),
        })
    }

    pub(crate) fn from_validated_rows(
        facts: TableRuntimeFacts,
        bytes: &[u8],
        rows: Vec<TableRow>,
        config: TableReaderConfig,
    ) -> TableRuntimeResult<ImmutableTableReader<'static>> {
        require_validate_on_open(config);
        perf_trace::record_table_reader_open();
        let fingerprint = table_content_fingerprint_from_bytes(bytes)?;
        let rows = EagerTableRows::from_vec(facts.clone(), fingerprint, rows, config)?;
        let runtime_facts = TableReaderRuntimeFacts::eager(
            TableReaderOpenMode::EagerBytes,
            facts.data_block_count(),
            facts.row_count(),
        )
        .with_filter_available(rows.filter.is_available());
        Ok(ImmutableTableReader {
            config,
            facts,
            fingerprint,
            runtime_facts,
            rows: TableReaderRows::Eager(rows),
        })
    }

    pub(crate) const fn config(&self) -> TableReaderConfig {
        self.config
    }

    pub(crate) const fn facts(&self) -> &TableRuntimeFacts {
        &self.facts
    }

    pub(crate) const fn runtime_facts(&self) -> TableReaderRuntimeFacts {
        self.runtime_facts
    }

    /// The reader's loaded filter, or `None` when unavailable/absent. Test-only: lets a test inspect
    /// the filter's shape to prove the persisted frame was loaded rather than rescanned.
    #[cfg(test)]
    pub(crate) fn loaded_filter(&self) -> Option<&TableReaderFilter> {
        self.rows.filter()
    }

    pub(crate) const fn byte_count(&self) -> u64 {
        self.facts.byte_count()
    }

    /// BS4.4j resident/object seam: the approximate in-RAM footprint of this reader, as opposed to
    /// `byte_count()` (the encoded object size used for compaction/level accounting). An eager reader
    /// holds the whole decoded object; a lazy (disk-resident) reader pins only its metadata (index +
    /// properties + loaded filter) — data blocks stay on disk and are accounted DB-wide by the block
    /// cache, so they are excluded here (counting them would double-count the cache's own resident total).
    pub(crate) fn resident_size_bytes(&self) -> u64 {
        match &self.rows {
            TableReaderRows::Eager(_) => self.byte_count(),
            TableReaderRows::Lazy(lazy) => lazy.state.metadata.resident_metadata_bytes(),
        }
    }

    pub(crate) fn rows(&self) -> &[TableRow] {
        self.try_rows()
            .expect("lazy table row materialization failed")
    }

    pub(crate) fn try_rows(&self) -> TableRuntimeResult<&[TableRow]> {
        self.rows.try_rows()
    }

    /// Materialize every row for a **debug oracle**, bypassing the BS4.4d `DenyRuntime` guard and
    /// the lazy-materialization counter that `rows()`/`try_rows()` enforce.
    ///
    /// The guard exists to catch a durable *consumer* forcing a full read of a lazy table. Debug
    /// oracles (extras provenance, read-view facts, duplicate-key, durable one-pass facts) legitimately
    /// need the full row set to cross-check a facts-based fast path; this is their only sanctioned path
    /// to it. Returns an owned copy and never populates the row cache. Source-guarded to the oracle call
    /// sites (`api/tests/source_guards.rs`).
    pub(crate) fn materialize_rows_for_oracle(&self) -> TableRuntimeResult<Vec<TableRow>> {
        self.rows.materialize_for_oracle()
    }

    pub(crate) fn get_exact(&self, key: &TableInternalKeyBytes) -> Option<TableRow> {
        self.try_get_exact(key)
            .expect("lazy table exact lookup failed")
    }

    pub(crate) fn try_get_exact(
        &self,
        key: &TableInternalKeyBytes,
    ) -> TableRuntimeResult<Option<TableRow>> {
        self.rows.try_get_exact(key)
    }

    pub(crate) fn seek_physical_key(
        &self,
        key: &PhysicalKey,
        max_commit_version: Option<CommitVersion>,
        max_commit_timestamp: Option<Timestamp>,
    ) -> (Option<TableRow>, usize) {
        let lookup = TablePreparedPointLookup::new(key, max_commit_version, max_commit_timestamp);
        self.seek_prepared_point(&lookup)
    }

    pub(crate) fn seek_prepared_point(
        &self,
        lookup: &TablePreparedPointLookup,
    ) -> (Option<TableRow>, usize) {
        self.try_seek_prepared_point(lookup)
            .expect("lazy table physical key seek failed")
    }

    pub(crate) fn try_seek_physical_key(
        &self,
        key: &PhysicalKey,
        max_commit_version: Option<CommitVersion>,
        max_commit_timestamp: Option<Timestamp>,
    ) -> TableRuntimeResult<(Option<TableRow>, usize)> {
        let lookup = TablePreparedPointLookup::new(key, max_commit_version, max_commit_timestamp);
        self.try_seek_prepared_point(&lookup)
    }

    pub(crate) fn try_seek_prepared_point(
        &self,
        lookup: &TablePreparedPointLookup,
    ) -> TableRuntimeResult<(Option<TableRow>, usize)> {
        self.rows.try_seek_prepared_point(lookup)
    }

    pub(crate) fn try_seek_prepared_point_candidate(
        &self,
        lookup: &TablePreparedPointLookup,
    ) -> TableRuntimeResult<(Option<TablePointLookupRow<'_>>, usize)> {
        self.rows.try_seek_prepared_point_candidate(lookup)
    }

    pub(crate) fn try_physical_key_rows(
        &self,
        key: &PhysicalKey,
    ) -> TableRuntimeResult<(Vec<TableRow>, usize)> {
        self.rows.try_physical_key_rows(key)
    }

    pub(crate) fn cursor(&self) -> ImmutableTableCursor<'_, 'a> {
        self.rows.cursor(None, true)
    }

    /// BS4.4g: a cursor that reads missed blocks but never inserts them into the block cache — for
    /// read-once compaction/materialization merge inputs, so streaming a large table cannot evict the
    /// point-read working set.
    pub(crate) fn cursor_without_cache_fill(&self) -> ImmutableTableCursor<'_, 'a> {
        self.rows.cursor(None, false)
    }

    pub(crate) fn bounded_cursor(&self, bounds: TableKeyBounds) -> BoundedTableCursor<'_> {
        BoundedTableCursor::new(
            Box::new(self.rows.cursor(Some(bounds.clone()), true)),
            bounds,
        )
    }

    /// BS4.4g: the no-cache-fill counterpart of [`bounded_cursor`](Self::bounded_cursor).
    pub(crate) fn bounded_cursor_without_cache_fill(
        &self,
        bounds: TableKeyBounds,
    ) -> BoundedTableCursor<'_> {
        BoundedTableCursor::new(
            Box::new(self.rows.cursor(Some(bounds.clone()), false)),
            bounds,
        )
    }

    pub(crate) fn into_materialized(self) -> TableRuntimeResult<ImmutableTableReader<'static>> {
        let config = self.config;
        let facts = self.facts;
        let runtime_facts = self.runtime_facts;
        let (rows, was_lazy) = self
            .rows
            .into_materialized(facts.clone(), self.fingerprint)?;
        let runtime_facts = if was_lazy {
            TableReaderRuntimeFacts::eager(
                TableReaderOpenMode::EagerSource,
                facts.data_block_count(),
                facts.row_count(),
            )
            .with_filter_available(rows.filter.is_available())
        } else {
            runtime_facts
        };
        Ok(ImmutableTableReader {
            config,
            facts,
            fingerprint: self.fingerprint,
            runtime_facts,
            rows: TableReaderRows::Eager(rows),
        })
    }

    pub(crate) fn with_block_cache(
        mut self,
        cache: Arc<TableBlockCache>,
    ) -> TableRuntimeResult<Self> {
        let table = TableCacheTableId::new(self.facts.identity().as_str().as_bytes())?;
        let cache_enabled = self.rows.with_block_cache(table, cache);
        self.runtime_facts = self.runtime_facts.with_cache_enabled(cache_enabled);
        Ok(self)
    }

    /// W2.4: warm the block cache with this table's data blocks sliced from
    /// its just-encoded bytes (publish-time write-through). No-evict inserts
    /// only — freshly built blocks are unproven and must never displace
    /// demand-cached ones. Returns the number of blocks admitted. No-op for
    /// eager readers and readers without an attached cache.
    pub(crate) fn warm_data_blocks_from_encoded(
        &self,
        encoded: &[u8],
    ) -> TableRuntimeResult<usize> {
        let TableReaderRows::Lazy(lazy) = &self.rows else {
            return Ok(0);
        };
        let state = &lazy.state;
        let Some(cache) = &state.cache else {
            return Ok(0);
        };
        let mut admitted = 0usize;
        for (block_index, entry) in state.metadata.index().entries().iter().enumerate() {
            let start = usize::try_from(entry.block_offset()).map_err(|_| {
                TableRuntimeError::InvalidRange {
                    field: "block_offset",
                }
            })?;
            let len = usize::try_from(entry.block_frame_len()).map_err(|_| {
                TableRuntimeError::InvalidRange {
                    field: "block_frame_len",
                }
            })?;
            let end = start
                .checked_add(len)
                .filter(|end| *end <= encoded.len())
                .ok_or(TableRuntimeError::InvalidRange {
                    field: "block_frame_len",
                })?;
            let key = cache_key_for_entry(&cache.table, entry, block_index)?;
            // W2.6 (B1): trusting consumers skip re-verification on cache
            // hits, so "only checked frames enter the cache" must be airtight
            // — verify each sliced frame before admission. The bytes were
            // just encoded in-process; a failure here indicates a slicing or
            // buffer-mixup bug, not disk corruption, so it is rejected (and
            // counted), never inserted.
            let slice = &encoded[start..end];
            if decode_immutable_table_data_block(entry, slice).is_err() {
                // Rationale: reject-and-continue (never panic) — the demand
                // path re-reads and re-verifies from the source on the next
                // miss, so a rejected warm frame costs one cache miss and
                // nothing more; the counter is the bug signal.
                perf_trace::record_table_warm_insert_reject();
                continue;
            }
            let frame: Arc<[u8]> = Arc::from(slice);
            match cache.cache.insert_if_free(key, frame)? {
                CacheInsert::Inserted(_) => admitted = admitted.saturating_add(1),
                CacheInsert::DuplicateExisting(_)
                | CacheInsert::SkippedDisabled(_)
                | CacheInsert::SkippedOversized(_)
                | CacheInsert::SkippedFull(_) => {}
            }
        }
        Ok(admitted)
    }

    /// C2: warm the block cache with this table's data blocks read back from
    /// its SOURCE (the background preheat path — the bytes are no longer in
    /// hand, unlike the publish-time write-through above). Same admission
    /// contract as W2.4: verify every frame before insertion, no-evict
    /// inserts only, reject-and-continue on verification failure. Bounded:
    /// stops once `max_bytes` of source bytes have been read and reports the
    /// resume block, so one call is one polite chunk of a larger sweep.
    /// No-op (all-zero report) for eager readers and readers without an
    /// attached cache.
    pub(crate) fn warm_data_blocks_from_source(
        &self,
        start_block: usize,
        max_bytes: u64,
    ) -> TableRuntimeResult<TableWarmFromSourceReport> {
        let mut report = TableWarmFromSourceReport::default();
        let TableReaderRows::Lazy(lazy) = &self.rows else {
            return Ok(report);
        };
        let state = &lazy.state;
        let Some(cache) = &state.cache else {
            return Ok(report);
        };
        let entries = state.metadata.index().entries();
        for (block_index, entry) in entries.iter().enumerate().skip(start_block) {
            if report.bytes_read >= max_bytes {
                report.next_block = Some(block_index);
                return Ok(report);
            }
            let key = cache_key_for_entry(&cache.table, entry, block_index)?;
            if cache.cache.contains_quiet(&key) {
                report.skipped_present = report.skipped_present.saturating_add(1);
                report.trailing_skipped_full = 0;
                continue;
            }
            let frame_len = usize::try_from(entry.block_frame_len()).map_err(|_| {
                TableRuntimeError::InvalidRange {
                    field: "block_frame_len",
                }
            })?;
            let frame_bytes = read_exact_source(
                &state.source,
                entry.block_offset(),
                frame_len,
                "short table data block preheat read",
            )?;
            report.bytes_read = report.bytes_read.saturating_add(frame_bytes.len() as u64);
            // W2.6 (B1): "only checked frames enter the cache" holds on this
            // path too — the frame came off disk, so a failure here IS
            // possible corruption. Reject-and-continue: the demand path
            // re-reads and surfaces the typed error to a real reader; a
            // silent preheat must not turn one bad block into a failed sweep.
            if decode_immutable_table_data_block(entry, &frame_bytes).is_err() {
                perf_trace::record_table_warm_insert_reject();
                report.trailing_skipped_full = 0;
                continue;
            }
            // FAIR insert, unlike the publish-time warm above (W2.4's
            // no-evict rule protects the demand set from UNPROVEN fresh
            // blocks mid-load). The preheat walks the LIVE canonical table
            // set while maintenance is idle: whatever LRU evicts to admit a
            // live block is colder by recency — dead tables' blocks from
            // load-churn publishes are never touched again, so they age to
            // the LRU end and are displaced first. No-evict here starved the
            // fill against exactly that garbage (measured: full-shard skips
            // at ~40% live occupancy).
            match cache.cache.insert(key, Arc::from(frame_bytes))? {
                CacheInsert::Inserted(_) => {
                    report.admitted = report.admitted.saturating_add(1);
                    report.trailing_skipped_full = 0;
                }
                CacheInsert::SkippedFull(_) => {
                    report.skipped_full = report.skipped_full.saturating_add(1);
                    report.trailing_skipped_full = report.trailing_skipped_full.saturating_add(1);
                }
                CacheInsert::DuplicateExisting(_)
                | CacheInsert::SkippedDisabled(_)
                | CacheInsert::SkippedOversized(_) => {
                    report.trailing_skipped_full = 0;
                }
            }
        }
        Ok(report)
    }

    pub(crate) fn with_table_filter(
        mut self,
        filter: TableReaderFilter,
    ) -> TableRuntimeResult<Self> {
        if !filter.matches_table(&self.facts, self.fingerprint) {
            return Err(TableRuntimeError::InvalidConfig {
                field: "table_filter",
                reason: "must match table bytes",
            });
        }
        let filter_available = self.rows.with_filter(filter);
        self.runtime_facts = self.runtime_facts.with_filter_available(filter_available);
        Ok(self)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ImmutableTableCursor<'rows, 'source> {
    state: ImmutableTableCursorState<'rows, 'source>,
}

#[derive(Clone, Debug)]
enum ImmutableTableCursorState<'rows, 'source> {
    Eager {
        rows: &'rows [TableRow],
        position: Option<usize>,
    },
    Lazy(Box<LazyImmutableTableCursor<'source>>),
    Failed(TableRuntimeError),
}

impl<'rows, 'source> ImmutableTableCursor<'rows, 'source> {
    fn eager(rows: &'rows [TableRow]) -> Self {
        Self {
            state: ImmutableTableCursorState::Eager {
                rows,
                position: None,
            },
        }
    }

    fn lazy(state: LazyTableState<'source>, bounds_hint: Option<TableKeyBounds>) -> Self {
        Self {
            state: ImmutableTableCursorState::Lazy(Box::new(LazyImmutableTableCursor::new(
                state,
                bounds_hint,
            ))),
        }
    }

    fn failed(error: TableRuntimeError) -> Self {
        Self {
            state: ImmutableTableCursorState::Failed(error),
        }
    }

    fn eager_seek_index(rows: &[TableRow], target: &TableInternalKeyBytes) -> Option<usize> {
        match rows.binary_search_by(|row| row.key().cmp(target)) {
            Ok(index) | Err(index) if index < rows.len() => Some(index),
            Ok(_) | Err(_) => None,
        }
    }
}

#[derive(Clone, Debug)]
struct LazyImmutableTableCursor<'source> {
    state: LazyTableState<'source>,
    bounds_hint: Option<TableKeyBounds>,
    block_index: Option<usize>,
    block_rows: Vec<TableRow>,
    row_index: Option<usize>,
}

impl<'source> LazyImmutableTableCursor<'source> {
    fn new(state: LazyTableState<'source>, bounds_hint: Option<TableKeyBounds>) -> Self {
        Self {
            state,
            bounds_hint,
            block_index: None,
            block_rows: Vec::new(),
            row_index: None,
        }
    }

    fn seek_to_first(&mut self) -> TableRuntimeResult<()> {
        if let Some(lower) = self
            .bounds_hint
            .as_ref()
            .and_then(TableKeyBounds::lower_seek_key)
        {
            return self.seek(&lower);
        }
        self.position_at_block(0, None)
    }

    fn seek(&mut self, target: &TableInternalKeyBytes) -> TableRuntimeResult<()> {
        let entries = self.state.metadata.index().entries();
        let Some(block_index) = first_candidate_block_for_key(entries, target) else {
            self.clear_position();
            return Ok(());
        };
        self.position_at_block(block_index, Some(target))
    }

    fn advance(&mut self) -> TableRuntimeResult<()> {
        let Some(row_index) = self.row_index else {
            return Ok(());
        };
        let next_row = row_index.saturating_add(1);
        if next_row < self.block_rows.len() {
            self.row_index = Some(next_row);
            record_cursor_position(self.row_index);
            return Ok(());
        }

        let Some(block_index) = self.block_index else {
            self.clear_position();
            return Ok(());
        };
        self.position_at_block(block_index.saturating_add(1), None)
    }

    fn current(&self) -> Option<&TableRow> {
        self.row_index
            .and_then(|row_index| self.block_rows.get(row_index))
    }

    fn position_at_block(
        &mut self,
        mut block_index: usize,
        target: Option<&TableInternalKeyBytes>,
    ) -> TableRuntimeResult<()> {
        while block_index < self.state.metadata.index().entries().len() {
            if self.entry_is_past_upper_bound(block_index) {
                self.clear_position();
                return Ok(());
            }

            // BS4.4g gave merge-input readers their own no-fill
            // constructors; those cursors re-verify cached frames (their
            // rows feed newly built tables). Interactive cursors trust.
            let trust = if self.state.fill_cache {
                TableReadTrust::TrustCache
            } else {
                TableReadTrust::AlwaysVerify
            };
            let rows = self.state.read_data_block_rows(block_index, trust)?;
            let row_index =
                target.map_or(0, |target| candidate_row_index_for_seek_key(&rows, target));
            if row_index < rows.len() {
                self.block_index = Some(block_index);
                self.block_rows = rows;
                self.row_index = Some(row_index);
                record_cursor_position(self.row_index);
                return Ok(());
            }

            block_index = block_index.saturating_add(1);
        }
        self.clear_position();
        Ok(())
    }

    fn entry_is_past_upper_bound(&self, block_index: usize) -> bool {
        let Some(bounds) = &self.bounds_hint else {
            return false;
        };
        let Some(entry) = self.state.metadata.index().entries().get(block_index) else {
            return false;
        };
        bounds.is_past_upper_bound_bytes(entry.first_key_bytes())
    }

    fn clear_position(&mut self) {
        self.block_index = None;
        self.block_rows.clear();
        self.row_index = None;
    }
}

impl super::TableCursor for ImmutableTableCursor<'_, '_> {
    fn seek_to_first(&mut self) -> TableRuntimeResult<()> {
        match &mut self.state {
            ImmutableTableCursorState::Eager { rows, position } => {
                *position = if rows.is_empty() { None } else { Some(0) };
                record_cursor_position(*position);
                Ok(())
            }
            ImmutableTableCursorState::Lazy(cursor) => cursor.seek_to_first(),
            ImmutableTableCursorState::Failed(error) => Err(error.clone()),
        }
    }

    fn seek(&mut self, target: &TableInternalKeyBytes) -> TableRuntimeResult<()> {
        match &mut self.state {
            ImmutableTableCursorState::Eager { rows, position } => {
                *position = Self::eager_seek_index(rows, target);
                record_cursor_position(*position);
                Ok(())
            }
            ImmutableTableCursorState::Lazy(cursor) => cursor.seek(target),
            ImmutableTableCursorState::Failed(error) => Err(error.clone()),
        }
    }

    fn advance(&mut self) -> TableRuntimeResult<()> {
        match &mut self.state {
            ImmutableTableCursorState::Eager { rows, position } => {
                *position = position.and_then(|position| {
                    let next = position.saturating_add(1);
                    (next < rows.len()).then_some(next)
                });
                record_cursor_position(*position);
                Ok(())
            }
            ImmutableTableCursorState::Lazy(cursor) => cursor.advance(),
            ImmutableTableCursorState::Failed(error) => Err(error.clone()),
        }
    }

    fn current(&self) -> Option<&TableRow> {
        match &self.state {
            ImmutableTableCursorState::Eager { rows, position } => {
                position.and_then(|position| rows.get(position))
            }
            ImmutableTableCursorState::Lazy(cursor) => cursor.current(),
            ImmutableTableCursorState::Failed(_) => None,
        }
    }
}

fn seek_prepared_point_in_slice<'a>(
    rows: &'a [TableRow],
    lookup: &TablePreparedPointLookup,
    filter: Option<&TableReaderFilter>,
) -> (Option<&'a TableRow>, usize) {
    perf_trace::record_table_seek();
    lookup.record_table_seek_use();
    let prefix = lookup.physical_key();
    match probe_eager_physical_filter(filter, prefix) {
        TableBloomProbe::DefinitelyAbsent => {
            perf_trace::record_table_point_rows_visited(0);
            return (None, 0);
        }
        TableBloomProbe::MaybePresent | TableBloomProbe::Unavailable => {}
    }
    let seek_key = lookup.seek_key();
    let start = match rows.binary_search_by(|row| row.key().cmp(seek_key)) {
        Ok(index) | Err(index) if index < rows.len() => index,
        Ok(_) | Err(_) => {
            perf_trace::record_table_point_rows_visited(0);
            return (None, 0);
        }
    };

    let mut visited = 0usize;
    for row in &rows[start..] {
        visited = visited.saturating_add(1);
        if !prefix.is_prefix_of(row.key()) {
            break;
        }
        if row_matches_point_bound(
            row,
            lookup.max_commit_version(),
            lookup.max_commit_timestamp(),
        ) {
            perf_trace::record_table_point_rows_visited(visited);
            return (Some(row), visited);
        }
    }
    perf_trace::record_table_point_rows_visited(visited);
    (None, visited)
}

fn probe_eager_physical_filter(
    filter: Option<&TableReaderFilter>,
    prefix: &TablePhysicalKeyBytes,
) -> TableBloomProbe {
    let probe = filter.map_or(TableBloomProbe::Unavailable, |filter| {
        filter.probe_physical_key(prefix)
    });
    match probe {
        TableBloomProbe::DefinitelyAbsent => perf_trace::record_table_eager_filter_negative_probe(),
        TableBloomProbe::MaybePresent => perf_trace::record_table_eager_filter_positive_probe(),
        TableBloomProbe::Unavailable => perf_trace::record_table_eager_filter_unavailable_probe(),
    }
    probe
}

fn physical_key_rows_in_slice(rows: &[TableRow], key: &PhysicalKey) -> (Vec<TableRow>, usize) {
    perf_trace::record_table_seek();
    let prefix = TablePhysicalKeyBytes::from_physical_key(key);
    let seek_key = TableInternalKeyBytes::from_internal_key(&InternalKey::new(
        key.clone(),
        CommitVersion::MAX,
    ));
    let start = match rows.binary_search_by(|row| row.key().cmp(&seek_key)) {
        Ok(index) | Err(index) if index < rows.len() => index,
        Ok(_) | Err(_) => {
            perf_trace::record_table_point_rows_visited(0);
            return (Vec::new(), 0);
        }
    };

    let mut matching = Vec::new();
    for row in &rows[start..] {
        if !prefix.is_prefix_of(row.key()) {
            break;
        }
        matching.push(row.clone());
    }
    let visited = matching.len();
    perf_trace::record_table_point_rows_visited(visited);
    (matching, visited)
}

fn record_cursor_position(position: Option<usize>) {
    if position.is_some() {
        perf_trace::record_table_cursor_row_visited();
    }
}

fn row_matches_point_bound(
    row: &TableRow,
    max_commit_version: Option<CommitVersion>,
    max_commit_timestamp: Option<Timestamp>,
) -> bool {
    max_commit_version.is_none_or(|version| row.commit_version().as_u64() <= version.as_u64())
        && max_commit_timestamp.is_none_or(|timestamp| row.commit_timestamp() <= timestamp)
}

fn require_validate_on_open(config: TableReaderConfig) {
    match config.validation_mode() {
        super::TableReaderValidationMode::ValidateOnOpen => {}
    }
}

fn decode_reader_rows(
    identity: TableIdentity,
    bytes: &[u8],
) -> TableRuntimeResult<(
    TableRuntimeFacts,
    TableContentFingerprint,
    Vec<TableRow>,
    Option<TableFilterFrame>,
)> {
    let decoded = decode_immutable_table(bytes)
        .map_err(|source| TableRuntimeError::DecodeFormat { source })?;
    perf_trace::record_table_data_blocks_decoded(decoded.data_blocks().len(), decoded.rows().len());
    let rows = decoded
        .rows()
        .iter()
        .cloned()
        .map(TableRow::new)
        .collect::<Vec<_>>();
    super::validate_strictly_sorted_unique_rows(&rows)?;
    let facts = table_facts_from_decoded(identity, bytes, &decoded)?;
    let fingerprint = table_content_fingerprint_from_bytes(bytes)?;
    // BS4.2b: carry the persisted filter frame (BS4.2a already decoded + validated it) so the eager
    // path can load it instead of rescanning every key.
    let persisted_filter = decoded.filter().cloned();
    Ok((facts, fingerprint, rows, persisted_filter))
}

fn validate_filter_rows_match_facts(
    facts: &TableRuntimeFacts,
    rows: &[TableRow],
) -> TableRuntimeResult<()> {
    if u64::try_from(rows.len()).map_err(|_| TableRuntimeError::InvalidRange {
        field: "table_filter_row_count",
    })? != facts.row_count()
    {
        return Err(TableRuntimeError::InvalidRange {
            field: "table_filter_row_count",
        });
    }
    super::validate_strictly_sorted_unique_rows(rows)?;
    let first = rows.first().ok_or(TableRuntimeError::InvalidRange {
        field: "table_filter_rows",
    })?;
    let last = rows.last().ok_or(TableRuntimeError::InvalidRange {
        field: "table_filter_rows",
    })?;
    if first.encoded_key() != facts.key_range().first_key()
        || last.encoded_key() != facts.key_range().last_key()
    {
        return Err(TableRuntimeError::InvalidRange {
            field: "table_filter_key_range",
        });
    }
    let min_commit =
        rows.iter()
            .map(TableRow::commit_version)
            .min()
            .ok_or(TableRuntimeError::InvalidRange {
                field: "table_filter_commit_range",
            })?;
    let max_commit =
        rows.iter()
            .map(TableRow::commit_version)
            .max()
            .ok_or(TableRuntimeError::InvalidRange {
                field: "table_filter_commit_range",
            })?;
    if TableCommitRange::new(min_commit, max_commit)? != facts.commit_range() {
        return Err(TableRuntimeError::InvalidRange {
            field: "table_filter_commit_range",
        });
    }
    Ok(())
}

fn build_filter_from_decoded_rows(
    facts: TableRuntimeFacts,
    fingerprint: TableContentFingerprint,
    rows: &[TableRow],
    bits_per_key: usize,
) -> TableRuntimeResult<TableReaderFilter> {
    TableReaderFilter::from_validated_rows(facts, fingerprint, rows, bits_per_key)
}

fn build_eager_filter(
    facts: TableRuntimeFacts,
    fingerprint: TableContentFingerprint,
    rows: &[TableRow],
    config: TableReaderConfig,
) -> TableRuntimeResult<TableReaderFilter> {
    match config.eager_filter_mode() {
        TableReaderEagerFilterMode::BuildOnOpen => build_filter_from_decoded_rows(
            facts,
            fingerprint,
            rows,
            DEFAULT_EAGER_FILTER_BITS_PER_KEY,
        ),
        TableReaderEagerFilterMode::Unavailable => {
            validate_filter_rows_match_facts(&facts, rows)?;
            Ok(TableReaderFilter::unavailable())
        }
    }
}

fn read_table_metadata(
    source: &impl TableByteSource,
) -> TableRuntimeResult<(ImmutableTableMetadata, TableContentFingerprint)> {
    let byte_count = source.byte_count();
    let footer_offset = byte_count.checked_sub(MAX_TABLE_FOOTER_SIZE as u64).ok_or(
        TableRuntimeError::InvalidRange {
            field: "byte_count",
        },
    )?;
    let header_bytes =
        read_exact_source(source, 0, MAX_TABLE_HEADER_SIZE, "short table header read")?;
    perf_trace::record_table_metadata_read(header_bytes.len());
    let footer_bytes = read_exact_source(
        source,
        footer_offset,
        MAX_TABLE_FOOTER_SIZE,
        "short table footer read",
    )?;
    perf_trace::record_table_metadata_read(footer_bytes.len());

    let (_, header_len) = decode_table_header(&header_bytes)
        .map_err(|source| TableRuntimeError::DecodeFormat { source })?;
    if header_len != MAX_TABLE_HEADER_SIZE {
        return Err(TableRuntimeError::DecodeFormat {
            source: crate::format::FormatError::InvalidLength {
                field: "table_header_size",
            },
        });
    }
    let footer = decode_table_footer_metadata(
        &footer_bytes,
        usize::try_from(footer_offset).map_err(|_| TableRuntimeError::InvalidRange {
            field: "footer_offset",
        })?,
    )
    .map_err(|source| TableRuntimeError::DecodeFormat { source })?;
    let index_len = usize::try_from(footer.index_block_frame_len()).map_err(|_| {
        TableRuntimeError::InvalidRange {
            field: "index_block_frame_len",
        }
    })?;
    let properties_len = usize::try_from(footer.properties_block_frame_len()).map_err(|_| {
        TableRuntimeError::InvalidRange {
            field: "properties_block_frame_len",
        }
    })?;
    let index_bytes = read_exact_source(
        source,
        footer.index_block_offset(),
        index_len,
        "short table index read",
    )?;
    perf_trace::record_table_index_read(index_bytes.len());
    let properties_bytes = read_exact_source(
        source,
        footer.properties_block_offset(),
        properties_len,
        "short table properties read",
    )?;
    perf_trace::record_table_properties_read(properties_bytes.len());
    let mut metadata_fingerprint = table_content_fingerprint_from_parts(
        byte_count,
        &header_bytes,
        &footer_bytes,
        &index_bytes,
        &properties_bytes,
    )?;
    let metadata = decode_immutable_table_metadata(
        byte_count,
        &header_bytes,
        &footer_bytes,
        &index_bytes,
        &properties_bytes,
    )
    .map_err(|source| TableRuntimeError::DecodeFormat { source })?;
    metadata_fingerprint.content_sha256 = source.exact_content_digest()?;
    let fingerprint = metadata_fingerprint;
    Ok((metadata, fingerprint))
}

fn table_content_fingerprint_from_bytes(
    bytes: &[u8],
) -> TableRuntimeResult<TableContentFingerprint> {
    let byte_count = u64::try_from(bytes.len()).map_err(|_| TableRuntimeError::InvalidRange {
        field: "byte_count",
    })?;
    let header_bytes =
        bytes
            .get(..MAX_TABLE_HEADER_SIZE)
            .ok_or(TableRuntimeError::InvalidRange {
                field: "table_header",
            })?;
    let footer_start =
        bytes
            .len()
            .checked_sub(MAX_TABLE_FOOTER_SIZE)
            .ok_or(TableRuntimeError::InvalidRange {
                field: "byte_count",
            })?;
    let footer_bytes = &bytes[footer_start..];
    let footer = decode_table_footer_metadata(footer_bytes, footer_start)
        .map_err(|source| TableRuntimeError::DecodeFormat { source })?;
    let index_bytes = table_fingerprint_range(
        bytes,
        footer.index_block_offset(),
        footer.index_block_frame_len(),
        "index_block",
    )?;
    let properties_bytes = table_fingerprint_range(
        bytes,
        footer.properties_block_offset(),
        footer.properties_block_frame_len(),
        "properties_block",
    )?;
    let mut fingerprint = table_content_fingerprint_from_parts(
        byte_count,
        header_bytes,
        footer_bytes,
        index_bytes,
        properties_bytes,
    )?;
    fingerprint.content_sha256 = Some(table_content_digest(bytes));
    Ok(fingerprint)
}

fn table_content_fingerprint_from_parts(
    byte_count: u64,
    header_bytes: &[u8],
    footer_bytes: &[u8],
    index_bytes: &[u8],
    properties_bytes: &[u8],
) -> TableRuntimeResult<TableContentFingerprint> {
    let crc_offset =
        MAX_TABLE_FOOTER_SIZE
            .checked_sub(4)
            .ok_or(TableRuntimeError::InvalidRange {
                field: "table_crc32",
            })?;
    let crc_end = crc_offset
        .checked_add(4)
        .ok_or(TableRuntimeError::InvalidRange {
            field: "table_crc32",
        })?;
    let crc32 = u32::from_le_bytes(
        footer_bytes
            .get(crc_offset..crc_end)
            .ok_or(TableRuntimeError::InvalidRange {
                field: "table_crc32",
            })?
            .try_into()
            .map_err(|_| TableRuntimeError::InvalidRange {
                field: "table_crc32",
            })?,
    );
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(header_bytes);
    hasher.update(footer_bytes);
    hasher.update(index_bytes);
    hasher.update(properties_bytes);
    Ok(TableContentFingerprint {
        byte_count,
        table_crc32: crc32,
        metadata_crc32: hasher.finalize(),
        content_sha256: None,
    })
}

fn table_content_digest(bytes: &[u8]) -> [u8; 32] {
    let digest = Sha256::digest(bytes);
    let mut output = [0; 32];
    output.copy_from_slice(&digest);
    output
}

fn table_fingerprint_range<'a>(
    bytes: &'a [u8],
    offset: u64,
    len: u32,
    field: &'static str,
) -> TableRuntimeResult<&'a [u8]> {
    let start = usize::try_from(offset).map_err(|_| TableRuntimeError::InvalidRange { field })?;
    let len = usize::try_from(len).map_err(|_| TableRuntimeError::InvalidRange { field })?;
    let end = start
        .checked_add(len)
        .ok_or(TableRuntimeError::InvalidRange { field })?;
    bytes
        .get(start..end)
        .ok_or(TableRuntimeError::InvalidRange { field })
}

fn read_rows_from_metadata(state: &LazyTableState<'_>) -> TableRuntimeResult<Vec<TableRow>> {
    let mut rows = Vec::new();
    for block_index in 0..state.metadata.index().entries().len() {
        // Full materialization feeds eager rows (and the oracle cross-check
        // path) — always re-verify cached frames.
        rows.extend(state.read_data_block_rows(block_index, TableReadTrust::AlwaysVerify)?);
    }
    Ok(rows)
}

fn read_and_validate_rows(state: &LazyTableState<'_>) -> TableRuntimeResult<Vec<TableRow>> {
    // BS4.4d: the single choke point where a lazy reader decodes every row (reached by `try_rows` and
    // `into_materialized`). Under `DenyRuntime` a full read is a bug on the durable path — debug-assert
    // to fail loudly in tests, then return a typed error in release. `Allow` readers tally the counter.
    if state.materialization_policy == TableMaterializationPolicy::DenyRuntime {
        debug_assert!(
            false,
            "lazy table materialized all rows under DenyRuntime — a durable consumer forced a full read"
        );
        return Err(TableRuntimeError::LazyMaterializationDenied {
            reason: "lazy reader full materialization denied by runtime policy",
        });
    }
    perf_trace::record_lazy_full_materialization();
    let rows = read_rows_from_metadata(state)?;
    super::validate_strictly_sorted_unique_rows(&rows)?;
    Ok(rows)
}

fn find_exact_in_rows(rows: &[TableRow], key: &TableInternalKeyBytes) -> Option<TableRow> {
    rows.binary_search_by(|row| row.key().cmp(key))
        .ok()
        .map(|index| rows[index].clone())
}

fn find_index_entry_for_key(
    metadata: &ImmutableTableMetadata,
    key: &TableInternalKeyBytes,
) -> Option<usize> {
    let key = key.as_slice();
    let entries = metadata.index().entries();
    let index = entries.partition_point(|entry| entry.last_key_bytes() < key);
    let entry = entries.get(index)?;
    (entry.first_key_bytes() <= key).then_some(index)
}

fn first_candidate_block_for_key(
    entries: &[TableIndexEntry],
    key: &TableInternalKeyBytes,
) -> Option<usize> {
    let key = key.as_slice();
    let index = entries.partition_point(|entry| entry.last_key_bytes() < key);
    (index < entries.len()).then_some(index)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PhysicalRangeOrdering {
    Before,
    MayContain,
    After,
}

fn compare_index_entry_physical_range(
    entry: &TableIndexEntry,
    target_physical_key: &[u8],
) -> PhysicalRangeOrdering {
    let first_key = table_internal_physical_key_bytes(entry.first_key_bytes());
    let last_key = table_internal_physical_key_bytes(entry.last_key_bytes());
    if last_key < target_physical_key {
        PhysicalRangeOrdering::Before
    } else if first_key > target_physical_key {
        PhysicalRangeOrdering::After
    } else {
        PhysicalRangeOrdering::MayContain
    }
}

fn candidate_row_index_for_seek_key(rows: &[TableRow], key: &TableInternalKeyBytes) -> usize {
    match rows.binary_search_by(|row| row.key().cmp(key)) {
        Ok(index) | Err(index) => index,
    }
}

fn cache_key_for_entry(
    table: &TableCacheTableId,
    entry: &TableIndexEntry,
    block_index: usize,
) -> TableRuntimeResult<TableBlockCacheKey> {
    let ordinal = u32::try_from(block_index).map_err(|_| TableRuntimeError::InvalidRange {
        field: "data_block_index",
    })?;
    let address = TableBlockAddress::new(
        TableBlockCacheKind::Data,
        entry.block_offset(),
        entry.block_frame_len(),
        Some(ordinal),
    )?;
    Ok(TableBlockCacheKey::new(table.clone(), address))
}

/// W2.3 (B3): the Accelerator-kind twin of a data block's cache key — same
/// table/offset/length/ordinal, distinct kind, so the derived entry-offset
/// index coexists with the frame it accelerates.
fn accelerator_key_for_entry(
    table: &TableCacheTableId,
    entry: &TableIndexEntry,
    block_index: usize,
) -> TableRuntimeResult<TableBlockCacheKey> {
    let ordinal = u32::try_from(block_index).map_err(|_| TableRuntimeError::InvalidRange {
        field: "data_block_index",
    })?;
    let address = TableBlockAddress::new(
        TableBlockCacheKind::Accelerator,
        entry.block_offset(),
        entry.block_frame_len(),
        Some(ordinal),
    )?;
    Ok(TableBlockCacheKey::new(table.clone(), address))
}

/// LE-encode the derived entry offsets for cache residency.
fn encode_entry_offsets(offsets: &[u32]) -> Arc<[u8]> {
    let mut bytes = Vec::with_capacity(offsets.len() * 4);
    for offset in offsets {
        bytes.extend_from_slice(&offset.to_le_bytes());
    }
    Arc::from(bytes)
}

/// Decode cached accelerator bytes; shape errors yield `None` (the caller
/// rebuilds from the verified payload — the indexed seek re-validates the
/// offsets against the payload regardless).
fn decode_entry_offsets(bytes: &[u8]) -> Option<Vec<u32>> {
    if bytes.is_empty() || bytes.len() % 4 != 0 {
        return None;
    }
    Some(
        bytes
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect(),
    )
}

fn table_facts_from_metadata(
    identity: TableIdentity,
    metadata: &ImmutableTableMetadata,
) -> TableRuntimeResult<TableRuntimeFacts> {
    let header = metadata.header();
    TableRuntimeFacts::new(
        identity,
        header.row_count(),
        header.data_block_count(),
        TableKeyRange::new(
            metadata.properties().min_key_bytes().to_vec(),
            metadata.properties().max_key_bytes().to_vec(),
        )?,
        TableCommitRange::new(header.commit_min(), header.commit_max())?,
        metadata.byte_count(),
    )
}

fn read_exact_source(
    source: &impl TableByteSource,
    offset: u64,
    len: usize,
    short_reason: &'static str,
) -> TableRuntimeResult<Vec<u8>> {
    let bytes = source.read_at(offset, len)?;
    if bytes.len() != len {
        let reason = if bytes.len() < len {
            short_reason
        } else {
            "long table source range read"
        };
        return Err(TableRuntimeError::source_read(reason));
    }
    Ok(bytes)
}
