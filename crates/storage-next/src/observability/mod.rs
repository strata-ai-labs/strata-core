//! Storage diagnostics, health reporting, and trace-friendly events.

#[cfg(feature = "perf-trace")]
pub mod perf_probe;
pub mod perf_trace;
