//! Immutable table reader.

use std::fmt;
use std::sync::{Arc, OnceLock};

use super::{
    BoundedTableCursor, TableBlockAddress, TableBlockCache, TableBlockCacheKey,
    TableBlockCacheKind, TableCacheTableId, TableCommitRange, TableIdentity, TableInternalKeyBytes,
    TableKeyBounds, TableKeyRange, TablePhysicalKeyBytes, TableReaderConfig, TableRow,
    TableRuntimeError, TableRuntimeFacts, TableRuntimeResult,
};
use crate::format::{
    decode_immutable_table, decode_immutable_table_data_block, decode_immutable_table_metadata,
    decode_table_footer_metadata, decode_table_header, ImmutableTableMetadata, TableIndexEntry,
    MAX_TABLE_FOOTER_SIZE, MAX_TABLE_HEADER_SIZE,
};
use crate::observability::perf_trace;
use crate::row::{InternalKey, PhysicalKey};
use strata_core_next::{CommitVersion, Timestamp};

use super::facts::table_facts_from_decoded;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BytesTableSource {
    bytes: Vec<u8>,
}

impl BytesTableSource {
    pub(crate) fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self {
            bytes: bytes.into(),
        }
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
}

pub(crate) trait TableByteSource {
    fn byte_count(&self) -> u64;
    fn read_at(&self, offset: u64, len: usize) -> TableRuntimeResult<Vec<u8>>;
}

impl<T: TableByteSource + ?Sized> TableByteSource for &T {
    fn byte_count(&self) -> u64 {
        (**self).byte_count()
    }

    fn read_at(&self, offset: u64, len: usize) -> TableRuntimeResult<Vec<u8>> {
        (**self).read_at(offset, len)
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ImmutableTableReader<'a> {
    config: TableReaderConfig,
    facts: TableRuntimeFacts,
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

impl TableReaderRows<'_> {
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

    fn into_materialized(self) -> TableRuntimeResult<Vec<TableRow>> {
        match self.rows.into_inner() {
            Some(rows) => rows,
            None => read_and_validate_rows(&self.state),
        }
    }

    fn with_block_cache(&mut self, table: TableCacheTableId, cache: Arc<TableBlockCache>) {
        self.state.cache = Some(LazyTableBlockCache { table, cache });
    }
}

#[derive(Clone, Debug)]
struct LazyTableState<'a> {
    source: SharedTableSource<'a>,
    metadata: ImmutableTableMetadata,
    cache: Option<LazyTableBlockCache>,
}

impl LazyTableState<'_> {
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
        let (facts, rows) = decode_reader_rows(identity, &bytes)?;
        let runtime_facts = TableReaderRuntimeFacts::eager(
            TableReaderOpenMode::EagerBytes,
            facts.data_block_count(),
            facts.row_count(),
        );
        Ok(Self {
            config,
            facts,
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
        let metadata = read_table_metadata(&source)?;
        let facts = table_facts_from_metadata(identity, &metadata)?;
        let runtime_facts = TableReaderRuntimeFacts::lazy(TableReaderOpenMode::LazySource);
        Ok(Self {
            config,
            facts,
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
    ) -> (Option<&TableRow>, usize) {
        seek_physical_key_in_slice(self.rows(), key, max_commit_version, max_commit_timestamp)
    }

    pub(crate) fn cursor(&self) -> ImmutableTableCursor<'_> {
        ImmutableTableCursor::new(self.rows())
    }

    pub(crate) fn bounded_cursor(&self, bounds: TableKeyBounds) -> BoundedTableCursor<'_> {
        BoundedTableCursor::new(Box::new(self.cursor()), bounds)
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
}

#[derive(Clone, Debug)]
pub(crate) struct ImmutableTableCursor<'a> {
    rows: &'a [TableRow],
    position: Option<usize>,
}

impl<'a> ImmutableTableCursor<'a> {
    fn new(rows: &'a [TableRow]) -> Self {
        Self {
            rows,
            position: None,
        }
    }

    fn seek_index(&self, target: &TableInternalKeyBytes) -> Option<usize> {
        match self.rows.binary_search_by(|row| row.key().cmp(target)) {
            Ok(index) | Err(index) if index < self.rows.len() => Some(index),
            Ok(_) | Err(_) => None,
        }
    }
}

impl super::TableCursor for ImmutableTableCursor<'_> {
    fn seek_to_first(&mut self) -> TableRuntimeResult<()> {
        self.position = if self.rows.is_empty() { None } else { Some(0) };
        record_cursor_position(self.position);
        Ok(())
    }

    fn seek(&mut self, target: &TableInternalKeyBytes) -> TableRuntimeResult<()> {
        self.position = self.seek_index(target);
        record_cursor_position(self.position);
        Ok(())
    }

    fn advance(&mut self) -> TableRuntimeResult<()> {
        self.position = self.position.and_then(|position| {
            let next = position.saturating_add(1);
            (next < self.rows.len()).then_some(next)
        });
        record_cursor_position(self.position);
        Ok(())
    }

    fn current(&self) -> Option<&TableRow> {
        self.position.and_then(|position| self.rows.get(position))
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
) -> TableRuntimeResult<(TableRuntimeFacts, Vec<TableRow>)> {
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
    Ok((facts, rows))
}

fn read_table_metadata(
    source: &impl TableByteSource,
) -> TableRuntimeResult<ImmutableTableMetadata> {
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
    decode_immutable_table_metadata(
        byte_count,
        &header_bytes,
        &footer_bytes,
        &index_bytes,
        &properties_bytes,
    )
    .map_err(|source| TableRuntimeError::DecodeFormat { source })
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
