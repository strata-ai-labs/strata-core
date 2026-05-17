//! Immutable table builder.

use super::{
    validate_strictly_sorted_unique_rows, FrozenTable, MutableTable, TableBuilderConfig,
    TableIdentity, TableRow, TableRuntimeConfig, TableRuntimeError, TableRuntimeFacts,
    TableRuntimeResult,
};
use crate::format::{decode_immutable_table, encode_immutable_table};
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
        validate_builder_rows(rows)?;
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

fn validate_builder_rows(rows: &[TableRow]) -> TableRuntimeResult<()> {
    if rows.is_empty() {
        return Err(TableRuntimeError::InvalidRange { field: "row_count" });
    }
    validate_strictly_sorted_unique_rows(rows)
}
