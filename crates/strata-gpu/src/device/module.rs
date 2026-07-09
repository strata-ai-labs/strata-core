//! PTX module JIT and kernel lookup.
//!
//! PTX source is embedded as strings and JIT-compiled by the driver at load
//! (the strata-inference pattern). PTX must be ASCII and NUL-terminated;
//! both are validated here rather than trusted.

use std::collections::HashMap;
use std::ffi::CString;
use std::os::raw::c_void;
use std::sync::Arc;

use crate::device::context::GpuContext;
use crate::device::driver::{CuFunction, CuModule, DriverApi};
use crate::device::error::GpuError;
use crate::device::stream::Stream;

/// A JIT-compiled PTX module with named kernels resolved eagerly.
pub struct PtxModule {
    api: Arc<DriverApi>,
    module: CuModule,
    functions: HashMap<&'static str, CuFunction>,
}

// SAFETY: module/function handles are used through the thread-safe driver
// API; the module is unloaded exactly once on drop.
unsafe impl Send for PtxModule {}
unsafe impl Sync for PtxModule {}

impl PtxModule {
    /// JIT-compiles `ptx` and resolves each kernel in `kernel_names`.
    ///
    /// Eager resolution turns a typo'd kernel name into a load-time error
    /// instead of a first-use failure.
    pub fn load(
        context: &GpuContext,
        ptx: &str,
        kernel_names: &[&'static str],
    ) -> Result<Self, GpuError> {
        if !ptx.is_ascii() {
            return Err(GpuError::InvalidConfig {
                detail: "PTX source must be ASCII (non-ASCII bytes break the driver JIT)"
                    .to_owned(),
            });
        }
        let source = CString::new(ptx).map_err(|_| GpuError::InvalidConfig {
            detail: "PTX source contains an interior NUL byte".to_owned(),
        })?;
        let api = Arc::clone(context.api());
        let module = api.module_load_data(&source)?;
        let mut functions = HashMap::with_capacity(kernel_names.len());
        for name in kernel_names {
            let symbol = CString::new(*name).map_err(|_| GpuError::InvalidConfig {
                detail: format!("kernel name `{name}` contains a NUL byte"),
            })?;
            match api.module_get_function(module, &symbol) {
                Ok(function) => {
                    functions.insert(*name, function);
                }
                Err(error) => {
                    // Unload eagerly: the module is unusable if any expected
                    // kernel is missing.
                    let _unload = api.module_unload(module);
                    return Err(error);
                }
            }
        }
        Ok(Self {
            api,
            module,
            functions,
        })
    }

    /// Launches a named kernel on `stream`.
    ///
    /// # Safety
    ///
    /// `params` must match the kernel's parameter layout exactly and remain
    /// valid until the launch call returns (the driver copies parameters at
    /// launch).
    pub unsafe fn launch(
        &self,
        kernel: &str,
        grid: (u32, u32, u32),
        block: (u32, u32, u32),
        shared_bytes: u32,
        stream: &Stream,
        params: &mut [*mut c_void],
    ) -> Result<(), GpuError> {
        let function =
            self.functions
                .get(kernel)
                .copied()
                .ok_or_else(|| GpuError::InvalidConfig {
                    detail: format!("kernel `{kernel}` was not resolved at module load"),
                })?;
        // SAFETY: forwarded contract — see function docs.
        unsafe {
            self.api
                .launch_kernel(function, grid, block, shared_bytes, stream.raw(), params)
        }
    }
}

impl Drop for PtxModule {
    fn drop(&mut self) {
        if let Err(error) = self.api.module_unload(self.module) {
            tracing::warn!(%error, "failed to unload PTX module");
        }
    }
}
