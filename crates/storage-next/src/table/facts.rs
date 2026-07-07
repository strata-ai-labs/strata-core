//! Table identity, facts, and statistics.

use super::key::{TablePhysicalKeyBytes, TableRow};
use super::{TableRuntimeError, TableRuntimeResult};
use crate::format::ImmutableTable;
use strata_core_next::{CommitVersion, Timestamp};

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

/// Per-table summary statistics computed once when a table is sealed.
///
/// These are the values a consumer would otherwise derive by scanning every row
/// of a table: timestamp bounds, physical-key bounds, and the put/tombstone row
/// split. They are immutable for a sealed table, so caching them lets higher
/// layers summarize a table without rescanning its rows. Internal-key bounds,
/// commit range, and row/byte/block counts are NOT duplicated here; they already
/// live on `TableRuntimeFacts`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableSummaryExtras {
    timestamp_min: Option<Timestamp>,
    timestamp_max: Option<Timestamp>,
    physical_key_min: Vec<u8>,
    physical_key_max: Vec<u8>,
    put_rows: u64,
    tombstone_rows: u64,
}

impl TableSummaryExtras {
    /// Compute the summary in a single pass over a sealed table's rows.
    ///
    /// The physical-key and timestamp bounds are the per-row extremes, and the
    /// put/tombstone split counts each row by kind. Equivalence tests and the
    /// recovery-time validation in higher layers guard against a cached summary
    /// drifting from a fresh scan.
    pub(crate) fn from_rows(rows: &[TableRow]) -> TableRuntimeResult<Self> {
        let Some(first) = rows.first() else {
            return Err(TableRuntimeError::InvalidRange { field: "rows" });
        };
        let mut physical_key_min = TablePhysicalKeyBytes::from_row(first.row());
        let mut physical_key_max = physical_key_min.clone();
        let mut timestamp_min = first.commit_timestamp();
        let mut timestamp_max = timestamp_min;
        let mut put_rows = 0u64;
        let mut tombstone_rows = 0u64;
        for row in rows {
            let physical = TablePhysicalKeyBytes::from_row(row.row());
            if physical < physical_key_min {
                physical_key_min = physical.clone();
            }
            if physical > physical_key_max {
                physical_key_max = physical;
            }
            let timestamp = row.commit_timestamp();
            if timestamp < timestamp_min {
                timestamp_min = timestamp;
            }
            if timestamp > timestamp_max {
                timestamp_max = timestamp;
            }
            if row.is_tombstone() {
                tombstone_rows = tombstone_rows.saturating_add(1);
            } else {
                put_rows = put_rows.saturating_add(1);
            }
        }
        Ok(Self {
            timestamp_min: Some(timestamp_min),
            timestamp_max: Some(timestamp_max),
            physical_key_min: physical_key_min.as_slice().to_vec(),
            physical_key_max: physical_key_max.as_slice().to_vec(),
            put_rows,
            tombstone_rows,
        })
    }

    /// Reassemble the summary from separately-persisted parts, without a row scan.
    ///
    /// Recovery uses this once durable readers are lazy: the timestamp/physical-key
    /// bounds come from the table's durable record (`TableManifestTableFacts` /
    /// `TableManifestTableBounds`, persisted since BS4.4a) and the put/tombstone split
    /// from the durable row-split extension (BS4.4g). A `from_rows` scan over the same
    /// sealed table yields a byte-identical summary — the recovery-time equivalence the
    /// row-split extension exists to preserve. The bound orderings are structural
    /// invariants of any non-empty table; they are debug-checked here so a corrupt or
    /// mismatched durable record fails loudly in tests without a release-path scan.
    pub(crate) fn from_parts(
        timestamp_min: Option<Timestamp>,
        timestamp_max: Option<Timestamp>,
        physical_key_min: Vec<u8>,
        physical_key_max: Vec<u8>,
        put_rows: u64,
        tombstone_rows: u64,
    ) -> Self {
        debug_assert!(
            physical_key_min <= physical_key_max,
            "physical-key bounds must be ordered"
        );
        debug_assert!(
            timestamp_min <= timestamp_max,
            "timestamp bounds must be ordered"
        );
        Self {
            timestamp_min,
            timestamp_max,
            physical_key_min,
            physical_key_max,
            put_rows,
            tombstone_rows,
        }
    }

    pub(crate) const fn timestamp_min(&self) -> Option<Timestamp> {
        self.timestamp_min
    }

    pub(crate) const fn timestamp_max(&self) -> Option<Timestamp> {
        self.timestamp_max
    }

    pub(crate) fn physical_key_min(&self) -> &[u8] {
        &self.physical_key_min
    }

    pub(crate) fn physical_key_max(&self) -> &[u8] {
        &self.physical_key_max
    }

    pub(crate) const fn put_rows(&self) -> u64 {
        self.put_rows
    }

    pub(crate) const fn tombstone_rows(&self) -> u64 {
        self.tombstone_rows
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

pub(super) fn table_facts_from_decoded(
    identity: TableIdentity,
    bytes: &[u8],
    decoded: &ImmutableTable,
) -> TableRuntimeResult<TableRuntimeFacts> {
    let properties = decoded.properties();
    let byte_count = u64::try_from(bytes.len()).map_err(|_| TableRuntimeError::InvalidRange {
        field: "byte_count",
    })?;
    let key_range = TableKeyRange::new(
        properties.min_key_bytes().to_vec(),
        properties.max_key_bytes().to_vec(),
    )?;
    let commit_range = TableCommitRange::new(properties.commit_min(), properties.commit_max())?;

    TableRuntimeFacts::new(
        identity,
        properties.row_count(),
        properties.data_block_count(),
        key_range,
        commit_range,
        byte_count,
    )
}
