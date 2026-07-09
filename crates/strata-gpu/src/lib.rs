//! Device runtime for the Strata GPU hot tier.
//!
//! Wraps the CUDA driver API loaded at runtime via `dlopen` — no CUDA
//! toolkit at build time, no link-time dependency; a machine without a
//! driver gets a clean `unavailable.gpu.driver_missing` error, never a
//! build or load failure. The pattern is extracted from the proven
//! strata-inference engine and generalized for the hot tier
//! (`docs/design/gpu-hot-tier.md` §3).
//!
//! What lives here: driver loading, one context per device, the tier's
//! streams and events, PTX module JIT, the pre-reserved device arena, and
//! pinned host pools. What does not: tier semantics (page tables,
//! promotion, eviction) — those live in `strata-tier` against a backend
//! trait so they stay testable without hardware.
//!
//! # Unsafe policy
//!
//! This crate is the workspace's audited-unsafe island for device work
//! (the inference-next `local/` discipline): every `unsafe` block wraps a
//! driver FFI call or pointer handoff and carries a `SAFETY` comment.
//! Consumers (`strata-tier` and above) stay `#![deny(unsafe_code)]`.

mod arena;
mod context;
mod dl;
mod driver;
mod error;
mod module;
mod pinned;
mod stream;

pub use arena::{DeviceArena, RegionSpec, SlotAllocator, SlotRegion};
pub use context::{DeviceFacts, GpuContext};
pub use driver::DevicePtr;
pub use error::GpuError;
pub use module::PtxModule;
pub use pinned::PinnedBuffer;
pub use stream::{Event, Stream};
