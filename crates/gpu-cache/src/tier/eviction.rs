//! Score- and edge-aware eviction policy (design §5).
//!
//! A pure function over candidate views: no device state, no side effects,
//! trivially testable. Priority favors keeping pages that are (a) recently
//! selected, (b) historically high-scoring, and (c) neighbors of resident
//! pages — HT-4's edge-driven principle applied symmetrically, so the hot
//! neighborhood stays warm as a unit.

use crate::tier::page_table::{Epoch, SlotState};

/// Candidates examined per eviction pick. Evicting the minimum of a
/// bounded rotating sample keeps eviction O(1) in pool size (a full scan
/// per eviction dominates maintenance at 64k+ slots); the minimum of 64
/// near-uniform samples sits around the bottom 1-2% of priorities, which
/// is all a cache victim needs to be. Tables at or below the budget are
/// scanned exactly.
pub(super) const SAMPLE_BUDGET: usize = 64;

/// How strongly resident-neighbor count protects a page from eviction.
const NEIGHBOR_WEIGHT: f32 = 0.25;
/// How quickly recency decays, per epoch since last touch.
const AGE_WEIGHT: f32 = 0.05;

/// Retention priority: higher = keep. The eviction pick is the minimum.
#[must_use]
pub(super) fn retention_priority(state: &SlotState, now: Epoch) -> f32 {
    let age = now.saturating_sub(state.last_touch_epoch);
    // Precision loss on ages beyond 2^24 epochs is irrelevant: the age term
    // saturates the priority long before.
    #[allow(clippy::cast_precision_loss)]
    let age_penalty = AGE_WEIGHT * age as f32;
    #[allow(clippy::cast_precision_loss)]
    let neighbor_bonus = NEIGHBOR_WEIGHT * (1.0 + state.resident_neighbors as f32).ln();
    state.score + neighbor_bonus - age_penalty
}

/// Picks the eviction victim among candidates: the slot with the lowest
/// retention priority. Ties break toward the lower slot index for
/// determinism. `None` when there are no candidates (everything is dirty or
/// in flight — the caller degrades, never stalls).
#[must_use]
pub(super) fn pick_victim<'a, I>(candidates: I, now: Epoch) -> Option<u32>
where
    I: Iterator<Item = (u32, &'a SlotState)>,
{
    candidates
        .map(|(slot, state)| (slot, retention_priority(state, now)))
        .min_by(|a, b| {
            a.1.partial_cmp(&b.1)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then(a.0.cmp(&b.0))
        })
        .map(|(slot, _)| slot)
}

#[cfg(test)]
mod tests {
    use super::{pick_victim, retention_priority};
    use crate::tier::page_table::{PageId, SlotState};

    fn state(score: f32, last_touch: u64, neighbors: u32) -> SlotState {
        SlotState {
            page_id: PageId(0),
            valid: true,
            dirty: false,
            score,
            last_touch_epoch: last_touch,
            resident_neighbors: neighbors,
        }
    }

    #[test]
    fn stale_low_score_loses_to_fresh_high_score() {
        let stale = state(0.1, 0, 0);
        let fresh = state(0.9, 99, 0);
        let victim = pick_victim([(0, &stale), (1, &fresh)].into_iter(), 100);
        assert_eq!(victim, Some(0));
    }

    #[test]
    fn resident_neighbors_protect_a_page() {
        let loner = state(0.5, 50, 0);
        let hub = state(0.5, 50, 8);
        assert!(retention_priority(&hub, 60) > retention_priority(&loner, 60));
        let victim = pick_victim([(0, &hub), (1, &loner)].into_iter(), 60);
        assert_eq!(victim, Some(1));
    }

    #[test]
    fn ties_break_deterministically_toward_lower_slot() {
        let a = state(0.5, 10, 0);
        let b = state(0.5, 10, 0);
        let victim = pick_victim([(3, &a), (7, &b)].into_iter(), 20);
        assert_eq!(victim, Some(3));
    }

    #[test]
    fn empty_candidates_yield_none() {
        assert_eq!(pick_victim(std::iter::empty(), 5), None);
    }
}
