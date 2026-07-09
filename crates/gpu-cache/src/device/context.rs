//! Device context: driver load, device selection, capability floor.

use std::sync::Arc;

use tracing::info;

use crate::device::driver::{
    CuContext, DriverApi, CU_DEVICE_ATTRIBUTE_COMPUTE_CAPABILITY_MAJOR,
    CU_DEVICE_ATTRIBUTE_COMPUTE_CAPABILITY_MINOR,
};
use crate::device::error::GpuError;

/// Consumer-GPU floor (HT-8): Ampere and newer.
const COMPUTE_CAPABILITY_FLOOR: (i32, i32) = (8, 0);

/// Static facts about the selected device.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeviceFacts {
    /// Marketing name reported by the driver.
    pub name: String,
    /// Compute capability (major, minor).
    pub compute_capability: (i32, i32),
    /// Total device memory in bytes.
    pub total_memory: u64,
}

/// One initialized device context.
///
/// Owns the driver handle and a retained reference to device 0's *primary*
/// context — the context the CUDA runtime shares, so torch and the tier see
/// one context and handles stay valid regardless of who initialized last.
/// (Edge is single-GPU by design — multi-device is out of scope.)
pub struct GpuContext {
    api: Arc<DriverApi>,
    ctx: CuContext,
    device: crate::device::driver::CuDevice,
    facts: DeviceFacts,
}

// SAFETY: the context handle is used through the thread-safe driver API and
// destroyed exactly once on drop.
unsafe impl Send for GpuContext {}
unsafe impl Sync for GpuContext {}

impl GpuContext {
    /// Loads the driver, selects device 0, verifies the capability floor,
    /// and creates the context.
    pub fn init() -> Result<Self, GpuError> {
        let api = Arc::new(DriverApi::load()?);
        if api.device_count()? == 0 {
            return Err(GpuError::NoDevice);
        }
        let device = api.device(0)?;
        let major = api.device_attribute(CU_DEVICE_ATTRIBUTE_COMPUTE_CAPABILITY_MAJOR, device)?;
        let minor = api.device_attribute(CU_DEVICE_ATTRIBUTE_COMPUTE_CAPABILITY_MINOR, device)?;
        if (major, minor) < COMPUTE_CAPABILITY_FLOOR {
            return Err(GpuError::ComputeCapability {
                major,
                minor,
                floor: COMPUTE_CAPABILITY_FLOOR,
            });
        }
        let name = api.device_name(device)?;
        let total_memory = api.device_total_mem(device)? as u64;
        let ctx = api.primary_ctx_retain(device)?;
        api.ctx_set_current(ctx)?;
        info!(
            device = %name,
            compute_capability = format_args!("{major}.{minor}"),
            total_memory,
            "GPU context initialized"
        );
        Ok(Self {
            api,
            ctx,
            device,
            facts: DeviceFacts {
                name,
                compute_capability: (major, minor),
                total_memory,
            },
        })
    }

    /// Facts about the selected device.
    #[must_use]
    pub fn facts(&self) -> &DeviceFacts {
        &self.facts
    }

    /// Free and total device memory right now, in bytes.
    pub fn memory_info(&self) -> Result<(u64, u64), GpuError> {
        let (free, total) = self.api.mem_get_info()?;
        Ok((free as u64, total as u64))
    }

    /// Number of synchronous host waits issued through this runtime — the
    /// tier's zero-implicit-sync rule is asserted against this counter.
    #[must_use]
    pub fn sync_call_count(&self) -> u64 {
        self.api.sync_call_count()
    }

    /// Blocks until all device work completes. Counted as a sync wait; test
    /// and shutdown paths only.
    pub fn synchronize(&self) -> Result<(), GpuError> {
        self.api.ctx_synchronize()
    }

    /// Re-binds the shared primary context as this thread's current
    /// context. Backend entry points call this because interop consumers
    /// (torch) rebind the current context between our calls.
    pub(crate) fn make_current(&self) -> Result<(), GpuError> {
        self.api.ctx_set_current(self.ctx)
    }

    pub(crate) fn api(&self) -> &Arc<DriverApi> {
        &self.api
    }
}

impl Drop for GpuContext {
    fn drop(&mut self) {
        if let Err(error) = self.api.primary_ctx_release(self.device) {
            tracing::warn!(%error, "failed to release the GPU primary context");
        }
    }
}
