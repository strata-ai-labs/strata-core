//! The backend seam: what the tier machinery needs from a device.
//!
//! Deliberately minimal — regions of device memory, staged copies whose
//! completion is observable without blocking, and a device-visible validity
//! flag per slot. Two implementations: `host_sim` (plain host memory with
//! test-controlled completion — all CI runs there) and `cuda` (the real
//! `device/` runtime). The tier's correctness never depends on which one is
//! underneath; that is the point.

use crate::GpuError;

/// Selection k ceiling (scratch layouts reserve for it).
pub const MAX_K: u16 = 64;
/// Expansion budget ceiling (scratch layouts reserve for it).
pub const MAX_EXPAND: u16 = 256;

/// The canonical scratch-region size for a given geometry: scores, selected
/// slots + scores, expansion output, cursor, dedup bitmap (word padded), and
/// the staged query vector. Every sizing site uses this one formula.
#[must_use]
pub const fn scratch_bytes(capacity: u64, summary_bytes: u64) -> u64 {
    let dim = summary_bytes / 4;
    capacity * 4                    // scores
        + (MAX_K as u64) * 4        // selected slots
        + (MAX_K as u64) * 4        // selected scores
        + (MAX_EXPAND as u64) * 4   // expansion output
        + 4                         // cursor
        + capacity.div_ceil(32) * 4 // dedup bitmap, word padded
        + dim * 4 // staged query
}

/// An exact-match predicate over one of a page's four metadata tags.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TagFilter {
    /// Which tag (0..=3).
    pub index: u8,
    /// The value it must equal.
    pub value: u64,
}

/// One selection request (HT-2: results stay on device; this readback type
/// is the test/oracle path until the GT4 `DLPack` seam).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TopkReadback {
    /// Selected `(slot, score)`, best first (score-descending, lower slot
    /// on ties).
    pub selected: Vec<(u32, f32)>,
    /// One-hop expansion of the selected set (deduplicated, order
    /// unspecified), when an expansion budget was given.
    pub expanded: Vec<u32>,
}

/// Identifies one of the tier's fixed regions on the device.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Region {
    /// The page pool (slot-sized blobs).
    Pages,
    /// Per-slot summary blobs.
    Summaries,
    /// Per-slot bounded-degree adjacency rows (`degree` u32 slots each;
    /// `u32::MAX` marks an empty entry).
    Adjacency,
    /// Per-slot selectability byte (1 = selectable by kernels).
    Validity,
    /// Per-slot metadata tags (4 × u64).
    Tags,
    /// Kernel scratch: scores, selection output, cursor, dedup bitmap.
    Scratch,
    /// Materialized selection: the top-k pages gathered contiguously
    /// (`MAX_K * page_bytes`; the stock-attention consumption path).
    Materialize,
}

/// Byte sizes for the tier's regions, fixed at open.
#[derive(Clone, Copy, Debug)]
pub struct RegionBytes {
    /// Page pool bytes.
    pub pages: u64,
    /// Summary region bytes.
    pub summaries: u64,
    /// Adjacency region bytes.
    pub adjacency: u64,
    /// Validity region bytes (one byte per slot).
    pub validity: u64,
    /// Tags region bytes (32 per slot).
    pub tags: u64,
    /// Kernel scratch bytes.
    pub scratch: u64,
    /// Materialize region bytes (`MAX_K * page_bytes`).
    pub materialize: u64,
}

/// A pending copy's completion handle. Non-blocking by construction: the
/// tier only ever *polls* copies (the zero-implicit-sync rule); there is no
/// blocking wait on this trait at all.
pub trait CopyFence {
    /// True once the copy's bytes are fully visible to device consumers.
    fn is_complete(&self) -> bool;
}

/// What the tier machinery requires of a device.
///
/// Offsets are byte offsets within a [`Region`]; the backend owns the base
/// addresses. Copies are **staged**: the backend takes the bytes eagerly
/// (from its own staging arrangements) and completion is observed through
/// the returned fence.
pub trait DeviceBackend {
    /// The backend's copy-completion fence.
    type Fence: CopyFence;

    /// Reserves the tier's regions. Called exactly once, at open.
    fn reserve(&mut self, bytes: RegionBytes) -> Result<(), GpuError>;

    /// Enqueues a copy of `bytes` into `region` at `offset` on the
    /// promotion lane. Returns a pollable fence.
    fn copy_in(
        &mut self,
        region: Region,
        offset: u64,
        bytes: &[u8],
    ) -> Result<Self::Fence, GpuError>;

    /// Records a fence on the promotion lane capturing all copies enqueued
    /// so far (used as the per-epoch reuse fence).
    fn fence_now(&mut self) -> Result<Self::Fence, GpuError>;

    /// Reads back `len` bytes from a region (test oracles and, later, the
    /// write-behind path; never the decode loop). Takes `&mut self`: real
    /// backends stage through their pinned slab.
    fn read_back(&mut self, region: Region, offset: u64, len: usize) -> Result<Vec<u8>, GpuError>;

    /// Runs baseline selection over the resident set: scores every
    /// selectable slot (dot product of `query` against its summary,
    /// masked by `filter`), takes the top `k`, and — when `expand_budget`
    /// is given — expands one hop through the adjacency table, deduplicated
    /// against the selection. Results land in device scratch (HT-2); the
    /// returned fence completes when they are consumable.
    fn topk(
        &mut self,
        query: &[f32],
        k: u16,
        expand_budget: Option<u16>,
        filter: Option<TagFilter>,
    ) -> Result<Self::Fence, GpuError>;

    /// Reads the most recent selection back to the host (test/oracle path;
    /// GT4 exposes the device-resident form instead). Blocks until ready.
    fn read_topk(&mut self) -> Result<TopkReadback, GpuError>;

    /// Gathers the most recent selection's pages contiguously into the
    /// materialize region, in selection order (pad slots zero-fill). The
    /// stock-attention path: consumers read `[k, page_bytes]` without any
    /// custom kernel. Stream-ordered after the selection.
    fn materialize_topk(&mut self) -> Result<Self::Fence, GpuError>;
}
