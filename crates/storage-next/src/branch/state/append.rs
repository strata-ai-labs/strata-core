//! Append-only mutation helpers for branch-local mutable rows.

use super::BranchLocalState;
use crate::branch::error::{BranchRuntimeError, BranchRuntimeResult};
use crate::branch::identity::require_row_branch;
use crate::observability::perf_trace;
use crate::row::StorageRow;
use crate::table::{MutableTableAppendBaseline, TableInternalKeyBytes, TableRuntimeError};
use std::collections::BTreeSet;
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BranchAppendMetadataSnapshot {
    max_commit_version: Option<CommitVersion>,
    timestamp_min: Option<Timestamp>,
    timestamp_max: Option<Timestamp>,
    put_rows: u64,
    tombstone_rows: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ValidatedAppendRow {
    key: TableInternalKeyBytes,
    commit_version: CommitVersion,
    commit_timestamp: Timestamp,
    is_tombstone: bool,
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
        let validate_timer = perf_trace::start_timer();
        let validated_rows = self.validate_committed_row_batch(&rows);
        perf_trace::record_append_batch_validate_elapsed(validate_timer);
        let validated_rows = validated_rows?;
        let active_rows_before = self.active.len();
        let active_snapshot = self.active.append_baseline();
        let metadata_snapshot = BranchAppendMetadataSnapshot::capture(self);
        let mut inserted_keys = Vec::with_capacity(validated_rows.len());

        // Atomicity is preserved by validating every fallible branch/table
        // condition before mutation. The rollback guard below handles any
        // unexpected table insertion rejection without cloning the full branch.
        let insert_timer = perf_trace::start_timer();
        for (row, validated) in rows.into_iter().zip(validated_rows.iter()) {
            if let Err(source) = self.active.insert_row(row) {
                perf_trace::record_append_insert_rows_elapsed(insert_timer);
                rollback_direct_append(self, active_snapshot, metadata_snapshot, &inserted_keys);
                return Err(BranchRuntimeError::TableRuntime { source });
            }
            inserted_keys.push(validated.key.clone());
            self.track_committed_row(
                validated.commit_version,
                validated.commit_timestamp,
                validated.is_tombstone,
            );
        }
        perf_trace::record_append_insert_rows_elapsed(insert_timer);

        perf_trace::record_append_rows_applied(inserted_keys.len());
        let outcome = BranchAppendBatchOutcome {
            branch_id: self.branch_id,
            appended_rows: self.active.len().checked_sub(active_rows_before).ok_or(
                BranchRuntimeError::InvalidBranchState {
                    reason: "active row count regressed during append",
                },
            )?,
            active_rows: self.active.len(),
            approximate_active_bytes: self.active.approximate_size_bytes(),
            max_commit_version: self.max_commit_version,
        };
        Ok(outcome)
    }

    fn validate_committed_row_batch(
        &self,
        rows: &[StorageRow],
    ) -> BranchRuntimeResult<Vec<ValidatedAppendRow>> {
        if rows.is_empty() {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "committed row batch must not be empty",
            });
        }

        let mut seen = BTreeSet::new();
        let mut validated = Vec::with_capacity(rows.len());
        for row in rows {
            let identity = require_row_branch(self.branch_id, row)?;
            let key = TableInternalKeyBytes::from_row(row);
            if !seen.insert(key.clone()) {
                return Err(duplicate_internal_key_error(&key));
            }
            validated.push(ValidatedAppendRow {
                key,
                commit_version: identity.commit_version(),
                commit_timestamp: identity.commit_timestamp(),
                is_tombstone: row.is_tombstone(),
            });
        }
        Ok(validated)
    }
}

impl BranchAppendMetadataSnapshot {
    const fn capture(state: &BranchLocalState) -> Self {
        Self {
            max_commit_version: state.max_commit_version,
            timestamp_min: state.timestamp_min,
            timestamp_max: state.timestamp_max,
            put_rows: state.put_rows,
            tombstone_rows: state.tombstone_rows,
        }
    }

    fn restore(self, state: &mut BranchLocalState) {
        state.max_commit_version = self.max_commit_version;
        state.timestamp_min = self.timestamp_min;
        state.timestamp_max = self.timestamp_max;
        state.put_rows = self.put_rows;
        state.tombstone_rows = self.tombstone_rows;
    }
}

fn rollback_direct_append(
    state: &mut BranchLocalState,
    active_snapshot: MutableTableAppendBaseline,
    metadata_snapshot: BranchAppendMetadataSnapshot,
    inserted_keys: &[TableInternalKeyBytes],
) {
    state
        .active
        .rollback_appended_rows(active_snapshot, inserted_keys);
    metadata_snapshot.restore(state);
}

fn duplicate_internal_key_error(key: &TableInternalKeyBytes) -> BranchRuntimeError {
    BranchRuntimeError::TableRuntime {
        source: TableRuntimeError::DuplicateInternalKey {
            key: key.as_slice().to_vec(),
        },
    }
}
