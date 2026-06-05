//! Low-overhead counters for storage-next performance proof runs.
//!
//! These counters are diagnostic evidence only. They are compiled into the
//! benchmark crate through the `perf-trace` feature and should not drive storage
//! behavior.

#[cfg(all(test, feature = "perf-trace"))]
use std::cell::Cell;
#[cfg(feature = "perf-trace")]
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(all(test, feature = "perf-trace"))]
use std::sync::{Mutex, MutexGuard};
#[cfg(feature = "perf-trace")]
use std::time::Instant;

/// Point-in-time storage hot-path counter snapshot.
#[cfg(feature = "perf-trace")]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StoragePerfSnapshot {
    api_commit_map_ns: u64,
    api_commit_runtime_ns: u64,
    api_scan_runtime_ns: u64,
    api_scan_map_ns: u64,
    api_scan_bounds_ns: u64,
    runtime_batch_validate_ns: u64,
    runtime_duplicate_mutation_key_checks: u64,
    commit_prepare_rows_ns: u64,
    append_batch_validate_ns: u64,
    append_insert_rows_ns: u64,
    append_absent_internal_key_checks: u64,
    mutable_insert_duplicate_checks: u64,
    commit_batches_prepared: u64,
    commit_user_mutation_rows: u64,
    commit_timeline_rows_prepared: u64,
    commit_rows_prepared: u64,
    append_rows_applied: u64,
    branch_facts_rows_observed: u64,
    read_view_captures: u64,
    read_view_rows_cloned: u64,
    read_view_validation_rows_scanned: u64,
    append_staging_clones: u64,
    append_staging_rows_cloned: u64,
    conflict_sources_built: u64,
    point_rows_visited: u64,
    point_candidates_materialized: u64,
    scan_rows_visited: u64,
    scan_candidates_materialized: u64,
    scan_cursor_seeks: u64,
    scan_cursor_rows_yielded: u64,
    branch_scan_source_setup_ns: u64,
    branch_scan_merge_ns: u64,
    branch_scan_min_key_ns: u64,
    branch_scan_group_key_ns: u64,
    branch_scan_candidate_ns: u64,
    branch_scan_advance_ns: u64,
    branch_scan_select_ns: u64,
    branch_scan_emit_ns: u64,
    scan_logical_key_encodes: u64,
    scan_candidate_row_clones: u64,
    scan_candidate_row_clone_bytes: u64,
    table_seeks: u64,
    table_bound_checks: u64,
    table_bound_check_ns: u64,
}

#[cfg(feature = "perf-trace")]
impl StoragePerfSnapshot {
    /// Nanoseconds spent mapping public API commit batches into runtime batches.
    pub const fn api_commit_map_ns(self) -> u64 {
        self.api_commit_map_ns
    }

    /// Nanoseconds spent inside cache/durable commit execution from the API.
    pub const fn api_commit_runtime_ns(self) -> u64 {
        self.api_commit_runtime_ns
    }

    /// Nanoseconds spent inside cache/durable scan execution from the API.
    pub const fn api_scan_runtime_ns(self) -> u64 {
        self.api_scan_runtime_ns
    }

    /// Nanoseconds spent mapping scan storage rows into public API rows.
    pub const fn api_scan_map_ns(self) -> u64 {
        self.api_scan_map_ns
    }

    /// Nanoseconds spent constructing physical scan bounds from public API keys.
    pub const fn api_scan_bounds_ns(self) -> u64 {
        self.api_scan_bounds_ns
    }

    /// Nanoseconds spent validating runtime commit batch shape and invariants.
    pub const fn runtime_batch_validate_ns(self) -> u64 {
        self.runtime_batch_validate_ns
    }

    /// Duplicate-mutation key checks performed by runtime validation.
    pub const fn runtime_duplicate_mutation_key_checks(self) -> u64 {
        self.runtime_duplicate_mutation_key_checks
    }

    /// Nanoseconds spent stamping user rows and timeline rows.
    pub const fn commit_prepare_rows_ns(self) -> u64 {
        self.commit_prepare_rows_ns
    }

    /// Nanoseconds spent validating append batches before applying rows.
    pub const fn append_batch_validate_ns(self) -> u64 {
        self.append_batch_validate_ns
    }

    /// Nanoseconds spent inserting append rows into the active table.
    pub const fn append_insert_rows_ns(self) -> u64 {
        self.append_insert_rows_ns
    }

    /// Internal-key absence checks performed before append/install.
    pub const fn append_absent_internal_key_checks(self) -> u64 {
        self.append_absent_internal_key_checks
    }

    /// Duplicate-key checks performed by mutable table insertion.
    pub const fn mutable_insert_duplicate_checks(self) -> u64 {
        self.mutable_insert_duplicate_checks
    }

    /// Number of mutating commit batches whose storage rows were prepared.
    pub const fn commit_batches_prepared(self) -> u64 {
        self.commit_batches_prepared
    }

    /// Number of user mutation rows prepared for commit application.
    pub const fn commit_user_mutation_rows(self) -> u64 {
        self.commit_user_mutation_rows
    }

    /// Number of timeline metadata rows prepared for commit application.
    pub const fn commit_timeline_rows_prepared(self) -> u64 {
        self.commit_timeline_rows_prepared
    }

    /// Total rows prepared for commit application.
    pub const fn commit_rows_prepared(self) -> u64 {
        self.commit_rows_prepared
    }

    /// Number of prepared rows handed to branch append.
    pub const fn append_rows_applied(self) -> u64 {
        self.append_rows_applied
    }

    /// Number of branch rows scanned while deriving branch facts.
    pub const fn branch_facts_rows_observed(self) -> u64 {
        self.branch_facts_rows_observed
    }

    /// Number of branch read views captured.
    pub const fn read_view_captures(self) -> u64 {
        self.read_view_captures
    }

    /// Number of rows copied into captured branch read views.
    pub const fn read_view_rows_cloned(self) -> u64 {
        self.read_view_rows_cloned
    }

    /// Number of branch rows scanned while validating captured read-view facts.
    pub const fn read_view_validation_rows_scanned(self) -> u64 {
        self.read_view_validation_rows_scanned
    }

    /// Number of whole-branch append staging clones performed.
    pub const fn append_staging_clones(self) -> u64 {
        self.append_staging_clones
    }

    /// Number of pre-existing branch rows copied by append staging clones.
    pub const fn append_staging_rows_cloned(self) -> u64 {
        self.append_staging_rows_cloned
    }

    /// Number of conflict-validation sources built.
    pub const fn conflict_sources_built(self) -> u64 {
        self.conflict_sources_built
    }

    /// Deprecated compatibility accessor for the original PERF-P0 counter name.
    pub const fn blind_conflict_sources_built(self) -> u64 {
        self.conflict_sources_built
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

    /// Number of bounded scan cursor seeks performed.
    pub const fn scan_cursor_seeks(self) -> u64 {
        self.scan_cursor_seeks
    }

    /// Number of rows reached by bounded scan cursors.
    pub const fn scan_cursor_rows_yielded(self) -> u64 {
        self.scan_cursor_rows_yielded
    }

    /// Nanoseconds spent building/seeking branch scan sources.
    pub const fn branch_scan_source_setup_ns(self) -> u64 {
        self.branch_scan_source_setup_ns
    }

    /// Nanoseconds spent merging branch scan sources and materializing candidates.
    pub const fn branch_scan_merge_ns(self) -> u64 {
        self.branch_scan_merge_ns
    }

    /// Nanoseconds spent finding the next logical key across scan cursors.
    pub const fn branch_scan_min_key_ns(self) -> u64 {
        self.branch_scan_min_key_ns
    }

    /// Nanoseconds spent checking whether cursors still match the selected scan key.
    pub const fn branch_scan_group_key_ns(self) -> u64 {
        self.branch_scan_group_key_ns
    }

    /// Nanoseconds spent materializing branch scan candidate rows.
    pub const fn branch_scan_candidate_ns(self) -> u64 {
        self.branch_scan_candidate_ns
    }

    /// Nanoseconds spent advancing branch scan cursors.
    pub const fn branch_scan_advance_ns(self) -> u64 {
        self.branch_scan_advance_ns
    }

    /// Nanoseconds spent selecting the visible row from grouped scan candidates.
    pub const fn branch_scan_select_ns(self) -> u64 {
        self.branch_scan_select_ns
    }

    /// Nanoseconds spent emitting selected branch scan rows and applying limits.
    pub const fn branch_scan_emit_ns(self) -> u64 {
        self.branch_scan_emit_ns
    }

    /// Number of logical physical-key encodes performed by branch scan grouping.
    pub const fn scan_logical_key_encodes(self) -> u64 {
        self.scan_logical_key_encodes
    }

    /// Number of candidate rows cloned during branch scan materialization.
    pub const fn scan_candidate_row_clones(self) -> u64 {
        self.scan_candidate_row_clones
    }

    /// Estimated bytes copied by candidate row clones during branch scans.
    pub const fn scan_candidate_row_clone_bytes(self) -> u64 {
        self.scan_candidate_row_clone_bytes
    }

    /// Number of ordered table seeks performed by the serving path.
    pub const fn table_seeks(self) -> u64 {
        self.table_seeks
    }

    /// Number of bounded table-key checks performed by scan cursors.
    pub const fn table_bound_checks(self) -> u64 {
        self.table_bound_checks
    }

    /// Nanoseconds spent checking bounded table-key predicates.
    pub const fn table_bound_check_ns(self) -> u64 {
        self.table_bound_check_ns
    }
}

#[cfg(feature = "perf-trace")]
pub(crate) type PerfTraceTimer = Instant;
#[cfg(not(feature = "perf-trace"))]
pub(crate) type PerfTraceTimer = ();

#[cfg(feature = "perf-trace")]
static API_COMMIT_MAP_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static API_COMMIT_RUNTIME_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static API_SCAN_RUNTIME_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static API_SCAN_MAP_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static API_SCAN_BOUNDS_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static RUNTIME_BATCH_VALIDATE_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static RUNTIME_DUPLICATE_MUTATION_KEY_CHECKS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_PREPARE_ROWS_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static APPEND_BATCH_VALIDATE_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static APPEND_INSERT_ROWS_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static APPEND_ABSENT_INTERNAL_KEY_CHECKS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static MUTABLE_INSERT_DUPLICATE_CHECKS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_BATCHES_PREPARED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_USER_MUTATION_ROWS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_TIMELINE_ROWS_PREPARED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_ROWS_PREPARED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static APPEND_ROWS_APPLIED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static BRANCH_FACTS_ROWS_OBSERVED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static READ_VIEW_CAPTURES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static READ_VIEW_ROWS_CLONED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static READ_VIEW_VALIDATION_ROWS_SCANNED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static APPEND_STAGING_CLONES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static APPEND_STAGING_ROWS_CLONED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static CONFLICT_SOURCES_BUILT: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static POINT_ROWS_VISITED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static POINT_CANDIDATES_MATERIALIZED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static SCAN_ROWS_VISITED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static SCAN_CANDIDATES_MATERIALIZED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static SCAN_CURSOR_SEEKS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static SCAN_CURSOR_ROWS_YIELDED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static BRANCH_SCAN_SOURCE_SETUP_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static BRANCH_SCAN_MERGE_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static BRANCH_SCAN_MIN_KEY_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static BRANCH_SCAN_GROUP_KEY_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static BRANCH_SCAN_CANDIDATE_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static BRANCH_SCAN_ADVANCE_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static BRANCH_SCAN_SELECT_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static BRANCH_SCAN_EMIT_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static SCAN_LOGICAL_KEY_ENCODES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static SCAN_CANDIDATE_ROW_CLONES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static SCAN_CANDIDATE_ROW_CLONE_BYTES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_SEEKS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_BOUND_CHECKS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_BOUND_CHECK_NS: AtomicU64 = AtomicU64::new(0);

#[cfg(all(test, feature = "perf-trace"))]
thread_local! {
    static TEST_CAPTURE_ENABLED: Cell<bool> = const { Cell::new(false) };
}

#[cfg(all(test, feature = "perf-trace"))]
static TEST_CAPTURE_LOCK: Mutex<()> = Mutex::new(());

#[cfg(all(test, feature = "perf-trace"))]
pub(crate) struct PerfTraceTestGuard {
    _lock: MutexGuard<'static, ()>,
    previous_enabled: bool,
}

#[cfg(all(test, feature = "perf-trace"))]
impl Drop for PerfTraceTestGuard {
    fn drop(&mut self) {
        TEST_CAPTURE_ENABLED.with(|enabled| enabled.set(self.previous_enabled));
    }
}

/// Start an isolated counter capture for a single unit-test thread.
#[cfg(all(test, feature = "perf-trace"))]
pub(crate) fn begin_test_capture() -> PerfTraceTestGuard {
    let lock = TEST_CAPTURE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let previous_enabled = TEST_CAPTURE_ENABLED.with(|enabled| {
        let previous = enabled.get();
        enabled.set(true);
        previous
    });
    reset();
    PerfTraceTestGuard {
        _lock: lock,
        previous_enabled,
    }
}

/// Reset all performance proof counters.
#[cfg(feature = "perf-trace")]
pub fn reset() {
    API_COMMIT_MAP_NS.store(0, Ordering::Relaxed);
    API_COMMIT_RUNTIME_NS.store(0, Ordering::Relaxed);
    API_SCAN_RUNTIME_NS.store(0, Ordering::Relaxed);
    API_SCAN_MAP_NS.store(0, Ordering::Relaxed);
    API_SCAN_BOUNDS_NS.store(0, Ordering::Relaxed);
    RUNTIME_BATCH_VALIDATE_NS.store(0, Ordering::Relaxed);
    RUNTIME_DUPLICATE_MUTATION_KEY_CHECKS.store(0, Ordering::Relaxed);
    COMMIT_PREPARE_ROWS_NS.store(0, Ordering::Relaxed);
    APPEND_BATCH_VALIDATE_NS.store(0, Ordering::Relaxed);
    APPEND_INSERT_ROWS_NS.store(0, Ordering::Relaxed);
    APPEND_ABSENT_INTERNAL_KEY_CHECKS.store(0, Ordering::Relaxed);
    MUTABLE_INSERT_DUPLICATE_CHECKS.store(0, Ordering::Relaxed);
    COMMIT_BATCHES_PREPARED.store(0, Ordering::Relaxed);
    COMMIT_USER_MUTATION_ROWS.store(0, Ordering::Relaxed);
    COMMIT_TIMELINE_ROWS_PREPARED.store(0, Ordering::Relaxed);
    COMMIT_ROWS_PREPARED.store(0, Ordering::Relaxed);
    APPEND_ROWS_APPLIED.store(0, Ordering::Relaxed);
    BRANCH_FACTS_ROWS_OBSERVED.store(0, Ordering::Relaxed);
    READ_VIEW_CAPTURES.store(0, Ordering::Relaxed);
    READ_VIEW_ROWS_CLONED.store(0, Ordering::Relaxed);
    READ_VIEW_VALIDATION_ROWS_SCANNED.store(0, Ordering::Relaxed);
    APPEND_STAGING_CLONES.store(0, Ordering::Relaxed);
    APPEND_STAGING_ROWS_CLONED.store(0, Ordering::Relaxed);
    CONFLICT_SOURCES_BUILT.store(0, Ordering::Relaxed);
    POINT_ROWS_VISITED.store(0, Ordering::Relaxed);
    POINT_CANDIDATES_MATERIALIZED.store(0, Ordering::Relaxed);
    SCAN_ROWS_VISITED.store(0, Ordering::Relaxed);
    SCAN_CANDIDATES_MATERIALIZED.store(0, Ordering::Relaxed);
    SCAN_CURSOR_SEEKS.store(0, Ordering::Relaxed);
    SCAN_CURSOR_ROWS_YIELDED.store(0, Ordering::Relaxed);
    BRANCH_SCAN_SOURCE_SETUP_NS.store(0, Ordering::Relaxed);
    BRANCH_SCAN_MERGE_NS.store(0, Ordering::Relaxed);
    BRANCH_SCAN_MIN_KEY_NS.store(0, Ordering::Relaxed);
    BRANCH_SCAN_GROUP_KEY_NS.store(0, Ordering::Relaxed);
    BRANCH_SCAN_CANDIDATE_NS.store(0, Ordering::Relaxed);
    BRANCH_SCAN_ADVANCE_NS.store(0, Ordering::Relaxed);
    BRANCH_SCAN_SELECT_NS.store(0, Ordering::Relaxed);
    BRANCH_SCAN_EMIT_NS.store(0, Ordering::Relaxed);
    SCAN_LOGICAL_KEY_ENCODES.store(0, Ordering::Relaxed);
    SCAN_CANDIDATE_ROW_CLONES.store(0, Ordering::Relaxed);
    SCAN_CANDIDATE_ROW_CLONE_BYTES.store(0, Ordering::Relaxed);
    TABLE_SEEKS.store(0, Ordering::Relaxed);
    TABLE_BOUND_CHECKS.store(0, Ordering::Relaxed);
    TABLE_BOUND_CHECK_NS.store(0, Ordering::Relaxed);
}

/// Capture all performance proof counters.
#[cfg(feature = "perf-trace")]
pub fn snapshot() -> StoragePerfSnapshot {
    StoragePerfSnapshot {
        api_commit_map_ns: API_COMMIT_MAP_NS.load(Ordering::Relaxed),
        api_commit_runtime_ns: API_COMMIT_RUNTIME_NS.load(Ordering::Relaxed),
        api_scan_runtime_ns: API_SCAN_RUNTIME_NS.load(Ordering::Relaxed),
        api_scan_map_ns: API_SCAN_MAP_NS.load(Ordering::Relaxed),
        api_scan_bounds_ns: API_SCAN_BOUNDS_NS.load(Ordering::Relaxed),
        runtime_batch_validate_ns: RUNTIME_BATCH_VALIDATE_NS.load(Ordering::Relaxed),
        runtime_duplicate_mutation_key_checks: RUNTIME_DUPLICATE_MUTATION_KEY_CHECKS
            .load(Ordering::Relaxed),
        commit_prepare_rows_ns: COMMIT_PREPARE_ROWS_NS.load(Ordering::Relaxed),
        append_batch_validate_ns: APPEND_BATCH_VALIDATE_NS.load(Ordering::Relaxed),
        append_insert_rows_ns: APPEND_INSERT_ROWS_NS.load(Ordering::Relaxed),
        append_absent_internal_key_checks: APPEND_ABSENT_INTERNAL_KEY_CHECKS
            .load(Ordering::Relaxed),
        mutable_insert_duplicate_checks: MUTABLE_INSERT_DUPLICATE_CHECKS.load(Ordering::Relaxed),
        commit_batches_prepared: COMMIT_BATCHES_PREPARED.load(Ordering::Relaxed),
        commit_user_mutation_rows: COMMIT_USER_MUTATION_ROWS.load(Ordering::Relaxed),
        commit_timeline_rows_prepared: COMMIT_TIMELINE_ROWS_PREPARED.load(Ordering::Relaxed),
        commit_rows_prepared: COMMIT_ROWS_PREPARED.load(Ordering::Relaxed),
        append_rows_applied: APPEND_ROWS_APPLIED.load(Ordering::Relaxed),
        branch_facts_rows_observed: BRANCH_FACTS_ROWS_OBSERVED.load(Ordering::Relaxed),
        read_view_captures: READ_VIEW_CAPTURES.load(Ordering::Relaxed),
        read_view_rows_cloned: READ_VIEW_ROWS_CLONED.load(Ordering::Relaxed),
        read_view_validation_rows_scanned: READ_VIEW_VALIDATION_ROWS_SCANNED
            .load(Ordering::Relaxed),
        append_staging_clones: APPEND_STAGING_CLONES.load(Ordering::Relaxed),
        append_staging_rows_cloned: APPEND_STAGING_ROWS_CLONED.load(Ordering::Relaxed),
        conflict_sources_built: CONFLICT_SOURCES_BUILT.load(Ordering::Relaxed),
        point_rows_visited: POINT_ROWS_VISITED.load(Ordering::Relaxed),
        point_candidates_materialized: POINT_CANDIDATES_MATERIALIZED.load(Ordering::Relaxed),
        scan_rows_visited: SCAN_ROWS_VISITED.load(Ordering::Relaxed),
        scan_candidates_materialized: SCAN_CANDIDATES_MATERIALIZED.load(Ordering::Relaxed),
        scan_cursor_seeks: SCAN_CURSOR_SEEKS.load(Ordering::Relaxed),
        scan_cursor_rows_yielded: SCAN_CURSOR_ROWS_YIELDED.load(Ordering::Relaxed),
        branch_scan_source_setup_ns: BRANCH_SCAN_SOURCE_SETUP_NS.load(Ordering::Relaxed),
        branch_scan_merge_ns: BRANCH_SCAN_MERGE_NS.load(Ordering::Relaxed),
        branch_scan_min_key_ns: BRANCH_SCAN_MIN_KEY_NS.load(Ordering::Relaxed),
        branch_scan_group_key_ns: BRANCH_SCAN_GROUP_KEY_NS.load(Ordering::Relaxed),
        branch_scan_candidate_ns: BRANCH_SCAN_CANDIDATE_NS.load(Ordering::Relaxed),
        branch_scan_advance_ns: BRANCH_SCAN_ADVANCE_NS.load(Ordering::Relaxed),
        branch_scan_select_ns: BRANCH_SCAN_SELECT_NS.load(Ordering::Relaxed),
        branch_scan_emit_ns: BRANCH_SCAN_EMIT_NS.load(Ordering::Relaxed),
        scan_logical_key_encodes: SCAN_LOGICAL_KEY_ENCODES.load(Ordering::Relaxed),
        scan_candidate_row_clones: SCAN_CANDIDATE_ROW_CLONES.load(Ordering::Relaxed),
        scan_candidate_row_clone_bytes: SCAN_CANDIDATE_ROW_CLONE_BYTES.load(Ordering::Relaxed),
        table_seeks: TABLE_SEEKS.load(Ordering::Relaxed),
        table_bound_checks: TABLE_BOUND_CHECKS.load(Ordering::Relaxed),
        table_bound_check_ns: TABLE_BOUND_CHECK_NS.load(Ordering::Relaxed),
    }
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn start_timer() -> PerfTraceTimer {}

#[cfg(feature = "perf-trace")]
pub(crate) fn start_timer() -> PerfTraceTimer {
    Instant::now()
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_api_commit_map_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_api_commit_map_elapsed(start: PerfTraceTimer) {
    record_elapsed(&API_COMMIT_MAP_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_api_commit_runtime_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_api_commit_runtime_elapsed(start: PerfTraceTimer) {
    record_elapsed(&API_COMMIT_RUNTIME_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_api_scan_runtime_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_api_scan_runtime_elapsed(start: PerfTraceTimer) {
    record_elapsed(&API_SCAN_RUNTIME_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_api_scan_map_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_api_scan_map_elapsed(start: PerfTraceTimer) {
    record_elapsed(&API_SCAN_MAP_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_api_scan_bounds_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_api_scan_bounds_elapsed(start: PerfTraceTimer) {
    record_elapsed(&API_SCAN_BOUNDS_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_runtime_batch_validate_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_runtime_batch_validate_elapsed(start: PerfTraceTimer) {
    record_elapsed(&RUNTIME_BATCH_VALIDATE_NS, start);
}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_runtime_duplicate_mutation_key_checks(checks: usize) {
    if !recording_enabled() {
        return;
    }
    RUNTIME_DUPLICATE_MUTATION_KEY_CHECKS.fetch_add(as_u64(checks), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_prepare_rows_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_prepare_rows_elapsed(start: PerfTraceTimer) {
    record_elapsed(&COMMIT_PREPARE_ROWS_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_append_batch_validate_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_append_batch_validate_elapsed(start: PerfTraceTimer) {
    record_elapsed(&APPEND_BATCH_VALIDATE_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_append_insert_rows_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_append_insert_rows_elapsed(start: PerfTraceTimer) {
    record_elapsed(&APPEND_INSERT_ROWS_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_append_absent_internal_key_check() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_append_absent_internal_key_check() {
    if !recording_enabled() {
        return;
    }
    APPEND_ABSENT_INTERNAL_KEY_CHECKS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_mutable_insert_duplicate_check() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_mutable_insert_duplicate_check() {
    if !recording_enabled() {
        return;
    }
    MUTABLE_INSERT_DUPLICATE_CHECKS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_rows_prepared(
    _user_mutation_rows: usize,
    _timeline_rows: usize,
    _total_rows: usize,
) {
}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_rows_prepared(
    user_mutation_rows: usize,
    timeline_rows: usize,
    total_rows: usize,
) {
    if !recording_enabled() {
        return;
    }
    COMMIT_BATCHES_PREPARED.fetch_add(1, Ordering::Relaxed);
    COMMIT_USER_MUTATION_ROWS.fetch_add(as_u64(user_mutation_rows), Ordering::Relaxed);
    COMMIT_TIMELINE_ROWS_PREPARED.fetch_add(as_u64(timeline_rows), Ordering::Relaxed);
    COMMIT_ROWS_PREPARED.fetch_add(as_u64(total_rows), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_append_rows_applied(_rows: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_append_rows_applied(rows: usize) {
    if !recording_enabled() {
        return;
    }
    APPEND_ROWS_APPLIED.fetch_add(as_u64(rows), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_branch_facts_observed(_rows_observed: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_branch_facts_observed(rows_observed: usize) {
    if !recording_enabled() {
        return;
    }
    BRANCH_FACTS_ROWS_OBSERVED.fetch_add(as_u64(rows_observed), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_read_view_capture(_rows_cloned: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_read_view_capture(rows_cloned: usize) {
    if !recording_enabled() {
        return;
    }
    READ_VIEW_CAPTURES.fetch_add(1, Ordering::Relaxed);
    READ_VIEW_ROWS_CLONED.fetch_add(as_u64(rows_cloned), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_read_view_validation_scan(_rows_scanned: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_read_view_validation_scan(rows_scanned: usize) {
    if !recording_enabled() {
        return;
    }
    READ_VIEW_VALIDATION_ROWS_SCANNED.fetch_add(as_u64(rows_scanned), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_conflict_source_built() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_conflict_source_built() {
    if !recording_enabled() {
        return;
    }
    CONFLICT_SOURCES_BUILT.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_point_candidate_collection(_rows_visited: usize, _candidates: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_point_candidate_collection(rows_visited: usize, candidates: usize) {
    if !recording_enabled() {
        return;
    }
    POINT_ROWS_VISITED.fetch_add(as_u64(rows_visited), Ordering::Relaxed);
    POINT_CANDIDATES_MATERIALIZED.fetch_add(as_u64(candidates), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_scan_candidate_collection(_rows_visited: usize, _candidates: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_scan_candidate_collection(rows_visited: usize, candidates: usize) {
    if !recording_enabled() {
        return;
    }
    SCAN_ROWS_VISITED.fetch_add(as_u64(rows_visited), Ordering::Relaxed);
    SCAN_CANDIDATES_MATERIALIZED.fetch_add(as_u64(candidates), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_scan_cursor_seek() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_scan_cursor_seek() {
    if !recording_enabled() {
        return;
    }
    SCAN_CURSOR_SEEKS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_scan_cursor_row_yielded() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_scan_cursor_row_yielded() {
    if !recording_enabled() {
        return;
    }
    SCAN_CURSOR_ROWS_YIELDED.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_branch_scan_source_setup_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_branch_scan_source_setup_elapsed(start: PerfTraceTimer) {
    record_elapsed(&BRANCH_SCAN_SOURCE_SETUP_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_branch_scan_merge_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_branch_scan_merge_elapsed(start: PerfTraceTimer) {
    record_elapsed(&BRANCH_SCAN_MERGE_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_branch_scan_min_key_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_branch_scan_min_key_elapsed(start: PerfTraceTimer) {
    record_elapsed(&BRANCH_SCAN_MIN_KEY_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_branch_scan_group_key_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_branch_scan_group_key_elapsed(start: PerfTraceTimer) {
    record_elapsed(&BRANCH_SCAN_GROUP_KEY_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_branch_scan_candidate_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_branch_scan_candidate_elapsed(start: PerfTraceTimer) {
    record_elapsed(&BRANCH_SCAN_CANDIDATE_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_branch_scan_advance_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_branch_scan_advance_elapsed(start: PerfTraceTimer) {
    record_elapsed(&BRANCH_SCAN_ADVANCE_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_branch_scan_select_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_branch_scan_select_elapsed(start: PerfTraceTimer) {
    record_elapsed(&BRANCH_SCAN_SELECT_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_branch_scan_emit_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_branch_scan_emit_elapsed(start: PerfTraceTimer) {
    record_elapsed(&BRANCH_SCAN_EMIT_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_scan_logical_key_encode() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_scan_logical_key_encode() {
    if !recording_enabled() {
        return;
    }
    SCAN_LOGICAL_KEY_ENCODES.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_scan_candidate_row_clone(_bytes: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_scan_candidate_row_clone(bytes: usize) {
    if !recording_enabled() {
        return;
    }
    SCAN_CANDIDATE_ROW_CLONES.fetch_add(1, Ordering::Relaxed);
    SCAN_CANDIDATE_ROW_CLONE_BYTES.fetch_add(as_u64(bytes), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_seek() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_seek() {
    if !recording_enabled() {
        return;
    }
    TABLE_SEEKS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_bound_check_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_bound_check_elapsed(start: PerfTraceTimer) {
    if !recording_enabled() {
        return;
    }
    TABLE_BOUND_CHECKS.fetch_add(1, Ordering::Relaxed);
    record_elapsed(&TABLE_BOUND_CHECK_NS, start);
}

#[cfg(all(test, feature = "perf-trace"))]
fn recording_enabled() -> bool {
    TEST_CAPTURE_ENABLED.with(Cell::get)
}

#[cfg(all(not(test), feature = "perf-trace"))]
const fn recording_enabled() -> bool {
    true
}

#[cfg(feature = "perf-trace")]
fn as_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

#[cfg(feature = "perf-trace")]
fn record_elapsed(counter: &AtomicU64, start: PerfTraceTimer) {
    if !recording_enabled() {
        return;
    }
    let nanos = start.elapsed().as_nanos();
    counter.fetch_add(u64::try_from(nanos).unwrap_or(u64::MAX), Ordering::Relaxed);
}
