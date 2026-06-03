//! Append-only mutation helpers for branch-local mutable rows.

use super::BranchLocalState;
use crate::branch::error::{BranchRuntimeError, BranchRuntimeResult};
use crate::branch::identity::require_row_branch;
use crate::observability::perf_trace;
use crate::row::StorageRow;
use crate::table::TableInternalKeyBytes;
use strata_core_next::{BranchId, CommitVersion, Timestamp};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchAppendOutcome {
    branch_id: BranchId,
    commit_version: CommitVersion,
    commit_timestamp: Timestamp,
    is_tombstone: bool,
    active_rows: usize,
    approximate_active_bytes: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchAppendBatchOutcome {
    branch_id: BranchId,
    appended_rows: usize,
    active_rows: usize,
    approximate_active_bytes: usize,
    max_commit_version: Option<CommitVersion>,
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

impl BranchAppendBatchOutcome {
    pub(crate) const fn branch_id(self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn appended_rows(self) -> usize {
        self.appended_rows
    }

    pub(crate) const fn active_rows(self) -> usize {
        self.active_rows
    }

    pub(crate) const fn approximate_active_bytes(self) -> usize {
        self.approximate_active_bytes
    }

    pub(crate) const fn max_commit_version(self) -> Option<CommitVersion> {
        self.max_commit_version
    }
}

impl BranchLocalState {
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

    pub(crate) fn append_committed_rows_atomically(
        &mut self,
        rows: impl IntoIterator<Item = StorageRow>,
    ) -> BranchRuntimeResult<BranchAppendBatchOutcome> {
        let rows = rows.into_iter().collect::<Vec<_>>();
        if rows.is_empty() {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "committed row batch must not be empty",
            });
        }

        perf_trace::record_append_staging_clone(branch_row_count(self));
        let mut staged = self.clone();
        for row in rows {
            staged.append_committed_row(row)?;
        }

        let outcome = BranchAppendBatchOutcome {
            branch_id: staged.branch_id,
            appended_rows: staged.active.len().checked_sub(self.active.len()).ok_or(
                BranchRuntimeError::InvalidBranchState {
                    reason: "staged active row count regressed",
                },
            )?,
            active_rows: staged.active.len(),
            approximate_active_bytes: staged.active.approximate_size_bytes(),
            max_commit_version: staged.max_commit_version,
        };
        *self = staged;
        Ok(outcome)
    }
}

fn branch_row_count(state: &BranchLocalState) -> usize {
    state
        .active
        .len()
        .saturating_add(state.frozen.iter().map(|table| table.len()).sum::<usize>())
        .saturating_add(
            state
                .owned_levels
                .iter()
                .flatten()
                .map(|table| table.rows().len())
                .sum::<usize>(),
        )
        .saturating_add(
            state
                .inherited_layers
                .iter()
                .flat_map(|layer| layer.owned_levels().iter().flatten())
                .map(|table| table.rows().len())
                .sum::<usize>(),
        )
}
