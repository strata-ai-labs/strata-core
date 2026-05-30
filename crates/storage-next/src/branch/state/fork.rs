//! Fork and inherited-layer attachment helpers for branch-local state.

use super::BranchLocalState;
use crate::branch::error::{BranchRuntimeError, BranchRuntimeResult};
use crate::branch::facts::{InheritedLayerDescriptor, InheritedLayerStatus};
use crate::branch::read::{inherited_table_count, BranchInheritedLayer};
use std::collections::BTreeSet;
use strata_core_next::{BranchId, CommitVersion};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchForkOutcome {
    source_branch_id: BranchId,
    destination_branch_id: BranchId,
    fork_version: CommitVersion,
    inherited_layer_count: usize,
    inherited_table_count: usize,
}

impl BranchForkOutcome {
    pub(crate) const fn source_branch_id(self) -> BranchId {
        self.source_branch_id
    }

    pub(crate) const fn destination_branch_id(self) -> BranchId {
        self.destination_branch_id
    }

    pub(crate) const fn fork_version(self) -> CommitVersion {
        self.fork_version
    }

    pub(crate) const fn inherited_layer_count(self) -> usize {
        self.inherited_layer_count
    }

    pub(crate) const fn inherited_table_count(self) -> usize {
        self.inherited_table_count
    }
}

impl BranchLocalState {
    pub(crate) fn attach_inherited_layers(
        &mut self,
        layers: Vec<BranchInheritedLayer>,
    ) -> BranchRuntimeResult<BranchForkOutcome> {
        self.validate_inherited_attach(&layers)?;
        let inherited_layer_count = layers.len();
        let inherited_table_count = inherited_table_count(&layers);
        self.inherited_layers = layers;
        self.refresh_observed_row_facts();
        Ok(BranchForkOutcome {
            source_branch_id: self
                .inherited_layers
                .first()
                .map_or(self.branch_id, BranchInheritedLayer::source_branch_id),
            destination_branch_id: self.branch_id,
            fork_version: self
                .inherited_layers
                .first()
                .map_or(CommitVersion::ZERO, BranchInheritedLayer::fork_version),
            inherited_layer_count,
            inherited_table_count,
        })
    }

    pub(crate) fn fork_into_empty_child(
        &self,
        destination_branch_id: BranchId,
    ) -> BranchRuntimeResult<(Self, BranchForkOutcome)> {
        if destination_branch_id == self.branch_id {
            return Err(BranchRuntimeError::InvalidInheritedLayer {
                reason: "fork source and destination branches must differ",
            });
        }
        if !self.active.is_empty() || !self.frozen.is_empty() {
            return Err(BranchRuntimeError::InvalidInheritedLayer {
                reason: "fork source must flush active and frozen rows before inheritance capture",
            });
        }
        let observed_rows = self.observe_rows();
        if observed_rows.max_commit_version.is_none() {
            return Err(BranchRuntimeError::InvalidInheritedLayer {
                reason: "fork source must contain at least one retained row",
            });
        }

        let fork_version = observed_rows
            .max_commit_version
            .expect("fork source retained-row precondition checked");
        let mut layers = Vec::with_capacity(self.inherited_layers.len() + 1);
        layers.push(BranchInheritedLayer::new(
            InheritedLayerDescriptor::new(
                self.branch_id,
                fork_version,
                InheritedLayerStatus::Active,
                self.owned_table_count(),
            ),
            self.owned_levels.clone(),
        )?);
        for layer in &self.inherited_layers {
            if let Some(layer) = layer.clone_active_for_fork()? {
                layers.push(layer);
            }
        }

        let mut child = Self::new(destination_branch_id, self.config)?;
        let attach_outcome = child.attach_inherited_layers(layers)?;
        let outcome = BranchForkOutcome {
            source_branch_id: self.branch_id,
            destination_branch_id,
            fork_version,
            inherited_layer_count: attach_outcome.inherited_layer_count(),
            inherited_table_count: attach_outcome.inherited_table_count(),
        };
        Ok((child, outcome))
    }

    fn validate_inherited_attach(
        &self,
        layers: &[BranchInheritedLayer],
    ) -> BranchRuntimeResult<()> {
        if !self.inherited_layers.is_empty() {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "branch already has inherited layers",
            });
        }
        if !self.active.is_empty() || !self.frozen.is_empty() || self.owned_table_count() != 0 {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "inherited layers can only attach to an empty own branch state",
            });
        }
        if layers.is_empty() {
            return Err(BranchRuntimeError::InvalidInheritedLayer {
                reason: "inherited layer attach must include at least one layer",
            });
        }
        if layers.len() > self.config.max_inherited_layers() {
            return Err(BranchRuntimeError::InvalidInheritedLayer {
                reason: "inherited layer count exceeds branch runtime configuration",
            });
        }
        let mut previous_fork_version = None::<CommitVersion>;
        let mut source_branches = BTreeSet::<[u8; BranchId::BYTE_LEN]>::new();
        for layer in layers {
            if layer.source_branch_id() == self.branch_id {
                return Err(BranchRuntimeError::InvalidInheritedLayer {
                    reason: "inherited layer source branch must differ from child branch",
                });
            }
            if !source_branches.insert(*layer.source_branch_id().as_bytes()) {
                return Err(BranchRuntimeError::InvalidInheritedLayer {
                    reason: "inherited layer source branches must be unique",
                });
            }
            if previous_fork_version
                .is_some_and(|previous| layer.fork_version().as_u64() > previous.as_u64())
            {
                return Err(BranchRuntimeError::InvalidInheritedLayer {
                    reason: "inherited layers must be ordered nearest-first by fork version",
                });
            }
            previous_fork_version = Some(layer.fork_version());
            if layer.status() == InheritedLayerStatus::Unavailable {
                return Err(BranchRuntimeError::InvalidInheritedLayer {
                    reason: "unavailable inherited layers cannot attach",
                });
            }
        }
        Ok(())
    }
}
