//! Mutable-to-frozen table rotation helpers for branch-local state.

use super::BranchLocalState;
use crate::table::MutableTable;

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

impl BranchLocalState {
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

    pub(crate) fn rotate_active_if_size_threshold_reached(
        &mut self,
    ) -> Option<BranchRotationOutcome> {
        if self.active.approximate_size_bytes() < self.config.active_rotation_bytes() {
            return None;
        }
        Some(self.rotate_active())
    }
}
