//! Immutable table reader.

use std::fmt;
use std::sync::{Arc, OnceLock};

use super::key::table_internal_physical_key_bytes;
use super::{
    BoundedTableCursor, TableBlockAddress, TableBlockCache, TableBlockCacheKey,
    TableBlockCacheKind, TableBloomFilter, TableBloomProbe, TableCacheTableId, TableCommitRange,
    TableIdentity, TableInternalKeyBytes, TableKeyBounds, TableKeyRange, TablePhysicalKeyBytes,
    TableReaderConfig, TableRow, TableRuntimeError, TableRuntimeFacts, TableRuntimeResult,
};
use crate::format::{
    decode_immutable_table, decode_immutable_table_data_block, decode_immutable_table_metadata,
    decode_table_footer_metadata, decode_table_header, ImmutableTableMetadata, TableIndexEntry,
    MAX_TABLE_FOOTER_SIZE, MAX_TABLE_HEADER_SIZE,
};
use crate::observability::perf_trace;
use crate::row::{InternalKey, PhysicalKey};
use sha2::{Digest, Sha256};
use strata_core_next::{CommitVersion, Timestamp};

use super::facts::table_facts_from_decoded;

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
}

#[derive(Clone, Debug, Eq, PartialEq)]
enum TableReaderFilterState {
    Unavailable,
    Bloom(Box<TableReaderBloomFilterState>),
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
        let physical_keys = rows
            .iter()
            .map(|row| TablePhysicalKeyBytes::from_physical_key(row.physical_key()))
            .collect::<Vec<_>>();
        let filter = TableBloomFilter::build(
            physical_keys.iter().map(TablePhysicalKeyBytes::as_slice),
            bits_per_key,
        )?;
        Ok(Self {
            state: TableReaderFilterState::Bloom(Box::new(TableReaderBloomFilterState {
                facts,
                fingerprint,
                filter,
            })),
        })
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
        if self.config != other.config || self.facts != other.facts {
            return false;
        }
        match (self.try_rows(), other.try_rows()) {
            (Ok(left), Ok(right)) => left == right,
            (Err(left), Err(right)) => left == right,
            _ => false,
        }
    }
}

impl Eq for ImmutableTableReader<'_> {}

#[derive(Clone, Debug)]
enum TableReaderRows<'a> {
    Eager(Vec<TableRow>),
    Lazy(Box<LazyTableRows<'a>>),
}

impl<'a> TableReaderRows<'a> {
    fn try_rows(&self) -> TableRuntimeResult<&[TableRow]> {
        match self {
            Self::Eager(rows) => Ok(rows),
            Self::Lazy(rows) => rows.try_rows(),
        }
    }

    fn into_materialized(self) -> TableRuntimeResult<(Vec<TableRow>, bool)> {
        match self {
            Self::Eager(rows) => Ok((rows, false)),
            Self::Lazy(rows) => rows.into_materialized().map(|rows| (rows, true)),
        }
    }

    fn try_get_exact(&self, key: &TableInternalKeyBytes) -> TableRuntimeResult<Option<TableRow>> {
        match self {
            Self::Eager(rows) => Ok(find_exact_in_rows(rows, key)),
            Self::Lazy(rows) => rows.try_get_exact(key),
        }
    }

    fn try_seek_physical_key(
        &self,
        key: &PhysicalKey,
        max_commit_version: Option<CommitVersion>,
        max_commit_timestamp: Option<Timestamp>,
    ) -> TableRuntimeResult<(Option<TableRow>, usize)> {
        match self {
            Self::Eager(rows) => {
                let (row, visited) =
                    seek_physical_key_in_slice(rows, key, max_commit_version, max_commit_timestamp);
                Ok((row.cloned(), visited))
            }
            Self::Lazy(rows) => {
                rows.try_seek_physical_key(key, max_commit_version, max_commit_timestamp)
            }
        }
    }

    fn cursor<'reader>(
        &'reader self,
        bounds_hint: Option<TableKeyBounds>,
    ) -> ImmutableTableCursor<'reader, 'a> {
        match self {
            Self::Eager(rows) => ImmutableTableCursor::eager(rows),
            Self::Lazy(rows) => rows.cursor(bounds_hint),
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
            Self::Eager(_) => false,
            Self::Lazy(rows) => rows.with_filter(filter),
        }
    }
}

#[derive(Clone, Debug)]
struct LazyTableRows<'a> {
    state: LazyTableState<'a>,
    rows: OnceLock<TableRuntimeResult<Vec<TableRow>>>,
}

impl<'a> LazyTableRows<'a> {
    fn new(source: SharedTableSource<'a>, metadata: ImmutableTableMetadata) -> Self {
        Self {
            state: LazyTableState {
                source,
                metadata,
                cache: None,
                filter: None,
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
        let rows = self.state.read_data_block_rows(block_index)?;
        Ok(find_exact_in_rows(&rows, key))
    }

    fn try_seek_physical_key(
        &self,
        key: &PhysicalKey,
        max_commit_version: Option<CommitVersion>,
        max_commit_timestamp: Option<Timestamp>,
    ) -> TableRuntimeResult<(Option<TableRow>, usize)> {
        if let Some(rows) = self.rows.get() {
            return match rows {
                Ok(rows) => {
                    let (row, visited) = seek_physical_key_in_slice(
                        rows,
                        key,
                        max_commit_version,
                        max_commit_timestamp,
                    );
                    Ok((row.cloned(), visited))
                }
                Err(error) => Err(error.clone()),
            };
        }
        self.state
            .seek_physical_key(key, max_commit_version, max_commit_timestamp)
    }

    fn into_materialized(self) -> TableRuntimeResult<Vec<TableRow>> {
        match self.rows.into_inner() {
            Some(rows) => rows,
            None => read_and_validate_rows(&self.state),
        }
    }

    fn cursor<'reader>(
        &'reader self,
        bounds_hint: Option<TableKeyBounds>,
    ) -> ImmutableTableCursor<'reader, 'a> {
        match self.rows.get() {
            Some(Ok(rows)) => ImmutableTableCursor::eager(rows),
            Some(Err(error)) => ImmutableTableCursor::failed(error.clone()),
            None => ImmutableTableCursor::lazy(self.state.clone(), bounds_hint),
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
    metadata: ImmutableTableMetadata,
    cache: Option<LazyTableBlockCache>,
    filter: Option<TableReaderFilter>,
}

impl LazyTableState<'_> {
    fn seek_physical_key(
        &self,
        key: &PhysicalKey,
        max_commit_version: Option<CommitVersion>,
        max_commit_timestamp: Option<Timestamp>,
    ) -> TableRuntimeResult<(Option<TableRow>, usize)> {
        perf_trace::record_table_seek();
        let prefix = TablePhysicalKeyBytes::from_physical_key(key);
        let target_physical_key = prefix.as_slice();
        if !self.contains_physical_key(target_physical_key) {
            perf_trace::record_table_point_rows_visited(0);
            return Ok((None, 0));
        }
        if self.probe_physical_filter(&prefix) == TableBloomProbe::DefinitelyAbsent {
            perf_trace::record_table_point_rows_visited(0);
            return Ok((None, 0));
        }

        let seek_version = max_commit_version.unwrap_or(CommitVersion::MAX);
        let seek_key =
            TableInternalKeyBytes::from_internal_key(&InternalKey::new(key.clone(), seek_version));
        let entries = self.metadata.index().entries();
        let Some(mut block_index) = first_candidate_block_for_key(entries, &seek_key) else {
            perf_trace::record_table_point_rows_visited(0);
            return Ok((None, 0));
        };
        let first_candidate_block_index = block_index;

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

            let rows = self.read_data_block_rows(block_index)?;
            let start = if block_index == first_candidate_block_index {
                candidate_row_index_for_seek_key(&rows, &seek_key)
            } else {
                0
            };

            let mut continue_to_next_block = false;
            for row in &rows[start..] {
                visited = visited.saturating_add(1);
                match row.key().physical_key_bytes().cmp(target_physical_key) {
                    std::cmp::Ordering::Less => {}
                    std::cmp::Ordering::Greater => {
                        perf_trace::record_table_point_rows_visited(visited);
                        return Ok((None, visited));
                    }
                    std::cmp::Ordering::Equal => {
                        continue_to_next_block = true;
                        if row_matches_point_bound(row, max_commit_version, max_commit_timestamp) {
                            perf_trace::record_table_point_rows_visited(visited);
                            return Ok((Some(row.clone()), visited));
                        }
                    }
                }
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

    fn contains_physical_key(&self, physical_key: &[u8]) -> bool {
        let properties = self.metadata.properties();
        let min_key = table_internal_physical_key_bytes(properties.min_key_bytes());
        let max_key = table_internal_physical_key_bytes(properties.max_key_bytes());
        min_key <= physical_key && physical_key <= max_key
    }

    fn probe_physical_filter(&self, key: &TablePhysicalKeyBytes) -> TableBloomProbe {
        self.filter
            .as_ref()
            .map_or(TableBloomProbe::Unavailable, |filter| {
                filter.probe_physical_key(key)
            })
    }

    fn read_data_block_rows(&self, block_index: usize) -> TableRuntimeResult<Vec<TableRow>> {
        let entry = self.metadata.index().entries().get(block_index).ok_or(
            TableRuntimeError::InvalidRange {
                field: "data_block_index",
            },
        )?;
        let frame = self.read_data_block_frame(entry, block_index)?;
        let block = decode_immutable_table_data_block(entry, frame.bytes.as_ref())
            .map_err(|source| TableRuntimeError::DecodeFormat { source })?;
        perf_trace::record_table_data_blocks_decoded(1, block.row_count());
        if let Some((cache, key)) = frame.cache_insert {
            cache.insert(key, Arc::clone(&frame.bytes))?;
        }
        Ok(block.rows().cloned().map(TableRow::new).collect())
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
        Ok(DataBlockFrame {
            bytes,
            cache_insert: self
                .cache
                .as_ref()
                .zip(cache_key)
                .map(|(cache, key)| (Arc::clone(&cache.cache), key)),
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
        let (facts, fingerprint, rows) = decode_reader_rows(identity, &bytes)?;
        let runtime_facts = TableReaderRuntimeFacts::eager(
            TableReaderOpenMode::EagerBytes,
            facts.data_block_count(),
            facts.row_count(),
        );
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
        let runtime_facts = TableReaderRuntimeFacts::lazy(TableReaderOpenMode::LazySource);
        Ok(Self {
            config,
            facts,
            fingerprint,
            runtime_facts,
            rows: TableReaderRows::Lazy(Box::new(LazyTableRows::new(source, metadata))),
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

    pub(crate) const fn byte_count(&self) -> u64 {
        self.facts.byte_count()
    }

    pub(crate) fn rows(&self) -> &[TableRow] {
        self.try_rows()
            .expect("lazy table row materialization failed")
    }

    pub(crate) fn try_rows(&self) -> TableRuntimeResult<&[TableRow]> {
        self.rows.try_rows()
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
        self.try_seek_physical_key(key, max_commit_version, max_commit_timestamp)
            .expect("lazy table physical key seek failed")
    }

    pub(crate) fn try_seek_physical_key(
        &self,
        key: &PhysicalKey,
        max_commit_version: Option<CommitVersion>,
        max_commit_timestamp: Option<Timestamp>,
    ) -> TableRuntimeResult<(Option<TableRow>, usize)> {
        self.rows
            .try_seek_physical_key(key, max_commit_version, max_commit_timestamp)
    }

    pub(crate) fn cursor(&self) -> ImmutableTableCursor<'_, 'a> {
        self.rows.cursor(None)
    }

    pub(crate) fn bounded_cursor(&self, bounds: TableKeyBounds) -> BoundedTableCursor<'_> {
        BoundedTableCursor::new(Box::new(self.rows.cursor(Some(bounds.clone()))), bounds)
    }

    pub(crate) fn into_materialized(self) -> TableRuntimeResult<ImmutableTableReader<'static>> {
        let config = self.config;
        let facts = self.facts;
        let runtime_facts = self.runtime_facts;
        let (rows, was_lazy) = self.rows.into_materialized()?;
        let runtime_facts = if was_lazy {
            TableReaderRuntimeFacts::eager(
                TableReaderOpenMode::EagerSource,
                facts.data_block_count(),
                facts.row_count(),
            )
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

            let rows = self.state.read_data_block_rows(block_index)?;
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

fn seek_physical_key_in_slice<'a>(
    rows: &'a [TableRow],
    key: &PhysicalKey,
    max_commit_version: Option<CommitVersion>,
    max_commit_timestamp: Option<Timestamp>,
) -> (Option<&'a TableRow>, usize) {
    perf_trace::record_table_seek();
    let prefix = TablePhysicalKeyBytes::from_physical_key(key);
    let seek_version = max_commit_version.unwrap_or(CommitVersion::MAX);
    let seek_key =
        TableInternalKeyBytes::from_internal_key(&InternalKey::new(key.clone(), seek_version));
    let start = match rows.binary_search_by(|row| row.key().cmp(&seek_key)) {
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
        if row_matches_point_bound(row, max_commit_version, max_commit_timestamp) {
            perf_trace::record_table_point_rows_visited(visited);
            return (Some(row), visited);
        }
    }
    perf_trace::record_table_point_rows_visited(visited);
    (None, visited)
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
) -> TableRuntimeResult<(TableRuntimeFacts, TableContentFingerprint, Vec<TableRow>)> {
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
    Ok((facts, fingerprint, rows))
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
        rows.extend(state.read_data_block_rows(block_index)?);
    }
    Ok(rows)
}

fn read_and_validate_rows(state: &LazyTableState<'_>) -> TableRuntimeResult<Vec<TableRow>> {
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
