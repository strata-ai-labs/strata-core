//! The promotion scheduler: request queue, in-flight tracking, activation.
//!
//! GT3 slimmed this to pure bookkeeping — the *tier* owns store reads and
//! device writes (structure installs happen in exactly one place). The
//! scheduler dedups requests by priority, tracks staged copies, and reports
//! which slots' fences completed so the tier can flip their validity.

use std::collections::{BinaryHeap, HashSet};

use crate::tier::backend::CopyFence;
use crate::tier::page_table::{PageId, PageTable};

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

    /// Pops up to `batch` non-resident requests for the tier to fetch.
    pub(crate) fn take_batch(&mut self, batch: usize, table: &PageTable<F>) -> Vec<PageId> {
        let mut ids = Vec::with_capacity(batch);
        while ids.len() < batch {
            let Some(next) = self.queue.pop() else { break };
            self.queued.remove(&next.page_id);
            // Already resident (raced with an append or a duplicate request).
            if table.slot_of(next.page_id).is_none() {
                ids.push(next.page_id);
            }
        }
        ids
    }

    /// Tracks a staged copy awaiting its fence.
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

    /// Returns the slots whose copies completed and still hold their page
    /// (eviction may race an in-flight promotion); the tier activates them.
    pub(crate) fn poll(&mut self, table: &PageTable<F>) -> Vec<u32> {
        let mut ready = Vec::new();
        let mut remaining = Vec::with_capacity(self.in_flight.len());
        for entry in self.in_flight.drain(..) {
            if entry.fence.is_complete() {
                if table
                    .state(entry.slot)
                    .is_some_and(|s| s.page_id == entry.page_id)
                {
                    ready.push(entry.slot);
                }
            } else {
                remaining.push(entry);
            }
        }
        self.in_flight = remaining;
        ready
    }
}
