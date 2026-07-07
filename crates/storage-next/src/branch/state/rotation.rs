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
        // Rotation seals the active memtable into a frozen table without changing the
        // row set, so it skips the observed-row-facts rescan; only the frozen resident
        // byte total shifts. Read the sealed size back from `frozen[0]` so the delta
        // uses the exact accessor the shape recompute folds.
        self.shape.frozen_bytes = self.shape.frozen_bytes.saturating_add(
            u64::try_from(self.frozen[0].approximate_size_bytes()).unwrap_or(u64::MAX),
        );
        #[cfg(debug_assertions)]
        self.debug_assert_shape_consistent();
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
