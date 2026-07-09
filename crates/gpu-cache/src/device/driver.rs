//! CUDA driver API surface, resolved at runtime.
//!
//! Extends strata-inference's `cuda/ffi.rs` pattern with the entry points
//! the hot tier needs: events and cross-stream fences, pinned host memory,
//! stream-ordered copies, host-function launch, and device attributes.
//! Every entry point is resolved by name from the dlopen'd driver; optional
//! newer symbols degrade to `None` instead of failing the load.

use std::ffi::CStr;
use std::os::raw::{c_char, c_int, c_uint, c_void};

use crate::device::dl::DynLib;
use crate::device::error::GpuError;

/// Raw device pointer (`CUdeviceptr`).
pub type DevicePtr = u64;
pub(crate) type CuContext = *mut c_void;
pub(crate) type CuStream = *mut c_void;
pub(crate) type CuEvent = *mut c_void;
pub(crate) type CuModule = *mut c_void;
pub(crate) type CuFunction = *mut c_void;
pub(crate) type CuDevice = c_int;

pub(crate) const CU_DEVICE_ATTRIBUTE_COMPUTE_CAPABILITY_MAJOR: c_int = 75;
pub(crate) const CU_DEVICE_ATTRIBUTE_COMPUTE_CAPABILITY_MINOR: c_int = 76;
/// `CU_EVENT_DEFAULT` — event timing enabled is not needed; default is fine
/// for fencing (timing events add overhead only when both record+elapsed
/// are used).
const CU_EVENT_DEFAULT: c_uint = 0;
/// `cuMemHostAlloc` flag: portable across contexts.
const CU_MEMHOSTALLOC_PORTABLE: c_uint = 0x01;

type FnCuInit = unsafe extern "C" fn(c_uint) -> c_int;
type FnCuDeviceGetCount = unsafe extern "C" fn(*mut c_int) -> c_int;
type FnCuDeviceGet = unsafe extern "C" fn(*mut CuDevice, c_int) -> c_int;
type FnCuDeviceGetName = unsafe extern "C" fn(*mut c_char, c_int, CuDevice) -> c_int;
type FnCuDeviceGetAttribute = unsafe extern "C" fn(*mut c_int, c_int, CuDevice) -> c_int;
type FnCuDeviceTotalMem = unsafe extern "C" fn(*mut usize, CuDevice) -> c_int;
type FnCuCtxCreate = unsafe extern "C" fn(*mut CuContext, c_uint, CuDevice) -> c_int;
type FnCuCtxDestroy = unsafe extern "C" fn(CuContext) -> c_int;
type FnCuCtxSynchronize = unsafe extern "C" fn() -> c_int;
type FnCuMemGetInfo = unsafe extern "C" fn(*mut usize, *mut usize) -> c_int;
type FnCuMemAlloc = unsafe extern "C" fn(*mut DevicePtr, usize) -> c_int;
type FnCuMemFree = unsafe extern "C" fn(DevicePtr) -> c_int;
type FnCuMemHostAlloc = unsafe extern "C" fn(*mut *mut c_void, usize, c_uint) -> c_int;
type FnCuMemFreeHost = unsafe extern "C" fn(*mut c_void) -> c_int;
type FnCuMemcpyHtoDAsync = unsafe extern "C" fn(DevicePtr, *const c_void, usize, CuStream) -> c_int;
type FnCuMemcpyDtoHAsync = unsafe extern "C" fn(*mut c_void, DevicePtr, usize, CuStream) -> c_int;
type FnCuMemsetD8 = unsafe extern "C" fn(DevicePtr, u8, usize) -> c_int;
type FnCuMemsetD8Async = unsafe extern "C" fn(DevicePtr, u8, usize, CuStream) -> c_int;
type FnCuStreamCreate = unsafe extern "C" fn(*mut CuStream, c_uint) -> c_int;
type FnCuStreamDestroy = unsafe extern "C" fn(CuStream) -> c_int;
type FnCuStreamSynchronize = unsafe extern "C" fn(CuStream) -> c_int;
type FnCuStreamWaitEvent = unsafe extern "C" fn(CuStream, CuEvent, c_uint) -> c_int;
type FnCuLaunchHostFunc =
    unsafe extern "C" fn(CuStream, unsafe extern "C" fn(*mut c_void), *mut c_void) -> c_int;
type FnCuEventCreate = unsafe extern "C" fn(*mut CuEvent, c_uint) -> c_int;
type FnCuEventDestroy = unsafe extern "C" fn(CuEvent) -> c_int;
type FnCuEventRecord = unsafe extern "C" fn(CuEvent, CuStream) -> c_int;
type FnCuEventQuery = unsafe extern "C" fn(CuEvent) -> c_int;
type FnCuEventSynchronize = unsafe extern "C" fn(CuEvent) -> c_int;
type FnCuModuleLoadData = unsafe extern "C" fn(*mut CuModule, *const c_void) -> c_int;
type FnCuModuleUnload = unsafe extern "C" fn(CuModule) -> c_int;
type FnCuModuleGetFunction =
    unsafe extern "C" fn(*mut CuFunction, CuModule, *const c_char) -> c_int;
type FnCuLaunchKernel = unsafe extern "C" fn(
    CuFunction,
    c_uint,
    c_uint,
    c_uint,
    c_uint,
    c_uint,
    c_uint,
    c_uint,
    CuStream,
    *mut *mut c_void,
    *mut *mut c_void,
) -> c_int;
type FnCuGetErrorString = unsafe extern "C" fn(c_int, *mut *const c_char) -> c_int;

macro_rules! load_sym {
    ($lib:expr, $name:expr, $ty:ty) => {{
        let cname = concat!($name, "\0");
        // SAFETY: cname is NUL-terminated by construction.
        let cstr = unsafe { CStr::from_bytes_with_nul_unchecked(cname.as_bytes()) };
        // SAFETY: the symbol is cast to the fn type declared alongside this
        // resolution; signatures mirror the CUDA driver API headers.
        let ptr = unsafe { $lib.sym(cstr) }.map_err(|e| GpuError::DriverMissing {
            detail: format!("failed to resolve {}: {e}", $name),
        })?;
        if ptr.is_null() {
            return Err(GpuError::DriverMissing {
                detail: format!("{} resolved to null", $name),
            });
        }
        // SAFETY: see above — transmute to the declared driver signature.
        unsafe { std::mem::transmute::<*mut c_void, $ty>(ptr) }
    }};
}

macro_rules! try_load_sym {
    ($lib:expr, $name:expr, $ty:ty) => {{
        let cname = concat!($name, "\0");
        // SAFETY: cname is NUL-terminated by construction.
        let cstr = unsafe { CStr::from_bytes_with_nul_unchecked(cname.as_bytes()) };
        match unsafe { $lib.sym(cstr) } {
            // SAFETY: transmute to the declared driver signature.
            Ok(ptr) if !ptr.is_null() => {
                Some(unsafe { std::mem::transmute::<*mut c_void, $ty>(ptr) })
            }
            _ => None,
        }
    }};
}

/// Resolved CUDA driver entry points.
///
/// Loading succeeds only when a driver is present and every required symbol
/// resolves. All wrapped calls are thread-safe per the CUDA programming
/// guide ("The driver API is thread-safe").
pub(crate) struct DriverApi {
    _lib: DynLib,
    cu_device_get_count: FnCuDeviceGetCount,
    cu_device_get: FnCuDeviceGet,
    cu_device_get_name: FnCuDeviceGetName,
    cu_device_get_attribute: FnCuDeviceGetAttribute,
    cu_device_total_mem: FnCuDeviceTotalMem,
    cu_ctx_create: FnCuCtxCreate,
    cu_ctx_destroy: FnCuCtxDestroy,
    cu_ctx_synchronize: FnCuCtxSynchronize,
    cu_mem_get_info: FnCuMemGetInfo,
    cu_mem_alloc: FnCuMemAlloc,
    cu_mem_free: FnCuMemFree,
    cu_mem_host_alloc: FnCuMemHostAlloc,
    cu_mem_free_host: FnCuMemFreeHost,
    cu_memcpy_h_to_d_async: FnCuMemcpyHtoDAsync,
    cu_memcpy_d_to_h_async: FnCuMemcpyDtoHAsync,
    cu_memset_d8: FnCuMemsetD8,
    cu_memset_d8_async: FnCuMemsetD8Async,
    cu_stream_create: FnCuStreamCreate,
    cu_stream_destroy: FnCuStreamDestroy,
    cu_stream_synchronize: FnCuStreamSynchronize,
    cu_stream_wait_event: FnCuStreamWaitEvent,
    cu_launch_host_func: Option<FnCuLaunchHostFunc>,
    cu_event_create: FnCuEventCreate,
    cu_event_destroy: FnCuEventDestroy,
    cu_event_record: FnCuEventRecord,
    cu_event_query: FnCuEventQuery,
    cu_event_synchronize: FnCuEventSynchronize,
    cu_module_load_data: FnCuModuleLoadData,
    cu_module_unload: FnCuModuleUnload,
    cu_module_get_function: FnCuModuleGetFunction,
    cu_launch_kernel: FnCuLaunchKernel,
    cu_get_error_string: Option<FnCuGetErrorString>,
    /// Counts every synchronous wait issued through this API — the tier's
    /// zero-implicit-sync rule is *measured* against this (design §12).
    sync_calls: std::sync::atomic::AtomicU64,
}

// SAFETY: driver API calls are thread-safe per the CUDA programming guide;
// the library handle outlives all resolved pointers because it is owned here.
unsafe impl Send for DriverApi {}
unsafe impl Sync for DriverApi {}

impl DriverApi {
    /// Loads the driver library and resolves entry points. Calls `cuInit`.
    pub(crate) fn load() -> Result<Self, GpuError> {
        #[cfg(unix)]
        let names: &[&[u8]] = &[b"libcuda.so.1\0", b"libcuda.so\0"];
        #[cfg(windows)]
        let names: &[&[u8]] = &[b"nvcuda.dll\0"];
        #[cfg(not(any(unix, windows)))]
        let names: &[&[u8]] = &[];

        let mut lib = None;
        let mut last_error = String::from("no candidate library names for this platform");
        for name in names {
            // SAFETY: candidate names are NUL-terminated literals.
            let cname = unsafe { CStr::from_bytes_with_nul_unchecked(name) };
            match DynLib::open(cname) {
                Ok(handle) => {
                    lib = Some(handle);
                    break;
                }
                Err(error) => last_error = error,
            }
        }
        let lib = lib.ok_or(GpuError::DriverMissing { detail: last_error })?;

        let cu_init: FnCuInit = load_sym!(lib, "cuInit", FnCuInit);
        // SAFETY: cuInit takes a flags word that must be zero.
        let rc = unsafe { cu_init(0) };
        if rc != 0 {
            return Err(GpuError::DriverCall {
                call: "cuInit",
                code: rc,
                detail: String::new(),
            });
        }

        let api = Self {
            cu_device_get_count: load_sym!(lib, "cuDeviceGetCount", FnCuDeviceGetCount),
            cu_device_get: load_sym!(lib, "cuDeviceGet", FnCuDeviceGet),
            cu_device_get_name: load_sym!(lib, "cuDeviceGetName", FnCuDeviceGetName),
            cu_device_get_attribute: load_sym!(lib, "cuDeviceGetAttribute", FnCuDeviceGetAttribute),
            cu_device_total_mem: load_sym!(lib, "cuDeviceTotalMem_v2", FnCuDeviceTotalMem),
            cu_ctx_create: load_sym!(lib, "cuCtxCreate_v2", FnCuCtxCreate),
            cu_ctx_destroy: load_sym!(lib, "cuCtxDestroy_v2", FnCuCtxDestroy),
            cu_ctx_synchronize: load_sym!(lib, "cuCtxSynchronize", FnCuCtxSynchronize),
            cu_mem_get_info: load_sym!(lib, "cuMemGetInfo_v2", FnCuMemGetInfo),
            cu_mem_alloc: load_sym!(lib, "cuMemAlloc_v2", FnCuMemAlloc),
            cu_mem_free: load_sym!(lib, "cuMemFree_v2", FnCuMemFree),
            cu_mem_host_alloc: load_sym!(lib, "cuMemHostAlloc", FnCuMemHostAlloc),
            cu_mem_free_host: load_sym!(lib, "cuMemFreeHost", FnCuMemFreeHost),
            cu_memcpy_h_to_d_async: load_sym!(lib, "cuMemcpyHtoDAsync_v2", FnCuMemcpyHtoDAsync),
            cu_memcpy_d_to_h_async: load_sym!(lib, "cuMemcpyDtoHAsync_v2", FnCuMemcpyDtoHAsync),
            cu_memset_d8: load_sym!(lib, "cuMemsetD8_v2", FnCuMemsetD8),
            cu_memset_d8_async: load_sym!(lib, "cuMemsetD8Async", FnCuMemsetD8Async),
            cu_stream_create: load_sym!(lib, "cuStreamCreate", FnCuStreamCreate),
            cu_stream_destroy: load_sym!(lib, "cuStreamDestroy_v2", FnCuStreamDestroy),
            cu_stream_synchronize: load_sym!(lib, "cuStreamSynchronize", FnCuStreamSynchronize),
            cu_stream_wait_event: load_sym!(lib, "cuStreamWaitEvent", FnCuStreamWaitEvent),
            cu_launch_host_func: try_load_sym!(lib, "cuLaunchHostFunc", FnCuLaunchHostFunc),
            cu_event_create: load_sym!(lib, "cuEventCreate", FnCuEventCreate),
            cu_event_destroy: load_sym!(lib, "cuEventDestroy_v2", FnCuEventDestroy),
            cu_event_record: load_sym!(lib, "cuEventRecord", FnCuEventRecord),
            cu_event_query: load_sym!(lib, "cuEventQuery", FnCuEventQuery),
            cu_event_synchronize: load_sym!(lib, "cuEventSynchronize", FnCuEventSynchronize),
            cu_module_load_data: load_sym!(lib, "cuModuleLoadData", FnCuModuleLoadData),
            cu_module_unload: load_sym!(lib, "cuModuleUnload", FnCuModuleUnload),
            cu_module_get_function: load_sym!(lib, "cuModuleGetFunction", FnCuModuleGetFunction),
            cu_launch_kernel: load_sym!(lib, "cuLaunchKernel", FnCuLaunchKernel),
            cu_get_error_string: try_load_sym!(lib, "cuGetErrorString", FnCuGetErrorString),
            sync_calls: std::sync::atomic::AtomicU64::new(0),
            _lib: lib,
        };
        Ok(api)
    }

    fn check(&self, call: &'static str, rc: c_int) -> Result<(), GpuError> {
        if rc == 0 {
            return Ok(());
        }
        let mut detail = String::new();
        if let Some(get_error_string) = self.cu_get_error_string {
            let mut ptr: *const c_char = std::ptr::null();
            // SAFETY: driver fills ptr with a static string for known codes.
            if unsafe { get_error_string(rc, &raw mut ptr) } == 0 && !ptr.is_null() {
                // SAFETY: the driver returns a NUL-terminated static string.
                detail = unsafe { CStr::from_ptr(ptr) }
                    .to_string_lossy()
                    .into_owned();
            }
        }
        Err(GpuError::DriverCall {
            call,
            code: rc,
            detail,
        })
    }

    fn count_sync(&self) {
        self.sync_calls
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }

    /// Number of synchronous waits issued through this API since load.
    pub(crate) fn sync_call_count(&self) -> u64 {
        self.sync_calls.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub(crate) fn device_count(&self) -> Result<i32, GpuError> {
        let mut count = 0;
        // SAFETY: out-pointer is valid for the call duration.
        let rc = unsafe { (self.cu_device_get_count)(&raw mut count) };
        self.check("cuDeviceGetCount", rc)?;
        Ok(count)
    }

    pub(crate) fn device(&self, ordinal: i32) -> Result<CuDevice, GpuError> {
        let mut device = 0;
        // SAFETY: out-pointer is valid for the call duration.
        let rc = unsafe { (self.cu_device_get)(&raw mut device, ordinal) };
        self.check("cuDeviceGet", rc)?;
        Ok(device)
    }

    pub(crate) fn device_name(&self, device: CuDevice) -> Result<String, GpuError> {
        let mut buffer = [0i8; 128];
        // SAFETY: buffer is valid for 128 bytes; the driver NUL-terminates.
        let rc = unsafe { (self.cu_device_get_name)(buffer.as_mut_ptr(), 128, device) };
        self.check("cuDeviceGetName", rc)?;
        // SAFETY: NUL-terminated by the driver within the buffer.
        Ok(unsafe { CStr::from_ptr(buffer.as_ptr()) }
            .to_string_lossy()
            .into_owned())
    }

    pub(crate) fn device_attribute(
        &self,
        attribute: c_int,
        device: CuDevice,
    ) -> Result<i32, GpuError> {
        let mut value = 0;
        // SAFETY: out-pointer is valid for the call duration.
        let rc = unsafe { (self.cu_device_get_attribute)(&raw mut value, attribute, device) };
        self.check("cuDeviceGetAttribute", rc)?;
        Ok(value)
    }

    pub(crate) fn device_total_mem(&self, device: CuDevice) -> Result<usize, GpuError> {
        let mut bytes = 0usize;
        // SAFETY: out-pointer is valid for the call duration.
        let rc = unsafe { (self.cu_device_total_mem)(&raw mut bytes, device) };
        self.check("cuDeviceTotalMem", rc)?;
        Ok(bytes)
    }

    pub(crate) fn ctx_create(&self, device: CuDevice) -> Result<CuContext, GpuError> {
        let mut ctx = std::ptr::null_mut();
        // SAFETY: out-pointer valid; flags 0 = default scheduling.
        let rc = unsafe { (self.cu_ctx_create)(&raw mut ctx, 0, device) };
        self.check("cuCtxCreate", rc)?;
        Ok(ctx)
    }

    pub(crate) fn ctx_destroy(&self, ctx: CuContext) -> Result<(), GpuError> {
        // SAFETY: ctx came from ctx_create and is destroyed once.
        let rc = unsafe { (self.cu_ctx_destroy)(ctx) };
        self.check("cuCtxDestroy", rc)
    }

    /// Full-context synchronize. Counted: this is a synchronous wait.
    pub(crate) fn ctx_synchronize(&self) -> Result<(), GpuError> {
        self.count_sync();
        // SAFETY: no arguments; blocks until the context drains.
        let rc = unsafe { (self.cu_ctx_synchronize)() };
        self.check("cuCtxSynchronize", rc)
    }

    pub(crate) fn mem_get_info(&self) -> Result<(usize, usize), GpuError> {
        let (mut free, mut total) = (0usize, 0usize);
        // SAFETY: out-pointers valid for the call duration.
        let rc = unsafe { (self.cu_mem_get_info)(&raw mut free, &raw mut total) };
        self.check("cuMemGetInfo", rc)?;
        Ok((free, total))
    }

    pub(crate) fn mem_alloc(&self, bytes: usize) -> Result<DevicePtr, GpuError> {
        let mut ptr: DevicePtr = 0;
        // SAFETY: out-pointer valid; bytes > 0 enforced by callers.
        let rc = unsafe { (self.cu_mem_alloc)(&raw mut ptr, bytes) };
        self.check("cuMemAlloc", rc)?;
        Ok(ptr)
    }

    pub(crate) fn mem_free(&self, ptr: DevicePtr) -> Result<(), GpuError> {
        // SAFETY: ptr came from mem_alloc and is freed once.
        let rc = unsafe { (self.cu_mem_free)(ptr) };
        self.check("cuMemFree", rc)
    }

    pub(crate) fn mem_host_alloc(&self, bytes: usize) -> Result<*mut c_void, GpuError> {
        let mut ptr = std::ptr::null_mut();
        // SAFETY: out-pointer valid; PORTABLE keeps the pinning usable from
        // any context.
        let rc = unsafe { (self.cu_mem_host_alloc)(&raw mut ptr, bytes, CU_MEMHOSTALLOC_PORTABLE) };
        self.check("cuMemHostAlloc", rc)?;
        Ok(ptr)
    }

    pub(crate) fn mem_free_host(&self, ptr: *mut c_void) -> Result<(), GpuError> {
        // SAFETY: ptr came from mem_host_alloc and is freed once.
        let rc = unsafe { (self.cu_mem_free_host)(ptr) };
        self.check("cuMemFreeHost", rc)
    }

    /// Stream-ordered host-to-device copy from pinned memory.
    ///
    /// # Safety
    ///
    /// `src` must remain valid (and pinned) until the copy completes on
    /// `stream`; `dst` must have `len` bytes.
    pub(crate) unsafe fn memcpy_h_to_d_async(
        &self,
        dst: DevicePtr,
        src: *const c_void,
        len: usize,
        stream: CuStream,
    ) -> Result<(), GpuError> {
        // SAFETY: forwarded contract — see function docs.
        let rc = unsafe { (self.cu_memcpy_h_to_d_async)(dst, src, len, stream) };
        self.check("cuMemcpyHtoDAsync", rc)
    }

    /// Stream-ordered device-to-host copy into pinned memory.
    ///
    /// # Safety
    ///
    /// `dst` must remain valid (and pinned) until the copy completes on
    /// `stream`; `src` must have `len` bytes.
    pub(crate) unsafe fn memcpy_d_to_h_async(
        &self,
        dst: *mut c_void,
        src: DevicePtr,
        len: usize,
        stream: CuStream,
    ) -> Result<(), GpuError> {
        // SAFETY: forwarded contract — see function docs.
        let rc = unsafe { (self.cu_memcpy_d_to_h_async)(dst, src, len, stream) };
        self.check("cuMemcpyDtoHAsync", rc)
    }

    pub(crate) fn memset_d8(&self, ptr: DevicePtr, value: u8, len: usize) -> Result<(), GpuError> {
        // SAFETY: ptr..ptr+len lies inside an arena region at all call sites.
        let rc = unsafe { (self.cu_memset_d8)(ptr, value, len) };
        self.check("cuMemsetD8", rc)
    }

    /// Stream-ordered fill.
    pub(crate) fn memset_d8_async(
        &self,
        ptr: DevicePtr,
        value: u8,
        len: usize,
        stream: CuStream,
    ) -> Result<(), GpuError> {
        // SAFETY: ptr..ptr+len lies inside an arena region at all call sites.
        let rc = unsafe { (self.cu_memset_d8_async)(ptr, value, len, stream) };
        self.check("cuMemsetD8Async", rc)
    }

    pub(crate) fn stream_create(&self) -> Result<CuStream, GpuError> {
        let mut stream = std::ptr::null_mut();
        // SAFETY: out-pointer valid; flags 0 = default stream behavior.
        let rc = unsafe { (self.cu_stream_create)(&raw mut stream, 0) };
        self.check("cuStreamCreate", rc)?;
        Ok(stream)
    }

    pub(crate) fn stream_destroy(&self, stream: CuStream) -> Result<(), GpuError> {
        // SAFETY: stream came from stream_create and is destroyed once.
        let rc = unsafe { (self.cu_stream_destroy)(stream) };
        self.check("cuStreamDestroy", rc)
    }

    /// Blocks until the stream drains. Counted: this is a synchronous wait.
    pub(crate) fn stream_synchronize(&self, stream: CuStream) -> Result<(), GpuError> {
        self.count_sync();
        // SAFETY: stream is live (owned by a Stream wrapper).
        let rc = unsafe { (self.cu_stream_synchronize)(stream) };
        self.check("cuStreamSynchronize", rc)
    }

    /// Makes `stream` wait for `event` without blocking the host.
    pub(crate) fn stream_wait_event(
        &self,
        stream: CuStream,
        event: CuEvent,
    ) -> Result<(), GpuError> {
        // SAFETY: both handles are live; flags must be 0.
        let rc = unsafe { (self.cu_stream_wait_event)(stream, event, 0) };
        self.check("cuStreamWaitEvent", rc)
    }

    /// Enqueues a host callback on the stream (driver ≥ 10.0).
    ///
    /// # Safety
    ///
    /// `user_data` must stay valid until the callback runs exactly once.
    pub(crate) unsafe fn launch_host_func(
        &self,
        stream: CuStream,
        callback: unsafe extern "C" fn(*mut c_void),
        user_data: *mut c_void,
    ) -> Result<(), GpuError> {
        let Some(launch) = self.cu_launch_host_func else {
            return Err(GpuError::DriverCall {
                call: "cuLaunchHostFunc",
                code: -1,
                detail: "driver does not expose cuLaunchHostFunc (needs >= 10.0)".to_owned(),
            });
        };
        // SAFETY: forwarded contract — see function docs.
        let rc = unsafe { launch(stream, callback, user_data) };
        self.check("cuLaunchHostFunc", rc)
    }

    pub(crate) fn event_create(&self) -> Result<CuEvent, GpuError> {
        let mut event = std::ptr::null_mut();
        // SAFETY: out-pointer valid for the call duration.
        let rc = unsafe { (self.cu_event_create)(&raw mut event, CU_EVENT_DEFAULT) };
        self.check("cuEventCreate", rc)?;
        Ok(event)
    }

    pub(crate) fn event_destroy(&self, event: CuEvent) -> Result<(), GpuError> {
        // SAFETY: event came from event_create and is destroyed once.
        let rc = unsafe { (self.cu_event_destroy)(event) };
        self.check("cuEventDestroy", rc)
    }

    pub(crate) fn event_record(&self, event: CuEvent, stream: CuStream) -> Result<(), GpuError> {
        // SAFETY: both handles are live.
        let rc = unsafe { (self.cu_event_record)(event, stream) };
        self.check("cuEventRecord", rc)
    }

    /// Non-blocking completion probe: `Ok(true)` when all preceding work has
    /// finished. `CUDA_ERROR_NOT_READY` (600) is a healthy `false`.
    pub(crate) fn event_query(&self, event: CuEvent) -> Result<bool, GpuError> {
        // SAFETY: event is live.
        let rc = unsafe { (self.cu_event_query)(event) };
        match rc {
            0 => Ok(true),
            600 => Ok(false),
            _ => {
                self.check("cuEventQuery", rc)?;
                Ok(true)
            }
        }
    }

    /// Blocks until the event completes. Counted: this is a synchronous wait.
    pub(crate) fn event_synchronize(&self, event: CuEvent) -> Result<(), GpuError> {
        self.count_sync();
        // SAFETY: event is live.
        let rc = unsafe { (self.cu_event_synchronize)(event) };
        self.check("cuEventSynchronize", rc)
    }

    pub(crate) fn module_load_data(&self, ptx: &CStr) -> Result<CuModule, GpuError> {
        let mut module = std::ptr::null_mut();
        // SAFETY: ptx is NUL-terminated PTX source; the driver JIT-compiles it.
        let rc =
            unsafe { (self.cu_module_load_data)(&raw mut module, ptx.as_ptr().cast::<c_void>()) };
        self.check("cuModuleLoadData", rc)?;
        Ok(module)
    }

    pub(crate) fn module_unload(&self, module: CuModule) -> Result<(), GpuError> {
        // SAFETY: module came from module_load_data and is unloaded once.
        let rc = unsafe { (self.cu_module_unload)(module) };
        self.check("cuModuleUnload", rc)
    }

    pub(crate) fn module_get_function(
        &self,
        module: CuModule,
        name: &CStr,
    ) -> Result<CuFunction, GpuError> {
        let mut function = std::ptr::null_mut();
        // SAFETY: module is live; name is a NUL-terminated kernel symbol.
        let rc = unsafe { (self.cu_module_get_function)(&raw mut function, module, name.as_ptr()) };
        self.check("cuModuleGetFunction", rc)?;
        Ok(function)
    }

    /// Launches a kernel on a stream.
    ///
    /// # Safety
    ///
    /// `params` must match the kernel's parameter layout exactly and stay
    /// valid for the duration of the call.
    #[allow(clippy::too_many_arguments)]
    pub(crate) unsafe fn launch_kernel(
        &self,
        function: CuFunction,
        grid: (u32, u32, u32),
        block: (u32, u32, u32),
        shared_bytes: u32,
        stream: CuStream,
        params: &mut [*mut c_void],
    ) -> Result<(), GpuError> {
        // SAFETY: forwarded contract — see function docs.
        let rc = unsafe {
            (self.cu_launch_kernel)(
                function,
                grid.0,
                grid.1,
                grid.2,
                block.0,
                block.1,
                block.2,
                shared_bytes,
                stream,
                params.as_mut_ptr(),
                std::ptr::null_mut(),
            )
        };
        self.check("cuLaunchKernel", rc)
    }
}
