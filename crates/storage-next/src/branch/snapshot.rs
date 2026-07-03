//! Published, refcounted branch read snapshots (BS2.3) + the off-lock registry (BS2.4).
//!
//! A [`BranchReadSlot`] holds the most recently published [`BranchReadView`] for one branch behind
//! an [`ArcSwap`], so a reader can load a consistent snapshot with a single atomic op and serve
//! every read verb off it. A [`BranchSnapshotPublisher`] owns one slot per live branch for a
//! runtime, keyed in an [`ArcSwap`]-wrapped map. The publisher mutates the map only under the
//! runtime lock (publish/remove); a [`registry_handle`](BranchSnapshotPublisher::registry_handle)
//! shares the same map with the runtime slot so reads load a snapshot **off-lock** via
//! [`load_from_registry`].

use std::collections::HashMap;
use std::sync::Arc;

use arc_swap::ArcSwap;
use strata_core_next::BranchId;

use crate::branch::read::BranchReadView;

/// The per-branch published snapshot map. Shared as `Arc<BranchSnapshotRegistry>` so the same map is
/// reachable from both the publish path (under the lock) and the off-lock read path.
pub(crate) type BranchSnapshotRegistry = ArcSwap<HashMap<BranchId, Arc<BranchReadSlot>>>;

/// The per-branch published snapshot cell. Shared as `Arc<BranchReadSlot>` so the same cell is
/// reachable from both the publish path (under the lock) and the off-lock registry.
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
///
/// The map lives behind an `ArcSwap`: publishing into an existing branch's slot never rebuilds the
/// map (the hot path), and only first-publish / removal (branch lifecycle events, both rare and
/// under the lock) swap a rebuilt map. The same `Arc` is shared with the runtime slot for off-lock
/// loads.
#[derive(Debug, Default)]
pub(crate) struct BranchSnapshotPublisher {
    read_slots: Arc<BranchSnapshotRegistry>,
}

impl BranchSnapshotPublisher {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Publish a freshly captured view, creating the branch's slot on first publish. Publishing into
    /// an existing slot is a single `ArcSwap` store with no map rebuild.
    pub(crate) fn publish_view(&mut self, branch_id: BranchId, view: Arc<BranchReadView>) {
        let map = self.read_slots.load();
        if let Some(slot) = map.get(&branch_id) {
            slot.publish(view);
        } else {
            let mut next = HashMap::clone(&map);
            next.insert(branch_id, Arc::new(BranchReadSlot::new(view)));
            self.read_slots.store(Arc::new(next));
        }
    }

    /// Drop a branch's slot (on delete). A racing reader completes on the snapshot it already holds.
    pub(crate) fn remove(&mut self, branch_id: BranchId) {
        let map = self.read_slots.load();
        if map.contains_key(&branch_id) {
            let mut next = HashMap::clone(&map);
            next.remove(&branch_id);
            self.read_slots.store(Arc::new(next));
        }
    }

    /// A shared handle to the registry so the runtime slot can load snapshots off-lock (BS2.4).
    pub(crate) fn registry_handle(&self) -> Arc<BranchSnapshotRegistry> {
        Arc::clone(&self.read_slots)
    }
}

/// Load the published snapshot for `branch_id` directly from a shared registry handle, off-lock:
/// one `ArcSwap` load for the map, one for the slot. `None` means the branch has no published slot
/// (never created, or deleted — the caller maps this to not-found).
pub(crate) fn load_from_registry(
    registry: &BranchSnapshotRegistry,
    branch_id: BranchId,
) -> Option<Arc<BranchReadView>> {
    registry.load().get(&branch_id).map(|slot| slot.load())
}
