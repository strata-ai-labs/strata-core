use super::super::{
    key::{decode_internal_key, encode_internal_key},
    storage_row::{decode_storage_row, encode_storage_row},
    ByteReader, FormatError,
};
use super::{MAX_TABLE_BLOCK_ENTRIES, MAX_TABLE_KEY_BYTES, MAX_TABLE_ROW_BYTES};
use crate::row::{InternalKey, StorageRow};

const TABLE_DATA_BLOCK_FORMAT: &str = "table_data_block";

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableDataEntry {
    internal_key: InternalKey,
    internal_key_bytes: Vec<u8>,
    row: StorageRow,
    row_bytes: Vec<u8>,
}

impl TableDataEntry {
    pub(crate) fn new(row: &StorageRow) -> Result<Self, FormatError> {
        let internal_key = InternalKey::new(row.physical_key().clone(), row.commit_version());
        let internal_key_bytes = encode_internal_key(&internal_key);
        let row_bytes = encode_storage_row(row)?;
        Self::from_parts(internal_key_bytes, row_bytes)
    }

    fn from_parts(internal_key_bytes: Vec<u8>, row_bytes: Vec<u8>) -> Result<Self, FormatError> {
        validate_len(
            internal_key_bytes.len(),
            MAX_TABLE_KEY_BYTES,
            "internal_key_len",
        )?;
        validate_len(row_bytes.len(), MAX_TABLE_ROW_BYTES, "row_len")?;

        let internal_key = decode_internal_key(&internal_key_bytes)?;
        let row = decode_storage_row(&row_bytes)?;
        // The key is stored redundantly so table scans can use ordering bytes
        // without decoding every row, but the row remains the authoritative
        // storage fact. Decode rejects drift between the two copies.
        if internal_key.physical_key() != row.physical_key() {
            return Err(FormatError::InvalidValue {
                field: "physical_key",
            });
        }
        if internal_key.commit_version() != row.commit_version() {
            return Err(FormatError::InvalidValue {
                field: "commit_version",
            });
        }

        Ok(Self {
            internal_key,
            internal_key_bytes,
            row,
            row_bytes,
        })
    }

    pub(crate) const fn internal_key(&self) -> &InternalKey {
        &self.internal_key
    }

    pub(crate) fn internal_key_bytes(&self) -> &[u8] {
        &self.internal_key_bytes
    }

    pub(crate) const fn row(&self) -> &StorageRow {
        &self.row
    }

    pub(crate) fn row_bytes(&self) -> &[u8] {
        &self.row_bytes
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableDataBlock {
    entries: Vec<TableDataEntry>,
}

impl TableDataBlock {
    pub(crate) fn from_rows(rows: &[StorageRow]) -> Result<Self, FormatError> {
        let entries = rows
            .iter()
            .map(TableDataEntry::new)
            .collect::<Result<Vec<_>, _>>()?;
        Self::from_entries(entries)
    }

    fn from_entries(entries: Vec<TableDataEntry>) -> Result<Self, FormatError> {
        validate_count(entries.len())?;
        validate_entry_order(&entries)?;
        Ok(Self { entries })
    }

    pub(crate) fn entries(&self) -> &[TableDataEntry] {
        &self.entries
    }

    pub(crate) fn rows(&self) -> impl Iterator<Item = &StorageRow> {
        self.entries.iter().map(TableDataEntry::row)
    }

    pub(crate) const fn row_count(&self) -> usize {
        self.entries.len()
    }

    pub(crate) fn first_key_bytes(&self) -> &[u8] {
        self.entries[0].internal_key_bytes()
    }

    pub(crate) fn last_key_bytes(&self) -> &[u8] {
        self.entries[self.entries.len() - 1].internal_key_bytes()
    }
}

pub(crate) fn encode_table_data_block(block: &TableDataBlock) -> Result<Vec<u8>, FormatError> {
    validate_count(block.entries().len())?;
    let entry_count =
        u32::try_from(block.entries().len()).map_err(|_| FormatError::InvalidLength {
            field: "entry_count",
        })?;
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&entry_count.to_le_bytes());
    for entry in block.entries() {
        append_len_prefixed(&mut bytes, entry.internal_key_bytes(), "internal_key_len")?;
        append_len_prefixed(&mut bytes, entry.row_bytes(), "row_len")?;
    }
    Ok(bytes)
}

pub(crate) fn decode_table_data_block(bytes: &[u8]) -> Result<TableDataBlock, FormatError> {
    let mut reader = ByteReader::new(TABLE_DATA_BLOCK_FORMAT, bytes);
    let entry_count =
        usize::try_from(reader.read_u32_le()?).map_err(|_| FormatError::InvalidLength {
            field: "entry_count",
        })?;
    validate_count(entry_count)?;

    let mut entries = Vec::new();
    for _ in 0..entry_count {
        let internal_key_len = read_len(&mut reader, MAX_TABLE_KEY_BYTES, "internal_key_len")?;
        let internal_key_bytes = reader.read_exact(internal_key_len)?.to_vec();
        let row_len = read_len(&mut reader, MAX_TABLE_ROW_BYTES, "row_len")?;
        let row_bytes = reader.read_exact(row_len)?.to_vec();
        entries.push(TableDataEntry::from_parts(internal_key_bytes, row_bytes)?);
    }
    reader.finish()?;

    TableDataBlock::from_entries(entries)
}

fn append_len_prefixed(
    target: &mut Vec<u8>,
    payload: &[u8],
    field: &'static str,
) -> Result<(), FormatError> {
    let max = match field {
        "internal_key_len" => MAX_TABLE_KEY_BYTES,
        _ => MAX_TABLE_ROW_BYTES,
    };
    validate_len(payload.len(), max, field)?;
    let len = u32::try_from(payload.len()).map_err(|_| FormatError::InvalidLength { field })?;
    target.extend_from_slice(&len.to_le_bytes());
    target.extend_from_slice(payload);
    Ok(())
}

fn read_len(
    reader: &mut ByteReader<'_>,
    max: usize,
    field: &'static str,
) -> Result<usize, FormatError> {
    let len =
        usize::try_from(reader.read_u32_le()?).map_err(|_| FormatError::InvalidLength { field })?;
    validate_len(len, max, field)?;
    Ok(len)
}

fn validate_count(count: usize) -> Result<(), FormatError> {
    if count == 0 || count > MAX_TABLE_BLOCK_ENTRIES as usize {
        return Err(FormatError::InvalidLength {
            field: "entry_count",
        });
    }
    Ok(())
}

fn validate_len(len: usize, max: usize, field: &'static str) -> Result<(), FormatError> {
    if len == 0 || len > max {
        return Err(FormatError::InvalidLength { field });
    }
    Ok(())
}

fn validate_entry_order(entries: &[TableDataEntry]) -> Result<(), FormatError> {
    for pair in entries.windows(2) {
        match pair[0]
            .internal_key_bytes()
            .cmp(pair[1].internal_key_bytes())
        {
            std::cmp::Ordering::Less => {}
            std::cmp::Ordering::Equal => {
                return Err(FormatError::InvalidValue {
                    field: "internal_key",
                });
            }
            std::cmp::Ordering::Greater => {
                return Err(FormatError::InvalidValue {
                    field: "internal_key_order",
                });
            }
        }
    }
    Ok(())
}
