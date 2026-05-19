//! Branch-local state and descriptor shells.

use super::{
    require_row_branch, BranchRuntimeConfig, BranchRuntimeError, BranchRuntimeResult,
    BranchStateFacts,
};
use crate::row::StorageRow;
use crate::table::{FrozenTable, MutableTable, TableInternalKeyBytes, TableRuntimeError};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchStateDescriptor {
    branch_id: BranchId,
    facts: BranchStateFacts,
}

impl BranchStateDescriptor {
    pub(crate) fn new(branch_id: BranchId, facts: BranchStateFacts) -> BranchRuntimeResult<Self> {
        if branch_id != facts.branch_id() {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "state descriptor branch id must match branch facts",
            });
        }
        Ok(Self { branch_id, facts })
    }

    pub(crate) const fn branch_id(self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn facts(self) -> BranchStateFacts {
        self.facts
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchViewDescriptor {
    branch_id: BranchId,
    facts: BranchStateFacts,
}

impl BranchViewDescriptor {
    pub(crate) fn new(branch_id: BranchId, facts: BranchStateFacts) -> BranchRuntimeResult<Self> {
        if branch_id != facts.branch_id() {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "view descriptor branch id must match branch facts",
            });
        }
        Ok(Self { branch_id, facts })
    }

    pub(crate) const fn branch_id(self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn facts(self) -> BranchStateFacts {
        self.facts
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchAppendOutcome {
    branch_id: BranchId,
    commit_version: CommitVersion,
    commit_timestamp: Timestamp,
    is_tombstone: bool,
    active_rows: usize,
    approximate_active_bytes: usize,
}

impl BranchAppendOutcome {
    pub(crate) const fn branch_id(self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn commit_version(self) -> CommitVersion {
        self.commit_version
    }

    pub(crate) const fn commit_timestamp(self) -> Timestamp {
        self.commit_timestamp
    }

    pub(crate) const fn is_tombstone(self) -> bool {
        self.is_tombstone
    }

    pub(crate) const fn active_rows(self) -> usize {
        self.active_rows
    }

    pub(crate) const fn approximate_active_bytes(self) -> usize {
        self.approximate_active_bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BranchRotationSkipReason {
    EmptyActive,
    FrozenLimitReached,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BranchRotationOutcome {
    Rotated {
        frozen_index: usize,
        frozen_rows: usize,
        frozen_tables: usize,
    },
    Skipped {
        reason: BranchRotationSkipReason,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchLocalState {
    branch_id: BranchId,
    config: BranchRuntimeConfig,
    active: MutableTable,
    frozen: Vec<FrozenTable>,
    max_commit_version: Option<CommitVersion>,
    timestamp_min: Option<Timestamp>,
    timestamp_max: Option<Timestamp>,
    put_rows: u64,
    tombstone_rows: u64,
}

impl BranchLocalState {
    pub(crate) fn new(
        branch_id: BranchId,
        config: BranchRuntimeConfig,
    ) -> BranchRuntimeResult<Self> {
        config.validate()?;
        Ok(Self {
            branch_id,
            config,
            active: MutableTable::new(),
            frozen: Vec::new(),
            max_commit_version: None,
            timestamp_min: None,
            timestamp_max: None,
            put_rows: 0,
            tombstone_rows: 0,
        })
    }

    pub(crate) fn empty(branch_id: BranchId) -> Self {
        Self::new(branch_id, BranchRuntimeConfig::default())
            .expect("default branch-local state configuration is valid")
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn config(&self) -> BranchRuntimeConfig {
        self.config
    }

    pub(crate) const fn active(&self) -> &MutableTable {
        &self.active
    }

    pub(crate) fn frozen(&self) -> &[FrozenTable] {
        &self.frozen
    }

    pub(crate) fn active_row_count(&self) -> usize {
        self.active.len()
    }

    pub(crate) fn frozen_table_count(&self) -> usize {
        self.frozen.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.active.is_empty() && self.frozen.is_empty()
    }

    pub(crate) const fn max_commit_version(&self) -> Option<CommitVersion> {
        self.max_commit_version
    }

    pub(crate) const fn timestamp_min(&self) -> Option<Timestamp> {
        self.timestamp_min
    }

    pub(crate) const fn timestamp_max(&self) -> Option<Timestamp> {
        self.timestamp_max
    }

    pub(crate) const fn put_rows(&self) -> u64 {
        self.put_rows
    }

    pub(crate) const fn tombstone_rows(&self) -> u64 {
        self.tombstone_rows
    }

    pub(crate) fn facts(&self) -> BranchRuntimeResult<BranchStateFacts> {
        BranchStateFacts::new(
            self.branch_id,
            u64::try_from(self.active.len()).expect("active row count fits in u64"),
            self.frozen.len(),
            0,
            0,
            self.max_commit_version,
            self.timestamp_min,
            self.timestamp_max,
        )
    }

    pub(crate) fn append_committed_row(
        &mut self,
        row: StorageRow,
    ) -> BranchRuntimeResult<BranchAppendOutcome> {
        let identity = require_row_branch(self.branch_id, &row)?;
        let key = TableInternalKeyBytes::from_row(&row);
        self.require_absent_internal_key(&key)?;

        let commit_version = identity.commit_version();
        let commit_timestamp = identity.commit_timestamp();
        let is_tombstone = row.is_tombstone();
        self.active
            .insert_row(row)
            .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
        self.track_committed_row(commit_version, commit_timestamp, is_tombstone);

        Ok(BranchAppendOutcome {
            branch_id: self.branch_id,
            commit_version,
            commit_timestamp,
            is_tombstone,
            active_rows: self.active.len(),
            approximate_active_bytes: self.active.approximate_size_bytes(),
        })
    }

    pub(crate) fn rotate_active(&mut self) -> BranchRotationOutcome {
        if self.active.is_empty() {
            return BranchRotationOutcome::Skipped {
                reason: BranchRotationSkipReason::EmptyActive,
            };
        }

        if self.frozen.len() >= self.config.max_frozen_tables() {
            return BranchRotationOutcome::Skipped {
                reason: BranchRotationSkipReason::FrozenLimitReached,
            };
        }

        let active = std::mem::replace(&mut self.active, MutableTable::new());
        let frozen_rows = active.len();
        self.frozen.insert(0, active.freeze());
        BranchRotationOutcome::Rotated {
            frozen_index: 0,
            frozen_rows,
            frozen_tables: self.frozen.len(),
        }
    }

    fn require_absent_internal_key(&self, key: &TableInternalKeyBytes) -> BranchRuntimeResult<()> {
        if self.active.get(key).is_some()
            || self.frozen.iter().any(|table| table.get(key).is_some())
        {
            return Err(BranchRuntimeError::TableRuntime {
                source: TableRuntimeError::DuplicateInternalKey {
                    key: key.as_slice().to_vec(),
                },
            });
        }
        Ok(())
    }

    fn track_committed_row(
        &mut self,
        commit_version: CommitVersion,
        commit_timestamp: Timestamp,
        is_tombstone: bool,
    ) {
        self.max_commit_version = Some(
            self.max_commit_version
                .map_or(commit_version, |max| max.max(commit_version)),
        );
        self.timestamp_min = Some(
            self.timestamp_min
                .map_or(commit_timestamp, |min| min.min(commit_timestamp)),
        );
        self.timestamp_max = Some(
            self.timestamp_max
                .map_or(commit_timestamp, |max| max.max(commit_timestamp)),
        );
        if is_tombstone {
            self.tombstone_rows = self.tombstone_rows.saturating_add(1);
        } else {
            self.put_rows = self.put_rows.saturating_add(1);
        }
    }
}
