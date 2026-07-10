//! Platform clock source.
//!
//! `std::time::Instant` traps at runtime on `wasm32-unknown-unknown`, so
//! wasm builds route through `web-time`, which mirrors the std API over
//! `performance.now()`. Native builds re-export std directly — same types,
//! zero behavior change.

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use std::time::Instant;

#[cfg(target_arch = "wasm32")]
pub(crate) use web_time::Instant;
