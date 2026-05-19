//! Generated branch-LSM scaffold contract helpers.

use super::TestkitError;
use crate::branch::{
    require_row_branch, rewrite_physical_key_branch, rewrite_row_branch, row_matches_branch,
    BranchEffectiveReadBound, BranchForkOutcome, BranchHistoryOptions, BranchHistoryRow,
    BranchImmutableInstallOutcome, BranchInheritedLayer, BranchLevel, BranchLocalState,
    BranchOwnedTable, BranchReachabilityFacts, BranchReadBound, BranchRotationOutcome,
    BranchRotationSkipReason, BranchRowCandidateFacts, BranchRowSource, BranchRuntimeConfig,
    BranchRuntimeError, BranchRuntimeStats, BranchScanBounds, BranchStateDescriptor,
    BranchStateFacts, BranchTableDescriptor, BranchUserKeyBound, BranchViewDescriptor,
    BranchVisibleRow, InheritedLayerDescriptor, InheritedLayerStatus,
};
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use crate::table::{
    sort_table_rows_by_key, ImmutableTableBuilder, ImmutableTableReader, TableBuilderConfig,
    TableCommitRange, TableIdentity, TableInternalKeyBytes, TableKeyRange, TablePhysicalKeyBytes,
    TableReaderConfig, TableRow, TableRuntimeFacts,
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
    timestamp_bound_deferrals: usize,
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

    pub const fn timestamp_bound_deferral_cases(self) -> usize {
        self.timestamp_bound_deferrals
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
        self.timestamp_bound_deferrals += outcome.timestamp_bound_deferrals;
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

    Ok(outcome)
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
    timestamp_bound_deferrals: usize,
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

    if !matches!(
        after_prefix.read_point(
            read_key,
            BranchReadBound::at_timestamp(Timestamp::from_micros(30)),
        ),
        Err(BranchRuntimeError::InvalidReadBound { .. })
    ) {
        return Err(TestkitError::new("timestamp read was not deferred"));
    }
    if !matches!(
        after_prefix.scan_prefix(
            &BranchScanBounds::prefix(read_key),
            BranchReadBound::at_timestamp(Timestamp::from_micros(30)),
        ),
        Err(BranchRuntimeError::InvalidReadBound { .. })
    ) {
        return Err(TestkitError::new("timestamp scan was not deferred"));
    }
    outcome.timestamp_bound_deferrals += 1;

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
    source_state
        .append_committed_row(storage_row_with(
            source,
            b"generated-source-active-only".to_vec(),
            6,
            60,
            Timestamp::EPOCH,
            b"active-only".to_vec(),
        )?)
        .map_err(|err| TestkitError::new(format!("inheritance source append failed: {err}")))?;

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
        || fork_outcome.fork_version() != CommitVersion::new(6)
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
    let layer = branch_inherited_layer(
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
    )?;
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

fn branch_id(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
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
