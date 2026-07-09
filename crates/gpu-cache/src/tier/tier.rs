//! The tier facade: config, open, the per-step protocol, and counters.
//!
//! GT1 scope: residency machinery only. Selection (`topk_pages`) arrives
//! with the kernels (GT3); the `DLPack` seam with GT4. What exists here is the
//! contract those layers sit on: request/append/maintain with the fence
//! discipline, and stats that make degradation observable (HT-9).

use crate::tier::backend::{DeviceBackend, Region, RegionBytes};
use crate::tier::eviction;
use crate::tier::page_table::{Epoch, PageId, PageTable};
use crate::tier::promotion::{OffsetPlan, PromotionScheduler};
use crate::tier::store::{PageBlob, PageStore};
use crate::GpuError;

/// Tier geometry and behavior, fixed at open.
#[derive(Clone, Copy, Debug)]
pub struct TierConfig {
    /// Page blob size in bytes (256-aligned).
    pub page_bytes: u64,
    /// Summary blob size in bytes.
    pub summary_bytes: u64,
    /// Page-pool capacity in slots.
    pub page_slots: u32,
    /// Max promotions drained from the queue per maintain call.
    pub promotion_batch: usize,
}

/// HT-9 counters. Degradation must be observable or the thesis is
/// unfalsifiable — every silent-miss path increments something here.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TierStats {
    /// Requests served by an already-resident page.
    pub hits: u64,
    /// Requests that queued a promotion.
    pub queued: u64,
    /// Copies enqueued toward residency.
    pub promotions_started: u64,
    /// Pages activated (copy fenced complete).
    pub promotions_completed: u64,
    /// Batches or copies that failed; pages stayed cold.
    pub promotion_failures: u64,
    /// Requested ids absent from the store of record.
    pub store_misses: u64,
    /// Placements skipped because no slot was free this round.
    pub degraded_placements: u64,
    /// Evictions staged (fence-gated).
    pub evictions: u64,
    /// Slots whose reuse gate opened.
    pub slots_reused: u64,
}

/// Outcome of a page request.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RequestOutcome {
    /// Resident and selectable now.
    Hit,
    /// Not resident; promotion queued (or already pending).
    Queued,
}

/// The GPU cache tier over a device backend and a store of record.
pub struct Tier<B: DeviceBackend, S: PageStore> {
    backend: B,
    store: S,
    table: PageTable<B::Fence>,
    scheduler: PromotionScheduler<B::Fence>,
    config: TierConfig,
    stats: TierStats,
}

impl<B: DeviceBackend, S: PageStore> Tier<B, S> {
    /// Opens the tier: validates geometry, reserves the device regions.
    pub fn open(mut backend: B, store: S, config: TierConfig) -> Result<Self, GpuError> {
        if config.page_bytes == 0 || config.page_bytes % 256 != 0 {
            return Err(GpuError::InvalidConfig {
                detail: format!(
                    "page_bytes {} must be a nonzero multiple of 256",
                    config.page_bytes
                ),
            });
        }
        if config.summary_bytes == 0 || config.page_slots == 0 || config.promotion_batch == 0 {
            return Err(GpuError::InvalidConfig {
                detail: "summary_bytes, page_slots, and promotion_batch must be nonzero".to_owned(),
            });
        }
        let slots = u64::from(config.page_slots);
        backend.reserve(RegionBytes {
            pages: slots * config.page_bytes,
            summaries: slots * config.summary_bytes,
            // GT3 sizes this from the adjacency degree; a placeholder row of
            // 8 bytes/slot keeps the region present and the math exercised.
            adjacency: slots * 8,
        })?;
        let table = PageTable::new(slots * config.page_bytes, config.page_bytes)?;
        Ok(Self {
            backend,
            store,
            table,
            scheduler: PromotionScheduler::new(),
            config,
            stats: TierStats::default(),
        })
    }

    /// Begins a decode step: fences the finished epoch and bumps the clock.
    pub fn step_begin(&mut self) -> Result<Epoch, GpuError> {
        let fence = self.backend.fence_now()?;
        Ok(self.table.step_begin(fence))
    }

    /// Requests residency for a page. Hits touch the page's score; misses
    /// queue a promotion. Never blocks.
    pub fn request(&mut self, page_id: PageId, priority: u32) -> RequestOutcome {
        if let Some(slot) = self.table.slot_of(page_id) {
            self.table.touch(slot, 1.0);
            self.stats.hits += 1;
            return RequestOutcome::Hit;
        }
        self.scheduler.request(page_id, priority);
        self.stats.queued += 1;
        RequestOutcome::Queued
    }

    /// Appends a new page: durable in the store first (GT2 moves this to
    /// write-behind), then placed hot if a slot is available.
    pub fn append(&mut self, blob: &PageBlob) -> Result<PageId, GpuError> {
        if blob.bytes.len() as u64 != self.config.page_bytes
            || blob.summary.len() as u64 != self.config.summary_bytes
        {
            return Err(GpuError::InvalidConfig {
                detail: format!(
                    "append geometry mismatch: got {}/{} bytes, expected {}/{}",
                    blob.bytes.len(),
                    blob.summary.len(),
                    self.config.page_bytes,
                    self.config.summary_bytes
                ),
            });
        }
        let page_id = self.store.append_page(blob.clone())?; // GT2: write-behind
        self.ensure_headroom(1);
        let Some(slot) = self.table.place(page_id, false) else {
            // Durable but not hot (victims still fence-gated): a degradation,
            // not a failure.
            self.stats.degraded_placements += 1;
            return Ok(page_id);
        };
        let page_offset = self.table.slot_offset(slot);
        let summary_offset = u64::from(slot) * self.config.summary_bytes;
        let copied = self
            .backend
            .copy_in(Region::Pages, page_offset, &blob.bytes)
            .and_then(|_| {
                self.backend
                    .copy_in(Region::Summaries, summary_offset, &blob.summary)
            })
            .and_then(|_| self.backend.fence_now());
        if let Ok(fence) = copied {
            self.scheduler.track(page_id, slot, fence);
            self.stats.promotions_started += 1;
        } else {
            self.table.abort_place(slot);
            self.stats.promotion_failures += 1;
        }
        Ok(page_id)
    }

    /// One maintenance round (between steps): open reuse gates, make
    /// headroom, drain promotions, activate completed copies.
    pub fn maintain(&mut self) {
        self.stats.slots_reused += u64::from(self.table.sweep_reusable());
        let wanted = self.scheduler.queue_len().min(self.config.promotion_batch);
        self.ensure_headroom(wanted);
        let plan = OffsetPlan {
            page_bytes: self.config.page_bytes,
            summary_bytes: self.config.summary_bytes,
        };
        self.scheduler.drain(
            self.config.promotion_batch,
            plan,
            &mut self.table,
            &mut self.backend,
            &self.store,
            &mut self.stats,
        );
        self.scheduler.poll(&mut self.table, &mut self.stats);
    }

    /// Stages evictions until free + gated slots cover `wanted`. Gated
    /// slots are not free *yet* (their fence must open first) — eviction
    /// provides future headroom; the present round degrades if none is free.
    /// Never evicts dirty pages; never stalls.
    fn ensure_headroom(&mut self, wanted: usize) {
        loop {
            let available = self.table.free_now() as usize + self.table.gated();
            if available >= wanted {
                return;
            }
            let now = self.table.epoch();
            let Some(victim) = eviction::pick_victim(self.table.candidates(), now) else {
                return; // nothing evictable: degrade, never stall
            };
            if self.table.evict(victim).is_err() {
                return;
            }
            self.stats.evictions += 1;
        }
    }

    /// Selection feedback for a resident page (the kernels' job at GT3;
    /// tests and the synthetic driver call it directly until then).
    pub fn touch(&mut self, page_id: PageId, score: f32) {
        if let Some(slot) = self.table.slot_of(page_id) {
            self.table.touch(slot, score);
        }
    }

    /// True when the page is resident and selectable.
    #[must_use]
    pub fn is_selectable(&self, page_id: PageId) -> bool {
        self.table
            .slot_of(page_id)
            .and_then(|slot| self.table.state(slot))
            .is_some_and(|state| state.valid)
    }

    /// Counters (HT-9).
    #[must_use]
    pub const fn stats(&self) -> &TierStats {
        &self.stats
    }

    /// Resident page count (selectable or in flight).
    #[must_use]
    pub fn resident(&self) -> u32 {
        self.table.resident()
    }

    /// Evicted slots still awaiting their epoch fence.
    #[must_use]
    pub fn gated(&self) -> usize {
        self.table.gated()
    }

    /// Free slots available right now.
    #[must_use]
    pub fn free_now(&self) -> u32 {
        self.table.free_now()
    }

    /// Total slot capacity.
    #[must_use]
    pub fn capacity(&self) -> u32 {
        self.table.capacity()
    }

    /// Backend access for test oracles.
    #[must_use]
    pub fn backend(&self) -> &B {
        &self.backend
    }

    /// Mutable backend access for fault knobs in tests.
    pub fn backend_mut(&mut self) -> &mut B {
        &mut self.backend
    }

    /// Store access for test oracles.
    #[must_use]
    pub fn store(&self) -> &S {
        &self.store
    }

    /// Mutable store access for fault knobs in tests.
    pub fn store_mut(&mut self) -> &mut S {
        &mut self.store
    }
}
