//! The real [`DeviceBackend`]: tier machinery over the CUDA runtime.
//!
//! GT1 scaffolding with one honest simplification: a single pinned staging
//! slab, so a copy must wait (by *polling*, never a counted sync) for the
//! previous copy to complete before reusing the slab. GT4 replaces this
//! with the ring the design describes; the trait seam is unchanged.

use std::sync::Arc;

use crate::device::arena::{DeviceArena, RegionSpec, SlotRegion};
use crate::device::context::GpuContext;
use crate::device::error::GpuError;
use crate::device::pinned::PinnedBuffer;
use crate::device::stream::{Event, Stream};
use crate::tier::backend::{CopyFence, DeviceBackend, Region, RegionBytes};

/// Completion fence over a CUDA event (polled, never blocked on).
#[derive(Clone)]
pub struct CudaFence(Arc<Event>);

impl CopyFence for CudaFence {
    fn is_complete(&self) -> bool {
        // A query failure is unrecoverable driver trouble; treating it as
        // incomplete fails safe (the gate simply never opens).
        self.0.is_complete().unwrap_or(false)
    }
}

/// The CUDA device backend.
pub struct CudaBackend {
    context: GpuContext,
    stream: Stream,
    arena: Option<DeviceArena>,
    staging: PinnedBuffer,
    /// The most recent copy's event: the slab is reusable once it completes.
    slab_busy_until: Option<CudaFence>,
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
            staging,
            slab_busy_until: None,
        })
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
            ],
        )?;
        self.arena = Some(arena);
        Ok(())
    }

    fn copy_in(
        &mut self,
        region: Region,
        offset: u64,
        bytes: &[u8],
    ) -> Result<Self::Fence, GpuError> {
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
        Ok(CudaFence(Arc::new(self.stream.record()?)))
    }

    fn read_back(&mut self, region: Region, offset: u64, len: usize) -> Result<Vec<u8>, GpuError> {
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
}
