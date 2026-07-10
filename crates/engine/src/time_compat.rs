//! Platform clock source.
//!
//! `std::time::SystemTime::now()` traps at runtime on `wasm32-unknown-unknown`,
//! so wasm builds route through `web-time`, which mirrors the std API over
//! `Date.now()`. Native builds re-export std directly — same types, zero
//! behavior change.

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(target_arch = "wasm32")]
pub(crate) use web_time::{SystemTime, UNIX_EPOCH};
