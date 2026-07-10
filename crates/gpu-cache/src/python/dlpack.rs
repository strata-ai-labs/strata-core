//! Minimal `DLPack` producer: device tensors as `dltensor` capsules.
//!
//! Implements the classic `DLManagedTensor` capsule contract (universally
//! consumed; the versioned form is a follow-up): the capsule owns a boxed
//! manager whose context keeps the producing [`Tier`](super::Tier) alive —
//! a torch tensor built from this capsule extends the tier's lifetime, so
//! the arena the data lives in cannot be freed under it.
//!
//! The deleter runs on the consumer's thread — possibly during interpreter
//! finalization (torch tears tensors down inside `Py_FinalizeEx`), where
//! acquiring the GIL aborts the process. So the deleter never touches the
//! GIL: it moves the keepalive into a graveyard that the next capsule
//! creation drains under the GIL it already holds. Keepalives still buried
//! at process exit leak to the OS, intentionally. (The graveyard is
//! resource reclamation, not semantic state — the per-database rule is
//! about the latter.)
//!
//! This module and `device/` are the crate's only unsafe territory (see
//! `lib.rs`).

use std::ffi::{c_void, CStr};
use std::sync::Mutex;

use pyo3::prelude::*;

/// Keepalives whose tensors died on threads (or in interpreter phases)
/// where the GIL could not be taken. Drained by [`capsule`].
static GRAVEYARD: Mutex<Vec<Py<PyAny>>> = Mutex::new(Vec::new());

/// `kDLCUDA`.
const DL_DEVICE_CUDA: i32 = 2;
/// `dltensor` (unconsumed capsule name per the `DLPack` protocol).
const CAPSULE_NAME: &CStr = c"dltensor";
/// Element type codes (`DLDataTypeCode`).
#[derive(Clone, Copy, Debug)]
pub(crate) enum DType {
    /// 32-bit signed integers (the block table; -1 pads).
    Int32,
    /// 32-bit floats (scores).
    Float32,
    /// Bytes (materialized pages).
    Uint8,
}

impl DType {
    const fn as_dl(self) -> DLDataType {
        match self {
            Self::Int32 => DLDataType {
                code: 0, // kDLInt
                bits: 32,
                lanes: 1,
            },
            Self::Float32 => DLDataType {
                code: 2, // kDLFloat
                bits: 32,
                lanes: 1,
            },
            Self::Uint8 => DLDataType {
                code: 1, // kDLUInt
                bits: 8,
                lanes: 1,
            },
        }
    }
}

#[repr(C)]
struct DLDevice {
    device_type: i32,
    device_id: i32,
}

#[repr(C)]
struct DLDataType {
    code: u8,
    bits: u8,
    lanes: u16,
}

#[repr(C)]
struct DLTensor {
    data: *mut c_void,
    device: DLDevice,
    ndim: i32,
    dtype: DLDataType,
    shape: *mut i64,
    strides: *mut i64,
    byte_offset: u64,
}

#[repr(C)]
struct DLManagedTensor {
    dl_tensor: DLTensor,
    manager_ctx: *mut c_void,
    deleter: Option<unsafe extern "C" fn(*mut DLManagedTensor)>,
}

/// Everything the tensor borrows: the shape array the `DLTensor` points at
/// and the Python object that owns the device memory.
struct ManagerCtx {
    keepalive: Py<PyAny>,
    shape: Box<[i64]>,
}

unsafe extern "C" fn deleter(managed: *mut DLManagedTensor) {
    if managed.is_null() {
        return;
    }
    // SAFETY: `managed` was produced by Box::into_raw in `capsule` below and
    // the protocol guarantees the deleter runs at most once.
    let managed = unsafe { Box::from_raw(managed) };
    let ctx = managed.manager_ctx.cast::<ManagerCtx>();
    if !ctx.is_null() {
        // SAFETY: paired Box::into_raw in `capsule`. No GIL here — a panic
        // in this extern "C" fn aborts, and taking the GIL can do exactly
        // that during interpreter finalization. Bury the keepalive instead.
        let ctx = unsafe { Box::from_raw(ctx) };
        GRAVEYARD
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(ctx.keepalive);
    }
}

unsafe extern "C" fn capsule_destructor(capsule: *mut pyo3::ffi::PyObject) {
    // Per the protocol: if the consumer took ownership it renamed the
    // capsule to `used_dltensor` and the deleter is theirs to call; only an
    // unconsumed capsule cleans up here.
    // SAFETY: called by Python with a valid capsule object.
    unsafe {
        if pyo3::ffi::PyCapsule_IsValid(capsule, CAPSULE_NAME.as_ptr()) == 1 {
            let managed = pyo3::ffi::PyCapsule_GetPointer(capsule, CAPSULE_NAME.as_ptr())
                .cast::<DLManagedTensor>();
            deleter(managed);
        }
    }
}

/// Builds a `dltensor` capsule over device memory at `data_ptr`.
pub(crate) fn capsule(
    py: Python<'_>,
    data_ptr: u64,
    shape: &[i64],
    dtype: DType,
    device_id: i32,
    keepalive: Py<PyAny>,
) -> PyResult<PyObject> {
    // Drop keepalives of tensors that died where the GIL was off-limits
    // (we hold it here). Steady-state decode never accumulates more than
    // the tensors of one step.
    let dead: Vec<Py<PyAny>> = std::mem::take(
        &mut GRAVEYARD
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner),
    );
    drop(dead);
    let shape_box: Box<[i64]> = shape.into();
    let ctx = Box::new(ManagerCtx {
        keepalive,
        shape: shape_box,
    });
    let shape_ptr = ctx.shape.as_ptr().cast_mut();
    let managed = Box::new(DLManagedTensor {
        dl_tensor: DLTensor {
            data: data_ptr as *mut c_void,
            device: DLDevice {
                device_type: DL_DEVICE_CUDA,
                device_id,
            },
            ndim: i32::try_from(shape.len()).expect("tiny ndim"),
            dtype: dtype.as_dl(),
            shape: shape_ptr,
            strides: std::ptr::null_mut(), // compact row-major
            byte_offset: 0,
        },
        manager_ctx: Box::into_raw(ctx).cast::<c_void>(),
        deleter: Some(deleter),
    });
    let managed_ptr = Box::into_raw(managed);
    // SAFETY: managed_ptr is a valid heap pointer; the capsule owns it via
    // the destructor above.
    let raw = unsafe {
        pyo3::ffi::PyCapsule_New(
            managed_ptr.cast::<c_void>(),
            CAPSULE_NAME.as_ptr(),
            Some(capsule_destructor),
        )
    };
    if raw.is_null() {
        // SAFETY: reclaim on failure; the deleter frees ctx.
        unsafe { deleter(managed_ptr) };
        return Err(pyo3::exceptions::PyRuntimeError::new_err(
            "failed to create the dltensor capsule",
        ));
    }
    // SAFETY: raw is a new capsule reference owned by us.
    Ok(unsafe { PyObject::from_owned_ptr(py, raw) })
}
