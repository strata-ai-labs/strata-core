//! The tier facade: config, open, the per-step protocol, and counters.
//!
//! GT1 scope: residency machinery only. Selection (`topk_pages`) arrives
//! with the kernels (GT3); the `DLPack` seam with GT4. What exists here is the
//! contract those layers sit on: request/append/maintain with the fence
//! discipline, and stats that make degradation observable (HT-9).

use crate::tier::backend::{DeviceBackend, Region, RegionBytes, TagFilter};
use crate::tier::eviction;
use crate::tier::page_table::{Epoch, PageId, PageTable};
use crate::tier::promotion::PromotionScheduler;
use crate::tier::store::{CommitReceipt, PageBlob, PageStore, TierManifest};
use crate::GpuError;

pub use crate::tier::backend::{MAX_EXPAND, MAX_K};

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
    /// Bounded adjacency degree: resident-neighbor entries per slot.
    pub adjacency_degree: u16,
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

/// A selection result mapped back to page ids (the GT3 test surface).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TierTopk {
    /// Selected pages, best first, with their scores.
    pub selected: Vec<(PageId, f32)>,
    /// One-hop expansion of the selection (deduplicated).
    pub expanded: Vec<PageId>,
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
    /// Fence of the most recent selection/materialization enqueue.
    selection_fence: Option<B::Fence>,
    /// Per-slot edge ids of the resident page (for unlink bookkeeping).
    slot_edges: Vec<Vec<PageId>>,
    /// Per-slot resident-adjacency mirror (what the device row holds).
    slot_adj: Vec<Vec<u32>>,
    /// Edges whose target is not resident yet: target id -> waiting slots.
    waiting_edges: std::collections::HashMap<PageId, Vec<u32>>,
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
            || config.adjacency_degree == 0
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

        if config.summary_bytes % 4 != 0 {
            return Err(GpuError::InvalidConfig {
                detail: "summary_bytes must be a multiple of 4 (f32 summaries)".to_owned(),
            });
        }
        let slots = u64::from(config.page_slots);
        backend.reserve(RegionBytes {
            pages: slots * config.page_bytes,
            summaries: slots * config.summary_bytes,
            adjacency: slots * u64::from(config.adjacency_degree) * 4,
            validity: slots,
            tags: slots * 32,
            scratch: crate::tier::backend::scratch_bytes(slots, config.summary_bytes),
            materialize: u64::from(MAX_K) * config.page_bytes,
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
            selection_fence: None,
            slot_edges: vec![Vec::new(); config.page_slots as usize],
            slot_adj: vec![Vec::new(); config.page_slots as usize],
            waiting_edges: std::collections::HashMap::new(),
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
        if blob.edges.len() > usize::from(self.config.adjacency_degree) {
            return Err(GpuError::InvalidConfig {
                detail: format!(
                    "page has {} edges; the adjacency degree is {}",
                    blob.edges.len(),
                    self.config.adjacency_degree
                ),
            });
        }
        let page_id = PageId(self.next_page_id);
        self.next_page_id += 1;
        self.write_behind.push_back((page_id, blob.clone()));
        self.ensure_headroom(1);
        self.install_page(page_id, blob, true);
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

        let ids = self
            .scheduler
            .take_batch(self.config.promotion_batch, &self.table);
        if !ids.is_empty() {
            match self.store.read_pages(&ids) {
                Ok(blobs) => {
                    for (page_id, blob) in ids.into_iter().zip(blobs) {
                        let Some(blob) = blob else {
                            self.stats.store_misses += 1;
                            continue;
                        };
                        self.install_page(page_id, &blob, false);
                    }
                }
                Err(_) => {
                    // The whole batch degrades; pages stay cold until
                    // re-requested.
                    self.stats.promotion_failures += ids.len() as u64;
                }
            }
        }

        for slot in self.scheduler.poll(&self.table) {
            self.activate_slot(slot);
        }
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

    /// Places a page and stages every device write except validity: page
    /// bytes, summary, tags, adjacency links (with symmetric neighbor-row
    /// updates and edge-driven prefetch). Validity flips only at
    /// [`Self::activate_slot`], once the fence completes. Failures degrade
    /// and roll back; nothing stalls.
    fn install_page(&mut self, page_id: PageId, blob: &PageBlob, dirty: bool) {
        debug_assert_eq!(blob.bytes.len() as u64, self.config.page_bytes);
        let Some(slot) = self.table.place(page_id, dirty) else {
            // Pool full right now (victims may still be fence-gated):
            // degrade; hot pages get re-requested.
            self.stats.degraded_placements += 1;
            return;
        };

        // Structure bookkeeping first (host mirrors + neighbor rows).
        let index = slot as usize;
        self.slot_edges[index].clone_from(&blob.edges);
        let mut linked = Vec::new();
        for edge in &blob.edges {
            if let Some(neighbor) = self.table.slot_of(*edge) {
                if neighbor != slot {
                    linked.push(neighbor);
                }
            } else {
                self.waiting_edges.entry(*edge).or_default().push(slot);
                // Edge-driven prefetch (HT-4): graph neighbors of a touched
                // page are promotion candidates the moment it is placed.
                self.scheduler.request(*edge, 0);
            }
        }
        // Pages waiting for *this* page link up now.
        if let Some(waiters) = self.waiting_edges.remove(&page_id) {
            for waiter in waiters {
                if waiter != slot && self.table.state(waiter).is_some() {
                    linked.push(waiter);
                }
            }
        }
        linked.sort_unstable();
        linked.dedup();
        let degree = usize::from(self.config.adjacency_degree);
        let mut dirty_rows = vec![slot];
        for neighbor in linked {
            if self.slot_adj[index].len() < degree {
                self.slot_adj[index].push(neighbor);
                self.table.add_resident_neighbor(slot, 1);
            }
            let neighbor_index = neighbor as usize;
            if self.slot_adj[neighbor_index].len() < degree {
                self.slot_adj[neighbor_index].push(slot);
                self.table.add_resident_neighbor(neighbor, 1);
                dirty_rows.push(neighbor);
            }
        }

        // Device writes: blob, summary, tags, adjacency rows; validity stays
        // 0 until activation.
        let page_offset = self.table.slot_offset(slot);
        let summary_offset = u64::from(slot) * self.config.summary_bytes;
        let mut tag_bytes = [0u8; 32];
        for (i, tag) in blob.tags.iter().enumerate() {
            tag_bytes[i * 8..i * 8 + 8].copy_from_slice(&tag.to_le_bytes());
        }
        let copied = (|| {
            self.backend
                .copy_in(Region::Pages, page_offset, &blob.bytes)?;
            self.backend
                .copy_in(Region::Summaries, summary_offset, &blob.summary)?;
            self.backend
                .copy_in(Region::Tags, u64::from(slot) * 32, &tag_bytes)?;
            for row in dirty_rows {
                self.write_adjacency_row(row)?;
            }
            self.backend.fence_now()
        })();
        if let Ok(fence) = copied {
            self.scheduler.track(page_id, slot, fence);
            self.stats.promotions_started += 1;
        } else {
            self.uninstall_structures(slot);
            self.table.abort_place(slot);
            self.stats.promotion_failures += 1;
        }
    }

    /// Flips a slot selectable: host state plus the device validity byte —
    /// the last write, so kernels never see a partially-installed page.
    fn activate_slot(&mut self, slot: u32) {
        if self
            .backend
            .copy_in(Region::Validity, u64::from(slot), &[1])
            .is_err()
        {
            // The page stays resident but unselectable; a future request
            // re-queues it after eviction. Counted as a failure.
            self.stats.promotion_failures += 1;
            return;
        }
        self.table.activate(slot);
        self.stats.promotions_completed += 1;
    }

    /// Unlinks a slot from the adjacency structures: neighbor rows lose the
    /// entry (host + device), waiting registrations are dropped. Idempotent.
    fn uninstall_structures(&mut self, slot: u32) {
        let index = slot as usize;
        for edge in std::mem::take(&mut self.slot_edges[index]) {
            if let Some(waiters) = self.waiting_edges.get_mut(&edge) {
                waiters.retain(|&w| w != slot);
                if waiters.is_empty() {
                    self.waiting_edges.remove(&edge);
                }
            }
        }
        let neighbors = std::mem::take(&mut self.slot_adj[index]);
        for neighbor in neighbors {
            let neighbor_index = neighbor as usize;
            let before = self.slot_adj[neighbor_index].len();
            self.slot_adj[neighbor_index].retain(|&n| n != slot);
            if self.slot_adj[neighbor_index].len() != before {
                self.table.add_resident_neighbor(neighbor, -1);
                let _ = self.write_adjacency_row(neighbor);
            }
        }
    }

    /// Rewrites one slot's device adjacency row from the host mirror
    /// (`u32::MAX` pads empty entries).
    fn write_adjacency_row(&mut self, slot: u32) -> Result<(), GpuError> {
        let degree = usize::from(self.config.adjacency_degree);
        let mut row = vec![0xFFu8; degree * 4];
        for (j, neighbor) in self.slot_adj[slot as usize].iter().enumerate() {
            row[j * 4..j * 4 + 4].copy_from_slice(&neighbor.to_le_bytes());
        }
        self.backend
            .copy_in(
                Region::Adjacency,
                u64::from(slot) * (degree as u64) * 4,
                &row,
            )
            .map(|_| ())
    }

    /// Enqueues selection without reading back (the GT4 device-resident
    /// path): results land in device scratch; readiness is polled via
    /// [`Self::selection_ready`], never blocked on.
    pub fn topk_enqueue(
        &mut self,
        query: &[f32],
        k: u16,
        expand_budget: Option<u16>,
        filter: Option<TagFilter>,
    ) -> Result<(), GpuError> {
        if k == 0 || k > MAX_K {
            return Err(GpuError::InvalidConfig {
                detail: format!("k must be in 1..={MAX_K}, got {k}"),
            });
        }
        let fence = self.backend.topk(query, k, expand_budget, filter)?;
        self.selection_fence = Some(fence);
        Ok(())
    }

    /// Enqueues the contiguous gather of the most recent selection into the
    /// materialize region (`[k, page_bytes]`, pad rows zeroed).
    pub fn materialize_enqueue(&mut self) -> Result<(), GpuError> {
        let fence = self.backend.materialize_topk()?;
        self.selection_fence = Some(fence);
        Ok(())
    }

    /// Non-blocking: true when the most recent selection/materialization
    /// has fully landed on the device.
    #[must_use]
    pub fn selection_ready(&self) -> bool {
        use crate::tier::backend::CopyFence;
        self.selection_fence
            .as_ref()
            .is_none_or(CopyFence::is_complete)
    }

    /// The most recent selection/materialization fence (the `DLPack` seam
    /// orders consumer streams against it).
    #[must_use]
    pub fn selection_fence(&self) -> Option<&B::Fence> {
        self.selection_fence.as_ref()
    }

    /// Maps device slots of the most recent selection to page ids (host
    /// bookkeeping only; no device access).
    #[must_use]
    pub fn page_of_slot(&self, slot: u32) -> Option<PageId> {
        self.table.state(slot).map(|state| state.page_id)
    }

    /// Baseline device selection (HT-2): scores every selectable page,
    /// returns the top `k` (and a bounded one-hop expansion when asked) as
    /// page ids. This readback form is the GT3 test surface; GT4 exposes
    /// the device-resident result instead.
    pub fn topk_pages(
        &mut self,
        query: &[f32],
        k: u16,
        expand_budget: Option<u16>,
        filter: Option<TagFilter>,
    ) -> Result<TierTopk, GpuError> {
        if k == 0 || k > MAX_K {
            return Err(GpuError::InvalidConfig {
                detail: format!("k must be in 1..={MAX_K}, got {k}"),
            });
        }
        if let Some(budget) = expand_budget {
            if budget > MAX_EXPAND {
                return Err(GpuError::InvalidConfig {
                    detail: format!("expansion budget must be <= {MAX_EXPAND}, got {budget}"),
                });
            }
        }
        self.backend.topk(query, k, expand_budget, filter)?;
        let readback = self.backend.read_topk()?;
        let mut selected = Vec::with_capacity(readback.selected.len());
        for (slot, score) in readback.selected {
            if let Some(state) = self.table.state(slot) {
                selected.push((state.page_id, score));
                self.table.touch(slot, 1.0);
            }
        }
        let mut expanded = Vec::with_capacity(readback.expanded.len());
        for slot in readback.expanded {
            if let Some(state) = self.table.state(slot) {
                expanded.push(state.page_id);
            }
        }
        Ok(TierTopk { selected, expanded })
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
            // Unselectable on the device first, then host bookkeeping.
            if self
                .backend
                .copy_in(Region::Validity, u64::from(victim), &[0])
                .is_err()
            {
                return; // cannot make it unselectable: do not evict
            }
            self.uninstall_structures(victim);
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
