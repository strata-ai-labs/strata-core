use super::super::{
    key::{decode_internal_key, encode_internal_key},
    ByteReader, FormatError,
};
use super::data::TableDataBlock;
use super::{validate_table_version, MAX_TABLE_DATA_BLOCKS, MAX_TABLE_KEY_BYTES, MAX_TABLE_ROWS};
use crate::row::InternalKey;
use strata_core::CommitVersion;

const TABLE_PROPERTIES_BLOCK_FORMAT: &str = "table_properties_block";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableProperties {
    row_count: u64,
    data_block_count: u32,
    commit_min: CommitVersion,
    commit_max: CommitVersion,
    min_key: InternalKey,
    min_key_bytes: Vec<u8>,
    max_key: InternalKey,
    max_key_bytes: Vec<u8>,
}

impl TableProperties {
    pub(crate) fn new(
        row_count: u64,
        data_block_count: u32,
        commit_min: CommitVersion,
        commit_max: CommitVersion,
        min_key: &InternalKey,
        max_key: &InternalKey,
    ) -> Result<Self, FormatError> {
        let min_key_bytes = encode_internal_key(min_key);
        let max_key_bytes = encode_internal_key(max_key);
        Self::from_parts(
            row_count,
            data_block_count,
            commit_min,
            commit_max,
            min_key_bytes,
            max_key_bytes,
        )
    }

    pub(crate) fn from_encoded_key_bytes(
        row_count: u64,
        data_block_count: u32,
        commit_min: CommitVersion,
        commit_max: CommitVersion,
        min_key_bytes: Vec<u8>,
        max_key_bytes: Vec<u8>,
    ) -> Result<Self, FormatError> {
        Self::from_parts(
            row_count,
            data_block_count,
            commit_min,
            commit_max,
            min_key_bytes,
            max_key_bytes,
        )
    }

    pub(crate) fn from_data_blocks(blocks: &[TableDataBlock]) -> Result<Self, FormatError> {
        if blocks.is_empty() || blocks.len() > MAX_TABLE_DATA_BLOCKS as usize {
            return Err(FormatError::InvalidLength {
                field: "data_block_count",
            });
        }
        validate_data_block_order(blocks)?;

        let mut row_count = 0u64;
        let mut commit_min: Option<CommitVersion> = None;
        let mut commit_max: Option<CommitVersion> = None;
        for block in blocks {
            row_count = row_count
                .checked_add(block.row_count() as u64)
                .ok_or(FormatError::InvalidLength { field: "row_count" })?;
            for row in block.rows() {
                let version = row.commit_version();
                commit_min = Some(match commit_min {
                    Some(current) if current.as_u64() <= version.as_u64() => current,
                    _ => version,
                });
                commit_max = Some(match commit_max {
                    Some(current) if current.as_u64() >= version.as_u64() => current,
                    _ => version,
                });
            }
        }

        let first_block = &blocks[0];
        let last_block = &blocks[blocks.len() - 1];
        Self::from_parts(
            row_count,
            u32::try_from(blocks.len()).map_err(|_| FormatError::InvalidLength {
                field: "data_block_count",
            })?,
            commit_min.ok_or(FormatError::InvalidLength { field: "row_count" })?,
            commit_max.ok_or(FormatError::InvalidLength { field: "row_count" })?,
            first_block.first_key_bytes().to_vec(),
            last_block.last_key_bytes().to_vec(),
        )
    }

    fn from_parts(
        row_count: u64,
        data_block_count: u32,
        commit_min: CommitVersion,
        commit_max: CommitVersion,
        min_key_bytes: Vec<u8>,
        max_key_bytes: Vec<u8>,
    ) -> Result<Self, FormatError> {
        validate_count_facts(row_count, data_block_count, commit_min, commit_max)?;
        validate_key_len(min_key_bytes.len(), "min_key_len")?;
        validate_key_len(max_key_bytes.len(), "max_key_len")?;
        let min_key = decode_internal_key(&min_key_bytes)?;
        let max_key = decode_internal_key(&max_key_bytes)?;
        if min_key_bytes > max_key_bytes {
            return Err(FormatError::InvalidValue {
                field: "table_key_range",
            });
        }

        Ok(Self {
            row_count,
            data_block_count,
            commit_min,
            commit_max,
            min_key,
            min_key_bytes,
            max_key,
            max_key_bytes,
        })
    }

    pub(crate) const fn row_count(&self) -> u64 {
        self.row_count
    }

    pub(crate) const fn data_block_count(&self) -> u32 {
        self.data_block_count
    }

    pub(crate) const fn commit_min(&self) -> CommitVersion {
        self.commit_min
    }

    pub(crate) const fn commit_max(&self) -> CommitVersion {
        self.commit_max
    }

    pub(crate) const fn min_key(&self) -> &InternalKey {
        &self.min_key
    }

    pub(crate) fn min_key_bytes(&self) -> &[u8] {
        &self.min_key_bytes
    }

    pub(crate) const fn max_key(&self) -> &InternalKey {
        &self.max_key
    }

    pub(crate) fn max_key_bytes(&self) -> &[u8] {
        &self.max_key_bytes
    }
}

pub(crate) fn encode_table_properties(
    properties: &TableProperties,
) -> Result<Vec<u8>, FormatError> {
    validate_count_facts(
        properties.row_count(),
        properties.data_block_count(),
        properties.commit_min(),
        properties.commit_max(),
    )?;

    let mut bytes = Vec::new();
    bytes.extend_from_slice(&1u32.to_le_bytes());
    bytes.extend_from_slice(&properties.row_count().to_le_bytes());
    bytes.extend_from_slice(&properties.data_block_count().to_le_bytes());
    bytes.extend_from_slice(&properties.commit_min().as_u64().to_le_bytes());
    bytes.extend_from_slice(&properties.commit_max().as_u64().to_le_bytes());
    append_len_prefixed(&mut bytes, properties.min_key_bytes(), "min_key_len")?;
    append_len_prefixed(&mut bytes, properties.max_key_bytes(), "max_key_len")?;
    Ok(bytes)
}

pub(crate) fn decode_table_properties(bytes: &[u8]) -> Result<TableProperties, FormatError> {
    let mut reader = ByteReader::new(TABLE_PROPERTIES_BLOCK_FORMAT, bytes);
    validate_table_version(reader.read_u32_le()?, TABLE_PROPERTIES_BLOCK_FORMAT)?;
    let row_count = reader.read_u64_le()?;
    let data_block_count = reader.read_u32_le()?;
    let commit_min = CommitVersion::new(reader.read_u64_le()?);
    let commit_max = CommitVersion::new(reader.read_u64_le()?);
    let min_key_len = read_key_len(&mut reader, "min_key_len")?;
    let min_key_bytes = reader.read_exact(min_key_len)?.to_vec();
    let max_key_len = read_key_len(&mut reader, "max_key_len")?;
    let max_key_bytes = reader.read_exact(max_key_len)?.to_vec();
    reader.finish()?;

    TableProperties::from_parts(
        row_count,
        data_block_count,
        commit_min,
        commit_max,
        min_key_bytes,
        max_key_bytes,
    )
}

pub(crate) fn validate_table_properties_against_header(
    properties: &TableProperties,
    row_count: u64,
    data_block_count: u32,
    commit_min: CommitVersion,
    commit_max: CommitVersion,
) -> Result<(), FormatError> {
    if properties.row_count() != row_count
        || properties.data_block_count() != data_block_count
        || properties.commit_min() != commit_min
        || properties.commit_max() != commit_max
    {
        return Err(FormatError::InvalidValue {
            field: "table_properties_facts",
        });
    }
    Ok(())
}

fn validate_data_block_order(blocks: &[TableDataBlock]) -> Result<(), FormatError> {
    for pair in blocks.windows(2) {
        // Whole-table decode later verifies that index entries point at these
        // exact frames. This helper still rejects impossible table-level key
        // ranges before deriving durable properties from caller-supplied blocks.
        if pair[0].first_key_bytes() >= pair[1].first_key_bytes() {
            return Err(FormatError::InvalidValue {
                field: "table_block_key_order",
            });
        }
        if pair[0].last_key_bytes() >= pair[1].first_key_bytes() {
            return Err(FormatError::InvalidValue {
                field: "table_block_key_overlap",
            });
        }
    }
    Ok(())
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

fn validate_count_facts(
    row_count: u64,
    data_block_count: u32,
    commit_min: CommitVersion,
    commit_max: CommitVersion,
) -> Result<(), FormatError> {
    if row_count == 0 || row_count > MAX_TABLE_ROWS {
        return Err(FormatError::InvalidLength { field: "row_count" });
    }
    if data_block_count == 0 || data_block_count > MAX_TABLE_DATA_BLOCKS {
        return Err(FormatError::InvalidLength {
            field: "data_block_count",
        });
    }
    if commit_min.as_u64() > commit_max.as_u64() {
        return Err(FormatError::InvalidValue {
            field: "commit_range",
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
