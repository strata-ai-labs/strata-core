//! Immutable table builder.

use super::{
    FrozenTable, MutableTable, TableBuilderConfig, TableCommitRange, TableIdentity,
    TableInternalKeyBytes, TableKeyRange, TableRow, TableRuntimeConfig, TableRuntimeError,
    TableRuntimeFacts, TableRuntimeResult,
};
use crate::format::{
    ImmutableTableStreamingEncoder, ImmutableTableStreamingOutput, MAX_TABLE_BLOCK_DECODED_BYTES,
    MAX_TABLE_BLOCK_ENTRIES, MAX_TABLE_KEY_BYTES, MAX_TABLE_ROW_BYTES,
};
use crate::observability::perf_trace;
use crate::row::StorageRow;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BuiltTableArtifact {
    bytes: Vec<u8>,
    facts: TableRuntimeFacts,
    rows: Vec<TableRow>,
}

impl BuiltTableArtifact {
    fn new(bytes: Vec<u8>, facts: TableRuntimeFacts, rows: Vec<TableRow>) -> Self {
        Self { bytes, facts, rows }
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

    pub(crate) fn into_parts_with_rows(self) -> (Vec<u8>, TableRuntimeFacts, Vec<TableRow>) {
        (self.bytes, self.facts, self.rows)
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
        let mut builder = self.begin_streaming(identity)?;
        for row in rows {
            builder.append(row)?;
        }
        builder.finish()
    }

    pub(crate) fn build_from_frozen(
        &self,
        identity: TableIdentity,
        table: &FrozenTable,
    ) -> TableRuntimeResult<BuiltTableArtifact> {
        let rows = table
            .iter()
            .map(|row| row.as_ref().clone())
            .collect::<Vec<_>>();
        self.build_from_rows(identity, &rows)
    }

    pub(crate) fn build_from_mutable(
        &self,
        identity: TableIdentity,
        table: &MutableTable,
    ) -> TableRuntimeResult<BuiltTableArtifact> {
        let rows = table
            .iter()
            .map(|row| row.as_ref().clone())
            .collect::<Vec<_>>();
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

    pub(crate) fn begin_streaming(
        &self,
        identity: TableIdentity,
    ) -> TableRuntimeResult<ImmutableTableStreamingBuilder> {
        ImmutableTableStreamingBuilder::new(identity, self.config)
    }
}

pub(crate) struct ImmutableTableStreamingBuilder {
    identity: TableIdentity,
    encoder: ImmutableTableStreamingEncoder,
    config: TableBuilderConfig,
    previous_key: Option<TableInternalKeyBytes>,
    current_block_entries: usize,
    current_block_decoded_len: usize,
    materialized_rows: Vec<TableRow>,
}

impl ImmutableTableStreamingBuilder {
    fn new(identity: TableIdentity, config: TableBuilderConfig) -> TableRuntimeResult<Self> {
        config.validate()?;
        let encoder = ImmutableTableStreamingEncoder::new(
            config.target_data_block_size(),
            config.rows_per_block(),
            config.compression(),
        )
        .map_err(|source| TableRuntimeError::BuildFormat { source })?;
        Ok(Self {
            identity,
            encoder,
            config,
            previous_key: None,
            current_block_entries: 0,
            current_block_decoded_len: 4,
            materialized_rows: Vec::new(),
        })
    }

    pub(crate) fn append(&mut self, row: &TableRow) -> TableRuntimeResult<()> {
        self.validate_next_row(row)?;
        self.encoder
            .append(row.row())
            .map_err(|source| TableRuntimeError::BuildFormat { source })?;
        self.materialized_rows.push(row.clone());
        self.previous_key = Some(row.key().clone());
        Ok(())
    }

    pub(crate) const fn buffered_rows(&self) -> usize {
        self.encoder.buffered_rows()
    }

    pub(crate) const fn peak_buffered_rows(&self) -> usize {
        self.encoder.peak_buffered_rows()
    }

    pub(crate) fn finish(self) -> TableRuntimeResult<BuiltTableArtifact> {
        if self.previous_key.is_none() {
            return Err(TableRuntimeError::InvalidRange { field: "row_count" });
        }
        let materialized_rows = self.materialized_rows;
        let output = self
            .encoder
            .finish_with_metadata()
            .map_err(|source| TableRuntimeError::BuildFormat { source })?;
        build_table_artifact_from_streaming_output(self.identity, output, materialized_rows)
    }

    fn validate_next_row(&mut self, row: &TableRow) -> TableRuntimeResult<()> {
        validate_builder_row_shape(row)?;
        validate_builder_row_order(self.previous_key.as_ref(), row.key())?;
        self.record_block_row_shape(row)?;
        Ok(())
    }

    fn record_block_row_shape(&mut self, row: &TableRow) -> TableRuntimeResult<()> {
        let entry_len = encoded_table_data_entry_len(row)?;
        self.current_block_decoded_len = self
            .current_block_decoded_len
            .checked_add(entry_len)
            .ok_or(TableRuntimeError::InvalidRange {
                field: "data_block_decoded_len",
            })?;
        self.current_block_entries = self.current_block_entries.saturating_add(1);
        if self.current_block_entries > MAX_TABLE_BLOCK_ENTRIES as usize
            || self.current_block_decoded_len > MAX_TABLE_BLOCK_DECODED_BYTES
        {
            return Err(TableRuntimeError::InvalidRange {
                field: "data_block_decoded_len",
            });
        }

        if self.current_block_entries == self.config.rows_per_block() {
            self.current_block_entries = 0;
            self.current_block_decoded_len = 4;
        }
        Ok(())
    }
}

fn build_table_artifact_from_streaming_output(
    identity: TableIdentity,
    output: ImmutableTableStreamingOutput,
    rows: Vec<TableRow>,
) -> TableRuntimeResult<BuiltTableArtifact> {
    let byte_count =
        u64::try_from(output.bytes().len()).map_err(|_| TableRuntimeError::InvalidRange {
            field: "byte_count",
        })?;
    let facts = TableRuntimeFacts::new(
        identity,
        output.row_count(),
        output.data_block_count(),
        TableKeyRange::new(
            output.min_key_bytes().to_vec(),
            output.max_key_bytes().to_vec(),
        )?,
        TableCommitRange::new(output.commit_min(), output.commit_max())?,
        byte_count,
    )?;
    perf_trace::record_table_build_facts_from_streaming_metadata();
    Ok(BuiltTableArtifact::new(output.into_bytes(), facts, rows))
}

fn validate_builder_row_shape(row: &TableRow) -> TableRuntimeResult<()> {
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
    Ok(())
}

fn validate_builder_row_order(
    previous_key: Option<&TableInternalKeyBytes>,
    current_key: &TableInternalKeyBytes,
) -> TableRuntimeResult<()> {
    let Some(previous_key) = previous_key else {
        return Ok(());
    };
    match previous_key.cmp(current_key) {
        std::cmp::Ordering::Less => Ok(()),
        std::cmp::Ordering::Equal => Err(TableRuntimeError::DuplicateInternalKey {
            key: current_key.as_slice().to_vec(),
        }),
        std::cmp::Ordering::Greater => Err(TableRuntimeError::InvalidRowOrder {
            previous: previous_key.as_slice().to_vec(),
            current: current_key.as_slice().to_vec(),
        }),
    }
}

fn encoded_table_data_entry_len(row: &TableRow) -> TableRuntimeResult<usize> {
    let key_len = row.encoded_key().len();
    let row_len = encoded_storage_row_len(row)?;
    4usize
        .checked_add(key_len)
        .and_then(|len| len.checked_add(4))
        .and_then(|len| len.checked_add(row_len))
        .ok_or(TableRuntimeError::InvalidRange {
            field: "table_data_entry_len",
        })
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
