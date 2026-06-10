use super::data::{decode_table_data_block, encode_table_data_block, TableDataBlock};
use super::index::{decode_table_index_block_for_data_blocks, encode_table_index_block};
use super::index::{TableIndexBlock, TableIndexEntry};
use super::properties::{
    decode_table_properties, encode_table_properties, validate_table_properties_against_header,
    TableProperties,
};
use super::{
    decode_table_block_frame_as, decode_table_footer, decode_table_footer_metadata,
    decode_table_header, encode_table_block_frame, encode_table_footer, encode_table_header,
    TableBlockFrame, TableBlockKind, TableCompression, TableFooter, TableHeader,
    MAX_TABLE_DATA_BLOCKS, MAX_TABLE_ROWS, TABLE_FOOTER_SIZE, TABLE_HEADER_SIZE,
};
use crate::format::FormatError;
use crate::row::{InternalKey, StorageRow};
use strata_core_next::CommitVersion;

const TABLE_ARTIFACT_FORMAT: &str = "immutable_table";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ImmutableTable {
    header: TableHeader,
    data_blocks: Vec<TableDataBlock>,
    index: TableIndexBlock,
    properties: TableProperties,
    rows: Vec<StorageRow>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ImmutableTableMetadata {
    header: TableHeader,
    index: TableIndexBlock,
    properties: TableProperties,
    byte_count: u64,
}

impl ImmutableTableMetadata {
    pub(crate) const fn header(&self) -> TableHeader {
        self.header
    }

    pub(crate) const fn index(&self) -> &TableIndexBlock {
        &self.index
    }

    pub(crate) const fn properties(&self) -> &TableProperties {
        &self.properties
    }

    pub(crate) const fn byte_count(&self) -> u64 {
        self.byte_count
    }
}

impl ImmutableTable {
    pub(crate) const fn header(&self) -> TableHeader {
        self.header
    }

    pub(crate) fn data_blocks(&self) -> &[TableDataBlock] {
        &self.data_blocks
    }

    pub(crate) const fn index(&self) -> &TableIndexBlock {
        &self.index
    }

    pub(crate) const fn properties(&self) -> &TableProperties {
        &self.properties
    }

    pub(crate) fn rows(&self) -> &[StorageRow] {
        &self.rows
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DataFrameLocation {
    offset: u64,
    frame_len: u32,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ImmutableTableStreamingEncoder {
    target_data_block_size: u32,
    rows_per_block: usize,
    compression: TableCompression,
    table_bytes: Vec<u8>,
    current_block_rows: Vec<StorageRow>,
    index_entries: Vec<TableIndexEntry>,
    row_count: u64,
    commit_min: Option<CommitVersion>,
    commit_max: Option<CommitVersion>,
    first_key: Option<InternalKey>,
    last_key: Option<InternalKey>,
    peak_buffered_rows: usize,
}

impl ImmutableTableStreamingEncoder {
    pub(crate) fn new(
        target_data_block_size: u32,
        rows_per_block: usize,
        compression: TableCompression,
    ) -> Result<Self, FormatError> {
        if target_data_block_size == 0 {
            return Err(FormatError::InvalidLength {
                field: "target_data_block_size",
            });
        }
        if rows_per_block == 0 {
            return Err(FormatError::InvalidLength {
                field: "rows_per_block",
            });
        }
        Ok(Self {
            target_data_block_size,
            rows_per_block,
            compression,
            table_bytes: vec![0; TABLE_HEADER_SIZE],
            current_block_rows: Vec::new(),
            index_entries: Vec::new(),
            row_count: 0,
            commit_min: None,
            commit_max: None,
            first_key: None,
            last_key: None,
            peak_buffered_rows: 0,
        })
    }

    pub(crate) fn append(&mut self, row: &StorageRow) -> Result<(), FormatError> {
        let next_row_count = self
            .row_count
            .checked_add(1)
            .ok_or(FormatError::InvalidLength { field: "row_count" })?;
        if next_row_count > MAX_TABLE_ROWS {
            return Err(FormatError::InvalidLength { field: "row_count" });
        }

        let version = row.commit_version();
        self.commit_min = Some(match self.commit_min {
            Some(current) if current.as_u64() <= version.as_u64() => current,
            _ => version,
        });
        self.commit_max = Some(match self.commit_max {
            Some(current) if current.as_u64() >= version.as_u64() => current,
            _ => version,
        });

        let key = InternalKey::new(row.physical_key().clone(), row.commit_version());
        if self.first_key.is_none() {
            self.first_key = Some(key.clone());
        }
        self.last_key = Some(key);
        self.row_count = next_row_count;
        self.current_block_rows.push(row.clone());
        self.peak_buffered_rows = self.peak_buffered_rows.max(self.current_block_rows.len());

        if self.current_block_rows.len() == self.rows_per_block {
            self.flush_current_block()?;
        }
        Ok(())
    }

    pub(crate) const fn buffered_rows(&self) -> usize {
        self.current_block_rows.len()
    }

    pub(crate) const fn peak_buffered_rows(&self) -> usize {
        self.peak_buffered_rows
    }

    pub(crate) fn finish(mut self) -> Result<Vec<u8>, FormatError> {
        self.flush_current_block()?;
        let data_block_count = self.index_entries.len();
        validate_build_counts(
            usize::try_from(self.row_count)
                .map_err(|_| FormatError::InvalidLength { field: "row_count" })?,
            data_block_count,
            self.target_data_block_size,
        )?;

        let commit_min = self
            .commit_min
            .ok_or(FormatError::InvalidLength { field: "row_count" })?;
        let commit_max = self
            .commit_max
            .ok_or(FormatError::InvalidLength { field: "row_count" })?;
        let first_key = self
            .first_key
            .as_ref()
            .ok_or(FormatError::InvalidLength { field: "row_count" })?;
        let last_key = self
            .last_key
            .as_ref()
            .ok_or(FormatError::InvalidLength { field: "row_count" })?;

        let data_block_count_u32 =
            u32::try_from(data_block_count).map_err(|_| FormatError::InvalidLength {
                field: "data_block_count",
            })?;
        let properties = TableProperties::new(
            self.row_count,
            data_block_count_u32,
            commit_min,
            commit_max,
            first_key,
            last_key,
        )?;
        let header = TableHeader::new(
            self.target_data_block_size,
            properties.data_block_count(),
            properties.row_count(),
            properties.commit_min(),
            properties.commit_max(),
        )?;
        let header_bytes = encode_table_header(&header);
        self.table_bytes[..TABLE_HEADER_SIZE].copy_from_slice(&header_bytes);

        let index = TableIndexBlock::new(self.index_entries)?;
        let index_frame = TableBlockFrame::new(
            TableBlockKind::Index,
            TableCompression::Uncompressed,
            encode_table_index_block(&index)?,
        )?;
        let index_block_offset =
            u64::try_from(self.table_bytes.len()).map_err(|_| FormatError::InvalidLength {
                field: "index_block_offset",
            })?;
        let index_frame_bytes = encode_table_block_frame(&index_frame)?;
        let index_block_frame_len =
            u32::try_from(index_frame_bytes.len()).map_err(|_| FormatError::InvalidLength {
                field: "index_block_frame_len",
            })?;
        self.table_bytes.extend_from_slice(&index_frame_bytes);

        let properties_frame = TableBlockFrame::new(
            TableBlockKind::Properties,
            TableCompression::Uncompressed,
            encode_table_properties(&properties)?,
        )?;
        let properties_block_offset =
            u64::try_from(self.table_bytes.len()).map_err(|_| FormatError::InvalidLength {
                field: "properties_block_offset",
            })?;
        let properties_frame_bytes = encode_table_block_frame(&properties_frame)?;
        let properties_block_frame_len =
            u32::try_from(properties_frame_bytes.len()).map_err(|_| {
                FormatError::InvalidLength {
                    field: "properties_block_frame_len",
                }
            })?;
        self.table_bytes.extend_from_slice(&properties_frame_bytes);

        let footer = TableFooter::new(
            index_block_offset,
            index_block_frame_len,
            properties_block_offset,
            properties_block_frame_len,
        )?;
        self.table_bytes
            .extend_from_slice(&encode_table_footer(&footer, &self.table_bytes)?);
        Ok(self.table_bytes)
    }

    fn flush_current_block(&mut self) -> Result<(), FormatError> {
        if self.current_block_rows.is_empty() {
            return Ok(());
        }
        let next_block_count =
            self.index_entries
                .len()
                .checked_add(1)
                .ok_or(FormatError::InvalidLength {
                    field: "data_block_count",
                })?;
        if next_block_count > MAX_TABLE_DATA_BLOCKS as usize {
            return Err(FormatError::InvalidLength {
                field: "data_block_count",
            });
        }

        let rows = std::mem::take(&mut self.current_block_rows);
        let data_block = TableDataBlock::from_rows(&rows)?;
        let data_payload = encode_table_data_block(&data_block)?;
        let frame = TableBlockFrame::new(TableBlockKind::Data, self.compression, data_payload)?;
        let frame_bytes = encode_table_block_frame(&frame)?;
        let block_offset =
            u64::try_from(self.table_bytes.len()).map_err(|_| FormatError::InvalidLength {
                field: "block_offset",
            })?;
        let block_frame_len =
            u32::try_from(frame_bytes.len()).map_err(|_| FormatError::InvalidLength {
                field: "block_frame_len",
            })?;
        let row_count = u32::try_from(data_block.row_count())
            .map_err(|_| FormatError::InvalidLength { field: "row_count" })?;
        self.index_entries.push(TableIndexEntry::new(
            data_block.entries()[0].internal_key(),
            data_block.entries()[data_block.entries().len() - 1].internal_key(),
            block_offset,
            block_frame_len,
            row_count,
        )?);
        self.table_bytes.extend_from_slice(&frame_bytes);
        Ok(())
    }
}

pub(crate) fn encode_immutable_table(
    rows: &[StorageRow],
    target_data_block_size: u32,
    rows_per_block: usize,
    compression: TableCompression,
) -> Result<Vec<u8>, FormatError> {
    if rows.is_empty() {
        return Err(FormatError::InvalidLength { field: "row_count" });
    }
    if rows_per_block == 0 {
        return Err(FormatError::InvalidLength {
            field: "rows_per_block",
        });
    }
    let block_count = rows.len().div_ceil(rows_per_block);
    validate_build_counts(rows.len(), block_count, target_data_block_size)?;
    encode_immutable_table_with_block_compressions(
        rows,
        target_data_block_size,
        rows_per_block,
        &vec![compression; block_count],
    )
}

pub(crate) fn encode_immutable_table_with_block_compressions(
    rows: &[StorageRow],
    target_data_block_size: u32,
    rows_per_block: usize,
    data_compressions: &[TableCompression],
) -> Result<Vec<u8>, FormatError> {
    if rows.is_empty() {
        return Err(FormatError::InvalidLength { field: "row_count" });
    }
    if rows_per_block == 0 {
        return Err(FormatError::InvalidLength {
            field: "rows_per_block",
        });
    }
    let expected_block_count = rows.len().div_ceil(rows_per_block);
    validate_build_counts(rows.len(), expected_block_count, target_data_block_size)?;
    if expected_block_count != data_compressions.len() {
        return Err(FormatError::InvalidValue {
            field: "data_compressions",
        });
    }

    let data_blocks = rows
        .chunks(rows_per_block)
        .map(TableDataBlock::from_rows)
        .collect::<Result<Vec<_>, _>>()?;

    let properties = TableProperties::from_data_blocks(&data_blocks)?;
    let header = TableHeader::new(
        target_data_block_size,
        properties.data_block_count(),
        properties.row_count(),
        properties.commit_min(),
        properties.commit_max(),
    )?;

    let mut table_bytes = encode_table_header(&header);
    let mut index_entries = Vec::new();
    for (data_block, compression) in data_blocks.iter().zip(data_compressions.iter().copied()) {
        let data_payload = encode_table_data_block(data_block)?;
        let frame = TableBlockFrame::new(TableBlockKind::Data, compression, data_payload)?;
        let frame_bytes = encode_table_block_frame(&frame)?;
        let block_offset =
            u64::try_from(table_bytes.len()).map_err(|_| FormatError::InvalidLength {
                field: "block_offset",
            })?;
        let block_frame_len =
            u32::try_from(frame_bytes.len()).map_err(|_| FormatError::InvalidLength {
                field: "block_frame_len",
            })?;
        let row_count = u32::try_from(data_block.row_count())
            .map_err(|_| FormatError::InvalidLength { field: "row_count" })?;
        index_entries.push(TableIndexEntry::new(
            data_block.entries()[0].internal_key(),
            data_block.entries()[data_block.entries().len() - 1].internal_key(),
            block_offset,
            block_frame_len,
            row_count,
        )?);
        table_bytes.extend_from_slice(&frame_bytes);
    }

    let index = TableIndexBlock::new(index_entries)?;
    let index_frame = TableBlockFrame::new(
        TableBlockKind::Index,
        TableCompression::Uncompressed,
        encode_table_index_block(&index)?,
    )?;
    let index_block_offset =
        u64::try_from(table_bytes.len()).map_err(|_| FormatError::InvalidLength {
            field: "index_block_offset",
        })?;
    let index_frame_bytes = encode_table_block_frame(&index_frame)?;
    let index_block_frame_len =
        u32::try_from(index_frame_bytes.len()).map_err(|_| FormatError::InvalidLength {
            field: "index_block_frame_len",
        })?;
    table_bytes.extend_from_slice(&index_frame_bytes);

    let properties_frame = TableBlockFrame::new(
        TableBlockKind::Properties,
        TableCompression::Uncompressed,
        encode_table_properties(&properties)?,
    )?;
    let properties_block_offset =
        u64::try_from(table_bytes.len()).map_err(|_| FormatError::InvalidLength {
            field: "properties_block_offset",
        })?;
    let properties_frame_bytes = encode_table_block_frame(&properties_frame)?;
    let properties_block_frame_len =
        u32::try_from(properties_frame_bytes.len()).map_err(|_| FormatError::InvalidLength {
            field: "properties_block_frame_len",
        })?;
    table_bytes.extend_from_slice(&properties_frame_bytes);

    let footer = TableFooter::new(
        index_block_offset,
        index_block_frame_len,
        properties_block_offset,
        properties_block_frame_len,
    )?;
    table_bytes.extend_from_slice(&encode_table_footer(&footer, &table_bytes)?);

    Ok(table_bytes)
}

pub(crate) fn decode_immutable_table(bytes: &[u8]) -> Result<ImmutableTable, FormatError> {
    let min_len =
        TABLE_HEADER_SIZE
            .checked_add(TABLE_FOOTER_SIZE)
            .ok_or(FormatError::InvalidLength {
                field: TABLE_ARTIFACT_FORMAT,
            })?;
    if bytes.len() < min_len {
        return Err(FormatError::InsufficientBytes {
            format: TABLE_ARTIFACT_FORMAT,
            needed: min_len,
            actual: bytes.len(),
        });
    }

    let (header, header_len) = decode_table_header(bytes)?;
    if header_len != TABLE_HEADER_SIZE {
        return Err(FormatError::InvalidLength {
            field: "table_header_size",
        });
    }
    let footer = decode_table_footer(bytes)?;
    let footer_start = bytes.len() - TABLE_FOOTER_SIZE;
    let (index_start, index_end) = table_range(
        footer.index_block_offset(),
        footer.index_block_frame_len(),
        "index_block",
    )?;
    let (properties_start, properties_end) = table_range(
        footer.properties_block_offset(),
        footer.properties_block_frame_len(),
        "properties_block",
    )?;
    if properties_start != index_end || properties_end != footer_start {
        return Err(FormatError::InvalidLength {
            field: "table_footer_layout",
        });
    }

    let (data_blocks, data_locations) = decode_data_region(bytes, index_start)?;
    let (index_frame, _) =
        decode_exact_frame(&bytes[index_start..index_end], TableBlockKind::Index)?;
    let index = decode_table_index_block_for_data_blocks(
        index_frame.decoded_payload(),
        header.data_block_count(),
    )?;
    let (properties_frame, _) = decode_exact_frame(
        &bytes[properties_start..properties_end],
        TableBlockKind::Properties,
    )?;
    let properties = decode_table_properties(properties_frame.decoded_payload())?;

    validate_table_against_decoded_facts(
        &header,
        &data_blocks,
        &data_locations,
        &index,
        &properties,
        index_start,
    )?;

    let rows = data_blocks
        .iter()
        .flat_map(|block| block.rows().cloned())
        .collect::<Vec<_>>();

    Ok(ImmutableTable {
        header,
        data_blocks,
        index,
        properties,
        rows,
    })
}

pub(crate) fn decode_immutable_table_metadata(
    byte_count: u64,
    header_bytes: &[u8],
    footer_bytes: &[u8],
    index_frame_bytes: &[u8],
    properties_frame_bytes: &[u8],
) -> Result<ImmutableTableMetadata, FormatError> {
    let byte_count_usize = usize::try_from(byte_count).map_err(|_| FormatError::InvalidLength {
        field: TABLE_ARTIFACT_FORMAT,
    })?;
    let min_len =
        TABLE_HEADER_SIZE
            .checked_add(TABLE_FOOTER_SIZE)
            .ok_or(FormatError::InvalidLength {
                field: TABLE_ARTIFACT_FORMAT,
            })?;
    if byte_count_usize < min_len {
        return Err(FormatError::InsufficientBytes {
            format: TABLE_ARTIFACT_FORMAT,
            needed: min_len,
            actual: byte_count_usize,
        });
    }
    let footer_start = byte_count_usize - TABLE_FOOTER_SIZE;

    let (header, header_len) = decode_table_header(header_bytes)?;
    if header_len != TABLE_HEADER_SIZE {
        return Err(FormatError::InvalidLength {
            field: "table_header_size",
        });
    }
    let footer = decode_table_footer_metadata(footer_bytes, footer_start)?;
    let (index_frame, _) = decode_exact_frame(index_frame_bytes, TableBlockKind::Index)?;
    let index = decode_table_index_block_for_data_blocks(
        index_frame.decoded_payload(),
        header.data_block_count(),
    )?;
    let (properties_frame, _) =
        decode_exact_frame(properties_frame_bytes, TableBlockKind::Properties)?;
    let properties = decode_table_properties(properties_frame.decoded_payload())?;

    validate_metadata_against_header_and_footer(&header, &footer, &index, &properties)?;
    let index_start =
        usize::try_from(footer.index_block_offset()).map_err(|_| FormatError::InvalidLength {
            field: "index_block_offset",
        })?;
    for index_entry in index.entries() {
        validate_index_entry_range(index_entry, index_start)?;
    }

    Ok(ImmutableTableMetadata {
        header,
        index,
        properties,
        byte_count,
    })
}

pub(crate) fn decode_immutable_table_data_block(
    index_entry: &TableIndexEntry,
    frame_bytes: &[u8],
) -> Result<TableDataBlock, FormatError> {
    let (frame, consumed) = decode_exact_frame(frame_bytes, TableBlockKind::Data)?;
    if consumed
        != usize::try_from(index_entry.block_frame_len()).map_err(|_| {
            FormatError::InvalidLength {
                field: "block_frame_len",
            }
        })?
    {
        return Err(FormatError::InvalidLength {
            field: "block_frame_len",
        });
    }
    let block = decode_table_data_block(frame.decoded_payload())?;
    validate_data_block_against_index(index_entry, &block)?;
    Ok(block)
}

fn decode_data_region(
    table_bytes: &[u8],
    index_start: usize,
) -> Result<(Vec<TableDataBlock>, Vec<DataFrameLocation>), FormatError> {
    if index_start < TABLE_HEADER_SIZE {
        return Err(FormatError::InvalidLength {
            field: "index_block_offset",
        });
    }

    let mut data_blocks = Vec::new();
    let mut data_locations = Vec::new();
    let mut offset = TABLE_HEADER_SIZE;
    while offset < index_start {
        let (frame, consumed) =
            decode_table_block_frame_as(&table_bytes[offset..index_start], TableBlockKind::Data)?;
        let frame_len = u32::try_from(consumed).map_err(|_| FormatError::InvalidLength {
            field: "block_frame_len",
        })?;
        data_blocks.push(decode_table_data_block(frame.decoded_payload())?);
        data_locations.push(DataFrameLocation {
            offset: u64::try_from(offset).map_err(|_| FormatError::InvalidLength {
                field: "block_offset",
            })?,
            frame_len,
        });
        offset = offset
            .checked_add(consumed)
            .ok_or(FormatError::InvalidLength {
                field: "data_block_range",
            })?;
    }
    Ok((data_blocks, data_locations))
}

fn decode_exact_frame(
    bytes: &[u8],
    expected_kind: TableBlockKind,
) -> Result<(TableBlockFrame, usize), FormatError> {
    let (frame, consumed) = decode_table_block_frame_as(bytes, expected_kind)?;
    if consumed != bytes.len() {
        return Err(FormatError::TrailingData {
            format: TABLE_ARTIFACT_FORMAT,
            remaining: bytes.len() - consumed,
        });
    }
    Ok((frame, consumed))
}

fn validate_table_against_decoded_facts(
    header: &TableHeader,
    data_blocks: &[TableDataBlock],
    data_locations: &[DataFrameLocation],
    index: &TableIndexBlock,
    properties: &TableProperties,
    index_start: usize,
) -> Result<(), FormatError> {
    if data_blocks.len() != header.data_block_count() as usize {
        return Err(FormatError::InvalidValue {
            field: "data_block_count",
        });
    }
    if data_locations.len() != data_blocks.len() || index.entry_count() != data_blocks.len() {
        return Err(FormatError::InvalidValue {
            field: "index_entry_count",
        });
    }

    let derived_properties = TableProperties::from_data_blocks(data_blocks)?;
    validate_table_properties_against_header(
        &derived_properties,
        header.row_count(),
        header.data_block_count(),
        header.commit_min(),
        header.commit_max(),
    )?;
    if derived_properties != *properties {
        return Err(FormatError::InvalidValue {
            field: "table_properties_facts",
        });
    }
    validate_table_properties_against_header(
        properties,
        header.row_count(),
        header.data_block_count(),
        header.commit_min(),
        header.commit_max(),
    )?;

    // Index entries are durable cross-references into the table byte stream, so
    // they must agree with both the decoded data frames and the decoded rows.
    for ((index_entry, data_block), data_location) in index
        .entries()
        .iter()
        .zip(data_blocks.iter())
        .zip(data_locations.iter())
    {
        validate_index_entry_range(index_entry, index_start)?;
        if index_entry.block_offset() != data_location.offset {
            return Err(FormatError::InvalidValue {
                field: "index_block_offset",
            });
        }
        if index_entry.block_frame_len() != data_location.frame_len {
            return Err(FormatError::InvalidValue {
                field: "index_block_frame_len",
            });
        }
        if index_entry.row_count() as usize != data_block.row_count() {
            return Err(FormatError::InvalidValue {
                field: "index_row_count",
            });
        }
        if index_entry.first_key_bytes() != data_block.first_key_bytes() {
            return Err(FormatError::InvalidValue {
                field: "index_first_key",
            });
        }
        if index_entry.last_key_bytes() != data_block.last_key_bytes() {
            return Err(FormatError::InvalidValue {
                field: "index_last_key",
            });
        }
    }

    Ok(())
}

fn validate_metadata_against_header_and_footer(
    header: &TableHeader,
    footer: &TableFooter,
    index: &TableIndexBlock,
    properties: &TableProperties,
) -> Result<(), FormatError> {
    validate_table_properties_against_header(
        properties,
        header.row_count(),
        header.data_block_count(),
        header.commit_min(),
        header.commit_max(),
    )?;
    if properties.min_key_bytes() != index.entries()[0].first_key_bytes() {
        return Err(FormatError::InvalidValue {
            field: "index_first_key",
        });
    }
    if properties.max_key_bytes() != index.entries()[index.entries().len() - 1].last_key_bytes() {
        return Err(FormatError::InvalidValue {
            field: "index_last_key",
        });
    }
    let indexed_rows = index.entries().iter().try_fold(0_u64, |total, entry| {
        total
            .checked_add(u64::from(entry.row_count()))
            .ok_or(FormatError::InvalidLength { field: "row_count" })
    })?;
    if indexed_rows != header.row_count() {
        return Err(FormatError::InvalidValue {
            field: "index_row_count",
        });
    }
    let index_end = footer
        .index_block_offset()
        .checked_add(u64::from(footer.index_block_frame_len()))
        .ok_or(FormatError::InvalidLength {
            field: "index_block_range",
        })?;
    if index_end != footer.properties_block_offset() {
        return Err(FormatError::InvalidLength {
            field: "properties_block_offset",
        });
    }
    Ok(())
}

fn validate_data_block_against_index(
    index_entry: &TableIndexEntry,
    data_block: &TableDataBlock,
) -> Result<(), FormatError> {
    if index_entry.row_count() as usize != data_block.row_count() {
        return Err(FormatError::InvalidValue {
            field: "index_row_count",
        });
    }
    if index_entry.first_key_bytes() != data_block.first_key_bytes() {
        return Err(FormatError::InvalidValue {
            field: "index_first_key",
        });
    }
    if index_entry.last_key_bytes() != data_block.last_key_bytes() {
        return Err(FormatError::InvalidValue {
            field: "index_last_key",
        });
    }
    Ok(())
}

fn validate_index_entry_range(
    index_entry: &TableIndexEntry,
    index_start: usize,
) -> Result<(), FormatError> {
    let start =
        usize::try_from(index_entry.block_offset()).map_err(|_| FormatError::InvalidLength {
            field: "index_block_offset",
        })?;
    let len =
        usize::try_from(index_entry.block_frame_len()).map_err(|_| FormatError::InvalidLength {
            field: "index_block_frame_len",
        })?;
    let end = start.checked_add(len).ok_or(FormatError::InvalidLength {
        field: "index_block_range",
    })?;
    if start < TABLE_HEADER_SIZE || end > index_start {
        return Err(FormatError::InvalidLength {
            field: "index_block_range",
        });
    }
    Ok(())
}

fn table_range(offset: u64, len: u32, field: &'static str) -> Result<(usize, usize), FormatError> {
    if len == 0 {
        return Err(FormatError::InvalidLength { field });
    }
    let start = usize::try_from(offset).map_err(|_| FormatError::InvalidLength { field })?;
    let len = usize::try_from(len).map_err(|_| FormatError::InvalidLength { field })?;
    let end = start
        .checked_add(len)
        .ok_or(FormatError::InvalidLength { field })?;
    Ok((start, end))
}

fn validate_build_counts(
    row_count: usize,
    block_count: usize,
    target_data_block_size: u32,
) -> Result<(), FormatError> {
    if target_data_block_size == 0 {
        return Err(FormatError::InvalidLength {
            field: "target_data_block_size",
        });
    }
    let row_count =
        u64::try_from(row_count).map_err(|_| FormatError::InvalidLength { field: "row_count" })?;
    if row_count == 0 || row_count > MAX_TABLE_ROWS {
        return Err(FormatError::InvalidLength { field: "row_count" });
    }
    if block_count == 0 || block_count > MAX_TABLE_DATA_BLOCKS as usize {
        return Err(FormatError::InvalidLength {
            field: "data_block_count",
        });
    }
    Ok(())
}
