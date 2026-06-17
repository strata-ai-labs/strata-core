//! Recovery oracle (STH-1): after a crash at a chosen point in a durable
//! workload, prove the recovered database is a *prefix of acknowledged commit
//! history* — no acknowledged commit silently lost, no torn batch, no gap, no
//! phantom/resurrected write.
//!
//! `model` records the acknowledged history; `verify` checks a reopened runtime
//! against it; `driver` generates seeded workloads, crashes at chosen points,
//! reopens, and verifies. The reusable post-condition for STH-2..5.

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
mod driver;
mod model;
mod verify;

#[cfg(all(feature = "localfs", not(target_arch = "wasm32")))]
pub use driver::{run_recovery_oracle_harness, RecoveryOracleOutcome};
