//! The page table: host-authoritative slot state, epoch pinning, and
//! event-fenced slot reuse (design §5).
//!
//! The invariant this module exists to uphold: **a slot is never handed back
//! to the allocator while any in-flight step could still read it.** The
//! mechanism costs zero synchronization:
//!
//! 1. Slots become *selectable* only when their promotion copy completes
//!    ([`PageTable::activate`]).
//! 2. Eviction flips the slot unselectable and stages a reuse gate stamped
//!    with the current epoch ([`PageTable::evict`]).
//! 3. `step_begin` installs a fence per finished epoch; a gate opens only
//!    when its epoch's fence reports complete ([`PageTable::sweep_reusable`])
//!    — i.e. after every step that could have selected the slot has drained.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, MutexGuard};

use crate::device::arena::{SlotAllocator, SlotRegion};
use crate::tier::backend::CopyFence;
use crate::GpuError;

/// Stable page identity (the T2 key; slots are transient placements).
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PageId(pub u64);

/// A decode-step epoch.
pub type Epoch = u64;

/// Per-slot host state.
#[derive(Clone, Debug)]
pub struct SlotState {
    /// The page occupying this slot.
    pub page_id: PageId,
    /// Selectable by steps (device validity mirrors this).
    pub valid: bool,
    /// Appended locally and not yet durable in the store of record.
    pub dirty: bool,
    /// Selection-feedback score (EMA; policy input).
    pub score: f32,
    /// Epoch of the last selection touch (recency; policy input).
    pub last_touch_epoch: Epoch,
    /// Resident graph neighbors (edge-awareness; policy input).
    pub resident_neighbors: u32,
}

/// The shared slot authority: one per device pool, shared by every tier
/// handle over it (HT-11). Owns which slots are allocated and how many
/// handles reference each one — the design's "table-count". A slot returns
/// to the allocator only when its last reference is released; per-handle
/// fence gating stays in each handle's [`PageTable`], so the reuse
/// invariant (§5) is enforced by the releasing handle before it releases.
pub(crate) struct SlotPool {
    slots: SlotAllocator,
    /// Handle references per slot.
    refs: Vec<u32>,
}

impl SlotPool {
    fn new(region_len: u64, page_bytes: u64) -> Result<Self, GpuError> {
        let region = SlotRegion {
            name: "pages",
            base: 0, // the pool speaks region *offsets*; the backend owns bases
            len: region_len,
        };
        let slots = SlotAllocator::new(region, page_bytes)?;
        let capacity = slots.capacity() as usize;
        Ok(Self {
            slots,
            refs: vec![0; capacity],
        })
    }

    /// Takes a free slot with one reference (the placing handle's).
    fn alloc(&mut self) -> Option<u32> {
        let slot = self.slots.alloc()?;
        self.refs[slot as usize] = 1;
        Some(slot)
    }

    /// Drops one handle's reference. The slot returns to the allocator at
    /// zero; `true` reports that it actually freed.
    fn release(&mut self, slot: u32) -> bool {
        let refs = &mut self.refs[slot as usize];
        debug_assert!(*refs > 0, "release of unreferenced slot {slot}");
        *refs = refs.saturating_sub(1);
        if *refs == 0 {
            self.slots.release(slot);
            true
        } else {
            false
        }
    }

    /// Adds a handle's reference to an allocated slot (fork adoption).
    fn add_ref(&mut self, slot: u32) {
        debug_assert!(
            self.refs[slot as usize] > 0,
            "add_ref on unallocated slot {slot}"
        );
        self.refs[slot as usize] += 1;
    }

    fn refs(&self, slot: u32) -> u32 {
        self.refs[slot as usize]
    }

    fn allocated(&self) -> u32 {
        self.slots.allocated()
    }
}

/// What an eviction did, given the slot's reference count (HT-11).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EvictOutcome {
    /// Other handles still reference the slot: this handle's reference was
    /// released immediately — the slot stays resident, valid, and
    /// selectable for the union, so no device write and no reuse gate. It
    /// frees when the last referencing handle evicts it.
    SharedRelease,
    /// This was the last reference: the slot is unselectable now and
    /// reusable only after this handle's current epoch fence completes.
    /// The reuse gate holds the final reference until the sweep releases
    /// it. Because every handle's work shares one device stream, that
    /// fence also covers the other handles' earlier-enqueued selections.
    LastGated,
}

/// The host-authoritative page table: one per tier handle. Slot allocation
/// goes through the [`SlotPool`] shared across handles; everything else —
/// id→slot mapping, per-slot metadata, epoch pinning, fence-gated reuse —
/// is this handle's own view.
pub struct PageTable<F: CopyFence> {
    pool: Arc<Mutex<SlotPool>>,
    page_bytes: u64,
    capacity: u32,
    entries: Vec<Option<SlotState>>,
    id_to_slot: HashMap<PageId, u32>,
    epoch: Epoch,
    /// Fence per *finished* epoch: installed by `step_begin(N+1)`, capturing
    /// all work steps ≤ N enqueued.
    epoch_fences: BTreeMap<Epoch, F>,
    /// Evicted slots waiting for their epoch's fence.
    reuse_gates: Vec<(u32, Epoch)>,
    /// Rotating start position for [`Self::sample_candidates`].
    scan_cursor: usize,
}

impl<F: CopyFence> PageTable<F> {
    /// Builds a table over a page region with `page_bytes` slots, creating
    /// a fresh slot pool (HT-11 fork shares the pool instead).
    pub fn new(region_len: u64, page_bytes: u64) -> Result<Self, GpuError> {
        let pool = SlotPool::new(region_len, page_bytes)?;
        let capacity = pool.slots.capacity();
        Ok(Self {
            pool: Arc::new(Mutex::new(pool)),
            page_bytes,
            capacity,
            entries: vec![None; capacity as usize],
            id_to_slot: HashMap::with_capacity(capacity as usize),
            epoch: 0,
            epoch_fences: BTreeMap::new(),
            reuse_gates: Vec::new(),
            scan_cursor: 0,
        })
    }

    /// Locks the shared pool. Scoped to one operation at every call site —
    /// never held across other table calls.
    fn pool(&self) -> MutexGuard<'_, SlotPool> {
        self.pool.lock().expect("slot pool lock poisoned")
    }

    /// Forks this handle's view (HT-11): clones the id→slot map and
    /// per-slot state into a new table sharing the same slot pool, bumping
    /// each cloned slot's reference count. Only *activated* placements are
    /// shared — an in-flight copy is tracked by the parent's scheduler
    /// alone, and a cloned-but-never-activated entry could never become
    /// selectable in the child. The child starts with the parent's epoch
    /// value, fresh fences, and no reuse gates.
    #[must_use]
    pub fn fork(&self) -> Self {
        let mut entries = vec![None; self.entries.len()];
        let mut id_to_slot = HashMap::with_capacity(self.id_to_slot.len());
        {
            let mut pool = self.pool();
            for (slot, entry) in self.entries.iter().enumerate() {
                if let Some(state) = entry.as_ref().filter(|state| state.valid) {
                    debug_assert!(!state.dirty, "fork requires a flushed parent");
                    let slot_index = u32::try_from(slot).expect("capacity fits u32");
                    pool.add_ref(slot_index);
                    entries[slot] = Some(state.clone());
                    id_to_slot.insert(state.page_id, slot_index);
                }
            }
        }
        Self {
            pool: Arc::clone(&self.pool),
            page_bytes: self.page_bytes,
            capacity: self.capacity,
            entries,
            id_to_slot,
            epoch: self.epoch,
            epoch_fences: BTreeMap::new(),
            reuse_gates: Vec::new(),
            scan_cursor: self.scan_cursor,
        }
    }

    /// How many handles currently reference `slot` (HT-11: eviction of a
    /// shared slot releases a reference; only the last flips validity).
    #[must_use]
    pub fn shared_references(&self, slot: u32) -> u32 {
        self.pool().refs(slot)
    }

    /// Releases this handle's references to slots it shares with other
    /// handles (drop path). Exclusive slots and pending reuse gates are
    /// deliberately left allocated: freeing them safely needs a fence this
    /// handle can no longer wait on, so they stay pinned until the family
    /// tears down (v0.5 contract; orphan-gate handoff is the follow-up).
    pub(crate) fn drop_shared_references(&mut self) {
        let pool = Arc::clone(&self.pool);
        // Drop path: a poisoned pool (a panic elsewhere in the family)
        // must not double-panic here — leak the references instead.
        let Ok(mut pool) = pool.lock() else {
            return;
        };
        for (slot, entry) in self.entries.iter_mut().enumerate() {
            if entry.is_none() {
                continue;
            }
            let slot_index = u32::try_from(slot).expect("capacity fits u32");
            if pool.refs(slot_index) > 1 {
                pool.release(slot_index);
                if let Some(state) = entry.take() {
                    self.id_to_slot.remove(&state.page_id);
                }
            }
        }
    }

    /// Current epoch.
    #[must_use]
    pub const fn epoch(&self) -> Epoch {
        self.epoch
    }

    /// Total slot capacity.
    #[must_use]
    pub const fn capacity(&self) -> u32 {
        self.capacity
    }

    /// Pages resident in *this handle's* table (selectable or not).
    #[must_use]
    pub fn resident(&self) -> u32 {
        u32::try_from(self.id_to_slot.len()).unwrap_or(u32::MAX)
    }

    /// Begins the next step: installs `fence` for the epoch that just
    /// finished and bumps the epoch counter.
    pub fn step_begin(&mut self, fence: F) -> Epoch {
        self.epoch_fences.insert(self.epoch, fence);
        self.epoch += 1;
        self.epoch
    }

    /// Byte offset of a slot in the page region (offsets are pure geometry:
    /// slot × page size from a zero base).
    #[must_use]
    pub const fn slot_offset(&self, slot: u32) -> u64 {
        slot as u64 * self.page_bytes
    }

    /// Takes a free slot for `page_id` (unselectable until [`Self::activate`]).
    /// `None` means the pool is full — the caller consults eviction, or
    /// degrades (never stalls).
    pub fn place(&mut self, page_id: PageId, dirty: bool) -> Option<u32> {
        if self.id_to_slot.contains_key(&page_id) {
            return None; // already resident or in flight; callers dedup, this backstops
        }
        let slot = self.pool().alloc()?;
        self.entries[slot as usize] = Some(SlotState {
            page_id,
            valid: false,
            dirty,
            score: 0.0,
            last_touch_epoch: self.epoch,
            resident_neighbors: 0,
        });
        self.id_to_slot.insert(page_id, slot);
        Some(slot)
    }

    /// Marks a placed slot selectable (its bytes are resident).
    pub fn activate(&mut self, slot: u32) {
        if let Some(state) = self.entries[slot as usize].as_mut() {
            state.valid = true;
        }
    }

    /// Selection feedback: bumps score EMA and recency.
    pub fn touch(&mut self, slot: u32, score: f32) {
        let epoch = self.epoch;
        if let Some(state) = self.entries[slot as usize].as_mut() {
            state.score = 0.75 * state.score + 0.25 * score;
            state.last_touch_epoch = epoch;
        }
    }

    /// Adjusts a slot's resident-neighbor count (the eviction policy's
    /// edge-awareness input). Saturating in both directions.
    pub fn add_resident_neighbor(&mut self, slot: u32, delta: i32) {
        if let Some(state) = self.entries[slot as usize].as_mut() {
            state.resident_neighbors = state.resident_neighbors.saturating_add_signed(delta);
        }
    }

    /// Marks a slot's page durable (write-behind completed).
    pub fn mark_clean(&mut self, slot: u32) {
        if let Some(state) = self.entries[slot as usize].as_mut() {
            state.dirty = false;
        }
    }

    /// Looks up a resident page's slot.
    #[must_use]
    pub fn slot_of(&self, page_id: PageId) -> Option<u32> {
        self.id_to_slot.get(&page_id).copied()
    }

    /// Reads a slot's state.
    #[must_use]
    pub fn state(&self, slot: u32) -> Option<&SlotState> {
        self.entries[slot as usize].as_ref()
    }

    /// Iterates eviction candidates: valid, clean slots.
    pub fn candidates(&self) -> impl Iterator<Item = (u32, &SlotState)> {
        self.entries.iter().enumerate().filter_map(|(slot, entry)| {
            entry
                .as_ref()
                .filter(|state| state.valid && !state.dirty)
                .map(|state| (u32::try_from(slot).unwrap_or(u32::MAX), state))
        })
    }

    /// Bounded eviction-candidate sample: up to `budget` valid, clean slots
    /// collected from a rotating cursor, so successive calls sweep the whole
    /// table. Evicting the minimum of a sample is the standard trade at
    /// scale — a full scan per eviction is O(capacity) and dominates
    /// maintenance once pools reach tens of thousands of slots. Tables that
    /// fit within the budget always yield every candidate (exact eviction,
    /// deterministic small-scale tests). The full circle is walked only when
    /// clean pages are scarce.
    pub fn sample_candidates(&mut self, budget: usize) -> Vec<(u32, &SlotState)> {
        let len = self.entries.len();
        if len == 0 {
            return Vec::new();
        }
        let start = self.scan_cursor % len;
        let mut picked: Vec<u32> = Vec::with_capacity(budget.min(len));
        let mut visited = 0;
        while visited < len && picked.len() < budget {
            let slot = (start + visited) % len;
            visited += 1;
            let qualifies = self.entries[slot]
                .as_ref()
                .is_some_and(|state| state.valid && !state.dirty);
            if qualifies {
                picked.push(u32::try_from(slot).unwrap_or(u32::MAX));
            }
        }
        self.scan_cursor = (start + visited) % len;
        picked
            .into_iter()
            .filter_map(|slot| {
                self.entries[slot as usize]
                    .as_ref()
                    .map(|state| (slot, state))
            })
            .collect()
    }

    /// Rolls back a placement whose copy never started or failed. The slot
    /// was never activated, so no step could have selected it — it returns
    /// to the pool immediately, no fence needed.
    pub fn abort_place(&mut self, slot: u32) {
        if let Some(state) = self.entries[slot as usize].take() {
            debug_assert!(!state.valid, "abort_place on an activated slot");
            self.id_to_slot.remove(&state.page_id);
            self.pool().release(slot);
        }
    }

    /// Evicts a slot from this handle's view. Dirty slots are refused —
    /// the write-behind owns them until [`Self::mark_clean`].
    ///
    /// The outcome depends on the slot's reference count: a shared slot
    /// releases this handle's reference and stays live for the union; the
    /// last reference stages a reuse gate on the current epoch (the gate
    /// holds that final reference until [`Self::sweep_reusable`] opens it).
    /// The caller flips device validity only on [`EvictOutcome::LastGated`].
    pub fn evict(&mut self, slot: u32) -> Result<EvictOutcome, GpuError> {
        let Some(state) = self.entries[slot as usize].as_ref() else {
            return Err(GpuError::InvalidConfig {
                detail: format!("evict of empty slot {slot}"),
            });
        };
        if state.dirty {
            return Err(GpuError::InvalidConfig {
                detail: format!("evict of dirty slot {slot} (write-behind owns it)"),
            });
        }
        let page_id = state.page_id;
        self.id_to_slot.remove(&page_id);
        self.entries[slot as usize] = None;
        let mut pool = self.pool();
        if pool.refs(slot) > 1 {
            pool.release(slot);
            Ok(EvictOutcome::SharedRelease)
        } else {
            drop(pool);
            self.reuse_gates.push((slot, self.epoch));
            Ok(EvictOutcome::LastGated)
        }
    }

    /// Opens reuse gates whose epoch fence has completed, returning slots to
    /// the allocator. Returns how many opened.
    pub fn sweep_reusable(&mut self) -> u32 {
        let mut opened = 0;
        let gates = std::mem::take(&mut self.reuse_gates);
        let mut remaining = Vec::with_capacity(gates.len());
        for (slot, gate_epoch) in gates {
            // The gate's fence is installed by step_begin(gate_epoch + 1);
            // until then the epoch is still producing work and the gate holds.
            let complete = self
                .epoch_fences
                .get(&gate_epoch)
                .is_some_and(CopyFence::is_complete);
            if complete {
                self.pool().release(slot);
                opened += 1;
            } else {
                remaining.push((slot, gate_epoch));
            }
        }
        self.reuse_gates = remaining;
        // Fences older than every open gate can never be consulted again.
        let oldest_gate = self.reuse_gates.iter().map(|(_, e)| *e).min();
        match oldest_gate {
            Some(oldest) => self.epoch_fences.retain(|epoch, _| *epoch >= oldest),
            None => self.epoch_fences.clear(),
        }
        opened
    }

    /// Slots currently gated (evicted, awaiting fence).
    #[must_use]
    pub fn gated(&self) -> usize {
        self.reuse_gates.len()
    }

    /// Free slots available right now (pool-wide: a slot referenced by any
    /// handle is not free).
    #[must_use]
    pub fn free_now(&self) -> u32 {
        self.capacity() - self.pool().allocated()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::{EvictOutcome, PageId, PageTable};
    use crate::tier::backend::CopyFence;

    /// Manually-completed fence for direct mechanics tests.
    #[derive(Clone, Default)]
    struct ManualFence(Rc<Cell<bool>>);

    impl ManualFence {
        fn complete(&self) {
            self.0.set(true);
        }
    }

    impl CopyFence for ManualFence {
        fn is_complete(&self) -> bool {
            self.0.get()
        }
    }

    fn table(slots: u64) -> PageTable<ManualFence> {
        PageTable::new(slots * 256, 256).expect("valid table")
    }

    #[test]
    fn place_activate_evict_lifecycle() {
        let mut table = table(2);
        let slot = table.place(PageId(7), false).expect("free slot");
        assert!(
            !table.state(slot).unwrap().valid,
            "unselectable until activate"
        );
        assert_eq!(table.candidates().count(), 0);
        table.activate(slot);
        assert!(table.state(slot).unwrap().valid);
        assert_eq!(table.candidates().count(), 1);
        assert_eq!(table.slot_of(PageId(7)), Some(slot));
        table.evict(slot).expect("clean evict");
        assert_eq!(table.slot_of(PageId(7)), None);
        assert_eq!(table.gated(), 1);
    }

    #[test]
    fn gate_holds_until_epoch_fence_completes() {
        let mut table = table(1);
        let slot = table.place(PageId(1), false).expect("slot");
        table.activate(slot);
        table.evict(slot).expect("evict");

        // No fence installed for the eviction epoch yet: gate must hold.
        assert_eq!(table.sweep_reusable(), 0);
        assert!(
            table.place(PageId(2), false).is_none(),
            "slot must not be reused"
        );

        // The next step installs the fence, but the epoch's work is still
        // in flight: gate still holds.
        let fence = ManualFence::default();
        table.step_begin(fence.clone());
        assert_eq!(table.sweep_reusable(), 0);
        assert!(table.place(PageId(2), false).is_none());

        // Work drains: gate opens, slot is reusable.
        fence.complete();
        assert_eq!(table.sweep_reusable(), 1);
        assert_eq!(table.gated(), 0);
        assert!(
            table.place(PageId(2), false).is_some(),
            "slot reusable after fence"
        );
    }

    #[test]
    fn dirty_slots_refuse_eviction() {
        let mut table = table(1);
        let slot = table.place(PageId(1), true).expect("slot");
        table.activate(slot);
        assert_eq!(
            table.candidates().count(),
            0,
            "dirty pages are not candidates"
        );
        assert!(table.evict(slot).is_err());
        table.mark_clean(slot);
        assert!(table.evict(slot).is_ok());
    }

    #[test]
    fn abort_place_returns_slot_immediately() {
        let mut table = table(1);
        let slot = table.place(PageId(1), false).expect("slot");
        table.abort_place(slot);
        assert_eq!(table.slot_of(PageId(1)), None);
        assert!(table.place(PageId(2), false).is_some(), "no fence needed");
    }

    #[test]
    fn duplicate_place_is_refused() {
        let mut table = table(2);
        assert!(table.place(PageId(1), false).is_some());
        assert!(table.place(PageId(1), false).is_none(), "id already placed");
    }

    #[test]
    fn accounting_is_conserved() {
        let mut table = table(4);
        let a = table.place(PageId(1), false).unwrap();
        let b = table.place(PageId(2), false).unwrap();
        table.activate(a);
        table.activate(b);
        table.evict(a).unwrap();
        // capacity = free + gated + resident
        assert_eq!(
            table.capacity(),
            table.free_now() + u32::try_from(table.gated()).unwrap() + table.resident()
        );
    }

    #[test]
    fn fork_shares_activated_entries_and_bumps_references() {
        let mut parent = table(4);
        let active = parent.place(PageId(1), false).unwrap();
        parent.activate(active);
        let inflight = parent.place(PageId(2), false).unwrap();

        let child = parent.fork();
        assert_eq!(child.slot_of(PageId(1)), Some(active), "warm set shared");
        assert_eq!(
            child.slot_of(PageId(2)),
            None,
            "in-flight placement stays the parent's"
        );
        assert_eq!(parent.shared_references(active), 2);
        assert_eq!(parent.shared_references(inflight), 1);
        assert_eq!(
            parent.free_now(),
            child.free_now(),
            "one pool: both handles see the same free space"
        );
    }

    #[test]
    fn shared_evict_releases_without_gate_or_free() {
        let mut parent = table(2);
        let slot = parent.place(PageId(1), false).unwrap();
        parent.activate(slot);
        let mut child = parent.fork();

        assert_eq!(
            parent.evict(slot).unwrap(),
            EvictOutcome::SharedRelease,
            "child still references the slot"
        );
        assert_eq!(parent.gated(), 0, "no reuse gate for a shared release");
        assert_eq!(parent.free_now(), 1, "slot stays allocated for the child");
        assert_eq!(child.slot_of(PageId(1)), Some(slot), "child unaffected");

        assert_eq!(
            child.evict(slot).unwrap(),
            EvictOutcome::LastGated,
            "last reference gates"
        );
        assert_eq!(child.gated(), 1);
    }

    #[test]
    fn last_release_frees_only_after_its_fence() {
        let mut parent = table(1);
        let slot = parent.place(PageId(1), false).unwrap();
        parent.activate(slot);
        let mut child = parent.fork();

        parent.evict(slot).unwrap();
        child.evict(slot).unwrap();
        assert_eq!(child.sweep_reusable(), 0, "gate holds before any fence");

        let fence = ManualFence::default();
        child.step_begin(fence.clone());
        assert_eq!(child.sweep_reusable(), 0, "epoch work still in flight");

        fence.complete();
        assert_eq!(child.sweep_reusable(), 1);
        assert!(
            parent.place(PageId(9), false).is_some(),
            "freed slot is reusable by any handle"
        );
    }

    #[test]
    fn drop_shared_references_leaves_exclusive_slots_pinned() {
        let mut parent = table(3);
        let shared = parent.place(PageId(1), false).unwrap();
        parent.activate(shared);
        let mut child = parent.fork();
        let exclusive = child.place(PageId(2), false).unwrap();
        child.activate(exclusive);

        child.drop_shared_references();
        assert_eq!(
            parent.shared_references(shared),
            1,
            "child's shared reference released"
        );
        assert_eq!(
            parent.shared_references(exclusive),
            1,
            "exclusive slot stays pinned (v0.5 leak contract)"
        );
        assert_eq!(parent.free_now(), 1, "only the never-used slot is free");
    }
}
