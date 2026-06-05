//! Low-overhead counters for legacy storage performance proof runs.
//!
//! These counters are diagnostic evidence only. They are enabled by the
//! `perf-trace` feature and should not drive storage behavior.

#[cfg(feature = "perf-trace")]
use std::sync::atomic::{AtomicU64, Ordering};

/// Point-in-time legacy storage hot-path counter snapshot.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StoragePerfSnapshot {
    storage_iterator_seeks: u64,
    storage_iterator_pipeline_builds: u64,
    storage_iterator_rows_yielded: u64,
    kv_scan_calls: u64,
    kv_scan_rows_returned: u64,
}

impl StoragePerfSnapshot {
    /// Number of seek calls issued to legacy storage iterators.
    pub const fn storage_iterator_seeks(self) -> u64 {
        self.storage_iterator_seeks
    }

    /// Number of seekable iterator pipelines built from branch snapshots.
    pub const fn storage_iterator_pipeline_builds(self) -> u64 {
        self.storage_iterator_pipeline_builds
    }

    /// Number of live rows yielded by legacy storage iterators.
    pub const fn storage_iterator_rows_yielded(self) -> u64 {
        self.storage_iterator_rows_yielded
    }

    /// Number of public KV scan calls served by the old engine.
    pub const fn kv_scan_calls(self) -> u64 {
        self.kv_scan_calls
    }

    /// Number of rows returned by public KV scan calls.
    pub const fn kv_scan_rows_returned(self) -> u64 {
        self.kv_scan_rows_returned
    }
}

#[cfg(feature = "perf-trace")]
static STORAGE_ITERATOR_SEEKS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static STORAGE_ITERATOR_PIPELINE_BUILDS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static STORAGE_ITERATOR_ROWS_YIELDED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static KV_SCAN_CALLS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static KV_SCAN_ROWS_RETURNED: AtomicU64 = AtomicU64::new(0);

/// Reset all legacy storage performance counters.
#[cfg(feature = "perf-trace")]
pub fn reset() {
    STORAGE_ITERATOR_SEEKS.store(0, Ordering::Relaxed);
    STORAGE_ITERATOR_PIPELINE_BUILDS.store(0, Ordering::Relaxed);
    STORAGE_ITERATOR_ROWS_YIELDED.store(0, Ordering::Relaxed);
    KV_SCAN_CALLS.store(0, Ordering::Relaxed);
    KV_SCAN_ROWS_RETURNED.store(0, Ordering::Relaxed);
}

/// Reset all legacy storage performance counters.
#[cfg(not(feature = "perf-trace"))]
pub const fn reset() {}

/// Capture all legacy storage performance counters.
#[cfg(feature = "perf-trace")]
#[must_use]
pub fn snapshot() -> StoragePerfSnapshot {
    StoragePerfSnapshot {
        storage_iterator_seeks: STORAGE_ITERATOR_SEEKS.load(Ordering::Relaxed),
        storage_iterator_pipeline_builds: STORAGE_ITERATOR_PIPELINE_BUILDS.load(Ordering::Relaxed),
        storage_iterator_rows_yielded: STORAGE_ITERATOR_ROWS_YIELDED.load(Ordering::Relaxed),
        kv_scan_calls: KV_SCAN_CALLS.load(Ordering::Relaxed),
        kv_scan_rows_returned: KV_SCAN_ROWS_RETURNED.load(Ordering::Relaxed),
    }
}

/// Capture all legacy storage performance counters.
#[cfg(not(feature = "perf-trace"))]
#[must_use]
pub const fn snapshot() -> StoragePerfSnapshot {
    StoragePerfSnapshot {
        storage_iterator_seeks: 0,
        storage_iterator_pipeline_builds: 0,
        storage_iterator_rows_yielded: 0,
        kv_scan_calls: 0,
        kv_scan_rows_returned: 0,
    }
}

/// Record one seek against a legacy storage iterator.
#[cfg(feature = "perf-trace")]
pub fn record_storage_iterator_seek() {
    STORAGE_ITERATOR_SEEKS.fetch_add(1, Ordering::Relaxed);
}

/// Record one seek against a legacy storage iterator.
#[cfg(not(feature = "perf-trace"))]
pub const fn record_storage_iterator_seek() {}

/// Record one seekable iterator pipeline build.
#[cfg(feature = "perf-trace")]
pub fn record_storage_iterator_pipeline_build() {
    STORAGE_ITERATOR_PIPELINE_BUILDS.fetch_add(1, Ordering::Relaxed);
}

/// Record one seekable iterator pipeline build.
#[cfg(not(feature = "perf-trace"))]
pub const fn record_storage_iterator_pipeline_build() {}

/// Record one live row yielded by a legacy storage iterator.
#[cfg(feature = "perf-trace")]
pub fn record_storage_iterator_row_yielded() {
    STORAGE_ITERATOR_ROWS_YIELDED.fetch_add(1, Ordering::Relaxed);
}

/// Record one live row yielded by a legacy storage iterator.
#[cfg(not(feature = "perf-trace"))]
pub const fn record_storage_iterator_row_yielded() {}

/// Record one public KV scan call.
#[cfg(feature = "perf-trace")]
pub fn record_kv_scan_call() {
    KV_SCAN_CALLS.fetch_add(1, Ordering::Relaxed);
}

/// Record one public KV scan call.
#[cfg(not(feature = "perf-trace"))]
pub const fn record_kv_scan_call() {}

/// Record rows returned by a public KV scan call.
#[cfg(feature = "perf-trace")]
pub fn record_kv_scan_rows_returned(rows: usize) {
    KV_SCAN_ROWS_RETURNED.fetch_add(as_u64(rows), Ordering::Relaxed);
}

/// Record rows returned by a public KV scan call.
#[cfg(not(feature = "perf-trace"))]
pub const fn record_kv_scan_rows_returned(_rows: usize) {}

#[cfg(feature = "perf-trace")]
fn as_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}
