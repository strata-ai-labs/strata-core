//! Generated branch-LSM scaffold contract helpers.

use super::TestkitError;
use crate::branch::{
    require_row_branch, rewrite_physical_key_branch, rewrite_row_branch, row_matches_branch,
    BranchEffectiveReadBound, BranchHistoryRow, BranchLevel, BranchLocalState,
    BranchReachabilityFacts, BranchReadBound, BranchRotationOutcome, BranchRotationSkipReason,
    BranchRowCandidateFacts, BranchRowSource, BranchRuntimeConfig, BranchRuntimeError,
    BranchRuntimeStats, BranchStateDescriptor, BranchStateFacts, BranchTableDescriptor,
    BranchViewDescriptor, BranchVisibleRow, InheritedLayerDescriptor, InheritedLayerStatus,
};
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use crate::table::{
    sort_table_rows_by_key, TableCommitRange, TableIdentity, TableInternalKeyBytes, TableKeyRange,
    TablePhysicalKeyBytes, TableRow, TableRuntimeFacts,
};
use std::error::Error;
use std::fmt;
use strata_core_next::{BranchId, CommitVersion, Timestamp};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
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
}

pub fn check_branch_lsm_scaffold_contract(
    script: &[u8],
) -> Result<BranchLsmScaffoldOutcome, TestkitError> {
    let mut outcome = BranchLsmScaffoldOutcome {
        valid_config: 0,
        invalid_config: 0,
        read_bounds: 0,
        valid_facts: 0,
        invalid_facts: 0,
        descriptors: 0,
        error_sources: 0,
        stats: 0,
        matching_rows: 0,
        mismatching_rows: 0,
        physical_key_rewrites: 0,
        row_rewrites: 0,
        own_bounds: 0,
        inherited_bounds: 0,
        candidate_puts: 0,
        candidate_tombstones: 0,
        edge_rows: 0,
        encoded_grouping: 0,
        row_chains: 0,
        fork_edges: 0,
        state_construction: 0,
        committed_put_appends: 0,
        committed_tombstone_appends: 0,
        wrong_branch_append_rejections: 0,
        active_duplicate_rejections: 0,
        frozen_duplicate_rejections: 0,
        same_key_version_appends: 0,
        same_version_key_appends: 0,
        active_rotations: 0,
        empty_rotation_skips: 0,
        frozen_limit_skips: 0,
        active_only_facts: 0,
        frozen_only_facts: 0,
        mixed_active_frozen_facts: 0,
        timestamp_edge_facts: 0,
        max_commit_edge_facts: 0,
    };

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

    let state_outcome = check_branch_local_state(script)?;
    outcome.state_construction += state_outcome.state_construction;
    outcome.committed_put_appends += state_outcome.committed_put_appends;
    outcome.committed_tombstone_appends += state_outcome.committed_tombstone_appends;
    outcome.wrong_branch_append_rejections += state_outcome.wrong_branch_append_rejections;
    outcome.active_duplicate_rejections += state_outcome.active_duplicate_rejections;
    outcome.frozen_duplicate_rejections += state_outcome.frozen_duplicate_rejections;
    outcome.same_key_version_appends += state_outcome.same_key_version_appends;
    outcome.same_version_key_appends += state_outcome.same_version_key_appends;
    outcome.active_rotations += state_outcome.active_rotations;
    outcome.empty_rotation_skips += state_outcome.empty_rotation_skips;
    outcome.frozen_limit_skips += state_outcome.frozen_limit_skips;
    outcome.active_only_facts += state_outcome.active_only_facts;
    outcome.frozen_only_facts += state_outcome.frozen_only_facts;
    outcome.mixed_active_frozen_facts += state_outcome.mixed_active_frozen_facts;
    outcome.timestamp_edge_facts += state_outcome.timestamp_edge_facts;
    outcome.max_commit_edge_facts += state_outcome.max_commit_edge_facts;

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
