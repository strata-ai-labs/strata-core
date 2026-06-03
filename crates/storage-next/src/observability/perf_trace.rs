//! Low-overhead counters for storage-next performance proof runs.
//!
//! These counters are diagnostic evidence only. They are compiled into the
//! benchmark crate through the `perf-trace` feature and should not drive storage
//! behavior.

#[cfg(feature = "perf-trace")]
use std::sync::atomic::{AtomicU64, Ordering};

/// Point-in-time storage hot-path counter snapshot.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StoragePerfSnapshot {
    read_view_captures: u64,
    read_view_rows_cloned: u64,
    append_staging_clones: u64,
    append_staging_rows_cloned: u64,
    blind_conflict_sources_built: u64,
    point_rows_visited: u64,
    point_candidates_materialized: u64,
    scan_rows_visited: u64,
    scan_candidates_materialized: u64,
    table_seeks: u64,
}

impl StoragePerfSnapshot {
    /// Number of branch read views captured.
    pub const fn read_view_captures(self) -> u64 {
        self.read_view_captures
    }

    /// Number of rows copied into captured branch read views.
    pub const fn read_view_rows_cloned(self) -> u64 {
        self.read_view_rows_cloned
    }

    /// Number of whole-branch append staging clones performed.
    pub const fn append_staging_clones(self) -> u64 {
        self.append_staging_clones
    }

    /// Number of pre-existing branch rows copied by append staging clones.
    pub const fn append_staging_rows_cloned(self) -> u64 {
        self.append_staging_rows_cloned
    }

    /// Number of conflict-validation sources built for blind commits.
    pub const fn blind_conflict_sources_built(self) -> u64 {
        self.blind_conflict_sources_built
    }

    /// Number of table rows visited during point candidate collection.
    pub const fn point_rows_visited(self) -> u64 {
        self.point_rows_visited
    }

    /// Number of candidate rows materialized during point candidate collection.
    pub const fn point_candidates_materialized(self) -> u64 {
        self.point_candidates_materialized
    }

    /// Number of table rows visited during scan candidate collection.
    pub const fn scan_rows_visited(self) -> u64 {
        self.scan_rows_visited
    }

    /// Number of candidate rows materialized during scan candidate collection.
    pub const fn scan_candidates_materialized(self) -> u64 {
        self.scan_candidates_materialized
    }

    /// Number of ordered table seeks performed by the serving path.
    pub const fn table_seeks(self) -> u64 {
        self.table_seeks
    }
}

#[cfg(feature = "perf-trace")]
static READ_VIEW_CAPTURES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static READ_VIEW_ROWS_CLONED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static APPEND_STAGING_CLONES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static APPEND_STAGING_ROWS_CLONED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static BLIND_CONFLICT_SOURCES_BUILT: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static POINT_ROWS_VISITED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static POINT_CANDIDATES_MATERIALIZED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static SCAN_ROWS_VISITED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static SCAN_CANDIDATES_MATERIALIZED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_SEEKS: AtomicU64 = AtomicU64::new(0);

/// Reset all performance proof counters.
#[cfg(feature = "perf-trace")]
pub fn reset() {
    READ_VIEW_CAPTURES.store(0, Ordering::Relaxed);
    READ_VIEW_ROWS_CLONED.store(0, Ordering::Relaxed);
    APPEND_STAGING_CLONES.store(0, Ordering::Relaxed);
    APPEND_STAGING_ROWS_CLONED.store(0, Ordering::Relaxed);
    BLIND_CONFLICT_SOURCES_BUILT.store(0, Ordering::Relaxed);
    POINT_ROWS_VISITED.store(0, Ordering::Relaxed);
    POINT_CANDIDATES_MATERIALIZED.store(0, Ordering::Relaxed);
    SCAN_ROWS_VISITED.store(0, Ordering::Relaxed);
    SCAN_CANDIDATES_MATERIALIZED.store(0, Ordering::Relaxed);
    TABLE_SEEKS.store(0, Ordering::Relaxed);
}

/// Capture all performance proof counters.
#[cfg(feature = "perf-trace")]
pub fn snapshot() -> StoragePerfSnapshot {
    StoragePerfSnapshot {
        read_view_captures: READ_VIEW_CAPTURES.load(Ordering::Relaxed),
        read_view_rows_cloned: READ_VIEW_ROWS_CLONED.load(Ordering::Relaxed),
        append_staging_clones: APPEND_STAGING_CLONES.load(Ordering::Relaxed),
        append_staging_rows_cloned: APPEND_STAGING_ROWS_CLONED.load(Ordering::Relaxed),
        blind_conflict_sources_built: BLIND_CONFLICT_SOURCES_BUILT.load(Ordering::Relaxed),
        point_rows_visited: POINT_ROWS_VISITED.load(Ordering::Relaxed),
        point_candidates_materialized: POINT_CANDIDATES_MATERIALIZED.load(Ordering::Relaxed),
        scan_rows_visited: SCAN_ROWS_VISITED.load(Ordering::Relaxed),
        scan_candidates_materialized: SCAN_CANDIDATES_MATERIALIZED.load(Ordering::Relaxed),
        table_seeks: TABLE_SEEKS.load(Ordering::Relaxed),
    }
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_read_view_capture(_rows_cloned: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_read_view_capture(rows_cloned: usize) {
    READ_VIEW_CAPTURES.fetch_add(1, Ordering::Relaxed);
    READ_VIEW_ROWS_CLONED.fetch_add(as_u64(rows_cloned), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_append_staging_clone(_rows_cloned: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_append_staging_clone(rows_cloned: usize) {
    APPEND_STAGING_CLONES.fetch_add(1, Ordering::Relaxed);
    APPEND_STAGING_ROWS_CLONED.fetch_add(as_u64(rows_cloned), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_blind_conflict_source_built() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_blind_conflict_source_built() {
    BLIND_CONFLICT_SOURCES_BUILT.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_point_candidate_collection(_rows_visited: usize, _candidates: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_point_candidate_collection(rows_visited: usize, candidates: usize) {
    POINT_ROWS_VISITED.fetch_add(as_u64(rows_visited), Ordering::Relaxed);
    POINT_CANDIDATES_MATERIALIZED.fetch_add(as_u64(candidates), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_scan_candidate_collection(_rows_visited: usize, _candidates: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_scan_candidate_collection(rows_visited: usize, candidates: usize) {
    SCAN_ROWS_VISITED.fetch_add(as_u64(rows_visited), Ordering::Relaxed);
    SCAN_CANDIDATES_MATERIALIZED.fetch_add(as_u64(candidates), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_seek() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_seek() {
    TABLE_SEEKS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "perf-trace")]
fn as_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}
