//! Storage substrate for Strata.
//!
//! The supported public boundary is [`api`]. Native callers that have a
//! database directory should open durable local storage with
//! `api::StorageRuntime::open_local(root)`.
//!
//! Volatile storage is available only through explicit APIs such as
//! `api::StorageRuntime::open_ephemeral()` for tests, demos, and sessions that
//! intentionally do not persist after the runtime is dropped.

#![deny(unsafe_code)]

#[cfg(all(target_arch = "wasm32", feature = "localfs"))]
compile_error!("the localfs feature is not supported on wasm32; use default-features = false");

pub mod api;
mod backend;
mod branch;
mod commit;
mod config;
mod debug_trace;
mod error;
mod format;
mod layout;
mod lifecycle;
mod object;
mod observability;
mod row;
mod service;
mod table;
mod time_compat;
mod timeline_index;

#[cfg(feature = "perf-trace")]
#[doc(hidden)]
pub use observability::perf_probe;

#[cfg(feature = "perf-trace")]
#[doc(hidden)]
pub use observability::perf_trace;

#[cfg(test)]
mod test_support;

#[cfg(any(test, feature = "testkit"))]
#[doc(hidden)]
pub mod testkit;
