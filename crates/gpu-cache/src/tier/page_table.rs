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

/// The host-authoritative page table.
pub struct PageTable<F: CopyFence> {
    slots: SlotAllocator,
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
    /// Builds a table over a page region with `page_bytes` slots.
    pub fn new(region_len: u64, page_bytes: u64) -> Result<Self, GpuError> {
        let region = SlotRegion {
            name: "pages",
            base: 0, // the table speaks region *offsets*; the backend owns bases
            len: region_len,
        };
        let slots = SlotAllocator::new(region, page_bytes)?;
        let capacity = slots.capacity() as usize;
        Ok(Self {
            slots,
            entries: vec![None; capacity],
            id_to_slot: HashMap::with_capacity(capacity),
            epoch: 0,
            epoch_fences: BTreeMap::new(),
            reuse_gates: Vec::new(),
            scan_cursor: 0,
        })
    }

    /// Current epoch.
    #[must_use]
    pub const fn epoch(&self) -> Epoch {
        self.epoch
    }

    /// Total slot capacity.
    #[must_use]
    pub const fn capacity(&self) -> u32 {
        self.slots.capacity()
    }

    /// Resident (placed) pages, selectable or not.
    #[must_use]
    pub fn resident(&self) -> u32 {
        self.slots.allocated() - u32::try_from(self.reuse_gates.len()).unwrap_or(u32::MAX)
    }

    /// Begins the next step: installs `fence` for the epoch that just
    /// finished and bumps the epoch counter.
    pub fn step_begin(&mut self, fence: F) -> Epoch {
        self.epoch_fences.insert(self.epoch, fence);
        self.epoch += 1;
        self.epoch
    }

    /// Byte offset of a slot in the page region.
    #[must_use]
    pub const fn slot_offset(&self, slot: u32) -> u64 {
        self.slots.slot_ptr(slot)
    }

    /// Takes a free slot for `page_id` (unselectable until [`Self::activate`]).
    /// `None` means the pool is full — the caller consults eviction, or
    /// degrades (never stalls).
    pub fn place(&mut self, page_id: PageId, dirty: bool) -> Option<u32> {
        if self.id_to_slot.contains_key(&page_id) {
            return None; // already resident or in flight; callers dedup, this backstops
        }
        let slot = self.slots.alloc()?;
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
            self.slots.release(slot);
        }
    }

    /// Evicts a slot: unselectable immediately, reusable only after the
    /// current epoch's fence completes. Dirty slots are refused — the
    /// write-behind owns them until [`Self::mark_clean`].
    pub fn evict(&mut self, slot: u32) -> Result<(), GpuError> {
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
        self.reuse_gates.push((slot, self.epoch));
        Ok(())
    }

    /// Opens reuse gates whose epoch fence has completed, returning slots to
    /// the allocator. Returns how many opened.
    pub fn sweep_reusable(&mut self) -> u32 {
        let mut opened = 0;
        let mut remaining = Vec::with_capacity(self.reuse_gates.len());
        for (slot, gate_epoch) in self.reuse_gates.drain(..) {
            // The gate's fence is installed by step_begin(gate_epoch + 1);
            // until then the epoch is still producing work and the gate holds.
            let complete = self
                .epoch_fences
                .get(&gate_epoch)
                .is_some_and(CopyFence::is_complete);
            if complete {
                self.slots.release(slot);
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

    /// Free slots available right now.
    #[must_use]
    pub fn free_now(&self) -> u32 {
        self.capacity() - self.slots.allocated()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;
    use std::rc::Rc;

    use super::{PageId, PageTable};
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
}
