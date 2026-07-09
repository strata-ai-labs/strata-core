//! The hot-tier machinery (GT1): residency without stalls.
//!
//! Everything here is safe Rust over the [`backend::DeviceBackend`] seam.
//! The pieces:
//!
//! - [`page_table`] — slot state, epoch pinning, event-fenced slot reuse
//!   (the design-§5 invariant this module tree exists to uphold).
//! - [`promotion`] *(internal)* — store → staging → device, batched,
//!   degrade-never-stall.
//! - [`eviction`] *(internal)* — the pure score+edge retention policy.
//! - [`store`] — the T2 seam ([`store::PageStore`]) with an in-memory fake;
//!   GT2 implements it over engine-next.
//! - [`host_sim`] — the CI backend: deterministic completion, fault knobs.
//! - [`tier`] — the facade: `open` / `step_begin` / `request` / `append` /
//!   `maintain`.

pub mod backend;
pub mod engine_store;
mod eviction;
pub mod host_sim;
pub mod page_table;
mod promotion;
pub mod store;
#[allow(clippy::module_inception)]
pub mod tier;

pub use crate::device::cuda_backend::{
    CudaBackend, CudaFence, PagePoolAddress, SelectionAddresses, SelectionTimings,
};
pub use tier::TierStats;
