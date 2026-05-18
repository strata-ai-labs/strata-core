//! Immutable table builder.

use super::{
    validate_strictly_sorted_unique_rows, FrozenTable, MutableTable, TableBuilderConfig,
    TableIdentity, TableRow, TableRuntimeConfig, TableRuntimeError, TableRuntimeFacts,
    TableRuntimeResult,
};
use crate::format::{
    decode_immutable_table, encode_immutable_table, MAX_TABLE_BLOCK_DECODED_BYTES,
    MAX_TABLE_BLOCK_ENTRIES, MAX_TABLE_DATA_BLOCKS, MAX_TABLE_KEY_BYTES, MAX_TABLE_ROWS,
    MAX_TABLE_ROW_BYTES,
};
use crate::row::StorageRow;

use super::facts::table_facts_from_decoded;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BuiltTableArtifact {
    bytes: Vec<u8>,
    facts: TableRuntimeFacts,
}

impl BuiltTableArtifact {
    fn new(bytes: Vec<u8>, facts: TableRuntimeFacts) -> Self {
        Self { bytes, facts }
    }

    pub(crate) fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub(crate) const fn facts(&self) -> &TableRuntimeFacts {
        &self.facts
    }

    pub(crate) fn byte_count(&self) -> u64 {
        self.facts.byte_count()
    }

    pub(crate) fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    pub(crate) fn into_parts(self) -> (Vec<u8>, TableRuntimeFacts) {
        (self.bytes, self.facts)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct ImmutableTableBuilder {
    config: TableBuilderConfig,
}

impl ImmutableTableBuilder {
    pub(crate) fn new(config: TableBuilderConfig) -> TableRuntimeResult<Self> {
        config.validate()?;
        Ok(Self { config })
    }

    pub(crate) fn from_runtime_config(config: &TableRuntimeConfig) -> TableRuntimeResult<Self> {
        Self::new(*config.builder())
    }

    pub(crate) const fn config(&self) -> TableBuilderConfig {
        self.config
    }

    pub(crate) fn build_from_rows(
        &self,
        identity: TableIdentity,
        rows: &[TableRow],
    ) -> TableRuntimeResult<BuiltTableArtifact> {
        validate_builder_rows(rows, self.config)?;
        let storage_rows = rows
            .iter()
            .map(|row| row.row().clone())
            .collect::<Vec<StorageRow>>();
        self.build_from_storage_rows_unchecked(identity, &storage_rows)
    }

    pub(crate) fn build_from_frozen(
        &self,
        identity: TableIdentity,
        table: &FrozenTable,
    ) -> TableRuntimeResult<BuiltTableArtifact> {
        let rows = table.iter().cloned().collect::<Vec<_>>();
        self.build_from_rows(identity, &rows)
    }

    pub(crate) fn build_from_mutable(
        &self,
        identity: TableIdentity,
        table: &MutableTable,
    ) -> TableRuntimeResult<BuiltTableArtifact> {
        let rows = table.iter().cloned().collect::<Vec<_>>();
        self.build_from_rows(identity, &rows)
    }

    pub(crate) fn build_from_storage_rows(
        &self,
        identity: TableIdentity,
        rows: &[StorageRow],
    ) -> TableRuntimeResult<BuiltTableArtifact> {
        let rows = rows.iter().cloned().map(TableRow::new).collect::<Vec<_>>();
        self.build_from_rows(identity, &rows)
    }

    fn build_from_storage_rows_unchecked(
        &self,
        identity: TableIdentity,
        rows: &[StorageRow],
    ) -> TableRuntimeResult<BuiltTableArtifact> {
        let bytes = encode_immutable_table(
            rows,
            self.config.target_data_block_size(),
            self.config.rows_per_block(),
            self.config.compression(),
        )
        .map_err(|source| TableRuntimeError::BuildFormat { source })?;
        let decoded = decode_immutable_table(&bytes)
            .map_err(|source| TableRuntimeError::DecodeFormat { source })?;
        let facts = table_facts_from_decoded(identity, &bytes, &decoded)?;
        Ok(BuiltTableArtifact::new(bytes, facts))
    }
}

fn validate_builder_rows(rows: &[TableRow], config: TableBuilderConfig) -> TableRuntimeResult<()> {
    if rows.is_empty() {
        return Err(TableRuntimeError::InvalidRange { field: "row_count" });
    }
    validate_builder_limits_before_encoding(rows, config)?;
    validate_strictly_sorted_unique_rows(rows)
}

fn validate_builder_limits_before_encoding(
    rows: &[TableRow],
    config: TableBuilderConfig,
) -> TableRuntimeResult<()> {
    if u64::try_from(rows.len())
        .map_err(|_| TableRuntimeError::InvalidRange { field: "row_count" })?
        > MAX_TABLE_ROWS
    {
        return Err(TableRuntimeError::InvalidRange { field: "row_count" });
    }
    let block_count = rows.len().div_ceil(config.rows_per_block());
    if block_count == 0 || block_count > MAX_TABLE_DATA_BLOCKS as usize {
        return Err(TableRuntimeError::InvalidRange {
            field: "data_block_count",
        });
    }

    let mut current_block_entries = 0usize;
    let mut current_block_decoded_len = 4usize;
    for row in rows {
        let key_len = row.encoded_key().len();
        if key_len == 0 || key_len > MAX_TABLE_KEY_BYTES {
            return Err(TableRuntimeError::InvalidRange {
                field: "internal_key_len",
            });
        }
        let row_len = encoded_storage_row_len(row)?;
        if row_len == 0 || row_len > MAX_TABLE_ROW_BYTES {
            return Err(TableRuntimeError::InvalidRange { field: "row_len" });
        }
        let entry_len = 4usize
            .checked_add(key_len)
            .and_then(|len| len.checked_add(4))
            .and_then(|len| len.checked_add(row_len))
            .ok_or(TableRuntimeError::InvalidRange {
                field: "table_data_entry_len",
            })?;
        current_block_decoded_len = current_block_decoded_len.checked_add(entry_len).ok_or(
            TableRuntimeError::InvalidRange {
                field: "data_block_decoded_len",
            },
        )?;
        current_block_entries = current_block_entries.saturating_add(1);
        if current_block_entries > MAX_TABLE_BLOCK_ENTRIES as usize
            || current_block_decoded_len > MAX_TABLE_BLOCK_DECODED_BYTES
        {
            return Err(TableRuntimeError::InvalidRange {
                field: "data_block_decoded_len",
            });
        }

        if current_block_entries == config.rows_per_block() {
            current_block_entries = 0;
            current_block_decoded_len = 4;
        }
    }

    Ok(())
}

fn encoded_storage_row_len(row: &TableRow) -> TableRuntimeResult<usize> {
    const STORAGE_ROW_FIXED_BYTES: usize = 1 + 4 + 8 + 8 + 8 + 4 + 1 + 4;
    const INTERNAL_KEY_COMMIT_SUFFIX_BYTES: usize = 8;

    let physical_key_len = row
        .encoded_key()
        .len()
        .checked_sub(INTERNAL_KEY_COMMIT_SUFFIX_BYTES)
        .ok_or(TableRuntimeError::InvalidRange {
            field: "internal_key_len",
        })?;
    STORAGE_ROW_FIXED_BYTES
        .checked_add(physical_key_len)
        .and_then(|len| len.checked_add(row.value().len()))
        .ok_or(TableRuntimeError::InvalidRange { field: "row_len" })
}
