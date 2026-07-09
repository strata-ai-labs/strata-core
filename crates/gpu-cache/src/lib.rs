//! Strata's GPU cache: a device-resident hot tier over the store of
//! record. Everything Strata runs on a GPU lives in this one crate.
//!
//! One crate, two layers (`docs/design/gpu-hot-tier.md` §2):
//!
//! - [`device`](crate::device) *(internal)* — the runtime: CUDA driver
//!   loaded at runtime via `dlopen` (no toolkit at build time, no link-time
//!   dependency; driverless machines get a clean typed error), one context
//!   per device, the pre-reserved arena, streams/events, pinned host
//!   memory, and PTX JIT. Extracted from the proven strata-inference
//!   engine. This module is the crate's only `unsafe` territory.
//! - `tier` *(GT1+)* — the hot-tier semantics: page pool and page table,
//!   epoch pinning with event-fenced slot reuse, promotion/eviction,
//!   write-behind to the store of record. Safe Rust against a backend
//!   trait, so all machinery tests on a host-sim backend in CI.
//!
//! The unsafe policy is module-scoped (the inference-next `local/`
//! discipline, workspace rule 38): the crate denies `unsafe_code`; only
//! `device/` may allow it, and every unsafe block there wraps a single
//! driver call with a `SAFETY` note.

#![deny(unsafe_code)]

#[allow(unsafe_code)]
mod device;

#[cfg(feature = "python")]
#[allow(unsafe_code)] // DLPack capsule FFI; every block carries a SAFETY note
mod python;

pub mod tier;

pub use device::arena::{DeviceArena, RegionSpec, SlotAllocator, SlotRegion};
pub use device::context::{DeviceFacts, GpuContext};
pub use device::driver::DevicePtr;
pub use device::error::GpuError;
pub use device::module::PtxModule;
pub use device::pinned::PinnedBuffer;
pub use device::stream::{Event, Stream};
