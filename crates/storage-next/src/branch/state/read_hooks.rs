//! Read-facing facts, descriptors, and view capture for branch-local state.

use super::BranchLocalState;
use crate::branch::config::BranchRuntimeConfig;
use crate::branch::error::{BranchRuntimeError, BranchRuntimeResult};
use crate::branch::facts::{BranchSourceLayout, BranchStateFacts, InheritedLayerStatus};
use crate::branch::read::{
    inherited_table_count, source_layout_from_sources, BranchInheritedLayer, BranchOwnedTable,
    BranchReadView, BranchTimestampCoverage,
};
use crate::observability::perf_trace;
use crate::table::{FrozenTable, MutableTable};
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

impl BranchLocalState {
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

    pub(crate) fn owned_levels(&self) -> &[Vec<BranchOwnedTable>] {
        &self.owned_levels
    }

    pub(crate) fn inherited_layers(&self) -> &[BranchInheritedLayer] {
        &self.inherited_layers
    }

    pub(crate) fn active_row_count(&self) -> usize {
        self.active.len()
    }

    pub(crate) fn frozen_table_count(&self) -> usize {
        self.frozen.len()
    }

    pub(crate) fn owned_table_count(&self) -> usize {
        self.owned_levels.iter().map(Vec::len).sum()
    }

    pub(crate) fn inherited_layer_count(&self) -> usize {
        self.inherited_layers.len()
    }

    pub(crate) fn inherited_table_count(&self) -> usize {
        inherited_table_count(&self.inherited_layers)
    }

    pub(crate) fn source_layout(&self) -> BranchSourceLayout {
        source_layout_from_sources(
            &self.active,
            &self.frozen,
            &self.owned_levels,
            &self.inherited_layers,
        )
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.active.is_empty()
            && self.frozen.is_empty()
            && self.owned_table_count() == 0
            && self.inherited_layers.is_empty()
    }

    pub(crate) const fn max_commit_version(&self) -> Option<CommitVersion> {
        self.max_commit_version
    }

    /// Map a wall-clock timestamp to the highest commit version whose row was
    /// stamped at or before `timestamp`. Walks active, frozen, owned, and
    /// inherited rows (inherited rows are bounded by their layer's fork
    /// version). Returns `None` when no row qualifies.
    ///
    /// Tiebreaker: among rows with identical timestamps, the largest commit
    /// version wins - consistent with the rest of the storage-next timeline
    /// rule.
    pub(crate) fn resolve_timestamp_to_commit_version(
        &self,
        timestamp: Timestamp,
    ) -> Option<CommitVersion> {
        perf_trace::record_branch_timestamp_rows(source_row_counts(
            &self.active,
            &self.frozen,
            &self.owned_levels,
            &self.inherited_layers,
            |_| true,
        ));
        let mut best: Option<CommitVersion> = None;
        let mut consider = |version: CommitVersion| {
            best = match best {
                Some(current) if current.as_u64() >= version.as_u64() => Some(current),
                _ => Some(version),
            };
        };
        for row in self.active.iter() {
            if row.row().commit_timestamp() <= timestamp {
                consider(row.row().commit_version());
            }
        }
        for table in &self.frozen {
            for row in table.iter() {
                if row.row().commit_timestamp() <= timestamp {
                    consider(row.row().commit_version());
                }
            }
        }
        for tables in &self.owned_levels {
            for table in tables {
                for row in table.rows() {
                    if row.row().commit_timestamp() <= timestamp {
                        consider(row.row().commit_version());
                    }
                }
            }
        }
        for layer in &self.inherited_layers {
            let fork_version = layer.fork_version();
            for tables in layer.owned_levels() {
                for table in tables {
                    for row in table.rows() {
                        if row.row().commit_timestamp() <= timestamp
                            && row.row().commit_version().as_u64() <= fork_version.as_u64()
                        {
                            consider(row.row().commit_version());
                        }
                    }
                }
            }
        }
        best
    }

    pub(crate) const fn timestamp_min(&self) -> Option<Timestamp> {
        self.timestamp_min
    }

    pub(crate) const fn timestamp_max(&self) -> Option<Timestamp> {
        self.timestamp_max
    }

    pub(crate) const fn timestamp_coverage(&self) -> BranchTimestampCoverage {
        self.timestamp_coverage
    }

    pub(crate) fn set_timestamp_coverage(&mut self, coverage: BranchTimestampCoverage) {
        self.timestamp_coverage = coverage;
    }

    pub(crate) const fn put_rows(&self) -> u64 {
        self.put_rows
    }

    pub(crate) const fn tombstone_rows(&self) -> u64 {
        self.tombstone_rows
    }

    pub(crate) fn facts(&self) -> BranchRuntimeResult<BranchStateFacts> {
        perf_trace::record_branch_facts_observed(read_view_clone_row_count(self));
        perf_trace::record_branch_fact_source_rows(source_row_counts(
            &self.active,
            &self.frozen,
            &self.owned_levels,
            &self.inherited_layers,
            |layer| {
                matches!(
                    layer.status(),
                    InheritedLayerStatus::Active | InheritedLayerStatus::Materializing
                )
            },
        ));
        let observed = self.observe_rows();
        BranchStateFacts::new(
            self.branch_id,
            u64::try_from(self.active.len()).expect("active row count fits in u64"),
            self.frozen.len(),
            self.owned_table_count(),
            self.inherited_layers.len(),
            observed.max_commit_version,
            observed.timestamp_min,
            observed.timestamp_max,
        )
    }

    pub(crate) fn capture_read_view(&self) -> BranchRuntimeResult<BranchReadView> {
        perf_trace::record_read_view_capture(read_view_clone_row_count(self));
        BranchReadView::new_with_inherited(
            self.branch_id,
            self.active.clone(),
            self.frozen.clone(),
            self.owned_levels.clone(),
            self.inherited_layers.clone(),
            self.facts()?,
        )
        .map(|view| view.with_timestamp_coverage(self.timestamp_coverage))
    }
}

fn read_view_clone_row_count(state: &BranchLocalState) -> usize {
    state
        .active
        .len()
        .saturating_add(state.frozen.iter().map(FrozenTable::len).sum::<usize>())
        .saturating_add(owned_levels_row_count(&state.owned_levels))
        .saturating_add(inherited_layers_row_count(&state.inherited_layers))
}

fn source_row_counts(
    active: &MutableTable,
    frozen: &[FrozenTable],
    owned_levels: &[Vec<BranchOwnedTable>],
    inherited_layers: &[BranchInheritedLayer],
    include_inherited_layer: impl Fn(&BranchInheritedLayer) -> bool,
) -> perf_trace::BranchSourceRowCounts {
    let mut counts = perf_trace::BranchSourceRowCounts {
        active: active.len(),
        frozen: frozen.iter().map(FrozenTable::len).sum(),
        ..Default::default()
    };
    for (level_index, tables) in owned_levels.iter().enumerate() {
        for table in tables {
            if level_index == 0 {
                counts.owned_l0 = counts.owned_l0.saturating_add(table.rows().len());
            } else {
                counts.owned_nonzero = counts.owned_nonzero.saturating_add(table.rows().len());
            }
        }
    }
    for layer in inherited_layers {
        if !include_inherited_layer(layer) {
            continue;
        }
        for (level_index, tables) in layer.owned_levels().iter().enumerate() {
            for table in tables {
                if level_index == 0 {
                    counts.inherited_l0 = counts.inherited_l0.saturating_add(table.rows().len());
                } else {
                    counts.inherited_nonzero =
                        counts.inherited_nonzero.saturating_add(table.rows().len());
                }
            }
        }
    }
    counts
}

fn inherited_layers_row_count(layers: &[BranchInheritedLayer]) -> usize {
    layers
        .iter()
        .map(|layer| owned_levels_row_count(layer.owned_levels()))
        .sum()
}

fn owned_levels_row_count(levels: &[Vec<BranchOwnedTable>]) -> usize {
    levels
        .iter()
        .flatten()
        .map(|table| table.rows().len())
        .sum()
}
