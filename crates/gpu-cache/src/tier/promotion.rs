//! The promotion scheduler: store → staging → device, never stalling.
//!
//! Requests queue with priorities; draining reads batches from the store of
//! record and enqueues staged copies; completion polling activates pages.
//! Every failure path degrades (the page just isn't resident this step) —
//! nothing here ever blocks the decode loop (HT-4).

use std::collections::{BinaryHeap, HashSet};

use crate::tier::backend::{CopyFence, DeviceBackend, Region};
use crate::tier::page_table::{PageId, PageTable};
use crate::tier::store::PageStore;
use crate::tier::TierStats;

/// Byte layout facts the scheduler needs for offsets.
#[derive(Clone, Copy, Debug)]
pub(crate) struct OffsetPlan {
    pub page_bytes: u64,
    pub summary_bytes: u64,
}

#[derive(Debug, Eq, PartialEq)]
struct Queued {
    priority: u32,
    /// FIFO tiebreak (older first) for equal priorities.
    seq: u64,
    page_id: PageId,
}

impl Ord for Queued {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.priority
            .cmp(&other.priority)
            .then(other.seq.cmp(&self.seq))
    }
}

impl PartialOrd for Queued {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

struct InFlight<F> {
    page_id: PageId,
    slot: u32,
    fence: F,
}

/// The scheduler.
pub(crate) struct PromotionScheduler<F: CopyFence> {
    queue: BinaryHeap<Queued>,
    queued: HashSet<PageId>,
    in_flight: Vec<InFlight<F>>,
    seq: u64,
}

impl<F: CopyFence> PromotionScheduler<F> {
    pub(crate) fn new() -> Self {
        Self {
            queue: BinaryHeap::new(),
            queued: HashSet::new(),
            in_flight: Vec::new(),
            seq: 0,
        }
    }

    /// Queues a promotion request (deduplicated).
    pub(crate) fn request(&mut self, page_id: PageId, priority: u32) {
        if self.queued.insert(page_id) {
            self.queue.push(Queued {
                priority,
                seq: self.seq,
                page_id,
            });
            self.seq += 1;
        }
    }

    /// Tracks an already-enqueued copy (the append path).
    pub(crate) fn track(&mut self, page_id: PageId, slot: u32, fence: F) {
        self.in_flight.push(InFlight {
            page_id,
            slot,
            fence,
        });
    }

    pub(crate) fn queue_len(&self) -> usize {
        self.queue.len()
    }

    /// Drains up to `batch` requests through the store into staged copies.
    /// Failures degrade and are counted; they never propagate.
    pub(crate) fn drain<B, S>(
        &mut self,
        batch: usize,
        plan: OffsetPlan,
        table: &mut PageTable<F>,
        backend: &mut B,
        store: &S,
        stats: &mut TierStats,
    ) where
        B: DeviceBackend<Fence = F>,
        S: PageStore + ?Sized,
    {
        let mut ids = Vec::with_capacity(batch);
        while ids.len() < batch {
            let Some(next) = self.queue.pop() else { break };
            self.queued.remove(&next.page_id);
            // Already resident (raced with an append or a duplicate request).
            if table.slot_of(next.page_id).is_none() {
                ids.push(next.page_id);
            }
        }
        if ids.is_empty() {
            return;
        }

        let Ok(blobs) = store.read_pages(&ids) else {
            // The whole batch degrades; pages stay cold until re-requested.
            stats.promotion_failures += ids.len() as u64;
            return;
        };

        for (page_id, blob) in ids.into_iter().zip(blobs) {
            let Some(blob) = blob else {
                stats.store_misses += 1;
                continue;
            };
            let Some(slot) = table.place(page_id, false) else {
                // Pool full right now (victims may still be fence-gated):
                // degrade, the caller re-requests next step if still hot.
                stats.degraded_placements += 1;
                continue;
            };
            let page_offset = table.slot_offset(slot);
            let summary_offset = u64::from(slot) * plan.summary_bytes;
            debug_assert_eq!(blob.bytes.len() as u64, plan.page_bytes);
            let copied = backend
                .copy_in(Region::Pages, page_offset, &blob.bytes)
                .and_then(|_| backend.copy_in(Region::Summaries, summary_offset, &blob.summary))
                .and_then(|_| backend.fence_now());
            if let Ok(fence) = copied {
                self.in_flight.push(InFlight {
                    page_id,
                    slot,
                    fence,
                });
                stats.promotions_started += 1;
            } else {
                // The slot was never activated — no step could have seen
                // it, so it returns to the pool immediately.
                table.abort_place(slot);
                stats.promotion_failures += 1;
            }
        }
    }

    /// Activates pages whose copies completed. Returns how many activated.
    pub(crate) fn poll(&mut self, table: &mut PageTable<F>, stats: &mut TierStats) -> u32 {
        let mut activated = 0;
        let mut remaining = Vec::with_capacity(self.in_flight.len());
        for entry in self.in_flight.drain(..) {
            if entry.fence.is_complete() {
                // The page may have been evicted while in flight (possible
                // once eviction races promotion); activate only if it still
                // owns the slot.
                if table
                    .state(entry.slot)
                    .is_some_and(|s| s.page_id == entry.page_id)
                {
                    table.activate(entry.slot);
                    activated += 1;
                    stats.promotions_completed += 1;
                }
            } else {
                remaining.push(entry);
            }
        }
        self.in_flight = remaining;
        activated
    }
}
