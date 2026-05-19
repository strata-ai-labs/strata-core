//! Branch facts and descriptor shells.

use super::{BranchRuntimeError, BranchRuntimeResult};
use crate::table::{TableIdentity, TableRuntimeFacts};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub(crate) struct BranchLevel(u8);

impl BranchLevel {
    pub(crate) const ZERO: Self = Self(0);

    pub(crate) const fn new(raw: u8) -> Self {
        Self(raw)
    }

    pub(crate) const fn raw(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchStateFacts {
    branch_id: BranchId,
    active_rows: u64,
    frozen_table_count: usize,
    owned_table_count: usize,
    inherited_layer_count: usize,
    max_commit_version: Option<CommitVersion>,
    timestamp_min: Option<Timestamp>,
    timestamp_max: Option<Timestamp>,
}

impl BranchStateFacts {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        branch_id: BranchId,
        active_rows: u64,
        frozen_table_count: usize,
        owned_table_count: usize,
        inherited_layer_count: usize,
        max_commit_version: Option<CommitVersion>,
        timestamp_min: Option<Timestamp>,
        timestamp_max: Option<Timestamp>,
    ) -> BranchRuntimeResult<Self> {
        match (timestamp_min, timestamp_max) {
            (Some(min), Some(max)) if min > max => {
                return Err(BranchRuntimeError::InvalidBranchState {
                    reason: "timestamp_min must not exceed timestamp_max",
                });
            }
            (Some(_), None) | (None, Some(_)) => {
                return Err(BranchRuntimeError::InvalidBranchState {
                    reason: "timestamp_min and timestamp_max must both be present or absent",
                });
            }
            (Some(_), Some(_)) | (None, None) => {}
        }

        if active_rows == 0
            && frozen_table_count == 0
            && owned_table_count == 0
            && inherited_layer_count == 0
        {
            if max_commit_version.is_some() {
                return Err(BranchRuntimeError::InvalidBranchState {
                    reason: "empty branch facts cannot report a max commit version",
                });
            }
            if timestamp_min.is_some() || timestamp_max.is_some() {
                return Err(BranchRuntimeError::InvalidBranchState {
                    reason: "empty branch facts cannot report a timestamp range",
                });
            }
        }

        Ok(Self {
            branch_id,
            active_rows,
            frozen_table_count,
            owned_table_count,
            inherited_layer_count,
            max_commit_version,
            timestamp_min,
            timestamp_max,
        })
    }

    pub(crate) fn empty(branch_id: BranchId) -> Self {
        Self::new(branch_id, 0, 0, 0, 0, None, None, None).expect("empty branch facts are valid")
    }

    pub(crate) const fn branch_id(self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn active_rows(self) -> u64 {
        self.active_rows
    }

    pub(crate) const fn frozen_table_count(self) -> usize {
        self.frozen_table_count
    }

    pub(crate) const fn owned_table_count(self) -> usize {
        self.owned_table_count
    }

    pub(crate) const fn inherited_layer_count(self) -> usize {
        self.inherited_layer_count
    }

    pub(crate) const fn max_commit_version(self) -> Option<CommitVersion> {
        self.max_commit_version
    }

    pub(crate) const fn timestamp_min(self) -> Option<Timestamp> {
        self.timestamp_min
    }

    pub(crate) const fn timestamp_max(self) -> Option<Timestamp> {
        self.timestamp_max
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchTableDescriptor {
    identity: TableIdentity,
    facts: TableRuntimeFacts,
    level: BranchLevel,
}

impl BranchTableDescriptor {
    pub(crate) fn new(
        identity: TableIdentity,
        facts: TableRuntimeFacts,
        level: BranchLevel,
    ) -> BranchRuntimeResult<Self> {
        if facts.identity() != &identity {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "table descriptor identity must match table facts identity",
            });
        }
        Ok(Self {
            identity,
            facts,
            level,
        })
    }

    pub(crate) const fn identity(&self) -> &TableIdentity {
        &self.identity
    }

    pub(crate) const fn facts(&self) -> &TableRuntimeFacts {
        &self.facts
    }

    pub(crate) const fn level(&self) -> BranchLevel {
        self.level
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InheritedLayerStatus {
    Active,
    Materializing,
    Materialized,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InheritedLayerDescriptor {
    source_branch_id: BranchId,
    fork_version: CommitVersion,
    status: InheritedLayerStatus,
    table_count: usize,
}

impl InheritedLayerDescriptor {
    pub(crate) const fn new(
        source_branch_id: BranchId,
        fork_version: CommitVersion,
        status: InheritedLayerStatus,
        table_count: usize,
    ) -> Self {
        Self {
            source_branch_id,
            fork_version,
            status,
            table_count,
        }
    }

    pub(crate) const fn source_branch_id(self) -> BranchId {
        self.source_branch_id
    }

    pub(crate) const fn fork_version(self) -> CommitVersion {
        self.fork_version
    }

    pub(crate) const fn status(self) -> InheritedLayerStatus {
        self.status
    }

    pub(crate) const fn table_count(self) -> usize {
        self.table_count
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchReachabilityFacts {
    branch_id: BranchId,
    owned_table_count: usize,
    inherited_table_count: usize,
}

impl BranchReachabilityFacts {
    pub(crate) const fn new(
        branch_id: BranchId,
        owned_table_count: usize,
        inherited_table_count: usize,
    ) -> Self {
        Self {
            branch_id,
            owned_table_count,
            inherited_table_count,
        }
    }

    pub(crate) const fn branch_id(self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn owned_table_count(self) -> usize {
        self.owned_table_count
    }

    pub(crate) const fn inherited_table_count(self) -> usize {
        self.inherited_table_count
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct BranchRuntimeStats {
    latest_reads: u64,
    bounded_reads: u64,
    history_reads: u64,
    inherited_layers_examined: u64,
}

impl BranchRuntimeStats {
    pub(crate) const fn new(
        latest_reads: u64,
        bounded_reads: u64,
        history_reads: u64,
        inherited_layers_examined: u64,
    ) -> Self {
        Self {
            latest_reads,
            bounded_reads,
            history_reads,
            inherited_layers_examined,
        }
    }

    pub(crate) const fn latest_reads(self) -> u64 {
        self.latest_reads
    }

    pub(crate) const fn bounded_reads(self) -> u64 {
        self.bounded_reads
    }

    pub(crate) const fn history_reads(self) -> u64 {
        self.history_reads
    }

    pub(crate) const fn inherited_layers_examined(self) -> u64 {
        self.inherited_layers_examined
    }
}
