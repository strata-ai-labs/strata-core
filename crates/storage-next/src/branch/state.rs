//! Branch-local state and descriptor shells.

use super::config::BranchRuntimeConfig;
use super::error::{BranchCompactionInvalidity, BranchRuntimeError, BranchRuntimeResult};
use super::facts::{
    BranchLevel, BranchReachabilitySnapshot, BranchTableDescriptor, BranchTableRef,
    BranchTableReferenceKind, InheritedLayerStatus,
};
use super::identity::{require_row_branch, rewrite_row_branch};
use super::pruning::{BranchCompactionPruningPolicy, BranchCompactionPruningProof};
use super::read::{
    require_table_physical_first_key, table_physical_ranges_overlap, BranchInheritedLayer,
    BranchMaterializationSource, BranchOwnedTable, BranchTimestampCoverage,
};
use crate::row::StorageRow;
use crate::table::{
    BuiltTableArtifact, FrozenTable, ImmutableTableBuilder, ImmutableTableReader,
    KeepAllTableCompactionPolicy, MutableTable, TableBuilderConfig, TableCompactionConfig,
    TableCompactionReport, TableCompactionSource, TableCompactionSourceId, TableCompactor,
    TableIdentity, TableInternalKeyBytes, TableReaderConfig, TableRuntimeError,
};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

pub(crate) mod append;
pub(crate) mod fork;
pub(crate) mod materialization;
pub(crate) mod read_hooks;
pub(crate) mod rotation;

pub(crate) use rotation::BranchRotationOutcome;

const SNAPSHOT_INSTALL_ROWS_PER_OUTPUT_TABLE: usize = 4_096;
const _: () = assert!(SNAPSHOT_INSTALL_ROWS_PER_OUTPUT_TABLE > 0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchImmutableInstallOutcome {
    branch_id: BranchId,
    level: BranchLevel,
    table_index: usize,
    level_table_count: usize,
    owned_table_count: usize,
    replaced_frozen_index: Option<usize>,
}

impl BranchImmutableInstallOutcome {
    pub(crate) const fn branch_id(self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn level(self) -> BranchLevel {
        self.level
    }

    pub(crate) const fn table_index(self) -> usize {
        self.table_index
    }

    pub(crate) const fn level_table_count(self) -> usize {
        self.level_table_count
    }

    pub(crate) const fn owned_table_count(self) -> usize {
        self.owned_table_count
    }

    pub(crate) const fn replaced_frozen_index(self) -> Option<usize> {
        self.replaced_frozen_index
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BranchCompactionKind {
    CompactL0,
    CompactL0ToLevelOne,
    CompactLevel {
        level: BranchLevel,
        table_index: usize,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BranchCompactionRetentionPolicy {
    KeepAll,
    DropOlderVersions,
    DropTombstones,
    DropExpired,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchCompactionRequest {
    branch_id: BranchId,
    kind: BranchCompactionKind,
    retention_policy: BranchCompactionRetentionPolicy,
    pruning_proof: Option<BranchCompactionPruningProof>,
    output_identity_seed: TableIdentity,
    table_compaction_config: TableCompactionConfig,
    table_builder_config: TableBuilderConfig,
}

impl BranchCompactionRequest {
    pub(crate) fn new(
        branch_id: BranchId,
        kind: BranchCompactionKind,
        output_identity_seed: impl Into<String>,
    ) -> BranchRuntimeResult<Self> {
        let output_identity_seed = TableIdentity::new(output_identity_seed.into())
            .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
        Ok(Self {
            branch_id,
            kind,
            retention_policy: BranchCompactionRetentionPolicy::KeepAll,
            pruning_proof: None,
            output_identity_seed,
            table_compaction_config: TableCompactionConfig::default(),
            table_builder_config: TableBuilderConfig::default(),
        })
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn kind(&self) -> BranchCompactionKind {
        self.kind
    }

    pub(crate) const fn retention_policy(&self) -> BranchCompactionRetentionPolicy {
        self.retention_policy
    }

    pub(crate) const fn output_identity_seed(&self) -> &TableIdentity {
        &self.output_identity_seed
    }

    pub(crate) const fn pruning_proof(&self) -> Option<BranchCompactionPruningProof> {
        self.pruning_proof
    }

    pub(crate) const fn table_compaction_config(&self) -> TableCompactionConfig {
        self.table_compaction_config
    }

    pub(crate) const fn table_builder_config(&self) -> TableBuilderConfig {
        self.table_builder_config
    }

    pub(crate) fn with_retention_policy(
        mut self,
        retention_policy: BranchCompactionRetentionPolicy,
    ) -> Self {
        self.retention_policy = retention_policy;
        self
    }

    pub(crate) fn with_pruning_proof(mut self, proof: BranchCompactionPruningProof) -> Self {
        self.pruning_proof = Some(proof);
        self
    }

    pub(crate) fn with_table_compaction_config(
        mut self,
        table_compaction_config: TableCompactionConfig,
    ) -> Self {
        self.table_compaction_config = table_compaction_config;
        self
    }

    pub(crate) fn with_table_builder_config(
        mut self,
        table_builder_config: TableBuilderConfig,
    ) -> Self {
        self.table_builder_config = table_builder_config;
        self
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BranchCompactionNoopReason {
    EmptyInputLevel,
    NotEnoughInputTables,
    LastLevel,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BranchCompactionRecovery {
    NoCandidate { reason: BranchCompactionNoopReason },
    InstalledReplacementTables,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchCompactionCandidate {
    branch_id: BranchId,
    input_refs: Vec<BranchTableRef>,
    overlap_refs: Vec<BranchTableRef>,
    output_level: BranchLevel,
    bottommost_for_branch: bool,
    source_count: usize,
    input_row_count: u64,
}

impl BranchCompactionCandidate {
    fn new(
        branch_id: BranchId,
        input_refs: Vec<BranchTableRef>,
        overlap_refs: Vec<BranchTableRef>,
        output_level: BranchLevel,
        bottommost_for_branch: bool,
        input_row_count: u64,
    ) -> Self {
        let source_count = input_refs.len().saturating_add(overlap_refs.len());
        Self {
            branch_id,
            input_refs,
            overlap_refs,
            output_level,
            bottommost_for_branch,
            source_count,
            input_row_count,
        }
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) fn input_refs(&self) -> &[BranchTableRef] {
        &self.input_refs
    }

    pub(crate) fn overlap_refs(&self) -> &[BranchTableRef] {
        &self.overlap_refs
    }

    pub(crate) const fn output_level(&self) -> BranchLevel {
        self.output_level
    }

    pub(crate) const fn bottommost_for_branch(&self) -> bool {
        self.bottommost_for_branch
    }

    pub(crate) const fn source_count(&self) -> usize {
        self.source_count
    }

    pub(crate) const fn input_row_count(&self) -> u64 {
        self.input_row_count
    }

    fn removed_refs(&self) -> Vec<BranchTableRef> {
        let mut refs = self.input_refs.clone();
        refs.extend(self.overlap_refs.clone());
        refs
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchCompactionPlan {
    branch_id: BranchId,
    kind: BranchCompactionKind,
    candidate: Option<BranchCompactionCandidate>,
    noop_reason: Option<BranchCompactionNoopReason>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchCompactionPreparedOutput {
    branch_id: BranchId,
    output_level: BranchLevel,
    artifacts: Vec<BuiltTableArtifact>,
    report: TableCompactionReport,
    materialization_source: Option<BranchMaterializationSource>,
}

impl BranchCompactionPlan {
    fn with_candidate(
        branch_id: BranchId,
        kind: BranchCompactionKind,
        candidate: BranchCompactionCandidate,
    ) -> Self {
        Self {
            branch_id,
            kind,
            candidate: Some(candidate),
            noop_reason: None,
        }
    }

    fn no_candidate(
        branch_id: BranchId,
        kind: BranchCompactionKind,
        noop_reason: BranchCompactionNoopReason,
    ) -> Self {
        Self {
            branch_id,
            kind,
            candidate: None,
            noop_reason: Some(noop_reason),
        }
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn kind(&self) -> BranchCompactionKind {
        self.kind
    }

    pub(crate) const fn candidate(&self) -> Option<&BranchCompactionCandidate> {
        self.candidate.as_ref()
    }

    pub(crate) const fn noop_reason(&self) -> Option<BranchCompactionNoopReason> {
        self.noop_reason
    }
}

impl BranchCompactionPreparedOutput {
    pub(crate) const fn output_level(&self) -> BranchLevel {
        self.output_level
    }

    pub(crate) fn artifacts(&self) -> &[BuiltTableArtifact] {
        &self.artifacts
    }

    pub(crate) const fn report(&self) -> &TableCompactionReport {
        &self.report
    }

    pub(crate) const fn materialization_source(&self) -> Option<BranchMaterializationSource> {
        self.materialization_source
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchCompactionOutcome {
    branch_id: BranchId,
    recovery: BranchCompactionRecovery,
    candidate: Option<BranchCompactionCandidate>,
    output_refs: Vec<BranchTableRef>,
    removed_refs: Vec<BranchTableRef>,
    table_report: Option<TableCompactionReport>,
    owned_table_count: usize,
}

impl BranchCompactionOutcome {
    fn no_candidate(
        branch_id: BranchId,
        reason: BranchCompactionNoopReason,
        owned_table_count: usize,
    ) -> Self {
        Self {
            branch_id,
            recovery: BranchCompactionRecovery::NoCandidate { reason },
            candidate: None,
            output_refs: Vec::new(),
            removed_refs: Vec::new(),
            table_report: None,
            owned_table_count,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn installed(
        branch_id: BranchId,
        candidate: BranchCompactionCandidate,
        output_refs: Vec<BranchTableRef>,
        removed_refs: Vec<BranchTableRef>,
        table_report: TableCompactionReport,
        owned_table_count: usize,
    ) -> Self {
        Self {
            branch_id,
            recovery: BranchCompactionRecovery::InstalledReplacementTables,
            candidate: Some(candidate),
            output_refs,
            removed_refs,
            table_report: Some(table_report),
            owned_table_count,
        }
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn recovery(&self) -> BranchCompactionRecovery {
        self.recovery
    }

    pub(crate) const fn candidate(&self) -> Option<&BranchCompactionCandidate> {
        self.candidate.as_ref()
    }

    pub(crate) fn output_refs(&self) -> &[BranchTableRef] {
        &self.output_refs
    }

    pub(crate) fn removed_refs(&self) -> &[BranchTableRef] {
        &self.removed_refs
    }

    pub(crate) const fn table_report(&self) -> Option<&TableCompactionReport> {
        self.table_report.as_ref()
    }

    pub(crate) const fn owned_table_count(&self) -> usize {
        self.owned_table_count
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BranchSnapshotMissingBranchPolicy {
    Reject,
    Create { config: BranchRuntimeConfig },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BranchSnapshotTargetStatePolicy {
    RequireEmpty,
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct BranchSnapshotInstallGroup {
    branch_id: BranchId,
    rows: Vec<StorageRow>,
}

impl BranchSnapshotInstallGroup {
    pub(crate) fn new(branch_id: BranchId, rows: Vec<StorageRow>) -> Self {
        Self { branch_id, rows }
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) fn rows(&self) -> &[StorageRow] {
        &self.rows
    }
}

impl fmt::Debug for BranchSnapshotInstallGroup {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BranchSnapshotInstallGroup")
            .field("branch_id", &self.branch_id)
            .field("row_count", &self.rows.len())
            .finish()
    }
}

#[derive(Clone, Eq, PartialEq)]
pub(crate) struct BranchSnapshotInstallRequest {
    output_identity_seed: TableIdentity,
    missing_branch_policy: BranchSnapshotMissingBranchPolicy,
    target_state_policy: BranchSnapshotTargetStatePolicy,
    table_builder_config: TableBuilderConfig,
    max_rows_per_table: usize,
    groups: Vec<BranchSnapshotInstallGroup>,
}

impl BranchSnapshotInstallRequest {
    pub(crate) fn new(
        output_identity_seed: impl Into<String>,
        groups: Vec<BranchSnapshotInstallGroup>,
    ) -> BranchRuntimeResult<Self> {
        let output_identity_seed = TableIdentity::new(output_identity_seed.into())
            .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
        Ok(Self {
            output_identity_seed,
            missing_branch_policy: BranchSnapshotMissingBranchPolicy::Reject,
            target_state_policy: BranchSnapshotTargetStatePolicy::RequireEmpty,
            table_builder_config: TableBuilderConfig::default(),
            max_rows_per_table: SNAPSHOT_INSTALL_ROWS_PER_OUTPUT_TABLE,
            groups,
        })
    }

    pub(crate) fn from_rows(
        output_identity_seed: impl Into<String>,
        rows: Vec<StorageRow>,
    ) -> BranchRuntimeResult<Self> {
        let mut groups = Vec::<BranchSnapshotInstallGroup>::new();
        for row in rows {
            let branch_id = row.physical_key().branch_id();
            if let Some(group) = groups.iter_mut().find(|group| group.branch_id == branch_id) {
                group.rows.push(row);
            } else {
                groups.push(BranchSnapshotInstallGroup::new(branch_id, vec![row]));
            }
        }
        groups.sort_by_key(|group| *group.branch_id.as_bytes());
        for group in &mut groups {
            group.rows.sort_by_key(TableInternalKeyBytes::from_row);
        }
        Self::new(output_identity_seed, groups)
    }

    pub(crate) const fn output_identity_seed(&self) -> &TableIdentity {
        &self.output_identity_seed
    }

    pub(crate) const fn missing_branch_policy(&self) -> BranchSnapshotMissingBranchPolicy {
        self.missing_branch_policy
    }

    pub(crate) const fn target_state_policy(&self) -> BranchSnapshotTargetStatePolicy {
        self.target_state_policy
    }

    pub(crate) const fn table_builder_config(&self) -> TableBuilderConfig {
        self.table_builder_config
    }

    pub(crate) const fn max_rows_per_table(&self) -> usize {
        self.max_rows_per_table
    }

    pub(crate) fn groups(&self) -> &[BranchSnapshotInstallGroup] {
        &self.groups
    }

    pub(crate) fn with_missing_branch_policy(
        mut self,
        missing_branch_policy: BranchSnapshotMissingBranchPolicy,
    ) -> Self {
        self.missing_branch_policy = missing_branch_policy;
        self
    }

    pub(crate) fn with_table_builder_config(
        mut self,
        table_builder_config: TableBuilderConfig,
    ) -> Self {
        self.table_builder_config = table_builder_config;
        self
    }

    pub(crate) fn with_max_rows_per_table(
        mut self,
        max_rows_per_table: usize,
    ) -> BranchRuntimeResult<Self> {
        if max_rows_per_table == 0 {
            return Err(BranchRuntimeError::InvalidSnapshotInstall {
                reason: "max rows per output table must be nonzero",
            });
        }
        self.max_rows_per_table = max_rows_per_table;
        Ok(self)
    }
}

impl fmt::Debug for BranchSnapshotInstallRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BranchSnapshotInstallRequest")
            .field("output_identity_seed", &self.output_identity_seed)
            .field("missing_branch_policy", &self.missing_branch_policy)
            .field("target_state_policy", &self.target_state_policy)
            .field("table_builder_config", &self.table_builder_config)
            .field("max_rows_per_table", &self.max_rows_per_table)
            .field("groups", &self.groups)
            .finish()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BranchSnapshotInstallRecovery {
    EmptyPlanNoop,
    Installed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchSnapshotInstallBranchOutcome {
    branch_id: BranchId,
    branch_created: bool,
    rows_installed: u64,
    tables_created: usize,
    max_commit_version: Option<CommitVersion>,
    timestamp_min: Option<Timestamp>,
    timestamp_max: Option<Timestamp>,
    table_identities: Vec<TableIdentity>,
}

impl BranchSnapshotInstallBranchOutcome {
    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn branch_created(&self) -> bool {
        self.branch_created
    }

    pub(crate) const fn rows_installed(&self) -> u64 {
        self.rows_installed
    }

    pub(crate) const fn tables_created(&self) -> usize {
        self.tables_created
    }

    pub(crate) const fn max_commit_version(&self) -> Option<CommitVersion> {
        self.max_commit_version
    }

    pub(crate) const fn timestamp_min(&self) -> Option<Timestamp> {
        self.timestamp_min
    }

    pub(crate) const fn timestamp_max(&self) -> Option<Timestamp> {
        self.timestamp_max
    }

    pub(crate) fn table_identities(&self) -> &[TableIdentity] {
        &self.table_identities
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchSnapshotInstallOutcome {
    recovery: BranchSnapshotInstallRecovery,
    branch_outcomes: Vec<BranchSnapshotInstallBranchOutcome>,
    rows_installed: u64,
    tables_created: usize,
    branches_created: usize,
    branches_replaced: usize,
}

impl BranchSnapshotInstallOutcome {
    fn empty() -> Self {
        Self {
            recovery: BranchSnapshotInstallRecovery::EmptyPlanNoop,
            branch_outcomes: Vec::new(),
            rows_installed: 0,
            tables_created: 0,
            branches_created: 0,
            branches_replaced: 0,
        }
    }

    fn installed(branch_outcomes: Vec<BranchSnapshotInstallBranchOutcome>) -> Self {
        let rows_installed = branch_outcomes
            .iter()
            .map(BranchSnapshotInstallBranchOutcome::rows_installed)
            .sum();
        let tables_created = branch_outcomes
            .iter()
            .map(BranchSnapshotInstallBranchOutcome::tables_created)
            .sum();
        let branches_created = branch_outcomes
            .iter()
            .filter(|outcome| outcome.branch_created())
            .count();
        let branches_replaced = branch_outcomes.len().saturating_sub(branches_created);
        Self {
            recovery: BranchSnapshotInstallRecovery::Installed,
            branch_outcomes,
            rows_installed,
            tables_created,
            branches_created,
            branches_replaced,
        }
    }

    pub(crate) const fn recovery(&self) -> BranchSnapshotInstallRecovery {
        self.recovery
    }

    pub(crate) fn branch_outcomes(&self) -> &[BranchSnapshotInstallBranchOutcome] {
        &self.branch_outcomes
    }

    pub(crate) const fn rows_installed(&self) -> u64 {
        self.rows_installed
    }

    pub(crate) const fn tables_created(&self) -> usize {
        self.tables_created
    }

    pub(crate) const fn branches_created(&self) -> usize {
        self.branches_created
    }

    pub(crate) const fn branches_replaced(&self) -> usize {
        self.branches_replaced
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchLocalState {
    branch_id: BranchId,
    config: BranchRuntimeConfig,
    active: MutableTable,
    frozen: Vec<FrozenTable>,
    owned_levels: Vec<Vec<BranchOwnedTable>>,
    inherited_layers: Vec<BranchInheritedLayer>,
    max_commit_version: Option<CommitVersion>,
    timestamp_min: Option<Timestamp>,
    timestamp_max: Option<Timestamp>,
    timestamp_coverage: BranchTimestampCoverage,
    put_rows: u64,
    tombstone_rows: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchTableManifestRecoveryRequest {
    branch_id: BranchId,
    owned_levels: Vec<Vec<BranchOwnedTable>>,
    inherited_layers: Vec<BranchInheritedLayer>,
    timestamp_coverage: BranchTimestampCoverage,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchTableManifestRecoveryOutcome {
    branch_id: BranchId,
    owned_table_count: usize,
    inherited_layer_count: usize,
    inherited_table_count: usize,
    max_commit_version: Option<CommitVersion>,
    timestamp_max: Option<Timestamp>,
}

impl BranchTableManifestRecoveryRequest {
    pub(crate) fn new(
        branch_id: BranchId,
        owned_levels: Vec<Vec<BranchOwnedTable>>,
        inherited_layers: Vec<BranchInheritedLayer>,
    ) -> BranchRuntimeResult<Self> {
        let request = Self {
            branch_id,
            owned_levels,
            inherited_layers,
            timestamp_coverage: BranchTimestampCoverage::unknown(),
        };
        request.validate()?;
        Ok(request)
    }

    pub(crate) fn with_timestamp_coverage(
        mut self,
        timestamp_coverage: BranchTimestampCoverage,
    ) -> Self {
        self.timestamp_coverage = timestamp_coverage;
        self
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    fn validate(&self) -> BranchRuntimeResult<()> {
        validate_manifest_recovery_inherited_layers(self.branch_id, &self.inherited_layers)?;
        validate_manifest_recovery_table_identities(&self.owned_levels, &self.inherited_layers)
    }

    fn validate_against_config(&self, config: BranchRuntimeConfig) -> BranchRuntimeResult<()> {
        if self.owned_levels.len() > config.max_level_count() {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "table manifest recovery level count exceeds branch runtime configuration",
            });
        }
        if self.inherited_layers.len() > config.max_inherited_layers() {
            return Err(BranchRuntimeError::InvalidInheritedLayer {
                reason:
                    "table manifest recovery inherited layer count exceeds branch runtime configuration",
            });
        }
        Ok(())
    }
}

impl BranchTableManifestRecoveryOutcome {
    #[allow(
        dead_code,
        reason = "table-manifest recovery diagnostics are consumed by lifecycle tests"
    )]
    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    #[allow(
        dead_code,
        reason = "table-manifest recovery diagnostics are consumed by lifecycle tests"
    )]
    pub(crate) const fn owned_table_count(&self) -> usize {
        self.owned_table_count
    }

    #[allow(
        dead_code,
        reason = "table-manifest recovery diagnostics are consumed by lifecycle tests"
    )]
    pub(crate) const fn inherited_layer_count(&self) -> usize {
        self.inherited_layer_count
    }

    #[allow(
        dead_code,
        reason = "table-manifest recovery diagnostics are consumed by lifecycle tests"
    )]
    pub(crate) const fn inherited_table_count(&self) -> usize {
        self.inherited_table_count
    }

    pub(crate) const fn total_table_count(&self) -> usize {
        self.owned_table_count + self.inherited_table_count
    }

    #[allow(
        dead_code,
        reason = "table-manifest recovery diagnostics are consumed by lifecycle tests"
    )]
    pub(crate) const fn max_commit_version(&self) -> Option<CommitVersion> {
        self.max_commit_version
    }

    #[allow(
        dead_code,
        reason = "table-manifest recovery diagnostics are consumed by lifecycle tests"
    )]
    pub(crate) const fn timestamp_max(&self) -> Option<Timestamp> {
        self.timestamp_max
    }
}

#[derive(Clone)]
struct StagedSnapshotBranch {
    branch_id: BranchId,
    state: BranchLocalState,
    outcome: BranchSnapshotInstallBranchOutcome,
}

pub(crate) fn install_snapshot_rows_into_branches(
    branches: &mut Vec<BranchLocalState>,
    request: &BranchSnapshotInstallRequest,
) -> BranchRuntimeResult<BranchSnapshotInstallOutcome> {
    validate_snapshot_request(request)?;
    validate_branch_state_set(branches)?;
    if request.groups().is_empty() {
        return Ok(BranchSnapshotInstallOutcome::empty());
    }

    let staged = stage_snapshot_install_branches(branches, request)?;
    let outcome = BranchSnapshotInstallOutcome::installed(
        staged
            .iter()
            .map(|branch| branch.outcome.clone())
            .collect::<Vec<_>>(),
    );

    let mut replacement = branches.clone();
    for staged_branch in staged {
        if let Some(index) = replacement
            .iter()
            .position(|state| state.branch_id() == staged_branch.branch_id)
        {
            replacement[index] = staged_branch.state;
        } else {
            replacement.push(staged_branch.state);
        }
    }
    *branches = replacement;

    Ok(outcome)
}

impl BranchLocalState {
    pub(crate) fn new(
        branch_id: BranchId,
        config: BranchRuntimeConfig,
    ) -> BranchRuntimeResult<Self> {
        config.validate()?;
        Ok(Self {
            branch_id,
            config,
            active: MutableTable::new(),
            frozen: Vec::new(),
            owned_levels: vec![Vec::new(); config.max_level_count()],
            inherited_layers: Vec::new(),
            max_commit_version: None,
            timestamp_min: None,
            timestamp_max: None,
            timestamp_coverage: BranchTimestampCoverage::unknown(),
            put_rows: 0,
            tombstone_rows: 0,
        })
    }

    pub(crate) fn empty(branch_id: BranchId) -> Self {
        Self::new(branch_id, BranchRuntimeConfig::default())
            .expect("default branch-local state configuration is valid")
    }

    pub(crate) fn checkpoint_rows(
        &self,
        watermark: CommitVersion,
    ) -> BranchRuntimeResult<Vec<StorageRow>> {
        if watermark == CommitVersion::ZERO {
            return Ok(Vec::new());
        }
        if !self.inherited_layers.is_empty() {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "checkpoint requires inherited layers to be materialized first",
            });
        }

        let mut rows = Vec::new();
        for row in self.active.iter() {
            push_checkpoint_row(self.branch_id, watermark, row.row(), &mut rows)?;
        }
        for table in &self.frozen {
            for row in table.iter() {
                push_checkpoint_row(self.branch_id, watermark, row.row(), &mut rows)?;
            }
        }
        for table in self.owned_levels.iter().flatten() {
            for row in table.rows() {
                push_checkpoint_row(self.branch_id, watermark, row.row(), &mut rows)?;
            }
        }

        rows.sort_by_key(TableInternalKeyBytes::from_row);
        validate_checkpoint_rows(&rows)?;
        Ok(rows)
    }

    pub(crate) fn fork_snapshot_rows(
        &self,
        watermark: CommitVersion,
        target_branch_id: BranchId,
    ) -> BranchRuntimeResult<Vec<StorageRow>> {
        if watermark == CommitVersion::ZERO {
            return Ok(Vec::new());
        }

        let mut rows_by_key = BTreeMap::<TableInternalKeyBytes, StorageRow>::new();
        for row in self.active.iter() {
            insert_own_fork_snapshot_row(
                &mut rows_by_key,
                self.branch_id,
                target_branch_id,
                watermark,
                row.row(),
            )?;
        }
        for table in &self.frozen {
            for row in table.iter() {
                insert_own_fork_snapshot_row(
                    &mut rows_by_key,
                    self.branch_id,
                    target_branch_id,
                    watermark,
                    row.row(),
                )?;
            }
        }
        for table in self.owned_levels.iter().flatten() {
            for row in table.rows() {
                insert_own_fork_snapshot_row(
                    &mut rows_by_key,
                    self.branch_id,
                    target_branch_id,
                    watermark,
                    row.row(),
                )?;
            }
        }
        for layer in &self.inherited_layers {
            match layer.status() {
                InheritedLayerStatus::Active | InheritedLayerStatus::Materializing => {}
                InheritedLayerStatus::Materialized => continue,
                InheritedLayerStatus::Unavailable => {
                    return Err(BranchRuntimeError::InvalidInheritedLayer {
                        reason: "unavailable inherited layers cannot be snapshotted for fork",
                    });
                }
            }
            let inherited_watermark = watermark.min(layer.fork_version());
            for table in layer.owned_levels().iter().flatten() {
                for row in table.rows() {
                    insert_lower_precedence_fork_snapshot_row(
                        &mut rows_by_key,
                        layer.source_branch_id(),
                        target_branch_id,
                        inherited_watermark,
                        row.row(),
                    )?;
                }
            }
        }

        let rows = rows_by_key.into_values().collect::<Vec<_>>();
        validate_checkpoint_rows(&rows)?;
        Ok(rows)
    }

    pub(crate) fn reachability_snapshot(&self) -> BranchRuntimeResult<BranchReachabilitySnapshot> {
        let mut table_refs = Vec::new();
        for tables in &self.owned_levels {
            for (table_index, table) in tables.iter().enumerate() {
                let table_ref = if let Some(materialization_source) = table.materialization_source()
                {
                    BranchTableRef::replacement(
                        self.branch_id,
                        materialization_source.source_branch_id(),
                        materialization_source.fork_version(),
                        table.level(),
                        table_index,
                        table.descriptor().identity().clone(),
                    )?
                } else {
                    BranchTableRef::owned(
                        self.branch_id,
                        table.level(),
                        table_index,
                        table.descriptor().identity().clone(),
                    )?
                };
                table_refs.push(table_ref);
            }
        }
        for (layer_index, layer) in self.inherited_layers.iter().enumerate() {
            let reference_kind = match layer.status() {
                InheritedLayerStatus::Active => InheritedReferenceKind::Active,
                InheritedLayerStatus::Materializing => InheritedReferenceKind::Materializing,
                InheritedLayerStatus::Materialized => continue,
                InheritedLayerStatus::Unavailable => {
                    return Err(BranchRuntimeError::InvalidInheritedLayer {
                        reason: "unavailable inherited layers cannot emit reachability",
                    });
                }
            };
            for tables in layer.owned_levels() {
                for (table_index, table) in tables.iter().enumerate() {
                    let table_ref = match reference_kind {
                        InheritedReferenceKind::Active => BranchTableRef::inherited(
                            self.branch_id,
                            layer.source_branch_id(),
                            layer.fork_version(),
                            layer_index,
                            table.level(),
                            table_index,
                            table.descriptor().identity().clone(),
                        )?,
                        InheritedReferenceKind::Materializing => {
                            BranchTableRef::materializing_source(
                                self.branch_id,
                                layer.source_branch_id(),
                                layer.fork_version(),
                                layer_index,
                                table.level(),
                                table_index,
                                table.descriptor().identity().clone(),
                            )?
                        }
                    };
                    table_refs.push(table_ref);
                }
            }
        }
        BranchReachabilitySnapshot::new(self.branch_id, table_refs)
    }

    pub(crate) fn install_l0_table(
        &mut self,
        table: BranchOwnedTable,
    ) -> BranchRuntimeResult<BranchImmutableInstallOutcome> {
        self.install_owned_table_at_level(BranchLevel::ZERO, table)
    }

    pub(crate) fn install_owned_table_at_level(
        &mut self,
        level: BranchLevel,
        table: BranchOwnedTable,
    ) -> BranchRuntimeResult<BranchImmutableInstallOutcome> {
        let level_index = self.validate_install(level, &table, None)?;
        let table_index = if level == BranchLevel::ZERO {
            self.owned_levels[level_index].insert(0, table);
            0
        } else {
            insert_sorted_by_range(&mut self.owned_levels[level_index], table)?
        };
        self.refresh_observed_row_facts();
        Ok(self.install_outcome(level, table_index, None))
    }

    pub(crate) fn replace_frozen_with_l0_table(
        &mut self,
        frozen_index: usize,
        table: BranchOwnedTable,
    ) -> BranchRuntimeResult<BranchImmutableInstallOutcome> {
        self.replace_frozen_with_level_zero_table(frozen_index, table)
    }

    pub(crate) fn replace_frozen_with_level_zero_table(
        &mut self,
        frozen_index: usize,
        table: BranchOwnedTable,
    ) -> BranchRuntimeResult<BranchImmutableInstallOutcome> {
        if frozen_index >= self.frozen.len() {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "frozen replacement index must exist",
            });
        }
        let level_index = self.validate_install(BranchLevel::ZERO, &table, Some(frozen_index))?;
        require_rows_match_frozen(&table, &self.frozen[frozen_index])?;

        self.owned_levels[level_index].insert(0, table);
        self.frozen.remove(frozen_index);
        self.refresh_observed_row_facts();
        Ok(self.install_outcome(BranchLevel::ZERO, 0, Some(frozen_index)))
    }

    pub(crate) fn install_table_manifest_recovery(
        &mut self,
        request: BranchTableManifestRecoveryRequest,
    ) -> BranchRuntimeResult<BranchTableManifestRecoveryOutcome> {
        if request.branch_id() != self.branch_id {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "table manifest recovery branch id must match branch state",
            });
        }
        if !self.is_empty() {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "table manifest recovery requires an empty branch table state",
            });
        }
        request.validate_against_config(self.config)?;

        let mut staged = Self::new(self.branch_id, self.config)?;
        staged.owned_levels = request.owned_levels;
        staged
            .owned_levels
            .resize_with(self.config.max_level_count(), Vec::new);
        staged.inherited_layers = request.inherited_layers;
        staged.timestamp_coverage = request.timestamp_coverage;
        validate_compaction_levels(&staged.owned_levels)?;
        validate_manifest_recovery_inherited_layers(staged.branch_id, &staged.inherited_layers)?;
        validate_manifest_recovery_table_identities(
            &staged.owned_levels,
            &staged.inherited_layers,
        )?;
        staged.refresh_observed_row_facts();
        staged.reachability_snapshot()?;
        staged.capture_read_view()?;

        let outcome = BranchTableManifestRecoveryOutcome {
            branch_id: staged.branch_id,
            owned_table_count: staged.owned_table_count(),
            inherited_layer_count: staged.inherited_layer_count(),
            inherited_table_count: staged.inherited_table_count(),
            max_commit_version: staged.max_commit_version,
            timestamp_max: staged.timestamp_max,
        };
        *self = staged;
        Ok(outcome)
    }

    pub(crate) fn plan_branch_compaction(
        &self,
        request: &BranchCompactionRequest,
    ) -> BranchRuntimeResult<BranchCompactionPlan> {
        self.validate_compaction_request(request)?;
        match request.kind() {
            BranchCompactionKind::CompactL0 => self.plan_l0_compaction(request.kind()),
            BranchCompactionKind::CompactL0ToLevelOne => {
                self.plan_l0_to_level_one_compaction(request.kind())
            }
            BranchCompactionKind::CompactLevel { level, table_index } => {
                self.plan_nonzero_level_compaction(request.kind(), level, table_index)
            }
        }
    }

    pub(crate) fn compact_branch_owned_tables(
        &mut self,
        request: &BranchCompactionRequest,
    ) -> BranchRuntimeResult<BranchCompactionOutcome> {
        let plan = self.plan_branch_compaction(request)?;
        self.install_branch_compaction_plan(request, &plan)
    }

    pub(crate) fn prepare_branch_compaction_plan(
        &self,
        request: &BranchCompactionRequest,
        plan: &BranchCompactionPlan,
    ) -> BranchRuntimeResult<Option<BranchCompactionPreparedOutput>> {
        self.validate_compaction_request(request)?;
        if plan.branch_id() != self.branch_id || plan.kind() != request.kind() {
            return Err(BranchRuntimeError::InvalidCompaction {
                reason: BranchCompactionInvalidity::Generic(
                    "compaction plan must match request branch and kind",
                ),
            });
        }
        let Some(candidate) = plan.candidate() else {
            return Ok(None);
        };
        self.require_candidate_current(candidate)?;
        let sources = self.compaction_sources(candidate)?;
        let compactor = TableCompactor::new(
            request.table_compaction_config(),
            request.table_builder_config(),
        )
        .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
        let output = match request.retention_policy() {
            BranchCompactionRetentionPolicy::KeepAll => {
                let mut policy = KeepAllTableCompactionPolicy;
                compactor
                    .compact(request.output_identity_seed(), &sources, &mut policy)
                    .map_err(|source| BranchRuntimeError::TableRuntime { source })?
            }
            BranchCompactionRetentionPolicy::DropOlderVersions
            | BranchCompactionRetentionPolicy::DropTombstones
            | BranchCompactionRetentionPolicy::DropExpired => {
                let proof =
                    request
                        .pruning_proof()
                        .ok_or(BranchRuntimeError::InvalidCompaction {
                            reason: BranchCompactionInvalidity::ProofMissing,
                        })?;
                proof.validate_for_branch(self, candidate, request.retention_policy())?;
                let mut policy =
                    BranchCompactionPruningPolicy::new(request.retention_policy(), proof);
                compactor
                    .compact(request.output_identity_seed(), &sources, &mut policy)
                    .map_err(|source| BranchRuntimeError::TableRuntime { source })?
            }
        };
        let (artifacts, report) = output.into_parts();
        Ok(Some(BranchCompactionPreparedOutput {
            branch_id: self.branch_id,
            output_level: candidate.output_level(),
            artifacts,
            report,
            materialization_source: Self::compaction_output_materialization_source(candidate),
        }))
    }

    pub(crate) fn install_branch_compaction_plan(
        &mut self,
        request: &BranchCompactionRequest,
        plan: &BranchCompactionPlan,
    ) -> BranchRuntimeResult<BranchCompactionOutcome> {
        self.validate_compaction_request(request)?;
        if plan.branch_id() != self.branch_id || plan.kind() != request.kind() {
            return Err(BranchRuntimeError::InvalidCompaction {
                reason: BranchCompactionInvalidity::Generic(
                    "compaction plan must match request branch and kind",
                ),
            });
        }
        let Some(candidate) = plan.candidate() else {
            return Ok(BranchCompactionOutcome::no_candidate(
                self.branch_id,
                plan.noop_reason()
                    .expect("no-candidate compaction plan records reason"),
                self.owned_table_count(),
            ));
        };
        let prepared = self.prepare_branch_compaction_plan(request, plan)?.ok_or(
            BranchRuntimeError::InvalidCompaction {
                reason: BranchCompactionInvalidity::Generic(
                    "compaction candidate must produce prepared output",
                ),
            },
        )?;
        let output_tables = self.compaction_output_tables(
            candidate.output_level(),
            prepared.artifacts,
            prepared.materialization_source,
        )?;
        self.install_branch_compaction_prepared_plan(request, plan, output_tables, prepared.report)
    }

    pub(crate) fn install_branch_compaction_prepared_plan(
        &mut self,
        request: &BranchCompactionRequest,
        plan: &BranchCompactionPlan,
        output_tables: Vec<BranchOwnedTable>,
        report: TableCompactionReport,
    ) -> BranchRuntimeResult<BranchCompactionOutcome> {
        self.validate_compaction_request(request)?;
        if plan.branch_id() != self.branch_id || plan.kind() != request.kind() {
            return Err(BranchRuntimeError::InvalidCompaction {
                reason: BranchCompactionInvalidity::Generic(
                    "compaction plan must match request branch and kind",
                ),
            });
        }
        let Some(candidate) = plan.candidate() else {
            return Ok(BranchCompactionOutcome::no_candidate(
                self.branch_id,
                plan.noop_reason()
                    .expect("no-candidate compaction plan records reason"),
                self.owned_table_count(),
            ));
        };
        self.require_candidate_current(candidate)?;
        if output_tables
            .iter()
            .any(|table| table.descriptor().level() != candidate.output_level())
        {
            return Err(BranchRuntimeError::InvalidCompaction {
                reason: BranchCompactionInvalidity::Generic(
                    "prepared compaction output level must match candidate",
                ),
            });
        }
        let output_identities = output_tables
            .iter()
            .map(|table| table.descriptor().identity().clone())
            .collect::<Vec<_>>();
        validate_compaction_output_identities(
            &output_identities,
            &self.owned_levels,
            &self.inherited_layers,
        )?;
        self.require_candidate_current(candidate)?;

        let mut replacement_levels = self.owned_levels.clone();
        remove_compacted_tables(&mut replacement_levels, candidate)?;
        insert_compaction_outputs(&mut replacement_levels, candidate, output_tables)?;
        validate_compaction_levels(&replacement_levels)?;

        self.owned_levels = replacement_levels;
        self.refresh_observed_row_facts();
        if report.dropped_rows() != 0 {
            if let Some(floor) = request
                .pruning_proof()
                .and_then(BranchCompactionPruningProof::retained_timestamp_floor)
            {
                self.timestamp_coverage = BranchTimestampCoverage::complete_since(floor);
            }
        }

        let output_refs =
            self.compaction_output_refs(candidate.output_level(), &output_identities)?;
        Ok(BranchCompactionOutcome::installed(
            self.branch_id,
            candidate.clone(),
            output_refs,
            candidate.removed_refs(),
            report,
            self.owned_table_count(),
        ))
    }

    fn require_absent_internal_key(&self, key: &TableInternalKeyBytes) -> BranchRuntimeResult<()> {
        self.require_absent_internal_key_except_frozen(key, None)
    }

    fn validate_compaction_request(
        &self,
        request: &BranchCompactionRequest,
    ) -> BranchRuntimeResult<()> {
        if request.branch_id() != self.branch_id {
            return Err(BranchRuntimeError::InvalidCompaction {
                reason: BranchCompactionInvalidity::Generic(
                    "compaction request branch id must match branch state",
                ),
            });
        }
        if request.retention_policy() == BranchCompactionRetentionPolicy::KeepAll
            && request.pruning_proof().is_some()
        {
            return Err(BranchRuntimeError::InvalidCompaction {
                reason: BranchCompactionInvalidity::Generic(
                    "branch compaction keep-all request must not carry a pruning proof",
                ),
            });
        }
        if request.retention_policy() != BranchCompactionRetentionPolicy::KeepAll
            && request.pruning_proof().is_none()
        {
            return Err(BranchRuntimeError::InvalidCompaction {
                reason: BranchCompactionInvalidity::ProofMissing,
            });
        }
        request
            .table_compaction_config()
            .validate()
            .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
        request
            .table_builder_config()
            .validate()
            .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
        Ok(())
    }

    fn plan_l0_compaction(
        &self,
        kind: BranchCompactionKind,
    ) -> BranchRuntimeResult<BranchCompactionPlan> {
        let level_index = 0;
        let input_count = self.owned_levels[level_index].len();
        if input_count == 0 {
            return Ok(BranchCompactionPlan::no_candidate(
                self.branch_id,
                kind,
                BranchCompactionNoopReason::EmptyInputLevel,
            ));
        }
        if input_count < 2 {
            return Ok(BranchCompactionPlan::no_candidate(
                self.branch_id,
                kind,
                BranchCompactionNoopReason::NotEnoughInputTables,
            ));
        }
        let input_refs = self.table_refs_at_level(level_index, 0..input_count)?;
        let input_row_count = self.table_ref_row_count(&input_refs)?;
        let candidate = BranchCompactionCandidate::new(
            self.branch_id,
            input_refs,
            Vec::new(),
            BranchLevel::ZERO,
            self.is_bottommost_output_level(0),
            input_row_count,
        );
        Ok(BranchCompactionPlan::with_candidate(
            self.branch_id,
            kind,
            candidate,
        ))
    }

    fn plan_l0_to_level_one_compaction(
        &self,
        kind: BranchCompactionKind,
    ) -> BranchRuntimeResult<BranchCompactionPlan> {
        if self.owned_levels.len() < 2 {
            return Ok(BranchCompactionPlan::no_candidate(
                self.branch_id,
                kind,
                BranchCompactionNoopReason::LastLevel,
            ));
        }
        let input_count = self.owned_levels[0].len();
        if input_count == 0 {
            return Ok(BranchCompactionPlan::no_candidate(
                self.branch_id,
                kind,
                BranchCompactionNoopReason::EmptyInputLevel,
            ));
        }
        let input_refs = self.table_refs_at_level(0, 0..input_count)?;
        let overlap_refs = self.overlapping_refs_for_input_range(&input_refs, 1)?;
        if input_refs.len().saturating_add(overlap_refs.len()) < 2 {
            return Ok(BranchCompactionPlan::no_candidate(
                self.branch_id,
                kind,
                BranchCompactionNoopReason::NotEnoughInputTables,
            ));
        }
        let input_row_count = self
            .table_ref_row_count(&input_refs)?
            .saturating_add(self.table_ref_row_count(&overlap_refs)?);
        let candidate = BranchCompactionCandidate::new(
            self.branch_id,
            input_refs,
            overlap_refs,
            BranchLevel::new(1),
            self.is_bottommost_output_level(1),
            input_row_count,
        );
        Ok(BranchCompactionPlan::with_candidate(
            self.branch_id,
            kind,
            candidate,
        ))
    }

    fn plan_nonzero_level_compaction(
        &self,
        kind: BranchCompactionKind,
        level: BranchLevel,
        table_index: usize,
    ) -> BranchRuntimeResult<BranchCompactionPlan> {
        let level_index = usize::from(level.raw());
        if level_index == 0 {
            return Err(BranchRuntimeError::InvalidCompaction {
                reason: BranchCompactionInvalidity::Generic(
                    "level-zero compaction requests must use CompactL0",
                ),
            });
        }
        if level_index >= self.owned_levels.len() {
            return Err(BranchRuntimeError::InvalidCompaction {
                reason: BranchCompactionInvalidity::Generic(
                    "compaction level is outside configured level count",
                ),
            });
        }
        if level_index + 1 >= self.owned_levels.len() {
            return Ok(BranchCompactionPlan::no_candidate(
                self.branch_id,
                kind,
                BranchCompactionNoopReason::LastLevel,
            ));
        }
        if self.owned_levels[level_index].is_empty() {
            return Ok(BranchCompactionPlan::no_candidate(
                self.branch_id,
                kind,
                BranchCompactionNoopReason::EmptyInputLevel,
            ));
        }
        if table_index >= self.owned_levels[level_index].len() {
            return Err(BranchRuntimeError::InvalidCompaction {
                reason: BranchCompactionInvalidity::Generic(
                    "compaction table index is outside requested level",
                ),
            });
        }
        let input_refs = self.table_refs_at_level(level_index, table_index..table_index + 1)?;
        let overlap_refs = self.overlapping_refs_for_input_range(&input_refs, level_index + 1)?;
        if input_refs.len().saturating_add(overlap_refs.len()) < 2 {
            return Ok(BranchCompactionPlan::no_candidate(
                self.branch_id,
                kind,
                BranchCompactionNoopReason::NotEnoughInputTables,
            ));
        }
        let input_row_count = self
            .table_ref_row_count(&input_refs)?
            .saturating_add(self.table_ref_row_count(&overlap_refs)?);
        let output_level = BranchLevel::new(u8::try_from(level_index + 1).map_err(|_| {
            BranchRuntimeError::InvalidCompaction {
                reason: BranchCompactionInvalidity::Generic(
                    "compaction output level must fit in BranchLevel",
                ),
            }
        })?);
        let candidate = BranchCompactionCandidate::new(
            self.branch_id,
            input_refs,
            overlap_refs,
            output_level,
            self.is_bottommost_output_level(level_index + 1),
            input_row_count,
        );
        Ok(BranchCompactionPlan::with_candidate(
            self.branch_id,
            kind,
            candidate,
        ))
    }

    fn table_refs_at_level(
        &self,
        level_index: usize,
        range: std::ops::Range<usize>,
    ) -> BranchRuntimeResult<Vec<BranchTableRef>> {
        let level = BranchLevel::new(u8::try_from(level_index).map_err(|_| {
            BranchRuntimeError::InvalidCompaction {
                reason: BranchCompactionInvalidity::Generic(
                    "compaction table level must fit in BranchLevel",
                ),
            }
        })?);
        range
            .map(|table_index| {
                let table = self.owned_levels[level_index].get(table_index).ok_or(
                    BranchRuntimeError::InvalidCompaction {
                        reason: BranchCompactionInvalidity::Generic(
                            "compaction table index must exist",
                        ),
                    },
                )?;
                branch_table_ref_for_owned(self.branch_id, level, table_index, table)
            })
            .collect()
    }

    fn overlapping_refs_for_input_range(
        &self,
        input_refs: &[BranchTableRef],
        target_level_index: usize,
    ) -> BranchRuntimeResult<Vec<BranchTableRef>> {
        let target_level = BranchLevel::new(u8::try_from(target_level_index).map_err(|_| {
            BranchRuntimeError::InvalidCompaction {
                reason: BranchCompactionInvalidity::Generic(
                    "compaction target level must fit in BranchLevel",
                ),
            }
        })?);
        let mut refs = Vec::new();
        for (table_index, table) in self.owned_levels[target_level_index].iter().enumerate() {
            if input_refs.iter().any(|input_ref| {
                self.table_for_ref(input_ref)
                    .is_some_and(|input_table| table_physical_ranges_overlap(input_table, table))
            }) {
                refs.push(branch_table_ref_for_owned(
                    self.branch_id,
                    target_level,
                    table_index,
                    table,
                )?);
            }
        }
        Ok(refs)
    }

    fn table_ref_row_count(&self, refs: &[BranchTableRef]) -> BranchRuntimeResult<u64> {
        let mut count = 0u64;
        for table_ref in refs {
            let table =
                self.table_for_ref(table_ref)
                    .ok_or(BranchRuntimeError::InvalidCompaction {
                        reason: BranchCompactionInvalidity::Generic(
                            "compaction table ref must exist",
                        ),
                    })?;
            count = count.saturating_add(u64::try_from(table.rows().len()).map_err(|_| {
                BranchRuntimeError::InvalidCompaction {
                    reason: BranchCompactionInvalidity::Generic(
                        "compaction input row count must fit in u64",
                    ),
                }
            })?);
        }
        Ok(count)
    }

    fn table_for_ref(&self, table_ref: &BranchTableRef) -> Option<&BranchOwnedTable> {
        let level_index = usize::from(table_ref.level().raw());
        self.owned_levels
            .get(level_index)?
            .get(table_ref.table_index())
            .filter(|table| {
                table.descriptor().identity() == table_ref.table_identity()
                    && table.branch_id() == self.branch_id
            })
    }

    fn is_bottommost_output_level(&self, output_level_index: usize) -> bool {
        self.owned_levels
            .iter()
            .enumerate()
            .skip(output_level_index + 1)
            .all(|(_, tables)| tables.is_empty())
    }

    fn require_candidate_current(
        &self,
        candidate: &BranchCompactionCandidate,
    ) -> BranchRuntimeResult<()> {
        if candidate.branch_id() != self.branch_id {
            return Err(BranchRuntimeError::InvalidCompaction {
                reason: BranchCompactionInvalidity::Generic(
                    "compaction candidate branch must match branch state",
                ),
            });
        }
        for table_ref in candidate
            .input_refs()
            .iter()
            .chain(candidate.overlap_refs().iter())
        {
            let Some(table) = self.table_for_ref(table_ref) else {
                return Err(BranchRuntimeError::InvalidCompaction {
                    reason: BranchCompactionInvalidity::Generic("compaction candidate is stale"),
                });
            };
            match table_ref.reference_kind() {
                BranchTableReferenceKind::Owned | BranchTableReferenceKind::Replacement { .. } => {}
                BranchTableReferenceKind::Inherited { .. }
                | BranchTableReferenceKind::MaterializingSource { .. } => {
                    return Err(BranchRuntimeError::InvalidCompaction {
                        reason: BranchCompactionInvalidity::Generic(
                            "compaction candidate must reference branch-owned tables",
                        ),
                    });
                }
            }
            if table.level() != table_ref.level() {
                return Err(BranchRuntimeError::InvalidCompaction {
                    reason: BranchCompactionInvalidity::Generic(
                        "compaction candidate table level is stale",
                    ),
                });
            }
        }
        Ok(())
    }

    fn compaction_sources(
        &self,
        candidate: &BranchCompactionCandidate,
    ) -> BranchRuntimeResult<Vec<TableCompactionSource>> {
        candidate
            .input_refs()
            .iter()
            .chain(candidate.overlap_refs().iter())
            .enumerate()
            .map(|(source_index, table_ref)| {
                let table =
                    self.table_for_ref(table_ref)
                        .ok_or(BranchRuntimeError::InvalidCompaction {
                            reason: BranchCompactionInvalidity::Generic(
                                "compaction candidate source table must exist",
                            ),
                        })?;
                let source_id = TableCompactionSourceId::new(format!(
                    "branch-{}-level-{}-table-{}-source-{source_index}",
                    self.branch_id,
                    table_ref.level().raw(),
                    table_ref.table_index(),
                ))
                .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
                TableCompactionSource::from_rows(source_id, table.rows().to_vec())
                    .map_err(|source| BranchRuntimeError::TableRuntime { source })
            })
            .collect()
    }

    fn compaction_output_tables(
        &self,
        output_level: BranchLevel,
        artifacts: Vec<crate::table::BuiltTableArtifact>,
        materialization_source: Option<BranchMaterializationSource>,
    ) -> BranchRuntimeResult<Vec<BranchOwnedTable>> {
        let mut tables = Vec::with_capacity(artifacts.len());
        for artifact in artifacts {
            let (bytes, facts) = artifact.into_parts();
            let identity = facts.identity().clone();
            let reader = ImmutableTableReader::open_bytes(
                identity.clone(),
                bytes,
                TableReaderConfig::default(),
            )
            .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
            let descriptor =
                BranchTableDescriptor::new(identity, reader.facts().clone(), output_level)?;
            let table = if let Some(source) = materialization_source {
                BranchOwnedTable::new_materialization_replacement(
                    self.branch_id,
                    descriptor,
                    reader,
                    source,
                )?
            } else {
                BranchOwnedTable::new(self.branch_id, descriptor, reader)?
            };
            tables.push(table);
        }
        Ok(tables)
    }

    fn compaction_output_materialization_source(
        candidate: &BranchCompactionCandidate,
    ) -> Option<BranchMaterializationSource> {
        let mut source = None::<BranchMaterializationSource>;
        for table_ref in candidate
            .input_refs()
            .iter()
            .chain(candidate.overlap_refs().iter())
        {
            let BranchTableReferenceKind::Replacement {
                source_branch_id,
                fork_version,
            } = table_ref.reference_kind()
            else {
                return None;
            };
            let table_source = BranchMaterializationSource::new(source_branch_id, fork_version);
            if source.is_some_and(|existing| existing != table_source) {
                return None;
            }
            source = Some(table_source);
        }
        source
    }

    fn compaction_output_refs(
        &self,
        output_level: BranchLevel,
        output_identities: &[TableIdentity],
    ) -> BranchRuntimeResult<Vec<BranchTableRef>> {
        let level_index = usize::from(output_level.raw());
        let level =
            self.owned_levels
                .get(level_index)
                .ok_or(BranchRuntimeError::InvalidCompaction {
                    reason: BranchCompactionInvalidity::Generic(
                        "compaction output level must exist",
                    ),
                })?;
        output_identities
            .iter()
            .map(|identity| {
                let (table_index, table) = level
                    .iter()
                    .enumerate()
                    .find(|(_, table)| table.descriptor().identity() == identity)
                    .ok_or(BranchRuntimeError::InvalidCompaction {
                        reason: BranchCompactionInvalidity::Generic(
                            "compaction output table must be installed",
                        ),
                    })?;
                branch_table_ref_for_owned(self.branch_id, output_level, table_index, table)
            })
            .collect()
    }

    fn require_absent_internal_key_except_frozen(
        &self,
        key: &TableInternalKeyBytes,
        skip_frozen_index: Option<usize>,
    ) -> BranchRuntimeResult<()> {
        if self.active.get(key).is_some()
            || self
                .frozen
                .iter()
                .enumerate()
                .any(|(index, table)| skip_frozen_index != Some(index) && table.get(key).is_some())
            || self
                .owned_levels
                .iter()
                .flatten()
                .any(|table| table.reader().get_exact(key).is_some())
        {
            return Err(BranchRuntimeError::TableRuntime {
                source: TableRuntimeError::DuplicateInternalKey {
                    key: key.as_slice().to_vec(),
                },
            });
        }
        Ok(())
    }

    fn validate_install(
        &self,
        level: BranchLevel,
        table: &BranchOwnedTable,
        skip_frozen_index: Option<usize>,
    ) -> BranchRuntimeResult<usize> {
        if table.branch_id() != self.branch_id {
            return Err(BranchRuntimeError::InvalidBranchRow {
                reason: "branch-owned table branch id must match branch state",
            });
        }
        if table.level() != level {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "branch-owned table level must match install level",
            });
        }
        let level_index = usize::from(level.raw());
        if level_index >= self.owned_levels.len() {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "branch-owned table level is outside configured level count",
            });
        }
        if branch_reachable_table_identity_exists(
            table.descriptor().identity(),
            &self.owned_levels,
            &self.inherited_layers,
        ) {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "branch-owned table identity must not collide with reachable table",
            });
        }
        for row in table.rows() {
            self.require_absent_internal_key_except_frozen(row.key(), skip_frozen_index)?;
        }
        if level != BranchLevel::ZERO {
            self.require_non_overlapping_level(level_index, table)?;
        }
        Ok(level_index)
    }

    fn require_non_overlapping_level(
        &self,
        level_index: usize,
        table: &BranchOwnedTable,
    ) -> BranchRuntimeResult<()> {
        if self.owned_levels[level_index]
            .iter()
            .any(|existing| table_physical_ranges_overlap(existing, table))
        {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "branch-owned nonzero level tables must not overlap by physical key range",
            });
        }
        Ok(())
    }

    fn install_outcome(
        &self,
        level: BranchLevel,
        table_index: usize,
        replaced_frozen_index: Option<usize>,
    ) -> BranchImmutableInstallOutcome {
        let level_index = usize::from(level.raw());
        BranchImmutableInstallOutcome {
            branch_id: self.branch_id,
            level,
            table_index,
            level_table_count: self.owned_levels[level_index].len(),
            owned_table_count: self.owned_table_count(),
            replaced_frozen_index,
        }
    }

    fn track_committed_row(
        &mut self,
        commit_version: CommitVersion,
        commit_timestamp: Timestamp,
        is_tombstone: bool,
    ) {
        self.max_commit_version = Some(
            self.max_commit_version
                .map_or(commit_version, |max| max.max(commit_version)),
        );
        self.timestamp_min = Some(
            self.timestamp_min
                .map_or(commit_timestamp, |min| min.min(commit_timestamp)),
        );
        self.timestamp_max = Some(
            self.timestamp_max
                .map_or(commit_timestamp, |max| max.max(commit_timestamp)),
        );
        if is_tombstone {
            self.tombstone_rows = self.tombstone_rows.saturating_add(1);
        } else {
            self.put_rows = self.put_rows.saturating_add(1);
        }
    }

    fn refresh_observed_row_facts(&mut self) {
        let observed = self.observe_rows();
        self.max_commit_version = observed.max_commit_version;
        self.timestamp_min = observed.timestamp_min;
        self.timestamp_max = observed.timestamp_max;
        self.put_rows = observed.put_rows;
        self.tombstone_rows = observed.tombstone_rows;
    }

    fn observe_own_rows(&self) -> ObservedBranchRows {
        let mut observed = ObservedBranchRows::default();
        for row in self.active.iter() {
            observed.record(row.row());
        }
        for table in &self.frozen {
            for row in table.iter() {
                observed.record(row.row());
            }
        }
        for table in self.owned_levels.iter().flatten() {
            for row in table.rows() {
                observed.record(row.row());
            }
        }
        observed
    }

    fn observe_rows(&self) -> ObservedBranchRows {
        let mut observed = self.observe_own_rows();
        for layer in &self.inherited_layers {
            observed.record_inherited_layer(layer);
        }
        observed
    }
}

fn validate_snapshot_request(request: &BranchSnapshotInstallRequest) -> BranchRuntimeResult<()> {
    request
        .table_builder_config()
        .validate()
        .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
    if request.max_rows_per_table() == 0 {
        return Err(BranchRuntimeError::InvalidSnapshotInstall {
            reason: "max rows per output table must be nonzero",
        });
    }
    let mut group_branches = BTreeSet::<[u8; BranchId::BYTE_LEN]>::new();
    let mut internal_keys = BTreeSet::<TableInternalKeyBytes>::new();
    let mut previous_branch = None::<[u8; BranchId::BYTE_LEN]>;
    for group in request.groups() {
        let branch_bytes = *group.branch_id().as_bytes();
        if previous_branch.is_some_and(|previous| branch_bytes < previous) {
            return Err(BranchRuntimeError::InvalidSnapshotInstall {
                reason: "snapshot install branch groups must be sorted by branch id",
            });
        }
        previous_branch = Some(branch_bytes);
        if !group_branches.insert(*group.branch_id().as_bytes()) {
            return Err(BranchRuntimeError::InvalidSnapshotInstall {
                reason: "snapshot install branch groups must be unique",
            });
        }
        if group.rows().is_empty() {
            return Err(BranchRuntimeError::InvalidSnapshotInstall {
                reason: "snapshot install branch groups must not be empty",
            });
        }
        let mut previous_key = None::<TableInternalKeyBytes>;
        for row in group.rows() {
            require_row_branch(group.branch_id(), row)?;
            let key = TableInternalKeyBytes::from_row(row);
            if let Some(previous) = &previous_key {
                if key == *previous {
                    return Err(BranchRuntimeError::TableRuntime {
                        source: TableRuntimeError::DuplicateInternalKey {
                            key: key.as_slice().to_vec(),
                        },
                    });
                }
                if key < *previous {
                    return Err(BranchRuntimeError::InvalidSnapshotInstall {
                        reason: "snapshot install rows must be strictly sorted by internal key",
                    });
                }
            }
            if !internal_keys.insert(key.clone()) {
                return Err(BranchRuntimeError::TableRuntime {
                    source: TableRuntimeError::DuplicateInternalKey {
                        key: key.as_slice().to_vec(),
                    },
                });
            }
            previous_key = Some(key);
        }
    }
    Ok(())
}

fn validate_branch_state_set(branches: &[BranchLocalState]) -> BranchRuntimeResult<()> {
    let mut seen = BTreeSet::<[u8; BranchId::BYTE_LEN]>::new();
    for branch in branches {
        if !seen.insert(*branch.branch_id().as_bytes()) {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "branch state set must not contain duplicate branches",
            });
        }
    }
    Ok(())
}

fn push_checkpoint_row(
    branch_id: BranchId,
    watermark: CommitVersion,
    row: &StorageRow,
    rows: &mut Vec<StorageRow>,
) -> BranchRuntimeResult<()> {
    require_row_branch(branch_id, row)?;
    if row.commit_version() <= watermark {
        rows.push(row.clone());
    }
    Ok(())
}

fn insert_own_fork_snapshot_row(
    rows_by_key: &mut BTreeMap<TableInternalKeyBytes, StorageRow>,
    source_branch_id: BranchId,
    target_branch_id: BranchId,
    watermark: CommitVersion,
    row: &StorageRow,
) -> BranchRuntimeResult<()> {
    if row.commit_version() > watermark {
        return Ok(());
    }
    let rewritten = rewrite_row_branch(row, source_branch_id, target_branch_id)?;
    let key = TableInternalKeyBytes::from_row(&rewritten);
    if rows_by_key.insert(key, rewritten).is_some() {
        return Err(BranchRuntimeError::InvalidBranchState {
            reason: "fork snapshot own rows must not contain duplicate internal keys",
        });
    }
    Ok(())
}

fn insert_lower_precedence_fork_snapshot_row(
    rows_by_key: &mut BTreeMap<TableInternalKeyBytes, StorageRow>,
    source_branch_id: BranchId,
    target_branch_id: BranchId,
    watermark: CommitVersion,
    row: &StorageRow,
) -> BranchRuntimeResult<()> {
    if row.commit_version() > watermark {
        return Ok(());
    }
    let rewritten = rewrite_row_branch(row, source_branch_id, target_branch_id)?;
    rows_by_key
        .entry(TableInternalKeyBytes::from_row(&rewritten))
        .or_insert(rewritten);
    Ok(())
}

fn validate_checkpoint_rows(rows: &[StorageRow]) -> BranchRuntimeResult<()> {
    let mut previous = None::<TableInternalKeyBytes>;
    for row in rows {
        let key = TableInternalKeyBytes::from_row(row);
        if previous.as_ref().is_some_and(|previous| previous >= &key) {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "checkpoint rows must be strictly sorted by internal key",
            });
        }
        previous = Some(key);
    }
    Ok(())
}

fn stage_snapshot_install_branches(
    branches: &[BranchLocalState],
    request: &BranchSnapshotInstallRequest,
) -> BranchRuntimeResult<Vec<StagedSnapshotBranch>> {
    let mut output_identities = BTreeSet::<String>::new();
    let mut staged = Vec::with_capacity(request.groups().len());
    for group in request.groups() {
        let (mut state, branch_created) =
            snapshot_target_state(branches, group.branch_id(), request)?;
        let tables = build_snapshot_l0_tables(
            group.branch_id(),
            request.output_identity_seed(),
            request.table_builder_config(),
            request.max_rows_per_table(),
            group.rows(),
        )?;
        let table_identities = tables
            .iter()
            .map(|table| table.descriptor().identity().clone())
            .collect::<Vec<_>>();
        for identity in &table_identities {
            if !output_identities.insert(identity.as_str().to_owned()) {
                return Err(BranchRuntimeError::InvalidSnapshotInstall {
                    reason: "snapshot output identities must be unique",
                });
            }
            if branches.iter().any(|branch| {
                branch_reachable_table_identity_exists(
                    identity,
                    &branch.owned_levels,
                    &branch.inherited_layers,
                )
            }) {
                return Err(BranchRuntimeError::InvalidSnapshotInstall {
                    reason:
                        "snapshot output identity must not collide with existing reachable table",
                });
            }
        }
        for table in tables.into_iter().rev() {
            state.install_l0_table(table)?;
        }
        state.reachability_snapshot()?;
        let facts = state.facts()?;
        let rows_installed = u64::try_from(group.rows().len()).map_err(|_| {
            BranchRuntimeError::InvalidSnapshotInstall {
                reason: "snapshot install row count must fit in u64",
            }
        })?;
        staged.push(StagedSnapshotBranch {
            branch_id: group.branch_id(),
            state,
            outcome: BranchSnapshotInstallBranchOutcome {
                branch_id: group.branch_id(),
                branch_created,
                rows_installed,
                tables_created: table_identities.len(),
                max_commit_version: facts.max_commit_version(),
                timestamp_min: facts.timestamp_min(),
                timestamp_max: facts.timestamp_max(),
                table_identities,
            },
        });
    }
    Ok(staged)
}

fn snapshot_target_state(
    branches: &[BranchLocalState],
    branch_id: BranchId,
    request: &BranchSnapshotInstallRequest,
) -> BranchRuntimeResult<(BranchLocalState, bool)> {
    match branches.iter().find(|state| state.branch_id() == branch_id) {
        Some(existing) => {
            if request.target_state_policy() == BranchSnapshotTargetStatePolicy::RequireEmpty
                && !existing.is_empty()
            {
                return Err(BranchRuntimeError::InvalidSnapshotInstall {
                    reason: "snapshot install target branch must be empty",
                });
            }
            Ok((BranchLocalState::new(branch_id, existing.config())?, false))
        }
        None => match request.missing_branch_policy() {
            BranchSnapshotMissingBranchPolicy::Reject => {
                Err(BranchRuntimeError::BranchNotFound { branch_id })
            }
            BranchSnapshotMissingBranchPolicy::Create { config } => {
                Ok((BranchLocalState::new(branch_id, config)?, true))
            }
        },
    }
}

fn build_snapshot_l0_tables(
    branch_id: BranchId,
    output_identity_seed: &TableIdentity,
    table_builder_config: TableBuilderConfig,
    max_rows_per_table: usize,
    rows: &[StorageRow],
) -> BranchRuntimeResult<Vec<BranchOwnedTable>> {
    let builder = ImmutableTableBuilder::new(table_builder_config)
        .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
    let mut tables = Vec::new();
    for (output_index, chunk) in rows.chunks(max_rows_per_table).enumerate() {
        let identity =
            snapshot_table_identity(output_identity_seed, branch_id, output_index, chunk)?;
        let artifact = builder
            .build_from_storage_rows(identity.clone(), chunk)
            .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
        let reader = ImmutableTableReader::open_bytes(
            identity.clone(),
            artifact.into_bytes(),
            TableReaderConfig::default(),
        )
        .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
        let descriptor =
            BranchTableDescriptor::new(identity, reader.facts().clone(), BranchLevel::ZERO)?;
        tables.push(BranchOwnedTable::new(branch_id, descriptor, reader)?);
    }
    Ok(tables)
}

fn snapshot_table_identity(
    output_identity_seed: &TableIdentity,
    branch_id: BranchId,
    output_index: usize,
    rows: &[StorageRow],
) -> BranchRuntimeResult<TableIdentity> {
    let fingerprint = snapshot_rows_fingerprint(rows);
    TableIdentity::new(format!(
        "{}-snapshot-{}-{}-{fingerprint}",
        output_identity_seed.as_str(),
        branch_id,
        output_index,
    ))
    .map_err(|source| BranchRuntimeError::TableRuntime { source })
}

fn snapshot_rows_fingerprint(rows: &[StorageRow]) -> String {
    let mut hash = Sha256::new();
    for row in rows {
        hash.update(TableInternalKeyBytes::from_row(row).as_slice());
        hash.update(row.commit_timestamp().as_micros().to_le_bytes());
        hash.update(row.expires_at().as_micros().to_le_bytes());
        hash.update([u8::from(row.is_tombstone())]);
        hash.update(
            u64::try_from(row.value().len())
                .expect("row value length fits in u64")
                .to_le_bytes(),
        );
        hash.update(row.value());
    }
    hex_lower(&hash.finalize())
}

fn hex_lower(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(char::from(HEX[usize::from(byte >> 4)]));
        out.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    out
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum InheritedReferenceKind {
    Active,
    Materializing,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ObservedBranchRows {
    max_commit_version: Option<CommitVersion>,
    timestamp_min: Option<Timestamp>,
    timestamp_max: Option<Timestamp>,
    put_rows: u64,
    tombstone_rows: u64,
}

impl ObservedBranchRows {
    fn record(&mut self, row: &StorageRow) {
        self.record_commit_version(row.commit_version());
        self.record_timestamp(row.commit_timestamp());
        if row.is_tombstone() {
            self.tombstone_rows = self.tombstone_rows.saturating_add(1);
        } else {
            self.put_rows = self.put_rows.saturating_add(1);
        }
    }

    fn record_inherited_layer(&mut self, layer: &BranchInheritedLayer) {
        if layer.status() == InheritedLayerStatus::Materialized {
            return;
        }
        for table in layer.owned_levels().iter().flatten() {
            for row in table.rows() {
                if row.commit_version().as_u64() <= layer.fork_version().as_u64() {
                    self.record_commit_version(row.commit_version());
                    self.record_timestamp(row.commit_timestamp());
                }
            }
        }
    }

    fn record_commit_version(&mut self, commit_version: CommitVersion) {
        self.max_commit_version = Some(
            self.max_commit_version
                .map_or(commit_version, |max| max.max(commit_version)),
        );
    }

    fn record_timestamp(&mut self, commit_timestamp: Timestamp) {
        self.timestamp_min = Some(
            self.timestamp_min
                .map_or(commit_timestamp, |min| min.min(commit_timestamp)),
        );
        self.timestamp_max = Some(
            self.timestamp_max
                .map_or(commit_timestamp, |max| max.max(commit_timestamp)),
        );
    }
}

fn branch_table_ref_for_owned(
    branch_id: BranchId,
    level: BranchLevel,
    table_index: usize,
    table: &BranchOwnedTable,
) -> BranchRuntimeResult<BranchTableRef> {
    if let Some(materialization_source) = table.materialization_source() {
        BranchTableRef::replacement(
            branch_id,
            materialization_source.source_branch_id(),
            materialization_source.fork_version(),
            level,
            table_index,
            table.descriptor().identity().clone(),
        )
    } else {
        BranchTableRef::owned(
            branch_id,
            level,
            table_index,
            table.descriptor().identity().clone(),
        )
    }
}

fn remove_compacted_tables(
    owned_levels: &mut [Vec<BranchOwnedTable>],
    candidate: &BranchCompactionCandidate,
) -> BranchRuntimeResult<()> {
    let mut refs = candidate.removed_refs();
    refs.sort_by(|left, right| {
        usize::from(right.level().raw())
            .cmp(&usize::from(left.level().raw()))
            .then_with(|| right.table_index().cmp(&left.table_index()))
    });

    for table_ref in refs {
        let level_index = usize::from(table_ref.level().raw());
        let level =
            owned_levels
                .get_mut(level_index)
                .ok_or(BranchRuntimeError::InvalidCompaction {
                    reason: BranchCompactionInvalidity::Generic(
                        "compaction removal level must exist",
                    ),
                })?;
        let table =
            level
                .get(table_ref.table_index())
                .ok_or(BranchRuntimeError::InvalidCompaction {
                    reason: BranchCompactionInvalidity::Generic(
                        "compaction removal table index must exist",
                    ),
                })?;
        if table.descriptor().identity() != table_ref.table_identity() {
            return Err(BranchRuntimeError::InvalidCompaction {
                reason: BranchCompactionInvalidity::Generic(
                    "compaction removal table identity is stale",
                ),
            });
        }
        level.remove(table_ref.table_index());
    }
    Ok(())
}

fn insert_compaction_outputs(
    owned_levels: &mut [Vec<BranchOwnedTable>],
    candidate: &BranchCompactionCandidate,
    mut output_tables: Vec<BranchOwnedTable>,
) -> BranchRuntimeResult<()> {
    let output_level_index = usize::from(candidate.output_level().raw());
    let output_level =
        owned_levels
            .get_mut(output_level_index)
            .ok_or(BranchRuntimeError::InvalidCompaction {
                reason: BranchCompactionInvalidity::Generic("compaction output level must exist"),
            })?;

    for table in &output_tables {
        if table.level() != candidate.output_level() {
            return Err(BranchRuntimeError::InvalidCompaction {
                reason: BranchCompactionInvalidity::Generic(
                    "compaction output table level must match candidate",
                ),
            });
        }
    }

    if candidate.output_level() == BranchLevel::ZERO {
        let insert_index = candidate
            .removed_refs()
            .iter()
            .filter(|table_ref| table_ref.level() == BranchLevel::ZERO)
            .map(BranchTableRef::table_index)
            .min()
            .unwrap_or(output_level.len())
            .min(output_level.len());
        for table in output_tables.drain(..).rev() {
            output_level.insert(insert_index, table);
        }
    } else {
        for table in output_tables {
            insert_sorted_by_range(output_level, table)?;
        }
    }
    Ok(())
}

fn validate_compaction_output_identities(
    output_identities: &[TableIdentity],
    owned_levels: &[Vec<BranchOwnedTable>],
    inherited_layers: &[BranchInheritedLayer],
) -> BranchRuntimeResult<()> {
    let mut output_seen = BTreeSet::<&str>::new();
    for identity in output_identities {
        if !output_seen.insert(identity.as_str()) {
            return Err(BranchRuntimeError::InvalidCompaction {
                reason: BranchCompactionInvalidity::Generic(
                    "compaction output identities must be unique",
                ),
            });
        }
        if branch_reachable_table_identity_exists(identity, owned_levels, inherited_layers) {
            return Err(BranchRuntimeError::InvalidCompaction {
                reason: BranchCompactionInvalidity::Generic(
                    "compaction output identity must not collide with existing reachable table",
                ),
            });
        }
    }
    Ok(())
}

fn branch_reachable_table_identity_exists(
    identity: &TableIdentity,
    owned_levels: &[Vec<BranchOwnedTable>],
    inherited_layers: &[BranchInheritedLayer],
) -> bool {
    owned_levels
        .iter()
        .flatten()
        .any(|table| table.descriptor().identity() == identity)
        || inherited_layers
            .iter()
            .flat_map(BranchInheritedLayer::owned_levels)
            .flatten()
            .any(|table| table.descriptor().identity() == identity)
}

fn validate_manifest_recovery_table_identities(
    owned_levels: &[Vec<BranchOwnedTable>],
    inherited_layers: &[BranchInheritedLayer],
) -> BranchRuntimeResult<()> {
    let mut seen = BTreeSet::<&str>::new();
    for table in owned_levels.iter().flatten().chain(
        inherited_layers
            .iter()
            .flat_map(BranchInheritedLayer::owned_levels)
            .flatten(),
    ) {
        if !seen.insert(table.descriptor().identity().as_str()) {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "table manifest recovery must not contain duplicate table identities",
            });
        }
    }
    Ok(())
}

fn validate_manifest_recovery_inherited_layers(
    branch_id: BranchId,
    layers: &[BranchInheritedLayer],
) -> BranchRuntimeResult<()> {
    let mut previous_fork_version = None::<CommitVersion>;
    let mut source_branches = BTreeSet::<[u8; BranchId::BYTE_LEN]>::new();
    for layer in layers {
        if layer.source_branch_id() == branch_id {
            return Err(BranchRuntimeError::InvalidInheritedLayer {
                reason: "table manifest inherited layer source branch must differ from branch",
            });
        }
        if !source_branches.insert(*layer.source_branch_id().as_bytes()) {
            return Err(BranchRuntimeError::InvalidInheritedLayer {
                reason: "table manifest inherited source branches must be unique",
            });
        }
        if previous_fork_version
            .is_some_and(|previous| layer.fork_version().as_u64() > previous.as_u64())
        {
            return Err(BranchRuntimeError::InvalidInheritedLayer {
                reason: "table manifest inherited layers must be nearest-first by fork version",
            });
        }
        previous_fork_version = Some(layer.fork_version());
        if layer.status() == InheritedLayerStatus::Unavailable {
            return Err(BranchRuntimeError::InvalidInheritedLayer {
                reason: "table manifest recovery cannot install unavailable inherited layers",
            });
        }
    }
    Ok(())
}

fn validate_compaction_levels(owned_levels: &[Vec<BranchOwnedTable>]) -> BranchRuntimeResult<()> {
    let mut seen_keys = BTreeSet::<TableInternalKeyBytes>::new();
    for (level_index, level) in owned_levels.iter().enumerate() {
        let branch_level = BranchLevel::new(u8::try_from(level_index).map_err(|_| {
            BranchRuntimeError::InvalidCompaction {
                reason: BranchCompactionInvalidity::Generic(
                    "compaction level index must fit in BranchLevel",
                ),
            }
        })?);
        let mut previous_first_key = None;
        for (left_index, table) in level.iter().enumerate() {
            if table.level() != branch_level {
                return Err(BranchRuntimeError::InvalidCompaction {
                    reason: BranchCompactionInvalidity::Generic(
                        "compaction table level must match installed level",
                    ),
                });
            }
            if branch_level != BranchLevel::ZERO {
                let first_key = require_table_physical_first_key(table)?;
                if previous_first_key
                    .as_ref()
                    .is_some_and(|previous| previous > &first_key)
                {
                    return Err(BranchRuntimeError::InvalidCompaction { reason: BranchCompactionInvalidity::Generic("compaction output leaves nonzero-level tables out of physical-key order") });
                }
                previous_first_key = Some(first_key);
            }
            for row in table.rows() {
                if !seen_keys.insert(row.key().clone()) {
                    return Err(BranchRuntimeError::InvalidCompaction {
                        reason: BranchCompactionInvalidity::Generic(
                            "compaction levels must not contain duplicate internal keys",
                        ),
                    });
                }
            }
            if branch_level != BranchLevel::ZERO
                && level
                    .iter()
                    .skip(left_index + 1)
                    .any(|right| table_physical_ranges_overlap(table, right))
            {
                return Err(BranchRuntimeError::InvalidCompaction {
                    reason: BranchCompactionInvalidity::Generic(
                        "compaction output leaves overlapping nonzero-level physical key ranges",
                    ),
                });
            }
        }
    }
    Ok(())
}

fn insert_sorted_by_range(
    tables: &mut Vec<BranchOwnedTable>,
    table: BranchOwnedTable,
) -> BranchRuntimeResult<usize> {
    let first_key = require_table_physical_first_key(&table)?;
    let mut index = 0;
    while index < tables.len() && require_table_physical_first_key(&tables[index])? < first_key {
        index += 1;
    }
    tables.insert(index, table);
    Ok(index)
}

fn require_rows_match_frozen(
    table: &BranchOwnedTable,
    frozen: &FrozenTable,
) -> BranchRuntimeResult<()> {
    if table.rows().len() != frozen.len() {
        return Err(BranchRuntimeError::InvalidBranchState {
            reason: "frozen replacement table row count must match frozen table",
        });
    }
    if !table
        .rows()
        .iter()
        .zip(frozen.iter())
        .all(|(left, right)| left.row() == right.row())
    {
        return Err(BranchRuntimeError::InvalidBranchState {
            reason: "frozen replacement table rows must match frozen table",
        });
    }
    Ok(())
}
