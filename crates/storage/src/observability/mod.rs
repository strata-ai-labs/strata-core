//! Storage diagnostics, health reporting, and trace-friendly events.

#[cfg(feature = "perf-trace")]
pub mod perf_probe;
#[cfg(feature = "perf-trace")]
pub mod perf_trace;
#[cfg(not(feature = "perf-trace"))]
pub(crate) mod perf_trace;
