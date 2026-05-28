//! API read request shells.

use strata_core_next::{BranchId, CommitVersion, Timestamp};

use super::{ReadLimit, ScanRange, StorageKey, StorageSpaceId};

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
