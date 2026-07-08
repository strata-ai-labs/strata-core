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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BranchPointSourceKind {
    Active,
    Frozen,
    OwnedL0,
    OwnedNonzero,
    Inherited,
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
    commit_wal_record_build_ns: u64,
    commit_wal_records_built: u64,
    commit_wal_record_rows: u64,
    commit_wal_record_bytes: u64,
    commit_wal_payload_bytes: u64,
    commit_wal_row_encode_bytes: u64,
    commit_wal_encode_buffer_allocations: u64,
    commit_wal_encode_buffer_reuses: u64,
    commit_wal_append_ns: u64,
    commit_wal_appends: u64,
    commit_wal_append_bytes: u64,
    commit_post_wal_growth_ns: u64,
    commit_wal_growth_facts_ns: u64,
    commit_wal_growth_manifest_ns: u64,
    commit_post_maintenance_ns: u64,
    commit_exec_admission_ns: u64,
    commit_exec_conflict_ns: u64,
    commit_exec_stage_ns: u64,
    commit_exec_apply_ns: u64,
    commit_exec_publish_ns: u64,
    commit_admit_ns: u64,
    commit_setup_ns: u64,
    commit_api_batch_clone_ns: u64,
    commit_api_post_ns: u64,
    commit_visible_publish_attempts: u64,
    commit_visible_publish_successes: u64,
    commit_visible_publish_failures: u64,
    commit_admission_pressure_facts: u64,
    commit_admission_under_pressure: u64,
    commit_admission_accepted_under_pressure: u64,
    commit_admission_requires_maintenance: u64,
    commit_admission_mutations: u64,
    commit_admission_approx_bytes: u64,
    commit_unresolved_gate_admission_attempts: u64,
    commit_unresolved_gate_admission_acquired: u64,
    commit_unresolved_gate_rejected_unresolved: u64,
    commit_unresolved_records: u64,
    commit_unresolved_durable_not_applied_records: u64,
    commit_unresolved_applied_not_visible_records: u64,
    lifecycle_write_admission_evaluations: u64,
    lifecycle_write_admission_clean_accepts: u64,
    lifecycle_write_admission_under_pressure_accepts: u64,
    lifecycle_write_admission_urgent_accepts: u64,
    lifecycle_write_admission_requires_maintenance: u64,
    lifecycle_write_admission_inline_attempts: u64,
    lifecycle_write_admission_urgent_inline_attempts: u64,
    lifecycle_write_admission_pressure_rejects: u64,
    lifecycle_write_admission_retryable_rejects: u64,
    lifecycle_write_admission_pressure_cleared_retries: u64,
    lifecycle_write_admission_wait_attempts: u64,
    lifecycle_write_admission_wait_timeouts: u64,
    lifecycle_write_admission_wait_progress_resets: u64,
    lifecycle_pressure_clear_wakes: u64,
    lifecycle_pressure_collection_calls: u64,
    lifecycle_pressure_collection_branches_inspected: u64,
    lifecycle_pressure_collection_levels_inspected: u64,
    lifecycle_pressure_collection_tables_inspected: u64,
    lifecycle_pressure_collection_ns: u64,
    lifecycle_pressure_collection_sampling_skips: u64,
    lifecycle_pressure_collection_full_scans: u64,
    lifecycle_active_byte_pressure_background: u64,
    lifecycle_active_byte_pressure_urgent: u64,
    lifecycle_active_byte_pressure_blocking: u64,
    lifecycle_post_commit_maintenance_evaluations: u64,
    lifecycle_post_commit_maintenance_disabled: u64,
    lifecycle_post_commit_maintenance_no_task: u64,
    lifecycle_post_commit_maintenance_tasks_suggested: u64,
    lifecycle_post_commit_maintenance_tasks_enqueued: u64,
    lifecycle_post_commit_maintenance_tasks_coalesced: u64,
    lifecycle_post_commit_maintenance_tasks_deferred: u64,
    lifecycle_maintenance_coverage_scans: u64,
    lifecycle_maintenance_coverage_branches_scanned: u64,
    lifecycle_maintenance_coverage_quiet_branches_with_pressure: u64,
    lifecycle_maintenance_coverage_tasks_enqueued: u64,
    lifecycle_maintenance_coverage_tasks_coalesced: u64,
    lifecycle_maintenance_coverage_idle_rounds_consumed: u64,
    lifecycle_maintenance_coverage_stop_no_pressure: u64,
    lifecycle_maintenance_coverage_stop_idle_limit: u64,
    lifecycle_maintenance_coverage_stop_queue_full: u64,
    lifecycle_maintenance_coverage_stop_failure: u64,
    lifecycle_inline_maintenance_attempts: u64,
    lifecycle_inline_maintenance_ns: u64,
    lifecycle_background_runtimes_created: u64,
    lifecycle_background_runtime_workers_created: u64,
    lifecycle_background_wake_submitted: u64,
    lifecycle_background_wake_coalesced: u64,
    lifecycle_background_wake_rejected: u64,
    lifecycle_background_stale_wake_noop: u64,
    lifecycle_background_drain_rounds: u64,
    lifecycle_background_tasks_completed: u64,
    lifecycle_background_shutdowns: u64,
    lifecycle_background_shutdown_joined_workers: u64,
    lifecycle_background_shutdown_drained_tasks: u64,
    lifecycle_background_shutdown_canceled_tasks: u64,
    lifecycle_background_shutdown_executor_tasks_completed: u64,
    lifecycle_background_shutdown_detached_workers: u64,
    lifecycle_background_submit_after_shutdown_rejected: u64,
    lifecycle_background_worker_panics: u64,
    lifecycle_background_task_failures: u64,
    lifecycle_background_task_start_failures: u64,
    lifecycle_background_task_publish_failures: u64,
    lifecycle_background_task_snapshot_lock_ns: u64,
    lifecycle_background_task_unlocked_build_ns: u64,
    lifecycle_background_task_publish_lock_ns: u64,
    lifecycle_background_task_low_tier_ns: u64,
    lifecycle_background_task_low_tier_runs: u64,
    lifecycle_background_publish_manifest_persist_ns: u64,
    lifecycle_background_publish_offlock_ns: u64,
    lifecycle_background_task_total_ns: u64,
    lifecycle_background_candidate_stale_deferred: u64,
    lifecycle_foreground_wait_background_lock_ns: u64,
    lifecycle_write_admission_block_wait_ns: u64,
    /// W1.4a: graded token-bucket pacing sleeps actually taken by commits.
    /// W2.3 (B3): point seeks that binary-searched the derived entry-offset
    /// accelerator instead of walking the block linearly.
    table_indexed_block_seeks: u64,
    /// W2.3 (B3): accelerator derivations (first hit per block, or miss-path).
    table_accelerator_builds: u64,
    /// W2.3 (B3): self-heal rebuilds after a malformed accelerator (~0).
    table_accelerator_rebuilds: u64,
    /// W2.6 (B1): data-block reads served by the trusted (no re-CRC, no
    /// copy) decode — cache hits on trusting consumers.
    table_trusted_block_seeks: u64,
    /// W2.6 (B1): data-block reads that ran the checked decode (misses, and
    /// always-verify consumers).
    table_checked_block_seeks: u64,
    /// W2.6 (B1): publish-time warm inserts rejected by verification.
    table_warm_insert_rejects: u64,
    /// W3.3a: WAL appends staged in the coalescing buffer.
    commit_wal_buffered_appends: u64,
    /// W3.3a: buffer drains by trigger, plus total drained bytes.
    commit_wal_buffer_flushes_threshold: u64,
    /// W3.3a: drains forced by group-sync ticket captures.
    commit_wal_buffer_flushes_capture: u64,
    /// W3.3a: drains forced by durability barriers (sync/rotation/close).
    commit_wal_buffer_flushes_durability: u64,
    /// W3.3b: drains triggered by the staleness window (trickle).
    commit_wal_buffer_flushes_trickle: u64,
    /// W3.3a: total bytes drained from the coalescing buffer.
    commit_wal_buffer_flush_bytes: u64,
    /// W3.2: wall time inside the grouped durable dispatch (join + lead +
    /// execute + wake) — the envelope around the staged commit timers.
    commit_group_dispatch_ns: u64,
    /// W3.2: wall time spent notifying the background drain from commits.
    commit_drain_notify_ns: u64,
    lifecycle_graded_throttle_delays: u64,
    lifecycle_graded_throttle_delay_ms_total: u64,
    lifecycle_graded_throttle_delay_ms_max: u64,
    /// W1.4a: debt-adaptive rate recomputes (install events) and how often the
    /// rate landed at the floor; last computed rate and max effective debt.
    lifecycle_admission_rate_recomputes: u64,
    lifecycle_admission_rate_floor_events: u64,
    lifecycle_admission_rate_last: u64,
    lifecycle_admission_effective_debt_max: u64,
    lifecycle_wal_retention_samples: u64,
    lifecycle_wal_retained_bytes_last: u64,
    lifecycle_wal_retained_bytes_max: u64,
    lifecycle_wal_retained_segments_last: u64,
    lifecycle_wal_retained_segments_max: u64,
    lifecycle_wal_checkpoint_enqueue_events: u64,
    lifecycle_wal_checkpoint_coalesced_events: u64,
    lifecycle_checkpoint_executions: u64,
    lifecycle_wal_truncation_deleted_segments: u64,
    lifecycle_wal_truncation_protected_segments: u64,
    lifecycle_wal_truncation_failed_segments: u64,
    lifecycle_flush_drain_frozen_tables_discovered: u64,
    lifecycle_flush_drain_operations_completed: u64,
    lifecycle_flush_drain_freeze_retries: u64,
    lifecycle_flush_drain_failures: u64,
    lifecycle_flush_drain_post_drain_frozen_tables: u64,
    lifecycle_flush_memory_measurements: u64,
    lifecycle_flush_active_bytes_before: u64,
    lifecycle_flush_frozen_bytes_before: u64,
    lifecycle_flush_active_bytes_after: u64,
    lifecycle_flush_frozen_bytes_after: u64,
    lifecycle_memory_release_deferrals: u64,
    lifecycle_memory_release_retained_bytes: u64,
    lifecycle_memory_release_threshold_bytes: u64,
    lifecycle_memory_release_attempts: u64,
    lifecycle_snapshot_floor_advancements: u64,
    lifecycle_snapshot_floor_implicit_rejections: u64,
    lifecycle_snapshot_pruning_with_proof: u64,
    lifecycle_snapshot_pruning_deleted: u64,
    lifecycle_snapshot_pruning_protected: u64,
    lifecycle_snapshot_pruning_failed: u64,
    lifecycle_compaction_score_candidates: u64,
    lifecycle_materialization_score_candidates: u64,
    lifecycle_materialization_score_layer_index_sum: u64,
    lifecycle_materialization_score_table_count: u64,
    lifecycle_materialization_score_byte_count: u64,
    lifecycle_compaction_selected: u64,
    lifecycle_compaction_selected_level_sum: u64,
    lifecycle_compaction_selected_score_sum: u64,
    lifecycle_compaction_selected_table_count: u64,
    lifecycle_compaction_selected_byte_count: u64,
    lifecycle_compaction_selected_target_bytes: u64,
    lifecycle_compaction_level_target_evaluations: u64,
    lifecycle_compaction_level_target_level_sum: u64,
    lifecycle_compaction_level_target_bytes: u64,
    lifecycle_compaction_nonzero_input_selections: u64,
    lifecycle_compaction_nonzero_input_level_sum: u64,
    lifecycle_compaction_nonzero_input_table_index_sum: u64,
    lifecycle_compaction_nonzero_input_bytes: u64,
    lifecycle_compaction_nonzero_input_rows: u64,
    lifecycle_compaction_largest_input_selections: u64,
    lifecycle_compaction_deeper_overlap_evaluations: u64,
    lifecycle_compaction_deeper_overlap_bytes: u64,
    lifecycle_compaction_output_split_budget_applied: u64,
    lifecycle_compaction_output_split_budget_deferred: u64,
    lifecycle_compaction_output_split_budget_deferred_bytes: u64,
    lifecycle_compaction_operations_completed: u64,
    lifecycle_compaction_l0_operations: u64,
    lifecycle_compaction_l0_to_level_one_operations: u64,
    lifecycle_compaction_nonzero_operations: u64,
    lifecycle_compaction_bottommost_operations: u64,
    lifecycle_compaction_input_tables: u64,
    lifecycle_compaction_overlap_tables: u64,
    lifecycle_compaction_output_tables: u64,
    lifecycle_compaction_input_bytes: u64,
    lifecycle_compaction_output_bytes: u64,
    lifecycle_compaction_metadata_bytes_avoided: u64,
    lifecycle_compaction_elapsed_ns: u64,
    lifecycle_compaction_input_rows: u64,
    lifecycle_compaction_rewrite_bytes_per_row: u64,
    lifecycle_compaction_io_budget_consumed_bytes: u64,
    lifecycle_compaction_io_budget_deferrals: u64,
    lifecycle_compaction_io_budget_deferred_bytes: u64,
    lifecycle_compaction_io_budget_limit_bytes: u64,
    lifecycle_compaction_flush_preemptions: u64,
    lifecycle_compaction_trivial_moves: u64,
    lifecycle_compaction_resubmits: u64,
    lifecycle_compaction_resubmit_coalesces: u64,
    lifecycle_compaction_resubmit_deferred: u64,
    lifecycle_table_rewrite_post_operation_scores: u64,
    lifecycle_table_rewrite_post_operation_remaining: u64,
    lifecycle_table_rewrite_post_operation_score_sum: u64,
    lifecycle_table_rewrite_post_operation_item_count: u64,
    lifecycle_table_rewrite_post_operation_byte_count: u64,
    commit_branch_registry_lookups: u64,
    commit_branch_registry_descriptors_scanned: u64,
    commit_branch_guard_attempts: u64,
    commit_branch_guard_acquired: u64,
    commit_branch_guard_rejected: u64,
    commit_quiesce_attempts: u64,
    commit_quiesce_acquired: u64,
    commit_quiesce_rejected: u64,
    commit_conflict_validation_calls: u64,
    commit_conflict_validation_skipped: u64,
    commit_conflict_validation_without_source: u64,
    commit_conflict_validation_with_source: u64,
    commit_conflict_read_facts_checked: u64,
    commit_conflict_cas_facts_checked: u64,
    commit_conflicts_detected: u64,
    commit_timeline_view_rows_scanned: u64,
    commit_timeline_timestamp_facts: u64,
    commit_timeline_version_facts: u64,
    commit_timeline_reconcile_calls: u64,
    commit_timeline_reconcile_timestamp_facts: u64,
    commit_timeline_reconcile_version_facts: u64,
    commit_timeline_reconcile_entry_checks: u64,
    commit_timeline_lookup_calls: u64,
    commit_timeline_lookup_entries_scanned: u64,
    commit_replay_classification_calls: u64,
    commit_replay_rows_classified: u64,
    commit_replay_history_calls: u64,
    commit_replay_source_probes: u64,
    append_rows_applied: u64,
    branch_facts_rows_observed: u64,
    read_view_captures: u64,
    read_pins: u64,
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
    table_build_facts_from_streaming_metadata: u64,
    table_rewrite_redundant_fact_decodes_avoided: u64,
    table_rewrite_reader_reopens_performed: u64,
    table_compaction_boundary_key_buffer_allocations: u64,
    table_compaction_boundary_key_buffer_reuses: u64,
    table_compaction_previous_key_buffer_allocations: u64,
    table_compaction_previous_key_buffer_reuses: u64,
    table_compaction_merge_ns: u64,
    table_compaction_merge_input_rows: u64,
    table_compaction_merge_ns_per_input_row: u64,
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
    point_candidate_row_clones: u64,
    point_candidate_row_clone_bytes: u64,
    point_selected_active: u64,
    point_selected_frozen: u64,
    point_selected_owned_l0: u64,
    point_selected_owned_nonzero: u64,
    point_selected_inherited: u64,
    point_early_exit_active: u64,
    point_early_exit_frozen: u64,
    point_early_exit_owned_l0: u64,
    point_early_exit_owned_nonzero: u64,
    point_early_exit_inherited: u64,
    point_remaining_source_skips: u64,
    point_inherited_key_rewrites: u64,
    table_point_lookup_key_builds: u64,
    table_point_lookup_key_reuses: u64,
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
    table_lazy_full_materializations: u64,
    durable_commit_admitted_over_budget: u64,
    table_metadata_read_bytes: u64,
    table_index_read_bytes: u64,
    table_properties_read_bytes: u64,
    table_data_block_reads: u64,
    table_data_block_read_bytes: u64,
    table_data_block_decodes: u64,
    table_rows_decoded: u64,
    table_lazy_point_block_scans: u64,
    table_lazy_point_entries_scanned: u64,
    table_lazy_point_rows_decoded: u64,
    table_lazy_point_full_block_decodes_avoided: u64,
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
    table_eager_filter_probes: u64,
    table_eager_filter_negative_probes: u64,
    table_eager_filter_positive_probes: u64,
    table_eager_filter_unavailable_probes: u64,
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

    /// Nanoseconds spent building durable WAL records.
    pub const fn commit_wal_record_build_ns(self) -> u64 {
        self.commit_wal_record_build_ns
    }

    /// Durable WAL records built by commit execution.
    pub const fn commit_wal_records_built(self) -> u64 {
        self.commit_wal_records_built
    }

    /// Rows included in durable WAL records built by commit execution.
    pub const fn commit_wal_record_rows(self) -> u64 {
        self.commit_wal_record_rows
    }

    /// Encoded durable WAL record bytes before the outer envelope.
    pub const fn commit_wal_record_bytes(self) -> u64 {
        self.commit_wal_record_bytes
    }

    /// Encoded durable WAL commit payload bytes.
    pub const fn commit_wal_payload_bytes(self) -> u64 {
        self.commit_wal_payload_bytes
    }

    /// Encoded durable WAL storage-row bytes across all rows.
    pub const fn commit_wal_row_encode_bytes(self) -> u64 {
        self.commit_wal_row_encode_bytes
    }

    /// Reusable WAL encode buffers that had to grow during append encoding.
    pub const fn commit_wal_encode_buffer_allocations(self) -> u64 {
        self.commit_wal_encode_buffer_allocations
    }

    /// Reusable WAL encode buffers satisfied from existing capacity.
    pub const fn commit_wal_encode_buffer_reuses(self) -> u64 {
        self.commit_wal_encode_buffer_reuses
    }

    /// Nanoseconds spent appending durable WAL records.
    pub const fn commit_wal_append_ns(self) -> u64 {
        self.commit_wal_append_ns
    }

    /// Durable WAL append calls made by commit execution.
    pub const fn commit_wal_appends(self) -> u64 {
        self.commit_wal_appends
    }

    /// Durable WAL append payload bytes reported by the WAL service.
    pub const fn commit_wal_append_bytes(self) -> u64 {
        self.commit_wal_append_bytes
    }

    /// Nanoseconds spent in the post-commit WAL-growth policy evaluation.
    pub const fn commit_post_wal_growth_ns(self) -> u64 {
        self.commit_post_wal_growth_ns
    }

    /// Nanoseconds spent gathering WAL growth facts (segment readdir + stat).
    pub const fn commit_wal_growth_facts_ns(self) -> u64 {
        self.commit_wal_growth_facts_ns
    }

    /// Nanoseconds spent loading the current manifest during WAL-growth evaluation.
    pub const fn commit_wal_growth_manifest_ns(self) -> u64 {
        self.commit_wal_growth_manifest_ns
    }

    /// Nanoseconds spent scheduling post-commit maintenance for the branch.
    pub const fn commit_post_maintenance_ns(self) -> u64 {
        self.commit_post_maintenance_ns
    }

    /// Nanoseconds spent in commit admission: entry validation, the durable gate,
    /// and branch registry/guard admission.
    pub const fn commit_exec_admission_ns(self) -> u64 {
        self.commit_exec_admission_ns
    }

    /// Nanoseconds spent in commit conflict validation.
    pub const fn commit_exec_conflict_ns(self) -> u64 {
        self.commit_exec_conflict_ns
    }

    /// Nanoseconds spent staging commit rows: allocation, prepare, pre-apply validation.
    pub const fn commit_exec_stage_ns(self) -> u64 {
        self.commit_exec_stage_ns
    }

    /// Nanoseconds spent applying committed rows to branch state after WAL append.
    pub const fn commit_exec_apply_ns(self) -> u64 {
        self.commit_exec_apply_ns
    }

    /// Nanoseconds spent publishing the new visible version.
    pub const fn commit_exec_publish_ns(self) -> u64 {
        self.commit_exec_publish_ns
    }

    /// Nanoseconds spent in lifecycle commit admission before the executor runs
    /// (state/generation/mode checks and the mutating write-admission gauntlet).
    pub const fn commit_admit_ns(self) -> u64 {
        self.commit_admit_ns
    }

    /// Nanoseconds spent fetching branch state and constructing the commit
    /// executor before `execute`.
    pub const fn commit_setup_ns(self) -> u64 {
        self.commit_setup_ns
    }

    /// Nanoseconds spent cloning the runtime batch in the api commit loop.
    pub const fn commit_api_batch_clone_ns(self) -> u64 {
        self.commit_api_batch_clone_ns
    }

    /// Nanoseconds spent in the api commit loop after the executor returns
    /// (admission/maintenance/wal-growth snapshot reads).
    pub const fn commit_api_post_ns(self) -> u64 {
        self.commit_api_post_ns
    }

    /// Visible-version publication attempts made by commit execution.
    pub const fn commit_visible_publish_attempts(self) -> u64 {
        self.commit_visible_publish_attempts
    }

    /// Successful visible-version publications made by commit execution.
    pub const fn commit_visible_publish_successes(self) -> u64 {
        self.commit_visible_publish_successes
    }

    /// Failed visible-version publications made by commit execution.
    pub const fn commit_visible_publish_failures(self) -> u64 {
        self.commit_visible_publish_failures
    }

    /// Commit admission pressure fact records emitted after batch validation.
    pub const fn commit_admission_pressure_facts(self) -> u64 {
        self.commit_admission_pressure_facts
    }

    /// Admission fact records that marked a commit over pressure thresholds.
    pub const fn commit_admission_under_pressure(self) -> u64 {
        self.commit_admission_under_pressure
    }

    /// Commits accepted for execution while over pressure thresholds.
    pub const fn commit_admission_accepted_under_pressure(self) -> u64 {
        self.commit_admission_accepted_under_pressure
    }

    /// Admission fact records that marked a commit as maintenance-worthy.
    pub const fn commit_admission_requires_maintenance(self) -> u64 {
        self.commit_admission_requires_maintenance
    }

    /// Mutations counted by admission pressure facts.
    pub const fn commit_admission_mutations(self) -> u64 {
        self.commit_admission_mutations
    }

    /// Approximate commit bytes counted by admission pressure facts.
    pub const fn commit_admission_approx_bytes(self) -> u64 {
        self.commit_admission_approx_bytes
    }

    /// Global unresolved-durable gate admission attempts.
    pub const fn commit_unresolved_gate_admission_attempts(self) -> u64 {
        self.commit_unresolved_gate_admission_attempts
    }

    /// Global unresolved-durable gate admissions acquired.
    pub const fn commit_unresolved_gate_admission_acquired(self) -> u64 {
        self.commit_unresolved_gate_admission_acquired
    }

    /// Admission attempts rejected because an unresolved durable commit exists.
    pub const fn commit_unresolved_gate_rejected_unresolved(self) -> u64 {
        self.commit_unresolved_gate_rejected_unresolved
    }

    /// Admission attempts rejected because another mutation is active.
    /// Unresolved durable commit records installed in the gate.
    pub const fn commit_unresolved_records(self) -> u64 {
        self.commit_unresolved_records
    }

    /// Durable-not-applied unresolved commit records installed in the gate.
    pub const fn commit_unresolved_durable_not_applied_records(self) -> u64 {
        self.commit_unresolved_durable_not_applied_records
    }

    /// Applied-not-visible unresolved commit records installed in the gate.
    pub const fn commit_unresolved_applied_not_visible_records(self) -> u64 {
        self.commit_unresolved_applied_not_visible_records
    }

    /// Lifecycle write-admission pressure evaluations before commit-runtime admission.
    pub const fn lifecycle_write_admission_evaluations(self) -> u64 {
        self.lifecycle_write_admission_evaluations
    }

    /// Mutating commits admitted without storage-pressure throttling.
    pub const fn lifecycle_write_admission_clean_accepts(self) -> u64 {
        self.lifecycle_write_admission_clean_accepts
    }

    /// Mutating commits admitted while storage pressure was urgent.
    pub const fn lifecycle_write_admission_under_pressure_accepts(self) -> u64 {
        self.lifecycle_write_admission_under_pressure_accepts
    }

    /// Urgent pressure admissions accepted without rejecting the caller.
    pub const fn lifecycle_write_admission_urgent_accepts(self) -> u64 {
        self.lifecycle_write_admission_urgent_accepts
    }

    /// Admission evaluations that found maintenance required before more writes.
    pub const fn lifecycle_write_admission_requires_maintenance(self) -> u64 {
        self.lifecycle_write_admission_requires_maintenance
    }

    /// Bounded inline maintenance attempts made before write admission.
    pub const fn lifecycle_write_admission_inline_attempts(self) -> u64 {
        self.lifecycle_write_admission_inline_attempts
    }

    /// Inline maintenance attempts made specifically for urgent admission pressure.
    pub const fn lifecycle_write_admission_urgent_inline_attempts(self) -> u64 {
        self.lifecycle_write_admission_urgent_inline_attempts
    }

    /// Mutating commit admissions rejected by storage pressure.
    pub const fn lifecycle_write_admission_pressure_rejects(self) -> u64 {
        self.lifecycle_write_admission_pressure_rejects
    }

    /// Storage-pressure rejections marked retryable after maintenance.
    pub const fn lifecycle_write_admission_retryable_rejects(self) -> u64 {
        self.lifecycle_write_admission_retryable_rejects
    }

    /// Later admissions accepted after a prior pressure rejection for that branch.
    pub const fn lifecycle_write_admission_pressure_cleared_retries(self) -> u64 {
        self.lifecycle_write_admission_pressure_cleared_retries
    }

    /// Admission attempts that entered a pressure wait policy.
    pub const fn lifecycle_write_admission_wait_attempts(self) -> u64 {
        self.lifecycle_write_admission_wait_attempts
    }

    /// Pressure wait attempts that timed out.
    pub const fn lifecycle_write_admission_wait_timeouts(self) -> u64 {
        self.lifecycle_write_admission_wait_timeouts
    }
    /// Pressure waits whose stall watchdog was reset because background
    /// maintenance completed a task (made progress) during the wait slice.
    pub const fn lifecycle_write_admission_wait_progress_resets(self) -> u64 {
        self.lifecycle_write_admission_wait_progress_resets
    }

    /// Notifications emitted after storage pressure cleared.
    pub const fn lifecycle_pressure_clear_wakes(self) -> u64 {
        self.lifecycle_pressure_clear_wakes
    }

    /// Storage pressure collection passes.
    pub const fn lifecycle_pressure_collection_calls(self) -> u64 {
        self.lifecycle_pressure_collection_calls
    }

    /// Branch states inspected during storage pressure collection.
    pub const fn lifecycle_pressure_collection_branches_inspected(self) -> u64 {
        self.lifecycle_pressure_collection_branches_inspected
    }

    /// Level vectors inspected during storage pressure collection.
    pub const fn lifecycle_pressure_collection_levels_inspected(self) -> u64 {
        self.lifecycle_pressure_collection_levels_inspected
    }

    /// Table descriptors inspected during storage pressure collection.
    pub const fn lifecycle_pressure_collection_tables_inspected(self) -> u64 {
        self.lifecycle_pressure_collection_tables_inspected
    }

    /// Nanoseconds spent collecting storage pressure.
    pub const fn lifecycle_pressure_collection_ns(self) -> u64 {
        self.lifecycle_pressure_collection_ns
    }

    /// Pressure collection scans skipped by sampling.
    pub const fn lifecycle_pressure_collection_sampling_skips(self) -> u64 {
        self.lifecycle_pressure_collection_sampling_skips
    }

    /// Full pressure collection scans executed instead of sampled skips.
    pub const fn lifecycle_pressure_collection_full_scans(self) -> u64 {
        self.lifecycle_pressure_collection_full_scans
    }

    /// Active mutable byte pressure observations at background severity.
    pub const fn lifecycle_active_byte_pressure_background(self) -> u64 {
        self.lifecycle_active_byte_pressure_background
    }

    /// Active mutable byte pressure observations at urgent severity.
    pub const fn lifecycle_active_byte_pressure_urgent(self) -> u64 {
        self.lifecycle_active_byte_pressure_urgent
    }

    /// Active mutable byte pressure observations at blocking severity.
    pub const fn lifecycle_active_byte_pressure_blocking(self) -> u64 {
        self.lifecycle_active_byte_pressure_blocking
    }

    /// Post-commit maintenance pressure evaluations.
    pub const fn lifecycle_post_commit_maintenance_evaluations(self) -> u64 {
        self.lifecycle_post_commit_maintenance_evaluations
    }

    /// Post-commit maintenance evaluations skipped by disabled policy.
    pub const fn lifecycle_post_commit_maintenance_disabled(self) -> u64 {
        self.lifecycle_post_commit_maintenance_disabled
    }

    /// Post-commit maintenance evaluations with no suggested task.
    pub const fn lifecycle_post_commit_maintenance_no_task(self) -> u64 {
        self.lifecycle_post_commit_maintenance_no_task
    }

    /// Post-commit maintenance evaluations that found a suggested task.
    pub const fn lifecycle_post_commit_maintenance_tasks_suggested(self) -> u64 {
        self.lifecycle_post_commit_maintenance_tasks_suggested
    }

    /// Post-commit maintenance tasks admitted to the maintenance queue.
    pub const fn lifecycle_post_commit_maintenance_tasks_enqueued(self) -> u64 {
        self.lifecycle_post_commit_maintenance_tasks_enqueued
    }

    /// Post-commit maintenance tasks coalesced with an existing queued task.
    pub const fn lifecycle_post_commit_maintenance_tasks_coalesced(self) -> u64 {
        self.lifecycle_post_commit_maintenance_tasks_coalesced
    }

    /// Post-commit maintenance tasks deferred by queue/admission failure.
    pub const fn lifecycle_post_commit_maintenance_tasks_deferred(self) -> u64 {
        self.lifecycle_post_commit_maintenance_tasks_deferred
    }

    /// Cross-branch maintenance coverage scans.
    pub const fn lifecycle_maintenance_coverage_scans(self) -> u64 {
        self.lifecycle_maintenance_coverage_scans
    }

    /// Live branches inspected by cross-branch maintenance coverage.
    pub const fn lifecycle_maintenance_coverage_branches_scanned(self) -> u64 {
        self.lifecycle_maintenance_coverage_branches_scanned
    }

    /// Non-committing branches that had maintenance pressure during coverage.
    pub const fn lifecycle_maintenance_coverage_quiet_branches_with_pressure(self) -> u64 {
        self.lifecycle_maintenance_coverage_quiet_branches_with_pressure
    }

    /// Coverage-discovered tasks admitted to the maintenance queue.
    pub const fn lifecycle_maintenance_coverage_tasks_enqueued(self) -> u64 {
        self.lifecycle_maintenance_coverage_tasks_enqueued
    }

    /// Coverage-discovered tasks coalesced with existing queued tasks.
    pub const fn lifecycle_maintenance_coverage_tasks_coalesced(self) -> u64 {
        self.lifecycle_maintenance_coverage_tasks_coalesced
    }

    /// No-work coverage rounds consumed by the in-process idle anchor.
    pub const fn lifecycle_maintenance_coverage_idle_rounds_consumed(self) -> u64 {
        self.lifecycle_maintenance_coverage_idle_rounds_consumed
    }

    /// Coverage passes that found no pressure in the inspected quiet branches.
    pub const fn lifecycle_maintenance_coverage_stop_no_pressure(self) -> u64 {
        self.lifecycle_maintenance_coverage_stop_no_pressure
    }

    /// Coverage passes that reached the bounded idle-round limit.
    pub const fn lifecycle_maintenance_coverage_stop_idle_limit(self) -> u64 {
        self.lifecycle_maintenance_coverage_stop_idle_limit
    }

    /// Coverage passes that stopped because the queue was full.
    pub const fn lifecycle_maintenance_coverage_stop_queue_full(self) -> u64 {
        self.lifecycle_maintenance_coverage_stop_queue_full
    }

    /// Coverage passes that stopped because enqueue or branch inspection failed.
    pub const fn lifecycle_maintenance_coverage_stop_failure(self) -> u64 {
        self.lifecycle_maintenance_coverage_stop_failure
    }

    /// Inline automatic maintenance attempts made during commit handling.
    pub const fn lifecycle_inline_maintenance_attempts(self) -> u64 {
        self.lifecycle_inline_maintenance_attempts
    }

    /// Nanoseconds spent running inline automatic maintenance during commit handling.
    pub const fn lifecycle_inline_maintenance_ns(self) -> u64 {
        self.lifecycle_inline_maintenance_ns
    }

    /// Background maintenance runtime controllers created by public API opens.
    pub const fn lifecycle_background_runtimes_created(self) -> u64 {
        self.lifecycle_background_runtimes_created
    }

    /// Background maintenance worker threads requested by created runtimes.
    pub const fn lifecycle_background_runtime_workers_created(self) -> u64 {
        self.lifecycle_background_runtime_workers_created
    }

    /// Accepted background lifecycle wake submissions.
    pub const fn lifecycle_background_wake_submitted(self) -> u64 {
        self.lifecycle_background_wake_submitted
    }

    /// Background lifecycle wake requests coalesced with an active wake.
    pub const fn lifecycle_background_wake_coalesced(self) -> u64 {
        self.lifecycle_background_wake_coalesced
    }

    /// Background lifecycle wake submissions rejected by shutdown/backpressure.
    pub const fn lifecycle_background_wake_rejected(self) -> u64 {
        self.lifecycle_background_wake_rejected
    }

    /// Background wakes that found no eligible lifecycle task.
    pub const fn lifecycle_background_stale_wake_noop(self) -> u64 {
        self.lifecycle_background_stale_wake_noop
    }

    /// Background lifecycle drain rounds executed by worker wakes.
    pub const fn lifecycle_background_drain_rounds(self) -> u64 {
        self.lifecycle_background_drain_rounds
    }

    /// Lifecycle maintenance tasks completed by background drain rounds.
    pub const fn lifecycle_background_tasks_completed(self) -> u64 {
        self.lifecycle_background_tasks_completed
    }

    /// Background runtime shutdowns requested.
    pub const fn lifecycle_background_shutdowns(self) -> u64 {
        self.lifecycle_background_shutdowns
    }

    /// Background worker threads joined during shutdown.
    pub const fn lifecycle_background_shutdown_joined_workers(self) -> u64 {
        self.lifecycle_background_shutdown_joined_workers
    }

    /// Close-required lifecycle maintenance tasks drained during close.
    pub const fn lifecycle_background_shutdown_drained_tasks(self) -> u64 {
        self.lifecycle_background_shutdown_drained_tasks
    }

    /// Lifecycle maintenance tasks canceled during background-aware close.
    pub const fn lifecycle_background_shutdown_canceled_tasks(self) -> u64 {
        self.lifecycle_background_shutdown_canceled_tasks
    }

    /// Executor closures completed after shutdown was requested.
    pub const fn lifecycle_background_shutdown_executor_tasks_completed(self) -> u64 {
        self.lifecycle_background_shutdown_executor_tasks_completed
    }

    /// Background worker threads detached because shutdown did not quiesce by deadline.
    pub const fn lifecycle_background_shutdown_detached_workers(self) -> u64 {
        self.lifecycle_background_shutdown_detached_workers
    }

    /// Background wake submissions rejected after shutdown started.
    pub const fn lifecycle_background_submit_after_shutdown_rejected(self) -> u64 {
        self.lifecycle_background_submit_after_shutdown_rejected
    }

    /// Background worker panics caught by the scheduler.
    pub const fn lifecycle_background_worker_panics(self) -> u64 {
        self.lifecycle_background_worker_panics
    }

    /// Background task failures observed by the drain loop.
    pub const fn lifecycle_background_task_failures(self) -> u64 {
        self.lifecycle_background_task_failures
    }

    /// Background task failures while starting/snapshotting work.
    pub const fn lifecycle_background_task_start_failures(self) -> u64 {
        self.lifecycle_background_task_start_failures
    }

    /// Background task failures while publishing prepared work.
    pub const fn lifecycle_background_task_publish_failures(self) -> u64 {
        self.lifecycle_background_task_publish_failures
    }

    /// Nanoseconds spent holding the runtime lock while snapshotting a background task.
    pub const fn lifecycle_background_task_snapshot_lock_ns(self) -> u64 {
        self.lifecycle_background_task_snapshot_lock_ns
    }

    /// Nanoseconds spent outside the runtime lock building background task output.
    pub const fn lifecycle_background_task_unlocked_build_ns(self) -> u64 {
        self.lifecycle_background_task_unlocked_build_ns
    }

    /// Nanoseconds spent holding the runtime lock while publishing background task output.
    pub const fn lifecycle_background_task_publish_lock_ns(self) -> u64 {
        self.lifecycle_background_task_publish_lock_ns
    }

    /// Nanoseconds low-tier maintenance (retention/purge/quarantine/repair)
    /// ran INSIDE the drain's start-section lock hold.
    pub const fn lifecycle_background_task_low_tier_ns(self) -> u64 {
        self.lifecycle_background_task_low_tier_ns
    }

    /// Low-tier maintenance executions timed by the counter above.
    pub const fn lifecycle_background_task_low_tier_runs(self) -> u64 {
        self.lifecycle_background_task_low_tier_runs
    }
    /// Nanoseconds spent persisting the table manifest (durable write + fsync)
    /// within the locked publish window; subtract from publish-lock time to
    /// attribute the in-memory pointer/state swap.
    pub const fn lifecycle_background_publish_manifest_persist_ns(self) -> u64 {
        self.lifecycle_background_publish_manifest_persist_ns
    }

    /// Wall-clock nanoseconds the background publish spent OUTSIDE the global runtime lock —
    /// the off-lock phase that performs the manifest fsync. Zero when nothing published off-lock.
    pub const fn lifecycle_background_publish_offlock_ns(self) -> u64 {
        self.lifecycle_background_publish_offlock_ns
    }

    /// Total nanoseconds spent by split background maintenance tasks.
    pub const fn lifecycle_background_task_total_ns(self) -> u64 {
        self.lifecycle_background_task_total_ns
    }

    /// Background candidates deferred because source shape changed before publish.
    pub const fn lifecycle_background_candidate_stale_deferred(self) -> u64 {
        self.lifecycle_background_candidate_stale_deferred
    }

    /// Nanoseconds foreground commits spent waiting to acquire the runtime lock.
    pub const fn lifecycle_foreground_wait_background_lock_ns(self) -> u64 {
        self.lifecycle_foreground_wait_background_lock_ns
    }

    /// Nanoseconds spent waiting for background progress under Block pressure.
    pub const fn lifecycle_write_admission_block_wait_ns(self) -> u64 {
        self.lifecycle_write_admission_block_wait_ns
    }

    /// W2.3 (B3): indexed (binary-searched) point seeks.
    pub const fn table_indexed_block_seeks(self) -> u64 {
        self.table_indexed_block_seeks
    }

    /// W2.3 (B3): accelerator derivations.
    pub const fn table_accelerator_builds(self) -> u64 {
        self.table_accelerator_builds
    }

    /// W2.3 (B3): accelerator self-heal rebuilds.
    pub const fn table_accelerator_rebuilds(self) -> u64 {
        self.table_accelerator_rebuilds
    }

    /// W2.6 (B1): trusted (no re-CRC) data-block reads.
    pub const fn table_trusted_block_seeks(self) -> u64 {
        self.table_trusted_block_seeks
    }

    /// W2.6 (B1): checked (re-verifying) data-block reads.
    pub const fn table_checked_block_seeks(self) -> u64 {
        self.table_checked_block_seeks
    }

    /// W2.6 (B1): warm inserts rejected by pre-insert verification.
    pub const fn table_warm_insert_rejects(self) -> u64 {
        self.table_warm_insert_rejects
    }

    /// W3.3a: WAL appends staged in the coalescing buffer.
    pub const fn commit_wal_buffered_appends(self) -> u64 {
        self.commit_wal_buffered_appends
    }

    /// W3.3a: buffer drains triggered by the size threshold.
    pub const fn commit_wal_buffer_flushes_threshold(self) -> u64 {
        self.commit_wal_buffer_flushes_threshold
    }

    /// W3.3a: buffer drains triggered by group-sync ticket captures.
    pub const fn commit_wal_buffer_flushes_capture(self) -> u64 {
        self.commit_wal_buffer_flushes_capture
    }

    /// W3.3a: buffer drains triggered by durability barriers.
    pub const fn commit_wal_buffer_flushes_durability(self) -> u64 {
        self.commit_wal_buffer_flushes_durability
    }

    /// W3.3b: buffer drains triggered by the staleness window.
    pub const fn commit_wal_buffer_flushes_trickle(self) -> u64 {
        self.commit_wal_buffer_flushes_trickle
    }

    /// W3.3a: total bytes drained from the coalescing buffer.
    pub const fn commit_wal_buffer_flush_bytes(self) -> u64 {
        self.commit_wal_buffer_flush_bytes
    }

    /// W3.2: grouped durable dispatch envelope, nanoseconds.
    pub const fn commit_group_dispatch_ns(self) -> u64 {
        self.commit_group_dispatch_ns
    }

    /// W3.2: background-drain notification cost from the commit path.
    pub const fn commit_drain_notify_ns(self) -> u64 {
        self.commit_drain_notify_ns
    }

    /// W1.4a: pacing sleeps actually taken by commits (count).
    pub const fn lifecycle_graded_throttle_delays(self) -> u64 {
        self.lifecycle_graded_throttle_delays
    }

    /// W1.4a: total wall-clock the token bucket slept, in milliseconds.
    pub const fn lifecycle_graded_throttle_delay_ms_total(self) -> u64 {
        self.lifecycle_graded_throttle_delay_ms_total
    }

    /// W1.4a: the largest single pacing sleep, in milliseconds.
    pub const fn lifecycle_graded_throttle_delay_ms_max(self) -> u64 {
        self.lifecycle_graded_throttle_delay_ms_max
    }

    /// W1.4a: debt-adaptive rate recomputations (install-event cadence).
    pub const fn lifecycle_admission_rate_recomputes(self) -> u64 {
        self.lifecycle_admission_rate_recomputes
    }

    /// W1.4a: recomputes that landed the rate at the configured floor.
    pub const fn lifecycle_admission_rate_floor_events(self) -> u64 {
        self.lifecycle_admission_rate_floor_events
    }

    /// W1.4a: the most recently computed write rate, bytes/sec.
    pub const fn lifecycle_admission_rate_last(self) -> u64 {
        self.lifecycle_admission_rate_last
    }

    /// W1.4a: the largest pacing debt observed at a recompute, in bytes.
    pub const fn lifecycle_admission_effective_debt_max(self) -> u64 {
        self.lifecycle_admission_effective_debt_max
    }

    /// WAL-growth policy samples taken after durable commits.
    pub const fn lifecycle_wal_retention_samples(self) -> u64 {
        self.lifecycle_wal_retention_samples
    }

    /// Retained WAL bytes observed by the last WAL-growth sample.
    pub const fn lifecycle_wal_retained_bytes_last(self) -> u64 {
        self.lifecycle_wal_retained_bytes_last
    }

    /// Maximum retained WAL bytes observed across WAL-growth samples.
    pub const fn lifecycle_wal_retained_bytes_max(self) -> u64 {
        self.lifecycle_wal_retained_bytes_max
    }

    /// Retained WAL segments observed by the last WAL-growth sample.
    pub const fn lifecycle_wal_retained_segments_last(self) -> u64 {
        self.lifecycle_wal_retained_segments_last
    }

    /// Maximum retained WAL segments observed across WAL-growth samples.
    pub const fn lifecycle_wal_retained_segments_max(self) -> u64 {
        self.lifecycle_wal_retained_segments_max
    }

    /// WAL-growth evaluations that enqueued a checkpoint task.
    pub const fn lifecycle_wal_checkpoint_enqueue_events(self) -> u64 {
        self.lifecycle_wal_checkpoint_enqueue_events
    }

    /// WAL-growth evaluations that coalesced with an existing checkpoint task.
    pub const fn lifecycle_wal_checkpoint_coalesced_events(self) -> u64 {
        self.lifecycle_wal_checkpoint_coalesced_events
    }

    /// Successful checkpoint publications executed by lifecycle maintenance.
    pub const fn lifecycle_checkpoint_executions(self) -> u64 {
        self.lifecycle_checkpoint_executions
    }

    /// Covered WAL segments deleted by WAL truncation.
    pub const fn lifecycle_wal_truncation_deleted_segments(self) -> u64 {
        self.lifecycle_wal_truncation_deleted_segments
    }

    /// WAL segments protected by WAL truncation proof bounds.
    pub const fn lifecycle_wal_truncation_protected_segments(self) -> u64 {
        self.lifecycle_wal_truncation_protected_segments
    }

    /// WAL segment delete attempts that failed during WAL truncation.
    pub const fn lifecycle_wal_truncation_failed_segments(self) -> u64 {
        self.lifecycle_wal_truncation_failed_segments
    }

    /// Frozen tables observed at flush-drain start.
    pub const fn lifecycle_flush_drain_frozen_tables_discovered(self) -> u64 {
        self.lifecycle_flush_drain_frozen_tables_discovered
    }

    /// Concrete flush operations completed by branch flush drains.
    pub const fn lifecycle_flush_drain_operations_completed(self) -> u64 {
        self.lifecycle_flush_drain_operations_completed
    }

    /// Extra flush operations used for frozen state that appeared during a drain.
    pub const fn lifecycle_flush_drain_freeze_retries(self) -> u64 {
        self.lifecycle_flush_drain_freeze_retries
    }

    /// Failed concrete flush operations inside branch flush drains.
    pub const fn lifecycle_flush_drain_failures(self) -> u64 {
        self.lifecycle_flush_drain_failures
    }

    /// Frozen tables left after branch flush drains complete or defer.
    pub const fn lifecycle_flush_drain_post_drain_frozen_tables(self) -> u64 {
        self.lifecycle_flush_drain_post_drain_frozen_tables
    }

    /// Flush memory-retention measurements recorded after branch drains.
    pub const fn lifecycle_flush_memory_measurements(self) -> u64 {
        self.lifecycle_flush_memory_measurements
    }

    /// Active mutable bytes observed before flush drains.
    pub const fn lifecycle_flush_active_bytes_before(self) -> u64 {
        self.lifecycle_flush_active_bytes_before
    }

    /// Frozen mutable bytes observed before flush drains.
    pub const fn lifecycle_flush_frozen_bytes_before(self) -> u64 {
        self.lifecycle_flush_frozen_bytes_before
    }

    /// Active mutable bytes observed after flush drains.
    pub const fn lifecycle_flush_active_bytes_after(self) -> u64 {
        self.lifecycle_flush_active_bytes_after
    }

    /// Frozen mutable bytes observed after flush drains.
    pub const fn lifecycle_flush_frozen_bytes_after(self) -> u64 {
        self.lifecycle_flush_frozen_bytes_after
    }

    /// Memory-release decisions deferred by lifecycle maintenance.
    pub const fn lifecycle_memory_release_deferrals(self) -> u64 {
        self.lifecycle_memory_release_deferrals
    }

    /// Retained bytes observed when memory release remains measure-first.
    pub const fn lifecycle_memory_release_retained_bytes(self) -> u64 {
        self.lifecycle_memory_release_retained_bytes
    }

    /// Retained-byte thresholds attached to memory-release deferral decisions.
    pub const fn lifecycle_memory_release_threshold_bytes(self) -> u64 {
        self.lifecycle_memory_release_threshold_bytes
    }

    /// Portable memory-release attempts made by lifecycle maintenance.
    pub const fn lifecycle_memory_release_attempts(self) -> u64 {
        self.lifecycle_memory_release_attempts
    }

    /// Snapshot floor advancement operations accepted by lifecycle.
    pub const fn lifecycle_snapshot_floor_advancements(self) -> u64 {
        self.lifecycle_snapshot_floor_advancements
    }

    /// Implicit snapshot floor advancement attempts rejected by lifecycle.
    pub const fn lifecycle_snapshot_floor_implicit_rejections(self) -> u64 {
        self.lifecycle_snapshot_floor_implicit_rejections
    }

    /// Snapshot pruning operations executed with a complete retention proof.
    pub const fn lifecycle_snapshot_pruning_with_proof(self) -> u64 {
        self.lifecycle_snapshot_pruning_with_proof
    }

    /// Snapshot objects deleted by proof-backed snapshot pruning.
    pub const fn lifecycle_snapshot_pruning_deleted(self) -> u64 {
        self.lifecycle_snapshot_pruning_deleted
    }

    /// Snapshot objects protected by proof-backed snapshot pruning.
    pub const fn lifecycle_snapshot_pruning_protected(self) -> u64 {
        self.lifecycle_snapshot_pruning_protected
    }

    /// Snapshot object deletions that failed after proof-backed pruning began.
    pub const fn lifecycle_snapshot_pruning_failed(self) -> u64 {
        self.lifecycle_snapshot_pruning_failed
    }

    /// Compaction score candidates evaluated by lifecycle maintenance.
    pub const fn lifecycle_compaction_score_candidates(self) -> u64 {
        self.lifecycle_compaction_score_candidates
    }

    /// Materialization score candidates evaluated by lifecycle maintenance.
    pub const fn lifecycle_materialization_score_candidates(self) -> u64 {
        self.lifecycle_materialization_score_candidates
    }

    /// Sum of materialization candidate inherited layer indexes.
    pub const fn lifecycle_materialization_score_layer_index_sum(self) -> u64 {
        self.lifecycle_materialization_score_layer_index_sum
    }

    /// Tables considered by materialization score candidates.
    pub const fn lifecycle_materialization_score_table_count(self) -> u64 {
        self.lifecycle_materialization_score_table_count
    }

    /// Bytes considered by materialization score candidates.
    pub const fn lifecycle_materialization_score_byte_count(self) -> u64 {
        self.lifecycle_materialization_score_byte_count
    }

    /// Compaction candidates selected by lifecycle maintenance.
    pub const fn lifecycle_compaction_selected(self) -> u64 {
        self.lifecycle_compaction_selected
    }

    /// Sum of selected compaction output levels.
    pub const fn lifecycle_compaction_selected_level_sum(self) -> u64 {
        self.lifecycle_compaction_selected_level_sum
    }

    /// Sum of selected compaction pressure scores.
    pub const fn lifecycle_compaction_selected_score_sum(self) -> u64 {
        self.lifecycle_compaction_selected_score_sum
    }

    /// Tables in selected compaction source levels.
    pub const fn lifecycle_compaction_selected_table_count(self) -> u64 {
        self.lifecycle_compaction_selected_table_count
    }

    /// Bytes in selected compaction source levels.
    pub const fn lifecycle_compaction_selected_byte_count(self) -> u64 {
        self.lifecycle_compaction_selected_byte_count
    }

    /// Target bytes associated with selected compaction source levels.
    pub const fn lifecycle_compaction_selected_target_bytes(self) -> u64 {
        self.lifecycle_compaction_selected_target_bytes
    }

    /// Nonzero level target evaluations made during compaction scoring.
    pub const fn lifecycle_compaction_level_target_evaluations(self) -> u64 {
        self.lifecycle_compaction_level_target_evaluations
    }

    /// Sum of levels whose target bytes were evaluated.
    pub const fn lifecycle_compaction_level_target_level_sum(self) -> u64 {
        self.lifecycle_compaction_level_target_level_sum
    }

    /// Sum of evaluated nonzero level target bytes.
    pub const fn lifecycle_compaction_level_target_bytes(self) -> u64 {
        self.lifecycle_compaction_level_target_bytes
    }

    /// Nonzero compaction input table selections.
    pub const fn lifecycle_compaction_nonzero_input_selections(self) -> u64 {
        self.lifecycle_compaction_nonzero_input_selections
    }

    /// Sum of levels selected by nonzero input policy.
    pub const fn lifecycle_compaction_nonzero_input_level_sum(self) -> u64 {
        self.lifecycle_compaction_nonzero_input_level_sum
    }

    /// Sum of selected nonzero input table indexes.
    pub const fn lifecycle_compaction_nonzero_input_table_index_sum(self) -> u64 {
        self.lifecycle_compaction_nonzero_input_table_index_sum
    }

    /// Bytes in nonzero input tables selected by lifecycle policy.
    pub const fn lifecycle_compaction_nonzero_input_bytes(self) -> u64 {
        self.lifecycle_compaction_nonzero_input_bytes
    }

    /// Rows in nonzero input tables selected by lifecycle policy.
    pub const fn lifecycle_compaction_nonzero_input_rows(self) -> u64 {
        self.lifecycle_compaction_nonzero_input_rows
    }

    /// Nonzero inputs selected by deterministic largest-input policy.
    pub const fn lifecycle_compaction_largest_input_selections(self) -> u64 {
        self.lifecycle_compaction_largest_input_selections
    }

    /// Deeper-overlap checks made for selected nonzero compaction inputs.
    pub const fn lifecycle_compaction_deeper_overlap_evaluations(self) -> u64 {
        self.lifecycle_compaction_deeper_overlap_evaluations
    }

    /// Bytes of deeper-level overlap seen for selected nonzero compaction inputs.
    pub const fn lifecycle_compaction_deeper_overlap_bytes(self) -> u64 {
        self.lifecycle_compaction_deeper_overlap_bytes
    }

    /// Output split budgets applied by lifecycle compaction policy.
    pub const fn lifecycle_compaction_output_split_budget_applied(self) -> u64 {
        self.lifecycle_compaction_output_split_budget_applied
    }

    /// Output split budgets deferred to lower table/branch layers.
    pub const fn lifecycle_compaction_output_split_budget_deferred(self) -> u64 {
        self.lifecycle_compaction_output_split_budget_deferred
    }

    /// Deeper-overlap bytes attached to deferred output split budget decisions.
    pub const fn lifecycle_compaction_output_split_budget_deferred_bytes(self) -> u64 {
        self.lifecycle_compaction_output_split_budget_deferred_bytes
    }

    /// Completed compaction operations recorded by lifecycle maintenance.
    pub const fn lifecycle_compaction_operations_completed(self) -> u64 {
        self.lifecycle_compaction_operations_completed
    }

    /// Completed lifecycle compactions that merged level-zero tables in place.
    pub const fn lifecycle_compaction_l0_operations(self) -> u64 {
        self.lifecycle_compaction_l0_operations
    }

    /// Completed lifecycle compactions that promoted level-zero tables to level one.
    pub const fn lifecycle_compaction_l0_to_level_one_operations(self) -> u64 {
        self.lifecycle_compaction_l0_to_level_one_operations
    }

    /// Completed lifecycle compactions that selected one nonzero-level input table.
    pub const fn lifecycle_compaction_nonzero_operations(self) -> u64 {
        self.lifecycle_compaction_nonzero_operations
    }

    /// Completed lifecycle compactions that compacted a bottommost nonzero-level run.
    pub const fn lifecycle_compaction_bottommost_operations(self) -> u64 {
        self.lifecycle_compaction_bottommost_operations
    }

    /// Input tables selected by completed lifecycle compactions.
    pub const fn lifecycle_compaction_input_tables(self) -> u64 {
        self.lifecycle_compaction_input_tables
    }

    /// Overlap tables selected by completed lifecycle compactions.
    pub const fn lifecycle_compaction_overlap_tables(self) -> u64 {
        self.lifecycle_compaction_overlap_tables
    }

    /// Output tables installed by completed lifecycle compactions.
    pub const fn lifecycle_compaction_output_tables(self) -> u64 {
        self.lifecycle_compaction_output_tables
    }

    /// Estimated input bytes read by completed lifecycle compactions.
    pub const fn lifecycle_compaction_input_bytes(self) -> u64 {
        self.lifecycle_compaction_input_bytes
    }

    /// Output bytes produced by completed lifecycle compactions.
    pub const fn lifecycle_compaction_output_bytes(self) -> u64 {
        self.lifecycle_compaction_output_bytes
    }

    /// Input bytes not rewritten because lifecycle selected metadata promotion.
    pub const fn lifecycle_compaction_metadata_bytes_avoided(self) -> u64 {
        self.lifecycle_compaction_metadata_bytes_avoided
    }

    /// Elapsed nanoseconds spent in completed lifecycle compactions.
    pub const fn lifecycle_compaction_elapsed_ns(self) -> u64 {
        self.lifecycle_compaction_elapsed_ns
    }

    /// Committed input rows processed by completed lifecycle compactions.
    pub const fn lifecycle_compaction_input_rows(self) -> u64 {
        self.lifecycle_compaction_input_rows
    }

    /// Weighted completed compaction bytes per committed input row.
    pub const fn lifecycle_compaction_rewrite_bytes_per_row(self) -> u64 {
        self.lifecycle_compaction_rewrite_bytes_per_row
    }

    /// Compaction IO bytes consumed under lifecycle budget accounting.
    pub const fn lifecycle_compaction_io_budget_consumed_bytes(self) -> u64 {
        self.lifecycle_compaction_io_budget_consumed_bytes
    }

    /// Compaction tasks deferred by lifecycle IO byte budget.
    pub const fn lifecycle_compaction_io_budget_deferrals(self) -> u64 {
        self.lifecycle_compaction_io_budget_deferrals
    }

    /// Estimated compaction IO bytes attached to budget deferrals.
    pub const fn lifecycle_compaction_io_budget_deferred_bytes(self) -> u64 {
        self.lifecycle_compaction_io_budget_deferred_bytes
    }

    /// Compaction IO byte limits attached to budget decisions.
    pub const fn lifecycle_compaction_io_budget_limit_bytes(self) -> u64 {
        self.lifecycle_compaction_io_budget_limit_bytes
    }

    /// Compactions preempted because flush pressure was present.
    pub const fn lifecycle_compaction_flush_preemptions(self) -> u64 {
        self.lifecycle_compaction_flush_preemptions
    }

    /// Metadata-only table promotions completed by lifecycle compactions.
    pub const fn lifecycle_compaction_trivial_moves(self) -> u64 {
        self.lifecycle_compaction_trivial_moves
    }

    /// Compaction chain tasks resubmitted after a completed operation.
    pub const fn lifecycle_compaction_resubmits(self) -> u64 {
        self.lifecycle_compaction_resubmits
    }

    /// Compaction chain resubmissions coalesced with an existing task.
    pub const fn lifecycle_compaction_resubmit_coalesces(self) -> u64 {
        self.lifecycle_compaction_resubmit_coalesces
    }

    /// Compaction chain resubmissions deferred by queue/admission failure.
    pub const fn lifecycle_compaction_resubmit_deferred(self) -> u64 {
        self.lifecycle_compaction_resubmit_deferred
    }

    /// Table rewrite pressure samples recorded after completed operations.
    pub const fn lifecycle_table_rewrite_post_operation_scores(self) -> u64 {
        self.lifecycle_table_rewrite_post_operation_scores
    }

    /// Post-operation samples that still had table rewrite pressure.
    pub const fn lifecycle_table_rewrite_post_operation_remaining(self) -> u64 {
        self.lifecycle_table_rewrite_post_operation_remaining
    }

    /// Sum of post-operation table rewrite pressure scores.
    pub const fn lifecycle_table_rewrite_post_operation_score_sum(self) -> u64 {
        self.lifecycle_table_rewrite_post_operation_score_sum
    }

    /// Items in post-operation table rewrite pressure samples.
    pub const fn lifecycle_table_rewrite_post_operation_item_count(self) -> u64 {
        self.lifecycle_table_rewrite_post_operation_item_count
    }

    /// Bytes in post-operation table rewrite pressure samples.
    pub const fn lifecycle_table_rewrite_post_operation_byte_count(self) -> u64 {
        self.lifecycle_table_rewrite_post_operation_byte_count
    }

    /// Branch registry lookup calls made by commit admission.
    pub const fn commit_branch_registry_lookups(self) -> u64 {
        self.commit_branch_registry_lookups
    }

    /// Branch descriptors scanned by branch registry lookups.
    pub const fn commit_branch_registry_descriptors_scanned(self) -> u64 {
        self.commit_branch_registry_descriptors_scanned
    }

    /// Branch guard acquisition attempts.
    pub const fn commit_branch_guard_attempts(self) -> u64 {
        self.commit_branch_guard_attempts
    }

    /// Branch guards acquired.
    pub const fn commit_branch_guard_acquired(self) -> u64 {
        self.commit_branch_guard_acquired
    }

    /// Branch guard attempts rejected by active branch or quiesce state.
    pub const fn commit_branch_guard_rejected(self) -> u64 {
        self.commit_branch_guard_rejected
    }

    /// Quiesce guard acquisition attempts.
    pub const fn commit_quiesce_attempts(self) -> u64 {
        self.commit_quiesce_attempts
    }

    /// Quiesce guards acquired.
    pub const fn commit_quiesce_acquired(self) -> u64 {
        self.commit_quiesce_acquired
    }

    /// Quiesce guard attempts rejected by active commit or quiesce state.
    pub const fn commit_quiesce_rejected(self) -> u64 {
        self.commit_quiesce_rejected
    }

    /// Conflict-validation calls.
    pub const fn commit_conflict_validation_calls(self) -> u64 {
        self.commit_conflict_validation_calls
    }

    /// Conflict-validation calls skipped by mode or diagnostic kind.
    pub const fn commit_conflict_validation_skipped(self) -> u64 {
        self.commit_conflict_validation_skipped
    }

    /// Conflict-validation calls that did not need a read source.
    pub const fn commit_conflict_validation_without_source(self) -> u64 {
        self.commit_conflict_validation_without_source
    }

    /// Conflict-validation calls that needed a read source.
    pub const fn commit_conflict_validation_with_source(self) -> u64 {
        self.commit_conflict_validation_with_source
    }

    /// Read-set facts checked by conflict validation.
    pub const fn commit_conflict_read_facts_checked(self) -> u64 {
        self.commit_conflict_read_facts_checked
    }

    /// CAS facts checked by conflict validation.
    pub const fn commit_conflict_cas_facts_checked(self) -> u64 {
        self.commit_conflict_cas_facts_checked
    }

    /// Commit conflicts detected by validation.
    pub const fn commit_conflicts_detected(self) -> u64 {
        self.commit_conflicts_detected
    }

    /// Rows scanned while constructing commit timeline views.
    pub const fn commit_timeline_view_rows_scanned(self) -> u64 {
        self.commit_timeline_view_rows_scanned
    }

    /// Timestamp-to-version facts observed by timeline view construction.
    pub const fn commit_timeline_timestamp_facts(self) -> u64 {
        self.commit_timeline_timestamp_facts
    }

    /// Version-to-timestamp facts observed by timeline view construction.
    pub const fn commit_timeline_version_facts(self) -> u64 {
        self.commit_timeline_version_facts
    }

    /// Timeline reconciliation calls.
    pub const fn commit_timeline_reconcile_calls(self) -> u64 {
        self.commit_timeline_reconcile_calls
    }

    /// Timestamp facts handed to timeline reconciliation.
    pub const fn commit_timeline_reconcile_timestamp_facts(self) -> u64 {
        self.commit_timeline_reconcile_timestamp_facts
    }

    /// Version facts handed to timeline reconciliation.
    pub const fn commit_timeline_reconcile_version_facts(self) -> u64 {
        self.commit_timeline_reconcile_version_facts
    }

    /// Timeline entries checked while reconciling timestamp and version facts.
    pub const fn commit_timeline_reconcile_entry_checks(self) -> u64 {
        self.commit_timeline_reconcile_entry_checks
    }

    /// Timeline timestamp lookup calls.
    pub const fn commit_timeline_lookup_calls(self) -> u64 {
        self.commit_timeline_lookup_calls
    }

    /// Timeline entries scanned by timestamp lookups.
    pub const fn commit_timeline_lookup_entries_scanned(self) -> u64 {
        self.commit_timeline_lookup_entries_scanned
    }

    /// Replay duplicate-classification passes.
    pub const fn commit_replay_classification_calls(self) -> u64 {
        self.commit_replay_classification_calls
    }

    /// WAL payload rows classified during replay duplicate detection.
    pub const fn commit_replay_rows_classified(self) -> u64 {
        self.commit_replay_rows_classified
    }

    /// Branch history probes issued by replay duplicate detection.
    pub const fn commit_replay_history_calls(self) -> u64 {
        self.commit_replay_history_calls
    }

    /// Branch sources probed during replay duplicate detection.
    pub const fn commit_replay_source_probes(self) -> u64 {
        self.commit_replay_source_probes
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

    /// Number of per-read active-memtable pins (`clone_for_read_view`) taken on the read path.
    pub const fn read_pins(self) -> u64 {
        self.read_pins
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

    /// Table facts computed from streaming build metadata rather than table decode.
    pub const fn table_build_facts_from_streaming_metadata(self) -> u64 {
        self.table_build_facts_from_streaming_metadata
    }

    /// Redundant rewrite-output fact decodes avoided by build-time facts.
    pub const fn table_rewrite_redundant_fact_decodes_avoided(self) -> u64 {
        self.table_rewrite_redundant_fact_decodes_avoided
    }

    /// Durable rewrite/compaction/materialization output reader lazy reopens performed —
    /// metadata-only, disk-resident install over the just-published object (BS4.4l).
    pub const fn table_rewrite_reader_reopens_performed(self) -> u64 {
        self.table_rewrite_reader_reopens_performed
    }

    /// Boundary-key buffers that needed allocation or capacity growth.
    pub const fn table_compaction_boundary_key_buffer_allocations(self) -> u64 {
        self.table_compaction_boundary_key_buffer_allocations
    }

    /// Boundary-key updates that reused existing buffer capacity.
    pub const fn table_compaction_boundary_key_buffer_reuses(self) -> u64 {
        self.table_compaction_boundary_key_buffer_reuses
    }

    /// Previous-kept-key buffers that needed allocation.
    pub const fn table_compaction_previous_key_buffer_allocations(self) -> u64 {
        self.table_compaction_previous_key_buffer_allocations
    }

    /// Previous-kept-key updates that reused existing buffer capacity.
    pub const fn table_compaction_previous_key_buffer_reuses(self) -> u64 {
        self.table_compaction_previous_key_buffer_reuses
    }

    /// Nanoseconds spent in the table-compaction merge loop.
    pub const fn table_compaction_merge_ns(self) -> u64 {
        self.table_compaction_merge_ns
    }

    /// Input rows observed by the timed table-compaction merge loop.
    pub const fn table_compaction_merge_input_rows(self) -> u64 {
        self.table_compaction_merge_input_rows
    }

    /// Average table-compaction merge nanoseconds per input row.
    pub const fn table_compaction_merge_ns_per_input_row(self) -> u64 {
        self.table_compaction_merge_ns_per_input_row
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

    /// Candidate rows cloned while collecting point-read candidates.
    pub const fn point_candidate_row_clones(self) -> u64 {
        self.point_candidate_row_clones
    }

    /// Approximate bytes cloned while collecting point-read candidates.
    pub const fn point_candidate_row_clone_bytes(self) -> u64 {
        self.point_candidate_row_clone_bytes
    }

    /// Point reads whose selected candidate came from the active table.
    pub const fn point_selected_active(self) -> u64 {
        self.point_selected_active
    }

    /// Point reads whose selected candidate came from a frozen table.
    pub const fn point_selected_frozen(self) -> u64 {
        self.point_selected_frozen
    }

    /// Point reads whose selected candidate came from an owned L0 table.
    pub const fn point_selected_owned_l0(self) -> u64 {
        self.point_selected_owned_l0
    }

    /// Point reads whose selected candidate came from an owned nonzero-level table.
    pub const fn point_selected_owned_nonzero(self) -> u64 {
        self.point_selected_owned_nonzero
    }

    /// Point reads whose selected candidate came from an inherited table.
    pub const fn point_selected_inherited(self) -> u64 {
        self.point_selected_inherited
    }

    /// Point reads that exited after active-table probing.
    pub const fn point_early_exit_active(self) -> u64 {
        self.point_early_exit_active
    }

    /// Point reads that exited after frozen-table probing.
    pub const fn point_early_exit_frozen(self) -> u64 {
        self.point_early_exit_frozen
    }

    /// Point reads that exited after owned L0 probing.
    pub const fn point_early_exit_owned_l0(self) -> u64 {
        self.point_early_exit_owned_l0
    }

    /// Point reads that exited after owned nonzero-level probing.
    pub const fn point_early_exit_owned_nonzero(self) -> u64 {
        self.point_early_exit_owned_nonzero
    }

    /// Point reads that exited after inherited-source probing.
    pub const fn point_early_exit_inherited(self) -> u64 {
        self.point_early_exit_inherited
    }

    /// Remaining point-read sources skipped because an early exit was proven.
    pub const fn point_remaining_source_skips(self) -> u64 {
        self.point_remaining_source_skips
    }

    /// Inherited physical-key rewrites performed by point reads.
    pub const fn point_inherited_key_rewrites(self) -> u64 {
        self.point_inherited_key_rewrites
    }

    /// Point lookup key encodings built by table seek helpers.
    pub const fn table_point_lookup_key_builds(self) -> u64 {
        self.table_point_lookup_key_builds
    }

    /// Point lookup key encodings reused by table seek helpers.
    pub const fn table_point_lookup_key_reuses(self) -> u64 {
        self.table_point_lookup_key_reuses
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

    /// BS4.4d: number of lazy (disk-backed) readers that materialized all their rows. Asserted zero on
    /// the durable read paths — a nonzero count means a consumer forced a full-table read.
    pub const fn table_lazy_full_materializations(self) -> u64 {
        self.table_lazy_full_materializations
    }

    /// BS4.5a: durable mutating commits admitted while measured residency exceeded the memory budget.
    /// After the disk-resident flip a durable dataset is no longer RAM-bounded, so this is an
    /// observability gauge (health WARN signal), not an admission failure — cache mode still hard-rejects.
    pub const fn durable_commit_admitted_over_budget(self) -> u64 {
        self.durable_commit_admitted_over_budget
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

    /// Number of lazy data blocks scanned by table-local point lookup.
    pub const fn table_lazy_point_block_scans(self) -> u64 {
        self.table_lazy_point_block_scans
    }

    /// Number of encoded data-block entries scanned by lazy point lookup.
    pub const fn table_lazy_point_entries_scanned(self) -> u64 {
        self.table_lazy_point_entries_scanned
    }

    /// Number of row payloads decoded by lazy point lookup.
    pub const fn table_lazy_point_rows_decoded(self) -> u64 {
        self.table_lazy_point_rows_decoded
    }

    /// Number of full data-block row decodes avoided by lazy point lookup.
    pub const fn table_lazy_point_full_block_decodes_avoided(self) -> u64 {
        self.table_lazy_point_full_block_decodes_avoided
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

    /// Eager-table point-read filter probes.
    pub const fn table_eager_filter_probes(self) -> u64 {
        self.table_eager_filter_probes
    }

    /// Eager-table point-read filter probes that rejected a key.
    pub const fn table_eager_filter_negative_probes(self) -> u64 {
        self.table_eager_filter_negative_probes
    }

    /// Eager-table point-read filter probes that may contain a key.
    pub const fn table_eager_filter_positive_probes(self) -> u64 {
        self.table_eager_filter_positive_probes
    }

    /// Eager-table point-read filter probes where no filter was available.
    pub const fn table_eager_filter_unavailable_probes(self) -> u64 {
        self.table_eager_filter_unavailable_probes
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
static COMMIT_WAL_RECORD_BUILD_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_WAL_RECORDS_BUILT: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_WAL_RECORD_ROWS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_WAL_RECORD_BYTES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_WAL_PAYLOAD_BYTES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_WAL_ROW_ENCODE_BYTES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_WAL_ENCODE_BUFFER_ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_WAL_ENCODE_BUFFER_REUSES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_WAL_APPEND_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_WAL_APPENDS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_WAL_APPEND_BYTES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_POST_WAL_GROWTH_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_WAL_GROWTH_FACTS_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_WAL_GROWTH_MANIFEST_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_POST_MAINTENANCE_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_EXEC_ADMISSION_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_EXEC_CONFLICT_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_EXEC_STAGE_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_EXEC_APPLY_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_EXEC_PUBLISH_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_ADMIT_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_SETUP_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_API_BATCH_CLONE_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_API_POST_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_VISIBLE_PUBLISH_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_VISIBLE_PUBLISH_SUCCESSES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_VISIBLE_PUBLISH_FAILURES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_ADMISSION_PRESSURE_FACTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_ADMISSION_UNDER_PRESSURE: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_ADMISSION_ACCEPTED_UNDER_PRESSURE: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_ADMISSION_REQUIRES_MAINTENANCE: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_ADMISSION_MUTATIONS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_ADMISSION_APPROX_BYTES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_UNRESOLVED_GATE_ADMISSION_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_UNRESOLVED_GATE_ADMISSION_ACQUIRED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_UNRESOLVED_GATE_REJECTED_UNRESOLVED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
#[cfg(feature = "perf-trace")]
static COMMIT_UNRESOLVED_RECORDS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_UNRESOLVED_DURABLE_NOT_APPLIED_RECORDS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_UNRESOLVED_APPLIED_NOT_VISIBLE_RECORDS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_WRITE_ADMISSION_EVALUATIONS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_WRITE_ADMISSION_CLEAN_ACCEPTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_WRITE_ADMISSION_UNDER_PRESSURE_ACCEPTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_WRITE_ADMISSION_URGENT_ACCEPTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_WRITE_ADMISSION_REQUIRES_MAINTENANCE: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_WRITE_ADMISSION_INLINE_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_WRITE_ADMISSION_URGENT_INLINE_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_WRITE_ADMISSION_PRESSURE_REJECTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_WRITE_ADMISSION_RETRYABLE_REJECTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_WRITE_ADMISSION_PRESSURE_CLEARED_RETRIES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_WRITE_ADMISSION_WAIT_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_WRITE_ADMISSION_WAIT_TIMEOUTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_WRITE_ADMISSION_WAIT_PROGRESS_RESETS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_PRESSURE_CLEAR_WAKES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_PRESSURE_COLLECTION_CALLS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_PRESSURE_COLLECTION_BRANCHES_INSPECTED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_PRESSURE_COLLECTION_LEVELS_INSPECTED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_PRESSURE_COLLECTION_TABLES_INSPECTED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_PRESSURE_COLLECTION_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_PRESSURE_COLLECTION_SAMPLING_SKIPS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_PRESSURE_COLLECTION_FULL_SCANS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_ACTIVE_BYTE_PRESSURE_BACKGROUND: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_ACTIVE_BYTE_PRESSURE_URGENT: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_ACTIVE_BYTE_PRESSURE_BLOCKING: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_POST_COMMIT_MAINTENANCE_EVALUATIONS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_POST_COMMIT_MAINTENANCE_DISABLED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_POST_COMMIT_MAINTENANCE_NO_TASK: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_POST_COMMIT_MAINTENANCE_TASKS_SUGGESTED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_POST_COMMIT_MAINTENANCE_TASKS_ENQUEUED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_POST_COMMIT_MAINTENANCE_TASKS_COALESCED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_POST_COMMIT_MAINTENANCE_TASKS_DEFERRED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_MAINTENANCE_COVERAGE_SCANS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_MAINTENANCE_COVERAGE_BRANCHES_SCANNED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_MAINTENANCE_COVERAGE_QUIET_BRANCHES_WITH_PRESSURE: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_MAINTENANCE_COVERAGE_TASKS_ENQUEUED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_MAINTENANCE_COVERAGE_TASKS_COALESCED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_MAINTENANCE_COVERAGE_IDLE_ROUNDS_CONSUMED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_MAINTENANCE_COVERAGE_STOP_NO_PRESSURE: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_MAINTENANCE_COVERAGE_STOP_IDLE_LIMIT: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_MAINTENANCE_COVERAGE_STOP_QUEUE_FULL: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_MAINTENANCE_COVERAGE_STOP_FAILURE: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_INLINE_MAINTENANCE_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_INLINE_MAINTENANCE_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_BACKGROUND_RUNTIMES_CREATED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_BACKGROUND_RUNTIME_WORKERS_CREATED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_BACKGROUND_WAKE_SUBMITTED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_BACKGROUND_WAKE_COALESCED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_BACKGROUND_WAKE_REJECTED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_BACKGROUND_STALE_WAKE_NOOP: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_BACKGROUND_DRAIN_ROUNDS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_BACKGROUND_TASKS_COMPLETED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_BACKGROUND_SHUTDOWNS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_BACKGROUND_SHUTDOWN_JOINED_WORKERS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_BACKGROUND_SHUTDOWN_DRAINED_TASKS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_BACKGROUND_SHUTDOWN_CANCELED_TASKS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_BACKGROUND_SHUTDOWN_EXECUTOR_TASKS_COMPLETED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_BACKGROUND_SHUTDOWN_DETACHED_WORKERS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_BACKGROUND_SUBMIT_AFTER_SHUTDOWN_REJECTED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_BACKGROUND_WORKER_PANICS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_BACKGROUND_TASK_FAILURES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_BACKGROUND_TASK_START_FAILURES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_BACKGROUND_TASK_PUBLISH_FAILURES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_BACKGROUND_TASK_SNAPSHOT_LOCK_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_BACKGROUND_TASK_UNLOCKED_BUILD_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_BACKGROUND_TASK_PUBLISH_LOCK_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_BACKGROUND_TASK_LOW_TIER_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_BACKGROUND_TASK_LOW_TIER_RUNS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_BACKGROUND_PUBLISH_MANIFEST_PERSIST_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_BACKGROUND_PUBLISH_OFFLOCK_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_BACKGROUND_TASK_TOTAL_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_BACKGROUND_CANDIDATE_STALE_DEFERRED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_FOREGROUND_WAIT_BACKGROUND_LOCK_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_WRITE_ADMISSION_BLOCK_WAIT_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_INDEXED_BLOCK_SEEKS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_ACCELERATOR_BUILDS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_ACCELERATOR_REBUILDS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_TRUSTED_BLOCK_SEEKS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_CHECKED_BLOCK_SEEKS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_WARM_INSERT_REJECTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_WAL_BUFFERED_APPENDS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_WAL_BUFFER_FLUSHES_THRESHOLD: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_WAL_BUFFER_FLUSHES_CAPTURE: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_WAL_BUFFER_FLUSHES_DURABILITY: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_WAL_BUFFER_FLUSHES_TRICKLE: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_WAL_BUFFER_FLUSH_BYTES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_GROUP_DISPATCH_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_DRAIN_NOTIFY_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_GRADED_THROTTLE_DELAYS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_GRADED_THROTTLE_DELAY_MS_TOTAL: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_GRADED_THROTTLE_DELAY_MS_MAX: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_ADMISSION_RATE_RECOMPUTES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_ADMISSION_RATE_FLOOR_EVENTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_ADMISSION_RATE_LAST: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_ADMISSION_EFFECTIVE_DEBT_MAX: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_WAL_RETENTION_SAMPLES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_WAL_RETAINED_BYTES_LAST: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_WAL_RETAINED_BYTES_MAX: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_WAL_RETAINED_SEGMENTS_LAST: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_WAL_RETAINED_SEGMENTS_MAX: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_WAL_CHECKPOINT_ENQUEUE_EVENTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_WAL_CHECKPOINT_COALESCED_EVENTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_CHECKPOINT_EXECUTIONS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_WAL_TRUNCATION_DELETED_SEGMENTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_WAL_TRUNCATION_PROTECTED_SEGMENTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_WAL_TRUNCATION_FAILED_SEGMENTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_FLUSH_DRAIN_FROZEN_TABLES_DISCOVERED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_FLUSH_DRAIN_OPERATIONS_COMPLETED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_FLUSH_DRAIN_FREEZE_RETRIES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_FLUSH_DRAIN_FAILURES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_FLUSH_DRAIN_POST_DRAIN_FROZEN_TABLES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_FLUSH_MEMORY_MEASUREMENTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_FLUSH_ACTIVE_BYTES_BEFORE: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_FLUSH_FROZEN_BYTES_BEFORE: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_FLUSH_ACTIVE_BYTES_AFTER: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_FLUSH_FROZEN_BYTES_AFTER: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_MEMORY_RELEASE_DEFERRALS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_MEMORY_RELEASE_RETAINED_BYTES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_MEMORY_RELEASE_THRESHOLD_BYTES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_MEMORY_RELEASE_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_SNAPSHOT_FLOOR_ADVANCEMENTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_SNAPSHOT_FLOOR_IMPLICIT_REJECTIONS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_SNAPSHOT_PRUNING_WITH_PROOF: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_SNAPSHOT_PRUNING_DELETED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_SNAPSHOT_PRUNING_PROTECTED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_SNAPSHOT_PRUNING_FAILED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_SCORE_CANDIDATES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_MATERIALIZATION_SCORE_CANDIDATES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_MATERIALIZATION_SCORE_LAYER_INDEX_SUM: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_MATERIALIZATION_SCORE_TABLE_COUNT: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_MATERIALIZATION_SCORE_BYTE_COUNT: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_SELECTED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_SELECTED_LEVEL_SUM: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_SELECTED_SCORE_SUM: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_SELECTED_TABLE_COUNT: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_SELECTED_BYTE_COUNT: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_SELECTED_TARGET_BYTES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_LEVEL_TARGET_EVALUATIONS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_LEVEL_TARGET_LEVEL_SUM: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_LEVEL_TARGET_BYTES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_NONZERO_INPUT_SELECTIONS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_NONZERO_INPUT_LEVEL_SUM: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_NONZERO_INPUT_TABLE_INDEX_SUM: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_NONZERO_INPUT_BYTES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_NONZERO_INPUT_ROWS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_LARGEST_INPUT_SELECTIONS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_DEEPER_OVERLAP_EVALUATIONS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_DEEPER_OVERLAP_BYTES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_OUTPUT_SPLIT_BUDGET_APPLIED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_OUTPUT_SPLIT_BUDGET_DEFERRED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_OUTPUT_SPLIT_BUDGET_DEFERRED_BYTES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_OPERATIONS_COMPLETED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_L0_OPERATIONS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_L0_TO_LEVEL_ONE_OPERATIONS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_NONZERO_OPERATIONS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_BOTTOMMOST_OPERATIONS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_INPUT_TABLES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_OVERLAP_TABLES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_OUTPUT_TABLES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_INPUT_BYTES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_OUTPUT_BYTES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_METADATA_BYTES_AVOIDED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_ELAPSED_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_INPUT_ROWS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_IO_BUDGET_CONSUMED_BYTES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_IO_BUDGET_DEFERRALS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_IO_BUDGET_DEFERRED_BYTES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_IO_BUDGET_LIMIT_BYTES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_FLUSH_PREEMPTIONS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_TRIVIAL_MOVES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_RESUBMITS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_RESUBMIT_COALESCES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_COMPACTION_RESUBMIT_DEFERRED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_TABLE_REWRITE_POST_OPERATION_SCORES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_TABLE_REWRITE_POST_OPERATION_REMAINING: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_TABLE_REWRITE_POST_OPERATION_SCORE_SUM: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_TABLE_REWRITE_POST_OPERATION_ITEM_COUNT: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static LIFECYCLE_TABLE_REWRITE_POST_OPERATION_BYTE_COUNT: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_BRANCH_REGISTRY_LOOKUPS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_BRANCH_REGISTRY_DESCRIPTORS_SCANNED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_BRANCH_GUARD_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_BRANCH_GUARD_ACQUIRED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_BRANCH_GUARD_REJECTED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_QUIESCE_ATTEMPTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_QUIESCE_ACQUIRED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_QUIESCE_REJECTED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_CONFLICT_VALIDATION_CALLS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_CONFLICT_VALIDATION_SKIPPED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_CONFLICT_VALIDATION_WITHOUT_SOURCE: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_CONFLICT_VALIDATION_WITH_SOURCE: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_CONFLICT_READ_FACTS_CHECKED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_CONFLICT_CAS_FACTS_CHECKED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_CONFLICTS_DETECTED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_TIMELINE_VIEW_ROWS_SCANNED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_TIMELINE_TIMESTAMP_FACTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_TIMELINE_VERSION_FACTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_TIMELINE_RECONCILE_CALLS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_TIMELINE_RECONCILE_TIMESTAMP_FACTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_TIMELINE_RECONCILE_VERSION_FACTS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_TIMELINE_RECONCILE_ENTRY_CHECKS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_TIMELINE_LOOKUP_CALLS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_TIMELINE_LOOKUP_ENTRIES_SCANNED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_REPLAY_CLASSIFICATION_CALLS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_REPLAY_ROWS_CLASSIFIED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_REPLAY_HISTORY_CALLS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static COMMIT_REPLAY_SOURCE_PROBES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static APPEND_ROWS_APPLIED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static BRANCH_FACTS_ROWS_OBSERVED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static READ_VIEW_CAPTURES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static READ_PINS: AtomicU64 = AtomicU64::new(0);
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
static TABLE_BUILD_FACTS_FROM_STREAMING_METADATA: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_REWRITE_REDUNDANT_FACT_DECODES_AVOIDED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_REWRITE_READER_REOPENS_PERFORMED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_COMPACTION_BOUNDARY_KEY_BUFFER_ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_COMPACTION_BOUNDARY_KEY_BUFFER_REUSES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_COMPACTION_PREVIOUS_KEY_BUFFER_ALLOCATIONS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_COMPACTION_PREVIOUS_KEY_BUFFER_REUSES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_COMPACTION_MERGE_NS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_COMPACTION_MERGE_INPUT_ROWS: AtomicU64 = AtomicU64::new(0);
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
static POINT_CANDIDATE_ROW_CLONES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static POINT_CANDIDATE_ROW_CLONE_BYTES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static POINT_SELECTED_ACTIVE: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static POINT_SELECTED_FROZEN: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static POINT_SELECTED_OWNED_L0: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static POINT_SELECTED_OWNED_NONZERO: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static POINT_SELECTED_INHERITED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static POINT_EARLY_EXIT_ACTIVE: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static POINT_EARLY_EXIT_FROZEN: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static POINT_EARLY_EXIT_OWNED_L0: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static POINT_EARLY_EXIT_OWNED_NONZERO: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static POINT_EARLY_EXIT_INHERITED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static POINT_REMAINING_SOURCE_SKIPS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static POINT_INHERITED_KEY_REWRITES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_POINT_LOOKUP_KEY_BUILDS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_POINT_LOOKUP_KEY_REUSES: AtomicU64 = AtomicU64::new(0);
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
static TABLE_LAZY_FULL_MATERIALIZATIONS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static DURABLE_COMMIT_ADMITTED_OVER_BUDGET: AtomicU64 = AtomicU64::new(0);
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
static TABLE_LAZY_POINT_BLOCK_SCANS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_LAZY_POINT_ENTRIES_SCANNED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_LAZY_POINT_ROWS_DECODED: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_LAZY_POINT_FULL_BLOCK_DECODES_AVOIDED: AtomicU64 = AtomicU64::new(0);
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
static TABLE_EAGER_FILTER_PROBES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_EAGER_FILTER_NEGATIVE_PROBES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_EAGER_FILTER_POSITIVE_PROBES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_EAGER_FILTER_UNAVAILABLE_PROBES: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_SEEKS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_BOUND_CHECKS: AtomicU64 = AtomicU64::new(0);
#[cfg(feature = "perf-trace")]
static TABLE_BOUND_CHECK_NS: AtomicU64 = AtomicU64::new(0);

#[cfg(all(test, feature = "perf-trace"))]
static TEST_CAPTURE_LOCK: Mutex<()> = Mutex::new(());
#[cfg(all(test, feature = "perf-trace"))]
thread_local! {
    static TEST_CAPTURE_THREAD_ENABLED: Cell<bool> = const { Cell::new(false) };
}

#[cfg(all(test, feature = "perf-trace"))]
pub(crate) struct PerfTraceTestGuard {
    _lock: MutexGuard<'static, ()>,
    previous_enabled: bool,
}

#[cfg(all(test, feature = "perf-trace"))]
impl Drop for PerfTraceTestGuard {
    fn drop(&mut self) {
        TEST_CAPTURE_THREAD_ENABLED.with(|enabled| enabled.set(self.previous_enabled));
    }
}

/// Start an isolated counter capture for a unit test and its worker threads.
#[cfg(all(test, feature = "perf-trace"))]
pub(crate) fn begin_test_capture() -> PerfTraceTestGuard {
    let lock = TEST_CAPTURE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let previous_enabled = TEST_CAPTURE_THREAD_ENABLED.with(|enabled| {
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

/// Return whether the current test thread is capturing perf-trace counters.
///
/// Tests run in parallel. The capture mutex serializes tests that explicitly
/// capture counters, but unrelated tests may still be executing at the same
/// time. Keeping the flag thread-local prevents those unrelated tests from
/// contributing to an isolated snapshot.
#[cfg(all(test, feature = "perf-trace"))]
pub(crate) fn test_capture_enabled_for_current_thread() -> bool {
    TEST_CAPTURE_THREAD_ENABLED.with(Cell::get)
}

/// Run a worker closure with the submitting test thread's capture state.
#[cfg(all(test, feature = "perf-trace"))]
pub(crate) fn with_test_capture_enabled_for_current_thread<T>(
    enabled: bool,
    work: impl FnOnce() -> T,
) -> T {
    TEST_CAPTURE_THREAD_ENABLED.with(|thread_enabled| {
        let previous = thread_enabled.replace(enabled);
        let result = work();
        thread_enabled.set(previous);
        result
    })
}

#[cfg(any(not(test), not(feature = "perf-trace")))]
pub(crate) const fn test_capture_enabled_for_current_thread() -> bool {
    false
}

#[cfg(any(not(test), not(feature = "perf-trace")))]
pub(crate) fn with_test_capture_enabled_for_current_thread<T>(
    _enabled: bool,
    work: impl FnOnce() -> T,
) -> T {
    work()
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
    COMMIT_WAL_RECORD_BUILD_NS.store(0, Ordering::Relaxed);
    COMMIT_WAL_RECORDS_BUILT.store(0, Ordering::Relaxed);
    COMMIT_WAL_RECORD_ROWS.store(0, Ordering::Relaxed);
    COMMIT_WAL_RECORD_BYTES.store(0, Ordering::Relaxed);
    COMMIT_WAL_PAYLOAD_BYTES.store(0, Ordering::Relaxed);
    COMMIT_WAL_ROW_ENCODE_BYTES.store(0, Ordering::Relaxed);
    COMMIT_WAL_ENCODE_BUFFER_ALLOCATIONS.store(0, Ordering::Relaxed);
    COMMIT_WAL_ENCODE_BUFFER_REUSES.store(0, Ordering::Relaxed);
    COMMIT_WAL_APPEND_NS.store(0, Ordering::Relaxed);
    COMMIT_WAL_APPENDS.store(0, Ordering::Relaxed);
    COMMIT_WAL_APPEND_BYTES.store(0, Ordering::Relaxed);
    COMMIT_POST_WAL_GROWTH_NS.store(0, Ordering::Relaxed);
    COMMIT_WAL_GROWTH_FACTS_NS.store(0, Ordering::Relaxed);
    COMMIT_WAL_GROWTH_MANIFEST_NS.store(0, Ordering::Relaxed);
    COMMIT_POST_MAINTENANCE_NS.store(0, Ordering::Relaxed);
    COMMIT_EXEC_ADMISSION_NS.store(0, Ordering::Relaxed);
    COMMIT_EXEC_CONFLICT_NS.store(0, Ordering::Relaxed);
    COMMIT_EXEC_STAGE_NS.store(0, Ordering::Relaxed);
    COMMIT_EXEC_APPLY_NS.store(0, Ordering::Relaxed);
    COMMIT_EXEC_PUBLISH_NS.store(0, Ordering::Relaxed);
    COMMIT_ADMIT_NS.store(0, Ordering::Relaxed);
    COMMIT_SETUP_NS.store(0, Ordering::Relaxed);
    COMMIT_API_BATCH_CLONE_NS.store(0, Ordering::Relaxed);
    COMMIT_API_POST_NS.store(0, Ordering::Relaxed);
    COMMIT_VISIBLE_PUBLISH_ATTEMPTS.store(0, Ordering::Relaxed);
    COMMIT_VISIBLE_PUBLISH_SUCCESSES.store(0, Ordering::Relaxed);
    COMMIT_VISIBLE_PUBLISH_FAILURES.store(0, Ordering::Relaxed);
    COMMIT_ADMISSION_PRESSURE_FACTS.store(0, Ordering::Relaxed);
    COMMIT_ADMISSION_UNDER_PRESSURE.store(0, Ordering::Relaxed);
    COMMIT_ADMISSION_ACCEPTED_UNDER_PRESSURE.store(0, Ordering::Relaxed);
    COMMIT_ADMISSION_REQUIRES_MAINTENANCE.store(0, Ordering::Relaxed);
    COMMIT_ADMISSION_MUTATIONS.store(0, Ordering::Relaxed);
    COMMIT_ADMISSION_APPROX_BYTES.store(0, Ordering::Relaxed);
    COMMIT_UNRESOLVED_GATE_ADMISSION_ATTEMPTS.store(0, Ordering::Relaxed);
    COMMIT_UNRESOLVED_GATE_ADMISSION_ACQUIRED.store(0, Ordering::Relaxed);
    COMMIT_UNRESOLVED_GATE_REJECTED_UNRESOLVED.store(0, Ordering::Relaxed);
    COMMIT_UNRESOLVED_RECORDS.store(0, Ordering::Relaxed);
    COMMIT_UNRESOLVED_DURABLE_NOT_APPLIED_RECORDS.store(0, Ordering::Relaxed);
    COMMIT_UNRESOLVED_APPLIED_NOT_VISIBLE_RECORDS.store(0, Ordering::Relaxed);
    LIFECYCLE_WRITE_ADMISSION_EVALUATIONS.store(0, Ordering::Relaxed);
    LIFECYCLE_WRITE_ADMISSION_CLEAN_ACCEPTS.store(0, Ordering::Relaxed);
    LIFECYCLE_WRITE_ADMISSION_UNDER_PRESSURE_ACCEPTS.store(0, Ordering::Relaxed);
    LIFECYCLE_WRITE_ADMISSION_URGENT_ACCEPTS.store(0, Ordering::Relaxed);
    LIFECYCLE_WRITE_ADMISSION_REQUIRES_MAINTENANCE.store(0, Ordering::Relaxed);
    LIFECYCLE_WRITE_ADMISSION_INLINE_ATTEMPTS.store(0, Ordering::Relaxed);
    LIFECYCLE_WRITE_ADMISSION_URGENT_INLINE_ATTEMPTS.store(0, Ordering::Relaxed);
    LIFECYCLE_WRITE_ADMISSION_PRESSURE_REJECTS.store(0, Ordering::Relaxed);
    LIFECYCLE_WRITE_ADMISSION_RETRYABLE_REJECTS.store(0, Ordering::Relaxed);
    LIFECYCLE_WRITE_ADMISSION_PRESSURE_CLEARED_RETRIES.store(0, Ordering::Relaxed);
    LIFECYCLE_WRITE_ADMISSION_WAIT_ATTEMPTS.store(0, Ordering::Relaxed);
    LIFECYCLE_WRITE_ADMISSION_WAIT_TIMEOUTS.store(0, Ordering::Relaxed);
    LIFECYCLE_WRITE_ADMISSION_WAIT_PROGRESS_RESETS.store(0, Ordering::Relaxed);
    LIFECYCLE_PRESSURE_CLEAR_WAKES.store(0, Ordering::Relaxed);
    LIFECYCLE_PRESSURE_COLLECTION_CALLS.store(0, Ordering::Relaxed);
    LIFECYCLE_PRESSURE_COLLECTION_BRANCHES_INSPECTED.store(0, Ordering::Relaxed);
    LIFECYCLE_PRESSURE_COLLECTION_LEVELS_INSPECTED.store(0, Ordering::Relaxed);
    LIFECYCLE_PRESSURE_COLLECTION_TABLES_INSPECTED.store(0, Ordering::Relaxed);
    LIFECYCLE_PRESSURE_COLLECTION_NS.store(0, Ordering::Relaxed);
    LIFECYCLE_PRESSURE_COLLECTION_SAMPLING_SKIPS.store(0, Ordering::Relaxed);
    LIFECYCLE_PRESSURE_COLLECTION_FULL_SCANS.store(0, Ordering::Relaxed);
    LIFECYCLE_ACTIVE_BYTE_PRESSURE_BACKGROUND.store(0, Ordering::Relaxed);
    LIFECYCLE_ACTIVE_BYTE_PRESSURE_URGENT.store(0, Ordering::Relaxed);
    LIFECYCLE_ACTIVE_BYTE_PRESSURE_BLOCKING.store(0, Ordering::Relaxed);
    LIFECYCLE_POST_COMMIT_MAINTENANCE_EVALUATIONS.store(0, Ordering::Relaxed);
    LIFECYCLE_POST_COMMIT_MAINTENANCE_DISABLED.store(0, Ordering::Relaxed);
    LIFECYCLE_POST_COMMIT_MAINTENANCE_NO_TASK.store(0, Ordering::Relaxed);
    LIFECYCLE_POST_COMMIT_MAINTENANCE_TASKS_SUGGESTED.store(0, Ordering::Relaxed);
    LIFECYCLE_POST_COMMIT_MAINTENANCE_TASKS_ENQUEUED.store(0, Ordering::Relaxed);
    LIFECYCLE_POST_COMMIT_MAINTENANCE_TASKS_COALESCED.store(0, Ordering::Relaxed);
    LIFECYCLE_POST_COMMIT_MAINTENANCE_TASKS_DEFERRED.store(0, Ordering::Relaxed);
    LIFECYCLE_MAINTENANCE_COVERAGE_SCANS.store(0, Ordering::Relaxed);
    LIFECYCLE_MAINTENANCE_COVERAGE_BRANCHES_SCANNED.store(0, Ordering::Relaxed);
    LIFECYCLE_MAINTENANCE_COVERAGE_QUIET_BRANCHES_WITH_PRESSURE.store(0, Ordering::Relaxed);
    LIFECYCLE_MAINTENANCE_COVERAGE_TASKS_ENQUEUED.store(0, Ordering::Relaxed);
    LIFECYCLE_MAINTENANCE_COVERAGE_TASKS_COALESCED.store(0, Ordering::Relaxed);
    LIFECYCLE_MAINTENANCE_COVERAGE_IDLE_ROUNDS_CONSUMED.store(0, Ordering::Relaxed);
    LIFECYCLE_MAINTENANCE_COVERAGE_STOP_NO_PRESSURE.store(0, Ordering::Relaxed);
    LIFECYCLE_MAINTENANCE_COVERAGE_STOP_IDLE_LIMIT.store(0, Ordering::Relaxed);
    LIFECYCLE_MAINTENANCE_COVERAGE_STOP_QUEUE_FULL.store(0, Ordering::Relaxed);
    LIFECYCLE_MAINTENANCE_COVERAGE_STOP_FAILURE.store(0, Ordering::Relaxed);
    LIFECYCLE_INLINE_MAINTENANCE_ATTEMPTS.store(0, Ordering::Relaxed);
    LIFECYCLE_INLINE_MAINTENANCE_NS.store(0, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_RUNTIMES_CREATED.store(0, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_RUNTIME_WORKERS_CREATED.store(0, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_WAKE_SUBMITTED.store(0, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_WAKE_COALESCED.store(0, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_WAKE_REJECTED.store(0, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_STALE_WAKE_NOOP.store(0, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_DRAIN_ROUNDS.store(0, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_TASKS_COMPLETED.store(0, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_SHUTDOWNS.store(0, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_SHUTDOWN_JOINED_WORKERS.store(0, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_SHUTDOWN_DRAINED_TASKS.store(0, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_SHUTDOWN_CANCELED_TASKS.store(0, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_SHUTDOWN_EXECUTOR_TASKS_COMPLETED.store(0, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_SHUTDOWN_DETACHED_WORKERS.store(0, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_SUBMIT_AFTER_SHUTDOWN_REJECTED.store(0, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_WORKER_PANICS.store(0, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_TASK_FAILURES.store(0, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_TASK_START_FAILURES.store(0, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_TASK_PUBLISH_FAILURES.store(0, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_TASK_SNAPSHOT_LOCK_NS.store(0, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_TASK_UNLOCKED_BUILD_NS.store(0, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_TASK_PUBLISH_LOCK_NS.store(0, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_TASK_LOW_TIER_NS.store(0, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_TASK_LOW_TIER_RUNS.store(0, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_PUBLISH_MANIFEST_PERSIST_NS.store(0, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_PUBLISH_OFFLOCK_NS.store(0, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_TASK_TOTAL_NS.store(0, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_CANDIDATE_STALE_DEFERRED.store(0, Ordering::Relaxed);
    LIFECYCLE_FOREGROUND_WAIT_BACKGROUND_LOCK_NS.store(0, Ordering::Relaxed);
    LIFECYCLE_WRITE_ADMISSION_BLOCK_WAIT_NS.store(0, Ordering::Relaxed);
    TABLE_INDEXED_BLOCK_SEEKS.store(0, Ordering::Relaxed);
    TABLE_ACCELERATOR_BUILDS.store(0, Ordering::Relaxed);
    TABLE_ACCELERATOR_REBUILDS.store(0, Ordering::Relaxed);
    TABLE_TRUSTED_BLOCK_SEEKS.store(0, Ordering::Relaxed);
    TABLE_CHECKED_BLOCK_SEEKS.store(0, Ordering::Relaxed);
    TABLE_WARM_INSERT_REJECTS.store(0, Ordering::Relaxed);
    COMMIT_WAL_BUFFERED_APPENDS.store(0, Ordering::Relaxed);
    COMMIT_WAL_BUFFER_FLUSHES_THRESHOLD.store(0, Ordering::Relaxed);
    COMMIT_WAL_BUFFER_FLUSHES_CAPTURE.store(0, Ordering::Relaxed);
    COMMIT_WAL_BUFFER_FLUSHES_DURABILITY.store(0, Ordering::Relaxed);
    COMMIT_WAL_BUFFER_FLUSHES_TRICKLE.store(0, Ordering::Relaxed);
    COMMIT_WAL_BUFFER_FLUSH_BYTES.store(0, Ordering::Relaxed);
    COMMIT_GROUP_DISPATCH_NS.store(0, Ordering::Relaxed);
    COMMIT_DRAIN_NOTIFY_NS.store(0, Ordering::Relaxed);
    LIFECYCLE_GRADED_THROTTLE_DELAYS.store(0, Ordering::Relaxed);
    LIFECYCLE_GRADED_THROTTLE_DELAY_MS_TOTAL.store(0, Ordering::Relaxed);
    LIFECYCLE_GRADED_THROTTLE_DELAY_MS_MAX.store(0, Ordering::Relaxed);
    LIFECYCLE_ADMISSION_RATE_RECOMPUTES.store(0, Ordering::Relaxed);
    LIFECYCLE_ADMISSION_RATE_FLOOR_EVENTS.store(0, Ordering::Relaxed);
    LIFECYCLE_ADMISSION_RATE_LAST.store(0, Ordering::Relaxed);
    LIFECYCLE_ADMISSION_EFFECTIVE_DEBT_MAX.store(0, Ordering::Relaxed);
    LIFECYCLE_WAL_RETENTION_SAMPLES.store(0, Ordering::Relaxed);
    LIFECYCLE_WAL_RETAINED_BYTES_LAST.store(0, Ordering::Relaxed);
    LIFECYCLE_WAL_RETAINED_BYTES_MAX.store(0, Ordering::Relaxed);
    LIFECYCLE_WAL_RETAINED_SEGMENTS_LAST.store(0, Ordering::Relaxed);
    LIFECYCLE_WAL_RETAINED_SEGMENTS_MAX.store(0, Ordering::Relaxed);
    LIFECYCLE_WAL_CHECKPOINT_ENQUEUE_EVENTS.store(0, Ordering::Relaxed);
    LIFECYCLE_WAL_CHECKPOINT_COALESCED_EVENTS.store(0, Ordering::Relaxed);
    LIFECYCLE_CHECKPOINT_EXECUTIONS.store(0, Ordering::Relaxed);
    LIFECYCLE_WAL_TRUNCATION_DELETED_SEGMENTS.store(0, Ordering::Relaxed);
    LIFECYCLE_WAL_TRUNCATION_PROTECTED_SEGMENTS.store(0, Ordering::Relaxed);
    LIFECYCLE_WAL_TRUNCATION_FAILED_SEGMENTS.store(0, Ordering::Relaxed);
    LIFECYCLE_FLUSH_DRAIN_FROZEN_TABLES_DISCOVERED.store(0, Ordering::Relaxed);
    LIFECYCLE_FLUSH_DRAIN_OPERATIONS_COMPLETED.store(0, Ordering::Relaxed);
    LIFECYCLE_FLUSH_DRAIN_FREEZE_RETRIES.store(0, Ordering::Relaxed);
    LIFECYCLE_FLUSH_DRAIN_FAILURES.store(0, Ordering::Relaxed);
    LIFECYCLE_FLUSH_DRAIN_POST_DRAIN_FROZEN_TABLES.store(0, Ordering::Relaxed);
    LIFECYCLE_FLUSH_MEMORY_MEASUREMENTS.store(0, Ordering::Relaxed);
    LIFECYCLE_FLUSH_ACTIVE_BYTES_BEFORE.store(0, Ordering::Relaxed);
    LIFECYCLE_FLUSH_FROZEN_BYTES_BEFORE.store(0, Ordering::Relaxed);
    LIFECYCLE_FLUSH_ACTIVE_BYTES_AFTER.store(0, Ordering::Relaxed);
    LIFECYCLE_FLUSH_FROZEN_BYTES_AFTER.store(0, Ordering::Relaxed);
    LIFECYCLE_MEMORY_RELEASE_DEFERRALS.store(0, Ordering::Relaxed);
    LIFECYCLE_MEMORY_RELEASE_RETAINED_BYTES.store(0, Ordering::Relaxed);
    LIFECYCLE_MEMORY_RELEASE_THRESHOLD_BYTES.store(0, Ordering::Relaxed);
    LIFECYCLE_MEMORY_RELEASE_ATTEMPTS.store(0, Ordering::Relaxed);
    LIFECYCLE_SNAPSHOT_FLOOR_ADVANCEMENTS.store(0, Ordering::Relaxed);
    LIFECYCLE_SNAPSHOT_FLOOR_IMPLICIT_REJECTIONS.store(0, Ordering::Relaxed);
    LIFECYCLE_SNAPSHOT_PRUNING_WITH_PROOF.store(0, Ordering::Relaxed);
    LIFECYCLE_SNAPSHOT_PRUNING_DELETED.store(0, Ordering::Relaxed);
    LIFECYCLE_SNAPSHOT_PRUNING_PROTECTED.store(0, Ordering::Relaxed);
    LIFECYCLE_SNAPSHOT_PRUNING_FAILED.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_SCORE_CANDIDATES.store(0, Ordering::Relaxed);
    LIFECYCLE_MATERIALIZATION_SCORE_CANDIDATES.store(0, Ordering::Relaxed);
    LIFECYCLE_MATERIALIZATION_SCORE_LAYER_INDEX_SUM.store(0, Ordering::Relaxed);
    LIFECYCLE_MATERIALIZATION_SCORE_TABLE_COUNT.store(0, Ordering::Relaxed);
    LIFECYCLE_MATERIALIZATION_SCORE_BYTE_COUNT.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_SELECTED.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_SELECTED_LEVEL_SUM.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_SELECTED_SCORE_SUM.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_SELECTED_TABLE_COUNT.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_SELECTED_BYTE_COUNT.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_SELECTED_TARGET_BYTES.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_LEVEL_TARGET_EVALUATIONS.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_LEVEL_TARGET_LEVEL_SUM.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_LEVEL_TARGET_BYTES.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_NONZERO_INPUT_SELECTIONS.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_NONZERO_INPUT_LEVEL_SUM.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_NONZERO_INPUT_TABLE_INDEX_SUM.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_NONZERO_INPUT_BYTES.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_NONZERO_INPUT_ROWS.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_LARGEST_INPUT_SELECTIONS.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_DEEPER_OVERLAP_EVALUATIONS.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_DEEPER_OVERLAP_BYTES.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_OUTPUT_SPLIT_BUDGET_APPLIED.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_OUTPUT_SPLIT_BUDGET_DEFERRED.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_OUTPUT_SPLIT_BUDGET_DEFERRED_BYTES.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_OPERATIONS_COMPLETED.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_L0_OPERATIONS.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_L0_TO_LEVEL_ONE_OPERATIONS.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_NONZERO_OPERATIONS.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_BOTTOMMOST_OPERATIONS.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_INPUT_TABLES.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_OVERLAP_TABLES.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_OUTPUT_TABLES.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_INPUT_BYTES.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_OUTPUT_BYTES.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_METADATA_BYTES_AVOIDED.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_ELAPSED_NS.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_INPUT_ROWS.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_IO_BUDGET_CONSUMED_BYTES.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_IO_BUDGET_DEFERRALS.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_IO_BUDGET_DEFERRED_BYTES.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_IO_BUDGET_LIMIT_BYTES.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_FLUSH_PREEMPTIONS.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_TRIVIAL_MOVES.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_RESUBMITS.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_RESUBMIT_COALESCES.store(0, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_RESUBMIT_DEFERRED.store(0, Ordering::Relaxed);
    LIFECYCLE_TABLE_REWRITE_POST_OPERATION_SCORES.store(0, Ordering::Relaxed);
    LIFECYCLE_TABLE_REWRITE_POST_OPERATION_REMAINING.store(0, Ordering::Relaxed);
    LIFECYCLE_TABLE_REWRITE_POST_OPERATION_SCORE_SUM.store(0, Ordering::Relaxed);
    LIFECYCLE_TABLE_REWRITE_POST_OPERATION_ITEM_COUNT.store(0, Ordering::Relaxed);
    LIFECYCLE_TABLE_REWRITE_POST_OPERATION_BYTE_COUNT.store(0, Ordering::Relaxed);
    COMMIT_BRANCH_REGISTRY_LOOKUPS.store(0, Ordering::Relaxed);
    COMMIT_BRANCH_REGISTRY_DESCRIPTORS_SCANNED.store(0, Ordering::Relaxed);
    COMMIT_BRANCH_GUARD_ATTEMPTS.store(0, Ordering::Relaxed);
    COMMIT_BRANCH_GUARD_ACQUIRED.store(0, Ordering::Relaxed);
    COMMIT_BRANCH_GUARD_REJECTED.store(0, Ordering::Relaxed);
    COMMIT_QUIESCE_ATTEMPTS.store(0, Ordering::Relaxed);
    COMMIT_QUIESCE_ACQUIRED.store(0, Ordering::Relaxed);
    COMMIT_QUIESCE_REJECTED.store(0, Ordering::Relaxed);
    COMMIT_CONFLICT_VALIDATION_CALLS.store(0, Ordering::Relaxed);
    COMMIT_CONFLICT_VALIDATION_SKIPPED.store(0, Ordering::Relaxed);
    COMMIT_CONFLICT_VALIDATION_WITHOUT_SOURCE.store(0, Ordering::Relaxed);
    COMMIT_CONFLICT_VALIDATION_WITH_SOURCE.store(0, Ordering::Relaxed);
    COMMIT_CONFLICT_READ_FACTS_CHECKED.store(0, Ordering::Relaxed);
    COMMIT_CONFLICT_CAS_FACTS_CHECKED.store(0, Ordering::Relaxed);
    COMMIT_CONFLICTS_DETECTED.store(0, Ordering::Relaxed);
    COMMIT_TIMELINE_VIEW_ROWS_SCANNED.store(0, Ordering::Relaxed);
    COMMIT_TIMELINE_TIMESTAMP_FACTS.store(0, Ordering::Relaxed);
    COMMIT_TIMELINE_VERSION_FACTS.store(0, Ordering::Relaxed);
    COMMIT_TIMELINE_RECONCILE_CALLS.store(0, Ordering::Relaxed);
    COMMIT_TIMELINE_RECONCILE_TIMESTAMP_FACTS.store(0, Ordering::Relaxed);
    COMMIT_TIMELINE_RECONCILE_VERSION_FACTS.store(0, Ordering::Relaxed);
    COMMIT_TIMELINE_RECONCILE_ENTRY_CHECKS.store(0, Ordering::Relaxed);
    COMMIT_TIMELINE_LOOKUP_CALLS.store(0, Ordering::Relaxed);
    COMMIT_TIMELINE_LOOKUP_ENTRIES_SCANNED.store(0, Ordering::Relaxed);
    COMMIT_REPLAY_CLASSIFICATION_CALLS.store(0, Ordering::Relaxed);
    COMMIT_REPLAY_ROWS_CLASSIFIED.store(0, Ordering::Relaxed);
    COMMIT_REPLAY_HISTORY_CALLS.store(0, Ordering::Relaxed);
    COMMIT_REPLAY_SOURCE_PROBES.store(0, Ordering::Relaxed);
    APPEND_ROWS_APPLIED.store(0, Ordering::Relaxed);
    BRANCH_FACTS_ROWS_OBSERVED.store(0, Ordering::Relaxed);
    READ_VIEW_CAPTURES.store(0, Ordering::Relaxed);
    READ_PINS.store(0, Ordering::Relaxed);
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
    TABLE_BUILD_FACTS_FROM_STREAMING_METADATA.store(0, Ordering::Relaxed);
    TABLE_REWRITE_REDUNDANT_FACT_DECODES_AVOIDED.store(0, Ordering::Relaxed);
    TABLE_REWRITE_READER_REOPENS_PERFORMED.store(0, Ordering::Relaxed);
    TABLE_COMPACTION_BOUNDARY_KEY_BUFFER_ALLOCATIONS.store(0, Ordering::Relaxed);
    TABLE_COMPACTION_BOUNDARY_KEY_BUFFER_REUSES.store(0, Ordering::Relaxed);
    TABLE_COMPACTION_PREVIOUS_KEY_BUFFER_ALLOCATIONS.store(0, Ordering::Relaxed);
    TABLE_COMPACTION_PREVIOUS_KEY_BUFFER_REUSES.store(0, Ordering::Relaxed);
    TABLE_COMPACTION_MERGE_NS.store(0, Ordering::Relaxed);
    TABLE_COMPACTION_MERGE_INPUT_ROWS.store(0, Ordering::Relaxed);
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
    POINT_CANDIDATE_ROW_CLONES.store(0, Ordering::Relaxed);
    POINT_CANDIDATE_ROW_CLONE_BYTES.store(0, Ordering::Relaxed);
    POINT_SELECTED_ACTIVE.store(0, Ordering::Relaxed);
    POINT_SELECTED_FROZEN.store(0, Ordering::Relaxed);
    POINT_SELECTED_OWNED_L0.store(0, Ordering::Relaxed);
    POINT_SELECTED_OWNED_NONZERO.store(0, Ordering::Relaxed);
    POINT_SELECTED_INHERITED.store(0, Ordering::Relaxed);
    POINT_EARLY_EXIT_ACTIVE.store(0, Ordering::Relaxed);
    POINT_EARLY_EXIT_FROZEN.store(0, Ordering::Relaxed);
    POINT_EARLY_EXIT_OWNED_L0.store(0, Ordering::Relaxed);
    POINT_EARLY_EXIT_OWNED_NONZERO.store(0, Ordering::Relaxed);
    POINT_EARLY_EXIT_INHERITED.store(0, Ordering::Relaxed);
    POINT_REMAINING_SOURCE_SKIPS.store(0, Ordering::Relaxed);
    POINT_INHERITED_KEY_REWRITES.store(0, Ordering::Relaxed);
    TABLE_POINT_LOOKUP_KEY_BUILDS.store(0, Ordering::Relaxed);
    TABLE_POINT_LOOKUP_KEY_REUSES.store(0, Ordering::Relaxed);
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
    TABLE_LAZY_FULL_MATERIALIZATIONS.store(0, Ordering::Relaxed);
    DURABLE_COMMIT_ADMITTED_OVER_BUDGET.store(0, Ordering::Relaxed);
    TABLE_METADATA_READ_BYTES.store(0, Ordering::Relaxed);
    TABLE_INDEX_READ_BYTES.store(0, Ordering::Relaxed);
    TABLE_PROPERTIES_READ_BYTES.store(0, Ordering::Relaxed);
    TABLE_DATA_BLOCK_READS.store(0, Ordering::Relaxed);
    TABLE_DATA_BLOCK_READ_BYTES.store(0, Ordering::Relaxed);
    TABLE_DATA_BLOCK_DECODES.store(0, Ordering::Relaxed);
    TABLE_ROWS_DECODED.store(0, Ordering::Relaxed);
    TABLE_LAZY_POINT_BLOCK_SCANS.store(0, Ordering::Relaxed);
    TABLE_LAZY_POINT_ENTRIES_SCANNED.store(0, Ordering::Relaxed);
    TABLE_LAZY_POINT_ROWS_DECODED.store(0, Ordering::Relaxed);
    TABLE_LAZY_POINT_FULL_BLOCK_DECODES_AVOIDED.store(0, Ordering::Relaxed);
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
    TABLE_EAGER_FILTER_PROBES.store(0, Ordering::Relaxed);
    TABLE_EAGER_FILTER_NEGATIVE_PROBES.store(0, Ordering::Relaxed);
    TABLE_EAGER_FILTER_POSITIVE_PROBES.store(0, Ordering::Relaxed);
    TABLE_EAGER_FILTER_UNAVAILABLE_PROBES.store(0, Ordering::Relaxed);
    TABLE_SEEKS.store(0, Ordering::Relaxed);
    TABLE_BOUND_CHECKS.store(0, Ordering::Relaxed);
    TABLE_BOUND_CHECK_NS.store(0, Ordering::Relaxed);
}

/// Capture all performance proof counters.
#[cfg(feature = "perf-trace")]
#[allow(clippy::too_many_lines)]
pub fn snapshot() -> StoragePerfSnapshot {
    let lifecycle_compaction_io_budget_consumed_bytes =
        LIFECYCLE_COMPACTION_IO_BUDGET_CONSUMED_BYTES.load(Ordering::Relaxed);
    let lifecycle_compaction_input_rows = LIFECYCLE_COMPACTION_INPUT_ROWS.load(Ordering::Relaxed);
    let lifecycle_compaction_rewrite_bytes_per_row = if lifecycle_compaction_input_rows == 0 {
        0
    } else {
        lifecycle_compaction_io_budget_consumed_bytes / lifecycle_compaction_input_rows
    };
    let table_compaction_merge_input_rows =
        TABLE_COMPACTION_MERGE_INPUT_ROWS.load(Ordering::Relaxed);
    let table_compaction_merge_ns = TABLE_COMPACTION_MERGE_NS.load(Ordering::Relaxed);
    let table_compaction_merge_ns_per_input_row = if table_compaction_merge_input_rows == 0 {
        0
    } else {
        table_compaction_merge_ns / table_compaction_merge_input_rows
    };
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
        commit_wal_record_build_ns: COMMIT_WAL_RECORD_BUILD_NS.load(Ordering::Relaxed),
        commit_wal_records_built: COMMIT_WAL_RECORDS_BUILT.load(Ordering::Relaxed),
        commit_wal_record_rows: COMMIT_WAL_RECORD_ROWS.load(Ordering::Relaxed),
        commit_wal_record_bytes: COMMIT_WAL_RECORD_BYTES.load(Ordering::Relaxed),
        commit_wal_payload_bytes: COMMIT_WAL_PAYLOAD_BYTES.load(Ordering::Relaxed),
        commit_wal_row_encode_bytes: COMMIT_WAL_ROW_ENCODE_BYTES.load(Ordering::Relaxed),
        commit_wal_encode_buffer_allocations: COMMIT_WAL_ENCODE_BUFFER_ALLOCATIONS
            .load(Ordering::Relaxed),
        commit_wal_encode_buffer_reuses: COMMIT_WAL_ENCODE_BUFFER_REUSES.load(Ordering::Relaxed),
        commit_wal_append_ns: COMMIT_WAL_APPEND_NS.load(Ordering::Relaxed),
        commit_wal_appends: COMMIT_WAL_APPENDS.load(Ordering::Relaxed),
        commit_wal_append_bytes: COMMIT_WAL_APPEND_BYTES.load(Ordering::Relaxed),
        commit_post_wal_growth_ns: COMMIT_POST_WAL_GROWTH_NS.load(Ordering::Relaxed),
        commit_wal_growth_facts_ns: COMMIT_WAL_GROWTH_FACTS_NS.load(Ordering::Relaxed),
        commit_wal_growth_manifest_ns: COMMIT_WAL_GROWTH_MANIFEST_NS.load(Ordering::Relaxed),
        commit_post_maintenance_ns: COMMIT_POST_MAINTENANCE_NS.load(Ordering::Relaxed),
        commit_exec_admission_ns: COMMIT_EXEC_ADMISSION_NS.load(Ordering::Relaxed),
        commit_exec_conflict_ns: COMMIT_EXEC_CONFLICT_NS.load(Ordering::Relaxed),
        commit_exec_stage_ns: COMMIT_EXEC_STAGE_NS.load(Ordering::Relaxed),
        commit_exec_apply_ns: COMMIT_EXEC_APPLY_NS.load(Ordering::Relaxed),
        commit_exec_publish_ns: COMMIT_EXEC_PUBLISH_NS.load(Ordering::Relaxed),
        commit_admit_ns: COMMIT_ADMIT_NS.load(Ordering::Relaxed),
        commit_setup_ns: COMMIT_SETUP_NS.load(Ordering::Relaxed),
        commit_api_batch_clone_ns: COMMIT_API_BATCH_CLONE_NS.load(Ordering::Relaxed),
        commit_api_post_ns: COMMIT_API_POST_NS.load(Ordering::Relaxed),
        commit_visible_publish_attempts: COMMIT_VISIBLE_PUBLISH_ATTEMPTS.load(Ordering::Relaxed),
        commit_visible_publish_successes: COMMIT_VISIBLE_PUBLISH_SUCCESSES.load(Ordering::Relaxed),
        commit_visible_publish_failures: COMMIT_VISIBLE_PUBLISH_FAILURES.load(Ordering::Relaxed),
        commit_admission_pressure_facts: COMMIT_ADMISSION_PRESSURE_FACTS.load(Ordering::Relaxed),
        commit_admission_under_pressure: COMMIT_ADMISSION_UNDER_PRESSURE.load(Ordering::Relaxed),
        commit_admission_accepted_under_pressure: COMMIT_ADMISSION_ACCEPTED_UNDER_PRESSURE
            .load(Ordering::Relaxed),
        commit_admission_requires_maintenance: COMMIT_ADMISSION_REQUIRES_MAINTENANCE
            .load(Ordering::Relaxed),
        commit_admission_mutations: COMMIT_ADMISSION_MUTATIONS.load(Ordering::Relaxed),
        commit_admission_approx_bytes: COMMIT_ADMISSION_APPROX_BYTES.load(Ordering::Relaxed),
        commit_unresolved_gate_admission_attempts: COMMIT_UNRESOLVED_GATE_ADMISSION_ATTEMPTS
            .load(Ordering::Relaxed),
        commit_unresolved_gate_admission_acquired: COMMIT_UNRESOLVED_GATE_ADMISSION_ACQUIRED
            .load(Ordering::Relaxed),
        commit_unresolved_gate_rejected_unresolved: COMMIT_UNRESOLVED_GATE_REJECTED_UNRESOLVED
            .load(Ordering::Relaxed),
        commit_unresolved_records: COMMIT_UNRESOLVED_RECORDS.load(Ordering::Relaxed),
        commit_unresolved_durable_not_applied_records:
            COMMIT_UNRESOLVED_DURABLE_NOT_APPLIED_RECORDS.load(Ordering::Relaxed),
        commit_unresolved_applied_not_visible_records:
            COMMIT_UNRESOLVED_APPLIED_NOT_VISIBLE_RECORDS.load(Ordering::Relaxed),
        lifecycle_write_admission_evaluations: LIFECYCLE_WRITE_ADMISSION_EVALUATIONS
            .load(Ordering::Relaxed),
        lifecycle_write_admission_clean_accepts: LIFECYCLE_WRITE_ADMISSION_CLEAN_ACCEPTS
            .load(Ordering::Relaxed),
        lifecycle_write_admission_under_pressure_accepts:
            LIFECYCLE_WRITE_ADMISSION_UNDER_PRESSURE_ACCEPTS.load(Ordering::Relaxed),
        lifecycle_write_admission_urgent_accepts: LIFECYCLE_WRITE_ADMISSION_URGENT_ACCEPTS
            .load(Ordering::Relaxed),
        lifecycle_write_admission_requires_maintenance:
            LIFECYCLE_WRITE_ADMISSION_REQUIRES_MAINTENANCE.load(Ordering::Relaxed),
        lifecycle_write_admission_inline_attempts: LIFECYCLE_WRITE_ADMISSION_INLINE_ATTEMPTS
            .load(Ordering::Relaxed),
        lifecycle_write_admission_urgent_inline_attempts:
            LIFECYCLE_WRITE_ADMISSION_URGENT_INLINE_ATTEMPTS.load(Ordering::Relaxed),
        lifecycle_write_admission_pressure_rejects: LIFECYCLE_WRITE_ADMISSION_PRESSURE_REJECTS
            .load(Ordering::Relaxed),
        lifecycle_write_admission_retryable_rejects: LIFECYCLE_WRITE_ADMISSION_RETRYABLE_REJECTS
            .load(Ordering::Relaxed),
        lifecycle_write_admission_pressure_cleared_retries:
            LIFECYCLE_WRITE_ADMISSION_PRESSURE_CLEARED_RETRIES.load(Ordering::Relaxed),
        lifecycle_write_admission_wait_attempts: LIFECYCLE_WRITE_ADMISSION_WAIT_ATTEMPTS
            .load(Ordering::Relaxed),
        lifecycle_write_admission_wait_timeouts: LIFECYCLE_WRITE_ADMISSION_WAIT_TIMEOUTS
            .load(Ordering::Relaxed),
        lifecycle_write_admission_wait_progress_resets:
            LIFECYCLE_WRITE_ADMISSION_WAIT_PROGRESS_RESETS.load(Ordering::Relaxed),
        lifecycle_pressure_clear_wakes: LIFECYCLE_PRESSURE_CLEAR_WAKES.load(Ordering::Relaxed),
        lifecycle_pressure_collection_calls: LIFECYCLE_PRESSURE_COLLECTION_CALLS
            .load(Ordering::Relaxed),
        lifecycle_pressure_collection_branches_inspected:
            LIFECYCLE_PRESSURE_COLLECTION_BRANCHES_INSPECTED.load(Ordering::Relaxed),
        lifecycle_pressure_collection_levels_inspected:
            LIFECYCLE_PRESSURE_COLLECTION_LEVELS_INSPECTED.load(Ordering::Relaxed),
        lifecycle_pressure_collection_tables_inspected:
            LIFECYCLE_PRESSURE_COLLECTION_TABLES_INSPECTED.load(Ordering::Relaxed),
        lifecycle_pressure_collection_ns: LIFECYCLE_PRESSURE_COLLECTION_NS.load(Ordering::Relaxed),
        lifecycle_pressure_collection_sampling_skips: LIFECYCLE_PRESSURE_COLLECTION_SAMPLING_SKIPS
            .load(Ordering::Relaxed),
        lifecycle_pressure_collection_full_scans: LIFECYCLE_PRESSURE_COLLECTION_FULL_SCANS
            .load(Ordering::Relaxed),
        lifecycle_active_byte_pressure_background: LIFECYCLE_ACTIVE_BYTE_PRESSURE_BACKGROUND
            .load(Ordering::Relaxed),
        lifecycle_active_byte_pressure_urgent: LIFECYCLE_ACTIVE_BYTE_PRESSURE_URGENT
            .load(Ordering::Relaxed),
        lifecycle_active_byte_pressure_blocking: LIFECYCLE_ACTIVE_BYTE_PRESSURE_BLOCKING
            .load(Ordering::Relaxed),
        lifecycle_post_commit_maintenance_evaluations:
            LIFECYCLE_POST_COMMIT_MAINTENANCE_EVALUATIONS.load(Ordering::Relaxed),
        lifecycle_post_commit_maintenance_disabled: LIFECYCLE_POST_COMMIT_MAINTENANCE_DISABLED
            .load(Ordering::Relaxed),
        lifecycle_post_commit_maintenance_no_task: LIFECYCLE_POST_COMMIT_MAINTENANCE_NO_TASK
            .load(Ordering::Relaxed),
        lifecycle_post_commit_maintenance_tasks_suggested:
            LIFECYCLE_POST_COMMIT_MAINTENANCE_TASKS_SUGGESTED.load(Ordering::Relaxed),
        lifecycle_post_commit_maintenance_tasks_enqueued:
            LIFECYCLE_POST_COMMIT_MAINTENANCE_TASKS_ENQUEUED.load(Ordering::Relaxed),
        lifecycle_post_commit_maintenance_tasks_coalesced:
            LIFECYCLE_POST_COMMIT_MAINTENANCE_TASKS_COALESCED.load(Ordering::Relaxed),
        lifecycle_post_commit_maintenance_tasks_deferred:
            LIFECYCLE_POST_COMMIT_MAINTENANCE_TASKS_DEFERRED.load(Ordering::Relaxed),
        lifecycle_maintenance_coverage_scans: LIFECYCLE_MAINTENANCE_COVERAGE_SCANS
            .load(Ordering::Relaxed),
        lifecycle_maintenance_coverage_branches_scanned:
            LIFECYCLE_MAINTENANCE_COVERAGE_BRANCHES_SCANNED.load(Ordering::Relaxed),
        lifecycle_maintenance_coverage_quiet_branches_with_pressure:
            LIFECYCLE_MAINTENANCE_COVERAGE_QUIET_BRANCHES_WITH_PRESSURE.load(Ordering::Relaxed),
        lifecycle_maintenance_coverage_tasks_enqueued:
            LIFECYCLE_MAINTENANCE_COVERAGE_TASKS_ENQUEUED.load(Ordering::Relaxed),
        lifecycle_maintenance_coverage_tasks_coalesced:
            LIFECYCLE_MAINTENANCE_COVERAGE_TASKS_COALESCED.load(Ordering::Relaxed),
        lifecycle_maintenance_coverage_idle_rounds_consumed:
            LIFECYCLE_MAINTENANCE_COVERAGE_IDLE_ROUNDS_CONSUMED.load(Ordering::Relaxed),
        lifecycle_maintenance_coverage_stop_no_pressure:
            LIFECYCLE_MAINTENANCE_COVERAGE_STOP_NO_PRESSURE.load(Ordering::Relaxed),
        lifecycle_maintenance_coverage_stop_idle_limit:
            LIFECYCLE_MAINTENANCE_COVERAGE_STOP_IDLE_LIMIT.load(Ordering::Relaxed),
        lifecycle_maintenance_coverage_stop_queue_full:
            LIFECYCLE_MAINTENANCE_COVERAGE_STOP_QUEUE_FULL.load(Ordering::Relaxed),
        lifecycle_maintenance_coverage_stop_failure: LIFECYCLE_MAINTENANCE_COVERAGE_STOP_FAILURE
            .load(Ordering::Relaxed),
        lifecycle_inline_maintenance_attempts: LIFECYCLE_INLINE_MAINTENANCE_ATTEMPTS
            .load(Ordering::Relaxed),
        lifecycle_inline_maintenance_ns: LIFECYCLE_INLINE_MAINTENANCE_NS.load(Ordering::Relaxed),
        lifecycle_background_runtimes_created: LIFECYCLE_BACKGROUND_RUNTIMES_CREATED
            .load(Ordering::Relaxed),
        lifecycle_background_runtime_workers_created: LIFECYCLE_BACKGROUND_RUNTIME_WORKERS_CREATED
            .load(Ordering::Relaxed),
        lifecycle_background_wake_submitted: LIFECYCLE_BACKGROUND_WAKE_SUBMITTED
            .load(Ordering::Relaxed),
        lifecycle_background_wake_coalesced: LIFECYCLE_BACKGROUND_WAKE_COALESCED
            .load(Ordering::Relaxed),
        lifecycle_background_wake_rejected: LIFECYCLE_BACKGROUND_WAKE_REJECTED
            .load(Ordering::Relaxed),
        lifecycle_background_stale_wake_noop: LIFECYCLE_BACKGROUND_STALE_WAKE_NOOP
            .load(Ordering::Relaxed),
        lifecycle_background_drain_rounds: LIFECYCLE_BACKGROUND_DRAIN_ROUNDS
            .load(Ordering::Relaxed),
        lifecycle_background_tasks_completed: LIFECYCLE_BACKGROUND_TASKS_COMPLETED
            .load(Ordering::Relaxed),
        lifecycle_background_shutdowns: LIFECYCLE_BACKGROUND_SHUTDOWNS.load(Ordering::Relaxed),
        lifecycle_background_shutdown_joined_workers: LIFECYCLE_BACKGROUND_SHUTDOWN_JOINED_WORKERS
            .load(Ordering::Relaxed),
        lifecycle_background_shutdown_drained_tasks: LIFECYCLE_BACKGROUND_SHUTDOWN_DRAINED_TASKS
            .load(Ordering::Relaxed),
        lifecycle_background_shutdown_canceled_tasks: LIFECYCLE_BACKGROUND_SHUTDOWN_CANCELED_TASKS
            .load(Ordering::Relaxed),
        lifecycle_background_shutdown_executor_tasks_completed:
            LIFECYCLE_BACKGROUND_SHUTDOWN_EXECUTOR_TASKS_COMPLETED.load(Ordering::Relaxed),
        lifecycle_background_shutdown_detached_workers:
            LIFECYCLE_BACKGROUND_SHUTDOWN_DETACHED_WORKERS.load(Ordering::Relaxed),
        lifecycle_background_submit_after_shutdown_rejected:
            LIFECYCLE_BACKGROUND_SUBMIT_AFTER_SHUTDOWN_REJECTED.load(Ordering::Relaxed),
        lifecycle_background_worker_panics: LIFECYCLE_BACKGROUND_WORKER_PANICS
            .load(Ordering::Relaxed),
        lifecycle_background_task_failures: LIFECYCLE_BACKGROUND_TASK_FAILURES
            .load(Ordering::Relaxed),
        lifecycle_background_task_start_failures: LIFECYCLE_BACKGROUND_TASK_START_FAILURES
            .load(Ordering::Relaxed),
        lifecycle_background_task_publish_failures: LIFECYCLE_BACKGROUND_TASK_PUBLISH_FAILURES
            .load(Ordering::Relaxed),
        lifecycle_background_task_snapshot_lock_ns: LIFECYCLE_BACKGROUND_TASK_SNAPSHOT_LOCK_NS
            .load(Ordering::Relaxed),
        lifecycle_background_task_unlocked_build_ns: LIFECYCLE_BACKGROUND_TASK_UNLOCKED_BUILD_NS
            .load(Ordering::Relaxed),
        lifecycle_background_task_low_tier_ns: LIFECYCLE_BACKGROUND_TASK_LOW_TIER_NS
            .load(Ordering::Relaxed),
        lifecycle_background_task_low_tier_runs: LIFECYCLE_BACKGROUND_TASK_LOW_TIER_RUNS
            .load(Ordering::Relaxed),
        lifecycle_background_task_publish_lock_ns: LIFECYCLE_BACKGROUND_TASK_PUBLISH_LOCK_NS
            .load(Ordering::Relaxed),
        lifecycle_background_publish_manifest_persist_ns:
            LIFECYCLE_BACKGROUND_PUBLISH_MANIFEST_PERSIST_NS.load(Ordering::Relaxed),
        lifecycle_background_publish_offlock_ns: LIFECYCLE_BACKGROUND_PUBLISH_OFFLOCK_NS
            .load(Ordering::Relaxed),
        lifecycle_background_task_total_ns: LIFECYCLE_BACKGROUND_TASK_TOTAL_NS
            .load(Ordering::Relaxed),
        lifecycle_background_candidate_stale_deferred:
            LIFECYCLE_BACKGROUND_CANDIDATE_STALE_DEFERRED.load(Ordering::Relaxed),
        lifecycle_foreground_wait_background_lock_ns: LIFECYCLE_FOREGROUND_WAIT_BACKGROUND_LOCK_NS
            .load(Ordering::Relaxed),
        lifecycle_write_admission_block_wait_ns: LIFECYCLE_WRITE_ADMISSION_BLOCK_WAIT_NS
            .load(Ordering::Relaxed),
        table_indexed_block_seeks: TABLE_INDEXED_BLOCK_SEEKS.load(Ordering::Relaxed),
        table_accelerator_builds: TABLE_ACCELERATOR_BUILDS.load(Ordering::Relaxed),
        table_accelerator_rebuilds: TABLE_ACCELERATOR_REBUILDS.load(Ordering::Relaxed),
        table_trusted_block_seeks: TABLE_TRUSTED_BLOCK_SEEKS.load(Ordering::Relaxed),
        table_checked_block_seeks: TABLE_CHECKED_BLOCK_SEEKS.load(Ordering::Relaxed),
        table_warm_insert_rejects: TABLE_WARM_INSERT_REJECTS.load(Ordering::Relaxed),
        commit_wal_buffered_appends: COMMIT_WAL_BUFFERED_APPENDS.load(Ordering::Relaxed),
        commit_wal_buffer_flushes_threshold: COMMIT_WAL_BUFFER_FLUSHES_THRESHOLD
            .load(Ordering::Relaxed),
        commit_wal_buffer_flushes_capture: COMMIT_WAL_BUFFER_FLUSHES_CAPTURE
            .load(Ordering::Relaxed),
        commit_wal_buffer_flushes_durability: COMMIT_WAL_BUFFER_FLUSHES_DURABILITY
            .load(Ordering::Relaxed),
        commit_wal_buffer_flushes_trickle: COMMIT_WAL_BUFFER_FLUSHES_TRICKLE
            .load(Ordering::Relaxed),
        commit_wal_buffer_flush_bytes: COMMIT_WAL_BUFFER_FLUSH_BYTES.load(Ordering::Relaxed),
        commit_group_dispatch_ns: COMMIT_GROUP_DISPATCH_NS.load(Ordering::Relaxed),
        commit_drain_notify_ns: COMMIT_DRAIN_NOTIFY_NS.load(Ordering::Relaxed),
        lifecycle_graded_throttle_delays: LIFECYCLE_GRADED_THROTTLE_DELAYS.load(Ordering::Relaxed),
        lifecycle_graded_throttle_delay_ms_total: LIFECYCLE_GRADED_THROTTLE_DELAY_MS_TOTAL
            .load(Ordering::Relaxed),
        lifecycle_graded_throttle_delay_ms_max: LIFECYCLE_GRADED_THROTTLE_DELAY_MS_MAX
            .load(Ordering::Relaxed),
        lifecycle_admission_rate_recomputes: LIFECYCLE_ADMISSION_RATE_RECOMPUTES
            .load(Ordering::Relaxed),
        lifecycle_admission_rate_floor_events: LIFECYCLE_ADMISSION_RATE_FLOOR_EVENTS
            .load(Ordering::Relaxed),
        lifecycle_admission_rate_last: LIFECYCLE_ADMISSION_RATE_LAST.load(Ordering::Relaxed),
        lifecycle_admission_effective_debt_max: LIFECYCLE_ADMISSION_EFFECTIVE_DEBT_MAX
            .load(Ordering::Relaxed),
        lifecycle_wal_retention_samples: LIFECYCLE_WAL_RETENTION_SAMPLES.load(Ordering::Relaxed),
        lifecycle_wal_retained_bytes_last: LIFECYCLE_WAL_RETAINED_BYTES_LAST
            .load(Ordering::Relaxed),
        lifecycle_wal_retained_bytes_max: LIFECYCLE_WAL_RETAINED_BYTES_MAX.load(Ordering::Relaxed),
        lifecycle_wal_retained_segments_last: LIFECYCLE_WAL_RETAINED_SEGMENTS_LAST
            .load(Ordering::Relaxed),
        lifecycle_wal_retained_segments_max: LIFECYCLE_WAL_RETAINED_SEGMENTS_MAX
            .load(Ordering::Relaxed),
        lifecycle_wal_checkpoint_enqueue_events: LIFECYCLE_WAL_CHECKPOINT_ENQUEUE_EVENTS
            .load(Ordering::Relaxed),
        lifecycle_wal_checkpoint_coalesced_events: LIFECYCLE_WAL_CHECKPOINT_COALESCED_EVENTS
            .load(Ordering::Relaxed),
        lifecycle_checkpoint_executions: LIFECYCLE_CHECKPOINT_EXECUTIONS.load(Ordering::Relaxed),
        lifecycle_wal_truncation_deleted_segments: LIFECYCLE_WAL_TRUNCATION_DELETED_SEGMENTS
            .load(Ordering::Relaxed),
        lifecycle_wal_truncation_protected_segments: LIFECYCLE_WAL_TRUNCATION_PROTECTED_SEGMENTS
            .load(Ordering::Relaxed),
        lifecycle_wal_truncation_failed_segments: LIFECYCLE_WAL_TRUNCATION_FAILED_SEGMENTS
            .load(Ordering::Relaxed),
        lifecycle_flush_drain_frozen_tables_discovered:
            LIFECYCLE_FLUSH_DRAIN_FROZEN_TABLES_DISCOVERED.load(Ordering::Relaxed),
        lifecycle_flush_drain_operations_completed: LIFECYCLE_FLUSH_DRAIN_OPERATIONS_COMPLETED
            .load(Ordering::Relaxed),
        lifecycle_flush_drain_freeze_retries: LIFECYCLE_FLUSH_DRAIN_FREEZE_RETRIES
            .load(Ordering::Relaxed),
        lifecycle_flush_drain_failures: LIFECYCLE_FLUSH_DRAIN_FAILURES.load(Ordering::Relaxed),
        lifecycle_flush_drain_post_drain_frozen_tables:
            LIFECYCLE_FLUSH_DRAIN_POST_DRAIN_FROZEN_TABLES.load(Ordering::Relaxed),
        lifecycle_flush_memory_measurements: LIFECYCLE_FLUSH_MEMORY_MEASUREMENTS
            .load(Ordering::Relaxed),
        lifecycle_flush_active_bytes_before: LIFECYCLE_FLUSH_ACTIVE_BYTES_BEFORE
            .load(Ordering::Relaxed),
        lifecycle_flush_frozen_bytes_before: LIFECYCLE_FLUSH_FROZEN_BYTES_BEFORE
            .load(Ordering::Relaxed),
        lifecycle_flush_active_bytes_after: LIFECYCLE_FLUSH_ACTIVE_BYTES_AFTER
            .load(Ordering::Relaxed),
        lifecycle_flush_frozen_bytes_after: LIFECYCLE_FLUSH_FROZEN_BYTES_AFTER
            .load(Ordering::Relaxed),
        lifecycle_memory_release_deferrals: LIFECYCLE_MEMORY_RELEASE_DEFERRALS
            .load(Ordering::Relaxed),
        lifecycle_memory_release_retained_bytes: LIFECYCLE_MEMORY_RELEASE_RETAINED_BYTES
            .load(Ordering::Relaxed),
        lifecycle_memory_release_threshold_bytes: LIFECYCLE_MEMORY_RELEASE_THRESHOLD_BYTES
            .load(Ordering::Relaxed),
        lifecycle_memory_release_attempts: LIFECYCLE_MEMORY_RELEASE_ATTEMPTS
            .load(Ordering::Relaxed),
        lifecycle_snapshot_floor_advancements: LIFECYCLE_SNAPSHOT_FLOOR_ADVANCEMENTS
            .load(Ordering::Relaxed),
        lifecycle_snapshot_floor_implicit_rejections: LIFECYCLE_SNAPSHOT_FLOOR_IMPLICIT_REJECTIONS
            .load(Ordering::Relaxed),
        lifecycle_snapshot_pruning_with_proof: LIFECYCLE_SNAPSHOT_PRUNING_WITH_PROOF
            .load(Ordering::Relaxed),
        lifecycle_snapshot_pruning_deleted: LIFECYCLE_SNAPSHOT_PRUNING_DELETED
            .load(Ordering::Relaxed),
        lifecycle_snapshot_pruning_protected: LIFECYCLE_SNAPSHOT_PRUNING_PROTECTED
            .load(Ordering::Relaxed),
        lifecycle_snapshot_pruning_failed: LIFECYCLE_SNAPSHOT_PRUNING_FAILED
            .load(Ordering::Relaxed),
        lifecycle_compaction_score_candidates: LIFECYCLE_COMPACTION_SCORE_CANDIDATES
            .load(Ordering::Relaxed),
        lifecycle_materialization_score_candidates: LIFECYCLE_MATERIALIZATION_SCORE_CANDIDATES
            .load(Ordering::Relaxed),
        lifecycle_materialization_score_layer_index_sum:
            LIFECYCLE_MATERIALIZATION_SCORE_LAYER_INDEX_SUM.load(Ordering::Relaxed),
        lifecycle_materialization_score_table_count: LIFECYCLE_MATERIALIZATION_SCORE_TABLE_COUNT
            .load(Ordering::Relaxed),
        lifecycle_materialization_score_byte_count: LIFECYCLE_MATERIALIZATION_SCORE_BYTE_COUNT
            .load(Ordering::Relaxed),
        lifecycle_compaction_selected: LIFECYCLE_COMPACTION_SELECTED.load(Ordering::Relaxed),
        lifecycle_compaction_selected_level_sum: LIFECYCLE_COMPACTION_SELECTED_LEVEL_SUM
            .load(Ordering::Relaxed),
        lifecycle_compaction_selected_score_sum: LIFECYCLE_COMPACTION_SELECTED_SCORE_SUM
            .load(Ordering::Relaxed),
        lifecycle_compaction_selected_table_count: LIFECYCLE_COMPACTION_SELECTED_TABLE_COUNT
            .load(Ordering::Relaxed),
        lifecycle_compaction_selected_byte_count: LIFECYCLE_COMPACTION_SELECTED_BYTE_COUNT
            .load(Ordering::Relaxed),
        lifecycle_compaction_selected_target_bytes: LIFECYCLE_COMPACTION_SELECTED_TARGET_BYTES
            .load(Ordering::Relaxed),
        lifecycle_compaction_level_target_evaluations:
            LIFECYCLE_COMPACTION_LEVEL_TARGET_EVALUATIONS.load(Ordering::Relaxed),
        lifecycle_compaction_level_target_level_sum: LIFECYCLE_COMPACTION_LEVEL_TARGET_LEVEL_SUM
            .load(Ordering::Relaxed),
        lifecycle_compaction_level_target_bytes: LIFECYCLE_COMPACTION_LEVEL_TARGET_BYTES
            .load(Ordering::Relaxed),
        lifecycle_compaction_nonzero_input_selections:
            LIFECYCLE_COMPACTION_NONZERO_INPUT_SELECTIONS.load(Ordering::Relaxed),
        lifecycle_compaction_nonzero_input_level_sum: LIFECYCLE_COMPACTION_NONZERO_INPUT_LEVEL_SUM
            .load(Ordering::Relaxed),
        lifecycle_compaction_nonzero_input_table_index_sum:
            LIFECYCLE_COMPACTION_NONZERO_INPUT_TABLE_INDEX_SUM.load(Ordering::Relaxed),
        lifecycle_compaction_nonzero_input_bytes: LIFECYCLE_COMPACTION_NONZERO_INPUT_BYTES
            .load(Ordering::Relaxed),
        lifecycle_compaction_nonzero_input_rows: LIFECYCLE_COMPACTION_NONZERO_INPUT_ROWS
            .load(Ordering::Relaxed),
        lifecycle_compaction_largest_input_selections:
            LIFECYCLE_COMPACTION_LARGEST_INPUT_SELECTIONS.load(Ordering::Relaxed),
        lifecycle_compaction_deeper_overlap_evaluations:
            LIFECYCLE_COMPACTION_DEEPER_OVERLAP_EVALUATIONS.load(Ordering::Relaxed),
        lifecycle_compaction_deeper_overlap_bytes: LIFECYCLE_COMPACTION_DEEPER_OVERLAP_BYTES
            .load(Ordering::Relaxed),
        lifecycle_compaction_output_split_budget_applied:
            LIFECYCLE_COMPACTION_OUTPUT_SPLIT_BUDGET_APPLIED.load(Ordering::Relaxed),
        lifecycle_compaction_output_split_budget_deferred:
            LIFECYCLE_COMPACTION_OUTPUT_SPLIT_BUDGET_DEFERRED.load(Ordering::Relaxed),
        lifecycle_compaction_output_split_budget_deferred_bytes:
            LIFECYCLE_COMPACTION_OUTPUT_SPLIT_BUDGET_DEFERRED_BYTES.load(Ordering::Relaxed),
        lifecycle_compaction_operations_completed: LIFECYCLE_COMPACTION_OPERATIONS_COMPLETED
            .load(Ordering::Relaxed),
        lifecycle_compaction_l0_operations: LIFECYCLE_COMPACTION_L0_OPERATIONS
            .load(Ordering::Relaxed),
        lifecycle_compaction_l0_to_level_one_operations:
            LIFECYCLE_COMPACTION_L0_TO_LEVEL_ONE_OPERATIONS.load(Ordering::Relaxed),
        lifecycle_compaction_nonzero_operations: LIFECYCLE_COMPACTION_NONZERO_OPERATIONS
            .load(Ordering::Relaxed),
        lifecycle_compaction_bottommost_operations: LIFECYCLE_COMPACTION_BOTTOMMOST_OPERATIONS
            .load(Ordering::Relaxed),
        lifecycle_compaction_input_tables: LIFECYCLE_COMPACTION_INPUT_TABLES
            .load(Ordering::Relaxed),
        lifecycle_compaction_overlap_tables: LIFECYCLE_COMPACTION_OVERLAP_TABLES
            .load(Ordering::Relaxed),
        lifecycle_compaction_output_tables: LIFECYCLE_COMPACTION_OUTPUT_TABLES
            .load(Ordering::Relaxed),
        lifecycle_compaction_input_bytes: LIFECYCLE_COMPACTION_INPUT_BYTES.load(Ordering::Relaxed),
        lifecycle_compaction_output_bytes: LIFECYCLE_COMPACTION_OUTPUT_BYTES
            .load(Ordering::Relaxed),
        lifecycle_compaction_metadata_bytes_avoided: LIFECYCLE_COMPACTION_METADATA_BYTES_AVOIDED
            .load(Ordering::Relaxed),
        lifecycle_compaction_elapsed_ns: LIFECYCLE_COMPACTION_ELAPSED_NS.load(Ordering::Relaxed),
        lifecycle_compaction_input_rows,
        lifecycle_compaction_rewrite_bytes_per_row,
        lifecycle_compaction_io_budget_consumed_bytes,
        lifecycle_compaction_io_budget_deferrals: LIFECYCLE_COMPACTION_IO_BUDGET_DEFERRALS
            .load(Ordering::Relaxed),
        lifecycle_compaction_io_budget_deferred_bytes:
            LIFECYCLE_COMPACTION_IO_BUDGET_DEFERRED_BYTES.load(Ordering::Relaxed),
        lifecycle_compaction_io_budget_limit_bytes: LIFECYCLE_COMPACTION_IO_BUDGET_LIMIT_BYTES
            .load(Ordering::Relaxed),
        lifecycle_compaction_flush_preemptions: LIFECYCLE_COMPACTION_FLUSH_PREEMPTIONS
            .load(Ordering::Relaxed),
        lifecycle_compaction_trivial_moves: LIFECYCLE_COMPACTION_TRIVIAL_MOVES
            .load(Ordering::Relaxed),
        lifecycle_compaction_resubmits: LIFECYCLE_COMPACTION_RESUBMITS.load(Ordering::Relaxed),
        lifecycle_compaction_resubmit_coalesces: LIFECYCLE_COMPACTION_RESUBMIT_COALESCES
            .load(Ordering::Relaxed),
        lifecycle_compaction_resubmit_deferred: LIFECYCLE_COMPACTION_RESUBMIT_DEFERRED
            .load(Ordering::Relaxed),
        lifecycle_table_rewrite_post_operation_scores:
            LIFECYCLE_TABLE_REWRITE_POST_OPERATION_SCORES.load(Ordering::Relaxed),
        lifecycle_table_rewrite_post_operation_remaining:
            LIFECYCLE_TABLE_REWRITE_POST_OPERATION_REMAINING.load(Ordering::Relaxed),
        lifecycle_table_rewrite_post_operation_score_sum:
            LIFECYCLE_TABLE_REWRITE_POST_OPERATION_SCORE_SUM.load(Ordering::Relaxed),
        lifecycle_table_rewrite_post_operation_item_count:
            LIFECYCLE_TABLE_REWRITE_POST_OPERATION_ITEM_COUNT.load(Ordering::Relaxed),
        lifecycle_table_rewrite_post_operation_byte_count:
            LIFECYCLE_TABLE_REWRITE_POST_OPERATION_BYTE_COUNT.load(Ordering::Relaxed),
        commit_branch_registry_lookups: COMMIT_BRANCH_REGISTRY_LOOKUPS.load(Ordering::Relaxed),
        commit_branch_registry_descriptors_scanned: COMMIT_BRANCH_REGISTRY_DESCRIPTORS_SCANNED
            .load(Ordering::Relaxed),
        commit_branch_guard_attempts: COMMIT_BRANCH_GUARD_ATTEMPTS.load(Ordering::Relaxed),
        commit_branch_guard_acquired: COMMIT_BRANCH_GUARD_ACQUIRED.load(Ordering::Relaxed),
        commit_branch_guard_rejected: COMMIT_BRANCH_GUARD_REJECTED.load(Ordering::Relaxed),
        commit_quiesce_attempts: COMMIT_QUIESCE_ATTEMPTS.load(Ordering::Relaxed),
        commit_quiesce_acquired: COMMIT_QUIESCE_ACQUIRED.load(Ordering::Relaxed),
        commit_quiesce_rejected: COMMIT_QUIESCE_REJECTED.load(Ordering::Relaxed),
        commit_conflict_validation_calls: COMMIT_CONFLICT_VALIDATION_CALLS.load(Ordering::Relaxed),
        commit_conflict_validation_skipped: COMMIT_CONFLICT_VALIDATION_SKIPPED
            .load(Ordering::Relaxed),
        commit_conflict_validation_without_source: COMMIT_CONFLICT_VALIDATION_WITHOUT_SOURCE
            .load(Ordering::Relaxed),
        commit_conflict_validation_with_source: COMMIT_CONFLICT_VALIDATION_WITH_SOURCE
            .load(Ordering::Relaxed),
        commit_conflict_read_facts_checked: COMMIT_CONFLICT_READ_FACTS_CHECKED
            .load(Ordering::Relaxed),
        commit_conflict_cas_facts_checked: COMMIT_CONFLICT_CAS_FACTS_CHECKED
            .load(Ordering::Relaxed),
        commit_conflicts_detected: COMMIT_CONFLICTS_DETECTED.load(Ordering::Relaxed),
        commit_timeline_view_rows_scanned: COMMIT_TIMELINE_VIEW_ROWS_SCANNED
            .load(Ordering::Relaxed),
        commit_timeline_timestamp_facts: COMMIT_TIMELINE_TIMESTAMP_FACTS.load(Ordering::Relaxed),
        commit_timeline_version_facts: COMMIT_TIMELINE_VERSION_FACTS.load(Ordering::Relaxed),
        commit_timeline_reconcile_calls: COMMIT_TIMELINE_RECONCILE_CALLS.load(Ordering::Relaxed),
        commit_timeline_reconcile_timestamp_facts: COMMIT_TIMELINE_RECONCILE_TIMESTAMP_FACTS
            .load(Ordering::Relaxed),
        commit_timeline_reconcile_version_facts: COMMIT_TIMELINE_RECONCILE_VERSION_FACTS
            .load(Ordering::Relaxed),
        commit_timeline_reconcile_entry_checks: COMMIT_TIMELINE_RECONCILE_ENTRY_CHECKS
            .load(Ordering::Relaxed),
        commit_timeline_lookup_calls: COMMIT_TIMELINE_LOOKUP_CALLS.load(Ordering::Relaxed),
        commit_timeline_lookup_entries_scanned: COMMIT_TIMELINE_LOOKUP_ENTRIES_SCANNED
            .load(Ordering::Relaxed),
        commit_replay_classification_calls: COMMIT_REPLAY_CLASSIFICATION_CALLS
            .load(Ordering::Relaxed),
        commit_replay_rows_classified: COMMIT_REPLAY_ROWS_CLASSIFIED.load(Ordering::Relaxed),
        commit_replay_history_calls: COMMIT_REPLAY_HISTORY_CALLS.load(Ordering::Relaxed),
        commit_replay_source_probes: COMMIT_REPLAY_SOURCE_PROBES.load(Ordering::Relaxed),
        append_rows_applied: APPEND_ROWS_APPLIED.load(Ordering::Relaxed),
        branch_facts_rows_observed: BRANCH_FACTS_ROWS_OBSERVED.load(Ordering::Relaxed),
        read_view_captures: READ_VIEW_CAPTURES.load(Ordering::Relaxed),
        read_pins: READ_PINS.load(Ordering::Relaxed),
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
        table_build_facts_from_streaming_metadata: TABLE_BUILD_FACTS_FROM_STREAMING_METADATA
            .load(Ordering::Relaxed),
        table_rewrite_redundant_fact_decodes_avoided: TABLE_REWRITE_REDUNDANT_FACT_DECODES_AVOIDED
            .load(Ordering::Relaxed),
        table_rewrite_reader_reopens_performed: TABLE_REWRITE_READER_REOPENS_PERFORMED
            .load(Ordering::Relaxed),
        table_compaction_boundary_key_buffer_allocations:
            TABLE_COMPACTION_BOUNDARY_KEY_BUFFER_ALLOCATIONS.load(Ordering::Relaxed),
        table_compaction_boundary_key_buffer_reuses: TABLE_COMPACTION_BOUNDARY_KEY_BUFFER_REUSES
            .load(Ordering::Relaxed),
        table_compaction_previous_key_buffer_allocations:
            TABLE_COMPACTION_PREVIOUS_KEY_BUFFER_ALLOCATIONS.load(Ordering::Relaxed),
        table_compaction_previous_key_buffer_reuses: TABLE_COMPACTION_PREVIOUS_KEY_BUFFER_REUSES
            .load(Ordering::Relaxed),
        table_compaction_merge_ns,
        table_compaction_merge_input_rows,
        table_compaction_merge_ns_per_input_row,
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
        point_candidate_row_clones: POINT_CANDIDATE_ROW_CLONES.load(Ordering::Relaxed),
        point_candidate_row_clone_bytes: POINT_CANDIDATE_ROW_CLONE_BYTES.load(Ordering::Relaxed),
        point_selected_active: POINT_SELECTED_ACTIVE.load(Ordering::Relaxed),
        point_selected_frozen: POINT_SELECTED_FROZEN.load(Ordering::Relaxed),
        point_selected_owned_l0: POINT_SELECTED_OWNED_L0.load(Ordering::Relaxed),
        point_selected_owned_nonzero: POINT_SELECTED_OWNED_NONZERO.load(Ordering::Relaxed),
        point_selected_inherited: POINT_SELECTED_INHERITED.load(Ordering::Relaxed),
        point_early_exit_active: POINT_EARLY_EXIT_ACTIVE.load(Ordering::Relaxed),
        point_early_exit_frozen: POINT_EARLY_EXIT_FROZEN.load(Ordering::Relaxed),
        point_early_exit_owned_l0: POINT_EARLY_EXIT_OWNED_L0.load(Ordering::Relaxed),
        point_early_exit_owned_nonzero: POINT_EARLY_EXIT_OWNED_NONZERO.load(Ordering::Relaxed),
        point_early_exit_inherited: POINT_EARLY_EXIT_INHERITED.load(Ordering::Relaxed),
        point_remaining_source_skips: POINT_REMAINING_SOURCE_SKIPS.load(Ordering::Relaxed),
        point_inherited_key_rewrites: POINT_INHERITED_KEY_REWRITES.load(Ordering::Relaxed),
        table_point_lookup_key_builds: TABLE_POINT_LOOKUP_KEY_BUILDS.load(Ordering::Relaxed),
        table_point_lookup_key_reuses: TABLE_POINT_LOOKUP_KEY_REUSES.load(Ordering::Relaxed),
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
        table_lazy_full_materializations: TABLE_LAZY_FULL_MATERIALIZATIONS.load(Ordering::Relaxed),
        durable_commit_admitted_over_budget: DURABLE_COMMIT_ADMITTED_OVER_BUDGET
            .load(Ordering::Relaxed),
        table_metadata_read_bytes: TABLE_METADATA_READ_BYTES.load(Ordering::Relaxed),
        table_index_read_bytes: TABLE_INDEX_READ_BYTES.load(Ordering::Relaxed),
        table_properties_read_bytes: TABLE_PROPERTIES_READ_BYTES.load(Ordering::Relaxed),
        table_data_block_reads: TABLE_DATA_BLOCK_READS.load(Ordering::Relaxed),
        table_data_block_read_bytes: TABLE_DATA_BLOCK_READ_BYTES.load(Ordering::Relaxed),
        table_data_block_decodes: TABLE_DATA_BLOCK_DECODES.load(Ordering::Relaxed),
        table_rows_decoded: TABLE_ROWS_DECODED.load(Ordering::Relaxed),
        table_lazy_point_block_scans: TABLE_LAZY_POINT_BLOCK_SCANS.load(Ordering::Relaxed),
        table_lazy_point_entries_scanned: TABLE_LAZY_POINT_ENTRIES_SCANNED.load(Ordering::Relaxed),
        table_lazy_point_rows_decoded: TABLE_LAZY_POINT_ROWS_DECODED.load(Ordering::Relaxed),
        table_lazy_point_full_block_decodes_avoided: TABLE_LAZY_POINT_FULL_BLOCK_DECODES_AVOIDED
            .load(Ordering::Relaxed),
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
        table_eager_filter_probes: TABLE_EAGER_FILTER_PROBES.load(Ordering::Relaxed),
        table_eager_filter_negative_probes: TABLE_EAGER_FILTER_NEGATIVE_PROBES
            .load(Ordering::Relaxed),
        table_eager_filter_positive_probes: TABLE_EAGER_FILTER_POSITIVE_PROBES
            .load(Ordering::Relaxed),
        table_eager_filter_unavailable_probes: TABLE_EAGER_FILTER_UNAVAILABLE_PROBES
            .load(Ordering::Relaxed),
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
pub(crate) fn timer_elapsed(_start: PerfTraceTimer) -> std::time::Duration {
    std::time::Duration::ZERO
}

#[cfg(feature = "perf-trace")]
pub(crate) fn timer_elapsed(start: PerfTraceTimer) -> std::time::Duration {
    start.elapsed()
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
pub(crate) fn record_commit_admission_pressure_facts(
    _mutations: usize,
    _approximate_commit_bytes: usize,
    _under_pressure: bool,
    _would_require_maintenance: bool,
) {
}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_admission_pressure_facts(
    mutations: usize,
    approximate_commit_bytes: usize,
    under_pressure: bool,
    would_require_maintenance: bool,
) {
    if !recording_enabled() {
        return;
    }
    COMMIT_ADMISSION_PRESSURE_FACTS.fetch_add(1, Ordering::Relaxed);
    COMMIT_ADMISSION_MUTATIONS.fetch_add(as_u64(mutations), Ordering::Relaxed);
    COMMIT_ADMISSION_APPROX_BYTES.fetch_add(as_u64(approximate_commit_bytes), Ordering::Relaxed);
    if under_pressure {
        COMMIT_ADMISSION_UNDER_PRESSURE.fetch_add(1, Ordering::Relaxed);
    }
    if would_require_maintenance {
        COMMIT_ADMISSION_REQUIRES_MAINTENANCE.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_admission_accepted_under_pressure(_under_pressure: bool) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_admission_accepted_under_pressure(under_pressure: bool) {
    if !recording_enabled() || !under_pressure {
        return;
    }
    COMMIT_ADMISSION_ACCEPTED_UNDER_PRESSURE.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_wal_record_built(_start: PerfTraceTimer, _rows: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_wal_record_built(start: PerfTraceTimer, rows: usize) {
    if !recording_enabled() {
        return;
    }
    record_elapsed(&COMMIT_WAL_RECORD_BUILD_NS, start);
    COMMIT_WAL_RECORDS_BUILT.fetch_add(1, Ordering::Relaxed);
    COMMIT_WAL_RECORD_ROWS.fetch_add(as_u64(rows), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_wal_encode_buffers(
    _record_bytes: usize,
    _payload_bytes: usize,
    _row_encode_bytes: usize,
    _buffer_allocations: usize,
    _buffer_reuses: usize,
) {
}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_wal_encode_buffers(
    record_bytes: usize,
    payload_bytes: usize,
    row_encode_bytes: usize,
    buffer_allocations: usize,
    buffer_reuses: usize,
) {
    if !recording_enabled() {
        return;
    }
    COMMIT_WAL_RECORD_BYTES.fetch_add(as_u64(record_bytes), Ordering::Relaxed);
    COMMIT_WAL_PAYLOAD_BYTES.fetch_add(as_u64(payload_bytes), Ordering::Relaxed);
    COMMIT_WAL_ROW_ENCODE_BYTES.fetch_add(as_u64(row_encode_bytes), Ordering::Relaxed);
    COMMIT_WAL_ENCODE_BUFFER_ALLOCATIONS.fetch_add(as_u64(buffer_allocations), Ordering::Relaxed);
    COMMIT_WAL_ENCODE_BUFFER_REUSES.fetch_add(as_u64(buffer_reuses), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_wal_append_elapsed(_start: PerfTraceTimer, _bytes: u64) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_wal_append_elapsed(start: PerfTraceTimer, bytes: u64) {
    if !recording_enabled() {
        return;
    }
    record_elapsed(&COMMIT_WAL_APPEND_NS, start);
    COMMIT_WAL_APPENDS.fetch_add(1, Ordering::Relaxed);
    COMMIT_WAL_APPEND_BYTES.fetch_add(bytes, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_post_wal_growth_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_post_wal_growth_elapsed(start: PerfTraceTimer) {
    record_elapsed(&COMMIT_POST_WAL_GROWTH_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_wal_growth_facts_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_wal_growth_facts_elapsed(start: PerfTraceTimer) {
    record_elapsed(&COMMIT_WAL_GROWTH_FACTS_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_wal_growth_manifest_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_wal_growth_manifest_elapsed(start: PerfTraceTimer) {
    record_elapsed(&COMMIT_WAL_GROWTH_MANIFEST_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_post_maintenance_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_post_maintenance_elapsed(start: PerfTraceTimer) {
    record_elapsed(&COMMIT_POST_MAINTENANCE_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_exec_admission_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_exec_admission_elapsed(start: PerfTraceTimer) {
    record_elapsed(&COMMIT_EXEC_ADMISSION_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_exec_conflict_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_exec_conflict_elapsed(start: PerfTraceTimer) {
    record_elapsed(&COMMIT_EXEC_CONFLICT_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_exec_stage_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_exec_stage_elapsed(start: PerfTraceTimer) {
    record_elapsed(&COMMIT_EXEC_STAGE_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_exec_apply_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_exec_apply_elapsed(start: PerfTraceTimer) {
    record_elapsed(&COMMIT_EXEC_APPLY_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_exec_publish_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_exec_publish_elapsed(start: PerfTraceTimer) {
    record_elapsed(&COMMIT_EXEC_PUBLISH_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_admit_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_admit_elapsed(start: PerfTraceTimer) {
    record_elapsed(&COMMIT_ADMIT_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_setup_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_setup_elapsed(start: PerfTraceTimer) {
    record_elapsed(&COMMIT_SETUP_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_api_batch_clone_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_api_batch_clone_elapsed(start: PerfTraceTimer) {
    record_elapsed(&COMMIT_API_BATCH_CLONE_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_api_post_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_api_post_elapsed(start: PerfTraceTimer) {
    record_elapsed(&COMMIT_API_POST_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_visible_publish_attempt() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_visible_publish_attempt() {
    if !recording_enabled() {
        return;
    }
    COMMIT_VISIBLE_PUBLISH_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_visible_publish_success() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_visible_publish_success() {
    if !recording_enabled() {
        return;
    }
    COMMIT_VISIBLE_PUBLISH_SUCCESSES.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_visible_publish_failure() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_visible_publish_failure() {
    if !recording_enabled() {
        return;
    }
    COMMIT_VISIBLE_PUBLISH_FAILURES.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_unresolved_gate_admission_attempt() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_unresolved_gate_admission_attempt() {
    if !recording_enabled() {
        return;
    }
    COMMIT_UNRESOLVED_GATE_ADMISSION_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_unresolved_gate_admission_acquired() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_unresolved_gate_admission_acquired() {
    if !recording_enabled() {
        return;
    }
    COMMIT_UNRESOLVED_GATE_ADMISSION_ACQUIRED.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_unresolved_gate_rejected_unresolved() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_unresolved_gate_rejected_unresolved() {
    if !recording_enabled() {
        return;
    }
    COMMIT_UNRESOLVED_GATE_REJECTED_UNRESOLVED.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_unresolved_record() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_unresolved_record() {
    if !recording_enabled() {
        return;
    }
    COMMIT_UNRESOLVED_RECORDS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_unresolved_durable_not_applied_record() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_unresolved_durable_not_applied_record() {
    if !recording_enabled() {
        return;
    }
    COMMIT_UNRESOLVED_DURABLE_NOT_APPLIED_RECORDS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_unresolved_applied_not_visible_record() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_unresolved_applied_not_visible_record() {
    if !recording_enabled() {
        return;
    }
    COMMIT_UNRESOLVED_APPLIED_NOT_VISIBLE_RECORDS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_write_admission_clean(_cleared_retry: bool) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_write_admission_clean(cleared_retry: bool) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_WRITE_ADMISSION_EVALUATIONS.fetch_add(1, Ordering::Relaxed);
    LIFECYCLE_WRITE_ADMISSION_CLEAN_ACCEPTS.fetch_add(1, Ordering::Relaxed);
    if cleared_retry {
        LIFECYCLE_WRITE_ADMISSION_PRESSURE_CLEARED_RETRIES.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_write_admission_under_pressure(_cleared_retry: bool) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_write_admission_under_pressure(cleared_retry: bool) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_WRITE_ADMISSION_EVALUATIONS.fetch_add(1, Ordering::Relaxed);
    LIFECYCLE_WRITE_ADMISSION_UNDER_PRESSURE_ACCEPTS.fetch_add(1, Ordering::Relaxed);
    LIFECYCLE_WRITE_ADMISSION_URGENT_ACCEPTS.fetch_add(1, Ordering::Relaxed);
    if cleared_retry {
        LIFECYCLE_WRITE_ADMISSION_PRESSURE_CLEARED_RETRIES.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_write_admission_requires_maintenance() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_write_admission_requires_maintenance() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_WRITE_ADMISSION_EVALUATIONS.fetch_add(1, Ordering::Relaxed);
    LIFECYCLE_WRITE_ADMISSION_REQUIRES_MAINTENANCE.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_write_admission_pressure_reject(_retryable: bool) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_write_admission_pressure_reject(retryable: bool) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_WRITE_ADMISSION_PRESSURE_REJECTS.fetch_add(1, Ordering::Relaxed);
    if retryable {
        LIFECYCLE_WRITE_ADMISSION_RETRYABLE_REJECTS.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_post_commit_maintenance_evaluation() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_post_commit_maintenance_evaluation() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_POST_COMMIT_MAINTENANCE_EVALUATIONS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_post_commit_maintenance_disabled() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_post_commit_maintenance_disabled() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_POST_COMMIT_MAINTENANCE_DISABLED.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_post_commit_maintenance_no_task() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_post_commit_maintenance_no_task() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_POST_COMMIT_MAINTENANCE_NO_TASK.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_post_commit_maintenance_task_suggested() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_post_commit_maintenance_task_suggested() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_POST_COMMIT_MAINTENANCE_TASKS_SUGGESTED.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_post_commit_maintenance_enqueue(_enqueued: bool, _coalesced: bool) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_post_commit_maintenance_enqueue(enqueued: bool, coalesced: bool) {
    if !recording_enabled() {
        return;
    }
    if enqueued {
        LIFECYCLE_POST_COMMIT_MAINTENANCE_TASKS_ENQUEUED.fetch_add(1, Ordering::Relaxed);
    }
    if coalesced {
        LIFECYCLE_POST_COMMIT_MAINTENANCE_TASKS_COALESCED.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_post_commit_maintenance_deferred() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_post_commit_maintenance_deferred() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_POST_COMMIT_MAINTENANCE_TASKS_DEFERRED.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_maintenance_coverage_scan(_branches_scanned: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_maintenance_coverage_scan(branches_scanned: usize) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_MAINTENANCE_COVERAGE_SCANS.fetch_add(1, Ordering::Relaxed);
    LIFECYCLE_MAINTENANCE_COVERAGE_BRANCHES_SCANNED
        .fetch_add(as_u64(branches_scanned), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_maintenance_coverage_quiet_branch_pressure() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_maintenance_coverage_quiet_branch_pressure() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_MAINTENANCE_COVERAGE_QUIET_BRANCHES_WITH_PRESSURE.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_maintenance_coverage_enqueue(_enqueued: bool, _coalesced: bool) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_maintenance_coverage_enqueue(enqueued: bool, coalesced: bool) {
    if !recording_enabled() {
        return;
    }
    if enqueued {
        LIFECYCLE_MAINTENANCE_COVERAGE_TASKS_ENQUEUED.fetch_add(1, Ordering::Relaxed);
    }
    if coalesced {
        LIFECYCLE_MAINTENANCE_COVERAGE_TASKS_COALESCED.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_maintenance_coverage_idle_round() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_maintenance_coverage_idle_round() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_MAINTENANCE_COVERAGE_IDLE_ROUNDS_CONSUMED.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_maintenance_coverage_stop_no_pressure() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_maintenance_coverage_stop_no_pressure() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_MAINTENANCE_COVERAGE_STOP_NO_PRESSURE.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_maintenance_coverage_stop_idle_limit() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_maintenance_coverage_stop_idle_limit() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_MAINTENANCE_COVERAGE_STOP_IDLE_LIMIT.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_maintenance_coverage_stop_queue_full() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_maintenance_coverage_stop_queue_full() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_MAINTENANCE_COVERAGE_STOP_QUEUE_FULL.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_maintenance_coverage_stop_failure() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_maintenance_coverage_stop_failure() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_MAINTENANCE_COVERAGE_STOP_FAILURE.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_write_admission_inline_attempt() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_write_admission_inline_attempt() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_WRITE_ADMISSION_INLINE_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_write_admission_urgent_inline_attempt() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_write_admission_urgent_inline_attempt() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_WRITE_ADMISSION_URGENT_INLINE_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
#[allow(
    dead_code,
    reason = "bounded pressure waits are not implemented by the current fail-fast policy"
)]
pub(crate) fn record_lifecycle_write_admission_wait_attempt() {}

#[cfg(feature = "perf-trace")]
#[allow(
    dead_code,
    reason = "bounded pressure waits are not implemented by the current fail-fast policy"
)]
pub(crate) fn record_lifecycle_write_admission_wait_attempt() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_WRITE_ADMISSION_WAIT_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
#[allow(
    dead_code,
    reason = "bounded pressure waits are not implemented by the current fail-fast policy"
)]
pub(crate) fn record_lifecycle_write_admission_wait_timeout() {}

#[cfg(feature = "perf-trace")]
#[allow(
    dead_code,
    reason = "bounded pressure waits are not implemented by the current fail-fast policy"
)]
pub(crate) fn record_lifecycle_write_admission_wait_timeout() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_WRITE_ADMISSION_WAIT_TIMEOUTS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_write_admission_wait_progress_reset() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_write_admission_wait_progress_reset() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_WRITE_ADMISSION_WAIT_PROGRESS_RESETS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
#[allow(
    dead_code,
    reason = "pressure-clear wakeups are not implemented by the current fail-fast policy"
)]
pub(crate) fn record_lifecycle_pressure_clear_wake() {}

#[cfg(feature = "perf-trace")]
#[allow(
    dead_code,
    reason = "pressure-clear wakeups are not implemented by the current fail-fast policy"
)]
pub(crate) fn record_lifecycle_pressure_clear_wake() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_PRESSURE_CLEAR_WAKES.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_pressure_collection(
    _branches: usize,
    _levels: usize,
    _tables: usize,
    _start: PerfTraceTimer,
    _sampled_skip: bool,
) {
}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_pressure_collection(
    branches: usize,
    levels: usize,
    tables: usize,
    start: PerfTraceTimer,
    sampled_skip: bool,
) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_PRESSURE_COLLECTION_CALLS.fetch_add(1, Ordering::Relaxed);
    LIFECYCLE_PRESSURE_COLLECTION_BRANCHES_INSPECTED.fetch_add(as_u64(branches), Ordering::Relaxed);
    LIFECYCLE_PRESSURE_COLLECTION_LEVELS_INSPECTED.fetch_add(as_u64(levels), Ordering::Relaxed);
    LIFECYCLE_PRESSURE_COLLECTION_TABLES_INSPECTED.fetch_add(as_u64(tables), Ordering::Relaxed);
    if sampled_skip {
        LIFECYCLE_PRESSURE_COLLECTION_SAMPLING_SKIPS.fetch_add(1, Ordering::Relaxed);
    } else {
        LIFECYCLE_PRESSURE_COLLECTION_FULL_SCANS.fetch_add(1, Ordering::Relaxed);
    }
    record_elapsed(&LIFECYCLE_PRESSURE_COLLECTION_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_active_byte_pressure(
    _severity: crate::lifecycle::LifecycleStoragePressureSeverity,
) {
}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_active_byte_pressure(
    severity: crate::lifecycle::LifecycleStoragePressureSeverity,
) {
    if !recording_enabled() {
        return;
    }
    match severity {
        crate::lifecycle::LifecycleStoragePressureSeverity::None => {}
        crate::lifecycle::LifecycleStoragePressureSeverity::Background => {
            LIFECYCLE_ACTIVE_BYTE_PRESSURE_BACKGROUND.fetch_add(1, Ordering::Relaxed);
        }
        crate::lifecycle::LifecycleStoragePressureSeverity::Urgent => {
            LIFECYCLE_ACTIVE_BYTE_PRESSURE_URGENT.fetch_add(1, Ordering::Relaxed);
        }
        crate::lifecycle::LifecycleStoragePressureSeverity::BlockMutatingAdmission => {
            LIFECYCLE_ACTIVE_BYTE_PRESSURE_BLOCKING.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_inline_maintenance(_duration: std::time::Duration) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_inline_maintenance(duration: std::time::Duration) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_INLINE_MAINTENANCE_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
    LIFECYCLE_INLINE_MAINTENANCE_NS.fetch_add(
        u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX),
        Ordering::Relaxed,
    );
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_background_runtime_created(_worker_count: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_background_runtime_created(worker_count: usize) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_BACKGROUND_RUNTIMES_CREATED.fetch_add(1, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_RUNTIME_WORKERS_CREATED.fetch_add(as_u64(worker_count), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_background_wake_submitted() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_background_wake_submitted() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_BACKGROUND_WAKE_SUBMITTED.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_background_wake_coalesced() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_background_wake_coalesced() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_BACKGROUND_WAKE_COALESCED.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_background_wake_rejected() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_background_wake_rejected() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_BACKGROUND_WAKE_REJECTED.fetch_add(1, Ordering::Relaxed);
}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_background_stale_wake_noop() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_BACKGROUND_STALE_WAKE_NOOP.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_background_drain_round(_tasks_completed: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_background_drain_round(tasks_completed: usize) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_BACKGROUND_DRAIN_ROUNDS.fetch_add(1, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_TASKS_COMPLETED.fetch_add(as_u64(tasks_completed), Ordering::Relaxed);
    if tasks_completed == 0 {
        record_lifecycle_background_stale_wake_noop();
    }
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_background_shutdown() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_background_shutdown() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_BACKGROUND_SHUTDOWNS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_background_shutdown_joined_workers(_worker_count: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_background_shutdown_joined_workers(worker_count: usize) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_BACKGROUND_SHUTDOWN_JOINED_WORKERS.fetch_add(as_u64(worker_count), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_background_shutdown_drained_tasks(_task_count: u64) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_background_shutdown_drained_tasks(task_count: u64) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_BACKGROUND_SHUTDOWN_DRAINED_TASKS.fetch_add(task_count, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_background_shutdown_canceled_tasks(_task_count: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_background_shutdown_canceled_tasks(task_count: usize) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_BACKGROUND_SHUTDOWN_CANCELED_TASKS.fetch_add(as_u64(task_count), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_background_shutdown_executor_tasks_completed(_task_count: u64) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_background_shutdown_executor_tasks_completed(task_count: u64) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_BACKGROUND_SHUTDOWN_EXECUTOR_TASKS_COMPLETED.fetch_add(task_count, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_background_shutdown_detached_workers(_worker_count: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_background_shutdown_detached_workers(worker_count: usize) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_BACKGROUND_SHUTDOWN_DETACHED_WORKERS
        .fetch_add(as_u64(worker_count), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_background_submit_after_shutdown_rejected() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_background_submit_after_shutdown_rejected() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_BACKGROUND_SUBMIT_AFTER_SHUTDOWN_REJECTED.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_background_worker_panic() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_background_worker_panic() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_BACKGROUND_WORKER_PANICS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_background_task_start_failure() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_background_task_start_failure() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_BACKGROUND_TASK_FAILURES.fetch_add(1, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_TASK_START_FAILURES.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_background_task_publish_failure() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_background_task_publish_failure() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_BACKGROUND_TASK_FAILURES.fetch_add(1, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_TASK_PUBLISH_FAILURES.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_background_task_snapshot_lock(_duration: std::time::Duration) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_background_task_snapshot_lock(duration: std::time::Duration) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_BACKGROUND_TASK_SNAPSHOT_LOCK_NS.fetch_add(
        u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX),
        Ordering::Relaxed,
    );
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_background_task_unlocked_build(_duration: std::time::Duration) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_background_task_unlocked_build(duration: std::time::Duration) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_BACKGROUND_TASK_UNLOCKED_BUILD_NS.fetch_add(
        u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX),
        Ordering::Relaxed,
    );
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_background_task_publish_lock(_duration: std::time::Duration) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_background_task_publish_lock(duration: std::time::Duration) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_BACKGROUND_TASK_PUBLISH_LOCK_NS.fetch_add(
        u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX),
        Ordering::Relaxed,
    );
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_background_task_low_tier(_duration: std::time::Duration) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_background_task_low_tier(duration: std::time::Duration) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_BACKGROUND_TASK_LOW_TIER_RUNS.fetch_add(1, Ordering::Relaxed);
    LIFECYCLE_BACKGROUND_TASK_LOW_TIER_NS.fetch_add(
        u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX),
        Ordering::Relaxed,
    );
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_background_publish_manifest_persist(_duration: std::time::Duration) {
}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_background_publish_manifest_persist(duration: std::time::Duration) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_BACKGROUND_PUBLISH_MANIFEST_PERSIST_NS.fetch_add(
        u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX),
        Ordering::Relaxed,
    );
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_background_publish_offlock(_duration: std::time::Duration) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_background_publish_offlock(duration: std::time::Duration) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_BACKGROUND_PUBLISH_OFFLOCK_NS.fetch_add(
        u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX),
        Ordering::Relaxed,
    );
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_background_task_total(_duration: std::time::Duration) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_background_task_total(duration: std::time::Duration) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_BACKGROUND_TASK_TOTAL_NS.fetch_add(
        u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX),
        Ordering::Relaxed,
    );
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_background_candidate_stale_deferred() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_background_candidate_stale_deferred() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_BACKGROUND_CANDIDATE_STALE_DEFERRED.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_foreground_wait_background_lock(_duration: std::time::Duration) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_foreground_wait_background_lock(duration: std::time::Duration) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_FOREGROUND_WAIT_BACKGROUND_LOCK_NS.fetch_add(
        u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX),
        Ordering::Relaxed,
    );
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_indexed_block_seek() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_indexed_block_seek() {
    if !recording_enabled() {
        return;
    }
    TABLE_INDEXED_BLOCK_SEEKS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_accelerator_build() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_accelerator_build() {
    if !recording_enabled() {
        return;
    }
    TABLE_ACCELERATOR_BUILDS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_accelerator_rebuild() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_accelerator_rebuild() {
    if !recording_enabled() {
        return;
    }
    TABLE_ACCELERATOR_REBUILDS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_trusted_block_seek() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_trusted_block_seek() {
    if !recording_enabled() {
        return;
    }
    TABLE_TRUSTED_BLOCK_SEEKS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_checked_block_seek() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_checked_block_seek() {
    if !recording_enabled() {
        return;
    }
    TABLE_CHECKED_BLOCK_SEEKS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_warm_insert_reject() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_warm_insert_reject() {
    if !recording_enabled() {
        return;
    }
    TABLE_WARM_INSERT_REJECTS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_wal_buffered_append() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_wal_buffered_append() {
    if !recording_enabled() {
        return;
    }
    COMMIT_WAL_BUFFERED_APPENDS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_wal_buffer_flush_threshold(_bytes: u64) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_wal_buffer_flush_threshold(bytes: u64) {
    if !recording_enabled() {
        return;
    }
    COMMIT_WAL_BUFFER_FLUSHES_THRESHOLD.fetch_add(1, Ordering::Relaxed);
    COMMIT_WAL_BUFFER_FLUSH_BYTES.fetch_add(bytes, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_wal_buffer_flush_capture(_bytes: u64) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_wal_buffer_flush_capture(bytes: u64) {
    if !recording_enabled() {
        return;
    }
    COMMIT_WAL_BUFFER_FLUSHES_CAPTURE.fetch_add(1, Ordering::Relaxed);
    COMMIT_WAL_BUFFER_FLUSH_BYTES.fetch_add(bytes, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_wal_buffer_flush_trickle(_bytes: u64) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_wal_buffer_flush_trickle(bytes: u64) {
    if !recording_enabled() {
        return;
    }
    COMMIT_WAL_BUFFER_FLUSHES_TRICKLE.fetch_add(1, Ordering::Relaxed);
    COMMIT_WAL_BUFFER_FLUSH_BYTES.fetch_add(bytes, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_wal_buffer_flush_durability(_bytes: u64) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_wal_buffer_flush_durability(bytes: u64) {
    if !recording_enabled() {
        return;
    }
    COMMIT_WAL_BUFFER_FLUSHES_DURABILITY.fetch_add(1, Ordering::Relaxed);
    COMMIT_WAL_BUFFER_FLUSH_BYTES.fetch_add(bytes, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_group_dispatch_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_group_dispatch_elapsed(start: PerfTraceTimer) {
    record_elapsed(&COMMIT_GROUP_DISPATCH_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_drain_notify_elapsed(_start: PerfTraceTimer) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_drain_notify_elapsed(start: PerfTraceTimer) {
    record_elapsed(&COMMIT_DRAIN_NOTIFY_NS, start);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_graded_throttle_delay(_delay_millis: u64) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_graded_throttle_delay(delay_millis: u64) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_GRADED_THROTTLE_DELAYS.fetch_add(1, Ordering::Relaxed);
    LIFECYCLE_GRADED_THROTTLE_DELAY_MS_TOTAL.fetch_add(delay_millis, Ordering::Relaxed);
    LIFECYCLE_GRADED_THROTTLE_DELAY_MS_MAX.fetch_max(delay_millis, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_admission_rate_recompute(
    _rate: u64,
    _effective_debt: u64,
    _at_floor: bool,
) {
}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_admission_rate_recompute(
    rate: u64,
    effective_debt: u64,
    at_floor: bool,
) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_ADMISSION_RATE_RECOMPUTES.fetch_add(1, Ordering::Relaxed);
    if at_floor {
        LIFECYCLE_ADMISSION_RATE_FLOOR_EVENTS.fetch_add(1, Ordering::Relaxed);
    }
    LIFECYCLE_ADMISSION_RATE_LAST.store(rate, Ordering::Relaxed);
    LIFECYCLE_ADMISSION_EFFECTIVE_DEBT_MAX.fetch_max(effective_debt, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_write_admission_block_wait(_duration: std::time::Duration) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_write_admission_block_wait(duration: std::time::Duration) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_WRITE_ADMISSION_WAIT_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
    LIFECYCLE_WRITE_ADMISSION_BLOCK_WAIT_NS.fetch_add(
        u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX),
        Ordering::Relaxed,
    );
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_wal_growth_sample(
    _retained_bytes: u64,
    _retained_segments: usize,
    _checkpoint_enqueued: bool,
    _checkpoint_coalesced: bool,
) {
}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_wal_growth_sample(
    retained_bytes: u64,
    retained_segments: usize,
    checkpoint_enqueued: bool,
    checkpoint_coalesced: bool,
) {
    if !recording_enabled() {
        return;
    }
    let retained_segments = as_u64(retained_segments);
    LIFECYCLE_WAL_RETENTION_SAMPLES.fetch_add(1, Ordering::Relaxed);
    LIFECYCLE_WAL_RETAINED_BYTES_LAST.store(retained_bytes, Ordering::Relaxed);
    LIFECYCLE_WAL_RETAINED_SEGMENTS_LAST.store(retained_segments, Ordering::Relaxed);
    record_atomic_max(&LIFECYCLE_WAL_RETAINED_BYTES_MAX, retained_bytes);
    record_atomic_max(&LIFECYCLE_WAL_RETAINED_SEGMENTS_MAX, retained_segments);
    if checkpoint_enqueued {
        LIFECYCLE_WAL_CHECKPOINT_ENQUEUE_EVENTS.fetch_add(1, Ordering::Relaxed);
    }
    if checkpoint_coalesced {
        LIFECYCLE_WAL_CHECKPOINT_COALESCED_EVENTS.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_checkpoint_execution() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_checkpoint_execution() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_CHECKPOINT_EXECUTIONS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_wal_truncation_report(
    _deleted_segments: usize,
    _protected_segments: usize,
    _failed_segments: usize,
) {
}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_wal_truncation_report(
    deleted_segments: usize,
    protected_segments: usize,
    failed_segments: usize,
) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_WAL_TRUNCATION_DELETED_SEGMENTS
        .fetch_add(as_u64(deleted_segments), Ordering::Relaxed);
    LIFECYCLE_WAL_TRUNCATION_PROTECTED_SEGMENTS
        .fetch_add(as_u64(protected_segments), Ordering::Relaxed);
    LIFECYCLE_WAL_TRUNCATION_FAILED_SEGMENTS.fetch_add(as_u64(failed_segments), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_flush_drain_frozen_tables_discovered(_count: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_flush_drain_frozen_tables_discovered(count: usize) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_FLUSH_DRAIN_FROZEN_TABLES_DISCOVERED.fetch_add(as_u64(count), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_flush_drain_operations_completed(_count: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_flush_drain_operations_completed(count: usize) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_FLUSH_DRAIN_OPERATIONS_COMPLETED.fetch_add(as_u64(count), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_flush_drain_freeze_retries(_count: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_flush_drain_freeze_retries(count: usize) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_FLUSH_DRAIN_FREEZE_RETRIES.fetch_add(as_u64(count), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_flush_drain_failures(_count: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_flush_drain_failures(count: usize) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_FLUSH_DRAIN_FAILURES.fetch_add(as_u64(count), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_flush_drain_post_drain_frozen_tables(_count: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_flush_drain_post_drain_frozen_tables(count: usize) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_FLUSH_DRAIN_POST_DRAIN_FROZEN_TABLES.fetch_add(as_u64(count), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_flush_memory_retention(
    _active_bytes_before: u64,
    _frozen_bytes_before: u64,
    _active_bytes_after: u64,
    _frozen_bytes_after: u64,
    _release_threshold_bytes: u64,
) {
}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_flush_memory_retention(
    active_bytes_before: u64,
    frozen_bytes_before: u64,
    active_bytes_after: u64,
    frozen_bytes_after: u64,
    release_threshold_bytes: u64,
) {
    if !recording_enabled() {
        return;
    }
    let retained_bytes = active_bytes_after.saturating_add(frozen_bytes_after);
    LIFECYCLE_FLUSH_MEMORY_MEASUREMENTS.fetch_add(1, Ordering::Relaxed);
    LIFECYCLE_FLUSH_ACTIVE_BYTES_BEFORE.fetch_add(active_bytes_before, Ordering::Relaxed);
    LIFECYCLE_FLUSH_FROZEN_BYTES_BEFORE.fetch_add(frozen_bytes_before, Ordering::Relaxed);
    LIFECYCLE_FLUSH_ACTIVE_BYTES_AFTER.fetch_add(active_bytes_after, Ordering::Relaxed);
    LIFECYCLE_FLUSH_FROZEN_BYTES_AFTER.fetch_add(frozen_bytes_after, Ordering::Relaxed);
    LIFECYCLE_MEMORY_RELEASE_DEFERRALS.fetch_add(1, Ordering::Relaxed);
    LIFECYCLE_MEMORY_RELEASE_RETAINED_BYTES.fetch_add(retained_bytes, Ordering::Relaxed);
    LIFECYCLE_MEMORY_RELEASE_THRESHOLD_BYTES.fetch_add(release_threshold_bytes, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_snapshot_floor_implicit_rejection() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_snapshot_floor_implicit_rejection() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_SNAPSHOT_FLOOR_IMPLICIT_REJECTIONS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_snapshot_pruning_with_proof(
    _deleted: usize,
    _protected: usize,
    _failed: usize,
) {
}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_snapshot_pruning_with_proof(
    deleted: usize,
    protected: usize,
    failed: usize,
) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_SNAPSHOT_PRUNING_WITH_PROOF.fetch_add(1, Ordering::Relaxed);
    LIFECYCLE_SNAPSHOT_PRUNING_DELETED.fetch_add(as_u64(deleted), Ordering::Relaxed);
    LIFECYCLE_SNAPSHOT_PRUNING_PROTECTED.fetch_add(as_u64(protected), Ordering::Relaxed);
    LIFECYCLE_SNAPSHOT_PRUNING_FAILED.fetch_add(as_u64(failed), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_compaction_score_candidate(
    _level: u8,
    _score: u64,
    _table_count: usize,
    _byte_count: u64,
) {
}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_compaction_score_candidate(
    _level: u8,
    _score: u64,
    _table_count: usize,
    _byte_count: u64,
) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_COMPACTION_SCORE_CANDIDATES.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_materialization_score_candidate(
    _layer_index: usize,
    _score: u64,
    _table_count: usize,
    _byte_count: u64,
) {
}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_materialization_score_candidate(
    layer_index: usize,
    _score: u64,
    table_count: usize,
    byte_count: u64,
) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_MATERIALIZATION_SCORE_CANDIDATES.fetch_add(1, Ordering::Relaxed);
    LIFECYCLE_MATERIALIZATION_SCORE_LAYER_INDEX_SUM
        .fetch_add(as_u64(layer_index), Ordering::Relaxed);
    LIFECYCLE_MATERIALIZATION_SCORE_TABLE_COUNT.fetch_add(as_u64(table_count), Ordering::Relaxed);
    LIFECYCLE_MATERIALIZATION_SCORE_BYTE_COUNT.fetch_add(byte_count, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_compaction_level_target(_level: u8, _target_bytes: u64) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_compaction_level_target(level: u8, target_bytes: u64) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_COMPACTION_LEVEL_TARGET_EVALUATIONS.fetch_add(1, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_LEVEL_TARGET_LEVEL_SUM.fetch_add(u64::from(level), Ordering::Relaxed);
    LIFECYCLE_COMPACTION_LEVEL_TARGET_BYTES.fetch_add(target_bytes, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_compaction_selected(
    _level: u8,
    _score: u64,
    _table_count: usize,
    _byte_count: u64,
    _target_bytes: u64,
) {
}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_compaction_selected(
    level: u8,
    score: u64,
    table_count: usize,
    byte_count: u64,
    target_bytes: u64,
) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_COMPACTION_SELECTED.fetch_add(1, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_SELECTED_LEVEL_SUM.fetch_add(u64::from(level), Ordering::Relaxed);
    LIFECYCLE_COMPACTION_SELECTED_SCORE_SUM.fetch_add(score, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_SELECTED_TABLE_COUNT.fetch_add(as_u64(table_count), Ordering::Relaxed);
    LIFECYCLE_COMPACTION_SELECTED_BYTE_COUNT.fetch_add(byte_count, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_SELECTED_TARGET_BYTES.fetch_add(target_bytes, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_compaction_nonzero_input_selected(
    _level: u8,
    _table_index: usize,
    _byte_count: u64,
    _row_count: u64,
) {
}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_compaction_nonzero_input_selected(
    level: u8,
    table_index: usize,
    byte_count: u64,
    row_count: u64,
) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_COMPACTION_NONZERO_INPUT_SELECTIONS.fetch_add(1, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_NONZERO_INPUT_LEVEL_SUM.fetch_add(u64::from(level), Ordering::Relaxed);
    LIFECYCLE_COMPACTION_NONZERO_INPUT_TABLE_INDEX_SUM
        .fetch_add(as_u64(table_index), Ordering::Relaxed);
    LIFECYCLE_COMPACTION_NONZERO_INPUT_BYTES.fetch_add(byte_count, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_NONZERO_INPUT_ROWS.fetch_add(row_count, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_LARGEST_INPUT_SELECTIONS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_compaction_deeper_overlap_considered(_level: u8, _byte_count: u64) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_compaction_deeper_overlap_considered(_level: u8, byte_count: u64) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_COMPACTION_DEEPER_OVERLAP_EVALUATIONS.fetch_add(1, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_DEEPER_OVERLAP_BYTES.fetch_add(byte_count, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_compaction_output_split_budget_deferred(_byte_count: u64) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_compaction_output_split_budget_deferred(byte_count: u64) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_COMPACTION_OUTPUT_SPLIT_BUDGET_DEFERRED.fetch_add(1, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_OUTPUT_SPLIT_BUDGET_DEFERRED_BYTES
        .fetch_add(byte_count, Ordering::Relaxed);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LifecycleCompactionOperationKind {
    L0,
    L0ToLevelOne,
    Nonzero,
    Bottommost,
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_compaction_operation(
    _level: u8,
    _kind: LifecycleCompactionOperationKind,
    _input_tables: usize,
    _overlap_tables: usize,
    _output_tables: usize,
    _input_bytes: u64,
    _output_bytes: u64,
    _metadata_bytes_avoided: u64,
    _elapsed: std::time::Duration,
    _input_rows: u64,
    _trivial_move: bool,
) {
}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_compaction_operation(
    _level: u8,
    kind: LifecycleCompactionOperationKind,
    input_tables: usize,
    overlap_tables: usize,
    output_tables: usize,
    input_bytes: u64,
    output_bytes: u64,
    metadata_bytes_avoided: u64,
    elapsed: std::time::Duration,
    input_rows: u64,
    trivial_move: bool,
) {
    if !recording_enabled() {
        return;
    }
    let budget_consumed = input_bytes.saturating_add(output_bytes);
    LIFECYCLE_COMPACTION_OPERATIONS_COMPLETED.fetch_add(1, Ordering::Relaxed);
    match kind {
        LifecycleCompactionOperationKind::L0 => {
            LIFECYCLE_COMPACTION_L0_OPERATIONS.fetch_add(1, Ordering::Relaxed);
        }
        LifecycleCompactionOperationKind::L0ToLevelOne => {
            LIFECYCLE_COMPACTION_L0_TO_LEVEL_ONE_OPERATIONS.fetch_add(1, Ordering::Relaxed);
        }
        LifecycleCompactionOperationKind::Nonzero => {
            LIFECYCLE_COMPACTION_NONZERO_OPERATIONS.fetch_add(1, Ordering::Relaxed);
        }
        LifecycleCompactionOperationKind::Bottommost => {
            LIFECYCLE_COMPACTION_BOTTOMMOST_OPERATIONS.fetch_add(1, Ordering::Relaxed);
        }
    }
    LIFECYCLE_COMPACTION_INPUT_TABLES.fetch_add(as_u64(input_tables), Ordering::Relaxed);
    LIFECYCLE_COMPACTION_OVERLAP_TABLES.fetch_add(as_u64(overlap_tables), Ordering::Relaxed);
    LIFECYCLE_COMPACTION_OUTPUT_TABLES.fetch_add(as_u64(output_tables), Ordering::Relaxed);
    LIFECYCLE_COMPACTION_INPUT_BYTES.fetch_add(input_bytes, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_OUTPUT_BYTES.fetch_add(output_bytes, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_METADATA_BYTES_AVOIDED
        .fetch_add(metadata_bytes_avoided, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_ELAPSED_NS.fetch_add(
        u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX),
        Ordering::Relaxed,
    );
    LIFECYCLE_COMPACTION_INPUT_ROWS.fetch_add(input_rows, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_IO_BUDGET_CONSUMED_BYTES.fetch_add(budget_consumed, Ordering::Relaxed);
    if trivial_move {
        LIFECYCLE_COMPACTION_TRIVIAL_MOVES.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_compaction_io_budget_deferred(
    _estimated_bytes: u64,
    _limit_bytes: u64,
) {
}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_compaction_io_budget_deferred(
    estimated_bytes: u64,
    limit_bytes: u64,
) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_COMPACTION_IO_BUDGET_DEFERRALS.fetch_add(1, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_IO_BUDGET_DEFERRED_BYTES.fetch_add(estimated_bytes, Ordering::Relaxed);
    LIFECYCLE_COMPACTION_IO_BUDGET_LIMIT_BYTES.fetch_add(limit_bytes, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_compaction_flush_preempted() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_compaction_flush_preempted() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_COMPACTION_FLUSH_PREEMPTIONS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_compaction_resubmit(_coalesced: bool) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_compaction_resubmit(coalesced: bool) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_COMPACTION_RESUBMITS.fetch_add(1, Ordering::Relaxed);
    if coalesced {
        LIFECYCLE_COMPACTION_RESUBMIT_COALESCES.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_compaction_resubmit_deferred() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_compaction_resubmit_deferred() {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_COMPACTION_RESUBMIT_DEFERRED.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_lifecycle_table_rewrite_post_operation_score(
    _remaining: bool,
    _score: u64,
    _item_count: usize,
    _byte_count: u64,
) {
}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lifecycle_table_rewrite_post_operation_score(
    remaining: bool,
    score: u64,
    item_count: usize,
    byte_count: u64,
) {
    if !recording_enabled() {
        return;
    }
    LIFECYCLE_TABLE_REWRITE_POST_OPERATION_SCORES.fetch_add(1, Ordering::Relaxed);
    if remaining {
        LIFECYCLE_TABLE_REWRITE_POST_OPERATION_REMAINING.fetch_add(1, Ordering::Relaxed);
    }
    LIFECYCLE_TABLE_REWRITE_POST_OPERATION_SCORE_SUM.fetch_add(score, Ordering::Relaxed);
    LIFECYCLE_TABLE_REWRITE_POST_OPERATION_ITEM_COUNT
        .fetch_add(as_u64(item_count), Ordering::Relaxed);
    LIFECYCLE_TABLE_REWRITE_POST_OPERATION_BYTE_COUNT.fetch_add(byte_count, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_branch_registry_lookup(_descriptors_scanned: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_branch_registry_lookup(descriptors_scanned: usize) {
    if !recording_enabled() {
        return;
    }
    COMMIT_BRANCH_REGISTRY_LOOKUPS.fetch_add(1, Ordering::Relaxed);
    COMMIT_BRANCH_REGISTRY_DESCRIPTORS_SCANNED
        .fetch_add(as_u64(descriptors_scanned), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_branch_guard_attempt() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_branch_guard_attempt() {
    if !recording_enabled() {
        return;
    }
    COMMIT_BRANCH_GUARD_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_branch_guard_acquired() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_branch_guard_acquired() {
    if !recording_enabled() {
        return;
    }
    COMMIT_BRANCH_GUARD_ACQUIRED.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_branch_guard_rejected() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_branch_guard_rejected() {
    if !recording_enabled() {
        return;
    }
    COMMIT_BRANCH_GUARD_REJECTED.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_quiesce_attempt() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_quiesce_attempt() {
    if !recording_enabled() {
        return;
    }
    COMMIT_QUIESCE_ATTEMPTS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_quiesce_acquired() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_quiesce_acquired() {
    if !recording_enabled() {
        return;
    }
    COMMIT_QUIESCE_ACQUIRED.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_quiesce_rejected() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_quiesce_rejected() {
    if !recording_enabled() {
        return;
    }
    COMMIT_QUIESCE_REJECTED.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_conflict_validation_call() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_conflict_validation_call() {
    if !recording_enabled() {
        return;
    }
    COMMIT_CONFLICT_VALIDATION_CALLS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_conflict_validation_skipped() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_conflict_validation_skipped() {
    if !recording_enabled() {
        return;
    }
    COMMIT_CONFLICT_VALIDATION_SKIPPED.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_conflict_validation_without_source() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_conflict_validation_without_source() {
    if !recording_enabled() {
        return;
    }
    COMMIT_CONFLICT_VALIDATION_WITHOUT_SOURCE.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_conflict_validation_with_source(_read_facts: usize, _cas_facts: usize) {
}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_conflict_validation_with_source(read_facts: usize, cas_facts: usize) {
    if !recording_enabled() {
        return;
    }
    COMMIT_CONFLICT_VALIDATION_WITH_SOURCE.fetch_add(1, Ordering::Relaxed);
    COMMIT_CONFLICT_READ_FACTS_CHECKED.fetch_add(as_u64(read_facts), Ordering::Relaxed);
    COMMIT_CONFLICT_CAS_FACTS_CHECKED.fetch_add(as_u64(cas_facts), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_conflict_detected() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_conflict_detected() {
    if !recording_enabled() {
        return;
    }
    COMMIT_CONFLICTS_DETECTED.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_timeline_view_rows(_rows_scanned: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_timeline_view_rows(rows_scanned: usize) {
    if !recording_enabled() {
        return;
    }
    COMMIT_TIMELINE_VIEW_ROWS_SCANNED.fetch_add(as_u64(rows_scanned), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_timeline_view_facts(_timestamp_facts: usize, _version_facts: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_timeline_view_facts(timestamp_facts: usize, version_facts: usize) {
    if !recording_enabled() {
        return;
    }
    COMMIT_TIMELINE_TIMESTAMP_FACTS.fetch_add(as_u64(timestamp_facts), Ordering::Relaxed);
    COMMIT_TIMELINE_VERSION_FACTS.fetch_add(as_u64(version_facts), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_timeline_reconcile(
    _timestamp_facts: usize,
    _version_facts: usize,
    _entry_checks: usize,
) {
}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_timeline_reconcile(
    timestamp_facts: usize,
    version_facts: usize,
    entry_checks: usize,
) {
    if !recording_enabled() {
        return;
    }
    COMMIT_TIMELINE_RECONCILE_CALLS.fetch_add(1, Ordering::Relaxed);
    COMMIT_TIMELINE_RECONCILE_TIMESTAMP_FACTS.fetch_add(as_u64(timestamp_facts), Ordering::Relaxed);
    COMMIT_TIMELINE_RECONCILE_VERSION_FACTS.fetch_add(as_u64(version_facts), Ordering::Relaxed);
    COMMIT_TIMELINE_RECONCILE_ENTRY_CHECKS.fetch_add(as_u64(entry_checks), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_timeline_lookup(_entries_scanned: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_timeline_lookup(entries_scanned: usize) {
    if !recording_enabled() {
        return;
    }
    COMMIT_TIMELINE_LOOKUP_CALLS.fetch_add(1, Ordering::Relaxed);
    COMMIT_TIMELINE_LOOKUP_ENTRIES_SCANNED.fetch_add(as_u64(entries_scanned), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_commit_replay_classification(
    _rows: usize,
    _history_calls: usize,
    _source_probes: usize,
) {
}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_commit_replay_classification(
    rows: usize,
    history_calls: usize,
    source_probes: usize,
) {
    if !recording_enabled() {
        return;
    }
    COMMIT_REPLAY_CLASSIFICATION_CALLS.fetch_add(1, Ordering::Relaxed);
    COMMIT_REPLAY_ROWS_CLASSIFIED.fetch_add(as_u64(rows), Ordering::Relaxed);
    COMMIT_REPLAY_HISTORY_CALLS.fetch_add(as_u64(history_calls), Ordering::Relaxed);
    COMMIT_REPLAY_SOURCE_PROBES.fetch_add(as_u64(source_probes), Ordering::Relaxed);
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
pub(crate) fn record_read_pin() {}

/// Count one per-read active-memtable pin (`clone_for_read_view` on the read path).
#[cfg(feature = "perf-trace")]
pub(crate) fn record_read_pin() {
    if !recording_enabled() {
        return;
    }
    READ_PINS.fetch_add(1, Ordering::Relaxed);
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
pub(crate) fn record_table_build_facts_from_streaming_metadata() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_build_facts_from_streaming_metadata() {
    if !recording_enabled() {
        return;
    }
    TABLE_BUILD_FACTS_FROM_STREAMING_METADATA.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_rewrite_redundant_fact_decode_avoided() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_rewrite_redundant_fact_decode_avoided() {
    if !recording_enabled() {
        return;
    }
    TABLE_REWRITE_REDUNDANT_FACT_DECODES_AVOIDED.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_rewrite_reader_reopen_performed() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_rewrite_reader_reopen_performed() {
    if !recording_enabled() {
        return;
    }
    TABLE_REWRITE_READER_REOPENS_PERFORMED.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_compaction_boundary_key_buffer_allocation() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_compaction_boundary_key_buffer_allocation() {
    if !recording_enabled() {
        return;
    }
    TABLE_COMPACTION_BOUNDARY_KEY_BUFFER_ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_compaction_boundary_key_buffer_reuse() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_compaction_boundary_key_buffer_reuse() {
    if !recording_enabled() {
        return;
    }
    TABLE_COMPACTION_BOUNDARY_KEY_BUFFER_REUSES.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_compaction_previous_key_buffer_allocation() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_compaction_previous_key_buffer_allocation() {
    if !recording_enabled() {
        return;
    }
    TABLE_COMPACTION_PREVIOUS_KEY_BUFFER_ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_compaction_previous_key_buffer_reuse() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_compaction_previous_key_buffer_reuse() {
    if !recording_enabled() {
        return;
    }
    TABLE_COMPACTION_PREVIOUS_KEY_BUFFER_REUSES.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_compaction_merge_elapsed(
    _elapsed: std::time::Duration,
    _input_rows: u64,
) {
}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_compaction_merge_elapsed(elapsed: std::time::Duration, input_rows: u64) {
    if !recording_enabled() {
        return;
    }
    TABLE_COMPACTION_MERGE_NS.fetch_add(
        u64::try_from(elapsed.as_nanos()).unwrap_or(u64::MAX),
        Ordering::Relaxed,
    );
    TABLE_COMPACTION_MERGE_INPUT_ROWS.fetch_add(input_rows, Ordering::Relaxed);
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
pub(crate) fn record_branch_point_candidate_row_clone(_row_bytes: usize) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_branch_point_candidate_row_clone(row_bytes: usize) {
    if !recording_enabled() {
        return;
    }
    POINT_CANDIDATE_ROW_CLONES.fetch_add(1, Ordering::Relaxed);
    POINT_CANDIDATE_ROW_CLONE_BYTES.fetch_add(as_u64(row_bytes), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_branch_point_selected(_source: BranchPointSourceKind) {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_branch_point_selected(source: BranchPointSourceKind) {
    if !recording_enabled() {
        return;
    }
    match source {
        BranchPointSourceKind::Active => {
            POINT_SELECTED_ACTIVE.fetch_add(1, Ordering::Relaxed);
        }
        BranchPointSourceKind::Frozen => {
            POINT_SELECTED_FROZEN.fetch_add(1, Ordering::Relaxed);
        }
        BranchPointSourceKind::OwnedL0 => {
            POINT_SELECTED_OWNED_L0.fetch_add(1, Ordering::Relaxed);
        }
        BranchPointSourceKind::OwnedNonzero => {
            POINT_SELECTED_OWNED_NONZERO.fetch_add(1, Ordering::Relaxed);
        }
        BranchPointSourceKind::Inherited => {
            POINT_SELECTED_INHERITED.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[cfg(not(feature = "perf-trace"))]
#[allow(dead_code)]
pub(crate) fn record_branch_point_early_exit(_source: BranchPointSourceKind) {}

#[cfg(feature = "perf-trace")]
#[allow(dead_code)]
pub(crate) fn record_branch_point_early_exit(source: BranchPointSourceKind) {
    if !recording_enabled() {
        return;
    }
    match source {
        BranchPointSourceKind::Active => {
            POINT_EARLY_EXIT_ACTIVE.fetch_add(1, Ordering::Relaxed);
        }
        BranchPointSourceKind::Frozen => {
            POINT_EARLY_EXIT_FROZEN.fetch_add(1, Ordering::Relaxed);
        }
        BranchPointSourceKind::OwnedL0 => {
            POINT_EARLY_EXIT_OWNED_L0.fetch_add(1, Ordering::Relaxed);
        }
        BranchPointSourceKind::OwnedNonzero => {
            POINT_EARLY_EXIT_OWNED_NONZERO.fetch_add(1, Ordering::Relaxed);
        }
        BranchPointSourceKind::Inherited => {
            POINT_EARLY_EXIT_INHERITED.fetch_add(1, Ordering::Relaxed);
        }
    }
}

#[cfg(not(feature = "perf-trace"))]
#[allow(dead_code)]
pub(crate) fn record_branch_point_remaining_source_skips(_sources: usize) {}

#[cfg(feature = "perf-trace")]
#[allow(dead_code)]
pub(crate) fn record_branch_point_remaining_source_skips(sources: usize) {
    if !recording_enabled() {
        return;
    }
    POINT_REMAINING_SOURCE_SKIPS.fetch_add(as_u64(sources), Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_branch_point_inherited_key_rewrite() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_branch_point_inherited_key_rewrite() {
    if !recording_enabled() {
        return;
    }
    POINT_INHERITED_KEY_REWRITES.fetch_add(1, Ordering::Relaxed);
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
pub(crate) fn record_lazy_full_materialization() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_lazy_full_materialization() {
    if !recording_enabled() {
        return;
    }
    TABLE_LAZY_FULL_MATERIALIZATIONS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_durable_commit_admitted_over_budget() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_durable_commit_admitted_over_budget() {
    if !recording_enabled() {
        return;
    }
    DURABLE_COMMIT_ADMITTED_OVER_BUDGET.fetch_add(1, Ordering::Relaxed);
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
pub(crate) fn record_table_lazy_point_block_scan(
    _entries_scanned: usize,
    _rows_decoded: usize,
    _full_block_rows_avoided: usize,
) {
}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_lazy_point_block_scan(
    entries_scanned: usize,
    rows_decoded: usize,
    full_block_rows_avoided: usize,
) {
    if !recording_enabled() {
        return;
    }
    TABLE_LAZY_POINT_BLOCK_SCANS.fetch_add(1, Ordering::Relaxed);
    TABLE_LAZY_POINT_ENTRIES_SCANNED.fetch_add(as_u64(entries_scanned), Ordering::Relaxed);
    TABLE_LAZY_POINT_ROWS_DECODED.fetch_add(as_u64(rows_decoded), Ordering::Relaxed);
    TABLE_LAZY_POINT_FULL_BLOCK_DECODES_AVOIDED
        .fetch_add(as_u64(full_block_rows_avoided), Ordering::Relaxed);
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
#[allow(dead_code)]
pub(crate) fn record_table_eager_filter_negative_probe() {}

#[cfg(feature = "perf-trace")]
#[allow(dead_code)]
pub(crate) fn record_table_eager_filter_negative_probe() {
    if !recording_enabled() {
        return;
    }
    TABLE_EAGER_FILTER_PROBES.fetch_add(1, Ordering::Relaxed);
    TABLE_EAGER_FILTER_NEGATIVE_PROBES.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
#[allow(dead_code)]
pub(crate) fn record_table_eager_filter_positive_probe() {}

#[cfg(feature = "perf-trace")]
#[allow(dead_code)]
pub(crate) fn record_table_eager_filter_positive_probe() {
    if !recording_enabled() {
        return;
    }
    TABLE_EAGER_FILTER_PROBES.fetch_add(1, Ordering::Relaxed);
    TABLE_EAGER_FILTER_POSITIVE_PROBES.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_eager_filter_unavailable_probe() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_eager_filter_unavailable_probe() {
    if !recording_enabled() {
        return;
    }
    TABLE_EAGER_FILTER_PROBES.fetch_add(1, Ordering::Relaxed);
    TABLE_EAGER_FILTER_UNAVAILABLE_PROBES.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
pub(crate) fn record_table_point_lookup_key_build() {}

#[cfg(feature = "perf-trace")]
pub(crate) fn record_table_point_lookup_key_build() {
    if !recording_enabled() {
        return;
    }
    TABLE_POINT_LOOKUP_KEY_BUILDS.fetch_add(1, Ordering::Relaxed);
}

#[cfg(not(feature = "perf-trace"))]
#[allow(dead_code)]
pub(crate) fn record_table_point_lookup_key_reuse() {}

#[cfg(feature = "perf-trace")]
#[allow(dead_code)]
pub(crate) fn record_table_point_lookup_key_reuse() {
    if !recording_enabled() {
        return;
    }
    TABLE_POINT_LOOKUP_KEY_REUSES.fetch_add(1, Ordering::Relaxed);
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
    test_capture_enabled_for_current_thread()
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
fn record_atomic_max(counter: &AtomicU64, value: u64) {
    let mut current = counter.load(Ordering::Relaxed);
    while value > current {
        match counter.compare_exchange_weak(current, value, Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(observed) => current = observed,
        }
    }
}

#[cfg(feature = "perf-trace")]
fn record_elapsed(counter: &AtomicU64, start: PerfTraceTimer) {
    if !recording_enabled() {
        return;
    }
    let nanos = start.elapsed().as_nanos();
    counter.fetch_add(u64::try_from(nanos).unwrap_or(u64::MAX), Ordering::Relaxed);
}
