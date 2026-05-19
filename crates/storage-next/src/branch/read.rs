//! Branch read-bound and selected-row vocabulary.

use super::BranchLevel;
use crate::row::{PhysicalKey, StorageRow};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BranchReadBound {
    Latest,
    AtVersion(CommitVersion),
    AtTimestamp(Timestamp),
}

impl BranchReadBound {
    pub(crate) const fn latest() -> Self {
        Self::Latest
    }

    pub(crate) const fn at_version(version: CommitVersion) -> Self {
        Self::AtVersion(version)
    }

    pub(crate) const fn at_timestamp(timestamp: Timestamp) -> Self {
        Self::AtTimestamp(timestamp)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchEffectiveReadBound {
    max_commit_version: Option<CommitVersion>,
    max_commit_timestamp: Option<Timestamp>,
}

impl BranchEffectiveReadBound {
    pub(crate) const fn new(
        max_commit_version: Option<CommitVersion>,
        max_commit_timestamp: Option<Timestamp>,
    ) -> Self {
        Self {
            max_commit_version,
            max_commit_timestamp,
        }
    }

    pub(crate) const fn for_own_branch(bound: BranchReadBound) -> Self {
        match bound {
            BranchReadBound::Latest => Self::new(None, None),
            BranchReadBound::AtVersion(version) => Self::new(Some(version), None),
            BranchReadBound::AtTimestamp(timestamp) => Self::new(None, Some(timestamp)),
        }
    }

    pub(crate) const fn for_inherited_layer(
        bound: BranchReadBound,
        fork_version: CommitVersion,
    ) -> Self {
        match bound {
            BranchReadBound::Latest => Self::new(Some(fork_version), None),
            BranchReadBound::AtVersion(version) => {
                let effective_version = if version.as_u64() <= fork_version.as_u64() {
                    version
                } else {
                    fork_version
                };
                Self::new(Some(effective_version), None)
            }
            BranchReadBound::AtTimestamp(timestamp) => {
                Self::new(Some(fork_version), Some(timestamp))
            }
        }
    }

    pub(crate) const fn max_commit_version(self) -> Option<CommitVersion> {
        self.max_commit_version
    }

    pub(crate) const fn max_commit_timestamp(self) -> Option<Timestamp> {
        self.max_commit_timestamp
    }

    pub(crate) const fn matches_row(self, row: &StorageRow) -> BranchRowBoundMatch {
        let version_in_bound = match self.max_commit_version {
            Some(version) => row.commit_version().as_u64() <= version.as_u64(),
            None => true,
        };
        let timestamp_in_bound = match self.max_commit_timestamp {
            Some(timestamp) => row.commit_timestamp().as_micros() <= timestamp.as_micros(),
            None => true,
        };
        BranchRowBoundMatch::new(version_in_bound, timestamp_in_bound)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BranchRowSource {
    Active,
    Frozen {
        index: usize,
    },
    OwnedTable {
        level: BranchLevel,
        table_index: usize,
    },
    Inherited {
        source_branch_id: BranchId,
        layer_index: usize,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchRowBoundMatch {
    version_in_bound: bool,
    timestamp_in_bound: bool,
}

impl BranchRowBoundMatch {
    pub(crate) const fn new(version_in_bound: bool, timestamp_in_bound: bool) -> Self {
        Self {
            version_in_bound,
            timestamp_in_bound,
        }
    }

    pub(crate) const fn version_in_bound(self) -> bool {
        self.version_in_bound
    }

    pub(crate) const fn timestamp_in_bound(self) -> bool {
        self.timestamp_in_bound
    }

    pub(crate) const fn matches_effective_bound(self) -> bool {
        self.version_in_bound && self.timestamp_in_bound
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchRowCandidateFacts {
    source: BranchRowSource,
    physical_key: PhysicalKey,
    commit_version: CommitVersion,
    commit_timestamp: Timestamp,
    expires_at: Timestamp,
    is_tombstone: bool,
    bound_match: BranchRowBoundMatch,
}

impl BranchRowCandidateFacts {
    pub(crate) fn from_row(
        row: &StorageRow,
        source: BranchRowSource,
        effective_bound: BranchEffectiveReadBound,
    ) -> Self {
        Self {
            source,
            physical_key: row.physical_key().clone(),
            commit_version: row.commit_version(),
            commit_timestamp: row.commit_timestamp(),
            expires_at: row.expires_at(),
            is_tombstone: row.is_tombstone(),
            bound_match: effective_bound.matches_row(row),
        }
    }

    pub(crate) const fn source(&self) -> BranchRowSource {
        self.source
    }

    pub(crate) const fn physical_key(&self) -> &PhysicalKey {
        &self.physical_key
    }

    pub(crate) const fn commit_version(&self) -> CommitVersion {
        self.commit_version
    }

    pub(crate) const fn commit_timestamp(&self) -> Timestamp {
        self.commit_timestamp
    }

    pub(crate) const fn expires_at(&self) -> Timestamp {
        self.expires_at
    }

    pub(crate) const fn is_tombstone(&self) -> bool {
        self.is_tombstone
    }

    pub(crate) const fn bound_match(&self) -> BranchRowBoundMatch {
        self.bound_match
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchVisibleRow {
    row: StorageRow,
    source: BranchRowSource,
}

impl BranchVisibleRow {
    pub(crate) fn new(row: StorageRow, source: BranchRowSource) -> Self {
        Self { row, source }
    }

    pub(crate) const fn row(&self) -> &StorageRow {
        &self.row
    }

    pub(crate) const fn source(&self) -> BranchRowSource {
        self.source
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchHistoryRow {
    row: StorageRow,
    source: BranchRowSource,
}

impl BranchHistoryRow {
    pub(crate) fn new(row: StorageRow, source: BranchRowSource) -> Self {
        Self { row, source }
    }

    pub(crate) const fn row(&self) -> &StorageRow {
        &self.row
    }

    pub(crate) const fn source(&self) -> BranchRowSource {
        self.source
    }
}
