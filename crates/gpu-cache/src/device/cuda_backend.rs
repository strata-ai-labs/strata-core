//! The real [`DeviceBackend`]: tier machinery over the CUDA runtime.
//!
//! GT1 scaffolding with one honest simplification: a single pinned staging
//! slab, so a copy must wait (by *polling*, never a counted sync) for the
//! previous copy to complete before reusing the slab. GT4 replaces this
//! with the ring the design describes; the trait seam is unchanged.

use std::sync::Arc;

use std::os::raw::c_void;

use crate::device::arena::{DeviceArena, RegionSpec, SlotRegion};
use crate::device::context::GpuContext;
use crate::device::error::GpuError;
use crate::device::kernels::{KERNELS, SELECTION_PTX};
use crate::device::module::PtxModule;
use crate::device::pinned::PinnedBuffer;
use crate::device::stream::{Event, Stream};
use crate::tier::backend::{
    CopyFence, DeviceBackend, Region, RegionBytes, TagFilter, TopkReadback,
};

use crate::tier::backend::{MAX_EXPAND as MAX_EXPAND_U16, MAX_K as MAX_K_U16};

const MAX_K: usize = MAX_K_U16 as usize;
const MAX_EXPAND: usize = MAX_EXPAND_U16 as usize;

/// Geometry derived from the reserved regions, plus the scratch layout.
#[derive(Clone, Copy, Debug)]
struct Plan {
    capacity: usize,
    dim: usize,
    degree: usize,
    page_bytes: usize,
    scores_off: u64,
    sel_slots_off: u64,
    sel_scores_off: u64,
    out_off: u64,
    cursor_off: u64,
    bitmap_off: u64,
    bitmap_len: usize,
    query_off: u64,
    cand_a_scores_off: u64,
    cand_a_slots_off: u64,
    cand_b_scores_off: u64,
    cand_b_slots_off: u64,
    blocks: u32,
}

impl Plan {
    #[allow(
        clippy::similar_names,
        reason = "cand_a_*/cand_b_* are the two halves of one ping-pong pair"
    )]
    fn derive(bytes: RegionBytes) -> Result<Self, GpuError> {
        let capacity = usize::try_from(bytes.validity).map_err(|_| GpuError::InvalidConfig {
            detail: "validity region exceeds host width".to_owned(),
        })?;
        if capacity == 0 {
            return Err(GpuError::InvalidConfig {
                detail: "empty validity region".to_owned(),
            });
        }
        let dim = usize::try_from(bytes.summaries).unwrap_or(0) / capacity / 4;
        let degree = usize::try_from(bytes.adjacency).unwrap_or(0) / capacity / 4;
        let page_bytes = usize::try_from(bytes.pages).unwrap_or(0) / capacity;
        let scores_off = 0u64;
        let sel_slots_off = scores_off + (capacity as u64) * 4;
        let sel_scores_off = sel_slots_off + (MAX_K as u64) * 4;
        let out_off = sel_scores_off + (MAX_K as u64) * 4;
        let cursor_off = out_off + (MAX_EXPAND as u64) * 4;
        let bitmap_off = cursor_off + 4;
        let bitmap_len = capacity.div_ceil(32) * 4;
        let query_off = bitmap_off + bitmap_len as u64;
        let blocks =
            u32::try_from(capacity.div_ceil(256)).map_err(|_| GpuError::InvalidConfig {
                detail: "capacity exceeds the block-count width".to_owned(),
            })?;
        // Merge-round ping-pong buffers: A holds the per-block shortlists
        // (blocks * MAX_K), B the next round's output (one entry set per
        // 256-candidate window).
        let a_entries = u64::from(blocks) * (MAX_K as u64);
        let b_entries = a_entries.div_ceil(256) * (MAX_K as u64);
        let cand_a_scores_off = query_off + (dim as u64) * 4;
        let cand_a_slots_off = cand_a_scores_off + a_entries * 4;
        let cand_b_scores_off = cand_a_slots_off + a_entries * 4;
        let cand_b_slots_off = cand_b_scores_off + b_entries * 4;
        let needed = cand_b_slots_off + b_entries * 4;
        if needed > bytes.scratch {
            return Err(GpuError::InvalidConfig {
                detail: format!(
                    "scratch region of {} bytes cannot hold the {needed}-byte selection layout",
                    bytes.scratch
                ),
            });
        }
        Ok(Self {
            capacity,
            dim,
            degree,
            page_bytes,
            scores_off,
            sel_slots_off,
            sel_scores_off,
            out_off,
            cursor_off,
            bitmap_off,
            bitmap_len,
            query_off,
            cand_a_scores_off,
            cand_a_slots_off,
            cand_b_scores_off,
            cand_b_slots_off,
            blocks,
        })
    }
}

/// Completion fence over a CUDA event (polled, never blocked on).
#[derive(Clone)]
pub struct CudaFence(pub(crate) Arc<Event>);

impl CopyFence for CudaFence {
    fn is_complete(&self) -> bool {
        // A query failure is unrecoverable driver trouble; treating it as
        // incomplete fails safe (the gate simply never opens).
        self.0.is_complete().unwrap_or(false)
    }
}

/// The CUDA device backend.
///
/// Field order is load-bearing: Rust drops fields in declaration order, and
/// every device resource here must be destroyed while the context is still
/// alive — `context` is therefore the last field. (Violating this is a
/// use-after-destroy inside the driver: a segfault, not an error return.)
pub struct CudaBackend {
    stream: Stream,
    arena: Option<DeviceArena>,
    module: Option<PtxModule>,
    plan: Option<Plan>,
    /// The (k, expand budget) of the most recent `topk`, for readback.
    last_topk: Option<(u16, Option<u16>)>,
    staging: PinnedBuffer,
    /// The most recent copy's event: the slab is reusable once it completes.
    slab_busy_until: Option<CudaFence>,
    /// Registered replacement for the baseline selection module (design D3:
    /// consumers replace kernels by registration, not by forking the tier).
    selection_ptx: Option<String>,
    context: GpuContext,
}

impl CudaBackend {
    /// Initializes the device and allocates the staging slab (once — the
    /// never-allocate-after-init rule).
    pub fn new(staging_bytes: usize) -> Result<Self, GpuError> {
        let context = GpuContext::init()?;
        let stream = Stream::new(&context)?;
        let staging = PinnedBuffer::alloc(&context, staging_bytes)?;
        Ok(Self {
            context,
            stream,
            arena: None,
            module: None,
            plan: None,
            last_topk: None,
            staging,
            slab_busy_until: None,
            selection_ptx: None,
        })
    }

    /// Registers a replacement selection module (Moho's fused kernels). The
    /// PTX must define every entry point in
    /// [`SELECTION_KERNEL_NAMES`](crate::SELECTION_KERNEL_NAMES) with the
    /// baseline ABI documented in the Moho guide; resolution is eager at
    /// [`DeviceBackend::reserve`], so a missing or misnamed entry fails the
    /// tier's `open`, never the first decode step. Must be called before
    /// `reserve`.
    pub fn register_selection_ptx(&mut self, ptx: impl Into<String>) -> Result<(), GpuError> {
        if self.module.is_some() {
            return Err(GpuError::InvalidConfig {
                detail: "selection kernels must be registered before reserve".to_owned(),
            });
        }
        self.selection_ptx = Some(ptx.into());
        Ok(())
    }

    /// Device context facts (for logs and tests).
    #[must_use]
    pub fn context(&self) -> &GpuContext {
        &self.context
    }

    fn region_base(&self, region: Region) -> Result<SlotRegion, GpuError> {
        let arena = self.arena.as_ref().ok_or_else(|| GpuError::InvalidConfig {
            detail: "backend used before reserve".to_owned(),
        })?;
        let name = match region {
            Region::Pages => "pages",
            Region::Summaries => "summaries",
            Region::Adjacency => "adjacency",
            Region::Validity => "validity",
            Region::Tags => "tags",
            Region::Scratch => "scratch",
            Region::Materialize => "materialize",
        };
        arena.region(name).ok_or_else(|| GpuError::InvalidConfig {
            detail: format!("region `{name}` missing from the arena"),
        })
    }

    fn wait_slab_free(&mut self) {
        if let Some(fence) = self.slab_busy_until.take() {
            // Polling, not a driver sync: the zero-implicit-sync counter
            // stays untouched. GT4's staging ring removes this wait.
            while !fence.is_complete() {
                std::thread::yield_now();
            }
        }
    }
}

impl DeviceBackend for CudaBackend {
    type Fence = CudaFence;

    fn reserve(&mut self, bytes: RegionBytes) -> Result<(), GpuError> {
        self.context.make_current()?;
        if self.arena.is_some() {
            return Err(GpuError::InvalidConfig {
                detail: "reserve called twice".to_owned(),
            });
        }
        let arena = DeviceArena::reserve(
            &self.context,
            &[
                RegionSpec {
                    name: "pages",
                    bytes: bytes.pages,
                },
                RegionSpec {
                    name: "summaries",
                    bytes: bytes.summaries,
                },
                RegionSpec {
                    name: "adjacency",
                    bytes: bytes.adjacency,
                },
                RegionSpec {
                    name: "validity",
                    bytes: bytes.validity,
                },
                RegionSpec {
                    name: "tags",
                    bytes: bytes.tags,
                },
                RegionSpec {
                    name: "scratch",
                    bytes: bytes.scratch,
                },
                RegionSpec {
                    name: "materialize",
                    bytes: bytes.materialize,
                },
            ],
        )?;
        // Kernels never select a slot whose validity byte is zero; zero the
        // region so freshly reserved slots are unselectable.
        let validity = arena.region("validity").expect("just carved");
        arena.zero_region(validity)?;
        self.plan = Some(Plan::derive(bytes)?);
        let source = self.selection_ptx.as_deref().unwrap_or(SELECTION_PTX);
        self.module = Some(PtxModule::load(&self.context, source, KERNELS)?);
        self.arena = Some(arena);
        Ok(())
    }

    fn copy_in(
        &mut self,
        region: Region,
        offset: u64,
        bytes: &[u8],
    ) -> Result<Self::Fence, GpuError> {
        self.context.make_current()?;
        if bytes.len() > self.staging.len() {
            return Err(GpuError::InvalidConfig {
                detail: format!(
                    "copy of {} bytes exceeds the {}-byte staging slab",
                    bytes.len(),
                    self.staging.len()
                ),
            });
        }
        let base = self.region_base(region)?;
        if offset + bytes.len() as u64 > base.len {
            return Err(GpuError::InvalidConfig {
                detail: format!("copy past region end ({} > {})", offset, base.len),
            });
        }
        self.wait_slab_free();
        // SAFETY: no stream-ordered copy references the slab (wait above);
        // the memcpy into it is plain host memory.
        unsafe {
            self.staging.as_mut_slice()[..bytes.len()].copy_from_slice(bytes);
        }
        // SAFETY: staging is pinned and stays alive; the fence below keeps
        // the slab reserved until the device has consumed it; dst lies inside
        // the region (checked above).
        unsafe {
            self.stream
                .copy_to_device(base.base + offset, self.staging.as_ptr(), bytes.len())?;
        }
        let fence = CudaFence(Arc::new(self.stream.record()?));
        self.slab_busy_until = Some(fence.clone());
        Ok(fence)
    }

    fn fence_now(&mut self) -> Result<Self::Fence, GpuError> {
        self.context.make_current()?;
        Ok(CudaFence(Arc::new(self.stream.record()?)))
    }

    fn read_back(&mut self, region: Region, offset: u64, len: usize) -> Result<Vec<u8>, GpuError> {
        self.context.make_current()?;
        let base = self.region_base(region)?;
        if offset + len as u64 > base.len {
            return Err(GpuError::InvalidConfig {
                detail: format!("read past region end ({offset} + {len} > {})", base.len),
            });
        }
        if len > self.staging.len() {
            return Err(GpuError::InvalidConfig {
                detail: format!(
                    "read of {len} bytes exceeds the {}-byte staging slab",
                    self.staging.len()
                ),
            });
        }
        // Test/write-behind path: a deliberate (counted) wait is correct here.
        self.wait_slab_free();
        // SAFETY: the slab is exclusively ours (wait above) and pinned; src
        // lies inside the region (checked above); the synchronize below
        // completes the copy before the slab is read.
        unsafe {
            self.stream
                .copy_to_host(self.staging.as_mut_ptr(), base.base + offset, len)?;
        }
        self.stream.synchronize()?;
        // SAFETY: the stream drained; no copy references the slab.
        let bytes = unsafe { self.staging.as_slice()[..len].to_vec() };
        Ok(bytes)
    }

    #[allow(
        clippy::too_many_lines,
        reason = "one linear kernel pipeline: stage query, zero scratch, score, select, seed, expand"
    )]
    fn topk(
        &mut self,
        query: &[f32],
        k: u16,
        expand_budget: Option<u16>,
        filter: Option<TagFilter>,
    ) -> Result<Self::Fence, GpuError> {
        self.context.make_current()?;
        let plan = self.plan.ok_or_else(|| GpuError::InvalidConfig {
            detail: "topk before reserve".to_owned(),
        })?;
        if query.len() != plan.dim {
            return Err(GpuError::InvalidConfig {
                detail: format!(
                    "query has {} dims, summaries have {}",
                    query.len(),
                    plan.dim
                ),
            });
        }
        if usize::from(k) > MAX_K {
            return Err(GpuError::InvalidConfig {
                detail: format!("k {k} exceeds the scratch ceiling {MAX_K}"),
            });
        }
        let budget = expand_budget.map_or(0, usize::from);
        if budget > MAX_EXPAND {
            return Err(GpuError::InvalidConfig {
                detail: format!("budget {budget} exceeds the scratch ceiling {MAX_EXPAND}"),
            });
        }

        let scratch = self.region_base(Region::Scratch)?;
        let pages_capacity = u32::try_from(plan.capacity).expect("capacity fits u32");

        // Stage the query into scratch (stream-ordered before the kernels).
        let mut query_bytes = Vec::with_capacity(query.len() * 4);
        for q in query {
            query_bytes.extend_from_slice(&q.to_le_bytes());
        }
        self.copy_in(Region::Scratch, plan.query_off, &query_bytes)?;

        // Zero the cursor + dedup bitmap (contiguous by layout).
        self.stream
            .memset(scratch.base + plan.cursor_off, 0, 4 + plan.bitmap_len)?;

        let module = self.module.as_ref().expect("loaded at reserve");
        let summaries = self.region_base(Region::Summaries)?.base;
        let valid = self.region_base(Region::Validity)?.base;
        let tags = self.region_base(Region::Tags)?.base;
        let adjacency = self.region_base(Region::Adjacency)?.base;

        let mut p_scores = scratch.base + plan.scores_off;
        let mut p_summaries = summaries;
        let mut p_valid = valid;
        let mut p_tags = tags;
        let mut p_query = scratch.base + plan.query_off;
        let mut p_capacity = pages_capacity;
        let mut p_dim = u32::try_from(plan.dim).expect("dim fits u32");
        let (mut p_findex, mut p_fvalue) =
            filter.map_or((u32::MAX, 0u64), |f| (u32::from(f.index), f.value));
        let mut score_params: [*mut c_void; 9] = [
            (&raw mut p_scores).cast(),
            (&raw mut p_summaries).cast(),
            (&raw mut p_valid).cast(),
            (&raw mut p_tags).cast(),
            (&raw mut p_query).cast(),
            (&raw mut p_capacity).cast(),
            (&raw mut p_dim).cast(),
            (&raw mut p_findex).cast(),
            (&raw mut p_fvalue).cast(),
        ];
        let grid = (pages_capacity.div_ceil(256).max(1), 1, 1);
        // SAFETY: parameter layouts match the kernel signatures in
        // kernels.rs; all addresses lie inside reserved regions.
        unsafe {
            module.launch(
                "score_slots",
                grid,
                (256, 1, 1),
                0,
                &self.stream,
                &mut score_params,
            )?;
        }

        // Exact hierarchical top-k: per-block bitonic shortlists, then
        // merge rounds over 256-candidate windows (the grid shrinks ~4x per
        // round at k=64) until a single block writes the final selection.
        let mut p_k = u32::from(k);
        let mut p_cand_scores = scratch.base + plan.cand_a_scores_off;
        let mut p_cand_slots = scratch.base + plan.cand_a_slots_off;
        let mut block_params: [*mut c_void; 5] = [
            (&raw mut p_scores).cast(),
            (&raw mut p_capacity).cast(),
            (&raw mut p_k).cast(),
            (&raw mut p_cand_scores).cast(),
            (&raw mut p_cand_slots).cast(),
        ];
        // SAFETY: as above.
        unsafe {
            module.launch(
                "block_topk",
                (plan.blocks, 1, 1),
                (256, 1, 1),
                0,
                &self.stream,
                &mut block_params,
            )?;
        }

        let buffers = [
            (plan.cand_a_scores_off, plan.cand_a_slots_off),
            (plan.cand_b_scores_off, plan.cand_b_slots_off),
        ];
        let mut p_sel_slots = scratch.base + plan.sel_slots_off;
        let p_sel_scores = scratch.base + plan.sel_scores_off;
        let mut source = 0usize;
        let mut count = plan.blocks * u32::from(k);
        loop {
            let grid = count.div_ceil(256).max(1);
            let last_round = grid == 1;
            let mut p_src_scores = scratch.base + buffers[source].0;
            let mut p_src_slots = scratch.base + buffers[source].1;
            let mut p_count = count;
            let (mut p_out_slots, mut p_out_scores) = if last_round {
                (p_sel_slots, p_sel_scores)
            } else {
                (
                    scratch.base + buffers[1 - source].1,
                    scratch.base + buffers[1 - source].0,
                )
            };
            let mut merge_params: [*mut c_void; 6] = [
                (&raw mut p_src_scores).cast(),
                (&raw mut p_src_slots).cast(),
                (&raw mut p_count).cast(),
                (&raw mut p_k).cast(),
                (&raw mut p_out_slots).cast(),
                (&raw mut p_out_scores).cast(),
            ];
            // SAFETY: as above.
            unsafe {
                module.launch(
                    "merge_topk",
                    (grid, 1, 1),
                    (256, 1, 1),
                    0,
                    &self.stream,
                    &mut merge_params,
                )?;
            }
            if last_round {
                break;
            }
            count = grid * u32::from(k);
            source = 1 - source;
        }

        if expand_budget.is_some() {
            let mut p_bitmap = scratch.base + plan.bitmap_off;
            let mut seed_params: [*mut c_void; 3] = [
                (&raw mut p_sel_slots).cast(),
                (&raw mut p_k).cast(),
                (&raw mut p_bitmap).cast(),
            ];
            // SAFETY: as above.
            unsafe {
                module.launch(
                    "seed_bitmap",
                    (1, 1, 1),
                    (u32::from(k).max(1), 1, 1),
                    0,
                    &self.stream,
                    &mut seed_params,
                )?;
            }

            let mut p_adj = adjacency;
            let mut p_degree = u32::try_from(plan.degree).expect("degree fits u32");
            let mut p_out = scratch.base + plan.out_off;
            let mut p_cursor = scratch.base + plan.cursor_off;
            let mut p_budget = u32::try_from(budget).expect("budget fits u32");
            let mut expand_params: [*mut c_void; 9] = [
                (&raw mut p_sel_slots).cast(),
                (&raw mut p_k).cast(),
                (&raw mut p_adj).cast(),
                (&raw mut p_degree).cast(),
                (&raw mut p_valid).cast(),
                (&raw mut p_bitmap).cast(),
                (&raw mut p_out).cast(),
                (&raw mut p_cursor).cast(),
                (&raw mut p_budget).cast(),
            ];
            let work = u32::from(k) * u32::try_from(plan.degree).expect("degree fits u32");
            let expand_grid = (work.div_ceil(256).max(1), 1, 1);
            // SAFETY: as above.
            unsafe {
                module.launch(
                    "expand",
                    expand_grid,
                    (256, 1, 1),
                    0,
                    &self.stream,
                    &mut expand_params,
                )?;
            }
        }

        self.last_topk = Some((k, expand_budget));
        self.fence_now()
    }

    fn read_topk(&mut self) -> Result<TopkReadback, GpuError> {
        self.context.make_current()?;
        let plan = self.plan.ok_or_else(|| GpuError::InvalidConfig {
            detail: "read_topk before reserve".to_owned(),
        })?;
        let (k, expand_budget) = self.last_topk.ok_or_else(|| GpuError::InvalidConfig {
            detail: "read_topk before any topk".to_owned(),
        })?;

        let slots_raw = self.read_back(Region::Scratch, plan.sel_slots_off, usize::from(k) * 4)?;
        let scores_raw =
            self.read_back(Region::Scratch, plan.sel_scores_off, usize::from(k) * 4)?;
        let mut selected = Vec::with_capacity(usize::from(k));
        for i in 0..usize::from(k) {
            let slot = u32::from_le_bytes(slots_raw[i * 4..i * 4 + 4].try_into().expect("u32"));
            if slot == u32::MAX {
                break; // fewer than k selectable slots: padded tail
            }
            let score = f32::from_le_bytes(scores_raw[i * 4..i * 4 + 4].try_into().expect("f32"));
            selected.push((slot, score));
        }

        let mut expanded = Vec::new();
        if let Some(budget) = expand_budget {
            let cursor_raw = self.read_back(Region::Scratch, plan.cursor_off, 4)?;
            let cursor = u32::from_le_bytes(cursor_raw.try_into().expect("u32"));
            let count = cursor.min(u32::from(budget)) as usize;
            if count > 0 {
                let out_raw = self.read_back(Region::Scratch, plan.out_off, count * 4)?;
                for i in 0..count {
                    expanded.push(u32::from_le_bytes(
                        out_raw[i * 4..i * 4 + 4].try_into().expect("u32"),
                    ));
                }
            }
        }
        Ok(TopkReadback { selected, expanded })
    }

    fn materialize_topk(&mut self) -> Result<Self::Fence, GpuError> {
        self.context.make_current()?;
        let plan = self.plan.ok_or_else(|| GpuError::InvalidConfig {
            detail: "materialize before reserve".to_owned(),
        })?;
        let (k, _) = self.last_topk.ok_or_else(|| GpuError::InvalidConfig {
            detail: "materialize before any topk".to_owned(),
        })?;
        let scratch = self.region_base(Region::Scratch)?;
        let pool = self.region_base(Region::Pages)?;
        let out = self.region_base(Region::Materialize)?;
        let words = u32::try_from(plan.page_bytes / 4).expect("page words fit u32");

        let mut p_slots = scratch.base + plan.sel_slots_off;
        let mut p_k = u32::from(k);
        let mut p_pool = pool.base;
        let mut p_words = words;
        let mut p_out = out.base;
        let mut params: [*mut c_void; 5] = [
            (&raw mut p_slots).cast(),
            (&raw mut p_k).cast(),
            (&raw mut p_pool).cast(),
            (&raw mut p_words).cast(),
            (&raw mut p_out).cast(),
        ];
        let work = u32::from(k) * words;
        let grid = (work.div_ceil(256).max(1), 1, 1);
        let module = self.module.as_ref().expect("loaded at reserve");
        // SAFETY: parameter layout matches gather_pages in kernels.rs; all
        // addresses lie inside reserved regions.
        unsafe {
            module.launch(
                "gather_pages",
                grid,
                (256, 1, 1),
                0,
                &self.stream,
                &mut params,
            )?;
        }
        self.fence_now()
    }
}

/// Static geometry of the device page pool, for consumers that gather
/// pages directly via the block table (fused paged-attention kernels)
/// instead of the materialized copy. The base is valid from `reserve` to
/// drop (the arena never moves); a *slot's contents* are stable only under
/// the epoch contract — a selected slot cannot be evicted or overwritten
/// while its epoch is pinned.
#[derive(Clone, Copy, Debug)]
pub struct PagePoolAddress {
    /// Device base address of slot 0.
    pub base: u64,
    /// Slot count.
    pub slots: u32,
    /// Bytes per slot (the page size).
    pub page_bytes: usize,
}

/// Device-address handles for the most recent selection — the raw material
/// of the GT4 `DLPack` seam. Addresses stay valid until the next `reserve`
/// (the arena never moves); *contents* are stable until the next `topk`.
#[derive(Clone, Copy, Debug)]
pub struct SelectionAddresses {
    /// Selected slot indices (i32; -1 pads), `k` entries.
    pub slots_ptr: u64,
    /// Selection scores (f32), `k` entries.
    pub scores_ptr: u64,
    /// Materialized pages (u8), `k * page_bytes` (valid after
    /// `materialize_topk`).
    pub materialized_ptr: u64,
    /// The `k` of the most recent selection.
    pub k: u16,
    /// Page size in bytes.
    pub page_bytes: usize,
}

impl CudaBackend {
    /// Addresses of the most recent selection's device buffers.
    pub fn selection_addresses(&self) -> Result<SelectionAddresses, GpuError> {
        let plan = self.plan.ok_or_else(|| GpuError::InvalidConfig {
            detail: "selection_addresses before reserve".to_owned(),
        })?;
        let (k, _) = self.last_topk.ok_or_else(|| GpuError::InvalidConfig {
            detail: "selection_addresses before any topk".to_owned(),
        })?;
        let scratch = self.region_base(Region::Scratch)?;
        let out = self.region_base(Region::Materialize)?;
        Ok(SelectionAddresses {
            slots_ptr: scratch.base + plan.sel_slots_off,
            scores_ptr: scratch.base + plan.sel_scores_off,
            materialized_ptr: out.base,
            k,
            page_bytes: plan.page_bytes,
        })
    }

    /// Device address and geometry of the page pool (the zero-copy
    /// consumption path — see [`PagePoolAddress`]).
    pub fn pool_address(&self) -> Result<PagePoolAddress, GpuError> {
        let plan = self.plan.ok_or_else(|| GpuError::InvalidConfig {
            detail: "pool_address before reserve".to_owned(),
        })?;
        let pool = self.region_base(Region::Pages)?;
        Ok(PagePoolAddress {
            base: pool.base,
            slots: u32::try_from(plan.capacity).unwrap_or(u32::MAX),
            page_bytes: plan.page_bytes,
        })
    }

    /// CUDA device ordinal (for the `__dlpack_device__` protocol).
    #[must_use]
    pub fn device_ordinal(&self) -> i32 {
        0 // single-device by design (edge)
    }

    /// Makes an external (consumer) stream wait for `fence` without any
    /// host synchronization — the `__dlpack__(stream=...)` contract.
    ///
    /// # Safety
    ///
    /// `raw_stream` must be a valid CUDA stream handle in this context (the
    /// integer torch passes to `__dlpack__`).
    pub unsafe fn wait_external_stream(
        &self,
        raw_stream: u64,
        fence: &CudaFence,
    ) -> Result<(), GpuError> {
        self.context.make_current()?;
        // SAFETY: forwarded contract — see function docs; the event is live
        // for the fence's lifetime.
        unsafe { fence.0.wait_on_raw_stream(raw_stream, self.context.api()) }
    }
}
