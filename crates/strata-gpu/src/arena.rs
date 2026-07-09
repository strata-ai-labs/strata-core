//! Pre-reserved device arena.
//!
//! One `cuMemAlloc` of the full tier budget at init, carved into named
//! regions; after init the tier never calls the CUDA allocator again, so it
//! cannot fragment device memory or steal VRAM from the hosting model
//! (HT-7). Fixed-size slot allocation inside a region is pure host math and
//! unit-tests without hardware.

use crate::context::GpuContext;
use crate::driver::{DevicePtr, DriverApi};
use crate::error::GpuError;
use std::sync::Arc;

/// Region alignment: matches the design's page alignment (§4) and satisfies
/// every vectorized access width the kernels use.
const REGION_ALIGN: u64 = 256;

/// One named region request.
#[derive(Clone, Copy, Debug)]
pub struct RegionSpec {
    /// Region name (page pool, summaries, adjacency, tables).
    pub name: &'static str,
    /// Requested bytes; rounded up to the region alignment.
    pub bytes: u64,
}

/// A carved region of the arena.
#[derive(Clone, Copy, Debug)]
pub struct SlotRegion {
    /// Region name.
    pub name: &'static str,
    /// Device base address (256-aligned).
    pub base: DevicePtr,
    /// Region length in bytes.
    pub len: u64,
}

/// The reserved arena.
pub struct DeviceArena {
    api: Arc<DriverApi>,
    base: DevicePtr,
    total: u64,
    regions: Vec<SlotRegion>,
}

impl DeviceArena {
    /// Reserves the sum of `specs` (aligned) in one allocation and carves
    /// the regions in order.
    pub fn reserve(context: &GpuContext, specs: &[RegionSpec]) -> Result<Self, GpuError> {
        let layout = carve(specs)?;
        let total = layout.total;
        let api = Arc::clone(context.api());
        let base =
            api.mem_alloc(usize::try_from(total).map_err(|_| GpuError::InvalidConfig {
                detail: format!("arena budget {total} exceeds the platform address width"),
            })?)?;
        let regions = layout
            .offsets
            .into_iter()
            .map(|(spec, offset, len)| SlotRegion {
                name: spec,
                base: base + offset,
                len,
            })
            .collect();
        Ok(Self {
            api,
            base,
            total,
            regions,
        })
    }

    /// Total reserved bytes.
    #[must_use]
    pub const fn total_bytes(&self) -> u64 {
        self.total
    }

    /// Looks up a carved region by name.
    #[must_use]
    pub fn region(&self, name: &str) -> Option<SlotRegion> {
        self.regions.iter().copied().find(|r| r.name == name)
    }

    /// Zero-fills a region (used at init for validity bitmaps and tables).
    pub fn zero_region(&self, region: SlotRegion) -> Result<(), GpuError> {
        self.api.memset_d8(
            region.base,
            0,
            usize::try_from(region.len).unwrap_or(usize::MAX),
        )
    }
}

impl Drop for DeviceArena {
    fn drop(&mut self) {
        if let Err(error) = self.api.mem_free(self.base) {
            tracing::warn!(%error, "failed to free device arena");
        }
    }
}

struct CarvedLayout {
    total: u64,
    offsets: Vec<(&'static str, u64, u64)>,
}

fn carve(specs: &[RegionSpec]) -> Result<CarvedLayout, GpuError> {
    if specs.is_empty() {
        return Err(GpuError::InvalidConfig {
            detail: "arena needs at least one region".to_owned(),
        });
    }
    let mut offsets = Vec::with_capacity(specs.len());
    let mut cursor = 0u64;
    for spec in specs {
        if spec.bytes == 0 {
            return Err(GpuError::InvalidConfig {
                detail: format!("region `{}` requests zero bytes", spec.name),
            });
        }
        let len = spec
            .bytes
            .checked_next_multiple_of(REGION_ALIGN)
            .ok_or_else(|| GpuError::InvalidConfig {
                detail: format!("region `{}` size overflows alignment", spec.name),
            })?;
        offsets.push((spec.name, cursor, len));
        cursor = cursor
            .checked_add(len)
            .ok_or_else(|| GpuError::InvalidConfig {
                detail: "arena regions overflow u64".to_owned(),
            })?;
    }
    Ok(CarvedLayout {
        total: cursor,
        offsets,
    })
}

/// Fixed-size slot allocator over a region: O(1) alloc/free with zero
/// fragmentation (every slot is the same size — the page-pool property the
/// design leans on). Pure host state; the device only ever sees
/// `base + slot * slot_bytes`.
#[derive(Debug)]
pub struct SlotAllocator {
    base: DevicePtr,
    slot_bytes: u64,
    capacity: u32,
    free: Vec<u32>,
}

impl SlotAllocator {
    /// Builds an allocator over `region` with fixed `slot_bytes` slots.
    pub fn new(region: SlotRegion, slot_bytes: u64) -> Result<Self, GpuError> {
        if slot_bytes == 0 || slot_bytes % REGION_ALIGN != 0 {
            return Err(GpuError::InvalidConfig {
                detail: format!(
                    "slot size {slot_bytes} must be a nonzero multiple of {REGION_ALIGN}"
                ),
            });
        }
        let capacity = u32::try_from(region.len / slot_bytes).unwrap_or(u32::MAX);
        if capacity == 0 {
            return Err(GpuError::ArenaExhausted {
                region: region.name,
                requested: slot_bytes,
                available: region.len,
            });
        }
        // LIFO free list: recently-freed slots are re-used first, which keeps
        // the hot end of the pool dense.
        let free = (0..capacity).rev().collect();
        Ok(Self {
            base: region.base,
            slot_bytes,
            capacity,
            free,
        })
    }

    /// Total slot count.
    #[must_use]
    pub const fn capacity(&self) -> u32 {
        self.capacity
    }

    /// Slots currently allocated.
    #[must_use]
    pub fn allocated(&self) -> u32 {
        self.capacity - u32::try_from(self.free.len()).unwrap_or(u32::MAX)
    }

    /// Takes a free slot, or `None` when the pool is full (the caller's
    /// eviction policy decides what happens next — never this allocator).
    pub fn alloc(&mut self) -> Option<u32> {
        self.free.pop()
    }

    /// Returns a slot to the pool.
    ///
    /// The caller (the page table) is responsible for ensuring the slot is
    /// fence-safe to reuse (design §5); the allocator only tracks liveness.
    pub fn release(&mut self, slot: u32) {
        debug_assert!(slot < self.capacity, "slot {slot} out of range");
        self.free.push(slot);
    }

    /// Device address of a slot.
    #[must_use]
    pub const fn slot_ptr(&self, slot: u32) -> DevicePtr {
        self.base + (slot as u64) * self.slot_bytes
    }
}

#[cfg(test)]
mod tests {
    use super::{carve, RegionSpec, SlotAllocator, SlotRegion, REGION_ALIGN};

    #[test]
    fn carve_aligns_and_orders_regions() {
        let layout = carve(&[
            RegionSpec {
                name: "pages",
                bytes: 1000,
            },
            RegionSpec {
                name: "summaries",
                bytes: 256,
            },
            RegionSpec {
                name: "tables",
                bytes: 1,
            },
        ])
        .expect("valid layout");
        assert_eq!(layout.offsets[0].1, 0);
        assert_eq!(layout.offsets[0].2, 1024); // 1000 -> 1024
        assert_eq!(layout.offsets[1].1, 1024);
        assert_eq!(layout.offsets[1].2, 256);
        assert_eq!(layout.offsets[2].1, 1280);
        assert_eq!(layout.offsets[2].2, REGION_ALIGN);
        assert_eq!(layout.total, 1280 + REGION_ALIGN);
    }

    #[test]
    fn carve_rejects_zero_and_empty() {
        assert!(carve(&[]).is_err());
        assert!(carve(&[RegionSpec {
            name: "pages",
            bytes: 0
        }])
        .is_err());
    }

    #[test]
    fn slot_allocator_round_trips_all_slots() {
        let region = SlotRegion {
            name: "pages",
            base: 0x1000,
            len: 4096,
        };
        let mut alloc = SlotAllocator::new(region, 1024).expect("valid allocator");
        assert_eq!(alloc.capacity(), 4);
        let mut slots = Vec::new();
        while let Some(slot) = alloc.alloc() {
            slots.push(slot);
        }
        assert_eq!(slots.len(), 4);
        assert_eq!(alloc.allocated(), 4);
        assert!(alloc.alloc().is_none(), "pool exhausted");
        // Addresses are disjoint, in-region, and slot-aligned.
        for &slot in &slots {
            let ptr = alloc.slot_ptr(slot);
            assert!(ptr >= 0x1000 && ptr + 1024 <= 0x1000 + 4096);
            assert_eq!((ptr - 0x1000) % 1024, 0);
        }
        alloc.release(slots[1]);
        assert_eq!(alloc.alloc(), Some(slots[1]), "LIFO reuse of freed slot");
    }

    #[test]
    fn slot_allocator_rejects_bad_slot_sizes() {
        let region = SlotRegion {
            name: "pages",
            base: 0,
            len: 4096,
        };
        assert!(SlotAllocator::new(region, 0).is_err());
        assert!(SlotAllocator::new(region, 100).is_err(), "unaligned slot");
        let tiny = SlotRegion {
            name: "pages",
            base: 0,
            len: 100,
        };
        assert!(SlotAllocator::new(tiny, 256).is_err(), "no whole slot fits");
    }
}
