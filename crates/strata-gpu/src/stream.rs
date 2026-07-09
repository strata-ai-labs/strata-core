//! Streams, events, and host callbacks.
//!
//! The tier owns its streams and never synchronizes on the caller's compute
//! stream; ordering across streams is expressed with events
//! (`Stream::wait`), and slot-reuse fencing uses `Event::query` — a
//! non-blocking probe.

use std::os::raw::c_void;
use std::sync::Arc;

use crate::context::GpuContext;
use crate::driver::{CuEvent, CuStream, DevicePtr, DriverApi};
use crate::error::GpuError;

/// An owned CUDA stream.
pub struct Stream {
    api: Arc<DriverApi>,
    raw: CuStream,
}

// SAFETY: stream operations go through the thread-safe driver API; the raw
// handle is destroyed exactly once on drop.
unsafe impl Send for Stream {}
unsafe impl Sync for Stream {}

impl Stream {
    /// Creates a new stream on the context's device.
    pub fn new(context: &GpuContext) -> Result<Self, GpuError> {
        let api = Arc::clone(context.api());
        let raw = api.stream_create()?;
        Ok(Self { api, raw })
    }

    /// Enqueues a host-to-device copy from pinned memory.
    ///
    /// # Safety
    ///
    /// `src` must point at pinned memory valid for `len` bytes until the
    /// copy completes on this stream; `dst` must have `len` bytes.
    pub unsafe fn copy_to_device(
        &self,
        dst: DevicePtr,
        src: *const u8,
        len: usize,
    ) -> Result<(), GpuError> {
        // SAFETY: forwarded contract — see function docs.
        unsafe {
            self.api
                .memcpy_h_to_d_async(dst, src.cast::<c_void>(), len, self.raw)
        }
    }

    /// Enqueues a device-to-host copy into pinned memory.
    ///
    /// # Safety
    ///
    /// `dst` must point at pinned memory valid for `len` bytes until the
    /// copy completes on this stream; `src` must have `len` bytes.
    pub unsafe fn copy_to_host(
        &self,
        dst: *mut u8,
        src: DevicePtr,
        len: usize,
    ) -> Result<(), GpuError> {
        // SAFETY: forwarded contract — see function docs.
        unsafe {
            self.api
                .memcpy_d_to_h_async(dst.cast::<c_void>(), src, len, self.raw)
        }
    }

    /// Records an event capturing all work enqueued so far.
    pub fn record(&self) -> Result<Event, GpuError> {
        let event = Event::new(&self.api)?;
        self.api.event_record(event.raw, self.raw)?;
        Ok(event)
    }

    /// Makes this stream wait for `event` without blocking the host.
    pub fn wait(&self, event: &Event) -> Result<(), GpuError> {
        self.api.stream_wait_event(self.raw, event.raw)
    }

    /// Enqueues a host closure to run when the stream reaches this point.
    ///
    /// The closure runs on a driver thread; it must not call back into the
    /// driver (CUDA forbids API calls inside host functions).
    pub fn launch_host_fn(&self, callback: Box<dyn FnOnce() + Send>) -> Result<(), GpuError> {
        unsafe extern "C" fn trampoline(user_data: *mut c_void) {
            // SAFETY: user_data was produced by Box::into_raw below and the
            // driver invokes the trampoline exactly once.
            let callback = unsafe { Box::from_raw(user_data.cast::<Box<dyn FnOnce() + Send>>()) };
            callback();
        }

        // Double-box: the outer box gives a thin pointer for the C trampoline.
        let user_data = Box::into_raw(Box::new(callback)).cast::<c_void>();

        // SAFETY: user_data stays valid until the trampoline consumes it.
        let result = unsafe { self.api.launch_host_func(self.raw, trampoline, user_data) };
        if result.is_err() {
            // The driver never took ownership; reclaim to avoid a leak.
            // SAFETY: on error the trampoline will never run.
            drop(unsafe { Box::from_raw(user_data.cast::<Box<dyn FnOnce() + Send>>()) });
        }
        result
    }

    /// Blocks the host until this stream drains. Counted as a sync wait —
    /// test and shutdown paths only, never the decode loop.
    pub fn synchronize(&self) -> Result<(), GpuError> {
        self.api.stream_synchronize(self.raw)
    }

    pub(crate) fn raw(&self) -> CuStream {
        self.raw
    }
}

impl Drop for Stream {
    fn drop(&mut self) {
        if let Err(error) = self.api.stream_destroy(self.raw) {
            tracing::warn!(%error, "failed to destroy stream");
        }
    }
}

/// An owned CUDA event, used as a cross-stream fence and a non-blocking
/// completion probe (the slot-reuse mechanism in the tier design).
pub struct Event {
    api: Arc<DriverApi>,
    raw: CuEvent,
}

// SAFETY: event operations go through the thread-safe driver API; the raw
// handle is destroyed exactly once on drop.
unsafe impl Send for Event {}
unsafe impl Sync for Event {}

impl Event {
    fn new(api: &Arc<DriverApi>) -> Result<Self, GpuError> {
        let raw = api.event_create()?;
        Ok(Self {
            api: Arc::clone(api),
            raw,
        })
    }

    /// Non-blocking: `true` when every operation recorded before this event
    /// has completed.
    pub fn is_complete(&self) -> Result<bool, GpuError> {
        self.api.event_query(self.raw)
    }

    /// Blocks the host until the event completes. Counted as a sync wait —
    /// test and shutdown paths only.
    pub fn synchronize(&self) -> Result<(), GpuError> {
        self.api.event_synchronize(self.raw)
    }
}

impl Drop for Event {
    fn drop(&mut self) {
        if let Err(error) = self.api.event_destroy(self.raw) {
            tracing::warn!(%error, "failed to destroy event");
        }
    }
}
