//! Table identity, facts, and statistics.

use super::{TableRuntimeError, TableRuntimeResult};
use strata_core_next::CommitVersion;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(crate) struct TableIdentity {
    text: String,
}

impl TableIdentity {
    pub(crate) fn new(text: impl Into<String>) -> TableRuntimeResult<Self> {
        let text = text.into();
        validate_identity_text(&text)?;
        Ok(Self { text })
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.text
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableKeyRange {
    first_key: Vec<u8>,
    last_key: Vec<u8>,
}

impl TableKeyRange {
    pub(crate) fn new(
        first_key: impl Into<Vec<u8>>,
        last_key: impl Into<Vec<u8>>,
    ) -> TableRuntimeResult<Self> {
        let first_key = first_key.into();
        let last_key = last_key.into();
        if first_key.is_empty() {
            return Err(TableRuntimeError::InvalidRange { field: "first_key" });
        }
        if last_key.is_empty() {
            return Err(TableRuntimeError::InvalidRange { field: "last_key" });
        }
        if first_key > last_key {
            return Err(TableRuntimeError::InvalidRange { field: "key_range" });
        }
        Ok(Self {
            first_key,
            last_key,
        })
    }

    pub(crate) fn first_key(&self) -> &[u8] {
        &self.first_key
    }

    pub(crate) fn last_key(&self) -> &[u8] {
        &self.last_key
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TableCommitRange {
    min: CommitVersion,
    max: CommitVersion,
}

impl TableCommitRange {
    pub(crate) fn new(min: CommitVersion, max: CommitVersion) -> TableRuntimeResult<Self> {
        if min > max {
            return Err(TableRuntimeError::InvalidRange {
                field: "commit_range",
            });
        }
        Ok(Self { min, max })
    }

    pub(crate) const fn min(self) -> CommitVersion {
        self.min
    }

    pub(crate) const fn max(self) -> CommitVersion {
        self.max
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableRuntimeFacts {
    identity: TableIdentity,
    row_count: u64,
    data_block_count: u32,
    key_range: TableKeyRange,
    commit_range: TableCommitRange,
    byte_count: u64,
}

impl TableRuntimeFacts {
    pub(crate) fn new(
        identity: TableIdentity,
        row_count: u64,
        data_block_count: u32,
        key_range: TableKeyRange,
        commit_range: TableCommitRange,
        byte_count: u64,
    ) -> TableRuntimeResult<Self> {
        if row_count == 0 {
            return Err(TableRuntimeError::InvalidRange { field: "row_count" });
        }
        if data_block_count == 0 {
            return Err(TableRuntimeError::InvalidRange {
                field: "data_block_count",
            });
        }
        if u64::from(data_block_count) > row_count {
            return Err(TableRuntimeError::InvalidRange {
                field: "data_block_count",
            });
        }
        if byte_count == 0 {
            return Err(TableRuntimeError::InvalidRange {
                field: "byte_count",
            });
        }
        Ok(Self {
            identity,
            row_count,
            data_block_count,
            key_range,
            commit_range,
            byte_count,
        })
    }

    pub(crate) const fn identity(&self) -> &TableIdentity {
        &self.identity
    }

    pub(crate) const fn row_count(&self) -> u64 {
        self.row_count
    }

    pub(crate) const fn data_block_count(&self) -> u32 {
        self.data_block_count
    }

    pub(crate) const fn key_range(&self) -> &TableKeyRange {
        &self.key_range
    }

    pub(crate) const fn commit_range(&self) -> TableCommitRange {
        self.commit_range
    }

    pub(crate) const fn byte_count(&self) -> u64 {
        self.byte_count
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct TableRuntimeStats {
    rows_read: u64,
    bytes_read: u64,
    cache_hits: u64,
    cache_misses: u64,
    compaction_input_rows: u64,
    compaction_output_rows: u64,
}

impl TableRuntimeStats {
    pub(crate) const fn new(
        rows_read: u64,
        bytes_read: u64,
        cache_hits: u64,
        cache_misses: u64,
        compaction_input_rows: u64,
        compaction_output_rows: u64,
    ) -> Self {
        Self {
            rows_read,
            bytes_read,
            cache_hits,
            cache_misses,
            compaction_input_rows,
            compaction_output_rows,
        }
    }

    pub(crate) const fn rows_read(self) -> u64 {
        self.rows_read
    }

    pub(crate) const fn bytes_read(self) -> u64 {
        self.bytes_read
    }

    pub(crate) const fn cache_hits(self) -> u64 {
        self.cache_hits
    }

    pub(crate) const fn cache_misses(self) -> u64 {
        self.cache_misses
    }

    pub(crate) const fn compaction_input_rows(self) -> u64 {
        self.compaction_input_rows
    }

    pub(crate) const fn compaction_output_rows(self) -> u64 {
        self.compaction_output_rows
    }
}

fn validate_identity_text(text: &str) -> TableRuntimeResult<()> {
    if text.is_empty() {
        return Err(TableRuntimeError::InvalidConfig {
            field: "table_identity",
            reason: "must not be empty",
        });
    }
    if text.bytes().any(|byte| byte == b'/' || byte == 0) {
        return Err(TableRuntimeError::InvalidConfig {
            field: "table_identity",
            reason: "must be an opaque single component",
        });
    }
    Ok(())
}
