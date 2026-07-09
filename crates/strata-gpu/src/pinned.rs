//! Pinned (page-locked) host memory for the T1 staging tier.
//!
//! Stream-ordered copies require pinned source/destination buffers;
//! allocation happens once at init (never-allocate-again, like the arena).

use std::os::raw::c_void;
use std::sync::Arc;

use crate::context::GpuContext;
use crate::driver::DriverApi;
use crate::error::GpuError;

/// An owned pinned host buffer.
pub struct PinnedBuffer {
    api: Arc<DriverApi>,
    ptr: *mut c_void,
    len: usize,
}

// SAFETY: the buffer is ordinary (page-locked) host memory owned uniquely by
// this struct; the driver handle is thread-safe.
unsafe impl Send for PinnedBuffer {}
unsafe impl Sync for PinnedBuffer {}

impl PinnedBuffer {
    /// Allocates `len` bytes of pinned host memory.
    pub fn alloc(context: &GpuContext, len: usize) -> Result<Self, GpuError> {
        if len == 0 {
            return Err(GpuError::InvalidConfig {
                detail: "pinned buffer length must be nonzero".to_owned(),
            });
        }
        let api = Arc::clone(context.api());
        let ptr = api.mem_host_alloc(len)?;
        Ok(Self { api, ptr, len })
    }

    /// Buffer length in bytes.
    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    /// Always false — zero-length buffers are rejected at allocation.
    #[must_use]
    pub const fn is_empty(&self) -> bool {
        false
    }

    /// Raw pointer for stream-ordered copies.
    #[must_use]
    pub const fn as_ptr(&self) -> *const u8 {
        self.ptr.cast_const().cast::<u8>()
    }

    /// Raw mutable pointer for stream-ordered copies.
    #[must_use]
    pub const fn as_mut_ptr(&self) -> *mut u8 {
        self.ptr.cast::<u8>()
    }

    /// Views the buffer as a byte slice.
    ///
    /// # Safety
    ///
    /// No stream-ordered copy may be writing this buffer concurrently.
    #[must_use]
    pub unsafe fn as_slice(&self) -> &[u8] {
        // SAFETY: ptr is valid for len bytes for the buffer's lifetime;
        // concurrent-write exclusion is the caller's contract.
        unsafe { std::slice::from_raw_parts(self.ptr.cast::<u8>(), self.len) }
    }

    /// Views the buffer as a mutable byte slice.
    ///
    /// # Safety
    ///
    /// No stream-ordered copy may be reading or writing this buffer
    /// concurrently.
    #[must_use]
    pub unsafe fn as_mut_slice(&mut self) -> &mut [u8] {
        // SAFETY: ptr is valid for len bytes; &mut self gives host-side
        // exclusivity, device-side exclusion is the caller's contract.
        unsafe { std::slice::from_raw_parts_mut(self.ptr.cast::<u8>(), self.len) }
    }
}

impl Drop for PinnedBuffer {
    fn drop(&mut self) {
        if let Err(error) = self.api.mem_free_host(self.ptr) {
            tracing::warn!(%error, "failed to free pinned buffer");
        }
    }
}
