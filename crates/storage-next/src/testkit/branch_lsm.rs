//! Generated branch-LSM scaffold contract helpers.

use super::TestkitError;
use crate::branch::{
    install_snapshot_rows_into_branches, require_row_branch, rewrite_physical_key_branch,
    rewrite_row_branch, row_matches_branch, BranchCompactionKind, BranchCompactionNoopReason,
    BranchCompactionRecovery, BranchCompactionRequest, BranchCompactionRetentionPolicy,
    BranchEffectiveReadBound, BranchForkOutcome, BranchHistoryOptions, BranchHistoryRow,
    BranchImmutableInstallOutcome, BranchInheritedLayer, BranchLevel, BranchLocalState,
    BranchMaterializationOutcome, BranchMaterializationRecovery, BranchMaterializationRequest,
    BranchOwnedTable, BranchProtectionReason, BranchReachabilityAggregate, BranchReachabilityFacts,
    BranchReachabilitySnapshot, BranchReadBound, BranchReadView, BranchReleasePlan,
    BranchRotationOutcome, BranchRotationSkipReason, BranchRowCandidateFacts, BranchRowSource,
    BranchRuntimeConfig, BranchRuntimeError, BranchRuntimeStats, BranchScanBounds,
    BranchSnapshotInstallGroup, BranchSnapshotInstallRecovery, BranchSnapshotInstallRequest,
    BranchSnapshotMissingBranchPolicy, BranchStateDescriptor, BranchStateFacts,
    BranchTableDescriptor, BranchTableRef, BranchTableReferenceKind, BranchTimestampCoverage,
    BranchTimestampHistorySource, BranchUserKeyBound, BranchViewDescriptor, BranchVisibleRow,
    InheritedLayerDescriptor, InheritedLayerStatus, SharedTableRegistry,
};
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use crate::table::{
    sort_table_rows_by_key, ImmutableTableBuilder, ImmutableTableReader, TableBuilderConfig,
    TableCommitRange, TableCompactionConfig, TableIdentity, TableInternalKeyBytes, TableKeyRange,
    TablePhysicalKeyBytes, TableReaderConfig, TableRow, TableRuntimeFacts,
};
use std::error::Error;
use std::fmt;
use strata_core_next::{BranchId, CommitVersion, Timestamp};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BranchLsmScaffoldOutcome {
    valid_config: usize,
    invalid_config: usize,
    read_bounds: usize,
    valid_facts: usize,
    invalid_facts: usize,
    descriptors: usize,
    error_sources: usize,
    stats: usize,
    matching_rows: usize,
    mismatching_rows: usize,
    physical_key_rewrites: usize,
    row_rewrites: usize,
    own_bounds: usize,
    inherited_bounds: usize,
    candidate_puts: usize,
    candidate_tombstones: usize,
    edge_rows: usize,
    encoded_grouping: usize,
    row_chains: usize,
    fork_edges: usize,
    state_construction: usize,
    committed_put_appends: usize,
    committed_tombstone_appends: usize,
    wrong_branch_append_rejections: usize,
    active_duplicate_rejections: usize,
    frozen_duplicate_rejections: usize,
    same_key_version_appends: usize,
    same_version_key_appends: usize,
    active_rotations: usize,
    empty_rotation_skips: usize,
    frozen_limit_skips: usize,
    active_only_facts: usize,
    frozen_only_facts: usize,
    mixed_active_frozen_facts: usize,
    timestamp_edge_facts: usize,
    max_commit_edge_facts: usize,
    read_view_captures: usize,
    pinned_append_isolations: usize,
    pinned_rotation_isolations: usize,
    latest_point_reads: usize,
    version_bounded_point_reads: usize,
    tombstone_shadow_reads: usize,
    history_reads: usize,
    history_tombstones: usize,
    history_limits: usize,
    prefix_scans: usize,
    range_scans: usize,
    scan_tombstone_suppressions: usize,
    active_frozen_merge_reads: usize,
    wrong_branch_read_rejections: usize,
    timestamp_point_reads: usize,
    active_timestamp_point_reads: usize,
    frozen_timestamp_point_reads: usize,
    owned_timestamp_point_reads: usize,
    timestamp_scan_reads: usize,
    timestamp_prefix_scans: usize,
    timestamp_range_scans: usize,
    ttl_before_expiry_reads: usize,
    ttl_exact_expiry_suppressions: usize,
    ttl_after_expiry_suppressions: usize,
    ttl_max_expiry_reads: usize,
    timestamp_tombstone_shadows: usize,
    timestamp_tombstone_after_non_shadows: usize,
    timestamp_scan_boundary_reads: usize,
    timestamp_scan_space_isolations: usize,
    timestamp_empty_scans: usize,
    non_monotonic_timestamp_reads: usize,
    inherited_timestamp_reads: usize,
    inherited_timestamp_point_reads: usize,
    inherited_timestamp_scan_reads: usize,
    inherited_timestamp_fork_gates: usize,
    inherited_timestamp_child_put_shadows: usize,
    inherited_timestamp_child_tombstone_shadows: usize,
    inherited_timestamp_nearest_ties: usize,
    pinned_timestamp_view_isolations: usize,
    unknown_timestamp_coverage_reads: usize,
    insufficient_timestamp_history_rejections: usize,
    immutable_descriptor_cases: usize,
    immutable_l0_installs: usize,
    immutable_l1_installs: usize,
    invalid_immutable_install_rejections: usize,
    immutable_l1_overlap_rejections: usize,
    frozen_replacements: usize,
    pinned_immutable_install_isolations: usize,
    immutable_latest_reads: usize,
    immutable_version_bounded_reads: usize,
    immutable_history_reads: usize,
    immutable_prefix_scans: usize,
    immutable_range_scans: usize,
    immutable_tombstone_shadows: usize,
    active_frozen_immutable_merge_reads: usize,
    immutable_source_attributions: usize,
    inherited_fork_captures: usize,
    inherited_layer_validations: usize,
    inherited_latest_reads: usize,
    inherited_version_bounded_reads: usize,
    inherited_history_reads: usize,
    inherited_prefix_scans: usize,
    inherited_range_scans: usize,
    inherited_key_rewrites: usize,
    inherited_child_put_shadows: usize,
    inherited_child_tombstone_shadows: usize,
    inherited_post_fork_invisibility: usize,
    inherited_chained_ancestry: usize,
    invalid_inherited_layer_rejections: usize,
    pinned_inherited_view_isolations: usize,
    materialization_attempts: usize,
    successful_materializations: usize,
    empty_materializations: usize,
    idempotent_materialization_retries: usize,
    materialized_rows: usize,
    materialized_tables: usize,
    skipped_materialization_post_fork_rows: usize,
    skipped_materialization_exact_duplicates: usize,
    materialization_latest_read_parity: usize,
    materialization_version_read_parity: usize,
    materialization_timestamp_read_parity: usize,
    materialization_history_read_parity: usize,
    materialization_prefix_scan_parity: usize,
    materialization_range_scan_parity: usize,
    materialization_pinned_view_isolations: usize,
    materialization_tombstone_preservations: usize,
    materialization_ttl_preservations: usize,
    invalid_materialization_rejections: usize,
    reachability_snapshots: usize,
    reachability_owned_refs: usize,
    reachability_inherited_refs: usize,
    materializing_reachability_refs: usize,
    reachability_aggregate_rebuilds: usize,
    shared_table_detections: usize,
    reachability_release_candidates: usize,
    protected_release_attempts: usize,
    registry_rebuilds: usize,
    registry_unregisters: usize,
    registry_disagreements: usize,
    fork_reachability_cases: usize,
    failed_fork_reachability_rollbacks: usize,
    materialization_release_cases: usize,
    branch_clear_release_cases: usize,
    reachability_deterministic_orderings: usize,
    invalid_reachability_rejections: usize,
    compaction_noop_cases: usize,
    l0_compaction_candidate_cases: usize,
    l0_to_l1_compaction_candidate_cases: usize,
    nonzero_level_compaction_candidate_cases: usize,
    keep_all_compaction_cases: usize,
    compaction_output_install_cases: usize,
    compaction_output_split_cases: usize,
    stale_candidate_rejection_cases: usize,
    unsafe_old_version_pruning_rejection_cases: usize,
    unsafe_tombstone_pruning_rejection_cases: usize,
    unsafe_ttl_pruning_rejection_cases: usize,
    compaction_latest_parity_cases: usize,
    compaction_version_parity_cases: usize,
    compaction_timestamp_parity_cases: usize,
    compaction_history_parity_cases: usize,
    compaction_prefix_scan_parity_cases: usize,
    compaction_range_scan_parity_cases: usize,
    compaction_pinned_view_isolation_cases: usize,
    compaction_release_candidate_cases: usize,
    compaction_protected_release_cases: usize,
    invalid_compaction_request_rejection_cases: usize,
    snapshot_empty_install_noop_cases: usize,
    snapshot_single_branch_install_cases: usize,
    snapshot_multi_branch_install_cases: usize,
    snapshot_missing_branch_rejection_cases: usize,
    snapshot_missing_branch_create_cases: usize,
    snapshot_non_empty_target_rejection_cases: usize,
    snapshot_empty_group_rejection_cases: usize,
    snapshot_duplicate_branch_group_rejection_cases: usize,
    snapshot_duplicate_row_rejection_cases: usize,
    snapshot_unsorted_row_rejection_cases: usize,
    snapshot_branch_mismatch_rejection_cases: usize,
    snapshot_output_identity_collision_rejection_cases: usize,
    snapshot_table_build_failure_atomicity_cases: usize,
    snapshot_latest_parity_cases: usize,
    snapshot_version_parity_cases: usize,
    snapshot_timestamp_parity_cases: usize,
    snapshot_history_parity_cases: usize,
    snapshot_prefix_scan_parity_cases: usize,
    snapshot_range_scan_parity_cases: usize,
    snapshot_tombstone_preservation_cases: usize,
    snapshot_ttl_preservation_cases: usize,
    snapshot_pinned_view_isolation_cases: usize,
    snapshot_reachability_cases: usize,
    snapshot_source_boundary_guard_cases: usize,
}

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

    pub const fn stats_cases(self) -> usize {
        self.stats
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

    pub const fn frozen_duplicate_rejection_cases(self) -> usize {
        self.frozen_duplicate_rejections
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

pub fn check_branch_lsm_scaffold_contract(
    script: &[u8],
) -> Result<BranchLsmScaffoldOutcome, TestkitError> {
    let mut outcome = BranchLsmScaffoldOutcome::default();

    check_valid_config(script)?;
    outcome.valid_config += 1;
    outcome.invalid_config += check_invalid_configs()?;

    check_read_bounds(script)?;
    outcome.read_bounds += 1;

    check_valid_facts(script)?;
    outcome.valid_facts += 1;
    outcome.invalid_facts += check_invalid_facts(script)?;

    check_descriptors(script)?;
    outcome.descriptors += 1;

    check_error_sources()?;
    outcome.error_sources += 1;

    check_stats(script)?;
    outcome.stats += 1;

    let identity_outcome = check_row_identity_and_rewrites(script)?;
    outcome.matching_rows += identity_outcome.matching_rows;
    outcome.mismatching_rows += identity_outcome.mismatching_rows;
    outcome.physical_key_rewrites += identity_outcome.physical_key_rewrites;
    outcome.row_rewrites += identity_outcome.row_rewrites;

    let bounds_outcome = check_effective_bounds_and_candidates(script)?;
    outcome.own_bounds += bounds_outcome.own_bounds;
    outcome.inherited_bounds += bounds_outcome.inherited_bounds;
    outcome.candidate_puts += bounds_outcome.candidate_puts;
    outcome.candidate_tombstones += bounds_outcome.candidate_tombstones;

    let edge_outcome = check_edge_rows_and_encoded_grouping(script)?;
    outcome.edge_rows += edge_outcome.edge_rows;
    outcome.encoded_grouping += edge_outcome.encoded_grouping;

    let chain_outcome = check_row_chains_and_fork_edges(script)?;
    outcome.row_chains += chain_outcome.row_chains;
    outcome.fork_edges += chain_outcome.fork_edges;

    outcome.absorb_state_outcome(check_branch_local_state(script)?);
    outcome.absorb_read_outcome(check_branch_read_view(script)?);
    outcome.absorb_immutable_outcome(check_branch_owned_immutable(script)?);
    outcome.absorb_inheritance_outcome(check_branch_inheritance(script)?);
    outcome.absorb_timestamp_outcome(check_branch_timestamp_visibility(script)?);
    outcome.absorb_materialization_outcome(check_branch_materialization(script)?);
    outcome.absorb_reachability_outcome(check_branch_reachability(script)?);
    outcome.absorb_compaction_outcome(check_branch_compaction(script)?);
    outcome.absorb_snapshot_install_outcome(check_branch_snapshot_install(script)?);

    Ok(outcome)
}

/// Runs the branch-read-specific fuzz contract over generated branch states.
pub fn check_branch_lsm_reads_contract(script: &[u8]) -> Result<(), TestkitError> {
    check_branch_lsm_reference_model_contract(script)?;
    let _ = check_row_chains_and_fork_edges(script)?;
    let _ = check_branch_read_view(script)?;
    let _ = check_branch_timestamp_visibility(script)?;
    let _ = check_branch_owned_immutable(script)?;
    Ok(())
}

/// Runs the branch-inheritance-specific fuzz contract over generated forks.
pub fn check_branch_lsm_inheritance_contract(script: &[u8]) -> Result<(), TestkitError> {
    check_branch_lsm_fault_window_contract(script)?;
    let _ = check_branch_inheritance(script)?;
    let _ = check_branch_timestamp_visibility(script)?;
    let _ = check_branch_materialization(script)?;
    let _ = check_branch_reachability(script)?;
    Ok(())
}

/// Runs the branch-install-specific fuzz contract over generated install plans.
pub fn check_branch_lsm_install_contract(script: &[u8]) -> Result<(), TestkitError> {
    check_branch_lsm_fault_window_contract(script)?;
    let _ = check_branch_owned_immutable(script)?;
    let _ = check_branch_compaction(script)?;
    let _ = check_branch_snapshot_install(script)?;
    let _ = check_branch_reachability(script)?;
    Ok(())
}

/// Replays generated own-branch operations against production and an
/// independent row-list model.
pub fn check_branch_lsm_reference_model_contract(script: &[u8]) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 194));
    let mut state = BranchLocalState::new(
        branch,
        BranchRuntimeConfig::new(7, 64, 64)
            .map_err(|err| TestkitError::new(format!("model config failed: {err}")))?,
    )
    .map_err(|err| TestkitError::new(format!("model state failed: {err}")))?;
    let mut model = ModelBranch::new(branch);
    let mut next_version = 1_u64;

    for step in 0..32 {
        let opcode = script_byte(script, 195 + step);
        match opcode % 5 {
            0 => {
                let row = model_put_row(branch, opcode, next_version, step)?;
                append_model_row(&mut state, &mut model, row)?;
                next_version = next_version.saturating_add(1);
            }
            1 => {
                let row = model_tombstone_row(branch, opcode, next_version, step)?;
                append_model_row(&mut state, &mut model, row)?;
                next_version = next_version.saturating_add(1);
            }
            2 => {
                let _ = state.rotate_active();
            }
            3 => {
                let first = model_put_row(branch, opcode, next_version, step)?;
                let second = model_put_row(branch, opcode.wrapping_add(1), next_version + 1, step)?;
                install_model_l0_rows(
                    &mut state,
                    &mut model,
                    &format!("model-l0-{step}"),
                    vec![first, second],
                )?;
                next_version = next_version.saturating_add(2);
            }
            _ => {
                let row = model_expiring_row(branch, opcode, next_version, step)?;
                append_model_row(&mut state, &mut model, row)?;
                next_version = next_version.saturating_add(1);
            }
        }
        assert_model_matches_state(script, step, &model, &state)?;
    }
    Ok(())
}

/// Exercises L6 state-transition fault windows that do not require an L1 backend.
pub fn check_branch_lsm_fault_window_contract(script: &[u8]) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 228));
    let other = branch_id(script_byte(script, 228).wrapping_add(1));

    let mut duplicate_state = BranchLocalState::empty(branch);
    let duplicate = storage_row_with(
        branch,
        b"fault-duplicate".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        vec![script_byte(script, 229)],
    )?;
    duplicate_state
        .append_committed_row(duplicate.clone())
        .map_err(|err| TestkitError::new(format!("fault duplicate seed failed: {err}")))?;
    let before_duplicate = duplicate_state.clone();
    expect_duplicate_internal_key(duplicate_state.append_committed_row(duplicate))?;
    if duplicate_state != before_duplicate {
        return Err(TestkitError::new("duplicate append fault mutated state"));
    }

    let mut install_state = BranchLocalState::empty(branch);
    let wrong_table = branch_owned_table(
        other,
        BranchLevel::ZERO,
        "fault-wrong-branch-install",
        vec![storage_row_with(
            other,
            b"fault-wrong-branch".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            vec![script_byte(script, 230)],
        )?],
    )?;
    let before_install = install_state.clone();
    expect_invalid_branch_row(install_state.install_l0_table(wrong_table))?;
    if install_state != before_install {
        return Err(TestkitError::new(
            "wrong-branch install fault mutated state",
        ));
    }

    check_materialization_fault_window(script, branch, other)?;
    check_snapshot_fault_window(script, branch, other)?;
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct IdentityOutcome {
    matching_rows: usize,
    mismatching_rows: usize,
    physical_key_rewrites: usize,
    row_rewrites: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct BoundsOutcome {
    own_bounds: usize,
    inherited_bounds: usize,
    candidate_puts: usize,
    candidate_tombstones: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct EdgeOutcome {
    edge_rows: usize,
    encoded_grouping: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ChainOutcome {
    row_chains: usize,
    fork_edges: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct StateOutcome {
    state_construction: usize,
    committed_put_appends: usize,
    committed_tombstone_appends: usize,
    wrong_branch_append_rejections: usize,
    active_duplicate_rejections: usize,
    frozen_duplicate_rejections: usize,
    same_key_version_appends: usize,
    same_version_key_appends: usize,
    active_rotations: usize,
    empty_rotation_skips: usize,
    frozen_limit_skips: usize,
    active_only_facts: usize,
    frozen_only_facts: usize,
    mixed_active_frozen_facts: usize,
    timestamp_edge_facts: usize,
    max_commit_edge_facts: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ReadOutcome {
    read_view_captures: usize,
    pinned_append_isolations: usize,
    pinned_rotation_isolations: usize,
    latest_point_reads: usize,
    version_bounded_point_reads: usize,
    tombstone_shadow_reads: usize,
    history_reads: usize,
    history_tombstones: usize,
    history_limits: usize,
    prefix_scans: usize,
    range_scans: usize,
    scan_tombstone_suppressions: usize,
    active_frozen_merge_reads: usize,
    wrong_branch_read_rejections: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ImmutableOutcome {
    immutable_descriptor_cases: usize,
    immutable_l0_installs: usize,
    immutable_l1_installs: usize,
    invalid_immutable_install_rejections: usize,
    immutable_l1_overlap_rejections: usize,
    frozen_replacements: usize,
    pinned_immutable_install_isolations: usize,
    immutable_latest_reads: usize,
    immutable_version_bounded_reads: usize,
    immutable_history_reads: usize,
    immutable_prefix_scans: usize,
    immutable_range_scans: usize,
    immutable_tombstone_shadows: usize,
    active_frozen_immutable_merge_reads: usize,
    immutable_source_attributions: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct InheritanceOutcome {
    inherited_fork_captures: usize,
    inherited_layer_validations: usize,
    inherited_latest_reads: usize,
    inherited_version_bounded_reads: usize,
    inherited_history_reads: usize,
    inherited_prefix_scans: usize,
    inherited_range_scans: usize,
    inherited_key_rewrites: usize,
    inherited_child_put_shadows: usize,
    inherited_child_tombstone_shadows: usize,
    inherited_post_fork_invisibility: usize,
    inherited_chained_ancestry: usize,
    invalid_inherited_layer_rejections: usize,
    pinned_inherited_view_isolations: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct TimestampOutcome {
    timestamp_point_reads: usize,
    active_timestamp_point_reads: usize,
    frozen_timestamp_point_reads: usize,
    owned_timestamp_point_reads: usize,
    timestamp_scan_reads: usize,
    timestamp_prefix_scans: usize,
    timestamp_range_scans: usize,
    ttl_before_expiry_reads: usize,
    ttl_exact_expiry_suppressions: usize,
    ttl_after_expiry_suppressions: usize,
    ttl_max_expiry_reads: usize,
    timestamp_tombstone_shadows: usize,
    timestamp_tombstone_after_non_shadows: usize,
    timestamp_scan_boundary_reads: usize,
    timestamp_scan_space_isolations: usize,
    timestamp_empty_scans: usize,
    non_monotonic_timestamp_reads: usize,
    inherited_timestamp_reads: usize,
    inherited_timestamp_point_reads: usize,
    inherited_timestamp_scan_reads: usize,
    inherited_timestamp_fork_gates: usize,
    inherited_timestamp_child_put_shadows: usize,
    inherited_timestamp_child_tombstone_shadows: usize,
    inherited_timestamp_nearest_ties: usize,
    pinned_timestamp_view_isolations: usize,
    unknown_timestamp_coverage_reads: usize,
    insufficient_timestamp_history_rejections: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct MaterializationOutcome {
    materialization_attempts: usize,
    successful_materializations: usize,
    empty_materializations: usize,
    idempotent_materialization_retries: usize,
    materialized_rows: usize,
    materialized_tables: usize,
    skipped_materialization_post_fork_rows: usize,
    skipped_materialization_exact_duplicates: usize,
    materialization_latest_read_parity: usize,
    materialization_version_read_parity: usize,
    materialization_timestamp_read_parity: usize,
    materialization_history_read_parity: usize,
    materialization_prefix_scan_parity: usize,
    materialization_range_scan_parity: usize,
    materialization_pinned_view_isolations: usize,
    materialization_tombstone_preservations: usize,
    materialization_ttl_preservations: usize,
    invalid_materialization_rejections: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ReachabilityOutcome {
    reachability_snapshots: usize,
    reachability_owned_refs: usize,
    reachability_inherited_refs: usize,
    materializing_reachability_refs: usize,
    reachability_aggregate_rebuilds: usize,
    shared_table_detections: usize,
    reachability_release_candidates: usize,
    protected_release_attempts: usize,
    registry_rebuilds: usize,
    registry_unregisters: usize,
    registry_disagreements: usize,
    fork_reachability_cases: usize,
    failed_fork_reachability_rollbacks: usize,
    materialization_release_cases: usize,
    branch_clear_release_cases: usize,
    reachability_deterministic_orderings: usize,
    invalid_reachability_rejections: usize,
}

#[allow(clippy::struct_field_names)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct CompactionOutcome {
    compaction_noop_cases: usize,
    l0_compaction_candidate_cases: usize,
    l0_to_l1_compaction_candidate_cases: usize,
    nonzero_level_compaction_candidate_cases: usize,
    keep_all_compaction_cases: usize,
    compaction_output_install_cases: usize,
    compaction_output_split_cases: usize,
    stale_candidate_rejection_cases: usize,
    unsafe_old_version_pruning_rejection_cases: usize,
    unsafe_tombstone_pruning_rejection_cases: usize,
    unsafe_ttl_pruning_rejection_cases: usize,
    compaction_latest_parity_cases: usize,
    compaction_version_parity_cases: usize,
    compaction_timestamp_parity_cases: usize,
    compaction_history_parity_cases: usize,
    compaction_prefix_scan_parity_cases: usize,
    compaction_range_scan_parity_cases: usize,
    compaction_pinned_view_isolation_cases: usize,
    compaction_release_candidate_cases: usize,
    compaction_protected_release_cases: usize,
    invalid_compaction_request_rejection_cases: usize,
}

#[allow(clippy::struct_field_names)]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct SnapshotInstallOutcome {
    snapshot_empty_install_noop_cases: usize,
    snapshot_single_branch_install_cases: usize,
    snapshot_multi_branch_install_cases: usize,
    snapshot_missing_branch_rejection_cases: usize,
    snapshot_missing_branch_create_cases: usize,
    snapshot_non_empty_target_rejection_cases: usize,
    snapshot_empty_group_rejection_cases: usize,
    snapshot_duplicate_branch_group_rejection_cases: usize,
    snapshot_duplicate_row_rejection_cases: usize,
    snapshot_unsorted_row_rejection_cases: usize,
    snapshot_branch_mismatch_rejection_cases: usize,
    snapshot_output_identity_collision_rejection_cases: usize,
    snapshot_table_build_failure_atomicity_cases: usize,
    snapshot_latest_parity_cases: usize,
    snapshot_version_parity_cases: usize,
    snapshot_timestamp_parity_cases: usize,
    snapshot_history_parity_cases: usize,
    snapshot_prefix_scan_parity_cases: usize,
    snapshot_range_scan_parity_cases: usize,
    snapshot_tombstone_preservation_cases: usize,
    snapshot_ttl_preservation_cases: usize,
    snapshot_pinned_view_isolation_cases: usize,
    snapshot_reachability_cases: usize,
    snapshot_source_boundary_guard_cases: usize,
}

fn check_valid_config(script: &[u8]) -> Result<(), TestkitError> {
    let levels = 1 + usize::from(script_byte(script, 0) % 8);
    let inherited = 1 + usize::from(script_byte(script, 1) % 16);
    let frozen = 1 + usize::from(script_byte(script, 2) % 16);
    let config = BranchRuntimeConfig::new(levels, inherited, frozen)
        .map_err(|err| TestkitError::new(format!("valid branch config rejected: {err}")))?;
    if config.max_level_count() != levels
        || config.max_inherited_layers() != inherited
        || config.max_frozen_tables() != frozen
    {
        return Err(TestkitError::new("valid branch config facts drifted"));
    }
    Ok(())
}

fn check_invalid_configs() -> Result<usize, TestkitError> {
    expect_invalid_config(BranchRuntimeConfig::new(0, 1, 1))?;
    expect_invalid_config(BranchRuntimeConfig::new(1, 0, 1))?;
    expect_invalid_config(BranchRuntimeConfig::new(1, 1, 0))?;
    Ok(3)
}

fn check_read_bounds(script: &[u8]) -> Result<(), TestkitError> {
    let version = CommitVersion::new(u64::from(script_byte(script, 3)));
    let timestamp = Timestamp::from_micros(u64::from(script_byte(script, 4)));
    if BranchReadBound::latest() != BranchReadBound::Latest {
        return Err(TestkitError::new("latest read bound drifted"));
    }
    if BranchReadBound::at_version(version) != BranchReadBound::AtVersion(version) {
        return Err(TestkitError::new("version read bound drifted"));
    }
    if BranchReadBound::at_timestamp(timestamp) != BranchReadBound::AtTimestamp(timestamp) {
        return Err(TestkitError::new("timestamp read bound drifted"));
    }
    Ok(())
}

fn check_valid_facts(script: &[u8]) -> Result<(), TestkitError> {
    let branch_id = branch_id(script_byte(script, 5));
    let facts = BranchStateFacts::new(
        branch_id,
        1,
        usize::from(script_byte(script, 6) % 4),
        usize::from(script_byte(script, 7) % 4),
        usize::from(script_byte(script, 8) % 4),
        Some(CommitVersion::new(10)),
        Some(Timestamp::from_micros(1)),
        Some(Timestamp::from_micros(2)),
    )
    .map_err(|err| TestkitError::new(format!("valid branch facts rejected: {err}")))?;
    if facts.branch_id() != branch_id
        || facts.active_rows() != 1
        || facts.max_commit_version() != Some(CommitVersion::new(10))
        || facts.timestamp_min() != Some(Timestamp::from_micros(1))
        || facts.timestamp_max() != Some(Timestamp::from_micros(2))
    {
        return Err(TestkitError::new("valid branch facts drifted"));
    }

    let empty = BranchStateFacts::empty(branch_id);
    if empty.max_commit_version().is_some() || empty.timestamp_min().is_some() {
        return Err(TestkitError::new("empty branch facts drifted"));
    }
    Ok(())
}

fn check_invalid_facts(script: &[u8]) -> Result<usize, TestkitError> {
    let branch_id = branch_id(script_byte(script, 9));
    expect_invalid_state(BranchStateFacts::new(
        branch_id,
        0,
        0,
        0,
        0,
        Some(CommitVersion::new(1)),
        None,
        None,
    ))?;
    expect_invalid_state(BranchStateFacts::new(
        branch_id,
        0,
        0,
        0,
        0,
        None,
        Some(Timestamp::from_micros(1)),
        Some(Timestamp::from_micros(1)),
    ))?;
    expect_invalid_state(BranchStateFacts::new(
        branch_id,
        1,
        0,
        0,
        0,
        Some(CommitVersion::new(1)),
        Some(Timestamp::from_micros(2)),
        Some(Timestamp::from_micros(1)),
    ))?;
    expect_invalid_state(BranchStateFacts::new(
        branch_id,
        1,
        0,
        0,
        0,
        Some(CommitVersion::new(1)),
        Some(Timestamp::from_micros(1)),
        None,
    ))?;
    Ok(4)
}

fn check_descriptors(script: &[u8]) -> Result<(), TestkitError> {
    let test_branch_id = branch_id(script_byte(script, 10));
    let facts = BranchStateFacts::empty(test_branch_id);
    let state = BranchStateDescriptor::new(test_branch_id, facts)
        .map_err(|err| TestkitError::new(format!("state descriptor failed: {err}")))?;
    let view = BranchViewDescriptor::new(test_branch_id, facts)
        .map_err(|err| TestkitError::new(format!("view descriptor failed: {err}")))?;
    if state.branch_id() != test_branch_id || view.facts() != facts {
        return Err(TestkitError::new("branch state descriptors drifted"));
    }

    let table_facts = table_facts("branch-scaffold")?;
    let table = BranchTableDescriptor::new(
        TableIdentity::new("branch-scaffold")
            .map_err(|err| TestkitError::new(format!("table identity failed: {err}")))?,
        table_facts.clone(),
        BranchLevel::new(script_byte(script, 11) % 4),
    )
    .map_err(|err| TestkitError::new(format!("branch table descriptor failed: {err}")))?;
    if table.facts() != &table_facts || table.identity().as_str() != "branch-scaffold" {
        return Err(TestkitError::new("branch table descriptor drifted"));
    }

    let inherited = InheritedLayerDescriptor::new(
        branch_id(99),
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        2,
    );
    if inherited.source_branch_id() != branch_id(99)
        || inherited.fork_version() != CommitVersion::new(5)
        || inherited.table_count() != 2
    {
        return Err(TestkitError::new("inherited layer descriptor drifted"));
    }

    let reachability = BranchReachabilityFacts::new(test_branch_id, 1, 2);
    if reachability.owned_table_count() != 1 || reachability.inherited_table_count() != 2 {
        return Err(TestkitError::new("branch reachability facts drifted"));
    }

    let row = storage_row(test_branch_id, 7)?;
    let source = BranchRowSource::OwnedTable {
        level: BranchLevel::ZERO,
        table_index: 0,
    };
    let visible = BranchVisibleRow::new(row.clone(), source);
    let history = BranchHistoryRow::new(row, source);
    if visible.source() != source || history.source() != source {
        return Err(TestkitError::new("branch row result source drifted"));
    }
    Ok(())
}

fn check_error_sources() -> Result<(), TestkitError> {
    let table_error = BranchRuntimeError::TableRuntime {
        source: crate::table::TableRuntimeError::Cache {
            reason: "scaffold cache",
        },
    };
    if table_error.source().is_none() {
        return Err(TestkitError::new("branch table error source missing"));
    }

    let publish_error = BranchRuntimeError::publish_with("scaffold publish", LeafError);
    match publish_error.source() {
        Some(source) if source.to_string() == "leaf source" => {}
        _ => return Err(TestkitError::new("branch publish error source missing")),
    }

    if BranchRuntimeError::publish("scaffold publish")
        .to_string()
        .contains("secret-payload")
    {
        return Err(TestkitError::new("branch error leaked payload text"));
    }
    Ok(())
}

fn check_stats(script: &[u8]) -> Result<(), TestkitError> {
    let empty = BranchRuntimeStats::default();
    if empty.latest_reads() != 0
        || empty.bounded_reads() != 0
        || empty.history_reads() != 0
        || empty.inherited_layers_examined() != 0
    {
        return Err(TestkitError::new("default branch stats drifted"));
    }

    let stats = BranchRuntimeStats::new(
        u64::from(script_byte(script, 12)),
        u64::from(script_byte(script, 13)),
        u64::from(script_byte(script, 14)),
        u64::from(script_byte(script, 15)),
    );
    if stats.latest_reads() != u64::from(script_byte(script, 12))
        || stats.bounded_reads() != u64::from(script_byte(script, 13))
        || stats.history_reads() != u64::from(script_byte(script, 14))
        || stats.inherited_layers_examined() != u64::from(script_byte(script, 15))
    {
        return Err(TestkitError::new("branch stats drifted"));
    }
    Ok(())
}

fn check_row_identity_and_rewrites(script: &[u8]) -> Result<IdentityOutcome, TestkitError> {
    let source = branch_id(script_byte(script, 16));
    let target = branch_id(script_byte(script, 16).wrapping_add(1));
    let row = storage_row_with(
        source,
        user_key(script, 18),
        u64::from(script_byte(script, 22)),
        u64::from(script_byte(script, 23)),
        Timestamp::from_micros(u64::from(script_byte(script, 24))),
        vec![script_byte(script, 25), 0x00, script_byte(script, 26)],
    )?;
    let tombstone = tombstone_row(
        source,
        user_key(script, 27),
        u64::from(script_byte(script, 31)),
        u64::from(script_byte(script, 32)),
    )?;

    require_row_branch(source, &row)
        .map_err(|err| TestkitError::new(format!("matching row rejected: {err}")))?;
    if !row_matches_branch(source, &row) {
        return Err(TestkitError::new("matching row predicate returned false"));
    }
    if row_matches_branch(target, &row) {
        return Err(TestkitError::new("mismatching row predicate returned true"));
    }
    if !matches!(
        require_row_branch(target, &row),
        Err(BranchRuntimeError::InvalidBranchRow { .. })
    ) {
        return Err(TestkitError::new("mismatching row was not rejected"));
    }

    let rewritten_key = rewrite_physical_key_branch(row.physical_key(), target)
        .map_err(|err| TestkitError::new(format!("physical key rewrite failed: {err}")))?;
    if rewritten_key.branch_id() != target
        || rewritten_key.space() != row.physical_key().space()
        || rewritten_key.storage_space_id() != row.physical_key().storage_space_id()
        || rewritten_key.user_key() != row.physical_key().user_key()
    {
        return Err(TestkitError::new(
            "physical key rewrite changed non-branch facts",
        ));
    }

    let rewritten = rewrite_row_branch(&row, source, target)
        .map_err(|err| TestkitError::new(format!("row rewrite failed: {err}")))?;
    if rewritten.physical_key().branch_id() != target
        || rewritten.commit_version() != row.commit_version()
        || rewritten.commit_timestamp() != row.commit_timestamp()
        || rewritten.expires_at() != row.expires_at()
        || rewritten.value() != row.value()
        || rewritten.is_tombstone()
    {
        return Err(TestkitError::new("put row rewrite changed row facts"));
    }
    let rewritten_tombstone = rewrite_row_branch(&tombstone, source, target)
        .map_err(|err| TestkitError::new(format!("tombstone rewrite failed: {err}")))?;
    if !rewritten_tombstone.is_tombstone()
        || !rewritten_tombstone.value().is_empty()
        || rewritten_tombstone.commit_version() != tombstone.commit_version()
    {
        return Err(TestkitError::new("tombstone rewrite changed row shape"));
    }
    if !matches!(
        rewrite_row_branch(&row, target, source),
        Err(BranchRuntimeError::InvalidBranchRow { .. })
    ) {
        return Err(TestkitError::new("row rewrite skipped source preflight"));
    }

    Ok(IdentityOutcome {
        matching_rows: 1,
        mismatching_rows: 1,
        physical_key_rewrites: 1,
        row_rewrites: 2,
    })
}

fn check_effective_bounds_and_candidates(script: &[u8]) -> Result<BoundsOutcome, TestkitError> {
    let branch = branch_id(script_byte(script, 33));
    let version = CommitVersion::new(1 + u64::from(script_byte(script, 34)));
    let timestamp = Timestamp::from_micros(u64::from(script_byte(script, 35)));
    let row = storage_row_with(
        branch,
        user_key(script, 36),
        version.as_u64(),
        timestamp.as_micros(),
        Timestamp::from_micros(timestamp.as_micros().saturating_sub(1)),
        vec![script_byte(script, 40)],
    )?;
    let tombstone = tombstone_row(
        branch,
        user_key(script, 41),
        version.as_u64(),
        timestamp.as_micros(),
    )?;

    let own_latest = BranchEffectiveReadBound::for_own_branch(BranchReadBound::latest());
    if own_latest.max_commit_version().is_some()
        || own_latest.max_commit_timestamp().is_some()
        || !own_latest.matches_row(&row).matches_effective_bound()
    {
        return Err(TestkitError::new("own latest bound drifted"));
    }
    let own_version =
        BranchEffectiveReadBound::for_own_branch(BranchReadBound::at_version(version));
    if !own_version.matches_row(&row).matches_effective_bound() {
        return Err(TestkitError::new("own version bound is not inclusive"));
    }
    let own_timestamp =
        BranchEffectiveReadBound::for_own_branch(BranchReadBound::at_timestamp(timestamp));
    if !own_timestamp.matches_row(&row).matches_effective_bound() {
        return Err(TestkitError::new("own timestamp bound is not inclusive"));
    }

    let fork_version = CommitVersion::new(version.as_u64().saturating_sub(1));
    let inherited_latest =
        BranchEffectiveReadBound::for_inherited_layer(BranchReadBound::latest(), fork_version);
    if inherited_latest.max_commit_version() != Some(fork_version) {
        return Err(TestkitError::new("inherited latest lost fork cap"));
    }
    let inherited_version = BranchEffectiveReadBound::for_inherited_layer(
        BranchReadBound::at_version(CommitVersion::MAX),
        fork_version,
    );
    if inherited_version.max_commit_version() != Some(fork_version) {
        return Err(TestkitError::new("inherited version did not cap at fork"));
    }
    let inherited_timestamp = BranchEffectiveReadBound::for_inherited_layer(
        BranchReadBound::at_timestamp(timestamp),
        fork_version,
    );
    let inherited_match = inherited_timestamp.matches_row(&row);
    if inherited_timestamp.max_commit_version() != Some(fork_version)
        || inherited_timestamp.max_commit_timestamp() != Some(timestamp)
        || inherited_match.matches_effective_bound()
        || !inherited_match.timestamp_in_bound()
    {
        return Err(TestkitError::new(
            "inherited timestamp bound did not combine timestamp and fork caps",
        ));
    }

    let put_candidate =
        BranchRowCandidateFacts::from_row(&row, BranchRowSource::Active, own_timestamp);
    if put_candidate.is_tombstone()
        || put_candidate.expires_at() != row.expires_at()
        || !put_candidate.bound_match().matches_effective_bound()
    {
        return Err(TestkitError::new("put candidate facts drifted"));
    }
    let tombstone_candidate = BranchRowCandidateFacts::from_row(
        &tombstone,
        BranchRowSource::Frozen { index: 0 },
        own_timestamp,
    );
    if !tombstone_candidate.is_tombstone()
        || tombstone_candidate.source() != (BranchRowSource::Frozen { index: 0 })
        || !tombstone_candidate.bound_match().matches_effective_bound()
    {
        return Err(TestkitError::new("tombstone candidate facts drifted"));
    }

    Ok(BoundsOutcome {
        own_bounds: 3,
        inherited_bounds: 3,
        candidate_puts: 1,
        candidate_tombstones: 1,
    })
}

fn check_edge_rows_and_encoded_grouping(script: &[u8]) -> Result<EdgeOutcome, TestkitError> {
    let source = branch_id(script_byte(script, 44));
    let target = branch_id(script_byte(script, 44).wrapping_add(1));
    let storage_owned = StorageRow::put(
        physical_key_with_space(
            source,
            "system",
            StorageSpaceId::COMMIT_TIMELINE,
            Vec::new(),
        )?,
        CommitVersion::MAX,
        Timestamp::MAX,
        Timestamp::MAX,
        Vec::new(),
    );
    let rewritten_storage_owned = rewrite_row_branch(&storage_owned, source, target)
        .map_err(|err| TestkitError::new(format!("storage-owned row rewrite failed: {err}")))?;
    if rewritten_storage_owned.physical_key().branch_id() != target
        || rewritten_storage_owned.physical_key().space() != "system"
        || rewritten_storage_owned.physical_key().storage_space_id()
            != StorageSpaceId::COMMIT_TIMELINE
        || !rewritten_storage_owned.physical_key().user_key().is_empty()
        || rewritten_storage_owned.commit_version() != CommitVersion::MAX
        || rewritten_storage_owned.commit_timestamp() != Timestamp::MAX
        || rewritten_storage_owned.expires_at() != Timestamp::MAX
        || !rewritten_storage_owned.value().is_empty()
    {
        return Err(TestkitError::new(
            "storage-owned empty-key row rewrite changed edge facts",
        ));
    }

    let shared_key = vec![script_byte(script, 45), 0x00, script_byte(script, 46)];
    let storage_space = StorageSpaceId::engine(0x21)
        .map_err(|err| TestkitError::new(format!("storage space failed: {err}")))?;
    let inherited = storage_row_with_space(
        source,
        "default",
        storage_space,
        shared_key.clone(),
        7,
        70,
        Timestamp::EPOCH,
        vec![script_byte(script, 47)],
    )?;
    let child_local = storage_row_with_space(
        target,
        "default",
        storage_space,
        shared_key,
        5,
        50,
        Timestamp::EPOCH,
        vec![script_byte(script, 48)],
    )?;
    let rewritten = rewrite_row_branch(&inherited, source, target)
        .map_err(|err| TestkitError::new(format!("inherited row rewrite failed: {err}")))?;

    let rewritten_prefix = TablePhysicalKeyBytes::from_row(&rewritten);
    let child_prefix = TablePhysicalKeyBytes::from_row(&child_local);
    if rewritten_prefix.as_slice() != child_prefix.as_slice() {
        return Err(TestkitError::new(
            "rewritten inherited row did not group with child-local physical key",
        ));
    }

    let mut rows = vec![TableRow::new(child_local), TableRow::new(rewritten)];
    sort_table_rows_by_key(&mut rows);
    if row_versions(&rows) != vec![7, 5] {
        return Err(TestkitError::new(
            "rewritten inherited row did not sort as newest version in child group",
        ));
    }

    Ok(EdgeOutcome {
        edge_rows: 1,
        encoded_grouping: 1,
    })
}

fn check_row_chains_and_fork_edges(script: &[u8]) -> Result<ChainOutcome, TestkitError> {
    let branch = branch_id(script_byte(script, 49));
    let wrong_branch = branch_id(script_byte(script, 49).wrapping_add(1));
    let key = vec![script_byte(script, 50), 0x00, script_byte(script, 51)];
    let mut rows = vec![
        TableRow::new(storage_row_with(
            branch,
            key.clone(),
            3,
            30,
            Timestamp::from_micros(25),
            vec![script_byte(script, 52)],
        )?),
        TableRow::new(storage_row_with(
            branch,
            key.clone(),
            5,
            50,
            Timestamp::EPOCH,
            vec![script_byte(script, 53)],
        )?),
        TableRow::new(tombstone_row(branch, key.clone(), 4, 40)?),
        TableRow::new(storage_row_with(
            branch,
            key,
            2,
            60,
            Timestamp::EPOCH,
            vec![script_byte(script, 54)],
        )?),
    ];
    sort_table_rows_by_key(&mut rows);
    if row_versions(&rows) != vec![5, 4, 3, 2] {
        return Err(TestkitError::new(
            "row chain did not preserve descending version order",
        ));
    }

    let version_bound = BranchEffectiveReadBound::new(Some(CommitVersion::new(4)), None);
    if matching_versions(&rows, version_bound) != vec![4, 3, 2] {
        return Err(TestkitError::new(
            "version bound did not filter row chain inclusively",
        ));
    }
    let timestamp_bound = BranchEffectiveReadBound::new(None, Some(Timestamp::from_micros(40)));
    if matching_versions(&rows, timestamp_bound) != vec![4, 3] {
        return Err(TestkitError::new(
            "timestamp bound did not filter row chain inclusively",
        ));
    }
    let combined_bound = BranchEffectiveReadBound::new(
        Some(CommitVersion::new(4)),
        Some(Timestamp::from_micros(40)),
    );
    let combined = matching_versions(&rows, combined_bound);
    if combined != vec![4, 3] {
        return Err(TestkitError::new(
            "combined row-chain bound did not intersect version and timestamp caps",
        ));
    }

    let candidates = rows
        .iter()
        .map(|row| {
            BranchRowCandidateFacts::from_row(row.row(), BranchRowSource::Active, combined_bound)
        })
        .filter(|candidate| candidate.bound_match().matches_effective_bound())
        .collect::<Vec<_>>();
    if candidates.len() != 2
        || !candidates.iter().any(BranchRowCandidateFacts::is_tombstone)
        || !candidates
            .iter()
            .any(|candidate| candidate.expires_at() == Timestamp::from_micros(25))
    {
        return Err(TestkitError::new(
            "row-chain candidates collapsed tombstone or expiry facts",
        ));
    }

    let wrong_row = storage_row(wrong_branch, 4)?;
    if !matches!(
        require_row_branch(branch, &wrong_row),
        Err(BranchRuntimeError::InvalidBranchRow { .. })
    ) {
        return Err(TestkitError::new(
            "row-chain branch preflight accepted a wrong-branch row",
        ));
    }

    check_fork_edge_bounds()?;
    Ok(ChainOutcome {
        row_chains: 1,
        fork_edges: 4,
    })
}

fn check_fork_edge_bounds() -> Result<(), TestkitError> {
    let fork_version = CommitVersion::new(4);
    let before_fork = BranchEffectiveReadBound::for_inherited_layer(
        BranchReadBound::at_version(CommitVersion::new(3)),
        fork_version,
    );
    let at_fork = BranchEffectiveReadBound::for_inherited_layer(
        BranchReadBound::at_version(CommitVersion::new(4)),
        fork_version,
    );
    let after_fork = BranchEffectiveReadBound::for_inherited_layer(
        BranchReadBound::at_version(CommitVersion::new(5)),
        fork_version,
    );
    let latest =
        BranchEffectiveReadBound::for_inherited_layer(BranchReadBound::latest(), fork_version);
    if before_fork.max_commit_version() != Some(CommitVersion::new(3))
        || at_fork.max_commit_version() != Some(fork_version)
        || after_fork.max_commit_version() != Some(fork_version)
        || latest.max_commit_version() != Some(fork_version)
    {
        return Err(TestkitError::new(
            "inherited fork edge bounds did not cap requested versions correctly",
        ));
    }
    Ok(())
}

fn check_branch_local_state(script: &[u8]) -> Result<StateOutcome, TestkitError> {
    let mut outcome = StateOutcome::default();
    let branch = branch_id(script_byte(script, 55));
    let config = BranchRuntimeConfig::new(7, 64, 2)
        .map_err(|err| TestkitError::new(format!("state config failed: {err}")))?;
    let mut state = BranchLocalState::new(branch, config)
        .map_err(|err| TestkitError::new(format!("state construction failed: {err}")))?;
    if state.branch_id() != branch
        || state.config() != config
        || !state.is_empty()
        || branch_state_facts(&state)? != BranchStateFacts::empty(branch)
    {
        return Err(TestkitError::new("empty branch-local state drifted"));
    }
    outcome.state_construction += 1;

    outcome.empty_rotation_skips += check_empty_rotation(&mut state)?;
    let put = check_branch_local_append_path(script, branch, &mut state, &mut outcome)?;
    check_branch_local_rotation_path(script, branch, &mut state, &mut outcome, &put)?;
    check_branch_local_edge_facts(branch, &mut outcome)?;
    outcome.frozen_limit_skips += check_frozen_limit_skip(branch)?;
    Ok(outcome)
}

fn check_branch_read_view(script: &[u8]) -> Result<ReadOutcome, TestkitError> {
    let mut outcome = ReadOutcome::default();
    let branch = branch_id(script_byte(script, 79));
    let mut state = BranchLocalState::empty(branch);
    let seed = seed_read_view_state(script, branch, &mut state)?;

    check_read_view_point_reads(&state, branch, &seed, &mut outcome)?;
    check_read_view_history(&state, &seed, &mut outcome)?;
    check_read_view_scans_and_pinning(script, branch, &mut state, &mut outcome)?;
    check_read_view_rejections(script, &seed.read_key, &state, &mut outcome)?;

    Ok(outcome)
}

struct ReadSeed {
    read_key: PhysicalKey,
    tombstone_key: PhysicalKey,
    newer: StorageRow,
    active_lower: StorageRow,
}

fn seed_read_view_state(
    script: &[u8],
    branch: BranchId,
    state: &mut BranchLocalState,
) -> Result<ReadSeed, TestkitError> {
    let mut key = user_key(script, 80);
    key.push(0x01);
    let older = storage_row_with(
        branch,
        key.clone(),
        1,
        10,
        Timestamp::from_micros(100),
        vec![script_byte(script, 84)],
    )?;
    let newer = storage_row_with(
        branch,
        key.clone(),
        3,
        30,
        Timestamp::from_micros(90),
        vec![script_byte(script, 85), 0x00],
    )?;
    let active_lower = storage_row_with(
        branch,
        key.clone(),
        2,
        20,
        Timestamp::EPOCH,
        vec![script_byte(script, 86)],
    )?;
    let mut tombstone_key = user_key(script, 87);
    tombstone_key.push(0x02);
    let tombstone_old = storage_row_with(
        branch,
        tombstone_key.clone(),
        4,
        40,
        Timestamp::EPOCH,
        b"shadowed".to_vec(),
    )?;
    let tombstone = tombstone_row(branch, tombstone_key.clone(), 5, 50)?;

    state
        .append_committed_row(older.clone())
        .map_err(|err| TestkitError::new(format!("read older append failed: {err}")))?;
    state
        .append_committed_row(newer.clone())
        .map_err(|err| TestkitError::new(format!("read newer append failed: {err}")))?;
    state
        .append_committed_row(tombstone_old)
        .map_err(|err| TestkitError::new(format!("read shadowed append failed: {err}")))?;
    state
        .append_committed_row(tombstone.clone())
        .map_err(|err| TestkitError::new(format!("read tombstone append failed: {err}")))?;
    check_successful_rotation(state, 4, 1)?;
    state
        .append_committed_row(active_lower.clone())
        .map_err(|err| TestkitError::new(format!("read active append failed: {err}")))?;

    Ok(ReadSeed {
        read_key: physical_key(branch, key)?,
        tombstone_key: physical_key(branch, tombstone_key)?,
        newer,
        active_lower,
    })
}

fn check_read_view_point_reads(
    state: &BranchLocalState,
    branch: BranchId,
    seed: &ReadSeed,
    outcome: &mut ReadOutcome,
) -> Result<(), TestkitError> {
    let view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("read view capture failed: {err}")))?;
    if view.branch_id() != branch || view.active_row_count() != 1 || view.frozen_table_count() != 1
    {
        return Err(TestkitError::new("read view capture facts drifted"));
    }
    outcome.read_view_captures += 1;

    let latest = view
        .latest(&seed.read_key)
        .map_err(|err| TestkitError::new(format!("latest read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("latest read missed row"))?;
    if latest.row() != &seed.newer || latest.source() != (BranchRowSource::Frozen { index: 0 }) {
        return Err(TestkitError::new(
            "latest read did not select newest version across active/frozen",
        ));
    }
    outcome.latest_point_reads += 1;
    outcome.active_frozen_merge_reads += 1;

    let bounded = view
        .at_version(&seed.read_key, CommitVersion::new(2))
        .map_err(|err| TestkitError::new(format!("version read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("version read missed row"))?;
    if bounded.row() != &seed.active_lower || bounded.source() != BranchRowSource::Active {
        return Err(TestkitError::new("version read selected wrong row"));
    }
    outcome.version_bounded_point_reads += 1;

    let tombstone_read = view
        .latest(&seed.tombstone_key)
        .map_err(|err| TestkitError::new(format!("tombstone read failed: {err}")))?;
    if tombstone_read.is_some() {
        return Err(TestkitError::new(
            "selected tombstone fell through to older put",
        ));
    }
    outcome.tombstone_shadow_reads += 1;
    Ok(())
}

fn check_read_view_history(
    state: &BranchLocalState,
    seed: &ReadSeed,
    outcome: &mut ReadOutcome,
) -> Result<(), TestkitError> {
    let view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("history view capture failed: {err}")))?;
    let history = view
        .history(&seed.read_key, BranchHistoryOptions::all())
        .map_err(|err| TestkitError::new(format!("history read failed: {err}")))?;
    if history_versions(&history) != vec![3, 2, 1] {
        return Err(TestkitError::new("history read order drifted"));
    }
    outcome.history_reads += 1;

    let tombstone_history = view
        .history(&seed.tombstone_key, BranchHistoryOptions::all())
        .map_err(|err| TestkitError::new(format!("tombstone history failed: {err}")))?;
    if !tombstone_history.iter().any(|row| row.row().is_tombstone()) {
        return Err(TestkitError::new("history dropped tombstone row"));
    }
    outcome.history_tombstones += 1;

    let limited = view
        .history(
            &seed.read_key,
            BranchHistoryOptions::all().before_version(CommitVersion::new(3)),
        )
        .map_err(|err| TestkitError::new(format!("bounded history failed: {err}")))?;
    if history_versions(&limited) != vec![2, 1] {
        return Err(TestkitError::new("bounded history drifted"));
    }
    let limited_one = view
        .history(&seed.read_key, BranchHistoryOptions::all().limit(1))
        .map_err(|err| TestkitError::new(format!("limited history failed: {err}")))?;
    if history_versions(&limited_one) != vec![3] {
        return Err(TestkitError::new("one-row history limit drifted"));
    }
    let limited_zero = view
        .history(&seed.read_key, BranchHistoryOptions::all().limit(0))
        .map_err(|err| TestkitError::new(format!("zero history limit failed: {err}")))?;
    if !limited_zero.is_empty() {
        return Err(TestkitError::new("zero history limit returned rows"));
    }
    outcome.history_limits += 1;
    Ok(())
}

fn check_read_view_scans_and_pinning(
    script: &[u8],
    branch: BranchId,
    state: &mut BranchLocalState,
    outcome: &mut ReadOutcome,
) -> Result<(), TestkitError> {
    let view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("pre-prefix capture failed: {err}")))?;
    let mut prefix_key = user_key(script, 91);
    prefix_key.push(0x03);
    let prefix_a = storage_row_with(
        branch,
        [prefix_key.clone(), b"a".to_vec()].concat(),
        6,
        60,
        Timestamp::EPOCH,
        b"prefix-a".to_vec(),
    )?;
    let prefix_b = tombstone_row(branch, [prefix_key.clone(), b"b".to_vec()].concat(), 7, 70)?;
    state
        .append_committed_row(prefix_a.clone())
        .map_err(|err| TestkitError::new(format!("prefix append failed: {err}")))?;
    state
        .append_committed_row(prefix_b.clone())
        .map_err(|err| TestkitError::new(format!("prefix tombstone append failed: {err}")))?;
    let after_prefix = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("post-prefix capture failed: {err}")))?;
    let prefix_rows = after_prefix
        .scan_prefix(
            &BranchScanBounds::prefix(&physical_key(branch, prefix_key.clone())?),
            BranchReadBound::latest(),
        )
        .map_err(|err| TestkitError::new(format!("prefix scan failed: {err}")))?;
    if visible_user_keys(&prefix_rows) != vec![prefix_a.physical_key().user_key().to_vec()] {
        return Err(TestkitError::new("prefix scan result drifted"));
    }
    outcome.prefix_scans += 1;
    outcome.scan_tombstone_suppressions += 1;

    let range_rows = after_prefix
        .scan_range(
            &BranchScanBounds::range(
                branch,
                "default",
                StorageSpaceId::engine(0x20)
                    .map_err(|err| TestkitError::new(format!("storage space failed: {err}")))?,
                BranchUserKeyBound::included(prefix_key.clone()),
                BranchUserKeyBound::excluded([prefix_key.clone(), b"z".to_vec()].concat()),
            )
            .map_err(|err| TestkitError::new(format!("range bounds failed: {err}")))?,
            BranchReadBound::latest(),
        )
        .map_err(|err| TestkitError::new(format!("range scan failed: {err}")))?;
    if visible_user_keys(&range_rows) != vec![prefix_a.physical_key().user_key().to_vec()] {
        return Err(TestkitError::new("range scan result drifted"));
    }
    outcome.range_scans += 1;

    let pinned_before_append = view
        .latest(&physical_key(branch, prefix_key.clone())?)
        .map_err(|err| TestkitError::new(format!("pinned prefix read failed: {err}")))?;
    if pinned_before_append.is_some() {
        return Err(TestkitError::new("pinned view saw append after capture"));
    }
    outcome.pinned_append_isolations += 1;

    let before_rotation_view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("pre-rotation capture failed: {err}")))?;
    check_successful_rotation(state, 3, 2)?;
    let pinned_after_rotation = before_rotation_view
        .latest(prefix_a.physical_key())
        .map_err(|err| TestkitError::new(format!("pinned rotation read failed: {err}")))?;
    if pinned_after_rotation
        .as_ref()
        .is_none_or(|row| row.row() != &prefix_a)
    {
        return Err(TestkitError::new(
            "pinned view lost active row after rotation",
        ));
    }
    outcome.pinned_rotation_isolations += 1;
    Ok(())
}

fn check_read_view_rejections(
    script: &[u8],
    read_key: &PhysicalKey,
    state: &BranchLocalState,
    outcome: &mut ReadOutcome,
) -> Result<(), TestkitError> {
    let after_prefix = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("rejection view capture failed: {err}")))?;
    expect_invalid_branch_row(after_prefix.latest(&physical_key(
        branch_id(script_byte(script, 79).wrapping_add(1)),
        read_key.user_key().to_vec(),
    )?))?;
    outcome.wrong_branch_read_rejections += 1;

    Ok(())
}

fn check_branch_owned_immutable(script: &[u8]) -> Result<ImmutableOutcome, TestkitError> {
    let mut outcome = ImmutableOutcome::default();
    let branch = branch_id(script_byte(script, 100));
    check_immutable_l0_reads_and_scans(script, branch, &mut outcome)?;
    check_immutable_l1_and_invalid_installs(script, branch, &mut outcome)?;
    check_immutable_frozen_replacement(branch, &mut outcome)?;
    check_active_frozen_immutable_merge(branch, &mut outcome)?;
    Ok(outcome)
}

fn check_immutable_l0_reads_and_scans(
    script: &[u8],
    branch: BranchId,
    outcome: &mut ImmutableOutcome,
) -> Result<(), TestkitError> {
    let mut state = BranchLocalState::empty(branch);
    let pinned = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("immutable pinned capture failed: {err}")))?;
    let live = storage_row_with(
        branch,
        [b"owned-prefix-".to_vec(), user_key(script, 101)].concat(),
        3,
        30,
        Timestamp::EPOCH,
        vec![script_byte(script, 105)],
    )?;
    let old_deleted = storage_row_with(
        branch,
        b"owned-prefix-deleted".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"old".to_vec(),
    )?;
    let tombstone = tombstone_row(branch, b"owned-prefix-deleted".to_vec(), 4, 40)?;
    let high = storage_row_with(
        branch,
        vec![b'o', b'w', b'n', b'e', b'd', 0x80],
        2,
        20,
        Timestamp::EPOCH,
        b"high".to_vec(),
    )?;
    let table = branch_owned_table(
        branch,
        BranchLevel::ZERO,
        "generated-owned-l0",
        vec![live.clone(), old_deleted, tombstone, high.clone()],
    )?;
    outcome.immutable_descriptor_cases += 1;
    let install: BranchImmutableInstallOutcome = state
        .install_l0_table(table)
        .map_err(|err| TestkitError::new(format!("immutable L0 install failed: {err}")))?;
    if install.table_index() != 0 || install.owned_table_count() != 1 {
        return Err(TestkitError::new("immutable L0 install outcome drifted"));
    }
    outcome.immutable_l0_installs += 1;

    let live_key = live.physical_key().clone();
    if pinned
        .latest(&live_key)
        .map_err(|err| TestkitError::new(format!("pinned immutable read failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new(
            "pinned view saw L0 install after capture",
        ));
    }
    outcome.pinned_immutable_install_isolations += 1;

    let view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("immutable view capture failed: {err}")))?;
    let latest = view
        .latest(&live_key)
        .map_err(|err| TestkitError::new(format!("immutable latest failed: {err}")))?
        .ok_or_else(|| TestkitError::new("immutable latest missed live row"))?;
    if latest.row() != &live || !matches!(latest.source(), BranchRowSource::OwnedTable { .. }) {
        return Err(TestkitError::new("immutable latest source drifted"));
    }
    outcome.immutable_latest_reads += 1;
    outcome.immutable_source_attributions += 1;

    if view
        .latest(&physical_key(branch, b"owned-prefix-deleted".to_vec())?)
        .map_err(|err| TestkitError::new(format!("immutable tombstone read failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new(
            "immutable tombstone fell through to old put",
        ));
    }
    outcome.immutable_tombstone_shadows += 1;

    let prefix = BranchScanBounds::prefix(&physical_key(branch, b"owned-prefix-".to_vec())?);
    let prefix_rows = view
        .scan_prefix(&prefix, BranchReadBound::latest())
        .map_err(|err| TestkitError::new(format!("immutable prefix scan failed: {err}")))?;
    if visible_user_keys(&prefix_rows) != vec![live.physical_key().user_key().to_vec()] {
        return Err(TestkitError::new("immutable prefix scan drifted"));
    }
    outcome.immutable_prefix_scans += 1;

    let range = BranchScanBounds::closed(&live_key, high.physical_key())
        .map_err(|err| TestkitError::new(format!("immutable range bounds failed: {err}")))?;
    let range_rows = view
        .scan_range(&range, BranchReadBound::latest())
        .map_err(|err| TestkitError::new(format!("immutable range scan failed: {err}")))?;
    if visible_user_keys(&range_rows)
        != vec![
            live.physical_key().user_key().to_vec(),
            high.physical_key().user_key().to_vec(),
        ]
    {
        return Err(TestkitError::new("immutable range scan drifted"));
    }
    outcome.immutable_range_scans += 1;
    Ok(())
}

fn check_immutable_l1_and_invalid_installs(
    script: &[u8],
    branch: BranchId,
    outcome: &mut ImmutableOutcome,
) -> Result<(), TestkitError> {
    let config = BranchRuntimeConfig::new(3, 64, 32)
        .map_err(|err| TestkitError::new(format!("immutable config failed: {err}")))?;
    let mut state = BranchLocalState::new(branch, config)
        .map_err(|err| TestkitError::new(format!("immutable state failed: {err}")))?;
    let level = BranchLevel::new(1);
    let first = branch_owned_table(
        branch,
        level,
        "generated-l1-a-c",
        vec![
            storage_row_with(
                branch,
                b"l1-a".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                Vec::new(),
            )?,
            storage_row_with(
                branch,
                b"l1-c".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                vec![script_byte(script, 106)],
            )?,
        ],
    )?;
    let second = branch_owned_table(
        branch,
        level,
        "generated-l1-z",
        vec![storage_row_with(
            branch,
            b"l1-z".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            Vec::new(),
        )?],
    )?;
    state
        .install_owned_table_at_level(level, first)
        .map_err(|err| TestkitError::new(format!("immutable L1 first failed: {err}")))?;
    state
        .install_owned_table_at_level(level, second)
        .map_err(|err| TestkitError::new(format!("immutable L1 second failed: {err}")))?;
    outcome.immutable_l1_installs += 2;

    let before_overlap = state.clone();
    let overlap = branch_owned_table(
        branch,
        level,
        "generated-l1-overlap",
        vec![storage_row_with(
            branch,
            b"l1-b".to_vec(),
            2,
            20,
            Timestamp::EPOCH,
            Vec::new(),
        )?],
    )?;
    expect_invalid_state(state.install_owned_table_at_level(level, overlap))?;
    if state != before_overlap {
        return Err(TestkitError::new("overlapping L1 install mutated state"));
    }
    outcome.immutable_l1_overlap_rejections += 1;

    let wrong_level = branch_owned_table(
        branch,
        BranchLevel::new(2),
        "generated-wrong-level",
        vec![storage_row(branch, 8)?],
    )?;
    expect_invalid_state(state.install_owned_table_at_level(level, wrong_level))?;
    outcome.invalid_immutable_install_rejections += 1;

    let other = branch_id(script_byte(script, 100).wrapping_add(1));
    let wrong_branch = branch_owned_table(
        other,
        BranchLevel::ZERO,
        "generated-wrong-branch",
        vec![storage_row_with(
            other,
            b"wrong-branch-owned".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            b"secret-payload".to_vec(),
        )?],
    )?;
    expect_invalid_branch_row(state.install_l0_table(wrong_branch))?;
    outcome.invalid_immutable_install_rejections += 1;
    Ok(())
}

fn check_immutable_frozen_replacement(
    branch: BranchId,
    outcome: &mut ImmutableOutcome,
) -> Result<(), TestkitError> {
    let mut state = BranchLocalState::empty(branch);
    let row = storage_row_with(
        branch,
        b"generated-flush".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"flush".to_vec(),
    )?;
    state
        .append_committed_row(row.clone())
        .map_err(|err| TestkitError::new(format!("flush append failed: {err}")))?;
    check_successful_rotation(&mut state, 1, 1)?;
    let replacement = branch_owned_table(
        branch,
        BranchLevel::ZERO,
        "generated-flush-l0",
        vec![row.clone()],
    )?;
    let outcome_value = state
        .replace_frozen_with_l0_table(0, replacement)
        .map_err(|err| TestkitError::new(format!("frozen replacement failed: {err}")))?;
    if outcome_value.replaced_frozen_index() != Some(0)
        || state.frozen_table_count() != 0
        || state.owned_table_count() != 1
    {
        return Err(TestkitError::new("frozen replacement outcome drifted"));
    }
    outcome.frozen_replacements += 1;
    Ok(())
}

fn check_active_frozen_immutable_merge(
    branch: BranchId,
    outcome: &mut ImmutableOutcome,
) -> Result<(), TestkitError> {
    let mut state = BranchLocalState::empty(branch);
    let frozen_newer = storage_row_with(
        branch,
        b"merge-owned".to_vec(),
        7,
        70,
        Timestamp::EPOCH,
        b"frozen".to_vec(),
    )?;
    let active_older = storage_row_with(
        branch,
        b"merge-owned".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"active".to_vec(),
    )?;
    let owned_middle = storage_row_with(
        branch,
        b"merge-owned".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"owned".to_vec(),
    )?;
    state
        .append_committed_row(frozen_newer)
        .map_err(|err| TestkitError::new(format!("merge frozen append failed: {err}")))?;
    check_successful_rotation(&mut state, 1, 1)?;
    state
        .append_committed_row(active_older)
        .map_err(|err| TestkitError::new(format!("merge active append failed: {err}")))?;
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "generated-merge-owned",
            vec![owned_middle],
        )?)
        .map_err(|err| TestkitError::new(format!("merge owned install failed: {err}")))?;
    let key = physical_key(branch, b"merge-owned".to_vec())?;
    let view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("merge view failed: {err}")))?;
    let bounded = view
        .at_version(&key, CommitVersion::new(5))
        .map_err(|err| TestkitError::new(format!("merge version read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("merge version read missed owned row"))?;
    if !matches!(bounded.source(), BranchRowSource::OwnedTable { .. }) {
        return Err(TestkitError::new(
            "merge version read did not select owned source",
        ));
    }
    if history_versions(
        &view
            .history(&key, BranchHistoryOptions::all())
            .map_err(|err| TestkitError::new(format!("merge history failed: {err}")))?,
    ) != vec![7, 5, 2]
    {
        return Err(TestkitError::new("active/frozen/immutable history drifted"));
    }
    outcome.immutable_version_bounded_reads += 1;
    outcome.immutable_history_reads += 1;
    outcome.active_frozen_immutable_merge_reads += 1;
    Ok(())
}

fn check_branch_inheritance(script: &[u8]) -> Result<InheritanceOutcome, TestkitError> {
    let mut outcome = InheritanceOutcome::default();
    let mut fixture = check_fork_capture_and_latest(script, &mut outcome)?;
    check_child_put_shadow_and_history(&mut fixture, &mut outcome)?;
    check_manual_inherited_tombstone_and_scans(&fixture, &mut outcome)?;
    check_chained_inheritance(script, &mut outcome)?;
    check_invalid_inherited_layers(fixture.child, &mut outcome)?;
    Ok(outcome)
}

struct DirectInheritanceFixture {
    source: BranchId,
    child: BranchId,
    child_key: PhysicalKey,
    rewritten_inherited: StorageRow,
    child_state: BranchLocalState,
}

struct SourceForkFixture {
    source: BranchId,
    child: BranchId,
    source_state: BranchLocalState,
    inherited: StorageRow,
}

fn build_inheritance_source(script: &[u8]) -> Result<SourceForkFixture, TestkitError> {
    let source = branch_id(script_byte(script, 108));
    let child = branch_id(script_byte(script, 108).wrapping_add(1));
    let mut source_state = BranchLocalState::empty(source);
    let inherited = storage_row_with(
        source,
        b"generated-inherited".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"source".to_vec(),
    )?;
    source_state
        .install_l0_table(branch_owned_table(
            source,
            BranchLevel::ZERO,
            "generated-inheritance-source",
            vec![inherited.clone()],
        )?)
        .map_err(|err| TestkitError::new(format!("inheritance source install failed: {err}")))?;
    Ok(SourceForkFixture {
        source,
        child,
        source_state,
        inherited,
    })
}

fn check_fork_capture_and_latest(
    script: &[u8],
    outcome: &mut InheritanceOutcome,
) -> Result<DirectInheritanceFixture, TestkitError> {
    let SourceForkFixture {
        source,
        child,
        mut source_state,
        inherited,
    } = build_inheritance_source(script)?;

    let (child_state, fork_outcome): (BranchLocalState, BranchForkOutcome) = source_state
        .fork_into_empty_child(child)
        .map_err(|err| TestkitError::new(format!("fork capture failed: {err}")))?;
    if fork_outcome.source_branch_id() != source
        || fork_outcome.destination_branch_id() != child
        || fork_outcome.fork_version() != CommitVersion::new(3)
        || fork_outcome.inherited_layer_count() != 1
        || fork_outcome.inherited_table_count() != 1
        || child_state.owned_table_count() != 0
        || child_state.inherited_layer_count() != 1
    {
        return Err(TestkitError::new("fork capture outcome drifted"));
    }
    outcome.inherited_fork_captures += 1;
    outcome.inherited_layer_validations += 1;

    let view = child_state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("child inherited view failed: {err}")))?;
    let child_key = physical_key(child, b"generated-inherited".to_vec())?;
    let rewritten = rewrite_row_branch(&inherited, source, child)
        .map_err(|err| TestkitError::new(format!("expected inherited rewrite failed: {err}")))?;
    let latest = view
        .latest(&child_key)
        .map_err(|err| TestkitError::new(format!("inherited latest failed: {err}")))?
        .ok_or_else(|| TestkitError::new("inherited latest missed row"))?;
    if latest.row() != &rewritten
        || latest.source()
            != (BranchRowSource::Inherited {
                source_branch_id: source,
                layer_index: 0,
            })
    {
        return Err(TestkitError::new("inherited latest/source rewrite drifted"));
    }
    outcome.inherited_latest_reads += 1;
    outcome.inherited_key_rewrites += 1;

    if view
        .latest(&physical_key(
            child,
            b"generated-source-active-only".to_vec(),
        )?)
        .map_err(|err| TestkitError::new(format!("source active-only read failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new(
            "fork inherited source active row without flush",
        ));
    }
    outcome.inherited_post_fork_invisibility += 1;

    source_state
        .install_l0_table(branch_owned_table(
            source,
            BranchLevel::ZERO,
            "generated-inheritance-source-later",
            vec![storage_row_with(
                source,
                b"generated-source-later".to_vec(),
                9,
                90,
                Timestamp::EPOCH,
                b"later".to_vec(),
            )?],
        )?)
        .map_err(|err| TestkitError::new(format!("source later install failed: {err}")))?;
    if view
        .latest(&physical_key(child, b"generated-source-later".to_vec())?)
        .map_err(|err| TestkitError::new(format!("pinned inherited read failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new(
            "pinned inherited view saw later source mutation",
        ));
    }
    outcome.pinned_inherited_view_isolations += 1;

    Ok(DirectInheritanceFixture {
        source,
        child,
        child_key,
        rewritten_inherited: rewritten,
        child_state,
    })
}

fn check_child_put_shadow_and_history(
    fixture: &mut DirectInheritanceFixture,
    outcome: &mut InheritanceOutcome,
) -> Result<(), TestkitError> {
    fixture
        .child_state
        .append_committed_row(storage_row_with(
            fixture.child,
            b"generated-inherited".to_vec(),
            7,
            70,
            Timestamp::EPOCH,
            b"child".to_vec(),
        )?)
        .map_err(|err| TestkitError::new(format!("child shadow put failed: {err}")))?;
    let shadow_view = fixture
        .child_state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("child shadow view failed: {err}")))?;
    let child_put = shadow_view
        .latest(&fixture.child_key)
        .map_err(|err| TestkitError::new(format!("child shadow latest failed: {err}")))?
        .ok_or_else(|| TestkitError::new("child shadow latest missed put"))?;
    if child_put.source() != BranchRowSource::Active {
        return Err(TestkitError::new("child put did not shadow inherited row"));
    }
    outcome.inherited_child_put_shadows += 1;
    let inherited_before_child = shadow_view
        .at_version(&fixture.child_key, CommitVersion::new(4))
        .map_err(|err| TestkitError::new(format!("inherited bounded read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("bounded inherited read missed row"))?;
    if inherited_before_child.row() != &fixture.rewritten_inherited {
        return Err(TestkitError::new("bounded inherited read drifted"));
    }
    outcome.inherited_version_bounded_reads += 1;

    let history = shadow_view
        .history(&fixture.child_key, BranchHistoryOptions::all())
        .map_err(|err| TestkitError::new(format!("inherited history failed: {err}")))?;
    if history_versions(&history) != vec![7, 3] {
        return Err(TestkitError::new("inherited history versions drifted"));
    }
    outcome.inherited_history_reads += 1;
    Ok(())
}

fn check_manual_inherited_tombstone_and_scans(
    fixture: &DirectInheritanceFixture,
    outcome: &mut InheritanceOutcome,
) -> Result<(), TestkitError> {
    let tombstone_key = physical_key(fixture.child, b"generated-delete-shadow".to_vec())?;
    let layer = branch_inherited_layer_unchecked_for_fork_gate_checks(
        fixture.source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            fixture.source,
            BranchLevel::ZERO,
            "generated-delete-shadow-source",
            vec![
                storage_row_with(
                    fixture.source,
                    b"generated-delete-shadow".to_vec(),
                    2,
                    20,
                    Timestamp::EPOCH,
                    b"source".to_vec(),
                )?,
                storage_row_with(
                    fixture.source,
                    b"generated-post-fork".to_vec(),
                    8,
                    80,
                    Timestamp::EPOCH,
                    b"post".to_vec(),
                )?,
                storage_row_with(
                    fixture.source,
                    b"generated-scan-visible".to_vec(),
                    3,
                    30,
                    Timestamp::EPOCH,
                    b"visible-scan".to_vec(),
                )?,
            ],
        )?]],
    );
    let mut tombstone_child = BranchLocalState::empty(fixture.child);
    tombstone_child
        .attach_inherited_layers(vec![layer])
        .map_err(|err| TestkitError::new(format!("manual inherited attach failed: {err}")))?;
    tombstone_child
        .append_committed_row(tombstone_row(
            fixture.child,
            b"generated-delete-shadow".to_vec(),
            6,
            60,
        )?)
        .map_err(|err| TestkitError::new(format!("child inherited tombstone failed: {err}")))?;
    let tombstone_view = tombstone_child
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("tombstone inherited view failed: {err}")))?;
    if tombstone_view
        .latest(&tombstone_key)
        .map_err(|err| TestkitError::new(format!("tombstone shadow read failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new(
            "child tombstone fell through to inherited put",
        ));
    }
    outcome.inherited_child_tombstone_shadows += 1;
    if tombstone_view
        .latest(&physical_key(
            fixture.child,
            b"generated-post-fork".to_vec(),
        )?)
        .map_err(|err| TestkitError::new(format!("post-fork inherited read failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new("manual fork gate exposed post-fork row"));
    }
    outcome.inherited_post_fork_invisibility += 1;

    let prefix = BranchScanBounds::prefix(&physical_key(fixture.child, b"generated-".to_vec())?);
    let prefix_rows = tombstone_view
        .scan_prefix(&prefix, BranchReadBound::latest())
        .map_err(|err| TestkitError::new(format!("inherited prefix scan failed: {err}")))?;
    if visible_user_keys(&prefix_rows) != vec![b"generated-scan-visible".to_vec()] {
        return Err(TestkitError::new(
            "inherited prefix scan did not rewrite and filter visible rows",
        ));
    }
    outcome.inherited_prefix_scans += 1;

    let range = BranchScanBounds::closed(
        &physical_key(fixture.child, b"generated-delete-shadow".to_vec())?,
        &physical_key(fixture.child, b"generated-scan-visible".to_vec())?,
    )
    .map_err(|err| TestkitError::new(format!("inherited range bounds failed: {err}")))?;
    let range_rows = tombstone_view
        .scan_range(&range, BranchReadBound::latest())
        .map_err(|err| TestkitError::new(format!("inherited range scan failed: {err}")))?;
    if visible_user_keys(&range_rows) != vec![b"generated-scan-visible".to_vec()] {
        return Err(TestkitError::new(
            "inherited range scan did not rewrite and filter visible rows",
        ));
    }
    outcome.inherited_range_scans += 1;
    Ok(())
}

fn check_chained_inheritance(
    script: &[u8],
    outcome: &mut InheritanceOutcome,
) -> Result<(), TestkitError> {
    let grandparent = branch_id(script_byte(script, 109));
    let parent = branch_id(script_byte(script, 109).wrapping_add(1));
    let child = branch_id(script_byte(script, 109).wrapping_add(2));
    let grandparent_row = storage_row_with(
        grandparent,
        b"generated-chain".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"grandparent".to_vec(),
    )?;
    let parent_row = storage_row_with(
        parent,
        b"generated-chain".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"parent".to_vec(),
    )?;
    let mut grandparent_state = BranchLocalState::empty(grandparent);
    grandparent_state
        .install_l0_table(branch_owned_table(
            grandparent,
            BranchLevel::ZERO,
            "generated-chain-grandparent",
            vec![grandparent_row],
        )?)
        .map_err(|err| TestkitError::new(format!("chain grandparent install failed: {err}")))?;
    let (mut parent_state, _) = grandparent_state
        .fork_into_empty_child(parent)
        .map_err(|err| TestkitError::new(format!("chain parent fork failed: {err}")))?;
    parent_state
        .install_l0_table(branch_owned_table(
            parent,
            BranchLevel::ZERO,
            "generated-chain-parent",
            vec![parent_row.clone()],
        )?)
        .map_err(|err| TestkitError::new(format!("chain parent install failed: {err}")))?;
    let (child_state, outcome_value) = parent_state
        .fork_into_empty_child(child)
        .map_err(|err| TestkitError::new(format!("chain child fork failed: {err}")))?;
    if outcome_value.inherited_layer_count() != 2 {
        return Err(TestkitError::new("chain inherited layer count drifted"));
    }
    let visible = child_state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("chain child view failed: {err}")))?
        .latest(&physical_key(child, b"generated-chain".to_vec())?)
        .map_err(|err| TestkitError::new(format!("chain latest failed: {err}")))?
        .ok_or_else(|| TestkitError::new("chain latest missed row"))?;
    if visible.source()
        != (BranchRowSource::Inherited {
            source_branch_id: parent,
            layer_index: 0,
        })
    {
        return Err(TestkitError::new("nearest inherited layer did not win"));
    }
    outcome.inherited_chained_ancestry += 1;
    Ok(())
}

fn check_invalid_inherited_layers(
    child: BranchId,
    outcome: &mut InheritanceOutcome,
) -> Result<(), TestkitError> {
    let source = branch_id(0xf1);
    let table = branch_owned_table(
        source,
        BranchLevel::ZERO,
        "generated-invalid-inherited-source",
        vec![storage_row_with(
            source,
            b"invalid-inherited".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            b"secret-payload".to_vec(),
        )?],
    )?;
    expect_invalid_inherited_layer(BranchInheritedLayer::new(
        InheritedLayerDescriptor::new(
            source,
            CommitVersion::new(1),
            InheritedLayerStatus::Active,
            2,
        ),
        vec![vec![table.clone()]],
    ))?;
    let mut child_state = BranchLocalState::empty(child);
    expect_invalid_inherited_layer(child_state.attach_inherited_layers(vec![
        branch_inherited_layer(
            child,
            CommitVersion::new(1),
            InheritedLayerStatus::Active,
            Vec::new(),
        )?,
    ]))?;
    outcome.invalid_inherited_layer_rejections += 2;
    Ok(())
}

fn check_branch_timestamp_visibility(script: &[u8]) -> Result<TimestampOutcome, TestkitError> {
    let mut outcome = TimestampOutcome::default();
    check_timestamp_point_reads(script, &mut outcome)?;
    check_timestamp_frozen_and_owned_point_reads(script, &mut outcome)?;
    check_timestamp_ttl(script, &mut outcome)?;
    check_timestamp_tombstones(script, &mut outcome)?;
    check_timestamp_scans(script, &mut outcome)?;
    check_inherited_timestamp_reads(script, &mut outcome)?;
    check_inherited_timestamp_scans(script, &mut outcome)?;
    check_inherited_timestamp_local_shadows_and_ties(script, &mut outcome)?;
    check_pinned_timestamp_views(script, &mut outcome)?;
    check_timestamp_coverage(script, &mut outcome)?;
    Ok(outcome)
}

fn check_timestamp_point_reads(
    script: &[u8],
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 110));
    let mut state = BranchLocalState::empty(branch);
    let key = b"generated-as-of".to_vec();
    let older = storage_row_with(
        branch,
        key.clone(),
        7,
        80,
        Timestamp::EPOCH,
        vec![script_byte(script, 111)],
    )?;
    let highest_version = storage_row_with(
        branch,
        key.clone(),
        10,
        100,
        Timestamp::EPOCH,
        vec![script_byte(script, 112)],
    )?;
    let lower_version_later_timestamp = storage_row_with(
        branch,
        key.clone(),
        8,
        120,
        Timestamp::EPOCH,
        vec![script_byte(script, 113)],
    )?;
    for row in [
        older.clone(),
        highest_version.clone(),
        lower_version_later_timestamp,
    ] {
        append_expect_put(&mut state, &row)?;
    }
    let read_key = physical_key(branch, key)?;
    let view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("timestamp point view failed: {err}")))?
        .with_timestamp_coverage(BranchTimestampCoverage::complete());
    if view
        .read_point(
            &read_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(79)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp below read failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new(
            "timestamp point read returned row before every eligible timestamp",
        ));
    }
    let at_older = view
        .read_point(
            &read_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(80)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp exact read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("timestamp exact read missed older row"))?;
    if at_older.row() != &older {
        return Err(TestkitError::new("timestamp exact read selected wrong row"));
    }
    let after_all = view
        .read_point(
            &read_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(130)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp nonmonotonic read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("timestamp after-all read missed row"))?;
    if after_all.row() != &highest_version {
        return Err(TestkitError::new(
            "timestamp read sorted by timestamp instead of commit version",
        ));
    }
    outcome.timestamp_point_reads += 2;
    outcome.active_timestamp_point_reads += 2;
    outcome.non_monotonic_timestamp_reads += 1;
    Ok(())
}

fn check_timestamp_frozen_and_owned_point_reads(
    script: &[u8],
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 125));
    let mut state = BranchLocalState::empty(branch);
    let frozen_key = b"generated-frozen-as-of".to_vec();
    let frozen_visible = storage_row_with(
        branch,
        frozen_key.clone(),
        2,
        20,
        Timestamp::EPOCH,
        vec![script_byte(script, 126)],
    )?;
    let frozen_future = storage_row_with(
        branch,
        frozen_key.clone(),
        3,
        50,
        Timestamp::EPOCH,
        b"frozen-future".to_vec(),
    )?;
    for row in [frozen_visible.clone(), frozen_future] {
        state
            .append_committed_row(row)
            .map_err(|err| TestkitError::new(format!("timestamp frozen append failed: {err}")))?;
    }
    match state.rotate_active() {
        BranchRotationOutcome::Rotated { .. } => {}
        outcome @ BranchRotationOutcome::Skipped { .. } => {
            return Err(TestkitError::new(format!(
                "timestamp frozen rotation skipped: {outcome:?}",
            )))
        }
    }

    let owned_key = b"generated-owned-as-of".to_vec();
    let owned_visible = storage_row_with(
        branch,
        owned_key.clone(),
        4,
        40,
        Timestamp::EPOCH,
        vec![script_byte(script, 127)],
    )?;
    let owned_future = storage_row_with(
        branch,
        owned_key.clone(),
        6,
        80,
        Timestamp::EPOCH,
        b"owned-future".to_vec(),
    )?;
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "generated-owned-as-of",
            vec![owned_visible.clone(), owned_future],
        )?)
        .map_err(|err| TestkitError::new(format!("timestamp owned install failed: {err}")))?;

    let view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("timestamp source view failed: {err}")))?;
    let frozen = view
        .read_point(
            &physical_key(branch, frozen_key)?,
            BranchReadBound::at_timestamp(Timestamp::from_micros(30)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp frozen read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("timestamp frozen read missed row"))?;
    if frozen.row() != &frozen_visible || frozen.source() != (BranchRowSource::Frozen { index: 0 })
    {
        return Err(TestkitError::new("timestamp frozen read/source drifted"));
    }
    outcome.timestamp_point_reads += 1;
    outcome.frozen_timestamp_point_reads += 1;

    let owned = view
        .read_point(
            &physical_key(branch, owned_key)?,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp owned read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("timestamp owned read missed row"))?;
    if owned.row() != &owned_visible
        || owned.source()
            != (BranchRowSource::OwnedTable {
                level: BranchLevel::ZERO,
                table_index: 0,
            })
    {
        return Err(TestkitError::new("timestamp owned read/source drifted"));
    }
    outcome.timestamp_point_reads += 1;
    outcome.owned_timestamp_point_reads += 1;
    Ok(())
}

fn check_timestamp_ttl(script: &[u8], outcome: &mut TimestampOutcome) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 114));
    let mut state = BranchLocalState::empty(branch);
    let ttl_key = b"generated-ttl".to_vec();
    let old = storage_row_with(
        branch,
        ttl_key.clone(),
        1,
        5,
        Timestamp::EPOCH,
        vec![script_byte(script, 115)],
    )?;
    let expiring = storage_row_with(
        branch,
        ttl_key.clone(),
        2,
        10,
        Timestamp::from_micros(20),
        vec![script_byte(script, 116)],
    )?;
    let epoch_key = b"generated-epoch-expiry".to_vec();
    let epoch_expiry = storage_row_with(
        branch,
        epoch_key.clone(),
        4,
        40,
        Timestamp::EPOCH,
        Vec::new(),
    )?;
    for row in [old, expiring.clone(), epoch_expiry.clone()] {
        state
            .append_committed_row(row)
            .map_err(|err| TestkitError::new(format!("timestamp ttl append failed: {err}")))?;
    }
    let view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("timestamp ttl view failed: {err}")))?;
    let ttl_physical_key = physical_key(branch, ttl_key)?;
    let before_expiry = view
        .read_point(
            &ttl_physical_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(19)),
        )
        .map_err(|err| TestkitError::new(format!("ttl before read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("ttl before expiry missed row"))?;
    if before_expiry.row() != &expiring {
        return Err(TestkitError::new("ttl before expiry selected wrong row"));
    }
    outcome.ttl_before_expiry_reads += 1;

    if view
        .read_point(
            &ttl_physical_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(20)),
        )
        .map_err(|err| TestkitError::new(format!("ttl exact read failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new(
            "ttl exact expiry remained visible or fell through",
        ));
    }
    outcome.ttl_exact_expiry_suppressions += 1;

    if view
        .read_point(
            &ttl_physical_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(21)),
        )
        .map_err(|err| TestkitError::new(format!("ttl after read failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new(
            "ttl after expiry remained visible or fell through",
        ));
    }
    outcome.ttl_after_expiry_suppressions += 1;

    let epoch_read = view
        .read_point(
            &physical_key(branch, epoch_key)?,
            BranchReadBound::at_timestamp(Timestamp::MAX),
        )
        .map_err(|err| TestkitError::new(format!("epoch expiry read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("epoch expiry read missed row"))?;
    if epoch_read.row() != &epoch_expiry {
        return Err(TestkitError::new(
            "epoch expiry sentinel did not behave as no expiry",
        ));
    }
    outcome.timestamp_point_reads += 1;

    check_timestamp_max_expiry(script, branch, outcome)?;
    Ok(())
}

fn check_timestamp_max_expiry(
    script: &[u8],
    branch: BranchId,
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    let mut state = BranchLocalState::empty(branch);
    let max_key = b"generated-max-expiry".to_vec();
    let max_expiry = storage_row_with(
        branch,
        max_key.clone(),
        5,
        50,
        Timestamp::MAX,
        vec![script_byte(script, 133)],
    )?;
    state
        .append_committed_row(max_expiry.clone())
        .map_err(|err| TestkitError::new(format!("timestamp max expiry append failed: {err}")))?;
    let view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("timestamp max expiry view failed: {err}")))?;
    let max_before = view
        .read_point(
            &physical_key(branch, max_key.clone())?,
            BranchReadBound::at_timestamp(Timestamp::from_micros(u64::MAX - 1)),
        )
        .map_err(|err| TestkitError::new(format!("max expiry before read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("max expiry before read missed row"))?;
    if max_before.row() != &max_expiry {
        return Err(TestkitError::new("max expiry selected wrong row"));
    }
    if view
        .read_point(
            &physical_key(branch, max_key)?,
            BranchReadBound::at_timestamp(Timestamp::MAX),
        )
        .map_err(|err| TestkitError::new(format!("max expiry exact read failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new(
            "Timestamp::MAX expiry behaved as no-expiry sentinel",
        ));
    }
    outcome.ttl_max_expiry_reads += 1;
    Ok(())
}

fn check_timestamp_tombstones(
    script: &[u8],
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 114));
    let mut state = BranchLocalState::empty(branch);
    let deleted_key = b"generated-ts-delete".to_vec();
    let deleted_put = storage_row_with(
        branch,
        deleted_key.clone(),
        1,
        10,
        Timestamp::EPOCH,
        vec![script_byte(script, 117)],
    )?;
    let deleted = tombstone_row(branch, deleted_key.clone(), 3, 30)?;
    for row in [deleted_put.clone(), deleted] {
        state.append_committed_row(row).map_err(|err| {
            TestkitError::new(format!("timestamp tombstone append failed: {err}"))
        })?;
    }
    let view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("timestamp tombstone view failed: {err}")))?;
    let deleted_physical_key = physical_key(branch, deleted_key)?;
    let before_tombstone = view
        .read_point(
            &deleted_physical_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(29)),
        )
        .map_err(|err| TestkitError::new(format!("pre-tombstone read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("pre-tombstone read missed put"))?;
    if before_tombstone.row() != &deleted_put {
        return Err(TestkitError::new("pre-tombstone read selected wrong row"));
    }
    outcome.timestamp_tombstone_after_non_shadows += 1;
    if view
        .read_point(
            &deleted_physical_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(30)),
        )
        .map_err(|err| TestkitError::new(format!("tombstone timestamp read failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new("timestamp tombstone fell through"));
    }
    outcome.timestamp_tombstone_shadows += 1;
    Ok(())
}

fn check_timestamp_scans(
    script: &[u8],
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 118));
    let mut state = BranchLocalState::empty(branch);
    let fixture = timestamp_scan_fixture(script, branch, &mut state)?;
    let view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("timestamp scan view failed: {err}")))?
        .with_timestamp_coverage(BranchTimestampCoverage::complete());
    check_timestamp_basic_scans(branch, &view, &fixture, outcome)?;
    check_timestamp_scan_edges(branch, &view, outcome)?;
    check_timestamp_scan_space_isolation(branch, &view, &fixture, outcome)?;
    Ok(())
}

struct TimestampScanFixture {
    visible: StorageRow,
    system_row: StorageRow,
    other_space_row: StorageRow,
    engine_space: StorageSpaceId,
    other_space: StorageSpaceId,
}

fn timestamp_scan_fixture(
    script: &[u8],
    branch: BranchId,
    state: &mut BranchLocalState,
) -> Result<TimestampScanFixture, TestkitError> {
    let engine_space = StorageSpaceId::engine(0x20)
        .map_err(|err| TestkitError::new(format!("storage space failed: {err}")))?;
    let other_space = StorageSpaceId::engine(0x21)
        .map_err(|err| TestkitError::new(format!("other storage space failed: {err}")))?;
    let visible = storage_row_with(
        branch,
        b"generated-ts-scan-a".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        vec![script_byte(script, 119)],
    )?;
    let future = storage_row_with(
        branch,
        b"generated-ts-scan-b".to_vec(),
        2,
        50,
        Timestamp::EPOCH,
        vec![script_byte(script, 120)],
    )?;
    let expired_old = storage_row_with(
        branch,
        b"generated-ts-scan-c".to_vec(),
        1,
        5,
        Timestamp::EPOCH,
        b"old".to_vec(),
    )?;
    let expired_new = storage_row_with(
        branch,
        b"generated-ts-scan-c".to_vec(),
        3,
        30,
        Timestamp::from_micros(35),
        b"expired".to_vec(),
    )?;
    let deleted_old = storage_row_with(
        branch,
        b"generated-ts-scan-d".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"deleted".to_vec(),
    )?;
    let deleted = tombstone_row(branch, b"generated-ts-scan-d".to_vec(), 4, 40)?;
    let system_row = storage_row_with_space(
        branch,
        "system",
        engine_space,
        b"generated-ts-scan-a".to_vec(),
        5,
        15,
        Timestamp::EPOCH,
        b"system-space".to_vec(),
    )?;
    let other_space_row = storage_row_with_space(
        branch,
        "default",
        other_space,
        b"generated-ts-scan-a".to_vec(),
        6,
        15,
        Timestamp::EPOCH,
        b"other-storage-space".to_vec(),
    )?;
    for row in [
        visible.clone(),
        future,
        expired_old,
        expired_new,
        deleted_old,
        deleted,
        system_row.clone(),
        other_space_row.clone(),
    ] {
        state
            .append_committed_row(row)
            .map_err(|err| TestkitError::new(format!("timestamp scan append failed: {err}")))?;
    }
    Ok(TimestampScanFixture {
        visible,
        system_row,
        other_space_row,
        engine_space,
        other_space,
    })
}

fn check_timestamp_basic_scans(
    branch: BranchId,
    view: &crate::branch::BranchReadView,
    fixture: &TimestampScanFixture,
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    let prefix = BranchScanBounds::prefix(&physical_key(branch, b"generated-ts-scan-".to_vec())?);
    let prefix_rows = view
        .scan_prefix(
            &prefix,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp prefix scan failed: {err}")))?;
    if visible_user_keys(&prefix_rows) != vec![fixture.visible.physical_key().user_key().to_vec()] {
        return Err(TestkitError::new("timestamp prefix scan drifted"));
    }
    outcome.timestamp_scan_reads += 1;
    outcome.timestamp_prefix_scans += 1;

    let range = BranchScanBounds::closed(
        &physical_key(branch, b"generated-ts-scan-a".to_vec())?,
        &physical_key(branch, b"generated-ts-scan-d".to_vec())?,
    )
    .map_err(|err| TestkitError::new(format!("timestamp scan bounds failed: {err}")))?;
    let range_rows = view
        .scan_range(
            &range,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp range scan failed: {err}")))?;
    if visible_user_keys(&range_rows) != vec![fixture.visible.physical_key().user_key().to_vec()] {
        return Err(TestkitError::new("timestamp range scan drifted"));
    }
    outcome.timestamp_scan_reads += 1;
    outcome.timestamp_range_scans += 1;
    Ok(())
}

fn check_timestamp_scan_edges(
    branch: BranchId,
    view: &crate::branch::BranchReadView,
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    let prefix = BranchScanBounds::prefix(&physical_key(branch, b"generated-ts-scan-".to_vec())?);
    let before_all_rows = view
        .scan_prefix(
            &prefix,
            BranchReadBound::at_timestamp(Timestamp::from_micros(4)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp empty prefix failed: {err}")))?;
    if !before_all_rows.is_empty() {
        return Err(TestkitError::new(
            "timestamp scan returned rows before every eligible timestamp",
        ));
    }
    outcome.timestamp_scan_reads += 1;
    outcome.timestamp_empty_scans += 1;

    let open = BranchScanBounds::open(
        &physical_key(branch, b"generated-ts-scan-a".to_vec())?,
        &physical_key(branch, b"generated-ts-scan-d".to_vec())?,
    )
    .map_err(|err| TestkitError::new(format!("timestamp open bounds failed: {err}")))?;
    let open_rows = view
        .scan_range(
            &open,
            BranchReadBound::at_timestamp(Timestamp::from_micros(60)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp open range failed: {err}")))?;
    if visible_user_keys(&open_rows) != vec![b"generated-ts-scan-b".to_vec()] {
        return Err(TestkitError::new(
            "timestamp open range failed to preserve exclusive edges",
        ));
    }
    outcome.timestamp_scan_reads += 1;
    outcome.timestamp_range_scans += 1;
    outcome.timestamp_scan_boundary_reads += 1;
    Ok(())
}

fn check_timestamp_scan_space_isolation(
    branch: BranchId,
    view: &crate::branch::BranchReadView,
    fixture: &TimestampScanFixture,
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    let system_rows = view
        .scan_prefix(
            &BranchScanBounds::prefix(&physical_key_with_space(
                branch,
                "system",
                fixture.engine_space,
                b"generated-ts-scan-".to_vec(),
            )?),
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp system scan failed: {err}")))?;
    let other_space_rows = view
        .scan_prefix(
            &BranchScanBounds::prefix(&physical_key_with_space(
                branch,
                "default",
                fixture.other_space,
                b"generated-ts-scan-".to_vec(),
            )?),
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp other-space scan failed: {err}")))?;
    if system_rows.len() != 1
        || system_rows[0].row() != &fixture.system_row
        || other_space_rows.len() != 1
        || other_space_rows[0].row() != &fixture.other_space_row
    {
        return Err(TestkitError::new(
            "timestamp scans leaked across key-space boundaries",
        ));
    }
    outcome.timestamp_scan_reads += 2;
    outcome.timestamp_prefix_scans += 2;
    outcome.timestamp_scan_space_isolations += 1;
    Ok(())
}

fn check_inherited_timestamp_reads(
    script: &[u8],
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    let source = branch_id(script_byte(script, 121));
    let child = branch_id(script_byte(script, 121).wrapping_add(1));
    let visible = storage_row_with(
        source,
        b"generated-inherited-time".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        vec![script_byte(script, 122)],
    )?;
    let future_timestamp = storage_row_with(
        source,
        b"generated-inherited-time".to_vec(),
        4,
        70,
        Timestamp::EPOCH,
        b"future".to_vec(),
    )?;
    let after_fork_old_timestamp = storage_row_with(
        source,
        b"generated-inherited-time".to_vec(),
        7,
        20,
        Timestamp::EPOCH,
        b"post-fork".to_vec(),
    )?;
    let layer = branch_inherited_layer_unchecked_for_fork_gate_checks(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "generated-inherited-time",
            vec![visible.clone(), future_timestamp, after_fork_old_timestamp],
        )?]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .map_err(|err| TestkitError::new(format!("timestamp inherited attach failed: {err}")))?;
    let view = child_state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("timestamp inherited view failed: {err}")))?
        .with_timestamp_coverage(BranchTimestampCoverage::complete());
    let child_key = physical_key(child, b"generated-inherited-time".to_vec())?;
    let inherited = view
        .read_point(
            &child_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp inherited read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("timestamp inherited read missed row"))?;
    let expected = rewrite_row_branch(&visible, source, child)
        .map_err(|err| TestkitError::new(format!("timestamp inherited rewrite failed: {err}")))?;
    if inherited.row() != &expected {
        return Err(TestkitError::new("timestamp inherited row drifted"));
    }
    outcome.inherited_timestamp_reads += 1;
    outcome.inherited_timestamp_point_reads += 1;

    if view
        .read_point(
            &child_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(25)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp fork gate read failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new(
            "timestamp inherited read exposed post-fork row with old timestamp",
        ));
    }
    outcome.inherited_timestamp_fork_gates += 1;
    Ok(())
}

fn check_inherited_timestamp_scans(
    script: &[u8],
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    let source = branch_id(script_byte(script, 128));
    let child = branch_id(script_byte(script, 128).wrapping_add(1));
    let visible = storage_row_with(
        source,
        b"generated-inherited-scan-a".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        vec![script_byte(script, 129)],
    )?;
    let future_timestamp = storage_row_with(
        source,
        b"generated-inherited-scan-b".to_vec(),
        4,
        70,
        Timestamp::EPOCH,
        b"future".to_vec(),
    )?;
    let after_fork_old_timestamp = storage_row_with(
        source,
        b"generated-inherited-scan-c".to_vec(),
        7,
        20,
        Timestamp::EPOCH,
        b"post-fork".to_vec(),
    )?;
    let layer = branch_inherited_layer_unchecked_for_fork_gate_checks(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "generated-inherited-scan",
            vec![visible.clone(), future_timestamp, after_fork_old_timestamp],
        )?]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .map_err(|err| {
            TestkitError::new(format!("timestamp inherited scan attach failed: {err}"))
        })?;
    let view = child_state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("timestamp inherited scan view failed: {err}")))?
        .with_timestamp_coverage(BranchTimestampCoverage::complete());
    let expected = rewrite_row_branch(&visible, source, child)
        .map_err(|err| TestkitError::new(format!("timestamp scan rewrite failed: {err}")))?;
    let prefix =
        BranchScanBounds::prefix(&physical_key(child, b"generated-inherited-scan-".to_vec())?);
    let prefix_rows = view
        .scan_prefix(
            &prefix,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp inherited prefix failed: {err}")))?;
    if visible_user_keys(&prefix_rows) != vec![expected.physical_key().user_key().to_vec()]
        || prefix_rows.first().map(BranchVisibleRow::row) != Some(&expected)
    {
        return Err(TestkitError::new(
            "timestamp inherited prefix scan did not rewrite before grouping",
        ));
    }
    outcome.timestamp_scan_reads += 1;
    outcome.timestamp_prefix_scans += 1;
    outcome.inherited_timestamp_scan_reads += 1;

    let range = BranchScanBounds::closed(
        &physical_key(child, b"generated-inherited-scan-a".to_vec())?,
        &physical_key(child, b"generated-inherited-scan-c".to_vec())?,
    )
    .map_err(|err| TestkitError::new(format!("timestamp inherited bounds failed: {err}")))?;
    let range_rows = view
        .scan_range(
            &range,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp inherited range failed: {err}")))?;
    if range_rows.len() != 1 || range_rows[0].row() != &expected {
        return Err(TestkitError::new("timestamp inherited range scan drifted"));
    }
    outcome.timestamp_scan_reads += 1;
    outcome.timestamp_range_scans += 1;
    outcome.inherited_timestamp_scan_reads += 1;
    Ok(())
}

fn check_inherited_timestamp_local_shadows_and_ties(
    script: &[u8],
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    let (mut child_state, fixture) = inherited_timestamp_shadow_fixture(script)?;
    check_inherited_timestamp_nearest_tie(&child_state, &fixture, outcome)?;
    check_inherited_timestamp_child_put_shadow(&mut child_state, &fixture, outcome)?;
    check_inherited_timestamp_child_tombstone_shadow(&mut child_state, &fixture, outcome)?;
    Ok(())
}

struct InheritedTimestampShadowFixture {
    child: BranchId,
    nearest_source: BranchId,
    key: Vec<u8>,
    child_key: PhysicalKey,
    expected_nearest: StorageRow,
}

fn inherited_timestamp_shadow_fixture(
    script: &[u8],
) -> Result<(BranchLocalState, InheritedTimestampShadowFixture), TestkitError> {
    let nearest_source = branch_id(script_byte(script, 133));
    let farther_source = branch_id(script_byte(script, 133).wrapping_add(1));
    let child = branch_id(script_byte(script, 133).wrapping_add(2));
    let key = b"generated-inherited-shadow".to_vec();
    let nearest = storage_row_with(
        nearest_source,
        key.clone(),
        3,
        30,
        Timestamp::EPOCH,
        vec![script_byte(script, 134)],
    )?;
    let farther = storage_row_with(
        farther_source,
        key.clone(),
        3,
        30,
        Timestamp::EPOCH,
        b"farther".to_vec(),
    )?;
    let nearest_layer = branch_inherited_layer(
        nearest_source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            nearest_source,
            BranchLevel::ZERO,
            "generated-nearest-time-tie",
            vec![nearest.clone()],
        )?]],
    )?;
    let farther_layer = branch_inherited_layer(
        farther_source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            farther_source,
            BranchLevel::ZERO,
            "generated-farther-time-tie",
            vec![farther],
        )?]],
    )?;
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![nearest_layer, farther_layer])
        .map_err(|err| {
            TestkitError::new(format!("timestamp inherited shadow attach failed: {err}"))
        })?;
    let expected_nearest = rewrite_row_branch(&nearest, nearest_source, child)
        .map_err(|err| TestkitError::new(format!("nearest inherited rewrite failed: {err}")))?;
    Ok((
        child_state,
        InheritedTimestampShadowFixture {
            child,
            nearest_source,
            key: key.clone(),
            child_key: physical_key(child, key)?,
            expected_nearest,
        },
    ))
}

fn check_inherited_timestamp_nearest_tie(
    child_state: &BranchLocalState,
    fixture: &InheritedTimestampShadowFixture,
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    let inherited = child_state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("timestamp inherited tie view failed: {err}")))?
        .read_point(
            &fixture.child_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp inherited tie read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("timestamp inherited tie missed row"))?;
    if inherited.row() != &fixture.expected_nearest
        || inherited.source()
            != (BranchRowSource::Inherited {
                source_branch_id: fixture.nearest_source,
                layer_index: 0,
            })
    {
        return Err(TestkitError::new(
            "nearest inherited timestamp layer did not win exact tie",
        ));
    }
    outcome.inherited_timestamp_reads += 1;
    outcome.inherited_timestamp_point_reads += 1;
    outcome.inherited_timestamp_nearest_ties += 1;
    Ok(())
}

fn check_inherited_timestamp_child_put_shadow(
    child_state: &mut BranchLocalState,
    fixture: &InheritedTimestampShadowFixture,
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    let child_put = storage_row_with(
        fixture.child,
        fixture.key.clone(),
        4,
        35,
        Timestamp::EPOCH,
        b"child-put".to_vec(),
    )?;
    child_state
        .append_committed_row(child_put.clone())
        .map_err(|err| TestkitError::new(format!("timestamp child put failed: {err}")))?;
    let put_read = child_state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("timestamp child put view failed: {err}")))?
        .read_point(
            &fixture.child_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp child put read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("timestamp child put missed row"))?;
    if put_read.row() != &child_put || put_read.source() != BranchRowSource::Active {
        return Err(TestkitError::new(
            "child-local put did not shadow inherited timestamp row",
        ));
    }
    outcome.inherited_timestamp_reads += 1;
    outcome.inherited_timestamp_point_reads += 1;
    outcome.inherited_timestamp_child_put_shadows += 1;
    Ok(())
}

fn check_inherited_timestamp_child_tombstone_shadow(
    child_state: &mut BranchLocalState,
    fixture: &InheritedTimestampShadowFixture,
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    child_state
        .append_committed_row(tombstone_row(fixture.child, fixture.key.clone(), 5, 45)?)
        .map_err(|err| TestkitError::new(format!("timestamp child tombstone failed: {err}")))?;
    if child_state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("timestamp child tombstone view failed: {err}")))?
        .read_point(
            &fixture.child_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(50)),
        )
        .map_err(|err| TestkitError::new(format!("timestamp child tombstone read failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new(
            "child-local tombstone did not shadow inherited timestamp row",
        ));
    }
    outcome.inherited_timestamp_reads += 1;
    outcome.inherited_timestamp_point_reads += 1;
    outcome.inherited_timestamp_child_tombstone_shadows += 1;
    Ok(())
}

fn check_pinned_timestamp_views(
    script: &[u8],
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 130));
    let mut state = BranchLocalState::empty(branch);
    let point = storage_row_with(
        branch,
        b"generated-pinned-ts".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        vec![script_byte(script, 131)],
    )?;
    let scan = storage_row_with(
        branch,
        b"generated-pinned-scan-a".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        vec![script_byte(script, 132)],
    )?;
    for row in [point.clone(), scan.clone()] {
        state
            .append_committed_row(row)
            .map_err(|err| TestkitError::new(format!("pinned timestamp append failed: {err}")))?;
    }
    let pinned = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("pinned timestamp view failed: {err}")))?;
    state
        .append_committed_row(storage_row_with(
            branch,
            b"generated-pinned-ts".to_vec(),
            3,
            30,
            Timestamp::EPOCH,
            b"later-point".to_vec(),
        )?)
        .map_err(|err| TestkitError::new(format!("later timestamp append failed: {err}")))?;
    state
        .append_committed_row(storage_row_with(
            branch,
            b"generated-pinned-scan-b".to_vec(),
            4,
            40,
            Timestamp::EPOCH,
            b"later-scan".to_vec(),
        )?)
        .map_err(|err| TestkitError::new(format!("later scan append failed: {err}")))?;
    match state.rotate_active() {
        BranchRotationOutcome::Rotated { .. } => {}
        outcome @ BranchRotationOutcome::Skipped { .. } => {
            return Err(TestkitError::new(format!(
                "pinned timestamp rotation skipped: {outcome:?}",
            )))
        }
    }
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "generated-pinned-owned",
            vec![storage_row_with(
                branch,
                b"generated-pinned-scan-c".to_vec(),
                5,
                50,
                Timestamp::EPOCH,
                b"owned".to_vec(),
            )?],
        )?)
        .map_err(|err| TestkitError::new(format!("pinned owned install failed: {err}")))?;

    let point_row = pinned
        .read_point(
            &physical_key(branch, b"generated-pinned-ts".to_vec())?,
            BranchReadBound::at_timestamp(Timestamp::from_micros(35)),
        )
        .map_err(|err| TestkitError::new(format!("pinned timestamp point failed: {err}")))?
        .ok_or_else(|| TestkitError::new("pinned timestamp point missed row"))?;
    if point_row.row() != &point {
        return Err(TestkitError::new(
            "pinned timestamp point saw later mutation",
        ));
    }
    let scan_rows = pinned
        .scan_prefix(
            &BranchScanBounds::prefix(&physical_key(branch, b"generated-pinned-scan-".to_vec())?),
            BranchReadBound::at_timestamp(Timestamp::from_micros(60)),
        )
        .map_err(|err| TestkitError::new(format!("pinned timestamp scan failed: {err}")))?;
    if visible_user_keys(&scan_rows) != vec![scan.physical_key().user_key().to_vec()] {
        return Err(TestkitError::new(
            "pinned timestamp scan saw later mutation",
        ));
    }
    outcome.pinned_timestamp_view_isolations += 1;
    Ok(())
}

fn check_timestamp_coverage(
    script: &[u8],
    outcome: &mut TimestampOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 123));
    let mut state = BranchLocalState::empty(branch);
    let row = storage_row_with(
        branch,
        b"generated-coverage".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        vec![script_byte(script, 124)],
    )?;
    state
        .append_committed_row(row.clone())
        .map_err(|err| TestkitError::new(format!("coverage append failed: {err}")))?;
    let key = physical_key(branch, b"generated-coverage".to_vec())?;
    let canonical = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("coverage canonical view failed: {err}")))?;
    match canonical.read_point(
        &key,
        BranchReadBound::at_timestamp(Timestamp::from_micros(49)),
    ) {
        Err(BranchRuntimeError::InsufficientTimestampHistory {
            branch_id,
            requested_timestamp,
            earliest_available_timestamp: Some(earliest),
            source: BranchTimestampHistorySource::Combined,
        }) if branch_id == branch
            && requested_timestamp == Timestamp::from_micros(49)
            && earliest == Timestamp::from_micros(50) => {}
        Err(err) => {
            return Err(TestkitError::new(format!(
                "canonical coverage returned wrong error: {err}",
            )))
        }
        Ok(_) => {
            return Err(TestkitError::new(
                "canonical coverage accepted insufficient timestamp",
            ))
        }
    }
    outcome.unknown_timestamp_coverage_reads += 1;

    let complete_since = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("coverage proof view failed: {err}")))?
        .with_timestamp_coverage(BranchTimestampCoverage::complete_since(
            Timestamp::from_micros(50),
        ));
    match complete_since.read_point(
        &key,
        BranchReadBound::at_timestamp(Timestamp::from_micros(49)),
    ) {
        Err(BranchRuntimeError::InsufficientTimestampHistory {
            branch_id,
            requested_timestamp,
            earliest_available_timestamp: Some(earliest),
            source: BranchTimestampHistorySource::Combined,
        }) if branch_id == branch
            && requested_timestamp == Timestamp::from_micros(49)
            && earliest == Timestamp::from_micros(50) => {}
        Err(err) => {
            return Err(TestkitError::new(format!(
                "coverage proof returned wrong error: {err}",
            )))
        }
        Ok(_) => {
            return Err(TestkitError::new(
                "coverage proof accepted insufficient timestamp",
            ))
        }
    }
    outcome.insufficient_timestamp_history_rejections += 1;

    let at_floor = complete_since
        .read_point(
            &key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(50)),
        )
        .map_err(|err| TestkitError::new(format!("coverage floor read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("coverage floor read missed row"))?;
    if at_floor.row() != &row {
        return Err(TestkitError::new("coverage floor read selected wrong row"));
    }
    outcome.timestamp_point_reads += 1;
    Ok(())
}

fn check_branch_materialization(script: &[u8]) -> Result<MaterializationOutcome, TestkitError> {
    let mut outcome = MaterializationOutcome::default();
    check_materialization_read_parity(script, &mut outcome)?;
    check_materialization_child_owned_immutable_collision(script, &mut outcome)?;
    check_materialization_tombstone_and_ttl(script, &mut outcome)?;
    check_materialization_empty_and_idempotent(script, &mut outcome)?;
    check_invalid_materialization_requests(script, &mut outcome)?;
    Ok(outcome)
}

#[allow(clippy::too_many_lines)]
fn check_materialization_read_parity(
    script: &[u8],
    outcome: &mut MaterializationOutcome,
) -> Result<(), TestkitError> {
    let source = branch_id(script_byte(script, 140));
    let child = branch_id(script_byte(script, 140).wrapping_add(1));
    let visible = storage_row_with(
        source,
        b"generated-materialize-a".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        vec![script_byte(script, 141)],
    )?;
    let historical = storage_row_with(
        source,
        b"generated-materialize-history".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        vec![script_byte(script, 142)],
    )?;
    let post_fork = storage_row_with(
        source,
        b"generated-materialize-post-fork".to_vec(),
        9,
        15,
        Timestamp::EPOCH,
        b"post-fork".to_vec(),
    )?;
    let exact_duplicate_source = storage_row_with(
        source,
        b"generated-materialize-duplicate".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"duplicate".to_vec(),
    )?;
    let same_internal_key_different_timestamp = storage_row_with(
        source,
        b"generated-materialize-same-key".to_vec(),
        4,
        30,
        Timestamp::EPOCH,
        b"inherited-timestamp".to_vec(),
    )?;
    let layer = branch_inherited_layer_unchecked_for_fork_gate_checks(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "generated-materialize-source",
            vec![
                visible.clone(),
                historical.clone(),
                post_fork,
                exact_duplicate_source.clone(),
                same_internal_key_different_timestamp.clone(),
            ],
        )?]],
    );
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .map_err(|err| TestkitError::new(format!("materialization attach failed: {err}")))?;
    let child_newer = storage_row_with(
        child,
        b"generated-materialize-history".to_vec(),
        7,
        70,
        Timestamp::EPOCH,
        b"child-newer".to_vec(),
    )?;
    child_state
        .append_committed_row(child_newer.clone())
        .map_err(|err| TestkitError::new(format!("materialization child append failed: {err}")))?;
    let exact_duplicate_child = rewrite_row_branch(&exact_duplicate_source, source, child)
        .map_err(|err| {
            TestkitError::new(format!("materialization duplicate rewrite failed: {err}"))
        })?;
    child_state
        .append_committed_row(exact_duplicate_child.clone())
        .map_err(|err| {
            TestkitError::new(format!("materialization duplicate append failed: {err}"))
        })?;
    let child_same_internal_key_later_timestamp = storage_row_with(
        child,
        b"generated-materialize-same-key".to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"child-timestamp".to_vec(),
    )?;
    child_state
        .append_committed_row(child_same_internal_key_later_timestamp.clone())
        .map_err(|err| {
            TestkitError::new(format!("materialization timestamp append failed: {err}"))
        })?;
    let visible_rewritten = rewrite_row_branch(&visible, source, child).map_err(|err| {
        TestkitError::new(format!("materialization visible rewrite failed: {err}"))
    })?;
    let historical_rewritten = rewrite_row_branch(&historical, source, child).map_err(|err| {
        TestkitError::new(format!("materialization historical rewrite failed: {err}"))
    })?;
    let same_key_rewritten =
        rewrite_row_branch(&same_internal_key_different_timestamp, source, child).map_err(
            |err| TestkitError::new(format!("materialization same-key rewrite failed: {err}")),
        )?;

    let before = child_state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("materialization before view failed: {err}")))?;
    let pinned = before.clone();
    let visible_key = physical_key(child, b"generated-materialize-a".to_vec())?;
    let history_key = physical_key(child, b"generated-materialize-history".to_vec())?;
    let timestamp_key = physical_key(child, b"generated-materialize-same-key".to_vec())?;
    let prefix =
        BranchScanBounds::prefix(&physical_key(child, b"generated-materialize-".to_vec())?);
    let range = BranchScanBounds::closed(
        &physical_key(child, b"generated-materialize-a".to_vec())?,
        &physical_key(child, b"generated-materialize-history".to_vec())?,
    )
    .map_err(|err| TestkitError::new(format!("materialization range bounds failed: {err}")))?;

    let before_latest = before
        .latest(&visible_key)
        .map_err(|err| TestkitError::new(format!("materialization before latest failed: {err}")))?
        .map(|row| row.row().clone());
    let before_version = before
        .at_version(&history_key, CommitVersion::new(2))
        .map_err(|err| TestkitError::new(format!("materialization before getv failed: {err}")))?
        .map(|row| row.row().clone());
    let before_timestamp = before
        .read_point(
            &timestamp_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .map_err(|err| TestkitError::new(format!("materialization before as-of failed: {err}")))?
        .map(|row| row.row().clone());
    let before_history_rows = before
        .history(
            &history_key,
            BranchHistoryOptions::all().include_tombstones(true),
        )
        .map_err(|err| TestkitError::new(format!("materialization before history failed: {err}")))?
        .into_iter()
        .map(|row| row.row().clone())
        .collect::<Vec<_>>();
    let before_prefix_keys = visible_user_keys(
        &before
            .scan_prefix(&prefix, BranchReadBound::latest())
            .map_err(|err| {
                TestkitError::new(format!("materialization before prefix failed: {err}"))
            })?,
    );
    let before_range_keys = visible_user_keys(
        &before
            .scan_range(&range, BranchReadBound::latest())
            .map_err(|err| {
                TestkitError::new(format!("materialization before range failed: {err}"))
            })?,
    );

    let materialization: BranchMaterializationOutcome = child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "generated-materialize").map_err(
                |err| TestkitError::new(format!("materialization request failed: {err}")),
            )?,
        )
        .map_err(|err| TestkitError::new(format!("materialization failed: {err}")))?;
    outcome.materialization_attempts += 1;
    outcome.successful_materializations += 1;
    outcome.materialized_rows += usize::try_from(materialization.rows_materialized())
        .map_err(|_| TestkitError::new("materialized row count did not fit usize"))?;
    outcome.materialized_tables += materialization.tables_created();
    outcome.skipped_materialization_post_fork_rows +=
        usize::try_from(materialization.skipped_post_fork_rows())
            .map_err(|_| TestkitError::new("skipped post-fork count did not fit usize"))?;
    outcome.skipped_materialization_exact_duplicates +=
        usize::try_from(materialization.skipped_exact_duplicate_rows())
            .map_err(|_| TestkitError::new("skipped duplicate count did not fit usize"))?;
    if materialization.recovery() != BranchMaterializationRecovery::ReplacementVisibleLayerRemoved
        || materialization.rows_materialized() != 3
        || materialization.skipped_post_fork_rows() != 1
        || materialization.skipped_exact_duplicate_rows() != 1
        || child_state.inherited_layer_count() != 0
    {
        return Err(TestkitError::new("materialization outcome facts drifted"));
    }

    let after = child_state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("materialization after view failed: {err}")))?;
    let mut model = ModelBranch::new(child);
    for row in [
        visible_rewritten,
        historical_rewritten,
        exact_duplicate_child,
        child_newer,
        same_key_rewritten,
        child_same_internal_key_later_timestamp,
    ] {
        model.push(row)?;
    }
    assert_materialization_model_matches(&after, &model, child)?;
    if after
        .latest(&visible_key)
        .map_err(|err| TestkitError::new(format!("materialization after latest failed: {err}")))?
        .map(|row| row.row().clone())
        != before_latest
    {
        return Err(TestkitError::new("materialization latest parity failed"));
    }
    outcome.materialization_latest_read_parity += 1;
    if after
        .at_version(&history_key, CommitVersion::new(2))
        .map_err(|err| TestkitError::new(format!("materialization after getv failed: {err}")))?
        .map(|row| row.row().clone())
        != before_version
    {
        return Err(TestkitError::new("materialization getv parity failed"));
    }
    outcome.materialization_version_read_parity += 1;
    if after
        .read_point(
            &timestamp_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .map_err(|err| TestkitError::new(format!("materialization after as-of failed: {err}")))?
        .map(|row| row.row().clone())
        != before_timestamp
    {
        return Err(TestkitError::new("materialization as-of parity failed"));
    }
    outcome.materialization_timestamp_read_parity += 1;
    let after_history_rows = after
        .history(
            &history_key,
            BranchHistoryOptions::all().include_tombstones(true),
        )
        .map_err(|err| TestkitError::new(format!("materialization after history failed: {err}")))?
        .into_iter()
        .map(|row| row.row().clone())
        .collect::<Vec<_>>();
    if after_history_rows != before_history_rows {
        return Err(TestkitError::new("materialization history parity failed"));
    }
    outcome.materialization_history_read_parity += 1;
    let after_prefix_keys = visible_user_keys(
        &after
            .scan_prefix(&prefix, BranchReadBound::latest())
            .map_err(|err| {
                TestkitError::new(format!("materialization after prefix failed: {err}"))
            })?,
    );
    if after_prefix_keys != before_prefix_keys {
        return Err(TestkitError::new("materialization prefix parity failed"));
    }
    outcome.materialization_prefix_scan_parity += 1;
    let after_range_keys = visible_user_keys(
        &after
            .scan_range(&range, BranchReadBound::latest())
            .map_err(|err| {
                TestkitError::new(format!("materialization after range failed: {err}"))
            })?,
    );
    if after_range_keys != before_range_keys {
        return Err(TestkitError::new("materialization range parity failed"));
    }
    outcome.materialization_range_scan_parity += 1;

    let pinned_row = pinned
        .latest(&visible_key)
        .map_err(|err| TestkitError::new(format!("materialization pinned read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("materialization pinned read missed row"))?;
    if !matches!(pinned_row.source(), BranchRowSource::Inherited { .. }) {
        return Err(TestkitError::new(
            "materialization pinned view lost inherited source",
        ));
    }
    outcome.materialization_pinned_view_isolations += 1;
    Ok(())
}

fn check_materialization_child_owned_immutable_collision(
    script: &[u8],
    outcome: &mut MaterializationOutcome,
) -> Result<(), TestkitError> {
    let source = branch_id(script_byte(script, 146));
    let child = branch_id(script_byte(script, 146).wrapping_add(1));
    let inherited = storage_row_with(
        source,
        b"generated-materialize-owned-collision".to_vec(),
        4,
        40,
        Timestamp::EPOCH,
        b"inherited".to_vec(),
    )?;
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(4),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "generated-materialize-owned-collision-source",
            vec![inherited],
        )?]],
    )?;
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .map_err(|err| {
            TestkitError::new(format!(
                "materialization owned collision attach failed: {err}"
            ))
        })?;
    child_state
        .install_l0_table(branch_owned_table(
            child,
            BranchLevel::ZERO,
            "generated-materialize-owned-collision-child",
            vec![storage_row_with(
                child,
                b"generated-materialize-owned-collision".to_vec(),
                4,
                45,
                Timestamp::EPOCH,
                b"child".to_vec(),
            )?],
        )?)
        .map_err(|err| {
            TestkitError::new(format!(
                "materialization owned collision child install failed: {err}"
            ))
        })?;
    let before = child_state.clone();

    expect_invalid_inherited_layer(
        child_state.materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "generated-materialize-owned-collision")
                .map_err(|err| {
                TestkitError::new(format!(
                    "materialization owned collision request failed: {err}"
                ))
            })?,
        ),
    )?;
    if child_state != before {
        return Err(TestkitError::new(
            "materialization owned collision mutated state",
        ));
    }
    outcome.invalid_materialization_rejections += 1;
    Ok(())
}

fn assert_materialization_model_matches(
    view: &BranchReadView,
    model: &ModelBranch,
    branch: BranchId,
) -> Result<(), TestkitError> {
    for (key, bound, label) in [
        (
            physical_key(branch, b"generated-materialize-a".to_vec())?,
            BranchReadBound::latest(),
            "latest",
        ),
        (
            physical_key(branch, b"generated-materialize-history".to_vec())?,
            BranchReadBound::latest(),
            "history latest",
        ),
        (
            physical_key(branch, b"generated-materialize-history".to_vec())?,
            BranchReadBound::at_version(CommitVersion::new(2)),
            "history version",
        ),
        (
            physical_key(branch, b"generated-materialize-same-key".to_vec())?,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
            "timestamp",
        ),
    ] {
        assert_model_point(view, model, &key, bound, label)?;
    }

    let history_key = physical_key(branch, b"generated-materialize-history".to_vec())?;
    let actual_history = view
        .history(
            &history_key,
            BranchHistoryOptions::all().include_tombstones(true),
        )
        .map_err(|err| TestkitError::new(format!("materialization model history failed: {err}")))?
        .into_iter()
        .map(|row| row.row().clone())
        .collect::<Vec<_>>();
    if actual_history != model.history(&history_key) {
        return Err(TestkitError::new("materialization model history mismatch"));
    }

    let prefix =
        BranchScanBounds::prefix(&physical_key(branch, b"generated-materialize-".to_vec())?);
    let actual_prefix = view
        .scan_prefix(&prefix, BranchReadBound::latest())
        .map_err(|err| TestkitError::new(format!("materialization model prefix failed: {err}")))?
        .into_iter()
        .map(|row| row.row().clone())
        .collect::<Vec<_>>();
    let expected_prefix = materialization_model_scan(
        model,
        b"generated-materialize-",
        None,
        BranchReadBound::latest(),
    );
    if actual_prefix != expected_prefix {
        return Err(TestkitError::new("materialization model prefix mismatch"));
    }

    let lower = b"generated-materialize-a".as_slice();
    let upper = b"generated-materialize-history".as_slice();
    let range = BranchScanBounds::closed(
        &physical_key(branch, lower.to_vec())?,
        &physical_key(branch, upper.to_vec())?,
    )
    .map_err(|err| {
        TestkitError::new(format!("materialization model range bounds failed: {err}"))
    })?;
    let actual_range = view
        .scan_range(&range, BranchReadBound::latest())
        .map_err(|err| TestkitError::new(format!("materialization model range failed: {err}")))?
        .into_iter()
        .map(|row| row.row().clone())
        .collect::<Vec<_>>();
    let expected_range =
        materialization_model_scan(model, lower, Some(upper), BranchReadBound::latest());
    if actual_range != expected_range {
        return Err(TestkitError::new("materialization model range mismatch"));
    }
    Ok(())
}

fn materialization_model_scan(
    model: &ModelBranch,
    lower_or_prefix: &[u8],
    upper: Option<&[u8]>,
    bound: BranchReadBound,
) -> Vec<StorageRow> {
    let mut keys = Vec::<PhysicalKey>::new();
    for row in &model.rows {
        let user_key = row.physical_key().user_key();
        let matches = if let Some(upper) = upper {
            lower_or_prefix <= user_key && user_key <= upper
        } else {
            user_key.starts_with(lower_or_prefix)
        };
        if matches && !keys.iter().any(|key| key == row.physical_key()) {
            keys.push(row.physical_key().clone());
        }
    }
    keys.sort_by(|left, right| left.user_key().cmp(right.user_key()));

    let mut rows = Vec::new();
    for key in keys {
        if let Some(row) = model.visible(&key, bound) {
            rows.push(row);
        }
    }
    rows
}

fn check_materialization_tombstone_and_ttl(
    script: &[u8],
    outcome: &mut MaterializationOutcome,
) -> Result<(), TestkitError> {
    let source = branch_id(script_byte(script, 148));
    let child = branch_id(script_byte(script, 148).wrapping_add(1));
    let expired = storage_row_with(
        source,
        b"generated-materialize-expired".to_vec(),
        2,
        20,
        Timestamp::from_micros(25),
        vec![script_byte(script, 149)],
    )?;
    let deleted_put = storage_row_with(
        source,
        b"generated-materialize-deleted".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        vec![script_byte(script, 150)],
    )?;
    let deleting_tombstone =
        tombstone_row(source, b"generated-materialize-deleted".to_vec(), 3, 30)?;
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(3),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "generated-materialize-ttl-source",
            vec![expired.clone(), deleted_put, deleting_tombstone],
        )?]],
    )?;
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![layer])
        .map_err(|err| TestkitError::new(format!("materialization ttl attach failed: {err}")))?;
    let materialization = child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "generated-materialize-ttl").map_err(
                |err| TestkitError::new(format!("materialization ttl request failed: {err}")),
            )?,
        )
        .map_err(|err| TestkitError::new(format!("materialization ttl failed: {err}")))?;
    outcome.materialization_attempts += 1;
    outcome.successful_materializations += 1;
    outcome.materialized_rows += usize::try_from(materialization.rows_materialized())
        .map_err(|_| TestkitError::new("ttl materialized rows did not fit usize"))?;
    outcome.materialized_tables += materialization.tables_created();

    let view = child_state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("materialization ttl view failed: {err}")))?;
    check_materialized_ttl_preserved(&view, &expired, source, child, outcome)?;
    check_materialized_tombstone_preserved(&view, child, outcome)
}

fn check_materialized_ttl_preserved(
    view: &BranchReadView,
    expired: &StorageRow,
    source: BranchId,
    child: BranchId,
    outcome: &mut MaterializationOutcome,
) -> Result<(), TestkitError> {
    let expired_key = physical_key(child, b"generated-materialize-expired".to_vec())?;
    let before_expiry = view
        .read_point(
            &expired_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(24)),
        )
        .map_err(|err| TestkitError::new(format!("materialization ttl before read failed: {err}")))?
        .ok_or_else(|| TestkitError::new("materialization ttl row missing before expiry"))?;
    let expected_expired = rewrite_row_branch(expired, source, child)
        .map_err(|err| TestkitError::new(format!("materialization ttl rewrite failed: {err}")))?;
    if before_expiry.row() != &expected_expired {
        return Err(TestkitError::new(
            "materialization ttl changed expired row facts",
        ));
    }
    if !matches!(
        before_expiry.source(),
        BranchRowSource::OwnedTable {
            level: BranchLevel::ZERO,
            ..
        }
    ) {
        return Err(TestkitError::new(
            "materialization ttl row did not move to owned table",
        ));
    }
    if view
        .read_point(
            &expired_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(25)),
        )
        .map_err(|err| TestkitError::new(format!("materialization ttl exact read failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new(
            "materialization ttl failed to suppress at expiry",
        ));
    }
    outcome.materialization_ttl_preservations += 1;
    Ok(())
}

fn check_materialized_tombstone_preserved(
    view: &BranchReadView,
    child: BranchId,
    outcome: &mut MaterializationOutcome,
) -> Result<(), TestkitError> {
    let deleted_key = physical_key(child, b"generated-materialize-deleted".to_vec())?;
    if view
        .read_point(
            &deleted_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(40)),
        )
        .map_err(|err| TestkitError::new(format!("materialization tombstone read failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new(
            "materialization tombstone failed to suppress put",
        ));
    }
    let history_rows = view
        .history(
            &deleted_key,
            BranchHistoryOptions::all().include_tombstones(true),
        )
        .map_err(|err| {
            TestkitError::new(format!("materialization tombstone history failed: {err}"))
        })?;
    if history_versions(&history_rows) != vec![3, 1] {
        return Err(TestkitError::new(
            "materialization tombstone history drifted",
        ));
    }
    if history_rows.iter().any(|row| {
        !matches!(
            row.source(),
            BranchRowSource::OwnedTable {
                level: BranchLevel::ZERO,
                ..
            }
        )
    }) {
        return Err(TestkitError::new(
            "materialization tombstone history did not move to owned table",
        ));
    }
    outcome.materialization_tombstone_preservations += 1;
    Ok(())
}

fn check_materialization_empty_and_idempotent(
    script: &[u8],
    outcome: &mut MaterializationOutcome,
) -> Result<(), TestkitError> {
    let source = branch_id(script_byte(script, 145));
    let child = branch_id(script_byte(script, 145).wrapping_add(1));
    let empty_layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        Vec::new(),
    )?;
    let mut empty_state = BranchLocalState::empty(child);
    empty_state
        .attach_inherited_layers(vec![empty_layer])
        .map_err(|err| TestkitError::new(format!("empty materialization attach failed: {err}")))?;
    let empty = empty_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "generated-materialize-empty").map_err(
                |err| TestkitError::new(format!("empty materialization request failed: {err}")),
            )?,
        )
        .map_err(|err| TestkitError::new(format!("empty materialization failed: {err}")))?;
    outcome.materialization_attempts += 1;
    outcome.successful_materializations += 1;
    if empty.rows_materialized() != 0
        || empty.tables_created() != 0
        || empty_state.inherited_layer_count() != 0
    {
        return Err(TestkitError::new("empty materialization drifted"));
    }
    outcome.empty_materializations += 1;

    let materialized_layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Materialized,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "generated-materialize-already",
            vec![storage_row_with(
                source,
                b"generated-materialized-stale".to_vec(),
                5,
                50,
                Timestamp::EPOCH,
                vec![script_byte(script, 146)],
            )?],
        )?]],
    )?;
    let mut materialized_state = BranchLocalState::empty(child);
    materialized_state
        .attach_inherited_layers(vec![materialized_layer])
        .map_err(|err| {
            TestkitError::new(format!("idempotent materialization attach failed: {err}"))
        })?;
    let before = materialized_state.clone();
    let retry = materialized_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "generated-materialize-retry").map_err(
                |err| TestkitError::new(format!("retry materialization request failed: {err}")),
            )?,
        )
        .map_err(|err| TestkitError::new(format!("retry materialization failed: {err}")))?;
    outcome.materialization_attempts += 1;
    if retry.recovery() != BranchMaterializationRecovery::LayerAlreadyMaterialized
        || retry.rows_materialized() != 0
        || materialized_state != before
    {
        return Err(TestkitError::new("idempotent materialization drifted"));
    }
    outcome.idempotent_materialization_retries += 1;
    Ok(())
}

fn check_invalid_materialization_requests(
    script: &[u8],
    outcome: &mut MaterializationOutcome,
) -> Result<(), TestkitError> {
    let source = branch_id(script_byte(script, 147));
    let child = branch_id(script_byte(script, 147).wrapping_add(1));
    let layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        Vec::new(),
    )?;
    let mut state = BranchLocalState::empty(child);
    state.attach_inherited_layers(vec![layer]).map_err(|err| {
        TestkitError::new(format!("invalid materialization attach failed: {err}"))
    })?;
    match state.materialize_inherited_layer(
        &BranchMaterializationRequest::new(source, 0, "generated-materialize-wrong-branch")
            .map_err(|err| TestkitError::new(format!("wrong-branch request failed: {err}")))?,
    ) {
        Err(BranchRuntimeError::InvalidBranchState { .. }) => {}
        Err(err) => {
            return Err(TestkitError::new(format!(
                "wrong-branch materialization returned wrong error: {err}",
            )))
        }
        Ok(_) => return Err(TestkitError::new("wrong-branch materialization succeeded")),
    }
    outcome.materialization_attempts += 1;
    outcome.invalid_materialization_rejections += 1;

    expect_invalid_config(BranchMaterializationRequest::new(
        child,
        0,
        "generated/materialize",
    ))?;
    outcome.invalid_materialization_rejections += 1;
    Ok(())
}

fn check_branch_reachability(script: &[u8]) -> Result<ReachabilityOutcome, TestkitError> {
    let mut outcome = ReachabilityOutcome::default();
    check_reachability_fact_model(script, &mut outcome)?;
    check_fork_reachability_registry_and_release(script, &mut outcome)?;
    check_materialization_reachability_release(script, &mut outcome)?;
    check_branch_clear_reachability_release(script, &mut outcome)?;
    Ok(outcome)
}

#[allow(clippy::similar_names, clippy::too_many_lines)]
fn check_reachability_fact_model(
    script: &[u8],
    outcome: &mut ReachabilityOutcome,
) -> Result<(), TestkitError> {
    let owner = branch_id(script_byte(script, 151));
    let source = branch_id(script_byte(script, 151).wrapping_add(1));
    let owned = BranchTableRef::owned(
        owner,
        BranchLevel::new(1),
        2,
        table_identity("generated-reach-owned")?,
    )
    .map_err(|err| TestkitError::new(format!("owned reachability ref failed: {err}")))?;
    let inherited = BranchTableRef::inherited(
        owner,
        source,
        CommitVersion::new(5),
        0,
        BranchLevel::ZERO,
        0,
        table_identity("generated-reach-inherited")?,
    )
    .map_err(|err| TestkitError::new(format!("inherited reachability ref failed: {err}")))?;
    let materializing = BranchTableRef::materializing_source(
        owner,
        source,
        CommitVersion::new(5),
        1,
        BranchLevel::ZERO,
        1,
        table_identity("generated-reach-materializing")?,
    )
    .map_err(|err| TestkitError::new(format!("materializing reachability ref failed: {err}")))?;
    let replacement = BranchTableRef::replacement(
        owner,
        source,
        CommitVersion::new(5),
        BranchLevel::ZERO,
        2,
        table_identity("generated-reach-replacement")?,
    )
    .map_err(|err| TestkitError::new(format!("replacement reachability ref failed: {err}")))?;
    let snapshot = BranchReachabilitySnapshot::new(
        owner,
        vec![
            replacement.clone(),
            materializing.clone(),
            inherited.clone(),
            owned.clone(),
        ],
    )
    .map_err(|err| TestkitError::new(format!("reachability snapshot failed: {err}")))?;
    if snapshot.facts().owned_table_count() != 2
        || snapshot.facts().inherited_table_count() != 2
        || snapshot.facts().reachable_table_count() != 4
        || snapshot.protected_table_count() != 4
    {
        return Err(TestkitError::new("reachability facts drifted"));
    }
    if snapshot
        .table_refs()
        .iter()
        .map(|table_ref| table_ref.table_identity().as_str())
        .collect::<Vec<_>>()
        != vec![
            "generated-reach-inherited",
            "generated-reach-materializing",
            "generated-reach-owned",
            "generated-reach-replacement",
        ]
    {
        return Err(TestkitError::new(
            "reachability snapshot order was nondeterministic",
        ));
    }
    outcome.reachability_snapshots += 1;
    outcome.reachability_owned_refs += 2;
    outcome.reachability_inherited_refs += 2;
    outcome.materializing_reachability_refs += 1;
    outcome.reachability_deterministic_orderings += 1;

    let aggregate = BranchReachabilityAggregate::from_snapshots(std::slice::from_ref(&snapshot))
        .map_err(|err| TestkitError::new(format!("reachability aggregate failed: {err}")))?;
    if aggregate.branch_count() != 1
        || aggregate.table_count() != 4
        || aggregate.reference_count_for(owned.table_identity()) != 1
    {
        return Err(TestkitError::new("single-branch aggregate facts drifted"));
    }
    if !aggregate
        .table_protections()
        .iter()
        .all(|protection| protection.reference_count() == 1 && protection.table_refs().len() == 1)
    {
        return Err(TestkitError::new("aggregate protection refs drifted"));
    }
    outcome.reachability_aggregate_rebuilds += 1;

    expect_invalid_reachability(BranchTableRef::inherited(
        owner,
        owner,
        CommitVersion::new(1),
        0,
        BranchLevel::ZERO,
        0,
        table_identity("generated-reach-invalid-same-branch")?,
    ))?;
    expect_invalid_reachability(BranchReachabilitySnapshot::new(
        owner,
        vec![inherited.clone(), inherited],
    ))?;
    expect_invalid_reachability(BranchReachabilityAggregate::from_snapshots(&[
        snapshot.clone(),
        snapshot,
    ]))?;
    outcome.invalid_reachability_rejections += 3;
    Ok(())
}

#[allow(clippy::similar_names, clippy::too_many_lines)]
fn check_fork_reachability_registry_and_release(
    script: &[u8],
    outcome: &mut ReachabilityOutcome,
) -> Result<(), TestkitError> {
    let parent = branch_id(script_byte(script, 152));
    let child_a = branch_id(script_byte(script, 152).wrapping_add(1));
    let child_b = branch_id(script_byte(script, 152).wrapping_add(2));
    let mut parent_state = BranchLocalState::empty(parent);
    parent_state
        .install_l0_table(branch_owned_table(
            parent,
            BranchLevel::ZERO,
            "generated-reach-shared-parent",
            vec![storage_row_with(
                parent,
                b"generated-reach-shared".to_vec(),
                3,
                30,
                Timestamp::EPOCH,
                vec![script_byte(script, 153)],
            )?],
        )?)
        .map_err(|err| TestkitError::new(format!("shared parent install failed: {err}")))?;
    let (child_a_state, _) = parent_state
        .fork_into_empty_child(child_a)
        .map_err(|err| TestkitError::new(format!("reachability fork a failed: {err}")))?;
    let (child_b_state, _) = parent_state
        .fork_into_empty_child(child_b)
        .map_err(|err| TestkitError::new(format!("reachability fork b failed: {err}")))?;
    outcome.fork_reachability_cases += 2;

    let parent_snapshot = parent_state
        .reachability_snapshot()
        .map_err(|err| TestkitError::new(format!("parent reachability failed: {err}")))?;
    let child_a_snapshot = child_a_state
        .reachability_snapshot()
        .map_err(|err| TestkitError::new(format!("child a reachability failed: {err}")))?;
    let child_b_snapshot = child_b_state
        .reachability_snapshot()
        .map_err(|err| TestkitError::new(format!("child b reachability failed: {err}")))?;
    let table_identity = parent_snapshot.table_refs()[0].table_identity().clone();
    outcome.reachability_snapshots += 3;
    outcome.reachability_owned_refs += parent_snapshot.facts().owned_table_count();
    outcome.reachability_inherited_refs += child_a_snapshot.facts().inherited_table_count()
        + child_b_snapshot.facts().inherited_table_count();

    let aggregate = BranchReachabilityAggregate::from_snapshots(&[
        parent_snapshot.clone(),
        child_a_snapshot.clone(),
        child_b_snapshot.clone(),
    ])
    .map_err(|err| TestkitError::new(format!("fork aggregate failed: {err}")))?;
    if aggregate.reference_count_for(&table_identity) != 3
        || !aggregate.is_reachable(&table_identity)
        || !aggregate.is_shared(&table_identity)
    {
        return Err(TestkitError::new("shared fork aggregate facts drifted"));
    }
    outcome.reachability_aggregate_rebuilds += 1;
    outcome.shared_table_detections += 1;

    let mut registry = SharedTableRegistry::rebuild_from_snapshots(&[
        parent_snapshot.clone(),
        child_a_snapshot.clone(),
        child_b_snapshot.clone(),
    ])
    .map_err(|err| TestkitError::new(format!("registry rebuild failed: {err}")))?;
    if registry.table_count() != 1
        || registry.reference_count(&table_identity) != 3
        || !registry.is_runtime_referenced(&table_identity)
    {
        return Err(TestkitError::new("registry rebuild facts drifted"));
    }
    outcome.registry_rebuilds += 1;

    let registry_before_failed_fork = registry.clone();
    if !matches!(
        parent_state.fork_into_empty_child(parent),
        Err(BranchRuntimeError::InvalidInheritedLayer { .. })
    ) {
        return Err(TestkitError::new(
            "same-branch fork did not reject before reachability publication",
        ));
    }
    if registry != registry_before_failed_fork {
        return Err(TestkitError::new(
            "failed fork mutated reachability registry",
        ));
    }
    outcome.failed_fork_reachability_rollbacks += 1;

    registry
        .unregister_snapshot(&child_a_snapshot)
        .map_err(|err| TestkitError::new(format!("child a unregister failed: {err}")))?;
    if registry.reference_count(&table_identity) != 2 {
        return Err(TestkitError::new("registry unregister count drifted"));
    }
    outcome.registry_unregisters += 1;

    let aggregate_after_child_a = BranchReachabilityAggregate::from_snapshots(&[
        parent_snapshot.clone(),
        child_b_snapshot.clone(),
    ])
    .map_err(|err| TestkitError::new(format!("post-release aggregate failed: {err}")))?;
    outcome.reachability_aggregate_rebuilds += 1;
    let protected = BranchReleasePlan::from_removed_refs(
        child_a,
        child_a_snapshot.table_refs().to_vec(),
        &aggregate_after_child_a,
        Some(&registry),
    )
    .map_err(|err| TestkitError::new(format!("shared release plan failed: {err}")))?;
    if !protected.releasable_tables().is_empty()
        || protected.protected_tables().len() != 1
        || protected.protected_tables()[0].reason() != BranchProtectionReason::StillReachable
    {
        return Err(TestkitError::new("shared release protection drifted"));
    }
    outcome.protected_release_attempts += 1;

    let durable_only_protected = BranchReleasePlan::from_removed_refs(
        child_b,
        child_b_snapshot.table_refs().to_vec(),
        &aggregate_after_child_a,
        None,
    )
    .map_err(|err| TestkitError::new(format!("durable-only release plan failed: {err}")))?;
    if durable_only_protected.protected_tables()[0].reason()
        != BranchProtectionReason::StillReachable
    {
        return Err(TestkitError::new(
            "missing runtime registry was misclassified as disagreement",
        ));
    }
    outcome.protected_release_attempts += 1;

    let empty_registry = SharedTableRegistry::new();
    let releasable = BranchReleasePlan::from_removed_refs(
        child_b,
        child_b_snapshot.table_refs().to_vec(),
        &BranchReachabilityAggregate::empty(),
        Some(&empty_registry),
    )
    .map_err(|err| TestkitError::new(format!("final release plan failed: {err}")))?;
    if releasable.released_branch_id() != child_b
        || releasable.removed_refs().len() != 1
        || releasable.releasable_tables().len() != 1
        || !releasable.protected_tables().is_empty()
    {
        return Err(TestkitError::new("final release candidate drifted"));
    }
    outcome.reachability_release_candidates += 1;

    let runtime_protected = BranchReleasePlan::from_removed_refs(
        child_b,
        child_b_snapshot.table_refs().to_vec(),
        &BranchReachabilityAggregate::empty(),
        Some(&registry),
    )
    .map_err(|err| TestkitError::new(format!("runtime-protected plan failed: {err}")))?;
    if runtime_protected.protected_tables()[0].reason() != BranchProtectionReason::RuntimeReferenced
    {
        return Err(TestkitError::new(
            "runtime registry protection reason drifted",
        ));
    }
    outcome.protected_release_attempts += 1;

    let disagreement = BranchReleasePlan::from_removed_refs(
        child_b,
        child_b_snapshot.table_refs().to_vec(),
        &aggregate_after_child_a,
        Some(&empty_registry),
    )
    .map_err(|err| TestkitError::new(format!("disagreement plan failed: {err}")))?;
    if disagreement.protected_tables()[0].reason() != BranchProtectionReason::RegistryDisagreement {
        return Err(TestkitError::new("registry disagreement reason drifted"));
    }
    outcome.protected_release_attempts += 1;
    outcome.registry_disagreements += 1;

    let mut count_mismatch_registry = SharedTableRegistry::new();
    count_mismatch_registry
        .register_snapshot(&child_b_snapshot)
        .map_err(|err| TestkitError::new(format!("mismatch registry register failed: {err}")))?;
    let count_mismatch = BranchReleasePlan::from_removed_refs(
        child_a,
        child_a_snapshot.table_refs().to_vec(),
        &aggregate_after_child_a,
        Some(&count_mismatch_registry),
    )
    .map_err(|err| TestkitError::new(format!("count-mismatch release plan failed: {err}")))?;
    if count_mismatch.protected_tables()[0].reason() != BranchProtectionReason::RegistryDisagreement
    {
        return Err(TestkitError::new(
            "registry count mismatch was not reported as disagreement",
        ));
    }
    outcome.protected_release_attempts += 1;
    outcome.registry_disagreements += 1;

    let mut replacement_registry = SharedTableRegistry::rebuild_from_snapshots(&[
        parent_snapshot,
        child_b_snapshot.clone(),
    ])
    .map_err(|err| TestkitError::new(format!("replacement registry rebuild failed: {err}")))?;
    replacement_registry
        .replace_snapshot(&BranchReachabilitySnapshot::empty(child_b))
        .map_err(|err| TestkitError::new(format!("registry snapshot replacement failed: {err}")))?;
    if replacement_registry.reference_count(&table_identity) != 1 {
        return Err(TestkitError::new("registry replacement count drifted"));
    }
    expect_invalid_reachability(replacement_registry.replace_snapshot(&child_a_snapshot))?;

    expect_invalid_reachability(registry.unregister_snapshot(&child_a_snapshot))?;
    outcome.invalid_reachability_rejections += 2;
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn check_materialization_reachability_release(
    script: &[u8],
    outcome: &mut ReachabilityOutcome,
) -> Result<(), TestkitError> {
    let source = branch_id(script_byte(script, 154));
    let child = branch_id(script_byte(script, 154).wrapping_add(1));
    let source_table = branch_owned_table(
        source,
        BranchLevel::ZERO,
        "generated-reach-materialize-source",
        vec![storage_row_with(
            source,
            b"generated-reach-materialize".to_vec(),
            4,
            40,
            Timestamp::EPOCH,
            vec![script_byte(script, 155)],
        )?],
    )?;
    let mut child_state = BranchLocalState::empty(child);
    child_state
        .attach_inherited_layers(vec![branch_inherited_layer(
            source,
            CommitVersion::new(4),
            InheritedLayerStatus::Active,
            vec![vec![source_table.clone()]],
        )?])
        .map_err(|err| {
            TestkitError::new(format!("reachability materialize attach failed: {err}"))
        })?;
    let before = child_state
        .reachability_snapshot()
        .map_err(|err| TestkitError::new(format!("pre-materialize reachability failed: {err}")))?;
    if before.facts().inherited_table_count() != 1 {
        return Err(TestkitError::new(
            "pre-materialization reachability missed source table",
        ));
    }
    outcome.reachability_snapshots += 1;
    outcome.reachability_inherited_refs += 1;

    let mut in_flight = BranchLocalState::empty(child);
    in_flight
        .attach_inherited_layers(vec![branch_inherited_layer(
            source,
            CommitVersion::new(4),
            InheritedLayerStatus::Materializing,
            vec![vec![source_table]],
        )?])
        .map_err(|err| TestkitError::new(format!("materializing attach failed: {err}")))?;
    let in_flight_snapshot = in_flight
        .reachability_snapshot()
        .map_err(|err| TestkitError::new(format!("materializing snapshot failed: {err}")))?;
    if !matches!(
        in_flight_snapshot.table_refs()[0].reference_kind(),
        BranchTableReferenceKind::MaterializingSource { .. }
    ) {
        return Err(TestkitError::new(
            "materializing layer did not retain source reachability",
        ));
    }
    outcome.reachability_snapshots += 1;
    outcome.reachability_inherited_refs += 1;
    outcome.materializing_reachability_refs += 1;

    child_state
        .materialize_inherited_layer(
            &BranchMaterializationRequest::new(child, 0, "generated-reach-materialized").map_err(
                |err| TestkitError::new(format!("materialization request failed: {err}")),
            )?,
        )
        .map_err(|err| TestkitError::new(format!("reachability materialization failed: {err}")))?;
    let after = child_state
        .reachability_snapshot()
        .map_err(|err| TestkitError::new(format!("post-materialize reachability failed: {err}")))?;
    if after.facts().owned_table_count() != 1 || after.facts().inherited_table_count() != 0 {
        return Err(TestkitError::new(
            "post-materialization replacement reachability drifted",
        ));
    }
    if !matches!(
        after.table_refs()[0].reference_kind(),
        BranchTableReferenceKind::Replacement {
            source_branch_id,
            fork_version,
        } if source_branch_id == source && fork_version == CommitVersion::new(4)
    ) {
        return Err(TestkitError::new(
            "materialized table reachability did not preserve replacement provenance",
        ));
    }
    outcome.reachability_snapshots += 1;
    outcome.reachability_owned_refs += 1;

    let aggregate_after = BranchReachabilityAggregate::from_snapshots(&[after])
        .map_err(|err| TestkitError::new(format!("materialized aggregate failed: {err}")))?;
    let release = BranchReleasePlan::from_removed_refs(
        child,
        before.table_refs().to_vec(),
        &aggregate_after,
        Some(&SharedTableRegistry::new()),
    )
    .map_err(|err| TestkitError::new(format!("materialized release failed: {err}")))?;
    if release.releasable_tables().len() != 1 || !release.protected_tables().is_empty() {
        return Err(TestkitError::new(
            "materialization removed-source release facts drifted",
        ));
    }
    outcome.reachability_aggregate_rebuilds += 1;
    outcome.materialization_release_cases += 1;
    outcome.reachability_release_candidates += 1;
    Ok(())
}

fn check_branch_clear_reachability_release(
    script: &[u8],
    outcome: &mut ReachabilityOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 156));
    let mut state = BranchLocalState::empty(branch);
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "generated-reach-clear-a",
            vec![storage_row_with(
                branch,
                b"generated-reach-clear-a".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                vec![script_byte(script, 157)],
            )?],
        )?)
        .map_err(|err| TestkitError::new(format!("clear table a failed: {err}")))?;
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "generated-reach-clear-b",
            vec![storage_row_with(
                branch,
                b"generated-reach-clear-b".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                vec![script_byte(script, 158)],
            )?],
        )?)
        .map_err(|err| TestkitError::new(format!("clear table b failed: {err}")))?;
    let snapshot = state
        .reachability_snapshot()
        .map_err(|err| TestkitError::new(format!("clear reachability failed: {err}")))?;
    if snapshot.facts().owned_table_count() != 2 || snapshot.protected_table_count() != 2 {
        return Err(TestkitError::new("clear snapshot reachability drifted"));
    }
    outcome.reachability_snapshots += 1;
    outcome.reachability_owned_refs += 2;

    let release = BranchReleasePlan::from_removed_refs(
        branch,
        snapshot.table_refs().to_vec(),
        &BranchReachabilityAggregate::empty(),
        Some(&SharedTableRegistry::new()),
    )
    .map_err(|err| TestkitError::new(format!("clear release failed: {err}")))?;
    if release.releasable_tables().len() != 2 || !release.protected_tables().is_empty() {
        return Err(TestkitError::new("clear release facts drifted"));
    }
    outcome.branch_clear_release_cases += 1;
    outcome.reachability_release_candidates += 2;
    Ok(())
}

fn check_branch_compaction(script: &[u8]) -> Result<CompactionOutcome, TestkitError> {
    let mut outcome = CompactionOutcome::default();
    check_compaction_noops_and_invalid_requests(script, &mut outcome)?;
    check_compaction_pruning_rejections(script, &mut outcome)?;
    check_l0_keep_all_compaction_parity_and_release(script, &mut outcome)?;
    check_compaction_output_splitting(script, &mut outcome)?;
    check_l0_to_l1_compaction_candidate(script, &mut outcome)?;
    check_nonzero_level_compaction_candidate(script, &mut outcome)?;
    check_stale_compaction_plan_rejection(script, &mut outcome)?;
    Ok(outcome)
}

fn check_compaction_noops_and_invalid_requests(
    script: &[u8],
    outcome: &mut CompactionOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 160));
    let mut state = BranchLocalState::new(branch, compaction_runtime_config()?)
        .map_err(|err| TestkitError::new(format!("compaction empty state failed: {err}")))?;
    let request = branch_compaction_request(
        branch,
        BranchCompactionKind::CompactL0,
        "generated-compact-empty",
    )?;
    let no_candidate = state
        .compact_branch_owned_tables(&request)
        .map_err(|err| TestkitError::new(format!("empty compaction failed: {err}")))?;
    if no_candidate.recovery()
        != (BranchCompactionRecovery::NoCandidate {
            reason: BranchCompactionNoopReason::EmptyInputLevel,
        })
        || !no_candidate.output_refs().is_empty()
        || !no_candidate.removed_refs().is_empty()
    {
        return Err(TestkitError::new("empty compaction noop facts drifted"));
    }
    outcome.compaction_noop_cases += 1;

    let wrong_branch = branch_id(script_byte(script, 160).wrapping_add(1));
    expect_invalid_compaction(state.plan_branch_compaction(&branch_compaction_request(
        wrong_branch,
        BranchCompactionKind::CompactL0,
        "generated-compact-wrong-branch",
    )?))?;
    outcome.invalid_compaction_request_rejection_cases += 1;
    Ok(())
}

fn check_compaction_pruning_rejections(
    script: &[u8],
    outcome: &mut CompactionOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 161));
    let mut state = compaction_state_with_l0_tables(
        branch,
        "generated-prune",
        vec![
            vec![storage_row_with(
                branch,
                b"generated-prune-a".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                vec![script_byte(script, 162)],
            )?],
            vec![tombstone_row(branch, b"generated-prune-b".to_vec(), 3, 30)?],
        ],
    )?;
    let before = state.clone();

    for (policy, counter) in [
        (
            BranchCompactionRetentionPolicy::DropOlderVersions,
            &mut outcome.unsafe_old_version_pruning_rejection_cases,
        ),
        (
            BranchCompactionRetentionPolicy::DropTombstones,
            &mut outcome.unsafe_tombstone_pruning_rejection_cases,
        ),
        (
            BranchCompactionRetentionPolicy::DropExpired,
            &mut outcome.unsafe_ttl_pruning_rejection_cases,
        ),
    ] {
        let request = branch_compaction_request(
            branch,
            BranchCompactionKind::CompactL0,
            format!("generated-prune-{policy:?}").to_ascii_lowercase(),
        )?
        .with_retention_policy(policy);
        expect_invalid_compaction(state.compact_branch_owned_tables(&request))?;
        if state != before {
            return Err(TestkitError::new(
                "unsafe compaction pruning rejection mutated state",
            ));
        }
        *counter += 1;
    }
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn check_l0_keep_all_compaction_parity_and_release(
    script: &[u8],
    outcome: &mut CompactionOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 163));
    let read_key = physical_key(branch, b"generated-compact-live".to_vec())?;
    let deleted_key = physical_key(branch, b"generated-compact-delete".to_vec())?;
    let prefix = physical_key(branch, b"generated-compact-scan-".to_vec())?;
    let lower = physical_key(branch, b"generated-compact-live".to_vec())?;
    let upper = physical_key(branch, b"generated-compact-scan-z".to_vec())?;

    let mut state = compaction_state_with_l0_tables(
        branch,
        "generated-keep",
        vec![
            vec![
                storage_row_with(
                    branch,
                    b"generated-compact-live".to_vec(),
                    6,
                    60,
                    Timestamp::EPOCH,
                    b"new".to_vec(),
                )?,
                tombstone_row(branch, b"generated-compact-delete".to_vec(), 7, 70)?,
                storage_row_with(
                    branch,
                    b"generated-compact-scan-a".to_vec(),
                    4,
                    40,
                    Timestamp::EPOCH,
                    vec![script_byte(script, 164)],
                )?,
            ],
            vec![
                storage_row_with(
                    branch,
                    b"generated-compact-live".to_vec(),
                    2,
                    20,
                    Timestamp::EPOCH,
                    b"old".to_vec(),
                )?,
                storage_row_with(
                    branch,
                    b"generated-compact-delete".to_vec(),
                    3,
                    30,
                    Timestamp::EPOCH,
                    b"deleted".to_vec(),
                )?,
                storage_row_with(
                    branch,
                    b"generated-compact-scan-z".to_vec(),
                    5,
                    50,
                    Timestamp::EPOCH,
                    vec![script_byte(script, 165)],
                )?,
            ],
        ],
    )?;
    let before_snapshot = state
        .reachability_snapshot()
        .map_err(|err| TestkitError::new(format!("pre-compact reachability failed: {err}")))?;
    let before_view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("pre-compact view failed: {err}")))?;
    let before_latest = visible_row(&before_view, &read_key, BranchReadBound::latest())?;
    let before_version = visible_row(
        &before_view,
        &read_key,
        BranchReadBound::at_version(CommitVersion::new(2)),
    )?;
    let before_timestamp = visible_row(
        &before_view,
        &read_key,
        BranchReadBound::at_timestamp(Timestamp::from_micros(25)),
    )?;
    let before_history = history_versions(
        &before_view
            .history(&read_key, BranchHistoryOptions::all())
            .map_err(|err| TestkitError::new(format!("pre-compact history failed: {err}")))?,
    );
    let before_prefix = visible_user_keys(
        &before_view
            .scan_prefix(
                &BranchScanBounds::prefix(&prefix),
                BranchReadBound::latest(),
            )
            .map_err(|err| TestkitError::new(format!("pre-compact prefix failed: {err}")))?,
    );
    let before_range = visible_user_keys(
        &before_view
            .scan_range(
                &BranchScanBounds::closed(&lower, &upper).map_err(|err| {
                    TestkitError::new(format!("compact range bound failed: {err}"))
                })?,
                BranchReadBound::latest(),
            )
            .map_err(|err| TestkitError::new(format!("pre-compact range failed: {err}")))?,
    );
    if before_view
        .latest(&deleted_key)
        .map_err(|err| TestkitError::new(format!("pre-compact tombstone failed: {err}")))?
        .is_some()
    {
        return Err(TestkitError::new("pre-compact tombstone did not shadow"));
    }

    let request = branch_compaction_request(
        branch,
        BranchCompactionKind::CompactL0,
        "generated-compact-keep",
    )?;
    let plan = state
        .plan_branch_compaction(&request)
        .map_err(|err| TestkitError::new(format!("L0 compaction plan failed: {err}")))?;
    let candidate = plan
        .candidate()
        .ok_or_else(|| TestkitError::new("L0 compaction missed candidate"))?;
    if candidate.input_refs().len() != 2
        || !candidate.overlap_refs().is_empty()
        || candidate.output_level() != BranchLevel::ZERO
    {
        return Err(TestkitError::new("L0 compaction candidate facts drifted"));
    }
    outcome.l0_compaction_candidate_cases += 1;

    let compaction = state
        .install_branch_compaction_plan(&request, &plan)
        .map_err(|err| TestkitError::new(format!("L0 compaction install failed: {err}")))?;
    if compaction.recovery() != BranchCompactionRecovery::InstalledReplacementTables
        || compaction.output_refs().is_empty()
        || compaction.removed_refs().len() != 2
    {
        return Err(TestkitError::new("L0 compaction outcome facts drifted"));
    }
    let report = compaction
        .table_report()
        .ok_or_else(|| TestkitError::new("installed compaction missed table report"))?;
    if report.input_sources() != 2
        || report.input_rows() != 6
        || report.kept_rows() != 6
        || report.dropped_rows() != 0
        || report.output_tables() != compaction.output_refs().len()
    {
        return Err(TestkitError::new("keep-all compaction report drifted"));
    }
    outcome.keep_all_compaction_cases += 1;
    outcome.compaction_output_install_cases += 1;

    let after_snapshot = state
        .reachability_snapshot()
        .map_err(|err| TestkitError::new(format!("post-compact reachability failed: {err}")))?;
    let after_view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("post-compact view failed: {err}")))?;
    if visible_row(&after_view, &read_key, BranchReadBound::latest())? != before_latest {
        return Err(TestkitError::new("compaction latest read parity drifted"));
    }
    outcome.compaction_latest_parity_cases += 1;
    if visible_row(
        &after_view,
        &read_key,
        BranchReadBound::at_version(CommitVersion::new(2)),
    )? != before_version
    {
        return Err(TestkitError::new("compaction version read parity drifted"));
    }
    outcome.compaction_version_parity_cases += 1;
    if visible_row(
        &after_view,
        &read_key,
        BranchReadBound::at_timestamp(Timestamp::from_micros(25)),
    )? != before_timestamp
    {
        return Err(TestkitError::new(
            "compaction timestamp read parity drifted",
        ));
    }
    outcome.compaction_timestamp_parity_cases += 1;
    let after_history = history_versions(
        &after_view
            .history(&read_key, BranchHistoryOptions::all())
            .map_err(|err| TestkitError::new(format!("post-compact history failed: {err}")))?,
    );
    if after_history != before_history {
        return Err(TestkitError::new("compaction history parity drifted"));
    }
    outcome.compaction_history_parity_cases += 1;
    let after_prefix = visible_user_keys(
        &after_view
            .scan_prefix(
                &BranchScanBounds::prefix(&prefix),
                BranchReadBound::latest(),
            )
            .map_err(|err| TestkitError::new(format!("post-compact prefix failed: {err}")))?,
    );
    if after_prefix != before_prefix {
        return Err(TestkitError::new("compaction prefix scan parity drifted"));
    }
    outcome.compaction_prefix_scan_parity_cases += 1;
    let after_range = visible_user_keys(
        &after_view
            .scan_range(
                &BranchScanBounds::closed(&lower, &upper).map_err(|err| {
                    TestkitError::new(format!("post-compact range bound failed: {err}"))
                })?,
                BranchReadBound::latest(),
            )
            .map_err(|err| TestkitError::new(format!("post-compact range failed: {err}")))?,
    );
    if after_range != before_range {
        return Err(TestkitError::new("compaction range scan parity drifted"));
    }
    outcome.compaction_range_scan_parity_cases += 1;
    if visible_row(&before_view, &read_key, BranchReadBound::latest())? != before_latest {
        return Err(TestkitError::new(
            "pinned pre-compact view drifted after install",
        ));
    }
    outcome.compaction_pinned_view_isolation_cases += 1;

    let aggregate_after =
        BranchReachabilityAggregate::from_snapshots(std::slice::from_ref(&after_snapshot))
            .map_err(|err| TestkitError::new(format!("post-compact aggregate failed: {err}")))?;
    let release = BranchReleasePlan::from_removed_refs(
        branch,
        compaction.removed_refs().to_vec(),
        &aggregate_after,
        Some(&SharedTableRegistry::new()),
    )
    .map_err(|err| TestkitError::new(format!("compaction release failed: {err}")))?;
    if release.releasable_tables().len() != compaction.removed_refs().len()
        || !release.protected_tables().is_empty()
    {
        return Err(TestkitError::new("compaction release candidate drifted"));
    }
    outcome.compaction_release_candidate_cases += 1;

    let mut runtime_registry = SharedTableRegistry::new();
    runtime_registry
        .register_snapshot(&before_snapshot)
        .map_err(|err| TestkitError::new(format!("protected registry failed: {err}")))?;
    let protected = BranchReleasePlan::from_removed_refs(
        branch,
        compaction.removed_refs().to_vec(),
        &aggregate_after,
        Some(&runtime_registry),
    )
    .map_err(|err| TestkitError::new(format!("protected compaction release failed: {err}")))?;
    if protected.protected_tables().len() != compaction.removed_refs().len()
        || !protected.releasable_tables().is_empty()
    {
        return Err(TestkitError::new("compaction protected release drifted"));
    }
    outcome.compaction_protected_release_cases += 1;
    Ok(())
}

fn check_compaction_output_splitting(
    script: &[u8],
    outcome: &mut CompactionOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 166));
    let mut state = compaction_state_with_l0_tables(
        branch,
        "generated-split",
        vec![
            vec![storage_row_with(
                branch,
                b"generated-split-a".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                vec![script_byte(script, 167), 0x01],
            )?],
            vec![storage_row_with(
                branch,
                b"generated-split-b".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                vec![script_byte(script, 168), 0x02],
            )?],
        ],
    )?;
    let config = TableCompactionConfig::new(1, 8)
        .map_err(|err| TestkitError::new(format!("split compaction config failed: {err}")))?;
    let request = branch_compaction_request(
        branch,
        BranchCompactionKind::CompactL0,
        "generated-compact-split",
    )?
    .with_table_compaction_config(config);
    let compaction = state
        .compact_branch_owned_tables(&request)
        .map_err(|err| TestkitError::new(format!("split compaction failed: {err}")))?;
    let report = compaction
        .table_report()
        .ok_or_else(|| TestkitError::new("split compaction missed report"))?;
    if compaction.output_refs().len() < 2 || report.split_count() == 0 {
        return Err(TestkitError::new(
            "compaction output split was not exercised",
        ));
    }
    outcome.compaction_output_split_cases += 1;
    Ok(())
}

fn check_l0_to_l1_compaction_candidate(
    script: &[u8],
    outcome: &mut CompactionOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 169));
    let mut state = BranchLocalState::new(branch, compaction_runtime_config()?)
        .map_err(|err| TestkitError::new(format!("L0-to-L1 state failed: {err}")))?;
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "generated-l0l1-overlap",
                vec![storage_row_with(
                    branch,
                    b"generated-l0l1-key".to_vec(),
                    1,
                    10,
                    Timestamp::EPOCH,
                    vec![script_byte(script, 170)],
                )?],
            )?,
        )
        .map_err(|err| TestkitError::new(format!("L1 overlap install failed: {err}")))?;
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "generated-l0l1-input",
            vec![storage_row_with(
                branch,
                b"generated-l0l1-key".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                vec![script_byte(script, 171)],
            )?],
        )?)
        .map_err(|err| TestkitError::new(format!("L0 input install failed: {err}")))?;
    let request = branch_compaction_request(
        branch,
        BranchCompactionKind::CompactL0ToLevelOne,
        "generated-compact-l0l1",
    )?;
    let plan = state
        .plan_branch_compaction(&request)
        .map_err(|err| TestkitError::new(format!("L0-to-L1 plan failed: {err}")))?;
    let candidate = plan
        .candidate()
        .ok_or_else(|| TestkitError::new("L0-to-L1 compaction missed candidate"))?;
    if candidate.input_refs().len() != 1
        || candidate.overlap_refs().len() != 1
        || candidate.output_level() != BranchLevel::new(1)
    {
        return Err(TestkitError::new("L0-to-L1 candidate facts drifted"));
    }
    outcome.l0_to_l1_compaction_candidate_cases += 1;
    Ok(())
}

fn check_nonzero_level_compaction_candidate(
    script: &[u8],
    outcome: &mut CompactionOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 172));
    let mut state = BranchLocalState::new(branch, compaction_runtime_config()?)
        .map_err(|err| TestkitError::new(format!("nonzero compaction state failed: {err}")))?;
    state
        .install_owned_table_at_level(
            BranchLevel::new(2),
            branch_owned_table(
                branch,
                BranchLevel::new(2),
                "generated-nonzero-overlap",
                vec![storage_row_with(
                    branch,
                    b"generated-nonzero-key".to_vec(),
                    1,
                    10,
                    Timestamp::EPOCH,
                    vec![script_byte(script, 173)],
                )?],
            )?,
        )
        .map_err(|err| TestkitError::new(format!("L2 overlap install failed: {err}")))?;
    state
        .install_owned_table_at_level(
            BranchLevel::new(1),
            branch_owned_table(
                branch,
                BranchLevel::new(1),
                "generated-nonzero-input",
                vec![storage_row_with(
                    branch,
                    b"generated-nonzero-key".to_vec(),
                    2,
                    20,
                    Timestamp::EPOCH,
                    vec![script_byte(script, 174)],
                )?],
            )?,
        )
        .map_err(|err| TestkitError::new(format!("L1 input install failed: {err}")))?;
    let request = branch_compaction_request(
        branch,
        BranchCompactionKind::CompactLevel {
            level: BranchLevel::new(1),
            table_index: 0,
        },
        "generated-compact-nonzero",
    )?;
    let plan = state
        .plan_branch_compaction(&request)
        .map_err(|err| TestkitError::new(format!("nonzero compaction plan failed: {err}")))?;
    let candidate = plan
        .candidate()
        .ok_or_else(|| TestkitError::new("nonzero compaction missed candidate"))?;
    if candidate.input_refs().len() != 1
        || candidate.overlap_refs().len() != 1
        || candidate.output_level() != BranchLevel::new(2)
    {
        return Err(TestkitError::new(
            "nonzero compaction candidate facts drifted",
        ));
    }
    outcome.nonzero_level_compaction_candidate_cases += 1;
    Ok(())
}

fn check_stale_compaction_plan_rejection(
    script: &[u8],
    outcome: &mut CompactionOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 175));
    let mut state = compaction_state_with_l0_tables(
        branch,
        "generated-stale",
        vec![
            vec![storage_row_with(
                branch,
                b"generated-stale-a".to_vec(),
                1,
                10,
                Timestamp::EPOCH,
                vec![script_byte(script, 176)],
            )?],
            vec![storage_row_with(
                branch,
                b"generated-stale-b".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                vec![script_byte(script, 177)],
            )?],
        ],
    )?;
    let request = branch_compaction_request(
        branch,
        BranchCompactionKind::CompactL0,
        "generated-compact-stale",
    )?;
    let plan = state
        .plan_branch_compaction(&request)
        .map_err(|err| TestkitError::new(format!("stale compaction plan failed: {err}")))?;
    state
        .install_l0_table(branch_owned_table(
            branch,
            BranchLevel::ZERO,
            "generated-stale-newer",
            vec![storage_row_with(
                branch,
                b"generated-stale-c".to_vec(),
                3,
                30,
                Timestamp::EPOCH,
                vec![script_byte(script, 178)],
            )?],
        )?)
        .map_err(|err| TestkitError::new(format!("stale plan mutation failed: {err}")))?;
    let before_install = state.clone();
    expect_invalid_compaction(state.install_branch_compaction_plan(&request, &plan))?;
    if state != before_install {
        return Err(TestkitError::new(
            "stale compaction rejection mutated state",
        ));
    }
    outcome.stale_candidate_rejection_cases += 1;
    Ok(())
}

fn check_branch_snapshot_install(script: &[u8]) -> Result<SnapshotInstallOutcome, TestkitError> {
    let mut outcome = SnapshotInstallOutcome::default();
    check_snapshot_empty_install(&mut outcome)?;
    check_snapshot_single_branch_install(script, &mut outcome)?;
    check_snapshot_multi_branch_install(script, &mut outcome)?;
    check_snapshot_invalid_requests(script, &mut outcome)?;
    check_snapshot_table_build_failure(script, &mut outcome)?;
    Ok(outcome)
}

fn check_snapshot_empty_install(outcome: &mut SnapshotInstallOutcome) -> Result<(), TestkitError> {
    let branch = branch_id(180);
    let mut branches = vec![BranchLocalState::empty(branch)];
    let before = branches.clone();
    let request =
        BranchSnapshotInstallRequest::from_rows("generated-snapshot-empty", Vec::new())
            .map_err(|err| TestkitError::new(format!("empty snapshot request failed: {err}")))?;
    let install = install_snapshot_rows_into_branches(&mut branches, &request)
        .map_err(|err| TestkitError::new(format!("empty snapshot install failed: {err}")))?;
    if install.recovery() != BranchSnapshotInstallRecovery::EmptyPlanNoop
        || install.rows_installed() != 0
        || install.tables_created() != 0
        || !install.branch_outcomes().is_empty()
        || branches != before
    {
        return Err(TestkitError::new("empty snapshot install facts drifted"));
    }
    outcome.snapshot_empty_install_noop_cases += 1;
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn check_snapshot_single_branch_install(
    script: &[u8],
    outcome: &mut SnapshotInstallOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(181);
    let empty_key = physical_key(branch, Vec::new())?;
    let live_key = physical_key(branch, b"generated-snapshot-live".to_vec())?;
    let tombstone_key = physical_key(branch, b"generated-snapshot-deleted".to_vec())?;
    let ttl_key = physical_key(branch, b"generated-snapshot-ttl".to_vec())?;
    let large_key = physical_key(branch, b"generated-snapshot-large".to_vec())?;
    let max_timestamp_key = physical_key(branch, b"generated-snapshot-max-timestamp".to_vec())?;
    let lower = physical_key(branch, b"generated-snapshot-scan-a".to_vec())?;
    let upper = physical_key(branch, b"generated-snapshot-scan-z".to_vec())?;
    let alt_space = StorageSpaceId::engine(0x21)
        .map_err(|err| TestkitError::new(format!("snapshot alternate space failed: {err}")))?;
    let alt_key = physical_key_with_space(
        branch,
        "alternate",
        alt_space,
        vec![0xff, script_byte(script, 179), 0x00],
    )?;
    let live_old = storage_row_with(
        branch,
        b"generated-snapshot-live".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        vec![script_byte(script, 180), 0x01],
    )?;
    let live_new = storage_row_with(
        branch,
        b"generated-snapshot-live".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        vec![script_byte(script, 181), 0x03],
    )?;
    let tombstone_put = storage_row_with(
        branch,
        b"generated-snapshot-deleted".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"will-delete".to_vec(),
    )?;
    let tombstone = tombstone_row(branch, b"generated-snapshot-deleted".to_vec(), 4, 40)?;
    let ttl_row = storage_row_with(
        branch,
        b"generated-snapshot-ttl".to_vec(),
        5,
        35,
        Timestamp::from_micros(50),
        b"ttl".to_vec(),
    )?;
    let empty_key_row = storage_row_with(branch, Vec::new(), 10, 5, Timestamp::EPOCH, Vec::new())?;
    let large_value_row = storage_row_with(
        branch,
        b"generated-snapshot-large".to_vec(),
        11,
        110,
        Timestamp::EPOCH,
        vec![script_byte(script, 186); 8 * 1024],
    )?;
    let max_timestamp_row = storage_row_with(
        branch,
        b"generated-snapshot-max-timestamp".to_vec(),
        12,
        u64::MAX,
        Timestamp::MAX,
        vec![script_byte(script, 187), 0xff],
    )?;
    let scan_a = storage_row_with(
        branch,
        b"generated-snapshot-scan-a".to_vec(),
        6,
        60,
        Timestamp::EPOCH,
        Vec::new(),
    )?;
    let scan_b = storage_row_with(
        branch,
        b"generated-snapshot-scan-b".to_vec(),
        7,
        70,
        Timestamp::EPOCH,
        vec![script_byte(script, 182)],
    )?;
    let scan_z = storage_row_with(
        branch,
        b"generated-snapshot-scan-z".to_vec(),
        13,
        130,
        Timestamp::EPOCH,
        vec![script_byte(script, 188)],
    )?;
    let high_bit = storage_row_with(
        branch,
        vec![0xff, script_byte(script, 183), 0x00],
        8,
        80,
        Timestamp::EPOCH,
        vec![0x80, script_byte(script, 184)],
    )?;
    let alt_row = storage_row_with_space(
        branch,
        "alternate",
        alt_space,
        alt_key.user_key().to_vec(),
        9,
        90,
        Timestamp::EPOCH,
        vec![script_byte(script, 185), 0x09],
    )?;
    let rows = sorted_snapshot_rows(vec![
        live_old.clone(),
        live_new.clone(),
        tombstone_put,
        tombstone.clone(),
        ttl_row.clone(),
        empty_key_row.clone(),
        large_value_row.clone(),
        max_timestamp_row.clone(),
        scan_a.clone(),
        scan_b.clone(),
        scan_z.clone(),
        high_bit.clone(),
        alt_row.clone(),
    ]);
    let request = BranchSnapshotInstallRequest::new(
        "generated-snapshot-single",
        vec![BranchSnapshotInstallGroup::new(branch, rows)],
    )
    .map_err(|err| TestkitError::new(format!("single snapshot request failed: {err}")))?
    .with_max_rows_per_table(3)
    .map_err(|err| TestkitError::new(format!("single snapshot chunk config failed: {err}")))?;
    let mut branches = vec![BranchLocalState::empty(branch)];
    let pinned_before = branches[0]
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("pre-snapshot read view failed: {err}")))?;
    let install = install_snapshot_rows_into_branches(&mut branches, &request)
        .map_err(|err| TestkitError::new(format!("single snapshot install failed: {err}")))?;
    let installed = branch_state_by_id(&branches, branch)?;
    if install.recovery() != BranchSnapshotInstallRecovery::Installed
        || install.rows_installed() != 13
        || install.tables_created() != 5
        || install.branches_replaced() != 1
        || install.branches_created() != 0
        || installed.active_row_count() != 0
        || installed.owned_table_count() != 5
    {
        return Err(TestkitError::new("single snapshot install facts drifted"));
    }
    outcome.snapshot_single_branch_install_cases += 1;

    let view = installed
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("post-snapshot read view failed: {err}")))?;
    if visible_row(&view, &live_key, BranchReadBound::latest())? != Some(live_new.clone()) {
        return Err(TestkitError::new("snapshot latest read parity drifted"));
    }
    outcome.snapshot_latest_parity_cases += 1;
    if visible_row(
        &view,
        &live_key,
        BranchReadBound::at_version(CommitVersion::new(1)),
    )? != Some(live_old)
    {
        return Err(TestkitError::new("snapshot version read parity drifted"));
    }
    outcome.snapshot_version_parity_cases += 1;
    if visible_row(
        &view,
        &live_key,
        BranchReadBound::at_timestamp(Timestamp::from_micros(20)),
    )? != Some(storage_row_with(
        branch,
        b"generated-snapshot-live".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        vec![script_byte(script, 180), 0x01],
    )?) {
        return Err(TestkitError::new("snapshot timestamp read parity drifted"));
    }
    outcome.snapshot_timestamp_parity_cases += 1;
    if history_versions(
        &view
            .history(&live_key, BranchHistoryOptions::all())
            .map_err(|err| TestkitError::new(format!("snapshot history failed: {err}")))?,
    ) != vec![3, 1]
    {
        return Err(TestkitError::new("snapshot history parity drifted"));
    }
    outcome.snapshot_history_parity_cases += 1;

    let prefix =
        BranchScanBounds::prefix(&physical_key(branch, b"generated-snapshot-scan-".to_vec())?);
    if visible_user_keys(
        &view
            .scan_prefix(&prefix, BranchReadBound::latest())
            .map_err(|err| TestkitError::new(format!("snapshot prefix scan failed: {err}")))?,
    ) != vec![
        b"generated-snapshot-scan-a".to_vec(),
        b"generated-snapshot-scan-b".to_vec(),
        b"generated-snapshot-scan-z".to_vec(),
    ] {
        return Err(TestkitError::new("snapshot prefix scan parity drifted"));
    }
    outcome.snapshot_prefix_scan_parity_cases += 1;
    let range = BranchScanBounds::closed(&lower, &upper)
        .map_err(|err| TestkitError::new(format!("snapshot range bounds failed: {err}")))?;
    if visible_user_keys(
        &view
            .scan_range(&range, BranchReadBound::latest())
            .map_err(|err| TestkitError::new(format!("snapshot range scan failed: {err}")))?,
    ) != vec![
        b"generated-snapshot-scan-a".to_vec(),
        b"generated-snapshot-scan-b".to_vec(),
        b"generated-snapshot-scan-z".to_vec(),
    ] {
        return Err(TestkitError::new("snapshot range scan parity drifted"));
    }
    outcome.snapshot_range_scan_parity_cases += 1;

    if visible_row(&view, &tombstone_key, BranchReadBound::latest())?.is_some()
        || history_versions(
            &view
                .history(&tombstone_key, BranchHistoryOptions::all())
                .map_err(|err| {
                    TestkitError::new(format!("snapshot tombstone history failed: {err}"))
                })?,
        ) != vec![4, 2]
    {
        return Err(TestkitError::new("snapshot tombstone preservation drifted"));
    }
    outcome.snapshot_tombstone_preservation_cases += 1;
    if visible_row(
        &view,
        &ttl_key,
        BranchReadBound::at_timestamp(Timestamp::from_micros(49)),
    )? != Some(ttl_row)
        || visible_row(
            &view,
            &ttl_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(50)),
        )?
        .is_some()
    {
        return Err(TestkitError::new("snapshot TTL preservation drifted"));
    }
    outcome.snapshot_ttl_preservation_cases += 1;
    if visible_row(&pinned_before, &live_key, BranchReadBound::latest())?.is_some()
        || visible_row(&pinned_before, &alt_key, BranchReadBound::latest())?.is_some()
    {
        return Err(TestkitError::new(
            "pre-install pinned view observed snapshot rows",
        ));
    }
    outcome.snapshot_pinned_view_isolation_cases += 1;
    let reachability = installed
        .reachability_snapshot()
        .map_err(|err| TestkitError::new(format!("snapshot reachability failed: {err}")))?;
    if reachability.facts().owned_table_count() != 5
        || reachability.table_refs().len() != 5
        || reachability
            .table_refs()
            .iter()
            .any(|table_ref| table_ref.reference_kind() != BranchTableReferenceKind::Owned)
    {
        return Err(TestkitError::new("snapshot reachability facts drifted"));
    }
    outcome.snapshot_reachability_cases += 1;
    if visible_row(&view, &alt_key, BranchReadBound::latest())? != Some(alt_row.clone())
        || visible_row(&view, &empty_key, BranchReadBound::latest())? != Some(empty_key_row)
        || visible_row(&view, &large_key, BranchReadBound::latest())? != Some(large_value_row)
        || visible_row(&view, high_bit.physical_key(), BranchReadBound::latest())?
            != Some(high_bit.clone())
        || visible_row(&view, &max_timestamp_key, BranchReadBound::latest())?
            != Some(max_timestamp_row)
    {
        return Err(TestkitError::new(
            "snapshot row-native boundary facts drifted",
        ));
    }
    outcome.snapshot_source_boundary_guard_cases += 1;
    Ok(())
}

fn check_snapshot_multi_branch_install(
    script: &[u8],
    outcome: &mut SnapshotInstallOutcome,
) -> Result<(), TestkitError> {
    let existing = branch_id(182);
    let created = branch_id(183);
    let untouched = branch_id(184);
    let existing_row = storage_row_with(
        existing,
        b"generated-snapshot-shared-key".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        vec![script_byte(script, 189)],
    )?;
    let created_row = storage_row_with(
        created,
        b"generated-snapshot-shared-key".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        vec![script_byte(script, 190)],
    )?;
    let request = BranchSnapshotInstallRequest::new(
        "generated-snapshot-multi",
        vec![
            BranchSnapshotInstallGroup::new(existing, sorted_snapshot_rows(vec![existing_row])),
            BranchSnapshotInstallGroup::new(created, sorted_snapshot_rows(vec![created_row])),
        ],
    )
    .map_err(|err| TestkitError::new(format!("multi snapshot request failed: {err}")))?
    .with_missing_branch_policy(BranchSnapshotMissingBranchPolicy::Create {
        config: BranchRuntimeConfig::default(),
    });
    let mut branches = vec![
        BranchLocalState::empty(untouched),
        BranchLocalState::empty(existing),
    ];
    let install = install_snapshot_rows_into_branches(&mut branches, &request)
        .map_err(|err| TestkitError::new(format!("multi snapshot install failed: {err}")))?;
    if install.recovery() != BranchSnapshotInstallRecovery::Installed
        || install.rows_installed() != 2
        || install.tables_created() != 2
        || install.branches_replaced() != 1
        || install.branches_created() != 1
        || branches
            .iter()
            .map(BranchLocalState::branch_id)
            .collect::<Vec<_>>()
            != vec![untouched, existing, created]
    {
        return Err(TestkitError::new("multi snapshot install facts drifted"));
    }
    outcome.snapshot_multi_branch_install_cases += 1;
    outcome.snapshot_missing_branch_create_cases += 1;
    Ok(())
}

#[allow(clippy::too_many_lines)]
fn check_snapshot_invalid_requests(
    script: &[u8],
    outcome: &mut SnapshotInstallOutcome,
) -> Result<(), TestkitError> {
    let target = branch_id(185);
    let other = branch_id(186);

    let mut missing_branches = vec![BranchLocalState::empty(other)];
    let missing_before = missing_branches.clone();
    let missing_request = BranchSnapshotInstallRequest::new(
        "generated-snapshot-missing",
        vec![BranchSnapshotInstallGroup::new(
            target,
            sorted_snapshot_rows(vec![storage_row(target, 1)?]),
        )],
    )
    .map_err(|err| TestkitError::new(format!("missing snapshot request failed: {err}")))?;
    expect_missing_snapshot_branch(
        install_snapshot_rows_into_branches(&mut missing_branches, &missing_request),
        target,
    )?;
    if missing_branches != missing_before {
        return Err(TestkitError::new(
            "missing snapshot rejection mutated state",
        ));
    }
    outcome.snapshot_missing_branch_rejection_cases += 1;

    let mut non_empty = vec![BranchLocalState::empty(target)];
    non_empty[0]
        .append_committed_row(storage_row_with(
            target,
            b"generated-snapshot-existing".to_vec(),
            1,
            10,
            Timestamp::EPOCH,
            b"existing".to_vec(),
        )?)
        .map_err(|err| TestkitError::new(format!("non-empty snapshot seed failed: {err}")))?;
    let non_empty_before = non_empty.clone();
    let non_empty_request = BranchSnapshotInstallRequest::new(
        "generated-snapshot-non-empty",
        vec![BranchSnapshotInstallGroup::new(
            target,
            sorted_snapshot_rows(vec![storage_row_with(
                target,
                b"generated-snapshot-new".to_vec(),
                2,
                20,
                Timestamp::EPOCH,
                vec![script_byte(script, 188)],
            )?]),
        )],
    )
    .map_err(|err| TestkitError::new(format!("non-empty snapshot request failed: {err}")))?;
    expect_invalid_snapshot_install(
        install_snapshot_rows_into_branches(&mut non_empty, &non_empty_request),
        "snapshot install target branch must be empty",
    )?;
    if non_empty != non_empty_before {
        return Err(TestkitError::new(
            "non-empty snapshot rejection mutated state",
        ));
    }
    outcome.snapshot_non_empty_target_rejection_cases += 1;

    let mut branches = vec![
        BranchLocalState::empty(target),
        BranchLocalState::empty(other),
    ];
    let before = branches.clone();

    let empty_group_request = BranchSnapshotInstallRequest::new(
        "generated-snapshot-empty-group",
        vec![BranchSnapshotInstallGroup::new(target, Vec::new())],
    )
    .map_err(|err| TestkitError::new(format!("empty-group snapshot request failed: {err}")))?;
    expect_invalid_snapshot_install(
        install_snapshot_rows_into_branches(&mut branches, &empty_group_request),
        "snapshot install branch groups must not be empty",
    )?;
    if branches != before {
        return Err(TestkitError::new(
            "empty-group snapshot rejection mutated state",
        ));
    }
    outcome.snapshot_empty_group_rejection_cases += 1;

    let duplicate_group_request = BranchSnapshotInstallRequest::new(
        "generated-snapshot-duplicate-group",
        vec![
            BranchSnapshotInstallGroup::new(
                target,
                sorted_snapshot_rows(vec![storage_row_with(
                    target,
                    b"generated-snapshot-group-a".to_vec(),
                    1,
                    10,
                    Timestamp::EPOCH,
                    vec![script_byte(script, 189)],
                )?]),
            ),
            BranchSnapshotInstallGroup::new(
                target,
                sorted_snapshot_rows(vec![storage_row_with(
                    target,
                    b"generated-snapshot-group-b".to_vec(),
                    2,
                    20,
                    Timestamp::EPOCH,
                    vec![script_byte(script, 190)],
                )?]),
            ),
        ],
    )
    .map_err(|err| TestkitError::new(format!("duplicate-group snapshot request failed: {err}")))?;
    expect_invalid_snapshot_install(
        install_snapshot_rows_into_branches(&mut branches, &duplicate_group_request),
        "snapshot install branch groups must be unique",
    )?;
    if branches != before {
        return Err(TestkitError::new(
            "duplicate-group snapshot rejection mutated state",
        ));
    }
    outcome.snapshot_duplicate_branch_group_rejection_cases += 1;

    let mismatch_request = BranchSnapshotInstallRequest::new(
        "generated-snapshot-mismatch",
        vec![BranchSnapshotInstallGroup::new(
            target,
            sorted_snapshot_rows(vec![storage_row(other, 1)?]),
        )],
    )
    .map_err(|err| TestkitError::new(format!("mismatch snapshot request failed: {err}")))?;
    expect_invalid_branch_row(install_snapshot_rows_into_branches(
        &mut branches,
        &mismatch_request,
    ))?;
    if branches != before {
        return Err(TestkitError::new(
            "branch-mismatch snapshot rejection mutated state",
        ));
    }
    outcome.snapshot_branch_mismatch_rejection_cases += 1;

    let duplicate = storage_row(target, 1)?;
    let duplicate_request = BranchSnapshotInstallRequest::new(
        "generated-snapshot-duplicate",
        vec![BranchSnapshotInstallGroup::new(
            target,
            vec![duplicate.clone(), duplicate],
        )],
    )
    .map_err(|err| TestkitError::new(format!("duplicate snapshot request failed: {err}")))?;
    expect_duplicate_internal_key(install_snapshot_rows_into_branches(
        &mut branches,
        &duplicate_request,
    ))?;
    if branches != before {
        return Err(TestkitError::new(
            "duplicate snapshot rejection mutated state",
        ));
    }
    outcome.snapshot_duplicate_row_rejection_cases += 1;

    let unsorted_request = BranchSnapshotInstallRequest::new(
        "generated-snapshot-unsorted",
        vec![BranchSnapshotInstallGroup::new(
            target,
            vec![
                storage_row_with(
                    target,
                    b"z-generated-snapshot".to_vec(),
                    1,
                    10,
                    Timestamp::EPOCH,
                    vec![script_byte(script, 191)],
                )?,
                storage_row_with(
                    target,
                    b"a-generated-snapshot".to_vec(),
                    1,
                    10,
                    Timestamp::EPOCH,
                    vec![script_byte(script, 192)],
                )?,
            ],
        )],
    )
    .map_err(|err| TestkitError::new(format!("unsorted snapshot request failed: {err}")))?;
    expect_invalid_snapshot_install(
        install_snapshot_rows_into_branches(&mut branches, &unsorted_request),
        "snapshot install rows must be strictly sorted by internal key",
    )?;
    if branches != before {
        return Err(TestkitError::new(
            "unsorted snapshot rejection mutated state",
        ));
    }
    outcome.snapshot_unsorted_row_rejection_cases += 1;

    let group_order_request = BranchSnapshotInstallRequest::new(
        "generated-snapshot-group-order",
        vec![
            BranchSnapshotInstallGroup::new(
                other,
                sorted_snapshot_rows(vec![storage_row(other, 1)?]),
            ),
            BranchSnapshotInstallGroup::new(
                target,
                sorted_snapshot_rows(vec![storage_row(target, 1)?]),
            ),
        ],
    )
    .map_err(|err| TestkitError::new(format!("group-order snapshot request failed: {err}")))?;
    expect_invalid_snapshot_install(
        install_snapshot_rows_into_branches(&mut branches, &group_order_request),
        "snapshot install branch groups must be sorted by branch id",
    )?;
    if branches != before {
        return Err(TestkitError::new(
            "group-order snapshot rejection mutated state",
        ));
    }

    let collision_existing = branch_id(189);
    let collision_target = branch_id(190);
    let collision_rows = sorted_snapshot_rows(vec![storage_row_with(
        collision_target,
        b"generated-snapshot-collision".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        vec![script_byte(script, 193)],
    )?]);
    let collision_request = BranchSnapshotInstallRequest::new(
        "generated-snapshot-collision",
        vec![BranchSnapshotInstallGroup::new(
            collision_target,
            collision_rows.clone(),
        )],
    )
    .map_err(|err| TestkitError::new(format!("collision snapshot request failed: {err}")))?
    .with_missing_branch_policy(BranchSnapshotMissingBranchPolicy::Create {
        config: BranchRuntimeConfig::default(),
    });
    let mut dry_run = Vec::new();
    let dry_run_outcome = install_snapshot_rows_into_branches(&mut dry_run, &collision_request)
        .map_err(|err| TestkitError::new(format!("collision dry run failed: {err}")))?;
    let collision_identity = dry_run_outcome.branch_outcomes()[0].table_identities()[0].clone();
    let mut collision_branches = vec![
        BranchLocalState::empty(collision_existing),
        BranchLocalState::empty(collision_target),
    ];
    collision_branches[0]
        .install_l0_table(branch_owned_table(
            collision_existing,
            BranchLevel::ZERO,
            collision_identity.as_str(),
            vec![storage_row(collision_existing, 1)?],
        )?)
        .map_err(|err| TestkitError::new(format!("collision table seed failed: {err}")))?;
    let collision_before = collision_branches.clone();
    let collision_request = BranchSnapshotInstallRequest::new(
        "generated-snapshot-collision",
        vec![BranchSnapshotInstallGroup::new(
            collision_target,
            collision_rows,
        )],
    )
    .map_err(|err| TestkitError::new(format!("collision snapshot request failed: {err}")))?;
    expect_invalid_snapshot_install(
        install_snapshot_rows_into_branches(&mut collision_branches, &collision_request),
        "snapshot output identity must not collide with existing reachable table",
    )?;
    if collision_branches != collision_before {
        return Err(TestkitError::new(
            "identity-collision snapshot rejection mutated state",
        ));
    }
    outcome.snapshot_output_identity_collision_rejection_cases += 1;
    Ok(())
}

fn check_snapshot_table_build_failure(
    script: &[u8],
    outcome: &mut SnapshotInstallOutcome,
) -> Result<(), TestkitError> {
    let lower = branch_id(187);
    let higher = branch_id(188);
    let huge_key = vec![script_byte(script, 191).max(1); 70 * 1024];
    let request = BranchSnapshotInstallRequest::new(
        "generated-snapshot-build-failure",
        vec![
            BranchSnapshotInstallGroup::new(
                lower,
                sorted_snapshot_rows(vec![storage_row(lower, 1)?]),
            ),
            BranchSnapshotInstallGroup::new(
                higher,
                sorted_snapshot_rows(vec![storage_row_with(
                    higher,
                    huge_key,
                    1,
                    10,
                    Timestamp::EPOCH,
                    b"secret-payload".to_vec(),
                )?]),
            ),
        ],
    )
    .map_err(|err| TestkitError::new(format!("build-failure snapshot request failed: {err}")))?;
    let debug_text = format!("{request:?}");
    if debug_text.contains("secret-payload") {
        return Err(TestkitError::new("snapshot request debug leaked row bytes"));
    }
    let mut branches = vec![
        BranchLocalState::empty(lower),
        BranchLocalState::empty(higher),
    ];
    let before = branches.clone();
    let Err(error) = install_snapshot_rows_into_branches(&mut branches, &request) else {
        return Err(TestkitError::new("oversized snapshot key was accepted"));
    };
    match error {
        BranchRuntimeError::TableRuntime { .. } => {}
        other => {
            return Err(TestkitError::new(format!(
                "snapshot table build returned wrong error: {other}"
            )))
        }
    }
    if error.to_string().contains("secret-payload") {
        return Err(TestkitError::new(
            "snapshot table build error leaked row bytes",
        ));
    }
    if branches != before {
        return Err(TestkitError::new(
            "snapshot table build failure mutated state",
        ));
    }
    outcome.snapshot_table_build_failure_atomicity_cases += 1;
    Ok(())
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ModelBranch {
    branch: BranchId,
    rows: Vec<StorageRow>,
}

impl ModelBranch {
    fn new(branch: BranchId) -> Self {
        Self {
            branch,
            rows: Vec::new(),
        }
    }

    fn push(&mut self, row: StorageRow) -> Result<(), TestkitError> {
        if row.physical_key().branch_id() != self.branch {
            return Err(TestkitError::new("model row branch drifted"));
        }
        if self.rows.iter().any(|existing| {
            TableInternalKeyBytes::from_row(existing) == TableInternalKeyBytes::from_row(&row)
        }) {
            return Err(TestkitError::new("model generated duplicate internal key"));
        }
        self.rows.push(row);
        Ok(())
    }

    fn visible(&self, key: &PhysicalKey, bound: BranchReadBound) -> Option<StorageRow> {
        let read_timestamp = match bound {
            BranchReadBound::Latest | BranchReadBound::AtVersion(_) => None,
            BranchReadBound::AtTimestamp(timestamp) => Some(timestamp),
        };
        self.rows
            .iter()
            .filter(|row| row.physical_key() == key)
            .filter(|row| model_row_matches_bound(row, bound))
            .max_by(|left, right| {
                left.commit_version()
                    .as_u64()
                    .cmp(&right.commit_version().as_u64())
            })
            .and_then(|row| {
                if row.is_tombstone() || model_row_is_expired_at(row, read_timestamp) {
                    None
                } else {
                    Some(row.clone())
                }
            })
    }

    fn history(&self, key: &PhysicalKey) -> Vec<StorageRow> {
        let mut rows = self
            .rows
            .iter()
            .filter(|row| row.physical_key() == key)
            .cloned()
            .collect::<Vec<_>>();
        rows.sort_by(|left, right| {
            right
                .commit_version()
                .as_u64()
                .cmp(&left.commit_version().as_u64())
        });
        rows
    }

    fn scan_visible(
        &self,
        branch: BranchId,
        bound: BranchReadBound,
    ) -> Result<Vec<StorageRow>, TestkitError> {
        let mut rows = Vec::new();
        for index in 0..MODEL_KEY_COUNT {
            let key = model_physical_key(branch, index)?;
            if let Some(row) = self.visible(&key, bound) {
                rows.push(row);
            }
        }
        rows.sort_by(|left, right| {
            left.physical_key()
                .user_key()
                .cmp(right.physical_key().user_key())
        });
        Ok(rows)
    }
}

const MODEL_KEY_COUNT: u8 = 6;

fn append_model_row(
    state: &mut BranchLocalState,
    model: &mut ModelBranch,
    row: StorageRow,
) -> Result<(), TestkitError> {
    state
        .append_committed_row(row.clone())
        .map_err(|err| TestkitError::new(format!("model append failed: {err}")))?;
    model.push(row)
}

fn install_model_l0_rows(
    state: &mut BranchLocalState,
    model: &mut ModelBranch,
    identity: &str,
    rows: Vec<StorageRow>,
) -> Result<(), TestkitError> {
    state
        .install_l0_table(branch_owned_table(
            model.branch,
            BranchLevel::ZERO,
            identity,
            rows.clone(),
        )?)
        .map_err(|err| TestkitError::new(format!("model L0 install failed: {err}")))?;
    for row in rows {
        model.push(row)?;
    }
    Ok(())
}

fn assert_model_matches_state(
    script: &[u8],
    step: usize,
    model: &ModelBranch,
    state: &BranchLocalState,
) -> Result<(), TestkitError> {
    let view = state
        .capture_read_view()
        .map_err(|err| TestkitError::new(format!("model view capture failed: {err}")))?
        .with_timestamp_coverage(BranchTimestampCoverage::complete());
    let version_bound = BranchReadBound::at_version(CommitVersion::new(1 + step as u64));
    let timestamp_bound = BranchReadBound::at_timestamp(Timestamp::from_micros(
        1 + u64::from(script_byte(script, 240 + (step % 16))) % 96,
    ));

    for key_index in 0..MODEL_KEY_COUNT {
        let key = model_physical_key(model.branch, key_index)?;
        assert_model_point(&view, model, &key, BranchReadBound::latest(), "latest")?;
        assert_model_point(&view, model, &key, version_bound, "version")?;
        assert_model_point(&view, model, &key, timestamp_bound, "timestamp")?;

        let actual_history = view
            .history(&key, BranchHistoryOptions::all())
            .map_err(|err| TestkitError::new(format!("model history failed: {err}")))?
            .into_iter()
            .map(|row| row.row().clone())
            .collect::<Vec<_>>();
        let expected_history = model.history(&key);
        if actual_history != expected_history {
            return Err(TestkitError::new("model history mismatch"));
        }
    }

    assert_model_scan(&view, model, BranchReadBound::latest(), "latest scan")?;
    assert_model_scan(&view, model, timestamp_bound, "timestamp scan")?;
    Ok(())
}

fn assert_model_point(
    view: &BranchReadView,
    model: &ModelBranch,
    key: &PhysicalKey,
    bound: BranchReadBound,
    label: &'static str,
) -> Result<(), TestkitError> {
    let actual = visible_row(view, key, bound)?;
    let expected = model.visible(key, bound);
    if actual != expected {
        return Err(TestkitError::new(format!(
            "model {label} mismatch for key {:?}",
            key.user_key()
        )));
    }
    Ok(())
}

fn assert_model_scan(
    view: &BranchReadView,
    model: &ModelBranch,
    bound: BranchReadBound,
    label: &'static str,
) -> Result<(), TestkitError> {
    let prefix = BranchScanBounds::prefix(&physical_key(model.branch, b"model-key-".to_vec())?);
    let actual = view
        .scan_prefix(&prefix, bound)
        .map_err(|err| TestkitError::new(format!("model scan failed: {err}")))?
        .into_iter()
        .map(|row| row.row().clone())
        .collect::<Vec<_>>();
    let expected = model.scan_visible(model.branch, bound)?;
    if actual != expected {
        return Err(TestkitError::new(format!("model {label} mismatch")));
    }
    Ok(())
}

fn model_row_matches_bound(row: &StorageRow, bound: BranchReadBound) -> bool {
    match bound {
        BranchReadBound::Latest => true,
        BranchReadBound::AtVersion(version) => row.commit_version().as_u64() <= version.as_u64(),
        BranchReadBound::AtTimestamp(timestamp) => {
            row.commit_timestamp().as_micros() <= timestamp.as_micros()
        }
    }
}

fn model_row_is_expired_at(row: &StorageRow, read_timestamp: Option<Timestamp>) -> bool {
    read_timestamp.is_some_and(|timestamp| {
        !row.is_tombstone() && row.expires_at() != Timestamp::EPOCH && row.expires_at() <= timestamp
    })
}

fn model_put_row(
    branch: BranchId,
    opcode: u8,
    version: u64,
    step: usize,
) -> Result<StorageRow, TestkitError> {
    storage_row_with(
        branch,
        model_user_key(opcode),
        version,
        model_timestamp(opcode, step),
        Timestamp::EPOCH,
        vec![opcode, u8::try_from(step % 251).expect("step byte")],
    )
}

fn model_expiring_row(
    branch: BranchId,
    opcode: u8,
    version: u64,
    step: usize,
) -> Result<StorageRow, TestkitError> {
    let timestamp = model_timestamp(opcode, step);
    storage_row_with(
        branch,
        model_user_key(opcode),
        version,
        timestamp,
        Timestamp::from_micros(timestamp.saturating_add(3)),
        vec![opcode, 0xee],
    )
}

fn model_tombstone_row(
    branch: BranchId,
    opcode: u8,
    version: u64,
    step: usize,
) -> Result<StorageRow, TestkitError> {
    tombstone_row(
        branch,
        model_user_key(opcode),
        version,
        model_timestamp(opcode, step),
    )
}

fn model_user_key(opcode: u8) -> Vec<u8> {
    format!("model-key-{}", opcode % MODEL_KEY_COUNT).into_bytes()
}

fn model_physical_key(branch: BranchId, key_index: u8) -> Result<PhysicalKey, TestkitError> {
    physical_key(branch, format!("model-key-{key_index}").into_bytes())
}

fn model_timestamp(opcode: u8, step: usize) -> u64 {
    1 + u64::from(opcode % 89) + u64::try_from(step % 7).expect("step fits in u64")
}

fn check_materialization_fault_window(
    script: &[u8],
    source: BranchId,
    child: BranchId,
) -> Result<(), TestkitError> {
    let other_source = branch_id(script_byte(script, 231).wrapping_add(2));
    let materialized_layer = branch_inherited_layer(
        source,
        CommitVersion::new(5),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            source,
            BranchLevel::ZERO,
            "fault-materialize-source",
            vec![storage_row_with(
                source,
                b"fault-materialize-a".to_vec(),
                5,
                50,
                Timestamp::EPOCH,
                vec![script_byte(script, 232)],
            )?],
        )?]],
    )?;
    let colliding_layer = branch_inherited_layer(
        other_source,
        CommitVersion::new(4),
        InheritedLayerStatus::Active,
        vec![vec![branch_owned_table(
            other_source,
            BranchLevel::ZERO,
            "fault-materialize-layer-0-table-0",
            vec![storage_row_with(
                other_source,
                b"fault-materialize-b".to_vec(),
                4,
                40,
                Timestamp::EPOCH,
                vec![script_byte(script, 233)],
            )?],
        )?]],
    )?;
    let mut state = BranchLocalState::empty(child);
    state
        .attach_inherited_layers(vec![materialized_layer, colliding_layer])
        .map_err(|err| TestkitError::new(format!("fault materialization attach failed: {err}")))?;
    let before = state.clone();
    expect_invalid_inherited_layer(state.materialize_inherited_layer(
        &BranchMaterializationRequest::new(child, 0, "fault-materialize").map_err(|err| {
            TestkitError::new(format!("fault materialization request failed: {err}"))
        })?,
    ))?;
    if state != before {
        return Err(TestkitError::new("materialization fault mutated state"));
    }
    Ok(())
}

fn check_snapshot_fault_window(
    script: &[u8],
    lower: BranchId,
    higher: BranchId,
) -> Result<(), TestkitError> {
    let request = BranchSnapshotInstallRequest::new(
        "fault-snapshot",
        vec![
            BranchSnapshotInstallGroup::new(
                lower,
                sorted_snapshot_rows(vec![storage_row_with(
                    lower,
                    b"fault-snapshot-ok".to_vec(),
                    1,
                    10,
                    Timestamp::EPOCH,
                    vec![script_byte(script, 234)],
                )?]),
            ),
            BranchSnapshotInstallGroup::new(
                higher,
                sorted_snapshot_rows(vec![storage_row_with(
                    higher,
                    vec![script_byte(script, 235).max(1); 70 * 1024],
                    1,
                    10,
                    Timestamp::EPOCH,
                    vec![script_byte(script, 236)],
                )?]),
            ),
        ],
    )
    .map_err(|err| TestkitError::new(format!("fault snapshot request failed: {err}")))?;
    let mut branches = vec![
        BranchLocalState::empty(lower),
        BranchLocalState::empty(higher),
    ];
    let before = branches.clone();
    if install_snapshot_rows_into_branches(&mut branches, &request).is_ok() {
        return Err(TestkitError::new("snapshot build fault was accepted"));
    }
    if branches != before {
        return Err(TestkitError::new("snapshot build fault mutated state"));
    }
    Ok(())
}

fn compaction_state_with_l0_tables(
    branch: BranchId,
    identity_prefix: &str,
    tables: Vec<Vec<StorageRow>>,
) -> Result<BranchLocalState, TestkitError> {
    let mut state = BranchLocalState::new(branch, compaction_runtime_config()?)
        .map_err(|err| TestkitError::new(format!("compaction state failed: {err}")))?;
    for (index, rows) in tables.into_iter().enumerate() {
        state
            .install_l0_table(branch_owned_table(
                branch,
                BranchLevel::ZERO,
                &format!("{identity_prefix}-l0-{index}"),
                rows,
            )?)
            .map_err(|err| TestkitError::new(format!("compaction L0 install failed: {err}")))?;
    }
    Ok(state)
}

fn compaction_runtime_config() -> Result<BranchRuntimeConfig, TestkitError> {
    BranchRuntimeConfig::new(3, 64, 32)
        .map_err(|err| TestkitError::new(format!("compaction config failed: {err}")))
}

fn branch_compaction_request(
    branch: BranchId,
    kind: BranchCompactionKind,
    output_identity_seed: impl Into<String>,
) -> Result<BranchCompactionRequest, TestkitError> {
    BranchCompactionRequest::new(branch, kind, output_identity_seed)
        .map_err(|err| TestkitError::new(format!("compaction request failed: {err}")))
}

fn visible_row(
    view: &BranchReadView,
    key: &PhysicalKey,
    bound: BranchReadBound,
) -> Result<Option<StorageRow>, TestkitError> {
    view.read_point(key, bound)
        .map(|row| row.map(|row| row.row().clone()))
        .map_err(|err| TestkitError::new(format!("compaction read failed: {err}")))
}

fn check_branch_local_append_path(
    script: &[u8],
    branch: BranchId,
    state: &mut BranchLocalState,
    outcome: &mut StateOutcome,
) -> Result<StorageRow, TestkitError> {
    let put = storage_row_with(
        branch,
        user_key(script, 56),
        5,
        50,
        Timestamp::from_micros(60),
        vec![script_byte(script, 60), 0x00],
    )?;
    append_expect_put(state, &put)?;
    outcome.committed_put_appends += 1;
    outcome.active_only_facts += 1;

    let wrong_branch_row = storage_row_with(
        branch_id(script_byte(script, 55).wrapping_add(1)),
        user_key(script, 61),
        9,
        90,
        Timestamp::EPOCH,
        b"secret-payload".to_vec(),
    )?;
    let facts_before_wrong_branch = branch_state_facts(state)?;
    expect_invalid_branch_row(state.append_committed_row(wrong_branch_row))?;
    if branch_state_facts(state)? != facts_before_wrong_branch {
        return Err(TestkitError::new(
            "wrong-branch append changed branch-local facts",
        ));
    }
    outcome.wrong_branch_append_rejections += 1;

    let facts_before_duplicate = branch_state_facts(state)?;
    expect_duplicate_internal_key(state.append_committed_row(put.clone()))?;
    if branch_state_facts(state)? != facts_before_duplicate {
        return Err(TestkitError::new(
            "active duplicate append changed branch-local facts",
        ));
    }
    outcome.active_duplicate_rejections += 1;

    let same_physical_key_older = storage_row_with(
        branch,
        put.physical_key().user_key().to_vec(),
        4,
        40,
        Timestamp::from_micros(40),
        Vec::new(),
    )?;
    append_expect_put(state, &same_physical_key_older)?;
    outcome.same_key_version_appends += 1;

    let mut other_key = user_key(script, 65);
    other_key.push(0x01);
    let same_version_other_key = storage_row_with(
        branch,
        other_key,
        5,
        70,
        Timestamp::EPOCH,
        vec![script_byte(script, 69)],
    )?;
    append_expect_put(state, &same_version_other_key)?;
    outcome.same_version_key_appends += 1;

    let tombstone = tombstone_row(branch, user_key(script, 70), 11, 30)?;
    let tombstone_key = TableInternalKeyBytes::from_row(&tombstone);
    let tombstone_outcome = state
        .append_committed_row(tombstone.clone())
        .map_err(|err| TestkitError::new(format!("tombstone append failed: {err}")))?;
    if !tombstone_outcome.is_tombstone()
        || state.tombstone_rows() != 1
        || state
            .active()
            .get(&tombstone_key)
            .is_none_or(|stored| stored.row() != &tombstone)
    {
        return Err(TestkitError::new("tombstone append facts drifted"));
    }
    outcome.committed_tombstone_appends += 1;

    let mixed_facts = branch_state_facts(state)?;
    if mixed_facts.active_rows() != 4
        || mixed_facts.frozen_table_count() != 0
        || mixed_facts.max_commit_version() != Some(CommitVersion::new(11))
        || mixed_facts.timestamp_min() != Some(Timestamp::from_micros(30))
        || mixed_facts.timestamp_max() != Some(Timestamp::from_micros(70))
    {
        return Err(TestkitError::new("active branch-local facts drifted"));
    }
    Ok(put)
}

fn check_branch_local_rotation_path(
    script: &[u8],
    branch: BranchId,
    state: &mut BranchLocalState,
    outcome: &mut StateOutcome,
    put: &StorageRow,
) -> Result<(), TestkitError> {
    outcome.active_rotations += check_successful_rotation(state, 4, 1)?;
    let frozen_only = branch_state_facts(state)?;
    if frozen_only.active_rows() != 0 || frozen_only.frozen_table_count() != 1 {
        return Err(TestkitError::new("frozen-only branch facts drifted"));
    }
    outcome.frozen_only_facts += 1;

    let duplicate_frozen = storage_row_with(
        branch,
        put.physical_key().user_key().to_vec(),
        5,
        50,
        Timestamp::EPOCH,
        b"duplicate".to_vec(),
    )?;
    let facts_before_frozen_duplicate = branch_state_facts(state)?;
    expect_duplicate_internal_key(state.append_committed_row(duplicate_frozen))?;
    if branch_state_facts(state)? != facts_before_frozen_duplicate {
        return Err(TestkitError::new(
            "frozen duplicate append changed branch-local facts",
        ));
    }
    outcome.frozen_duplicate_rejections += 1;

    let later = storage_row_with(
        branch,
        user_key(script, 74),
        12,
        120,
        Timestamp::EPOCH,
        vec![script_byte(script, 78)],
    )?;
    state
        .append_committed_row(later)
        .map_err(|err| TestkitError::new(format!("mixed append failed: {err}")))?;
    let mixed = branch_state_facts(state)?;
    if mixed.active_rows() != 1
        || mixed.frozen_table_count() != 1
        || mixed.max_commit_version() != Some(CommitVersion::new(12))
        || mixed.timestamp_max() != Some(Timestamp::from_micros(120))
    {
        return Err(TestkitError::new("mixed active/frozen facts drifted"));
    }
    outcome.mixed_active_frozen_facts += 1;
    Ok(())
}

fn check_branch_local_edge_facts(
    branch: BranchId,
    outcome: &mut StateOutcome,
) -> Result<(), TestkitError> {
    let mut state = BranchLocalState::empty(branch);
    let zero = storage_row_with(
        branch,
        b"generated-zero-edge".to_vec(),
        0,
        0,
        Timestamp::EPOCH,
        Vec::new(),
    )?;
    let max = storage_row_with(
        branch,
        b"generated-max-edge".to_vec(),
        u64::MAX,
        u64::MAX,
        Timestamp::MAX,
        b"max".to_vec(),
    )?;

    append_expect_put(&mut state, &zero)?;
    let zero_facts = branch_state_facts(&state)?;
    if zero_facts.max_commit_version() != Some(CommitVersion::ZERO)
        || zero_facts.timestamp_min() != Some(Timestamp::EPOCH)
        || zero_facts.timestamp_max() != Some(Timestamp::EPOCH)
    {
        return Err(TestkitError::new(
            "zero version/timestamp branch-local edge facts drifted",
        ));
    }

    append_expect_put(&mut state, &max)?;
    let max_facts = branch_state_facts(&state)?;
    if max_facts.max_commit_version() != Some(CommitVersion::MAX)
        || max_facts.timestamp_min() != Some(Timestamp::EPOCH)
        || max_facts.timestamp_max() != Some(Timestamp::MAX)
    {
        return Err(TestkitError::new(
            "max version/timestamp branch-local edge facts drifted",
        ));
    }

    check_successful_rotation(&mut state, 2, 1)?;
    let rotated = branch_state_facts(&state)?;
    if rotated.active_rows() != 0
        || rotated.frozen_table_count() != 1
        || rotated.max_commit_version() != Some(CommitVersion::MAX)
        || rotated.timestamp_min() != Some(Timestamp::EPOCH)
        || rotated.timestamp_max() != Some(Timestamp::MAX)
    {
        return Err(TestkitError::new("rotated branch-local edge facts drifted"));
    }

    outcome.timestamp_edge_facts += 1;
    outcome.max_commit_edge_facts += 1;
    Ok(())
}

fn check_empty_rotation(state: &mut BranchLocalState) -> Result<usize, TestkitError> {
    match state.rotate_active() {
        BranchRotationOutcome::Skipped {
            reason: BranchRotationSkipReason::EmptyActive,
        } if state.frozen().is_empty() => Ok(1),
        other => Err(TestkitError::new(format!(
            "empty rotation returned unexpected outcome: {other:?}"
        ))),
    }
}

fn check_successful_rotation(
    state: &mut BranchLocalState,
    expected_rows: usize,
    expected_tables: usize,
) -> Result<usize, TestkitError> {
    match state.rotate_active() {
        BranchRotationOutcome::Rotated {
            frozen_index: 0,
            frozen_rows,
            frozen_tables,
        } if frozen_rows == expected_rows
            && frozen_tables == expected_tables
            && state.active().is_empty()
            && state.frozen_table_count() == expected_tables =>
        {
            Ok(1)
        }
        other => Err(TestkitError::new(format!(
            "active rotation returned unexpected outcome: {other:?}"
        ))),
    }
}

fn check_frozen_limit_skip(branch: BranchId) -> Result<usize, TestkitError> {
    let config = BranchRuntimeConfig::new(7, 64, 1)
        .map_err(|err| TestkitError::new(format!("limit config failed: {err}")))?;
    let mut state = BranchLocalState::new(branch, config)
        .map_err(|err| TestkitError::new(format!("limit state failed: {err}")))?;
    let first = storage_row_with(
        branch,
        b"limit-first".to_vec(),
        1,
        10,
        Timestamp::EPOCH,
        b"first".to_vec(),
    )?;
    let second = storage_row_with(
        branch,
        b"limit-second".to_vec(),
        2,
        20,
        Timestamp::EPOCH,
        b"second".to_vec(),
    )?;
    state
        .append_committed_row(first.clone())
        .map_err(|err| TestkitError::new(format!("limit first append failed: {err}")))?;
    check_successful_rotation(&mut state, 1, 1)?;
    state
        .append_committed_row(second.clone())
        .map_err(|err| TestkitError::new(format!("limit second append failed: {err}")))?;
    let before_skip = branch_state_facts(&state)?;

    match state.rotate_active() {
        BranchRotationOutcome::Skipped {
            reason: BranchRotationSkipReason::FrozenLimitReached,
        } if branch_state_facts(&state)? == before_skip
            && state
                .active()
                .get(&TableInternalKeyBytes::from_row(&second))
                .is_some()
            && state.frozen()[0]
                .get(&TableInternalKeyBytes::from_row(&first))
                .is_some() => {}
        other => {
            return Err(TestkitError::new(format!(
                "frozen-limit rotation returned unexpected outcome: {other:?}"
            )))
        }
    }

    let third = storage_row_with(
        branch,
        b"limit-third".to_vec(),
        3,
        30,
        Timestamp::EPOCH,
        b"third".to_vec(),
    )?;
    state
        .append_committed_row(third.clone())
        .map_err(|err| TestkitError::new(format!("limit third append failed: {err}")))?;
    if state.active_row_count() != 2
        || state.frozen_table_count() != 1
        || state
            .active()
            .get(&TableInternalKeyBytes::from_row(&third))
            .is_none_or(|stored| stored.row() != &third)
    {
        return Err(TestkitError::new(
            "append after frozen-limit skip did not preserve active state",
        ));
    }
    Ok(1)
}

fn append_expect_put(state: &mut BranchLocalState, row: &StorageRow) -> Result<(), TestkitError> {
    let key = TableInternalKeyBytes::from_row(row);
    let outcome = state
        .append_committed_row(row.clone())
        .map_err(|err| TestkitError::new(format!("put append failed: {err}")))?;
    if outcome.is_tombstone()
        || outcome.commit_version() != row.commit_version()
        || outcome.commit_timestamp() != row.commit_timestamp()
        || state
            .active()
            .get(&key)
            .is_none_or(|stored| stored.row() != row)
    {
        return Err(TestkitError::new("put append facts drifted"));
    }
    Ok(())
}

fn expect_invalid_branch_row<T>(result: Result<T, BranchRuntimeError>) -> Result<(), TestkitError> {
    match result {
        Err(BranchRuntimeError::InvalidBranchRow { .. }) => Ok(()),
        Err(err) => Err(TestkitError::new(format!(
            "wrong-branch append returned wrong error: {err}"
        ))),
        Ok(_) => Err(TestkitError::new("wrong-branch append succeeded")),
    }
}

fn expect_duplicate_internal_key<T>(
    result: Result<T, BranchRuntimeError>,
) -> Result<(), TestkitError> {
    match result {
        Err(BranchRuntimeError::TableRuntime {
            source: crate::table::TableRuntimeError::DuplicateInternalKey { .. },
        }) => Ok(()),
        Err(err) => Err(TestkitError::new(format!(
            "duplicate append returned wrong error: {err}"
        ))),
        Ok(_) => Err(TestkitError::new("duplicate append succeeded")),
    }
}

fn expect_invalid_inherited_layer<T>(
    result: Result<T, BranchRuntimeError>,
) -> Result<(), TestkitError> {
    match result {
        Err(BranchRuntimeError::InvalidInheritedLayer { .. }) => Ok(()),
        Err(err) => Err(TestkitError::new(format!(
            "invalid inherited layer returned wrong error: {err}"
        ))),
        Ok(_) => Err(TestkitError::new("invalid inherited layer was accepted")),
    }
}

fn branch_state_facts(state: &BranchLocalState) -> Result<BranchStateFacts, TestkitError> {
    state.facts().map_err(|error| state_fact_error(&error))
}

fn state_fact_error(error: &BranchRuntimeError) -> TestkitError {
    TestkitError::new(format!("branch-local facts failed: {error}"))
}

fn expect_invalid_config<T>(result: Result<T, BranchRuntimeError>) -> Result<(), TestkitError> {
    match result {
        Err(BranchRuntimeError::InvalidConfig { .. }) => Ok(()),
        Err(err) => Err(TestkitError::new(format!(
            "invalid config returned wrong error: {err}"
        ))),
        Ok(_) => Err(TestkitError::new("invalid branch config was accepted")),
    }
}

fn expect_invalid_state<T>(result: Result<T, BranchRuntimeError>) -> Result<(), TestkitError> {
    match result {
        Err(BranchRuntimeError::InvalidBranchState { .. }) => Ok(()),
        Err(err) => Err(TestkitError::new(format!(
            "invalid branch facts returned wrong error: {err}"
        ))),
        Ok(_) => Err(TestkitError::new("invalid branch facts were accepted")),
    }
}

fn expect_invalid_reachability<T>(
    result: Result<T, BranchRuntimeError>,
) -> Result<(), TestkitError> {
    match result {
        Err(BranchRuntimeError::InvalidReachability { .. }) => Ok(()),
        Err(err) => Err(TestkitError::new(format!(
            "invalid reachability returned wrong error: {err}"
        ))),
        Ok(_) => Err(TestkitError::new("invalid reachability was accepted")),
    }
}

fn expect_invalid_compaction<T>(result: Result<T, BranchRuntimeError>) -> Result<(), TestkitError> {
    match result {
        Err(BranchRuntimeError::InvalidCompaction { .. }) => Ok(()),
        Err(err) => Err(TestkitError::new(format!(
            "invalid compaction returned wrong error: {err}"
        ))),
        Ok(_) => Err(TestkitError::new("invalid compaction was accepted")),
    }
}

fn expect_invalid_snapshot_install<T>(
    result: Result<T, BranchRuntimeError>,
    expected_reason: &'static str,
) -> Result<(), TestkitError> {
    match result {
        Err(BranchRuntimeError::InvalidSnapshotInstall { reason }) if reason == expected_reason => {
            Ok(())
        }
        Err(err) => Err(TestkitError::new(format!(
            "invalid snapshot install returned wrong error: {err}"
        ))),
        Ok(_) => Err(TestkitError::new("invalid snapshot install was accepted")),
    }
}

fn expect_missing_snapshot_branch<T>(
    result: Result<T, BranchRuntimeError>,
    expected_branch: BranchId,
) -> Result<(), TestkitError> {
    match result {
        Err(BranchRuntimeError::BranchNotFound { branch_id }) if branch_id == expected_branch => {
            Ok(())
        }
        Err(err) => Err(TestkitError::new(format!(
            "missing snapshot branch returned wrong error: {err}"
        ))),
        Ok(_) => Err(TestkitError::new("missing snapshot branch was accepted")),
    }
}

fn branch_state_by_id(
    branches: &[BranchLocalState],
    branch_id: BranchId,
) -> Result<&BranchLocalState, TestkitError> {
    branches
        .iter()
        .find(|state| state.branch_id() == branch_id)
        .ok_or_else(|| TestkitError::new("snapshot branch state missing after install"))
}

fn sorted_snapshot_rows(rows: Vec<StorageRow>) -> Vec<StorageRow> {
    let mut rows = rows.into_iter().map(TableRow::new).collect::<Vec<_>>();
    sort_table_rows_by_key(&mut rows);
    rows.into_iter().map(TableRow::into_row).collect()
}

fn branch_id(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}

fn table_identity(identity: &str) -> Result<TableIdentity, TestkitError> {
    TableIdentity::new(identity)
        .map_err(|err| TestkitError::new(format!("table identity failed: {err}")))
}

fn branch_owned_table(
    branch_id: BranchId,
    level: BranchLevel,
    identity: &str,
    rows: Vec<StorageRow>,
) -> Result<BranchOwnedTable, TestkitError> {
    let reader = immutable_reader(identity, rows)?;
    let descriptor = branch_table_descriptor(level, &reader)?;
    BranchOwnedTable::new(branch_id, descriptor, reader)
        .map_err(|err| TestkitError::new(format!("branch-owned table failed: {err}")))
}

fn branch_inherited_layer(
    source_branch_id: BranchId,
    fork_version: CommitVersion,
    status: InheritedLayerStatus,
    owned_levels: Vec<Vec<BranchOwnedTable>>,
) -> Result<BranchInheritedLayer, TestkitError> {
    let table_count = owned_levels.iter().map(Vec::len).sum();
    let descriptor =
        InheritedLayerDescriptor::new(source_branch_id, fork_version, status, table_count);
    BranchInheritedLayer::new(descriptor, owned_levels)
        .map_err(|err| TestkitError::new(format!("branch inherited layer failed: {err}")))
}

fn branch_inherited_layer_unchecked_for_fork_gate_checks(
    source_branch_id: BranchId,
    fork_version: CommitVersion,
    status: InheritedLayerStatus,
    owned_levels: Vec<Vec<BranchOwnedTable>>,
) -> BranchInheritedLayer {
    let table_count = owned_levels.iter().map(Vec::len).sum();
    let descriptor =
        InheritedLayerDescriptor::new(source_branch_id, fork_version, status, table_count);
    BranchInheritedLayer::new_unchecked_for_test(descriptor, owned_levels)
}

fn branch_table_descriptor(
    level: BranchLevel,
    reader: &ImmutableTableReader,
) -> Result<BranchTableDescriptor, TestkitError> {
    BranchTableDescriptor::new(
        reader.facts().identity().clone(),
        reader.facts().clone(),
        level,
    )
    .map_err(|err| TestkitError::new(format!("branch table descriptor failed: {err}")))
}

fn immutable_reader(
    identity: &str,
    rows: Vec<StorageRow>,
) -> Result<ImmutableTableReader, TestkitError> {
    let mut rows = rows.into_iter().map(TableRow::new).collect::<Vec<_>>();
    sort_table_rows_by_key(&mut rows);
    let identity = TableIdentity::new(identity)
        .map_err(|err| TestkitError::new(format!("table identity failed: {err}")))?;
    let builder = ImmutableTableBuilder::new(TableBuilderConfig::default())
        .map_err(|err| TestkitError::new(format!("table builder failed: {err}")))?;
    let artifact = builder
        .build_from_rows(identity.clone(), &rows)
        .map_err(|err| TestkitError::new(format!("immutable table build failed: {err}")))?;
    ImmutableTableReader::open_bytes(
        identity,
        artifact.into_bytes(),
        TableReaderConfig::default(),
    )
    .map_err(|err| TestkitError::new(format!("immutable table reader failed: {err}")))
}

fn table_facts(identity: &str) -> Result<TableRuntimeFacts, TestkitError> {
    TableRuntimeFacts::new(
        TableIdentity::new(identity)
            .map_err(|err| TestkitError::new(format!("table identity failed: {err}")))?,
        1,
        1,
        TableKeyRange::new(vec![0x01], vec![0x02])
            .map_err(|err| TestkitError::new(format!("table key range failed: {err}")))?,
        TableCommitRange::new(CommitVersion::new(1), CommitVersion::new(1))
            .map_err(|err| TestkitError::new(format!("table commit range failed: {err}")))?,
        128,
    )
    .map_err(|err| TestkitError::new(format!("table facts failed: {err}")))
}

fn row_versions(rows: &[TableRow]) -> Vec<u64> {
    rows.iter()
        .map(|row| row.commit_version().as_u64())
        .collect()
}

fn matching_versions(rows: &[TableRow], bound: BranchEffectiveReadBound) -> Vec<u64> {
    rows.iter()
        .filter(|row| bound.matches_row(row.row()).matches_effective_bound())
        .map(|row| row.commit_version().as_u64())
        .collect()
}

fn history_versions(rows: &[BranchHistoryRow]) -> Vec<u64> {
    rows.iter()
        .map(|row| row.row().commit_version().as_u64())
        .collect()
}

fn visible_user_keys(rows: &[BranchVisibleRow]) -> Vec<Vec<u8>> {
    rows.iter()
        .map(|row| row.row().physical_key().user_key().to_vec())
        .collect()
}

fn storage_row(branch_id: BranchId, version: u64) -> Result<StorageRow, TestkitError> {
    storage_row_with(
        branch_id,
        b"key".to_vec(),
        version,
        version,
        Timestamp::EPOCH,
        b"row-bytes".to_vec(),
    )
}

fn storage_row_with(
    branch_id: BranchId,
    user_key: Vec<u8>,
    version: u64,
    timestamp: u64,
    expires_at: Timestamp,
    value: Vec<u8>,
) -> Result<StorageRow, TestkitError> {
    storage_row_with_space(
        branch_id,
        "default",
        StorageSpaceId::engine(0x20)
            .map_err(|err| TestkitError::new(format!("storage space failed: {err}")))?,
        user_key,
        version,
        timestamp,
        expires_at,
        value,
    )
}

fn storage_row_with_space(
    branch_id: BranchId,
    space_name: &str,
    space: StorageSpaceId,
    user_key: Vec<u8>,
    version: u64,
    timestamp: u64,
    expires_at: Timestamp,
    value: Vec<u8>,
) -> Result<StorageRow, TestkitError> {
    Ok(StorageRow::put(
        physical_key_with_space(branch_id, space_name, space, user_key)?,
        CommitVersion::new(version),
        Timestamp::from_micros(timestamp),
        expires_at,
        value,
    ))
}

fn tombstone_row(
    branch_id: BranchId,
    user_key: Vec<u8>,
    version: u64,
    timestamp: u64,
) -> Result<StorageRow, TestkitError> {
    Ok(StorageRow::tombstone(
        physical_key(branch_id, user_key)?,
        CommitVersion::new(version),
        Timestamp::from_micros(timestamp),
    ))
}

fn physical_key(branch_id: BranchId, user_key: Vec<u8>) -> Result<PhysicalKey, TestkitError> {
    let space = StorageSpaceId::engine(0x20)
        .map_err(|err| TestkitError::new(format!("storage space failed: {err}")))?;
    physical_key_with_space(branch_id, "default", space, user_key)
}

fn physical_key_with_space(
    branch_id: BranchId,
    space_name: &str,
    space: StorageSpaceId,
    user_key: Vec<u8>,
) -> Result<PhysicalKey, TestkitError> {
    PhysicalKey::new(branch_id, space_name, space, user_key)
        .map_err(|err| TestkitError::new(format!("physical key failed: {err}")))
}

fn user_key(script: &[u8], start: usize) -> Vec<u8> {
    vec![
        script_byte(script, start),
        0x00,
        script_byte(script, start + 1),
        script_byte(script, start + 2),
    ]
}

fn script_byte(script: &[u8], index: usize) -> u8 {
    script.get(index).copied().unwrap_or(0)
}

#[derive(Debug)]
struct LeafError;

impl fmt::Display for LeafError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("leaf source")
    }
}

impl Error for LeafError {}

#[cfg(test)]
mod tests {
    use super::{
        check_branch_lsm_inheritance_contract, check_branch_lsm_install_contract,
        check_branch_lsm_reads_contract, check_branch_lsm_scaffold_contract,
    };

    #[test]
    fn dedicated_branch_lsm_fuzz_contracts_exercise_their_surfaces() {
        let script = b"branch-lsm-dedicated-fuzz-contract-seed";

        check_branch_lsm_reads_contract(script).expect("branch read fuzz contract");
        check_branch_lsm_inheritance_contract(script).expect("branch inheritance fuzz contract");
        check_branch_lsm_install_contract(script).expect("branch install fuzz contract");
    }

    #[test]
    fn branch_lsm_scaffold_contract_checks_generated_scripts() {
        let outcome = check_branch_lsm_scaffold_contract(b"branch-lsm-scaffold-seed")
            .expect("branch scaffold contract");
        assert_ne!(outcome.latest_point_read_cases(), 0);
        assert_ne!(outcome.inherited_latest_read_cases(), 0);
        assert_ne!(outcome.compaction_output_install_cases(), 0);
        assert_ne!(outcome.snapshot_single_branch_install_cases(), 0);
    }
}
