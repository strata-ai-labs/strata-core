//! The device runtime: dlopen'd CUDA driver, context, arena, streams,
//! events, pinned memory, PTX JIT.
//!
//! This module is the crate's **audited-unsafe island** — the only place
//! `unsafe` is permitted (see `lib.rs`). Every unsafe block wraps one driver
//! FFI call or pointer handoff and carries a `SAFETY` comment. Everything
//! above (the tier's page tables, promotion, eviction) is safe Rust built on
//! these wrappers.

pub(crate) mod arena;
pub(crate) mod context;
pub(crate) mod dl;
pub(crate) mod driver;
pub(crate) mod error;
pub(crate) mod module;
pub(crate) mod pinned;
pub(crate) mod stream;
