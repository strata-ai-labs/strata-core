//! The backend seam: what the tier machinery needs from a device.
//!
//! Deliberately minimal — regions of device memory, staged copies whose
//! completion is observable without blocking, and a device-visible validity
//! flag per slot. Two implementations: `host_sim` (plain host memory with
//! test-controlled completion — all CI runs there) and `cuda` (the real
//! `device/` runtime). The tier's correctness never depends on which one is
//! underneath; that is the point.

use crate::GpuError;

/// Identifies one of the tier's fixed regions on the device.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Region {
    /// The page pool (slot-sized blobs).
    Pages,
    /// Per-slot summary blobs.
    Summaries,
    /// Per-slot bounded-degree adjacency rows.
    Adjacency,
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
}
