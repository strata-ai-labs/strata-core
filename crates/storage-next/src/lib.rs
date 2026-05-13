//! Storage substrate for Strata.
//!
//! This crate currently defines storage module ownership buckets. The public API
//! remains empty until the backend, format, and lifecycle contracts are
//! implemented.

#![deny(unsafe_code)]

#[cfg(all(target_arch = "wasm32", feature = "localfs"))]
compile_error!("the localfs feature is not supported on wasm32; use default-features = false");

mod api;
mod backend;
mod branch;
mod commit;
mod config;
mod error;
mod format;
mod layout;
mod lifecycle;
mod object;
mod observability;
mod row;
mod service;
mod table;

#[cfg(test)]
mod test_support;

#[cfg(any(test, feature = "testkit"))]
#[doc(hidden)]
pub mod testkit;
