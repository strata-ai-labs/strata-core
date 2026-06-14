use super::super::{
    key::{decode_internal_key, encode_internal_key},
    ByteReader, FormatError,
};
use super::{validate_table_version, MAX_TABLE_DATA_BLOCKS, MAX_TABLE_KEY_BYTES};
use crate::row::InternalKey;

const TABLE_INDEX_BLOCK_FORMAT: &str = "table_index_block";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableIndexEntry {
    first_key: InternalKey,
    first_key_bytes: Vec<u8>,
    last_key: InternalKey,
    last_key_bytes: Vec<u8>,
    block_offset: u64,
    block_frame_len: u32,
    row_count: u32,
}

impl TableIndexEntry {
    pub(crate) fn new(
        first_key: &InternalKey,
        last_key: &InternalKey,
        block_offset: u64,
        block_frame_len: u32,
        row_count: u32,
    ) -> Result<Self, FormatError> {
        let first_key_bytes = encode_internal_key(first_key);
        let last_key_bytes = encode_internal_key(last_key);
        Self::from_parts(
            first_key_bytes,
            last_key_bytes,
            block_offset,
            block_frame_len,
            row_count,
        )
    }

    pub(crate) fn from_encoded_key_bytes(
        first_key_bytes: Vec<u8>,
        last_key_bytes: Vec<u8>,
        block_offset: u64,
        block_frame_len: u32,
        row_count: u32,
    ) -> Result<Self, FormatError> {
        Self::from_parts(
            first_key_bytes,
            last_key_bytes,
            block_offset,
            block_frame_len,
            row_count,
        )
    }

    fn from_parts(
        first_key_bytes: Vec<u8>,
        last_key_bytes: Vec<u8>,
        block_offset: u64,
        block_frame_len: u32,
        row_count: u32,
    ) -> Result<Self, FormatError> {
        validate_key_len(first_key_bytes.len(), "first_key_len")?;
        validate_key_len(last_key_bytes.len(), "last_key_len")?;
        let first_key = decode_internal_key(&first_key_bytes)?;
        let last_key = decode_internal_key(&last_key_bytes)?;
        if first_key_bytes > last_key_bytes {
            return Err(FormatError::InvalidValue {
                field: "index_key_range",
            });
        }
        if block_frame_len == 0 {
            return Err(FormatError::InvalidLength {
                field: "block_frame_len",
            });
        }
        if row_count == 0 {
            return Err(FormatError::InvalidLength { field: "row_count" });
        }

        Ok(Self {
            first_key,
            first_key_bytes,
            last_key,
            last_key_bytes,
            block_offset,
            block_frame_len,
            row_count,
        })
    }

    pub(crate) const fn first_key(&self) -> &InternalKey {
        &self.first_key
    }

    pub(crate) fn first_key_bytes(&self) -> &[u8] {
        &self.first_key_bytes
    }

    pub(crate) const fn last_key(&self) -> &InternalKey {
        &self.last_key
    }

    pub(crate) fn last_key_bytes(&self) -> &[u8] {
        &self.last_key_bytes
    }

    pub(crate) const fn block_offset(&self) -> u64 {
        self.block_offset
    }

    pub(crate) const fn block_frame_len(&self) -> u32 {
        self.block_frame_len
    }

    pub(crate) const fn row_count(&self) -> u32 {
        self.row_count
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableIndexBlock {
    entries: Vec<TableIndexEntry>,
}

impl TableIndexBlock {
    pub(crate) fn new(entries: Vec<TableIndexEntry>) -> Result<Self, FormatError> {
        validate_count(entries.len())?;
        validate_entry_order(&entries)?;
        Ok(Self { entries })
    }

    pub(crate) fn entries(&self) -> &[TableIndexEntry] {
        &self.entries
    }

    pub(crate) const fn entry_count(&self) -> usize {
        self.entries.len()
    }
}

pub(crate) fn encode_table_index_block(block: &TableIndexBlock) -> Result<Vec<u8>, FormatError> {
    validate_count(block.entries().len())?;
    let entry_count =
        u32::try_from(block.entries().len()).map_err(|_| FormatError::InvalidLength {
            field: "entry_count",
        })?;

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&entry_count.to_le_bytes());
    for entry in block.entries() {
        append_len_prefixed(&mut bytes, entry.first_key_bytes(), "first_key_len")?;
        append_len_prefixed(&mut bytes, entry.last_key_bytes(), "last_key_len")?;
        bytes.extend_from_slice(&entry.block_offset().to_le_bytes());
        bytes.extend_from_slice(&entry.block_frame_len().to_le_bytes());
        bytes.extend_from_slice(&entry.row_count().to_le_bytes());
    }
    Ok(bytes)
}

pub(crate) fn decode_table_index_block(bytes: &[u8]) -> Result<TableIndexBlock, FormatError> {
    let mut reader = ByteReader::new(TABLE_INDEX_BLOCK_FORMAT, bytes);
    validate_table_version(reader.read_u32_le()?, TABLE_INDEX_BLOCK_FORMAT)?;
    let entry_count =
        usize::try_from(reader.read_u32_le()?).map_err(|_| FormatError::InvalidLength {
            field: "entry_count",
        })?;
    validate_count(entry_count)?;

    let mut entries = Vec::new();
    for _ in 0..entry_count {
        let first_key_len = read_key_len(&mut reader, "first_key_len")?;
        let first_key_bytes = reader.read_exact(first_key_len)?.to_vec();
        let last_key_len = read_key_len(&mut reader, "last_key_len")?;
        let last_key_bytes = reader.read_exact(last_key_len)?.to_vec();
        let block_offset = reader.read_u64_le()?;
        let block_frame_len = reader.read_u32_le()?;
        let row_count = reader.read_u32_le()?;
        // Offset bounds and frame contents require the surrounding table bytes;
        // payload-local validation checks the entry shape, and whole-table
        // decode checks that these facts point at real data-block frames.
        entries.push(TableIndexEntry::from_parts(
            first_key_bytes,
            last_key_bytes,
            block_offset,
            block_frame_len,
            row_count,
        )?);
    }
    reader.finish()?;

    TableIndexBlock::new(entries)
}

pub(crate) fn decode_table_index_block_for_data_blocks(
    bytes: &[u8],
    expected_data_block_count: u32,
) -> Result<TableIndexBlock, FormatError> {
    let block = decode_table_index_block(bytes)?;
    if block.entry_count() != expected_data_block_count as usize {
        return Err(FormatError::InvalidValue {
            field: "index_entry_count",
        });
    }
    Ok(block)
}

fn append_len_prefixed(
    target: &mut Vec<u8>,
    payload: &[u8],
    field: &'static str,
) -> Result<(), FormatError> {
    validate_key_len(payload.len(), field)?;
    let len = u32::try_from(payload.len()).map_err(|_| FormatError::InvalidLength { field })?;
    target.extend_from_slice(&len.to_le_bytes());
    target.extend_from_slice(payload);
    Ok(())
}

fn read_key_len(reader: &mut ByteReader<'_>, field: &'static str) -> Result<usize, FormatError> {
    let len =
        usize::try_from(reader.read_u32_le()?).map_err(|_| FormatError::InvalidLength { field })?;
    validate_key_len(len, field)?;
    Ok(len)
}

fn validate_count(count: usize) -> Result<(), FormatError> {
    if count == 0 || count > MAX_TABLE_DATA_BLOCKS as usize {
        return Err(FormatError::InvalidLength {
            field: "entry_count",
        });
    }
    Ok(())
}

fn validate_key_len(len: usize, field: &'static str) -> Result<(), FormatError> {
    if len == 0 || len > MAX_TABLE_KEY_BYTES {
        return Err(FormatError::InvalidLength { field });
    }
    Ok(())
}

fn validate_entry_order(entries: &[TableIndexEntry]) -> Result<(), FormatError> {
    for pair in entries.windows(2) {
        if pair[0].first_key_bytes() >= pair[1].first_key_bytes() {
            return Err(FormatError::InvalidValue {
                field: "index_key_order",
            });
        }
        if pair[0].last_key_bytes() >= pair[1].first_key_bytes() {
            return Err(FormatError::InvalidValue {
                field: "index_key_overlap",
            });
        }
    }
    Ok(())
}
