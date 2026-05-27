//! Immutable table reader.

use super::{
    BoundedTableCursor, TableCommitRange, TableIdentity, TableInternalKeyBytes, TableKeyBounds,
    TableKeyRange, TableReaderConfig, TableRow, TableRuntimeError, TableRuntimeFacts,
    TableRuntimeResult,
};
use crate::format::{
    decode_immutable_table, decode_immutable_table_data_block, decode_immutable_table_metadata,
    decode_table_footer_metadata, decode_table_header, ImmutableTableMetadata,
    MAX_TABLE_FOOTER_SIZE, MAX_TABLE_HEADER_SIZE,
};

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

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ImmutableTableReader {
    config: TableReaderConfig,
    facts: TableRuntimeFacts,
    rows: Vec<TableRow>,
}

impl ImmutableTableReader {
    pub(crate) fn open_bytes(
        identity: TableIdentity,
        bytes: Vec<u8>,
        config: TableReaderConfig,
    ) -> TableRuntimeResult<Self> {
        require_validate_on_open(config);
        let source = BytesTableSource::new(bytes);
        let (facts, rows) = decode_reader_rows(identity, source.bytes())?;
        Ok(Self {
            config,
            facts,
            rows,
        })
    }

    pub(crate) fn open_source<S: TableByteSource>(
        identity: TableIdentity,
        source: S,
        config: TableReaderConfig,
    ) -> TableRuntimeResult<Self> {
        require_validate_on_open(config);
        let metadata = read_table_metadata(&source)?;
        let facts = table_facts_from_metadata(identity, &metadata)?;
        let rows = read_rows_from_metadata(&source, &metadata)?;
        super::validate_strictly_sorted_unique_rows(&rows)?;
        Ok(Self {
            config,
            facts,
            rows,
        })
    }

    pub(crate) const fn config(&self) -> TableReaderConfig {
        self.config
    }

    pub(crate) const fn facts(&self) -> &TableRuntimeFacts {
        &self.facts
    }

    pub(crate) const fn byte_count(&self) -> u64 {
        self.facts.byte_count()
    }

    pub(crate) fn rows(&self) -> &[TableRow] {
        &self.rows
    }

    pub(crate) fn get_exact(&self, key: &TableInternalKeyBytes) -> Option<TableRow> {
        self.rows
            .binary_search_by(|row| row.key().cmp(key))
            .ok()
            .map(|index| self.rows[index].clone())
    }

    pub(crate) fn cursor(&self) -> ImmutableTableCursor<'_> {
        ImmutableTableCursor::new(&self.rows)
    }

    pub(crate) fn bounded_cursor(&self, bounds: TableKeyBounds) -> BoundedTableCursor<'_> {
        BoundedTableCursor::new(Box::new(self.cursor()), bounds)
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
        Ok(())
    }

    fn seek(&mut self, target: &TableInternalKeyBytes) -> TableRuntimeResult<()> {
        self.position = self.seek_index(target);
        Ok(())
    }

    fn advance(&mut self) -> TableRuntimeResult<()> {
        self.position = self.position.and_then(|position| {
            let next = position.saturating_add(1);
            (next < self.rows.len()).then_some(next)
        });
        Ok(())
    }

    fn current(&self) -> Option<&TableRow> {
        self.position.and_then(|position| self.rows.get(position))
    }
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
    let footer_bytes = read_exact_source(
        source,
        footer_offset,
        MAX_TABLE_FOOTER_SIZE,
        "short table footer read",
    )?;

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
    let properties_bytes = read_exact_source(
        source,
        footer.properties_block_offset(),
        properties_len,
        "short table properties read",
    )?;
    decode_immutable_table_metadata(
        byte_count,
        &header_bytes,
        &footer_bytes,
        &index_bytes,
        &properties_bytes,
    )
    .map_err(|source| TableRuntimeError::DecodeFormat { source })
}

fn read_rows_from_metadata(
    source: &impl TableByteSource,
    metadata: &ImmutableTableMetadata,
) -> TableRuntimeResult<Vec<TableRow>> {
    let mut rows = Vec::new();
    for entry in metadata.index().entries() {
        let frame_len = usize::try_from(entry.block_frame_len()).map_err(|_| {
            TableRuntimeError::InvalidRange {
                field: "block_frame_len",
            }
        })?;
        let frame_bytes = read_exact_source(
            source,
            entry.block_offset(),
            frame_len,
            "short table data block read",
        )?;
        let block = decode_immutable_table_data_block(entry, &frame_bytes)
            .map_err(|source| TableRuntimeError::DecodeFormat { source })?;
        rows.extend(block.rows().cloned().map(TableRow::new));
    }
    Ok(rows)
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
        return Err(TableRuntimeError::source_read(short_reason));
    }
    Ok(bytes)
}
