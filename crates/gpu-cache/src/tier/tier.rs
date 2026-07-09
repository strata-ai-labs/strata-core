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
use crate::tier::store::{CommitReceipt, PageBlob, PageStore, TierManifest};
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
    /// Write-behind entries per durable batch commit.
    pub write_behind_batch: usize,
    /// Max entries the write-behind queue may hold before appends refuse
    /// with `resource_exhausted.tier.write_backlog`.
    pub write_backlog_cap: usize,
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
    /// Durable write-behind batch commits.
    pub write_commits: u64,
    /// Batch commits that failed (entries requeued for retry).
    pub write_commit_failures: u64,
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
/// Field order is load-bearing: the table and scheduler hold backend fences
/// (device events), which must drop before the backend itself (see
/// `CudaBackend`'s field-order note).
pub struct Tier<B: DeviceBackend, S: PageStore> {
    table: PageTable<B::Fence>,
    scheduler: PromotionScheduler<B::Fence>,
    backend: B,
    store: S,
    config: TierConfig,
    stats: TierStats,
    /// Appended pages awaiting their durable batch commit (HT-6).
    write_behind: std::collections::VecDeque<(PageId, PageBlob)>,
    /// Next page id; seeded from the store watermark at open.
    next_page_id: u64,
    /// Receipt of the most recent durable batch commit.
    last_receipt: Option<CommitReceipt>,
}

impl<B: DeviceBackend, S: PageStore> Tier<B, S> {
    /// Opens the tier: validates geometry, reserves the device regions.
    pub fn open(mut backend: B, mut store: S, config: TierConfig) -> Result<Self, GpuError> {
        if config.page_bytes == 0 || config.page_bytes % 256 != 0 {
            return Err(GpuError::InvalidConfig {
                detail: format!(
                    "page_bytes {} must be a nonzero multiple of 256",
                    config.page_bytes
                ),
            });
        }
        if config.summary_bytes == 0
            || config.page_slots == 0
            || config.promotion_batch == 0
            || config.write_behind_batch == 0
            || config.write_backlog_cap < config.write_behind_batch
        {
            return Err(GpuError::InvalidConfig {
                detail: "summary_bytes, page_slots, promotion_batch, and write_behind_batch \
                         must be nonzero, with write_backlog_cap >= write_behind_batch"
                    .to_owned(),
            });
        }

        // Geometry is a durable contract: reopening with different sizes is
        // refused, never silently reinterpreted (design §10).
        let configured = TierManifest {
            page_bytes: config.page_bytes,
            summary_bytes: config.summary_bytes,
        };
        match store.load_manifest()? {
            Some(stored) if stored != configured => {
                return Err(GpuError::GeometryMismatch {
                    stored: (stored.page_bytes, stored.summary_bytes),
                    configured: (configured.page_bytes, configured.summary_bytes),
                });
            }
            Some(_) => {}
            None => store.write_manifest(configured)?,
        }
        let next_page_id = store.watermark()?.map_or(0, |w| w.0 + 1);

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
            write_behind: std::collections::VecDeque::new(),
            next_page_id,
            last_receipt: None,
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

    /// Appends a new page: hot in T0 immediately (dirty), durable at the
    /// next batch commit or [`Self::flush`] (HT-6 write-behind). Refuses
    /// with `resource_exhausted.tier.write_backlog` at the queue cap —
    /// bounded loss, never silent loss.
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
        if self.write_behind.len() >= self.config.write_backlog_cap {
            return Err(GpuError::WriteBacklog {
                queued: self.write_behind.len(),
                cap: self.config.write_backlog_cap,
            });
        }
        let page_id = PageId(self.next_page_id);
        self.next_page_id += 1;
        self.write_behind.push_back((page_id, blob.clone()));
        self.ensure_headroom(1);
        let Some(slot) = self.table.place(page_id, true) else {
            // Queued for durability but not hot (victims still fence-gated):
            // a degradation, not a failure.
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
        // Opportunistic durability: full batches commit without waiting for
        // an explicit flush.
        while self.write_behind.len() >= self.config.write_behind_batch {
            if self.commit_one_batch().is_err() {
                break; // counted; entries requeued; retry next round
            }
        }
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
            &mut self.store,
            &mut self.stats,
        );
        self.scheduler.poll(&mut self.table, &mut self.stats);
    }

    /// Drains the entire write-behind queue and returns the durability
    /// point: the receipt of the last committed batch (or the previous one
    /// when nothing was pending). The one tier call where store errors
    /// surface instead of degrading — a failed flush means the durability
    /// point did not advance, and the caller must know.
    pub fn flush(&mut self) -> Result<Option<CommitReceipt>, GpuError> {
        while !self.write_behind.is_empty() {
            self.commit_one_batch()?;
        }
        Ok(self.last_receipt)
    }

    /// Commits one write-behind batch atomically (pages + watermark).
    /// On failure the entries return to the front of the queue for retry.
    fn commit_one_batch(&mut self) -> Result<(), GpuError> {
        let take = self.write_behind.len().min(self.config.write_behind_batch);
        if take == 0 {
            return Ok(());
        }
        let batch: Vec<(PageId, PageBlob)> = self.write_behind.drain(..take).collect();
        let watermark = batch
            .iter()
            .map(|(id, _)| *id)
            .max_by_key(|id| id.0)
            .expect("nonempty batch");
        match self.store.commit_batch(&batch, watermark) {
            Ok(receipt) => {
                self.last_receipt = Some(receipt);
                self.stats.write_commits += 1;
                for (id, _) in &batch {
                    if let Some(slot) = self.table.slot_of(*id) {
                        self.table.mark_clean(slot);
                    }
                }
                Ok(())
            }
            Err(error) => {
                self.stats.write_commit_failures += 1;
                for entry in batch.into_iter().rev() {
                    self.write_behind.push_front(entry);
                }
                Err(error)
            }
        }
    }

    /// Entries awaiting their durable commit.
    #[must_use]
    pub fn write_backlog(&self) -> usize {
        self.write_behind.len()
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
