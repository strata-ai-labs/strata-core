//! Branch-owned table compaction planning and installation.

use super::{branch_reachable_table_identity_exists, insert_sorted_by_range, BranchLocalState};
use crate::branch::error::{BranchCompactionInvalidity, BranchRuntimeError, BranchRuntimeResult};
use crate::branch::facts::BranchTableReferenceKind;
use crate::branch::facts::{BranchLevel, BranchTableDescriptor, BranchTableRef};
use crate::branch::pruning::{BranchCompactionPruningPolicy, BranchCompactionPruningProof};
use crate::branch::read::{
    require_table_physical_first_key, table_physical_ranges_overlap, BranchInheritedLayer,
    BranchMaterializationSource, BranchOwnedTable, BranchTimestampCoverage,
};
use crate::observability::perf_trace;
use crate::table::{
    BuiltTableArtifact, ImmutableTableReader, TableBuilderConfig, TableCompactionConfig,
    TableCompactionDecision, TableCompactionInput, TableCompactionPolicy, TableCompactionReport,
    TableCompactionRowContext, TableCompactionSourceId, TableCompactor, TableCursor, TableIdentity,
    TableInternalKeyBytes, TableReaderConfig, TableRow, TableRuntimeResult,
};
use std::collections::BTreeSet;
use strata_core_next::BranchId;

fn keep_all_policy() -> impl TableCompactionPolicy {
    |_: &TableCompactionRowContext<'_>, _: &TableRow| Ok(TableCompactionDecision::Keep)
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

    pub(crate) fn output_level(&self) -> Option<BranchLevel> {
        self.candidate
            .as_ref()
            .map(BranchCompactionCandidate::output_level)
    }

    pub(crate) fn materialization_source(&self) -> Option<BranchMaterializationSource> {
        self.candidate
            .as_ref()
            .and_then(BranchLocalState::compaction_output_materialization_source)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchCompactionOutcome {
    branch_id: BranchId,
    noop_reason: Option<BranchCompactionNoopReason>,
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
            noop_reason: Some(reason),
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
            noop_reason: None,
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

    pub(crate) const fn noop_reason(&self) -> Option<BranchCompactionNoopReason> {
        self.noop_reason
    }

    pub(crate) const fn installed_replacement_tables(&self) -> bool {
        self.noop_reason.is_none()
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

struct BranchTableCompactionSource<'a> {
    id: TableCompactionSourceId,
    table: &'a BranchOwnedTable,
}

impl<'a> BranchTableCompactionSource<'a> {
    const fn new(id: TableCompactionSourceId, table: &'a BranchOwnedTable) -> Self {
        Self { id, table }
    }
}

impl TableCompactionInput for BranchTableCompactionSource<'_> {
    fn id(&self) -> &TableCompactionSourceId {
        &self.id
    }

    fn open_cursor(&self) -> TableRuntimeResult<Box<dyn TableCursor + '_>> {
        perf_trace::record_branch_compaction_source_opens(1);
        Ok(Box::new(self.table.reader().cursor()))
    }
}

impl BranchLocalState {
    pub(crate) fn plan_branch_compaction(
        &self,
        request: &BranchCompactionRequest,
    ) -> BranchRuntimeResult<BranchCompactionPlan> {
        self.validate_compaction_request(request)?;
        match request.kind() {
            BranchCompactionKind::CompactL0 => self.plan_l0_compaction(request.kind()),
            BranchCompactionKind::CompactL0ToLevelOne => {
                self.plan_l0_to_l1_compaction(request.kind())
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
    ) -> BranchRuntimeResult<Option<(Vec<BuiltTableArtifact>, TableCompactionReport)>> {
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
        let source_refs = sources
            .iter()
            .map(|source| source as &dyn TableCompactionInput)
            .collect::<Vec<_>>();
        let compactor = TableCompactor::new(
            request.table_compaction_config(),
            request.table_builder_config(),
        )
        .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
        let output = match request.retention_policy() {
            BranchCompactionRetentionPolicy::KeepAll => {
                let mut policy = keep_all_policy();
                compactor
                    .compact_inputs(request.output_identity_seed(), &source_refs, &mut policy)
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
                    .compact_inputs(request.output_identity_seed(), &source_refs, &mut policy)
                    .map_err(|source| BranchRuntimeError::TableRuntime { source })?
            }
        };
        let (artifacts, report) = output.into_parts();
        perf_trace::record_branch_compaction_peak_buffered_rows(report.peak_buffered_rows());
        Ok(Some((artifacts, report)))
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
        let (artifacts, report) = self.prepare_branch_compaction_plan(request, plan)?.ok_or(
            BranchRuntimeError::InvalidCompaction {
                reason: BranchCompactionInvalidity::Generic(
                    "compaction candidate must produce prepared output",
                ),
            },
        )?;
        let output_tables = self.compaction_output_tables(
            candidate.output_level(),
            artifacts,
            Self::compaction_output_materialization_source(candidate),
        )?;
        self.install_branch_compaction_prepared_plan(request, plan, output_tables, report)
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

    fn plan_l0_to_l1_compaction(
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
    ) -> BranchRuntimeResult<Vec<BranchTableCompactionSource<'_>>> {
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
                Ok(BranchTableCompactionSource::new(source_id, table))
            })
            .collect()
    }

    fn compaction_output_tables(
        &self,
        output_level: BranchLevel,
        artifacts: Vec<BuiltTableArtifact>,
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

pub(super) fn validate_compaction_levels(
    owned_levels: &[Vec<BranchOwnedTable>],
) -> BranchRuntimeResult<()> {
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
