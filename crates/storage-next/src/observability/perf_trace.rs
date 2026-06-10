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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct BranchPointSourceCounts {
    pub(crate) active_probes: usize,
    pub(crate) frozen_probes: usize,
    pub(crate) owned_l0_table_probes: usize,
    pub(crate) owned_nonzero_level_searches: usize,
    pub(crate) owned_nonzero_table_probes: usize,
    pub(crate) inherited_layer_searches: usize,
    pub(crate) inherited_l0_table_probes: usize,
    pub(crate) inherited_nonzero_level_searches: usize,
    pub(crate) inherited_nonzero_table_probes: usize,
    pub(crate) table_seeks: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct BranchScanSourceCounts {
    pub(crate) active_cursors: usize,
    pub(crate) frozen_cursors: usize,
    pub(crate) owned_l0_cursors: usize,
    pub(crate) owned_nonzero_level_cursors: usize,
    pub(crate) owned_nonzero_table_cursors_opened: usize,
    pub(crate) inherited_l0_cursors: usize,
    pub(crate) inherited_nonzero_level_cursors: usize,
    pub(crate) inherited_nonzero_table_cursors_opened: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct BranchSourceRowCounts {
    pub(crate) active: usize,
    pub(crate) frozen: usize,
    pub(crate) owned_l0: usize,
    pub(crate) owned_nonzero: usize,
    pub(crate) inherited_l0: usize,
    pub(crate) inherited_nonzero: usize,
}

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
    read_view_source_handles_cloned: u64,
    read_view_rows_cloned: u64,
    read_view_row_clone_bytes: u64,
    read_view_validation_rows_scanned: u64,
    branch_compaction_source_opens: u64,
    branch_compaction_peak_buffered_rows: u64,
    branch_materialization_source_opens: u64,
    branch_materialization_rows_rewritten: u64,
    branch_materialization_rows_skipped_by_fork: u64,
    branch_materialization_rows_skipped_by_shadowing: u64,
    branch_materialization_output_tables: u64,
    branch_materialization_peak_buffered_rows: u64,
    table_compaction_merge_cursor_opens: u64,
    table_compaction_merge_advances: u64,
    table_compaction_pre_validation_rows_scanned: u64,
    table_compaction_row_clones: u64,
    table_compaction_heap_key_clones: u64,
    table_compaction_source_order_key_clones: u64,
    table_compaction_boundary_key_allocations: u64,
    table_compaction_kept_rows: u64,
    table_compaction_dropped_rows: u64,
    table_compaction_peak_buffered_rows: u64,
    table_compaction_output_tables_built: u64,
    append_staging_clones: u64,
    append_staging_rows_cloned: u64,
    conflict_sources_built: u64,
    point_rows_visited: u64,
    point_candidates_materialized: u64,
    point_active_probes: u64,
    point_frozen_probes: u64,
    point_owned_l0_table_probes: u64,
    point_owned_nonzero_level_searches: u64,
    point_owned_nonzero_table_probes: u64,
    point_inherited_layer_searches: u64,
    point_inherited_l0_table_probes: u64,
    point_inherited_nonzero_level_searches: u64,
    point_inherited_nonzero_table_probes: u64,
    point_table_seeks: u64,
    scan_rows_visited: u64,
    scan_candidates_materialized: u64,
    scan_cursor_seeks: u64,
    scan_cursor_rows_yielded: u64,
    scan_active_cursors: u64,
    scan_frozen_cursors: u64,
    scan_owned_l0_cursors: u64,
    scan_owned_nonzero_level_cursors: u64,
    scan_owned_nonzero_table_cursors_opened: u64,
    scan_inherited_l0_cursors: u64,
    scan_inherited_nonzero_level_cursors: u64,
    scan_inherited_nonzero_table_cursors_opened: u64,
    scan_source_cursor_seeks: u64,
    scan_rows_returned: u64,
    history_active_rows_visited: u64,
    history_frozen_rows_visited: u64,
    history_owned_l0_rows_visited: u64,
    history_owned_nonzero_rows_visited: u64,
    history_inherited_l0_rows_visited: u64,
    history_inherited_nonzero_rows_visited: u64,
    history_candidates_materialized: u64,
    timestamp_active_rows_scanned: u64,
    timestamp_frozen_rows_scanned: u64,
    timestamp_owned_l0_rows_scanned: u64,
    timestamp_owned_nonzero_rows_scanned: u64,
    timestamp_inherited_l0_rows_scanned: u64,
    timestamp_inherited_nonzero_rows_scanned: u64,
    branch_facts_active_rows_observed: u64,
    branch_facts_frozen_rows_observed: u64,
    branch_facts_owned_l0_rows_observed: u64,
    branch_facts_owned_nonzero_rows_observed: u64,
    branch_facts_inherited_l0_rows_observed: u64,
    branch_facts_inherited_nonzero_rows_observed: u64,
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
    table_reader_opens: u64,
    table_metadata_read_bytes: u64,
    table_index_read_bytes: u64,
    table_properties_read_bytes: u64,
    table_data_block_reads: u64,
    table_data_block_read_bytes: u64,
    table_data_block_decodes: u64,
    table_rows_decoded: u64,
    table_point_rows_visited: u64,
    table_cursor_rows_visited: u64,
    table_cache_hits: u64,
    table_cache_misses: u64,
    table_cache_inserts: u64,
    table_cache_skipped_inserts: u64,
    table_filter_probes: u64,
    table_filter_negative_probes: u64,
    table_filter_positive_probes: u64,
    table_filter_absent_probes: u64,
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

    /// Number of branch source handles copied into captured read views.
    pub const fn read_view_source_handles_cloned(self) -> u64 {
        self.read_view_source_handles_cloned
    }

    /// Number of rows copied into captured branch read views.
    pub const fn read_view_rows_cloned(self) -> u64 {
        self.read_view_rows_cloned
    }

    /// Approximate bytes copied into captured branch read views.
    pub const fn read_view_row_clone_bytes(self) -> u64 {
        self.read_view_row_clone_bytes
    }

    /// Number of branch rows scanned while validating captured read-view facts.
    pub const fn read_view_validation_rows_scanned(self) -> u64 {
        self.read_view_validation_rows_scanned
    }

    /// Number of branch compaction source handles selected for table compaction.
    pub const fn branch_compaction_source_opens(self) -> u64 {
        self.branch_compaction_source_opens
    }

    /// Peak rows buffered by branch-owned compaction output construction.
    pub const fn branch_compaction_peak_buffered_rows(self) -> u64 {
        self.branch_compaction_peak_buffered_rows
    }

    /// Number of inherited tables opened while streaming branch materialization.
    pub const fn branch_materialization_source_opens(self) -> u64 {
        self.branch_materialization_source_opens
    }

    /// Number of inherited rows rewritten into the child branch during materialization.
    pub const fn branch_materialization_rows_rewritten(self) -> u64 {
        self.branch_materialization_rows_rewritten
    }

    /// Number of inherited materialization rows skipped because they are after the fork.
    pub const fn branch_materialization_rows_skipped_by_fork(self) -> u64 {
        self.branch_materialization_rows_skipped_by_fork
    }

    /// Number of inherited materialization rows skipped because child state already shadows them.
    pub const fn branch_materialization_rows_skipped_by_shadowing(self) -> u64 {
        self.branch_materialization_rows_skipped_by_shadowing
    }

    /// Number of replacement tables produced by branch materialization.
    pub const fn branch_materialization_output_tables(self) -> u64 {
        self.branch_materialization_output_tables
    }

    /// Peak rows buffered while building branch materialization replacement tables.
    pub const fn branch_materialization_peak_buffered_rows(self) -> u64 {
        self.branch_materialization_peak_buffered_rows
    }

    /// Number of table-compaction source cursors opened by merge cursors.
    pub const fn table_compaction_merge_cursor_opens(self) -> u64 {
        self.table_compaction_merge_cursor_opens
    }

    /// Number of merge-cursor advances performed by table compaction.
    pub const fn table_compaction_merge_advances(self) -> u64 {
        self.table_compaction_merge_advances
    }

    /// Rows scanned by table compaction's pre-merge validation pass.
    pub const fn table_compaction_pre_validation_rows_scanned(self) -> u64 {
        self.table_compaction_pre_validation_rows_scanned
    }

    /// Rows cloned by table compaction for output ownership.
    pub const fn table_compaction_row_clones(self) -> u64 {
        self.table_compaction_row_clones
    }

    /// Internal keys cloned into table compaction heap items.
    pub const fn table_compaction_heap_key_clones(self) -> u64 {
        self.table_compaction_heap_key_clones
    }

    /// Internal keys cloned for table compaction source-order validation.
    pub const fn table_compaction_source_order_key_clones(self) -> u64 {
        self.table_compaction_source_order_key_clones
    }

    /// Boundary-key byte allocations performed by table compaction.
    pub const fn table_compaction_boundary_key_allocations(self) -> u64 {
        self.table_compaction_boundary_key_allocations
    }

    /// Rows kept by table compaction policy decisions.
    pub const fn table_compaction_kept_rows(self) -> u64 {
        self.table_compaction_kept_rows
    }

    /// Rows dropped by table compaction policy decisions.
    pub const fn table_compaction_dropped_rows(self) -> u64 {
        self.table_compaction_dropped_rows
    }

    /// Peak rows buffered by table compaction output construction.
    pub const fn table_compaction_peak_buffered_rows(self) -> u64 {
        self.table_compaction_peak_buffered_rows
    }

    /// Output table artifacts built by table compaction.
    pub const fn table_compaction_output_tables_built(self) -> u64 {
        self.table_compaction_output_tables_built
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

    /// Branch active-table probes performed by point reads.
    pub const fn point_active_probes(self) -> u64 {
        self.point_active_probes
    }

    /// Branch frozen-table probes performed by point reads.
    pub const fn point_frozen_probes(self) -> u64 {
        self.point_frozen_probes
    }

    /// Branch-owned L0 table probes performed by point reads.
    pub const fn point_owned_l0_table_probes(self) -> u64 {
        self.point_owned_l0_table_probes
    }

    /// Branch-owned nonzero level searches performed by point reads.
    pub const fn point_owned_nonzero_level_searches(self) -> u64 {
        self.point_owned_nonzero_level_searches
    }

    /// Branch-owned nonzero table probes performed by point reads.
    pub const fn point_owned_nonzero_table_probes(self) -> u64 {
        self.point_owned_nonzero_table_probes
    }

    /// Inherited layers searched by point reads.
    pub const fn point_inherited_layer_searches(self) -> u64 {
        self.point_inherited_layer_searches
    }

    /// Inherited L0 table probes performed by point reads.
    pub const fn point_inherited_l0_table_probes(self) -> u64 {
        self.point_inherited_l0_table_probes
    }

    /// Inherited nonzero level searches performed by point reads.
    pub const fn point_inherited_nonzero_level_searches(self) -> u64 {
        self.point_inherited_nonzero_level_searches
    }

    /// Inherited nonzero table probes performed by point reads.
    pub const fn point_inherited_nonzero_table_probes(self) -> u64 {
        self.point_inherited_nonzero_table_probes
    }

    /// Branch-level seek/probe calls performed by point reads.
    pub const fn point_table_seeks(self) -> u64 {
        self.point_table_seeks
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

    /// Active table cursors opened by branch scans.
    pub const fn scan_active_cursors(self) -> u64 {
        self.scan_active_cursors
    }

    /// Frozen table cursors opened by branch scans.
    pub const fn scan_frozen_cursors(self) -> u64 {
        self.scan_frozen_cursors
    }

    /// Owned L0 table cursors opened by branch scans.
    pub const fn scan_owned_l0_cursors(self) -> u64 {
        self.scan_owned_l0_cursors
    }

    /// Owned nonzero level cursors opened by branch scans.
    pub const fn scan_owned_nonzero_level_cursors(self) -> u64 {
        self.scan_owned_nonzero_level_cursors
    }

    /// Owned nonzero table cursors opened by branch scans.
    pub const fn scan_owned_nonzero_table_cursors_opened(self) -> u64 {
        self.scan_owned_nonzero_table_cursors_opened
    }

    /// Inherited L0 table cursors opened by branch scans.
    pub const fn scan_inherited_l0_cursors(self) -> u64 {
        self.scan_inherited_l0_cursors
    }

    /// Inherited nonzero level cursors opened by branch scans.
    pub const fn scan_inherited_nonzero_level_cursors(self) -> u64 {
        self.scan_inherited_nonzero_level_cursors
    }

    /// Inherited nonzero table cursors opened by branch scans.
    pub const fn scan_inherited_nonzero_table_cursors_opened(self) -> u64 {
        self.scan_inherited_nonzero_table_cursors_opened
    }

    /// Branch scan source cursors seeked during setup.
    pub const fn scan_source_cursor_seeks(self) -> u64 {
        self.scan_source_cursor_seeks
    }

    /// Rows returned by branch scan calls.
    pub const fn scan_rows_returned(self) -> u64 {
        self.scan_rows_returned
    }

    /// Active rows visited by single-key history collection.
    pub const fn history_active_rows_visited(self) -> u64 {
        self.history_active_rows_visited
    }

    /// Frozen rows visited by single-key history collection.
    pub const fn history_frozen_rows_visited(self) -> u64 {
        self.history_frozen_rows_visited
    }

    /// Owned L0 rows visited by single-key history collection.
    pub const fn history_owned_l0_rows_visited(self) -> u64 {
        self.history_owned_l0_rows_visited
    }

    /// Owned nonzero rows visited by single-key history collection.
    pub const fn history_owned_nonzero_rows_visited(self) -> u64 {
        self.history_owned_nonzero_rows_visited
    }

    /// Inherited L0 rows visited by single-key history collection.
    pub const fn history_inherited_l0_rows_visited(self) -> u64 {
        self.history_inherited_l0_rows_visited
    }

    /// Inherited nonzero rows visited by single-key history collection.
    pub const fn history_inherited_nonzero_rows_visited(self) -> u64 {
        self.history_inherited_nonzero_rows_visited
    }

    /// Candidate rows materialized by single-key history collection.
    pub const fn history_candidates_materialized(self) -> u64 {
        self.history_candidates_materialized
    }

    /// Active rows scanned while resolving timestamps to commit versions.
    pub const fn timestamp_active_rows_scanned(self) -> u64 {
        self.timestamp_active_rows_scanned
    }

    /// Frozen rows scanned while resolving timestamps to commit versions.
    pub const fn timestamp_frozen_rows_scanned(self) -> u64 {
        self.timestamp_frozen_rows_scanned
    }

    /// Owned L0 rows scanned while resolving timestamps to commit versions.
    pub const fn timestamp_owned_l0_rows_scanned(self) -> u64 {
        self.timestamp_owned_l0_rows_scanned
    }

    /// Owned nonzero rows scanned while resolving timestamps to commit versions.
    pub const fn timestamp_owned_nonzero_rows_scanned(self) -> u64 {
        self.timestamp_owned_nonzero_rows_scanned
    }

    /// Inherited L0 rows scanned while resolving timestamps to commit versions.
    pub const fn timestamp_inherited_l0_rows_scanned(self) -> u64 {
        self.timestamp_inherited_l0_rows_scanned
    }

    /// Inherited nonzero rows scanned while resolving timestamps to commit versions.
    pub const fn timestamp_inherited_nonzero_rows_scanned(self) -> u64 {
        self.timestamp_inherited_nonzero_rows_scanned
    }

    /// Active rows observed while deriving branch facts.
    pub const fn branch_facts_active_rows_observed(self) -> u64 {
        self.branch_facts_active_rows_observed
    }

    /// Frozen rows observed while deriving branch facts.
    pub const fn branch_facts_frozen_rows_observed(self) -> u64 {
        self.branch_facts_frozen_rows_observed
    }

    /// Owned L0 rows observed while deriving branch facts.
    pub const fn branch_facts_owned_l0_rows_observed(self) -> u64 {
        self.branch_facts_owned_l0_rows_observed
    }

    /// Owned nonzero rows observed while deriving branch facts.
    pub const fn branch_facts_owned_nonzero_rows_observed(self) -> u64 {
        self.branch_facts_owned_nonzero_rows_observed
    }

    /// Inherited L0 rows observed while deriving branch facts.
    pub const fn branch_facts_inherited_l0_rows_observed(self) -> u64 {
        self.branch_facts_inherited_l0_rows_observed
    }

    /// Inherited nonzero rows observed while deriving branch facts.
    pub const fn branch_facts_inherited_nonzero_rows_observed(self) -> u64 {
        self.branch_facts_inherited_nonzero_rows_observed
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

    /// Number of immutable table reader opens performed.
    pub const fn table_reader_opens(self) -> u64 {
        self.table_reader_opens
    }

    /// Bytes read for table header/footer metadata.
    pub const fn table_metadata_read_bytes(self) -> u64 {
        self.table_metadata_read_bytes
    }

    /// Bytes read for table index blocks.
    pub const fn table_index_read_bytes(self) -> u64 {
        self.table_index_read_bytes
    }

    /// Bytes read for table properties blocks.
    pub const fn table_properties_read_bytes(self) -> u64 {
        self.table_properties_read_bytes
    }

    /// Number of table data-block source reads performed.
    pub const fn table_data_block_reads(self) -> u64 {
        self.table_data_block_reads
    }

    /// Bytes read for table data-block frames.
    pub const fn table_data_block_read_bytes(self) -> u64 {
        self.table_data_block_read_bytes
    }

    /// Number of table data blocks decoded.
    pub const fn table_data_block_decodes(self) -> u64 {
        self.table_data_block_decodes
    }

    /// Number of table rows decoded from immutable table data blocks.
    pub const fn table_rows_decoded(self) -> u64 {
        self.table_rows_decoded
    }

    /// Number of table rows visited during table-local point lookup.
    pub const fn table_point_rows_visited(self) -> u64 {
        self.table_point_rows_visited
    }

    /// Number of table rows reached by immutable table cursor movement.
    pub const fn table_cursor_rows_visited(self) -> u64 {
        self.table_cursor_rows_visited
    }

    /// Number of table block-cache hits.
    pub const fn table_cache_hits(self) -> u64 {
        self.table_cache_hits
    }

    /// Number of table block-cache misses.
    pub const fn table_cache_misses(self) -> u64 {
        self.table_cache_misses
    }

    /// Number of table block-cache inserts.
    pub const fn table_cache_inserts(self) -> u64 {
        self.table_cache_inserts
    }

    /// Number of table block-cache insert attempts skipped by cache policy.
    pub const fn table_cache_skipped_inserts(self) -> u64 {
        self.table_cache_skipped_inserts
    }

    /// Number of table filter probes.
    pub const fn table_filter_probes(self) -> u64 {
        self.table_filter_probes
    }

    /// Number of table filter probes that returned definitely absent.
    pub const fn table_filter_negative_probes(self) -> u64 {
        self.table_filter_negative_probes
    }

    /// Number of table filter probes that returned maybe present.
    pub const fn table_filter_positive_probes(self) -> u64 {
        self.table_filter_positive_probes
    }

    /// Number of table filter probes where the filter was unavailable.
    pub const fn table_filter_absent_probes(self) -> u64 {
        self.table_filter_absent_probes
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
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct PerfTraceTimer;

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
static READ_VIEW_SOURCE_HANDLES_CLONED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static READ_VIEW_ROWS_CLONED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static READ_VIEW_ROW_CLONE_BYTES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static READ_VIEW_VALIDATION_ROWS_SCANNED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static BRANCH_COMPACTION_SOURCE_OPENS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static BRANCH_COMPACTION_PEAK_BUFFERED_ROWS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static BRANCH_MATERIALIZATION_SOURCE_OPENS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static BRANCH_MATERIALIZATION_ROWS_REWRITTEN: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static BRANCH_MATERIALIZATION_ROWS_SKIPPED_BY_FORK: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static BRANCH_MATERIALIZATION_ROWS_SKIPPED_BY_SHADOWING: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static BRANCH_MATERIALIZATION_OUTPUT_TABLES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static BRANCH_MATERIALIZATION_PEAK_BUFFERED_ROWS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_COMPACTION_MERGE_CURSOR_OPENS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_COMPACTION_MERGE_ADVANCES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_COMPACTION_PRE_VALIDATION_ROWS_SCANNED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_COMPACTION_ROW_CLONES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_COMPACTION_HEAP_KEY_CLONES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_COMPACTION_SOURCE_ORDER_KEY_CLONES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_COMPACTION_BOUNDARY_KEY_ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_COMPACTION_KEPT_ROWS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_COMPACTION_DROPPED_ROWS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_COMPACTION_PEAK_BUFFERED_ROWS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_COMPACTION_OUTPUT_TABLES_BUILT: AtomicU64 = AtomicU64::new(0);
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
static POINT_ACTIVE_PROBES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static POINT_FROZEN_PROBES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static POINT_OWNED_L0_TABLE_PROBES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static POINT_OWNED_NONZERO_LEVEL_SEARCHES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static POINT_OWNED_NONZERO_TABLE_PROBES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static POINT_INHERITED_LAYER_SEARCHES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static POINT_INHERITED_L0_TABLE_PROBES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static POINT_INHERITED_NONZERO_LEVEL_SEARCHES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static POINT_INHERITED_NONZERO_TABLE_PROBES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static POINT_TABLE_SEEKS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static SCAN_ROWS_VISITED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static SCAN_CANDIDATES_MATERIALIZED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static SCAN_CURSOR_SEEKS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static SCAN_CURSOR_ROWS_YIELDED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static SCAN_ACTIVE_CURSORS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static SCAN_FROZEN_CURSORS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static SCAN_OWNED_L0_CURSORS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static SCAN_OWNED_NONZERO_LEVEL_CURSORS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static SCAN_OWNED_NONZERO_TABLE_CURSORS_OPENED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static SCAN_INHERITED_L0_CURSORS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static SCAN_INHERITED_NONZERO_LEVEL_CURSORS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static SCAN_INHERITED_NONZERO_TABLE_CURSORS_OPENED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static SCAN_SOURCE_CURSOR_SEEKS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static SCAN_ROWS_RETURNED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static HISTORY_ACTIVE_ROWS_VISITED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static HISTORY_FROZEN_ROWS_VISITED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static HISTORY_OWNED_L0_ROWS_VISITED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static HISTORY_OWNED_NONZERO_ROWS_VISITED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static HISTORY_INHERITED_L0_ROWS_VISITED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static HISTORY_INHERITED_NONZERO_ROWS_VISITED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static HISTORY_CANDIDATES_MATERIALIZED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TIMESTAMP_ACTIVE_ROWS_SCANNED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TIMESTAMP_FROZEN_ROWS_SCANNED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TIMESTAMP_OWNED_L0_ROWS_SCANNED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TIMESTAMP_OWNED_NONZERO_ROWS_SCANNED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TIMESTAMP_INHERITED_L0_ROWS_SCANNED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TIMESTAMP_INHERITED_NONZERO_ROWS_SCANNED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static BRANCH_FACTS_ACTIVE_ROWS_OBSERVED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static BRANCH_FACTS_FROZEN_ROWS_OBSERVED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static BRANCH_FACTS_OWNED_L0_ROWS_OBSERVED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static BRANCH_FACTS_OWNED_NONZERO_ROWS_OBSERVED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static BRANCH_FACTS_INHERITED_L0_ROWS_OBSERVED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static BRANCH_FACTS_INHERITED_NONZERO_ROWS_OBSERVED: AtomicU64 = AtomicU64::new(0);
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
static TABLE_READER_OPENS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_METADATA_READ_BYTES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_INDEX_READ_BYTES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_PROPERTIES_READ_BYTES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_DATA_BLOCK_READS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_DATA_BLOCK_READ_BYTES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_DATA_BLOCK_DECODES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_ROWS_DECODED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_POINT_ROWS_VISITED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_CURSOR_ROWS_VISITED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_CACHE_HITS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_CACHE_MISSES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_CACHE_INSERTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_CACHE_SKIPPED_INSERTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_FILTER_PROBES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_FILTER_NEGATIVE_PROBES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_FILTER_POSITIVE_PROBES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_FILTER_ABSENT_PROBES: AtomicU64 = AtomicU64::new(0);
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
#[allow(clippy::too_many_lines)]
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
    READ_VIEW_SOURCE_HANDLES_CLONED.store(0, Ordering::Relaxed);
    READ_VIEW_ROWS_CLONED.store(0, Ordering::Relaxed);
    READ_VIEW_ROW_CLONE_BYTES.store(0, Ordering::Relaxed);
    READ_VIEW_VALIDATION_ROWS_SCANNED.store(0, Ordering::Relaxed);
    BRANCH_COMPACTION_SOURCE_OPENS.store(0, Ordering::Relaxed);
    BRANCH_COMPACTION_PEAK_BUFFERED_ROWS.store(0, Ordering::Relaxed);
    BRANCH_MATERIALIZATION_SOURCE_OPENS.store(0, Ordering::Relaxed);
    BRANCH_MATERIALIZATION_ROWS_REWRITTEN.store(0, Ordering::Relaxed);
    BRANCH_MATERIALIZATION_ROWS_SKIPPED_BY_FORK.store(0, Ordering::Relaxed);
    BRANCH_MATERIALIZATION_ROWS_SKIPPED_BY_SHADOWING.store(0, Ordering::Relaxed);
    BRANCH_MATERIALIZATION_OUTPUT_TABLES.store(0, Ordering::Relaxed);
    BRANCH_MATERIALIZATION_PEAK_BUFFERED_ROWS.store(0, Ordering::Relaxed);
    TABLE_COMPACTION_MERGE_CURSOR_OPENS.store(0, Ordering::Relaxed);
    TABLE_COMPACTION_MERGE_ADVANCES.store(0, Ordering::Relaxed);
    TABLE_COMPACTION_PRE_VALIDATION_ROWS_SCANNED.store(0, Ordering::Relaxed);
    TABLE_COMPACTION_ROW_CLONES.store(0, Ordering::Relaxed);
    TABLE_COMPACTION_HEAP_KEY_CLONES.store(0, Ordering::Relaxed);
    TABLE_COMPACTION_SOURCE_ORDER_KEY_CLONES.store(0, Ordering::Relaxed);
    TABLE_COMPACTION_BOUNDARY_KEY_ALLOCATIONS.store(0, Ordering::Relaxed);
    TABLE_COMPACTION_KEPT_ROWS.store(0, Ordering::Relaxed);
    TABLE_COMPACTION_DROPPED_ROWS.store(0, Ordering::Relaxed);
    TABLE_COMPACTION_PEAK_BUFFERED_ROWS.store(0, Ordering::Relaxed);
    TABLE_COMPACTION_OUTPUT_TABLES_BUILT.store(0, Ordering::Relaxed);
    APPEND_STAGING_CLONES.store(0, Ordering::Relaxed);
    APPEND_STAGING_ROWS_CLONED.store(0, Ordering::Relaxed);
    CONFLICT_SOURCES_BUILT.store(0, Ordering::Relaxed);
    POINT_ROWS_VISITED.store(0, Ordering::Relaxed);
    POINT_CANDIDATES_MATERIALIZED.store(0, Ordering::Relaxed);
    POINT_ACTIVE_PROBES.store(0, Ordering::Relaxed);
    POINT_FROZEN_PROBES.store(0, Ordering::Relaxed);
    POINT_OWNED_L0_TABLE_PROBES.store(0, Ordering::Relaxed);
    POINT_OWNED_NONZERO_LEVEL_SEARCHES.store(0, Ordering::Relaxed);
    POINT_OWNED_NONZERO_TABLE_PROBES.store(0, Ordering::Relaxed);
    POINT_INHERITED_LAYER_SEARCHES.store(0, Ordering::Relaxed);
    POINT_INHERITED_L0_TABLE_PROBES.store(0, Ordering::Relaxed);
    POINT_INHERITED_NONZERO_LEVEL_SEARCHES.store(0, Ordering::Relaxed);
    POINT_INHERITED_NONZERO_TABLE_PROBES.store(0, Ordering::Relaxed);
    POINT_TABLE_SEEKS.store(0, Ordering::Relaxed);
    SCAN_ROWS_VISITED.store(0, Ordering::Relaxed);
    SCAN_CANDIDATES_MATERIALIZED.store(0, Ordering::Relaxed);
    SCAN_CURSOR_SEEKS.store(0, Ordering::Relaxed);
    SCAN_CURSOR_ROWS_YIELDED.store(0, Ordering::Relaxed);
    SCAN_ACTIVE_CURSORS.store(0, Ordering::Relaxed);
    SCAN_FROZEN_CURSORS.store(0, Ordering::Relaxed);
    SCAN_OWNED_L0_CURSORS.store(0, Ordering::Relaxed);
    SCAN_OWNED_NONZERO_LEVEL_CURSORS.store(0, Ordering::Relaxed);
    SCAN_OWNED_NONZERO_TABLE_CURSORS_OPENED.store(0, Ordering::Relaxed);
    SCAN_INHERITED_L0_CURSORS.store(0, Ordering::Relaxed);
    SCAN_INHERITED_NONZERO_LEVEL_CURSORS.store(0, Ordering::Relaxed);
    SCAN_INHERITED_NONZERO_TABLE_CURSORS_OPENED.store(0, Ordering::Relaxed);
    SCAN_SOURCE_CURSOR_SEEKS.store(0, Ordering::Relaxed);
    SCAN_ROWS_RETURNED.store(0, Ordering::Relaxed);
    HISTORY_ACTIVE_ROWS_VISITED.store(0, Ordering::Relaxed);
    HISTORY_FROZEN_ROWS_VISITED.store(0, Ordering::Relaxed);
    HISTORY_OWNED_L0_ROWS_VISITED.store(0, Ordering::Relaxed);
    HISTORY_OWNED_NONZERO_ROWS_VISITED.store(0, Ordering::Relaxed);
    HISTORY_INHERITED_L0_ROWS_VISITED.store(0, Ordering::Relaxed);
    HISTORY_INHERITED_NONZERO_ROWS_VISITED.store(0, Ordering::Relaxed);
    HISTORY_CANDIDATES_MATERIALIZED.store(0, Ordering::Relaxed);
    TIMESTAMP_ACTIVE_ROWS_SCANNED.store(0, Ordering::Relaxed);
    TIMESTAMP_FROZEN_ROWS_SCANNED.store(0, Ordering::Relaxed);
    TIMESTAMP_OWNED_L0_ROWS_SCANNED.store(0, Ordering::Relaxed);
    TIMESTAMP_OWNED_NONZERO_ROWS_SCANNED.store(0, Ordering::Relaxed);
    TIMESTAMP_INHERITED_L0_ROWS_SCANNED.store(0, Ordering::Relaxed);
    TIMESTAMP_INHERITED_NONZERO_ROWS_SCANNED.store(0, Ordering::Relaxed);
    BRANCH_FACTS_ACTIVE_ROWS_OBSERVED.store(0, Ordering::Relaxed);
    BRANCH_FACTS_FROZEN_ROWS_OBSERVED.store(0, Ordering::Relaxed);
    BRANCH_FACTS_OWNED_L0_ROWS_OBSERVED.store(0, Ordering::Relaxed);
    BRANCH_FACTS_OWNED_NONZERO_ROWS_OBSERVED.store(0, Ordering::Relaxed);
    BRANCH_FACTS_INHERITED_L0_ROWS_OBSERVED.store(0, Ordering::Relaxed);
    BRANCH_FACTS_INHERITED_NONZERO_ROWS_OBSERVED.store(0, Ordering::Relaxed);
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
    TABLE_READER_OPENS.store(0, Ordering::Relaxed);
    TABLE_METADATA_READ_BYTES.store(0, Ordering::Relaxed);
    TABLE_INDEX_READ_BYTES.store(0, Ordering::Relaxed);
    TABLE_PROPERTIES_READ_BYTES.store(0, Ordering::Relaxed);
    TABLE_DATA_BLOCK_READS.store(0, Ordering::Relaxed);
    TABLE_DATA_BLOCK_READ_BYTES.store(0, Ordering::Relaxed);
    TABLE_DATA_BLOCK_DECODES.store(0, Ordering::Relaxed);
    TABLE_ROWS_DECODED.store(0, Ordering::Relaxed);
    TABLE_POINT_ROWS_VISITED.store(0, Ordering::Relaxed);
    TABLE_CURSOR_ROWS_VISITED.store(0, Ordering::Relaxed);
    TABLE_CACHE_HITS.store(0, Ordering::Relaxed);
    TABLE_CACHE_MISSES.store(0, Ordering::Relaxed);
    TABLE_CACHE_INSERTS.store(0, Ordering::Relaxed);
    TABLE_CACHE_SKIPPED_INSERTS.store(0, Ordering::Relaxed);
    TABLE_FILTER_PROBES.store(0, Ordering::Relaxed);
    TABLE_FILTER_NEGATIVE_PROBES.store(0, Ordering::Relaxed);
    TABLE_FILTER_POSITIVE_PROBES.store(0, Ordering::Relaxed);
    TABLE_FILTER_ABSENT_PROBES.store(0, Ordering::Relaxed);
    TABLE_SEEKS.store(0, Ordering::Relaxed);
    TABLE_BOUND_CHECKS.store(0, Ordering::Relaxed);
    TABLE_BOUND_CHECK_NS.store(0, Ordering::Relaxed);
}

/// Capture all performance proof counters.
#[cfg(feature = "perf-trace")]
#[allow(clippy::too_many_lines)]
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
        read_view_source_handles_cloned: READ_VIEW_SOURCE_HANDLES_CLONED.load(Ordering::Relaxed),
        read_view_rows_cloned: READ_VIEW_ROWS_CLONED.load(Ordering::Relaxed),
        read_view_row_clone_bytes: READ_VIEW_ROW_CLONE_BYTES.load(Ordering::Relaxed),
        read_view_validation_rows_scanned: READ_VIEW_VALIDATION_ROWS_SCANNED
            .load(Ordering::Relaxed),
        branch_compaction_source_opens: BRANCH_COMPACTION_SOURCE_OPENS.load(Ordering::Relaxed),
        branch_compaction_peak_buffered_rows: BRANCH_COMPACTION_PEAK_BUFFERED_ROWS
            .load(Ordering::Relaxed),
        branch_materialization_source_opens: BRANCH_MATERIALIZATION_SOURCE_OPENS
            .load(Ordering::Relaxed),
        branch_materialization_rows_rewritten: BRANCH_MATERIALIZATION_ROWS_REWRITTEN
            .load(Ordering::Relaxed),
        branch_materialization_rows_skipped_by_fork: BRANCH_MATERIALIZATION_ROWS_SKIPPED_BY_FORK
            .load(Ordering::Relaxed),
        branch_materialization_rows_skipped_by_shadowing:
            BRANCH_MATERIALIZATION_ROWS_SKIPPED_BY_SHADOWING.load(Ordering::Relaxed),
        branch_materialization_output_tables: BRANCH_MATERIALIZATION_OUTPUT_TABLES
            .load(Ordering::Relaxed),
        branch_materialization_peak_buffered_rows: BRANCH_MATERIALIZATION_PEAK_BUFFERED_ROWS
            .load(Ordering::Relaxed),
        table_compaction_merge_cursor_opens: TABLE_COMPACTION_MERGE_CURSOR_OPENS
            .load(Ordering::Relaxed),
        table_compaction_merge_advances: TABLE_COMPACTION_MERGE_ADVANCES.load(Ordering::Relaxed),
        table_compaction_pre_validation_rows_scanned: TABLE_COMPACTION_PRE_VALIDATION_ROWS_SCANNED
            .load(Ordering::Relaxed),
        table_compaction_row_clones: TABLE_COMPACTION_ROW_CLONES.load(Ordering::Relaxed),
        table_compaction_heap_key_clones: TABLE_COMPACTION_HEAP_KEY_CLONES.load(Ordering::Relaxed),
        table_compaction_source_order_key_clones: TABLE_COMPACTION_SOURCE_ORDER_KEY_CLONES
            .load(Ordering::Relaxed),
        table_compaction_boundary_key_allocations: TABLE_COMPACTION_BOUNDARY_KEY_ALLOCATIONS
            .load(Ordering::Relaxed),
        table_compaction_kept_rows: TABLE_COMPACTION_KEPT_ROWS.load(Ordering::Relaxed),
        table_compaction_dropped_rows: TABLE_COMPACTION_DROPPED_ROWS.load(Ordering::Relaxed),
        table_compaction_peak_buffered_rows: TABLE_COMPACTION_PEAK_BUFFERED_ROWS
            .load(Ordering::Relaxed),
        table_compaction_output_tables_built: TABLE_COMPACTION_OUTPUT_TABLES_BUILT
            .load(Ordering::Relaxed),
        append_staging_clones: APPEND_STAGING_CLONES.load(Ordering::Relaxed),
        append_staging_rows_cloned: APPEND_STAGING_ROWS_CLONED.load(Ordering::Relaxed),
        conflict_sources_built: CONFLICT_SOURCES_BUILT.load(Ordering::Relaxed),
        point_rows_visited: POINT_ROWS_VISITED.load(Ordering::Relaxed),
        point_candidates_materialized: POINT_CANDIDATES_MATERIALIZED.load(Ordering::Relaxed),
        point_active_probes: POINT_ACTIVE_PROBES.load(Ordering::Relaxed),
        point_frozen_probes: POINT_FROZEN_PROBES.load(Ordering::Relaxed),
        point_owned_l0_table_probes: POINT_OWNED_L0_TABLE_PROBES.load(Ordering::Relaxed),
        point_owned_nonzero_level_searches: POINT_OWNED_NONZERO_LEVEL_SEARCHES
            .load(Ordering::Relaxed),
        point_owned_nonzero_table_probes: POINT_OWNED_NONZERO_TABLE_PROBES.load(Ordering::Relaxed),
        point_inherited_layer_searches: POINT_INHERITED_LAYER_SEARCHES.load(Ordering::Relaxed),
        point_inherited_l0_table_probes: POINT_INHERITED_L0_TABLE_PROBES.load(Ordering::Relaxed),
        point_inherited_nonzero_level_searches: POINT_INHERITED_NONZERO_LEVEL_SEARCHES
            .load(Ordering::Relaxed),
        point_inherited_nonzero_table_probes: POINT_INHERITED_NONZERO_TABLE_PROBES
            .load(Ordering::Relaxed),
        point_table_seeks: POINT_TABLE_SEEKS.load(Ordering::Relaxed),
        scan_rows_visited: SCAN_ROWS_VISITED.load(Ordering::Relaxed),
        scan_candidates_materialized: SCAN_CANDIDATES_MATERIALIZED.load(Ordering::Relaxed),
        scan_cursor_seeks: SCAN_CURSOR_SEEKS.load(Ordering::Relaxed),
        scan_cursor_rows_yielded: SCAN_CURSOR_ROWS_YIELDED.load(Ordering::Relaxed),
        scan_active_cursors: SCAN_ACTIVE_CURSORS.load(Ordering::Relaxed),
        scan_frozen_cursors: SCAN_FROZEN_CURSORS.load(Ordering::Relaxed),
        scan_owned_l0_cursors: SCAN_OWNED_L0_CURSORS.load(Ordering::Relaxed),
        scan_owned_nonzero_level_cursors: SCAN_OWNED_NONZERO_LEVEL_CURSORS.load(Ordering::Relaxed),
        scan_owned_nonzero_table_cursors_opened: SCAN_OWNED_NONZERO_TABLE_CURSORS_OPENED
            .load(Ordering::Relaxed),
        scan_inherited_l0_cursors: SCAN_INHERITED_L0_CURSORS.load(Ordering::Relaxed),
        scan_inherited_nonzero_level_cursors: SCAN_INHERITED_NONZERO_LEVEL_CURSORS
            .load(Ordering::Relaxed),
        scan_inherited_nonzero_table_cursors_opened: SCAN_INHERITED_NONZERO_TABLE_CURSORS_OPENED
            .load(Ordering::Relaxed),
        scan_source_cursor_seeks: SCAN_SOURCE_CURSOR_SEEKS.load(Ordering::Relaxed),
        scan_rows_returned: SCAN_ROWS_RETURNED.load(Ordering::Relaxed),
        history_active_rows_visited: HISTORY_ACTIVE_ROWS_VISITED.load(Ordering::Relaxed),
        history_frozen_rows_visited: HISTORY_FROZEN_ROWS_VISITED.load(Ordering::Relaxed),
        history_owned_l0_rows_visited: HISTORY_OWNED_L0_ROWS_VISITED.load(Ordering::Relaxed),
        history_owned_nonzero_rows_visited: HISTORY_OWNED_NONZERO_ROWS_VISITED
            .load(Ordering::Relaxed),
        history_inherited_l0_rows_visited: HISTORY_INHERITED_L0_ROWS_VISITED
            .load(Ordering::Relaxed),
        history_inherited_nonzero_rows_visited: HISTORY_INHERITED_NONZERO_ROWS_VISITED
            .load(Ordering::Relaxed),
        history_candidates_materialized: HISTORY_CANDIDATES_MATERIALIZED.load(Ordering::Relaxed),
        timestamp_active_rows_scanned: TIMESTAMP_ACTIVE_ROWS_SCANNED.load(Ordering::Relaxed),
        timestamp_frozen_rows_scanned: TIMESTAMP_FROZEN_ROWS_SCANNED.load(Ordering::Relaxed),
        timestamp_owned_l0_rows_scanned: TIMESTAMP_OWNED_L0_ROWS_SCANNED.load(Ordering::Relaxed),
        timestamp_owned_nonzero_rows_scanned: TIMESTAMP_OWNED_NONZERO_ROWS_SCANNED
            .load(Ordering::Relaxed),
        timestamp_inherited_l0_rows_scanned: TIMESTAMP_INHERITED_L0_ROWS_SCANNED
            .load(Ordering::Relaxed),
        timestamp_inherited_nonzero_rows_scanned: TIMESTAMP_INHERITED_NONZERO_ROWS_SCANNED
            .load(Ordering::Relaxed),
        branch_facts_active_rows_observed: BRANCH_FACTS_ACTIVE_ROWS_OBSERVED
            .load(Ordering::Relaxed),
        branch_facts_frozen_rows_observed: BRANCH_FACTS_FROZEN_ROWS_OBSERVED
            .load(Ordering::Relaxed),
        branch_facts_owned_l0_rows_observed: BRANCH_FACTS_OWNED_L0_ROWS_OBSERVED
            .load(Ordering::Relaxed),
        branch_facts_owned_nonzero_rows_observed: BRANCH_FACTS_OWNED_NONZERO_ROWS_OBSERVED
            .load(Ordering::Relaxed),
        branch_facts_inherited_l0_rows_observed: BRANCH_FACTS_INHERITED_L0_ROWS_OBSERVED
            .load(Ordering::Relaxed),
        branch_facts_inherited_nonzero_rows_observed: BRANCH_FACTS_INHERITED_NONZERO_ROWS_OBSERVED
            .load(Ordering::Relaxed),
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
        table_reader_opens: TABLE_READER_OPENS.load(Ordering::Relaxed),
        table_metadata_read_bytes: TABLE_METADATA_READ_BYTES.load(Ordering::Relaxed),
        table_index_read_bytes: TABLE_INDEX_READ_BYTES.load(Ordering::Relaxed),
        table_properties_read_bytes: TABLE_PROPERTIES_READ_BYTES.load(Ordering::Relaxed),
        table_data_block_reads: TABLE_DATA_BLOCK_READS.load(Ordering::Relaxed),
        table_data_block_read_bytes: TABLE_DATA_BLOCK_READ_BYTES.load(Ordering::Relaxed),
        table_data_block_decodes: TABLE_DATA_BLOCK_DECODES.load(Ordering::Relaxed),
        table_rows_decoded: TABLE_ROWS_DECODED.load(Ordering::Relaxed),
        table_point_rows_visited: TABLE_POINT_ROWS_VISITED.load(Ordering::Relaxed),
        table_cursor_rows_visited: TABLE_CURSOR_ROWS_VISITED.load(Ordering::Relaxed),
        table_cache_hits: TABLE_CACHE_HITS.load(Ordering::Relaxed),
        table_cache_misses: TABLE_CACHE_MISSES.load(Ordering::Relaxed),
        table_cache_inserts: TABLE_CACHE_INSERTS.load(Ordering::Relaxed),
        table_cache_skipped_inserts: TABLE_CACHE_SKIPPED_INSERTS.load(Ordering::Relaxed),
        table_filter_probes: TABLE_FILTER_PROBES.load(Ordering::Relaxed),
        table_filter_negative_probes: TABLE_FILTER_NEGATIVE_PROBES.load(Ordering::Relaxed),
        table_filter_positive_probes: TABLE_FILTER_POSITIVE_PROBES.load(Ordering::Relaxed),
        table_filter_absent_probes: TABLE_FILTER_ABSENT_PROBES.load(Ordering::Relaxed),
        table_seeks: TABLE_SEEKS.load(Ordering::Relaxed),
        table_bound_checks: TABLE_BOUND_CHECKS.load(Ordering::Relaxed),
        table_bound_check_ns: TABLE_BOUND_CHECK_NS.load(Ordering::Relaxed),
    }
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn start_timer() -> PerfTraceTimer {
    PerfTraceTimer
}

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
pub(crate) fn record_read_view_capture(
    _source_handles_cloned: usize,
    _rows_cloned: usize,
    _row_clone_bytes: usize,
) {
}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_read_view_capture(
    source_handles_cloned: usize,
    rows_cloned: usize,
    row_clone_bytes: usize,
) {
    if !recording_enabled() {
        return;
    }
    READ_VIEW_CAPTURES.fetch_add(1, Ordering::Relaxed);
    READ_VIEW_SOURCE_HANDLES_CLONED.fetch_add(as_u64(source_handles_cloned), Ordering::Relaxed);
    READ_VIEW_ROWS_CLONED.fetch_add(as_u64(rows_cloned), Ordering::Relaxed);
    READ_VIEW_ROW_CLONE_BYTES.fetch_add(as_u64(row_clone_bytes), Ordering::Relaxed);
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
pub(crate) fn record_branch_compaction_source_opens(_sources: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_branch_compaction_source_opens(sources: usize) {
    if !recording_enabled() {
        return;
    }
    BRANCH_COMPACTION_SOURCE_OPENS.fetch_add(as_u64(sources), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_branch_compaction_peak_buffered_rows(_rows: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_branch_compaction_peak_buffered_rows(rows: usize) {
    if !recording_enabled() {
        return;
    }
    BRANCH_COMPACTION_PEAK_BUFFERED_ROWS.fetch_max(as_u64(rows), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_branch_materialization_source_opens(_sources: u64) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_branch_materialization_source_opens(sources: u64) {
    if !recording_enabled() {
        return;
    }
    BRANCH_MATERIALIZATION_SOURCE_OPENS.fetch_add(sources, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_branch_materialization_rows_rewritten(_rows: u64) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_branch_materialization_rows_rewritten(rows: u64) {
    if !recording_enabled() {
        return;
    }
    BRANCH_MATERIALIZATION_ROWS_REWRITTEN.fetch_add(rows, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_branch_materialization_rows_skipped_by_fork(_rows: u64) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_branch_materialization_rows_skipped_by_fork(rows: u64) {
    if !recording_enabled() {
        return;
    }
    BRANCH_MATERIALIZATION_ROWS_SKIPPED_BY_FORK.fetch_add(rows, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_branch_materialization_rows_skipped_by_shadowing(_rows: u64) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_branch_materialization_rows_skipped_by_shadowing(rows: u64) {
    if !recording_enabled() {
        return;
    }
    BRANCH_MATERIALIZATION_ROWS_SKIPPED_BY_SHADOWING.fetch_add(rows, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_branch_materialization_output_tables(_tables: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_branch_materialization_output_tables(tables: usize) {
    if !recording_enabled() {
        return;
    }
    BRANCH_MATERIALIZATION_OUTPUT_TABLES.fetch_add(as_u64(tables), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_branch_materialization_peak_buffered_rows(_rows: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_branch_materialization_peak_buffered_rows(rows: usize) {
    if !recording_enabled() {
        return;
    }
    BRANCH_MATERIALIZATION_PEAK_BUFFERED_ROWS.fetch_max(as_u64(rows), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_compaction_merge_cursor_opens(_cursors: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_compaction_merge_cursor_opens(cursors: usize) {
    if !recording_enabled() {
        return;
    }
    TABLE_COMPACTION_MERGE_CURSOR_OPENS.fetch_add(as_u64(cursors), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_compaction_merge_advance() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_compaction_merge_advance() {
    if !recording_enabled() {
        return;
    }
    TABLE_COMPACTION_MERGE_ADVANCES.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_compaction_pre_validation_row() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_compaction_pre_validation_row() {
    if !recording_enabled() {
        return;
    }
    TABLE_COMPACTION_PRE_VALIDATION_ROWS_SCANNED.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_compaction_heap_key_clone() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_compaction_heap_key_clone() {
    if !recording_enabled() {
        return;
    }
    TABLE_COMPACTION_HEAP_KEY_CLONES.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_compaction_source_order_key_clone() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_compaction_source_order_key_clone() {
    if !recording_enabled() {
        return;
    }
    TABLE_COMPACTION_SOURCE_ORDER_KEY_CLONES.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_compaction_boundary_key_allocation() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_compaction_boundary_key_allocation() {
    if !recording_enabled() {
        return;
    }
    TABLE_COMPACTION_BOUNDARY_KEY_ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_compaction_keep() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_compaction_keep() {
    if !recording_enabled() {
        return;
    }
    TABLE_COMPACTION_KEPT_ROWS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_compaction_drop() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_compaction_drop() {
    if !recording_enabled() {
        return;
    }
    TABLE_COMPACTION_DROPPED_ROWS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_compaction_peak_buffered_rows(_rows: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_compaction_peak_buffered_rows(rows: usize) {
    if !recording_enabled() {
        return;
    }
    TABLE_COMPACTION_PEAK_BUFFERED_ROWS.fetch_max(as_u64(rows), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_compaction_output_table_built() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_compaction_output_table_built() {
    if !recording_enabled() {
        return;
    }
    TABLE_COMPACTION_OUTPUT_TABLES_BUILT.fetch_add(1, Ordering::Relaxed);
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
pub(crate) fn record_branch_point_sources(_counts: BranchPointSourceCounts) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_branch_point_sources(counts: BranchPointSourceCounts) {
    if !recording_enabled() {
        return;
    }
    POINT_ACTIVE_PROBES.fetch_add(as_u64(counts.active_probes), Ordering::Relaxed);
    POINT_FROZEN_PROBES.fetch_add(as_u64(counts.frozen_probes), Ordering::Relaxed);
    POINT_OWNED_L0_TABLE_PROBES.fetch_add(as_u64(counts.owned_l0_table_probes), Ordering::Relaxed);
    POINT_OWNED_NONZERO_LEVEL_SEARCHES.fetch_add(
        as_u64(counts.owned_nonzero_level_searches),
        Ordering::Relaxed,
    );
    POINT_OWNED_NONZERO_TABLE_PROBES
        .fetch_add(as_u64(counts.owned_nonzero_table_probes), Ordering::Relaxed);
    POINT_INHERITED_LAYER_SEARCHES
        .fetch_add(as_u64(counts.inherited_layer_searches), Ordering::Relaxed);
    POINT_INHERITED_L0_TABLE_PROBES
        .fetch_add(as_u64(counts.inherited_l0_table_probes), Ordering::Relaxed);
    POINT_INHERITED_NONZERO_LEVEL_SEARCHES.fetch_add(
        as_u64(counts.inherited_nonzero_level_searches),
        Ordering::Relaxed,
    );
    POINT_INHERITED_NONZERO_TABLE_PROBES.fetch_add(
        as_u64(counts.inherited_nonzero_table_probes),
        Ordering::Relaxed,
    );
    POINT_TABLE_SEEKS.fetch_add(as_u64(counts.table_seeks), Ordering::Relaxed);
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
pub(crate) fn record_branch_scan_sources(_counts: BranchScanSourceCounts) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_branch_scan_sources(counts: BranchScanSourceCounts) {
    if !recording_enabled() {
        return;
    }
    SCAN_ACTIVE_CURSORS.fetch_add(as_u64(counts.active_cursors), Ordering::Relaxed);
    SCAN_FROZEN_CURSORS.fetch_add(as_u64(counts.frozen_cursors), Ordering::Relaxed);
    SCAN_OWNED_L0_CURSORS.fetch_add(as_u64(counts.owned_l0_cursors), Ordering::Relaxed);
    SCAN_OWNED_NONZERO_LEVEL_CURSORS.fetch_add(
        as_u64(counts.owned_nonzero_level_cursors),
        Ordering::Relaxed,
    );
    SCAN_OWNED_NONZERO_TABLE_CURSORS_OPENED.fetch_add(
        as_u64(counts.owned_nonzero_table_cursors_opened),
        Ordering::Relaxed,
    );
    SCAN_INHERITED_L0_CURSORS.fetch_add(as_u64(counts.inherited_l0_cursors), Ordering::Relaxed);
    SCAN_INHERITED_NONZERO_LEVEL_CURSORS.fetch_add(
        as_u64(counts.inherited_nonzero_level_cursors),
        Ordering::Relaxed,
    );
    SCAN_INHERITED_NONZERO_TABLE_CURSORS_OPENED.fetch_add(
        as_u64(counts.inherited_nonzero_table_cursors_opened),
        Ordering::Relaxed,
    );
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_branch_scan_source_cursor_seeks(_seeks: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_branch_scan_source_cursor_seeks(seeks: usize) {
    if !recording_enabled() {
        return;
    }
    SCAN_SOURCE_CURSOR_SEEKS.fetch_add(as_u64(seeks), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_branch_scan_rows_returned(_rows: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_branch_scan_rows_returned(rows: usize) {
    if !recording_enabled() {
        return;
    }
    SCAN_ROWS_RETURNED.fetch_add(as_u64(rows), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_branch_history_rows(_counts: BranchSourceRowCounts, _candidates: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_branch_history_rows(counts: BranchSourceRowCounts, candidates: usize) {
    if !recording_enabled() {
        return;
    }
    HISTORY_ACTIVE_ROWS_VISITED.fetch_add(as_u64(counts.active), Ordering::Relaxed);
    HISTORY_FROZEN_ROWS_VISITED.fetch_add(as_u64(counts.frozen), Ordering::Relaxed);
    HISTORY_OWNED_L0_ROWS_VISITED.fetch_add(as_u64(counts.owned_l0), Ordering::Relaxed);
    HISTORY_OWNED_NONZERO_ROWS_VISITED.fetch_add(as_u64(counts.owned_nonzero), Ordering::Relaxed);
    HISTORY_INHERITED_L0_ROWS_VISITED.fetch_add(as_u64(counts.inherited_l0), Ordering::Relaxed);
    HISTORY_INHERITED_NONZERO_ROWS_VISITED
        .fetch_add(as_u64(counts.inherited_nonzero), Ordering::Relaxed);
    HISTORY_CANDIDATES_MATERIALIZED.fetch_add(as_u64(candidates), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_branch_timestamp_rows(_counts: BranchSourceRowCounts) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_branch_timestamp_rows(counts: BranchSourceRowCounts) {
    if !recording_enabled() {
        return;
    }
    TIMESTAMP_ACTIVE_ROWS_SCANNED.fetch_add(as_u64(counts.active), Ordering::Relaxed);
    TIMESTAMP_FROZEN_ROWS_SCANNED.fetch_add(as_u64(counts.frozen), Ordering::Relaxed);
    TIMESTAMP_OWNED_L0_ROWS_SCANNED.fetch_add(as_u64(counts.owned_l0), Ordering::Relaxed);
    TIMESTAMP_OWNED_NONZERO_ROWS_SCANNED.fetch_add(as_u64(counts.owned_nonzero), Ordering::Relaxed);
    TIMESTAMP_INHERITED_L0_ROWS_SCANNED.fetch_add(as_u64(counts.inherited_l0), Ordering::Relaxed);
    TIMESTAMP_INHERITED_NONZERO_ROWS_SCANNED
        .fetch_add(as_u64(counts.inherited_nonzero), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_branch_fact_source_rows(_counts: BranchSourceRowCounts) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_branch_fact_source_rows(counts: BranchSourceRowCounts) {
    if !recording_enabled() {
        return;
    }
    BRANCH_FACTS_ACTIVE_ROWS_OBSERVED.fetch_add(as_u64(counts.active), Ordering::Relaxed);
    BRANCH_FACTS_FROZEN_ROWS_OBSERVED.fetch_add(as_u64(counts.frozen), Ordering::Relaxed);
    BRANCH_FACTS_OWNED_L0_ROWS_OBSERVED.fetch_add(as_u64(counts.owned_l0), Ordering::Relaxed);
    BRANCH_FACTS_OWNED_NONZERO_ROWS_OBSERVED
        .fetch_add(as_u64(counts.owned_nonzero), Ordering::Relaxed);
    BRANCH_FACTS_INHERITED_L0_ROWS_OBSERVED
        .fetch_add(as_u64(counts.inherited_l0), Ordering::Relaxed);
    BRANCH_FACTS_INHERITED_NONZERO_ROWS_OBSERVED
        .fetch_add(as_u64(counts.inherited_nonzero), Ordering::Relaxed);
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
pub(crate) fn record_table_reader_open() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_reader_open() {
    if !recording_enabled() {
        return;
    }
    TABLE_READER_OPENS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_metadata_read(_bytes: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_metadata_read(bytes: usize) {
    if !recording_enabled() {
        return;
    }
    TABLE_METADATA_READ_BYTES.fetch_add(as_u64(bytes), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_index_read(_bytes: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_index_read(bytes: usize) {
    if !recording_enabled() {
        return;
    }
    TABLE_INDEX_READ_BYTES.fetch_add(as_u64(bytes), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_properties_read(_bytes: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_properties_read(bytes: usize) {
    if !recording_enabled() {
        return;
    }
    TABLE_PROPERTIES_READ_BYTES.fetch_add(as_u64(bytes), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_data_block_read(_bytes: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_data_block_read(bytes: usize) {
    if !recording_enabled() {
        return;
    }
    TABLE_DATA_BLOCK_READS.fetch_add(1, Ordering::Relaxed);
    TABLE_DATA_BLOCK_READ_BYTES.fetch_add(as_u64(bytes), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_data_blocks_decoded(_blocks: usize, _rows: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_data_blocks_decoded(blocks: usize, rows: usize) {
    if !recording_enabled() {
        return;
    }
    TABLE_DATA_BLOCK_DECODES.fetch_add(as_u64(blocks), Ordering::Relaxed);
    TABLE_ROWS_DECODED.fetch_add(as_u64(rows), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_point_rows_visited(_rows: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_point_rows_visited(rows: usize) {
    if !recording_enabled() {
        return;
    }
    TABLE_POINT_ROWS_VISITED.fetch_add(as_u64(rows), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_cursor_row_visited() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_cursor_row_visited() {
    if !recording_enabled() {
        return;
    }
    TABLE_CURSOR_ROWS_VISITED.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_cache_hit() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_cache_hit() {
    if !recording_enabled() {
        return;
    }
    TABLE_CACHE_HITS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_cache_miss() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_cache_miss() {
    if !recording_enabled() {
        return;
    }
    TABLE_CACHE_MISSES.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_cache_insert() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_cache_insert() {
    if !recording_enabled() {
        return;
    }
    TABLE_CACHE_INSERTS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_cache_skipped_insert() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_cache_skipped_insert() {
    if !recording_enabled() {
        return;
    }
    TABLE_CACHE_SKIPPED_INSERTS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_filter_negative_probe() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_filter_negative_probe() {
    if !recording_enabled() {
        return;
    }
    TABLE_FILTER_PROBES.fetch_add(1, Ordering::Relaxed);
    TABLE_FILTER_NEGATIVE_PROBES.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_filter_positive_probe() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_filter_positive_probe() {
    if !recording_enabled() {
        return;
    }
    TABLE_FILTER_PROBES.fetch_add(1, Ordering::Relaxed);
    TABLE_FILTER_POSITIVE_PROBES.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_filter_absent_probe() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_filter_absent_probe() {
    if !recording_enabled() {
        return;
    }
    TABLE_FILTER_PROBES.fetch_add(1, Ordering::Relaxed);
    TABLE_FILTER_ABSENT_PROBES.fetch_add(1, Ordering::Relaxed);
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
