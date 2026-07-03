//! Published, refcounted branch read snapshots (BS2.3).
//!
//! A [`BranchReadSlot`] holds the most recently published [`BranchReadView`] for one branch behind
//! an [`ArcSwap`], so a reader can load a consistent, pinned snapshot with a single atomic op and
//! serve every read verb off it. A [`BranchSnapshotPublisher`] owns one slot per live branch for a
//! runtime. In BS2.3 the slots are published under the runtime lock and read only by the debug
//! equivalence oracle (reads still lock); BS2.4 moves the load off-lock and cuts the read verbs
//! over to it.

use std::collections::HashMap;
use std::sync::Arc;

use arc_swap::ArcSwap;
use strata_core_next::BranchId;

use crate::branch::read::BranchReadView;

/// The per-branch published snapshot cell. Shared as `Arc<BranchReadSlot>` so the same cell is
/// reachable from both the publish path (under the lock) and, in BS2.4, the off-lock registry.
#[derive(Debug)]
pub(crate) struct BranchReadSlot {
    snapshot: ArcSwap<BranchReadView>,
}

impl BranchReadSlot {
    fn new(view: Arc<BranchReadView>) -> Self {
        Self {
            snapshot: ArcSwap::from(view),
        }
    }

    /// Publish a freshly captured snapshot, replacing the previous one (release ordering). Old
    /// snapshots stay valid for any reader still holding them and die by `Arc` drop.
    fn publish(&self, view: Arc<BranchReadView>) {
        self.snapshot.store(view);
    }

    /// Load the currently published snapshot with a single atomic refcount bump.
    fn load(&self) -> Arc<BranchReadView> {
        self.snapshot.load_full()
    }
}

/// Owns the per-branch published snapshots for one runtime. The runtime captures a `BranchReadView`
/// (it holds the catalog) and hands the owned view here to store; this type never touches the
/// catalog, so capture and store never overlap a borrow.
#[derive(Debug, Default)]
pub(crate) struct BranchSnapshotPublisher {
    read_slots: HashMap<BranchId, Arc<BranchReadSlot>>,
}

impl BranchSnapshotPublisher {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Publish a freshly captured view, creating the branch's slot on first publish.
    pub(crate) fn publish_view(&mut self, branch_id: BranchId, view: Arc<BranchReadView>) {
        match self.read_slots.get(&branch_id) {
            Some(slot) => slot.publish(view),
            None => {
                self.read_slots
                    .insert(branch_id, Arc::new(BranchReadSlot::new(view)));
            }
        }
    }

    /// The published snapshot for `branch_id`, if a slot exists. `None` means no snapshot has been
    /// published for the branch yet — a missed-creation bug the oracle flags.
    pub(crate) fn load(&self, branch_id: BranchId) -> Option<Arc<BranchReadView>> {
        self.read_slots.get(&branch_id).map(|slot| slot.load())
    }

    /// Drop a branch's slot (on delete). A racing reader completes on the snapshot it already holds.
    pub(crate) fn remove(&mut self, branch_id: BranchId) {
        self.read_slots.remove(&branch_id);
    }
}
