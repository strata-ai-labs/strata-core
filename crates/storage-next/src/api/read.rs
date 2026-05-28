//! API read request shells.

use strata_core_next::{BranchId, CommitVersion, Timestamp};

use super::{ReadLimit, ScanRange, StorageKey, StorageSpaceId, StorageValue};

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReadBound {
    Latest,
    AtVersion(CommitVersion),
    AtTimestamp(Timestamp),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PointReadRequest {
    branch_id: BranchId,
    storage_space: StorageSpaceId,
    key: StorageKey,
    bound: ReadBound,
}

impl PointReadRequest {
    #[must_use]
    pub const fn new(
        branch_id: BranchId,
        storage_space: StorageSpaceId,
        key: StorageKey,
        bound: ReadBound,
    ) -> Self {
        Self {
            branch_id,
            storage_space,
            key,
            bound,
        }
    }

    #[must_use]
    pub const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    #[must_use]
    pub const fn storage_space(&self) -> &StorageSpaceId {
        &self.storage_space
    }

    #[must_use]
    pub const fn key(&self) -> &StorageKey {
        &self.key
    }

    #[must_use]
    pub const fn bound(&self) -> ReadBound {
        self.bound
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryReadRequest {
    branch_id: BranchId,
    storage_space: StorageSpaceId,
    key: StorageKey,
    before_version: Option<CommitVersion>,
    limit: Option<ReadLimit>,
    include_tombstones: bool,
}

impl HistoryReadRequest {
    #[must_use]
    pub const fn new(branch_id: BranchId, storage_space: StorageSpaceId, key: StorageKey) -> Self {
        Self {
            branch_id,
            storage_space,
            key,
            before_version: None,
            limit: None,
            include_tombstones: true,
        }
    }

    #[must_use]
    pub const fn before_version(mut self, version: CommitVersion) -> Self {
        self.before_version = Some(version);
        self
    }

    #[must_use]
    pub const fn limit(mut self, limit: ReadLimit) -> Self {
        self.limit = Some(limit);
        self
    }

    #[must_use]
    pub const fn include_tombstones(mut self, include_tombstones: bool) -> Self {
        self.include_tombstones = include_tombstones;
        self
    }

    #[must_use]
    pub const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    #[must_use]
    pub const fn storage_space(&self) -> &StorageSpaceId {
        &self.storage_space
    }

    #[must_use]
    pub const fn key(&self) -> &StorageKey {
        &self.key
    }

    #[must_use]
    pub const fn before_version_bound(&self) -> Option<CommitVersion> {
        self.before_version
    }

    #[must_use]
    pub const fn limit_bound(&self) -> Option<ReadLimit> {
        self.limit
    }

    #[must_use]
    pub const fn includes_tombstones(&self) -> bool {
        self.include_tombstones
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrefixScanReadRequest {
    branch_id: BranchId,
    storage_space: StorageSpaceId,
    prefix: StorageKey,
    bound: ReadBound,
    limit: Option<ReadLimit>,
}

impl PrefixScanReadRequest {
    #[must_use]
    pub const fn new(
        branch_id: BranchId,
        storage_space: StorageSpaceId,
        prefix: StorageKey,
        bound: ReadBound,
        limit: Option<ReadLimit>,
    ) -> Self {
        Self {
            branch_id,
            storage_space,
            prefix,
            bound,
            limit,
        }
    }

    #[must_use]
    pub const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    #[must_use]
    pub const fn storage_space(&self) -> &StorageSpaceId {
        &self.storage_space
    }

    #[must_use]
    pub const fn prefix(&self) -> &StorageKey {
        &self.prefix
    }

    #[must_use]
    pub const fn bound(&self) -> ReadBound {
        self.bound
    }

    #[must_use]
    pub const fn limit(&self) -> Option<ReadLimit> {
        self.limit
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScanReadRequest {
    branch_id: BranchId,
    storage_space: StorageSpaceId,
    range: ScanRange,
    bound: ReadBound,
    limit: Option<ReadLimit>,
}

impl ScanReadRequest {
    #[must_use]
    pub const fn new(
        branch_id: BranchId,
        storage_space: StorageSpaceId,
        range: ScanRange,
        bound: ReadBound,
        limit: Option<ReadLimit>,
    ) -> Self {
        Self {
            branch_id,
            storage_space,
            range,
            bound,
            limit,
        }
    }

    #[must_use]
    pub const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    #[must_use]
    pub const fn storage_space(&self) -> &StorageSpaceId {
        &self.storage_space
    }

    #[must_use]
    pub const fn range(&self) -> &ScanRange {
        &self.range
    }

    #[must_use]
    pub const fn bound(&self) -> ReadBound {
        self.bound
    }

    #[must_use]
    pub const fn limit(&self) -> Option<ReadLimit> {
        self.limit
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimestampLookupRequest {
    branch_id: BranchId,
    timestamp: Timestamp,
}

impl TimestampLookupRequest {
    #[must_use]
    pub const fn new(branch_id: BranchId, timestamp: Timestamp) -> Self {
        Self {
            branch_id,
            timestamp,
        }
    }

    #[must_use]
    pub const fn branch_id(self) -> BranchId {
        self.branch_id
    }

    #[must_use]
    pub const fn timestamp(self) -> Timestamp {
        self.timestamp
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VersionLookupRequest {
    branch_id: BranchId,
    version: CommitVersion,
}

impl VersionLookupRequest {
    #[must_use]
    pub const fn new(branch_id: BranchId, version: CommitVersion) -> Self {
        Self { branch_id, version }
    }

    #[must_use]
    pub const fn branch_id(self) -> BranchId {
        self.branch_id
    }

    #[must_use]
    pub const fn version(self) -> CommitVersion {
        self.version
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimelineBoundsRequest {
    branch_id: BranchId,
}

impl TimelineBoundsRequest {
    #[must_use]
    pub const fn new(branch_id: BranchId) -> Self {
        Self { branch_id }
    }

    #[must_use]
    pub const fn branch_id(self) -> BranchId {
        self.branch_id
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageReadRow {
    storage_space: StorageSpaceId,
    key: StorageKey,
    value: Option<StorageValue>,
    commit_version: CommitVersion,
    commit_timestamp: Timestamp,
    expires_at: Option<Timestamp>,
    tombstone: bool,
}

impl StorageReadRow {
    #[must_use]
    pub const fn new(
        storage_space: StorageSpaceId,
        key: StorageKey,
        value: Option<StorageValue>,
        commit_version: CommitVersion,
        commit_timestamp: Timestamp,
        expires_at: Option<Timestamp>,
        tombstone: bool,
    ) -> Self {
        Self {
            storage_space,
            key,
            value,
            commit_version,
            commit_timestamp,
            expires_at,
            tombstone,
        }
    }

    #[must_use]
    pub const fn storage_space(&self) -> &StorageSpaceId {
        &self.storage_space
    }

    #[must_use]
    pub const fn key(&self) -> &StorageKey {
        &self.key
    }

    #[must_use]
    pub const fn value(&self) -> Option<&StorageValue> {
        self.value.as_ref()
    }

    #[must_use]
    pub const fn commit_version(&self) -> CommitVersion {
        self.commit_version
    }

    #[must_use]
    pub const fn commit_timestamp(&self) -> Timestamp {
        self.commit_timestamp
    }

    #[must_use]
    pub const fn expires_at(&self) -> Option<Timestamp> {
        self.expires_at
    }

    #[must_use]
    pub const fn is_tombstone(&self) -> bool {
        self.tombstone
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PointReadOutcome {
    row: Option<StorageReadRow>,
}

impl PointReadOutcome {
    #[must_use]
    pub const fn new(row: Option<StorageReadRow>) -> Self {
        Self { row }
    }

    #[must_use]
    pub const fn row(&self) -> Option<&StorageReadRow> {
        self.row.as_ref()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryReadOutcome {
    rows: Vec<StorageReadRow>,
}

impl HistoryReadOutcome {
    #[must_use]
    pub const fn new(rows: Vec<StorageReadRow>) -> Self {
        Self { rows }
    }

    #[must_use]
    pub fn rows(&self) -> &[StorageReadRow] {
        &self.rows
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScanReadOutcome {
    rows: Vec<StorageReadRow>,
}

impl ScanReadOutcome {
    #[must_use]
    pub const fn new(rows: Vec<StorageReadRow>) -> Self {
        Self { rows }
    }

    #[must_use]
    pub fn rows(&self) -> &[StorageReadRow] {
        &self.rows
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TimestampLookupMiss {
    AfterLatestRetained,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimestampLookupOutcome {
    query_timestamp: Timestamp,
    matched_version: CommitVersion,
    matched_timestamp: Timestamp,
    miss: Option<TimestampLookupMiss>,
}

impl TimestampLookupOutcome {
    #[must_use]
    pub const fn new(
        query_timestamp: Timestamp,
        matched_version: CommitVersion,
        matched_timestamp: Timestamp,
        miss: Option<TimestampLookupMiss>,
    ) -> Self {
        Self {
            query_timestamp,
            matched_version,
            matched_timestamp,
            miss,
        }
    }

    #[must_use]
    pub const fn query_timestamp(self) -> Timestamp {
        self.query_timestamp
    }

    #[must_use]
    pub const fn matched_version(self) -> CommitVersion {
        self.matched_version
    }

    #[must_use]
    pub const fn matched_timestamp(self) -> Timestamp {
        self.matched_timestamp
    }

    #[must_use]
    pub const fn miss(self) -> Option<TimestampLookupMiss> {
        self.miss
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct VersionLookupOutcome {
    version: CommitVersion,
    timestamp: Timestamp,
}

impl VersionLookupOutcome {
    #[must_use]
    pub const fn new(version: CommitVersion, timestamp: Timestamp) -> Self {
        Self { version, timestamp }
    }

    #[must_use]
    pub const fn version(self) -> CommitVersion {
        self.version
    }

    #[must_use]
    pub const fn timestamp(self) -> Timestamp {
        self.timestamp
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimelineBoundsOutcome {
    min_timestamp: Option<Timestamp>,
    max_timestamp: Option<Timestamp>,
    min_version: Option<CommitVersion>,
    max_version: Option<CommitVersion>,
}

impl TimelineBoundsOutcome {
    #[must_use]
    pub const fn new(
        min_timestamp: Option<Timestamp>,
        max_timestamp: Option<Timestamp>,
        min_version: Option<CommitVersion>,
        max_version: Option<CommitVersion>,
    ) -> Self {
        Self {
            min_timestamp,
            max_timestamp,
            min_version,
            max_version,
        }
    }

    #[must_use]
    pub const fn min_timestamp(self) -> Option<Timestamp> {
        self.min_timestamp
    }

    #[must_use]
    pub const fn max_timestamp(self) -> Option<Timestamp> {
        self.max_timestamp
    }

    #[must_use]
    pub const fn min_version(self) -> Option<CommitVersion> {
        self.min_version
    }

    #[must_use]
    pub const fn max_version(self) -> Option<CommitVersion> {
        self.max_version
    }
}
