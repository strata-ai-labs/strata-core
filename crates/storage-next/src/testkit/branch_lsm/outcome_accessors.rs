impl BranchLsmScaffoldOutcome {
    pub const fn valid_config_cases(self) -> usize {
        self.valid_config
    }

    pub const fn invalid_config_cases(self) -> usize {
        self.invalid_config
    }

    pub const fn read_bound_cases(self) -> usize {
        self.read_bounds
    }

    pub const fn valid_fact_cases(self) -> usize {
        self.valid_facts
    }

    pub const fn invalid_fact_cases(self) -> usize {
        self.invalid_facts
    }

    pub const fn descriptor_cases(self) -> usize {
        self.descriptors
    }

    pub const fn error_source_cases(self) -> usize {
        self.error_sources
    }

    pub const fn matching_row_cases(self) -> usize {
        self.matching_rows
    }

    pub const fn mismatching_row_cases(self) -> usize {
        self.mismatching_rows
    }

    pub const fn physical_key_rewrite_cases(self) -> usize {
        self.physical_key_rewrites
    }

    pub const fn row_rewrite_cases(self) -> usize {
        self.row_rewrites
    }

    pub const fn own_bound_cases(self) -> usize {
        self.own_bounds
    }

    pub const fn inherited_bound_cases(self) -> usize {
        self.inherited_bounds
    }

    pub const fn candidate_put_cases(self) -> usize {
        self.candidate_puts
    }

    pub const fn candidate_tombstone_cases(self) -> usize {
        self.candidate_tombstones
    }

    pub const fn edge_row_cases(self) -> usize {
        self.edge_rows
    }

    pub const fn encoded_grouping_cases(self) -> usize {
        self.encoded_grouping
    }

    pub const fn row_chain_cases(self) -> usize {
        self.row_chains
    }

    pub const fn fork_edge_cases(self) -> usize {
        self.fork_edges
    }

    pub const fn state_construction_cases(self) -> usize {
        self.state_construction
    }

    pub const fn committed_put_append_cases(self) -> usize {
        self.committed_put_appends
    }

    pub const fn committed_tombstone_append_cases(self) -> usize {
        self.committed_tombstone_appends
    }

    pub const fn wrong_branch_append_rejection_cases(self) -> usize {
        self.wrong_branch_append_rejections
    }

    pub const fn active_duplicate_rejection_cases(self) -> usize {
        self.active_duplicate_rejections
    }

    pub const fn cross_layer_duplicate_append_cases(self) -> usize {
        self.cross_layer_duplicate_appends
    }

    pub const fn same_key_version_append_cases(self) -> usize {
        self.same_key_version_appends
    }

    pub const fn same_version_key_append_cases(self) -> usize {
        self.same_version_key_appends
    }

    pub const fn active_rotation_cases(self) -> usize {
        self.active_rotations
    }

    pub const fn empty_rotation_skip_cases(self) -> usize {
        self.empty_rotation_skips
    }

    pub const fn frozen_limit_skip_cases(self) -> usize {
        self.frozen_limit_skips
    }

    pub const fn active_only_fact_cases(self) -> usize {
        self.active_only_facts
    }

    pub const fn frozen_only_fact_cases(self) -> usize {
        self.frozen_only_facts
    }

    pub const fn mixed_active_frozen_fact_cases(self) -> usize {
        self.mixed_active_frozen_facts
    }

    pub const fn timestamp_edge_fact_cases(self) -> usize {
        self.timestamp_edge_facts
    }

    pub const fn max_commit_edge_fact_cases(self) -> usize {
        self.max_commit_edge_facts
    }

    pub const fn read_view_capture_cases(self) -> usize {
        self.read_view_captures
    }

    pub const fn pinned_append_isolation_cases(self) -> usize {
        self.pinned_append_isolations
    }

    pub const fn pinned_rotation_isolation_cases(self) -> usize {
        self.pinned_rotation_isolations
    }

    pub const fn latest_point_read_cases(self) -> usize {
        self.latest_point_reads
    }

    pub const fn version_bounded_point_read_cases(self) -> usize {
        self.version_bounded_point_reads
    }

    pub const fn tombstone_shadow_read_cases(self) -> usize {
        self.tombstone_shadow_reads
    }

    pub const fn history_read_cases(self) -> usize {
        self.history_reads
    }

    pub const fn history_tombstone_cases(self) -> usize {
        self.history_tombstones
    }

    pub const fn history_limit_cases(self) -> usize {
        self.history_limits
    }

    pub const fn prefix_scan_cases(self) -> usize {
        self.prefix_scans
    }

    pub const fn range_scan_cases(self) -> usize {
        self.range_scans
    }

    pub const fn scan_tombstone_suppression_cases(self) -> usize {
        self.scan_tombstone_suppressions
    }

    pub const fn active_frozen_merge_read_cases(self) -> usize {
        self.active_frozen_merge_reads
    }

    pub const fn wrong_branch_read_rejection_cases(self) -> usize {
        self.wrong_branch_read_rejections
    }

    pub const fn timestamp_point_read_cases(self) -> usize {
        self.timestamp_point_reads
    }

    pub const fn active_timestamp_point_read_cases(self) -> usize {
        self.active_timestamp_point_reads
    }

    pub const fn frozen_timestamp_point_read_cases(self) -> usize {
        self.frozen_timestamp_point_reads
    }

    pub const fn owned_timestamp_point_read_cases(self) -> usize {
        self.owned_timestamp_point_reads
    }

    pub const fn timestamp_scan_read_cases(self) -> usize {
        self.timestamp_scan_reads
    }

    pub const fn timestamp_prefix_scan_cases(self) -> usize {
        self.timestamp_prefix_scans
    }

    pub const fn timestamp_range_scan_cases(self) -> usize {
        self.timestamp_range_scans
    }

    pub const fn ttl_before_expiry_read_cases(self) -> usize {
        self.ttl_before_expiry_reads
    }

    pub const fn ttl_exact_expiry_suppression_cases(self) -> usize {
        self.ttl_exact_expiry_suppressions
    }

    pub const fn ttl_after_expiry_suppression_cases(self) -> usize {
        self.ttl_after_expiry_suppressions
    }

    pub const fn ttl_max_expiry_read_cases(self) -> usize {
        self.ttl_max_expiry_reads
    }

    pub const fn timestamp_tombstone_shadow_cases(self) -> usize {
        self.timestamp_tombstone_shadows
    }

    pub const fn timestamp_tombstone_after_non_shadow_cases(self) -> usize {
        self.timestamp_tombstone_after_non_shadows
    }

    pub const fn timestamp_scan_boundary_cases(self) -> usize {
        self.timestamp_scan_boundary_reads
    }

    pub const fn timestamp_scan_space_isolation_cases(self) -> usize {
        self.timestamp_scan_space_isolations
    }

    pub const fn timestamp_empty_scan_cases(self) -> usize {
        self.timestamp_empty_scans
    }

    pub const fn non_monotonic_timestamp_read_cases(self) -> usize {
        self.non_monotonic_timestamp_reads
    }

    pub const fn inherited_timestamp_read_cases(self) -> usize {
        self.inherited_timestamp_reads
    }

    pub const fn inherited_timestamp_point_read_cases(self) -> usize {
        self.inherited_timestamp_point_reads
    }

    pub const fn inherited_timestamp_scan_read_cases(self) -> usize {
        self.inherited_timestamp_scan_reads
    }

    pub const fn inherited_timestamp_fork_gate_cases(self) -> usize {
        self.inherited_timestamp_fork_gates
    }

    pub const fn inherited_timestamp_child_put_shadow_cases(self) -> usize {
        self.inherited_timestamp_child_put_shadows
    }

    pub const fn inherited_timestamp_child_tombstone_shadow_cases(self) -> usize {
        self.inherited_timestamp_child_tombstone_shadows
    }

    pub const fn inherited_timestamp_nearest_tie_cases(self) -> usize {
        self.inherited_timestamp_nearest_ties
    }

    pub const fn pinned_timestamp_view_isolation_cases(self) -> usize {
        self.pinned_timestamp_view_isolations
    }

    pub const fn unknown_timestamp_coverage_read_cases(self) -> usize {
        self.unknown_timestamp_coverage_reads
    }

    pub const fn insufficient_timestamp_history_rejection_cases(self) -> usize {
        self.insufficient_timestamp_history_rejections
    }

    pub const fn immutable_descriptor_cases(self) -> usize {
        self.immutable_descriptor_cases
    }

    pub const fn immutable_l0_install_cases(self) -> usize {
        self.immutable_l0_installs
    }

    pub const fn immutable_l1_install_cases(self) -> usize {
        self.immutable_l1_installs
    }

    pub const fn invalid_immutable_install_rejection_cases(self) -> usize {
        self.invalid_immutable_install_rejections
    }

    pub const fn immutable_l1_overlap_rejection_cases(self) -> usize {
        self.immutable_l1_overlap_rejections
    }

    pub const fn frozen_replacement_cases(self) -> usize {
        self.frozen_replacements
    }

    pub const fn pinned_immutable_install_isolation_cases(self) -> usize {
        self.pinned_immutable_install_isolations
    }

    pub const fn immutable_latest_read_cases(self) -> usize {
        self.immutable_latest_reads
    }

    pub const fn immutable_version_bounded_read_cases(self) -> usize {
        self.immutable_version_bounded_reads
    }

    pub const fn immutable_history_cases(self) -> usize {
        self.immutable_history_reads
    }

    pub const fn immutable_prefix_scan_cases(self) -> usize {
        self.immutable_prefix_scans
    }

    pub const fn immutable_range_scan_cases(self) -> usize {
        self.immutable_range_scans
    }

    pub const fn immutable_tombstone_shadow_cases(self) -> usize {
        self.immutable_tombstone_shadows
    }

    pub const fn active_frozen_immutable_merge_read_cases(self) -> usize {
        self.active_frozen_immutable_merge_reads
    }

    pub const fn immutable_source_attribution_cases(self) -> usize {
        self.immutable_source_attributions
    }

    pub const fn inherited_fork_capture_cases(self) -> usize {
        self.inherited_fork_captures
    }

    pub const fn inherited_layer_validation_cases(self) -> usize {
        self.inherited_layer_validations
    }

    pub const fn inherited_latest_read_cases(self) -> usize {
        self.inherited_latest_reads
    }

    pub const fn inherited_version_bounded_read_cases(self) -> usize {
        self.inherited_version_bounded_reads
    }

    pub const fn inherited_history_read_cases(self) -> usize {
        self.inherited_history_reads
    }

    pub const fn inherited_prefix_scan_cases(self) -> usize {
        self.inherited_prefix_scans
    }

    pub const fn inherited_range_scan_cases(self) -> usize {
        self.inherited_range_scans
    }

    pub const fn inherited_key_rewrite_cases(self) -> usize {
        self.inherited_key_rewrites
    }

    pub const fn inherited_child_put_shadow_cases(self) -> usize {
        self.inherited_child_put_shadows
    }

    pub const fn inherited_child_tombstone_shadow_cases(self) -> usize {
        self.inherited_child_tombstone_shadows
    }

    pub const fn inherited_post_fork_invisibility_cases(self) -> usize {
        self.inherited_post_fork_invisibility
    }

    pub const fn inherited_chained_ancestry_cases(self) -> usize {
        self.inherited_chained_ancestry
    }

    pub const fn invalid_inherited_layer_rejection_cases(self) -> usize {
        self.invalid_inherited_layer_rejections
    }

    pub const fn pinned_inherited_view_isolation_cases(self) -> usize {
        self.pinned_inherited_view_isolations
    }

    pub const fn inheritance_model_cases(self) -> usize {
        self.inheritance_model_cases
    }

    pub const fn materialization_attempt_cases(self) -> usize {
        self.materialization_attempts
    }

    pub const fn successful_materialization_cases(self) -> usize {
        self.successful_materializations
    }

    pub const fn empty_materialization_cases(self) -> usize {
        self.empty_materializations
    }

    pub const fn idempotent_materialization_retry_cases(self) -> usize {
        self.idempotent_materialization_retries
    }

    pub const fn materialized_row_cases(self) -> usize {
        self.materialized_rows
    }

    pub const fn materialized_table_cases(self) -> usize {
        self.materialized_tables
    }

    pub const fn skipped_materialization_post_fork_row_cases(self) -> usize {
        self.skipped_materialization_post_fork_rows
    }

    pub const fn skipped_materialization_exact_duplicate_cases(self) -> usize {
        self.skipped_materialization_exact_duplicates
    }

    pub const fn materialization_latest_read_parity_cases(self) -> usize {
        self.materialization_latest_read_parity
    }

    pub const fn materialization_version_read_parity_cases(self) -> usize {
        self.materialization_version_read_parity
    }

    pub const fn materialization_timestamp_read_parity_cases(self) -> usize {
        self.materialization_timestamp_read_parity
    }

    pub const fn materialization_history_read_parity_cases(self) -> usize {
        self.materialization_history_read_parity
    }

    pub const fn materialization_prefix_scan_parity_cases(self) -> usize {
        self.materialization_prefix_scan_parity
    }

    pub const fn materialization_range_scan_parity_cases(self) -> usize {
        self.materialization_range_scan_parity
    }

    pub const fn materialization_pinned_view_isolation_cases(self) -> usize {
        self.materialization_pinned_view_isolations
    }

    pub const fn materialization_tombstone_preservation_cases(self) -> usize {
        self.materialization_tombstone_preservations
    }

    pub const fn materialization_ttl_preservation_cases(self) -> usize {
        self.materialization_ttl_preservations
    }

    pub const fn invalid_materialization_rejection_cases(self) -> usize {
        self.invalid_materialization_rejections
    }

    pub const fn reachability_snapshot_cases(self) -> usize {
        self.reachability_snapshots
    }

    pub const fn reachability_owned_ref_cases(self) -> usize {
        self.reachability_owned_refs
    }

    pub const fn reachability_inherited_ref_cases(self) -> usize {
        self.reachability_inherited_refs
    }

    pub const fn materializing_reachability_ref_cases(self) -> usize {
        self.materializing_reachability_refs
    }

    pub const fn reachability_aggregate_rebuild_cases(self) -> usize {
        self.reachability_aggregate_rebuilds
    }

    pub const fn shared_table_detection_cases(self) -> usize {
        self.shared_table_detections
    }

    pub const fn reachability_release_candidate_cases(self) -> usize {
        self.reachability_release_candidates
    }

    pub const fn protected_release_attempt_cases(self) -> usize {
        self.protected_release_attempts
    }

    pub const fn registry_rebuild_cases(self) -> usize {
        self.registry_rebuilds
    }

    pub const fn registry_unregister_cases(self) -> usize {
        self.registry_unregisters
    }

    pub const fn registry_disagreement_cases(self) -> usize {
        self.registry_disagreements
    }

    pub const fn fork_reachability_cases(self) -> usize {
        self.fork_reachability_cases
    }

    pub const fn failed_fork_reachability_rollback_cases(self) -> usize {
        self.failed_fork_reachability_rollbacks
    }

    pub const fn materialization_release_cases(self) -> usize {
        self.materialization_release_cases
    }

    pub const fn branch_clear_release_cases(self) -> usize {
        self.branch_clear_release_cases
    }

    pub const fn reachability_deterministic_ordering_cases(self) -> usize {
        self.reachability_deterministic_orderings
    }

    pub const fn invalid_reachability_rejection_cases(self) -> usize {
        self.invalid_reachability_rejections
    }

    pub const fn compaction_noop_cases(self) -> usize {
        self.compaction_noop_cases
    }

    pub const fn l0_compaction_candidate_cases(self) -> usize {
        self.l0_compaction_candidate_cases
    }

    pub const fn l0_to_l1_compaction_candidate_cases(self) -> usize {
        self.l0_to_l1_compaction_candidate_cases
    }

    pub const fn nonzero_level_compaction_candidate_cases(self) -> usize {
        self.nonzero_level_compaction_candidate_cases
    }

    pub const fn keep_all_compaction_cases(self) -> usize {
        self.keep_all_compaction_cases
    }

    pub const fn compaction_output_install_cases(self) -> usize {
        self.compaction_output_install_cases
    }

    pub const fn compaction_output_split_cases(self) -> usize {
        self.compaction_output_split_cases
    }

    pub const fn stale_candidate_rejection_cases(self) -> usize {
        self.stale_candidate_rejection_cases
    }

    pub const fn unsafe_old_version_pruning_rejection_cases(self) -> usize {
        self.unsafe_old_version_pruning_rejection_cases
    }

    pub const fn unsafe_tombstone_pruning_rejection_cases(self) -> usize {
        self.unsafe_tombstone_pruning_rejection_cases
    }

    pub const fn unsafe_ttl_pruning_rejection_cases(self) -> usize {
        self.unsafe_ttl_pruning_rejection_cases
    }

    pub const fn compaction_latest_parity_cases(self) -> usize {
        self.compaction_latest_parity_cases
    }

    pub const fn compaction_version_parity_cases(self) -> usize {
        self.compaction_version_parity_cases
    }

    pub const fn compaction_timestamp_parity_cases(self) -> usize {
        self.compaction_timestamp_parity_cases
    }

    pub const fn compaction_history_parity_cases(self) -> usize {
        self.compaction_history_parity_cases
    }

    pub const fn compaction_prefix_scan_parity_cases(self) -> usize {
        self.compaction_prefix_scan_parity_cases
    }

    pub const fn compaction_range_scan_parity_cases(self) -> usize {
        self.compaction_range_scan_parity_cases
    }

    pub const fn compaction_pinned_view_isolation_cases(self) -> usize {
        self.compaction_pinned_view_isolation_cases
    }

    pub const fn compaction_release_candidate_cases(self) -> usize {
        self.compaction_release_candidate_cases
    }

    pub const fn compaction_protected_release_cases(self) -> usize {
        self.compaction_protected_release_cases
    }

    pub const fn invalid_compaction_request_rejection_cases(self) -> usize {
        self.invalid_compaction_request_rejection_cases
    }

    pub const fn snapshot_empty_install_noop_cases(self) -> usize {
        self.snapshot_empty_install_noop_cases
    }

    pub const fn snapshot_single_branch_install_cases(self) -> usize {
        self.snapshot_single_branch_install_cases
    }

    pub const fn snapshot_multi_branch_install_cases(self) -> usize {
        self.snapshot_multi_branch_install_cases
    }

    pub const fn snapshot_missing_branch_rejection_cases(self) -> usize {
        self.snapshot_missing_branch_rejection_cases
    }

    pub const fn snapshot_missing_branch_create_cases(self) -> usize {
        self.snapshot_missing_branch_create_cases
    }

    pub const fn snapshot_non_empty_target_rejection_cases(self) -> usize {
        self.snapshot_non_empty_target_rejection_cases
    }

    pub const fn snapshot_empty_group_rejection_cases(self) -> usize {
        self.snapshot_empty_group_rejection_cases
    }

    pub const fn snapshot_duplicate_branch_group_rejection_cases(self) -> usize {
        self.snapshot_duplicate_branch_group_rejection_cases
    }

    pub const fn snapshot_duplicate_row_rejection_cases(self) -> usize {
        self.snapshot_duplicate_row_rejection_cases
    }

    pub const fn snapshot_unsorted_row_rejection_cases(self) -> usize {
        self.snapshot_unsorted_row_rejection_cases
    }

    pub const fn snapshot_branch_mismatch_rejection_cases(self) -> usize {
        self.snapshot_branch_mismatch_rejection_cases
    }

    pub const fn snapshot_output_identity_collision_rejection_cases(self) -> usize {
        self.snapshot_output_identity_collision_rejection_cases
    }

    pub const fn snapshot_table_build_failure_atomicity_cases(self) -> usize {
        self.snapshot_table_build_failure_atomicity_cases
    }

    pub const fn snapshot_latest_parity_cases(self) -> usize {
        self.snapshot_latest_parity_cases
    }

    pub const fn snapshot_version_parity_cases(self) -> usize {
        self.snapshot_version_parity_cases
    }

    pub const fn snapshot_timestamp_parity_cases(self) -> usize {
        self.snapshot_timestamp_parity_cases
    }

    pub const fn snapshot_history_parity_cases(self) -> usize {
        self.snapshot_history_parity_cases
    }

    pub const fn snapshot_prefix_scan_parity_cases(self) -> usize {
        self.snapshot_prefix_scan_parity_cases
    }

    pub const fn snapshot_range_scan_parity_cases(self) -> usize {
        self.snapshot_range_scan_parity_cases
    }

    pub const fn snapshot_tombstone_preservation_cases(self) -> usize {
        self.snapshot_tombstone_preservation_cases
    }

    pub const fn snapshot_ttl_preservation_cases(self) -> usize {
        self.snapshot_ttl_preservation_cases
    }

    pub const fn snapshot_pinned_view_isolation_cases(self) -> usize {
        self.snapshot_pinned_view_isolation_cases
    }

    pub const fn snapshot_reachability_cases(self) -> usize {
        self.snapshot_reachability_cases
    }

    pub const fn snapshot_source_boundary_guard_cases(self) -> usize {
        self.snapshot_source_boundary_guard_cases
    }

    pub const fn install_model_cases(self) -> usize {
        self.install_model_cases
    }
}
