//! KV read outcome types.

use strata_core_next::{CommitVersion, Timestamp};

use super::{KvKey, KvValue};

/// Latest or historical KV value with commit metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KvVersionedValue {
    value: KvValue,
    version: CommitVersion,
    timestamp: Timestamp,
}

impl KvVersionedValue {
    pub(crate) const fn new(value: KvValue, version: CommitVersion, timestamp: Timestamp) -> Self {
        Self {
            value,
            version,
            timestamp,
        }
    }

    #[must_use]
    /// Returns the stored value bytes.
    pub const fn value(&self) -> &KvValue {
        &self.value
    }

    #[must_use]
    /// Returns the commit version that wrote this value.
    pub const fn version(&self) -> CommitVersion {
        self.version
    }

    #[must_use]
    /// Returns the commit timestamp that wrote this value.
    pub const fn timestamp(&self) -> Timestamp {
        self.timestamp
    }
}

/// KV scan row with decoded user key and commit metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KvScanRow {
    key: KvKey,
    value: KvValue,
    version: CommitVersion,
    timestamp: Timestamp,
}

impl KvScanRow {
    pub(crate) const fn new(
        key: KvKey,
        value: KvValue,
        version: CommitVersion,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            key,
            value,
            version,
            timestamp,
        }
    }

    #[must_use]
    /// Returns the decoded user key.
    pub const fn key(&self) -> &KvKey {
        &self.key
    }

    #[must_use]
    /// Returns the stored value bytes.
    pub const fn value(&self) -> &KvValue {
        &self.value
    }

    #[must_use]
    /// Returns the commit version that wrote this row.
    pub const fn version(&self) -> CommitVersion {
        self.version
    }

    #[must_use]
    /// Returns the commit timestamp that wrote this row.
    pub const fn timestamp(&self) -> Timestamp {
        self.timestamp
    }
}

/// Paginated list result for decoded KV keys.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KvListPage {
    keys: Vec<KvKey>,
    has_more: bool,
    cursor: Option<KvKey>,
}

impl KvListPage {
    pub(crate) const fn new(keys: Vec<KvKey>, has_more: bool, cursor: Option<KvKey>) -> Self {
        Self {
            keys,
            has_more,
            cursor,
        }
    }

    #[must_use]
    /// Returns the page keys.
    pub fn keys(&self) -> &[KvKey] {
        &self.keys
    }

    #[must_use]
    /// Returns true when another page is available.
    pub const fn has_more(&self) -> bool {
        self.has_more
    }

    #[must_use]
    /// Returns the cursor for the next page.
    pub const fn cursor(&self) -> Option<&KvKey> {
        self.cursor.as_ref()
    }
}

/// Version-history row for one KV key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KvHistoryRow {
    value: Option<KvValue>,
    tombstone: bool,
    version: CommitVersion,
    timestamp: Timestamp,
}

impl KvHistoryRow {
    pub(crate) const fn new(
        value: Option<KvValue>,
        tombstone: bool,
        version: CommitVersion,
        timestamp: Timestamp,
    ) -> Self {
        Self {
            value,
            tombstone,
            version,
            timestamp,
        }
    }

    #[must_use]
    /// Returns the row value, if this history row is not a tombstone.
    pub const fn value(&self) -> Option<&KvValue> {
        self.value.as_ref()
    }

    #[must_use]
    /// Returns true when this history row represents a delete.
    pub const fn is_tombstone(&self) -> bool {
        self.tombstone
    }

    #[must_use]
    /// Returns the row commit version.
    pub const fn version(&self) -> CommitVersion {
        self.version
    }

    #[must_use]
    /// Returns the row commit timestamp.
    pub const fn timestamp(&self) -> Timestamp {
        self.timestamp
    }
}

/// Full version history for one KV key.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KvHistory {
    rows: Vec<KvHistoryRow>,
}

impl KvHistory {
    pub(crate) const fn new(rows: Vec<KvHistoryRow>) -> Self {
        Self { rows }
    }

    #[must_use]
    /// Returns history rows newest-first.
    pub fn rows(&self) -> &[KvHistoryRow] {
        &self.rows
    }
}

/// KV sample result.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KvSample {
    total_count: u64,
    rows: Vec<KvScanRow>,
}

impl KvSample {
    pub(crate) const fn new(total_count: u64, rows: Vec<KvScanRow>) -> Self {
        Self { total_count, rows }
    }

    #[must_use]
    /// Returns the total number of matching live rows.
    pub const fn total_count(&self) -> u64 {
        self.total_count
    }

    #[must_use]
    /// Returns sampled rows.
    pub fn rows(&self) -> &[KvScanRow] {
        &self.rows
    }
}
