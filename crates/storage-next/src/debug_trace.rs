//! Per-compaction timing probe (`STRATA_TRACE`).
//!
//! Logs one line per compaction (level, input tables, input bytes, duration) so a subcompaction
//! A/B can measure the L0->L1 build wall-time. Zero cost unless `STRATA_TRACE` is set.

use std::fmt;
use std::sync::OnceLock;
use std::time::Instant;

fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("STRATA_TRACE").is_some())
}

fn elapsed_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    let start = *START.get_or_init(Instant::now);
    u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX)
}

pub(crate) fn trace(args: fmt::Arguments<'_>) {
    if !enabled() {
        return;
    }
    eprintln!("WT t={} {}", elapsed_ms(), args);
}
