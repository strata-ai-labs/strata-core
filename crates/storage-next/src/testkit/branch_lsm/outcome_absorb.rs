impl BranchLsmScaffoldOutcome {
    fn absorb_state_outcome(&mut self, outcome: StateOutcome) {
        self.state_construction += outcome.state_construction;
        self.committed_put_appends += outcome.committed_put_appends;
        self.committed_tombstone_appends += outcome.committed_tombstone_appends;
        self.wrong_branch_append_rejections += outcome.wrong_branch_append_rejections;
        self.active_duplicate_rejections += outcome.active_duplicate_rejections;
        self.frozen_duplicate_rejections += outcome.frozen_duplicate_rejections;
        self.same_key_version_appends += outcome.same_key_version_appends;
        self.same_version_key_appends += outcome.same_version_key_appends;
        self.active_rotations += outcome.active_rotations;
        self.empty_rotation_skips += outcome.empty_rotation_skips;
        self.frozen_limit_skips += outcome.frozen_limit_skips;
        self.active_only_facts += outcome.active_only_facts;
        self.frozen_only_facts += outcome.frozen_only_facts;
        self.mixed_active_frozen_facts += outcome.mixed_active_frozen_facts;
        self.timestamp_edge_facts += outcome.timestamp_edge_facts;
        self.max_commit_edge_facts += outcome.max_commit_edge_facts;
    }

    fn absorb_read_outcome(&mut self, outcome: ReadOutcome) {
        self.read_view_captures += outcome.read_view_captures;
        self.pinned_append_isolations += outcome.pinned_append_isolations;
        self.pinned_rotation_isolations += outcome.pinned_rotation_isolations;
        self.latest_point_reads += outcome.latest_point_reads;
        self.version_bounded_point_reads += outcome.version_bounded_point_reads;
        self.tombstone_shadow_reads += outcome.tombstone_shadow_reads;
        self.history_reads += outcome.history_reads;
        self.history_tombstones += outcome.history_tombstones;
        self.history_limits += outcome.history_limits;
        self.prefix_scans += outcome.prefix_scans;
        self.range_scans += outcome.range_scans;
        self.scan_tombstone_suppressions += outcome.scan_tombstone_suppressions;
        self.active_frozen_merge_reads += outcome.active_frozen_merge_reads;
        self.wrong_branch_read_rejections += outcome.wrong_branch_read_rejections;
    }

    fn absorb_immutable_outcome(&mut self, outcome: ImmutableOutcome) {
        self.immutable_descriptor_cases += outcome.immutable_descriptor_cases;
        self.immutable_l0_installs += outcome.immutable_l0_installs;
        self.immutable_l1_installs += outcome.immutable_l1_installs;
        self.invalid_immutable_install_rejections += outcome.invalid_immutable_install_rejections;
        self.immutable_l1_overlap_rejections += outcome.immutable_l1_overlap_rejections;
        self.frozen_replacements += outcome.frozen_replacements;
        self.pinned_immutable_install_isolations += outcome.pinned_immutable_install_isolations;
        self.immutable_latest_reads += outcome.immutable_latest_reads;
        self.immutable_version_bounded_reads += outcome.immutable_version_bounded_reads;
        self.immutable_history_reads += outcome.immutable_history_reads;
        self.immutable_prefix_scans += outcome.immutable_prefix_scans;
        self.immutable_range_scans += outcome.immutable_range_scans;
        self.immutable_tombstone_shadows += outcome.immutable_tombstone_shadows;
        self.active_frozen_immutable_merge_reads += outcome.active_frozen_immutable_merge_reads;
        self.immutable_source_attributions += outcome.immutable_source_attributions;
    }

    fn absorb_inheritance_outcome(&mut self, outcome: InheritanceOutcome) {
        self.inherited_fork_captures += outcome.inherited_fork_captures;
        self.inherited_layer_validations += outcome.inherited_layer_validations;
        self.inherited_latest_reads += outcome.inherited_latest_reads;
        self.inherited_version_bounded_reads += outcome.inherited_version_bounded_reads;
        self.inherited_history_reads += outcome.inherited_history_reads;
        self.inherited_prefix_scans += outcome.inherited_prefix_scans;
        self.inherited_range_scans += outcome.inherited_range_scans;
        self.inherited_key_rewrites += outcome.inherited_key_rewrites;
        self.inherited_child_put_shadows += outcome.inherited_child_put_shadows;
        self.inherited_child_tombstone_shadows += outcome.inherited_child_tombstone_shadows;
        self.inherited_post_fork_invisibility += outcome.inherited_post_fork_invisibility;
        self.inherited_chained_ancestry += outcome.inherited_chained_ancestry;
        self.invalid_inherited_layer_rejections += outcome.invalid_inherited_layer_rejections;
        self.pinned_inherited_view_isolations += outcome.pinned_inherited_view_isolations;
    }

    fn absorb_timestamp_outcome(&mut self, outcome: TimestampOutcome) {
        self.timestamp_point_reads += outcome.timestamp_point_reads;
        self.active_timestamp_point_reads += outcome.active_timestamp_point_reads;
        self.frozen_timestamp_point_reads += outcome.frozen_timestamp_point_reads;
        self.owned_timestamp_point_reads += outcome.owned_timestamp_point_reads;
        self.timestamp_scan_reads += outcome.timestamp_scan_reads;
        self.timestamp_prefix_scans += outcome.timestamp_prefix_scans;
        self.timestamp_range_scans += outcome.timestamp_range_scans;
        self.ttl_before_expiry_reads += outcome.ttl_before_expiry_reads;
        self.ttl_exact_expiry_suppressions += outcome.ttl_exact_expiry_suppressions;
        self.ttl_after_expiry_suppressions += outcome.ttl_after_expiry_suppressions;
        self.ttl_max_expiry_reads += outcome.ttl_max_expiry_reads;
        self.timestamp_tombstone_shadows += outcome.timestamp_tombstone_shadows;
        self.timestamp_tombstone_after_non_shadows += outcome.timestamp_tombstone_after_non_shadows;
        self.timestamp_scan_boundary_reads += outcome.timestamp_scan_boundary_reads;
        self.timestamp_scan_space_isolations += outcome.timestamp_scan_space_isolations;
        self.timestamp_empty_scans += outcome.timestamp_empty_scans;
        self.non_monotonic_timestamp_reads += outcome.non_monotonic_timestamp_reads;
        self.inherited_timestamp_reads += outcome.inherited_timestamp_reads;
        self.inherited_timestamp_point_reads += outcome.inherited_timestamp_point_reads;
        self.inherited_timestamp_scan_reads += outcome.inherited_timestamp_scan_reads;
        self.inherited_timestamp_fork_gates += outcome.inherited_timestamp_fork_gates;
        self.inherited_timestamp_child_put_shadows += outcome.inherited_timestamp_child_put_shadows;
        self.inherited_timestamp_child_tombstone_shadows +=
            outcome.inherited_timestamp_child_tombstone_shadows;
        self.inherited_timestamp_nearest_ties += outcome.inherited_timestamp_nearest_ties;
        self.pinned_timestamp_view_isolations += outcome.pinned_timestamp_view_isolations;
        self.unknown_timestamp_coverage_reads += outcome.unknown_timestamp_coverage_reads;
        self.insufficient_timestamp_history_rejections +=
            outcome.insufficient_timestamp_history_rejections;
    }

    fn absorb_materialization_outcome(&mut self, outcome: MaterializationOutcome) {
        self.materialization_attempts += outcome.materialization_attempts;
        self.successful_materializations += outcome.successful_materializations;
        self.empty_materializations += outcome.empty_materializations;
        self.idempotent_materialization_retries += outcome.idempotent_materialization_retries;
        self.materialized_rows += outcome.materialized_rows;
        self.materialized_tables += outcome.materialized_tables;
        self.skipped_materialization_post_fork_rows +=
            outcome.skipped_materialization_post_fork_rows;
        self.skipped_materialization_exact_duplicates +=
            outcome.skipped_materialization_exact_duplicates;
        self.materialization_latest_read_parity += outcome.materialization_latest_read_parity;
        self.materialization_version_read_parity += outcome.materialization_version_read_parity;
        self.materialization_timestamp_read_parity += outcome.materialization_timestamp_read_parity;
        self.materialization_history_read_parity += outcome.materialization_history_read_parity;
        self.materialization_prefix_scan_parity += outcome.materialization_prefix_scan_parity;
        self.materialization_range_scan_parity += outcome.materialization_range_scan_parity;
        self.materialization_pinned_view_isolations +=
            outcome.materialization_pinned_view_isolations;
        self.materialization_tombstone_preservations +=
            outcome.materialization_tombstone_preservations;
        self.materialization_ttl_preservations += outcome.materialization_ttl_preservations;
        self.invalid_materialization_rejections += outcome.invalid_materialization_rejections;
    }

    fn absorb_reachability_outcome(&mut self, outcome: ReachabilityOutcome) {
        self.reachability_snapshots += outcome.reachability_snapshots;
        self.reachability_owned_refs += outcome.reachability_owned_refs;
        self.reachability_inherited_refs += outcome.reachability_inherited_refs;
        self.materializing_reachability_refs += outcome.materializing_reachability_refs;
        self.reachability_aggregate_rebuilds += outcome.reachability_aggregate_rebuilds;
        self.shared_table_detections += outcome.shared_table_detections;
        self.reachability_release_candidates += outcome.reachability_release_candidates;
        self.protected_release_attempts += outcome.protected_release_attempts;
        self.registry_rebuilds += outcome.registry_rebuilds;
        self.registry_unregisters += outcome.registry_unregisters;
        self.registry_disagreements += outcome.registry_disagreements;
        self.fork_reachability_cases += outcome.fork_reachability_cases;
        self.failed_fork_reachability_rollbacks += outcome.failed_fork_reachability_rollbacks;
        self.materialization_release_cases += outcome.materialization_release_cases;
        self.branch_clear_release_cases += outcome.branch_clear_release_cases;
        self.reachability_deterministic_orderings += outcome.reachability_deterministic_orderings;
        self.invalid_reachability_rejections += outcome.invalid_reachability_rejections;
    }

    fn absorb_compaction_outcome(&mut self, outcome: CompactionOutcome) {
        self.compaction_noop_cases += outcome.compaction_noop_cases;
        self.l0_compaction_candidate_cases += outcome.l0_compaction_candidate_cases;
        self.l0_to_l1_compaction_candidate_cases += outcome.l0_to_l1_compaction_candidate_cases;
        self.nonzero_level_compaction_candidate_cases +=
            outcome.nonzero_level_compaction_candidate_cases;
        self.keep_all_compaction_cases += outcome.keep_all_compaction_cases;
        self.compaction_output_install_cases += outcome.compaction_output_install_cases;
        self.compaction_output_split_cases += outcome.compaction_output_split_cases;
        self.stale_candidate_rejection_cases += outcome.stale_candidate_rejection_cases;
        self.unsafe_old_version_pruning_rejection_cases +=
            outcome.unsafe_old_version_pruning_rejection_cases;
        self.unsafe_tombstone_pruning_rejection_cases +=
            outcome.unsafe_tombstone_pruning_rejection_cases;
        self.unsafe_ttl_pruning_rejection_cases += outcome.unsafe_ttl_pruning_rejection_cases;
        self.compaction_latest_parity_cases += outcome.compaction_latest_parity_cases;
        self.compaction_version_parity_cases += outcome.compaction_version_parity_cases;
        self.compaction_timestamp_parity_cases += outcome.compaction_timestamp_parity_cases;
        self.compaction_history_parity_cases += outcome.compaction_history_parity_cases;
        self.compaction_prefix_scan_parity_cases += outcome.compaction_prefix_scan_parity_cases;
        self.compaction_range_scan_parity_cases += outcome.compaction_range_scan_parity_cases;
        self.compaction_pinned_view_isolation_cases +=
            outcome.compaction_pinned_view_isolation_cases;
        self.compaction_release_candidate_cases += outcome.compaction_release_candidate_cases;
        self.compaction_protected_release_cases += outcome.compaction_protected_release_cases;
        self.invalid_compaction_request_rejection_cases +=
            outcome.invalid_compaction_request_rejection_cases;
    }

    fn absorb_snapshot_install_outcome(&mut self, outcome: SnapshotInstallOutcome) {
        self.snapshot_empty_install_noop_cases += outcome.snapshot_empty_install_noop_cases;
        self.snapshot_single_branch_install_cases += outcome.snapshot_single_branch_install_cases;
        self.snapshot_multi_branch_install_cases += outcome.snapshot_multi_branch_install_cases;
        self.snapshot_missing_branch_rejection_cases +=
            outcome.snapshot_missing_branch_rejection_cases;
        self.snapshot_missing_branch_create_cases += outcome.snapshot_missing_branch_create_cases;
        self.snapshot_non_empty_target_rejection_cases +=
            outcome.snapshot_non_empty_target_rejection_cases;
        self.snapshot_empty_group_rejection_cases += outcome.snapshot_empty_group_rejection_cases;
        self.snapshot_duplicate_branch_group_rejection_cases +=
            outcome.snapshot_duplicate_branch_group_rejection_cases;
        self.snapshot_duplicate_row_rejection_cases +=
            outcome.snapshot_duplicate_row_rejection_cases;
        self.snapshot_unsorted_row_rejection_cases += outcome.snapshot_unsorted_row_rejection_cases;
        self.snapshot_branch_mismatch_rejection_cases +=
            outcome.snapshot_branch_mismatch_rejection_cases;
        self.snapshot_output_identity_collision_rejection_cases +=
            outcome.snapshot_output_identity_collision_rejection_cases;
        self.snapshot_table_build_failure_atomicity_cases +=
            outcome.snapshot_table_build_failure_atomicity_cases;
        self.snapshot_latest_parity_cases += outcome.snapshot_latest_parity_cases;
        self.snapshot_version_parity_cases += outcome.snapshot_version_parity_cases;
        self.snapshot_timestamp_parity_cases += outcome.snapshot_timestamp_parity_cases;
        self.snapshot_history_parity_cases += outcome.snapshot_history_parity_cases;
        self.snapshot_prefix_scan_parity_cases += outcome.snapshot_prefix_scan_parity_cases;
        self.snapshot_range_scan_parity_cases += outcome.snapshot_range_scan_parity_cases;
        self.snapshot_tombstone_preservation_cases += outcome.snapshot_tombstone_preservation_cases;
        self.snapshot_ttl_preservation_cases += outcome.snapshot_ttl_preservation_cases;
        self.snapshot_pinned_view_isolation_cases += outcome.snapshot_pinned_view_isolation_cases;
        self.snapshot_reachability_cases += outcome.snapshot_reachability_cases;
        self.snapshot_source_boundary_guard_cases += outcome.snapshot_source_boundary_guard_cases;
    }
}
