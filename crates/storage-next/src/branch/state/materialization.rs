//! Inherited-layer materialization helpers for branch-local state.

use super::{branch_reachable_table_identity_exists, BranchLocalState};
use crate::branch::error::{BranchRuntimeError, BranchRuntimeResult};
use crate::branch::facts::{
    BranchLevel, BranchReachabilitySnapshot, BranchTableDescriptor, InheritedLayerStatus,
};
use crate::branch::identity::rewrite_row_branch;
use crate::branch::read::{BranchMaterializationSource, BranchOwnedTable};
use crate::row::StorageRow;
use crate::table::{
    BuiltTableArtifact, ImmutableTableBuilder, ImmutableTableReader, TableBuilderConfig,
    TableIdentity, TableInternalKeyBytes, TableReaderConfig,
};
use std::collections::{BTreeMap, BTreeSet};
use strata_core_next::{BranchId, CommitVersion};

const MATERIALIZATION_ROWS_PER_OUTPUT_TABLE: usize = 4_096;
const _: () = assert!(MATERIALIZATION_ROWS_PER_OUTPUT_TABLE > 0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BranchMaterializationRecovery {
    ReplacementVisibleLayerRemoved,
    ReplacementAlreadyVisibleLayerRemoved,
    LayerAlreadyMaterialized,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchMaterializationHandle {
    child_branch_id: BranchId,
    source_branch_id: BranchId,
    fork_version: CommitVersion,
    layer_index: usize,
}

impl BranchMaterializationHandle {
    pub(crate) const fn new(
        child_branch_id: BranchId,
        source_branch_id: BranchId,
        fork_version: CommitVersion,
        layer_index: usize,
    ) -> Self {
        Self {
            child_branch_id,
            source_branch_id,
            fork_version,
            layer_index,
        }
    }

    pub(crate) const fn child_branch_id(self) -> BranchId {
        self.child_branch_id
    }

    pub(crate) const fn source_branch_id(self) -> BranchId {
        self.source_branch_id
    }

    pub(crate) const fn fork_version(self) -> CommitVersion {
        self.fork_version
    }

    pub(crate) const fn layer_index(self) -> usize {
        self.layer_index
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchMaterializationIntent {
    handle: BranchMaterializationHandle,
    reachability_snapshot: BranchReachabilitySnapshot,
}

impl BranchMaterializationIntent {
    pub(crate) const fn new(
        handle: BranchMaterializationHandle,
        reachability_snapshot: BranchReachabilitySnapshot,
    ) -> Self {
        Self {
            handle,
            reachability_snapshot,
        }
    }

    pub(crate) const fn handle(&self) -> BranchMaterializationHandle {
        self.handle
    }

    pub(crate) const fn reachability_snapshot(&self) -> &BranchReachabilitySnapshot {
        &self.reachability_snapshot
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchMaterializationRequest {
    child_branch_id: BranchId,
    layer_index: usize,
    source_branch_id: Option<BranchId>,
    fork_version: Option<CommitVersion>,
    output_identity_prefix: String,
}

impl BranchMaterializationRequest {
    pub(crate) fn new(
        child_branch_id: BranchId,
        layer_index: usize,
        output_identity_prefix: impl Into<String>,
    ) -> BranchRuntimeResult<Self> {
        let output_identity_prefix = output_identity_prefix.into();
        Self::new_inner(
            child_branch_id,
            layer_index,
            None,
            None,
            output_identity_prefix,
        )
    }

    pub(crate) fn new_for_source(
        child_branch_id: BranchId,
        layer_index: usize,
        source_branch_id: BranchId,
        fork_version: CommitVersion,
        output_identity_prefix: impl Into<String>,
    ) -> BranchRuntimeResult<Self> {
        let output_identity_prefix = output_identity_prefix.into();
        Self::new_inner(
            child_branch_id,
            layer_index,
            Some(source_branch_id),
            Some(fork_version),
            output_identity_prefix,
        )
    }

    pub(crate) fn from_handle(
        handle: BranchMaterializationHandle,
        output_identity_prefix: impl Into<String>,
    ) -> BranchRuntimeResult<Self> {
        Self::new_for_source(
            handle.child_branch_id(),
            handle.layer_index(),
            handle.source_branch_id(),
            handle.fork_version(),
            output_identity_prefix,
        )
    }

    fn new_inner(
        child_branch_id: BranchId,
        layer_index: usize,
        source_branch_id: Option<BranchId>,
        fork_version: Option<CommitVersion>,
        output_identity_prefix: String,
    ) -> BranchRuntimeResult<Self> {
        if output_identity_prefix.is_empty() {
            return Err(BranchRuntimeError::InvalidConfig {
                field: "output_identity_prefix",
                reason: "must not be empty",
            });
        }
        if output_identity_prefix
            .bytes()
            .any(|byte| byte == b'/' || byte == 0)
        {
            return Err(BranchRuntimeError::InvalidConfig {
                field: "output_identity_prefix",
                reason: "must be an opaque single component",
            });
        }
        Ok(Self {
            child_branch_id,
            layer_index,
            source_branch_id,
            fork_version,
            output_identity_prefix,
        })
    }

    pub(crate) const fn child_branch_id(&self) -> BranchId {
        self.child_branch_id
    }

    pub(crate) const fn layer_index(&self) -> usize {
        self.layer_index
    }

    pub(crate) const fn materialization_source(&self) -> Option<BranchMaterializationSource> {
        match (self.source_branch_id, self.fork_version) {
            (Some(source_branch_id), Some(fork_version)) => Some(BranchMaterializationSource::new(
                source_branch_id,
                fork_version,
            )),
            _ => None,
        }
    }

    pub(crate) fn output_identity_prefix(&self) -> &str {
        &self.output_identity_prefix
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchMaterializationOutcome {
    child_branch_id: BranchId,
    source_branch_id: BranchId,
    fork_version: CommitVersion,
    layer_index: usize,
    rows_materialized: u64,
    tables_created: usize,
    created_table_identities: Vec<TableIdentity>,
    skipped_post_fork_rows: u64,
    skipped_exact_duplicate_rows: u64,
    inherited_layers_remaining: usize,
    replacement_owned_table_count: usize,
    recovery: BranchMaterializationRecovery,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchMaterializationPreparedOutput {
    child_branch_id: BranchId,
    source_branch_id: BranchId,
    fork_version: CommitVersion,
    layer_index: usize,
    rows: Vec<StorageRow>,
    artifacts: Vec<BuiltTableArtifact>,
    skipped_post_fork_rows: u64,
    skipped_exact_duplicate_rows: u64,
    existing_rows: u64,
    existing_tables: usize,
    materialization_source: BranchMaterializationSource,
}

impl BranchMaterializationOutcome {
    pub(crate) const fn child_branch_id(&self) -> BranchId {
        self.child_branch_id
    }

    pub(crate) const fn source_branch_id(&self) -> BranchId {
        self.source_branch_id
    }

    pub(crate) const fn fork_version(&self) -> CommitVersion {
        self.fork_version
    }

    pub(crate) const fn layer_index(&self) -> usize {
        self.layer_index
    }

    pub(crate) const fn rows_materialized(&self) -> u64 {
        self.rows_materialized
    }

    pub(crate) const fn tables_created(&self) -> usize {
        self.tables_created
    }

    pub(crate) fn created_table_identities(&self) -> &[TableIdentity] {
        &self.created_table_identities
    }

    pub(crate) const fn skipped_post_fork_rows(&self) -> u64 {
        self.skipped_post_fork_rows
    }

    pub(crate) const fn skipped_exact_duplicate_rows(&self) -> u64 {
        self.skipped_exact_duplicate_rows
    }

    pub(crate) const fn inherited_layers_remaining(&self) -> usize {
        self.inherited_layers_remaining
    }

    pub(crate) const fn replacement_owned_table_count(&self) -> usize {
        self.replacement_owned_table_count
    }

    pub(crate) const fn recovery(&self) -> BranchMaterializationRecovery {
        self.recovery
    }
}

impl BranchMaterializationPreparedOutput {
    pub(crate) fn artifacts(&self) -> &[BuiltTableArtifact] {
        &self.artifacts
    }

    pub(crate) const fn materialization_source(&self) -> BranchMaterializationSource {
        self.materialization_source
    }
}

impl BranchLocalState {
    pub(crate) fn mark_inherited_layer_materializing(
        &mut self,
        layer_index: usize,
    ) -> BranchRuntimeResult<BranchMaterializationIntent> {
        let layer = self.inherited_layers.get(layer_index).ok_or(
            BranchRuntimeError::InvalidInheritedLayer {
                reason: "materialization layer index must exist",
            },
        )?;
        let handle = BranchMaterializationHandle::new(
            self.branch_id,
            layer.source_branch_id(),
            layer.fork_version(),
            layer_index,
        );
        match layer.status() {
            InheritedLayerStatus::Active => {
                self.inherited_layers[layer_index] =
                    layer.with_status(InheritedLayerStatus::Materializing)?;
                self.materialization_intent(handle)
            }
            InheritedLayerStatus::Materializing | InheritedLayerStatus::Materialized => {
                self.materialization_intent(handle)
            }
            InheritedLayerStatus::Unavailable => Err(BranchRuntimeError::InvalidInheritedLayer {
                reason: "unavailable inherited layers cannot be materialized",
            }),
        }
    }

    pub(crate) fn materialize_inherited_layer(
        &mut self,
        request: &BranchMaterializationRequest,
    ) -> BranchRuntimeResult<BranchMaterializationOutcome> {
        if request.child_branch_id() != self.branch_id {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "materialization request branch id must match branch state",
            });
        }
        let Some(layer) = self.inherited_layers.get(request.layer_index()) else {
            return self.materialize_absent_layer_retry(request);
        };
        let source_branch_id = layer.source_branch_id();
        let fork_version = layer.fork_version();
        let materialization_source =
            BranchMaterializationSource::new(source_branch_id, fork_version);
        if request
            .materialization_source()
            .is_some_and(|expected| expected != materialization_source)
        {
            return Err(BranchRuntimeError::InvalidInheritedLayer {
                reason: "materialization request source identity must match target layer",
            });
        }

        match layer.status() {
            InheritedLayerStatus::Active | InheritedLayerStatus::Materializing => {}
            InheritedLayerStatus::Materialized => {
                return Ok(self.materialized_layer_noop_outcome(
                    source_branch_id,
                    fork_version,
                    request.layer_index(),
                ));
            }
            InheritedLayerStatus::Unavailable => {
                return Err(BranchRuntimeError::InvalidInheritedLayer {
                    reason: "unavailable inherited layers cannot be materialized",
                });
            }
        }

        let Some(prepared) = self.prepare_materialization_output(request)? else {
            return self.materialize_absent_layer_retry(request);
        };
        let replacement_tables = self.materialization_tables_from_artifacts(
            prepared.layer_index,
            prepared.materialization_source,
            prepared.artifacts.clone(),
        )?;
        self.install_materialization_prepared_output(request, &prepared, replacement_tables)
    }

    pub(crate) fn prepare_materialization_output(
        &self,
        request: &BranchMaterializationRequest,
    ) -> BranchRuntimeResult<Option<BranchMaterializationPreparedOutput>> {
        if request.child_branch_id() != self.branch_id {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "materialization request branch id must match branch state",
            });
        }
        let Some(layer) = self.inherited_layers.get(request.layer_index()) else {
            return Ok(None);
        };
        let materialization_source =
            BranchMaterializationSource::new(layer.source_branch_id(), layer.fork_version());
        if request
            .materialization_source()
            .is_some_and(|expected| expected != materialization_source)
        {
            return Err(BranchRuntimeError::InvalidInheritedLayer {
                reason: "materialization request source identity must match target layer",
            });
        }
        match layer.status() {
            InheritedLayerStatus::Active | InheritedLayerStatus::Materializing => {}
            InheritedLayerStatus::Materialized => {
                return Ok(Some(BranchMaterializationPreparedOutput {
                    child_branch_id: self.branch_id,
                    source_branch_id: layer.source_branch_id(),
                    fork_version: layer.fork_version(),
                    layer_index: request.layer_index(),
                    rows: Vec::new(),
                    artifacts: Vec::new(),
                    skipped_post_fork_rows: 0,
                    skipped_exact_duplicate_rows: 0,
                    existing_rows: 0,
                    existing_tables: 0,
                    materialization_source,
                }));
            }
            InheritedLayerStatus::Unavailable => {
                return Err(BranchRuntimeError::InvalidInheritedLayer {
                    reason: "unavailable inherited layers cannot be materialized",
                });
            }
        }
        let materialized = self.collect_materialization_rows(request.layer_index())?;
        let existing_summary = self.existing_materialization_replacement_summary(
            materialization_source,
            request.output_identity_prefix(),
            request.layer_index(),
        );
        let artifacts = Self::build_materialized_l0_artifacts(
            request.layer_index(),
            request.output_identity_prefix(),
            existing_summary.map_or(0, |summary| summary.next_output_index),
            &materialized.rows,
        )?;
        Ok(Some(BranchMaterializationPreparedOutput {
            child_branch_id: self.branch_id,
            source_branch_id: layer.source_branch_id(),
            fork_version: layer.fork_version(),
            layer_index: request.layer_index(),
            rows: materialized.rows,
            artifacts,
            skipped_post_fork_rows: materialized.skipped_post_fork_rows,
            skipped_exact_duplicate_rows: materialized.skipped_exact_duplicate_rows,
            existing_rows: existing_summary.map_or(0, |summary| summary.rows),
            existing_tables: existing_summary.map_or(0, |summary| summary.tables),
            materialization_source,
        }))
    }

    pub(crate) fn install_materialization_prepared_output(
        &mut self,
        request: &BranchMaterializationRequest,
        prepared: &BranchMaterializationPreparedOutput,
        replacement_tables: Vec<BranchOwnedTable>,
    ) -> BranchRuntimeResult<BranchMaterializationOutcome> {
        if request.child_branch_id() != self.branch_id {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "materialization request branch id must match branch state",
            });
        }
        if prepared.child_branch_id != self.branch_id
            || prepared.layer_index != request.layer_index()
            || request
                .materialization_source()
                .is_some_and(|expected| expected != prepared.materialization_source)
        {
            return Err(BranchRuntimeError::InvalidInheritedLayer {
                reason: "prepared materialization output must match request",
            });
        }
        let Some(layer) = self.inherited_layers.get(request.layer_index()) else {
            return self.materialize_absent_layer_retry(request);
        };
        if layer.source_branch_id() != prepared.source_branch_id
            || layer.fork_version() != prepared.fork_version
        {
            return Err(BranchRuntimeError::InvalidInheritedLayer {
                reason: "prepared materialization source must match target layer",
            });
        }
        match layer.status() {
            InheritedLayerStatus::Active | InheritedLayerStatus::Materializing => {}
            InheritedLayerStatus::Materialized => {
                return Ok(self.materialized_layer_noop_outcome(
                    prepared.source_branch_id,
                    prepared.fork_version,
                    prepared.layer_index,
                ));
            }
            InheritedLayerStatus::Unavailable => {
                return Err(BranchRuntimeError::InvalidInheritedLayer {
                    reason: "unavailable inherited layers cannot be materialized",
                });
            }
        }
        let materialized = self.collect_materialization_rows(request.layer_index())?;
        if materialized.rows != prepared.rows {
            return Err(BranchRuntimeError::InvalidInheritedLayer {
                reason: "materialization candidate changed after output publication",
            });
        }
        if let Some(outcome) = self.try_finish_existing_materialization_retry(
            request,
            prepared.materialization_source,
            &materialized,
            self.existing_materialization_replacement_summary(
                prepared.materialization_source,
                request.output_identity_prefix(),
                request.layer_index(),
            ),
        )? {
            return Ok(outcome);
        }
        let created_table_identities = replacement_tables
            .iter()
            .map(|table| table.facts().identity().clone())
            .collect();
        let new_rows_materialized = u64::try_from(prepared.rows.len()).map_err(|_| {
            BranchRuntimeError::InvalidBranchState {
                reason: "materialized row count must fit in u64",
            }
        })?;
        let rows_materialized = prepared.existing_rows.saturating_add(new_rows_materialized);
        let tables_created = replacement_tables.len();
        let replacement_owned_table_count = prepared.existing_tables.saturating_add(tables_created);

        let mut staged = self.clone();
        staged.install_materialization_replacement_tables(replacement_tables)?;
        staged.remove_inherited_layer_by_source(prepared.materialization_source)?;
        *self = staged;
        self.refresh_observed_row_facts();

        Ok(BranchMaterializationOutcome {
            child_branch_id: self.branch_id,
            source_branch_id: prepared.source_branch_id,
            fork_version: prepared.fork_version,
            layer_index: prepared.layer_index,
            rows_materialized,
            tables_created,
            created_table_identities,
            skipped_post_fork_rows: prepared.skipped_post_fork_rows,
            skipped_exact_duplicate_rows: prepared.skipped_exact_duplicate_rows,
            inherited_layers_remaining: self.inherited_layers.len(),
            replacement_owned_table_count,
            recovery: BranchMaterializationRecovery::ReplacementVisibleLayerRemoved,
        })
    }

    fn try_finish_existing_materialization_retry(
        &mut self,
        request: &BranchMaterializationRequest,
        source: BranchMaterializationSource,
        materialized: &MaterializedRows,
        existing_summary: Option<ExistingMaterializationReplacementSummary>,
    ) -> BranchRuntimeResult<Option<BranchMaterializationOutcome>> {
        if !materialized.rows.is_empty() {
            return Ok(None);
        }
        let Some(summary) = existing_summary else {
            return Ok(None);
        };
        let mut staged = self.clone();
        staged.remove_inherited_layer_by_source(source)?;
        *self = staged;
        self.refresh_observed_row_facts();
        Ok(Some(BranchMaterializationOutcome {
            child_branch_id: self.branch_id,
            source_branch_id: source.source_branch_id(),
            fork_version: source.fork_version(),
            layer_index: request.layer_index(),
            rows_materialized: summary.rows,
            tables_created: 0,
            created_table_identities: Vec::new(),
            skipped_post_fork_rows: materialized.skipped_post_fork_rows,
            skipped_exact_duplicate_rows: materialized.skipped_exact_duplicate_rows,
            inherited_layers_remaining: self.inherited_layers.len(),
            replacement_owned_table_count: summary.tables,
            recovery: BranchMaterializationRecovery::ReplacementAlreadyVisibleLayerRemoved,
        }))
    }

    fn materialization_intent(
        &self,
        handle: BranchMaterializationHandle,
    ) -> BranchRuntimeResult<BranchMaterializationIntent> {
        Ok(BranchMaterializationIntent::new(
            handle,
            self.reachability_snapshot()?,
        ))
    }

    fn materialized_layer_noop_outcome(
        &self,
        source_branch_id: BranchId,
        fork_version: CommitVersion,
        layer_index: usize,
    ) -> BranchMaterializationOutcome {
        BranchMaterializationOutcome {
            child_branch_id: self.branch_id,
            source_branch_id,
            fork_version,
            layer_index,
            rows_materialized: 0,
            tables_created: 0,
            created_table_identities: Vec::new(),
            skipped_post_fork_rows: 0,
            skipped_exact_duplicate_rows: 0,
            inherited_layers_remaining: self.inherited_layers.len(),
            replacement_owned_table_count: 0,
            recovery: BranchMaterializationRecovery::LayerAlreadyMaterialized,
        }
    }

    fn materialize_absent_layer_retry(
        &self,
        request: &BranchMaterializationRequest,
    ) -> BranchRuntimeResult<BranchMaterializationOutcome> {
        if let Some(source) = request.materialization_source() {
            if let Some(summary) = self.existing_materialization_replacement_summary(
                source,
                request.output_identity_prefix(),
                request.layer_index(),
            ) {
                return Ok(BranchMaterializationOutcome {
                    child_branch_id: self.branch_id,
                    source_branch_id: source.source_branch_id(),
                    fork_version: source.fork_version(),
                    layer_index: request.layer_index(),
                    rows_materialized: summary.rows,
                    tables_created: 0,
                    created_table_identities: Vec::new(),
                    skipped_post_fork_rows: 0,
                    skipped_exact_duplicate_rows: 0,
                    inherited_layers_remaining: self.inherited_layers.len(),
                    replacement_owned_table_count: summary.tables,
                    recovery: BranchMaterializationRecovery::LayerAlreadyMaterialized,
                });
            }
        }
        Err(BranchRuntimeError::InvalidInheritedLayer {
            reason: "materialization layer index must exist",
        })
    }

    fn install_materialization_replacement_tables(
        &mut self,
        replacement_tables: Vec<BranchOwnedTable>,
    ) -> BranchRuntimeResult<()> {
        let output_identities = replacement_tables
            .iter()
            .map(|table| table.descriptor().identity().clone())
            .collect::<Vec<_>>();
        validate_materialization_output_identities(
            &output_identities,
            &self.owned_levels,
            &self.inherited_layers,
        )?;
        // Materialized rows represent an inherited snapshot, so they sit behind
        // ordinary child-owned L0 tables even though they become owned by the
        // child branch.
        let level_index = usize::from(BranchLevel::ZERO.raw());
        for table in replacement_tables {
            self.validate_install(BranchLevel::ZERO, &table, None)?;
            self.owned_levels[level_index].push(table);
        }
        self.refresh_observed_row_facts();
        Ok(())
    }

    fn remove_inherited_layer_by_source(
        &mut self,
        source: BranchMaterializationSource,
    ) -> BranchRuntimeResult<()> {
        let index = self
            .inherited_layers
            .iter()
            .position(|layer| {
                layer.source_branch_id() == source.source_branch_id()
                    && layer.fork_version() == source.fork_version()
            })
            .ok_or(BranchRuntimeError::InvalidInheritedLayer {
                reason: "materialization source layer must still exist",
            })?;
        self.inherited_layers.remove(index);
        self.refresh_observed_row_facts();
        Ok(())
    }

    fn existing_materialization_replacement_summary(
        &self,
        source: BranchMaterializationSource,
        output_identity_prefix: &str,
        layer_index: usize,
    ) -> Option<ExistingMaterializationReplacementSummary> {
        let mut summary = ExistingMaterializationReplacementSummary::default();
        for table in self.owned_levels.iter().flatten() {
            if table.materialization_source() == Some(source) {
                let Ok(row_count) = u64::try_from(table.rows().len()) else {
                    return None;
                };
                summary.tables = summary.tables.saturating_add(1);
                summary.rows = summary.rows.saturating_add(row_count);
                if let Some(output_index) = materialized_table_output_index(
                    table.descriptor().identity(),
                    output_identity_prefix,
                    layer_index,
                ) {
                    summary.next_output_index = summary
                        .next_output_index
                        .max(output_index.saturating_add(1));
                }
            }
        }
        if summary.tables == 0 {
            None
        } else {
            Some(summary)
        }
    }

    fn collect_materialization_rows(
        &self,
        layer_index: usize,
    ) -> BranchRuntimeResult<MaterializedRows> {
        let layer = self.inherited_layers.get(layer_index).ok_or(
            BranchRuntimeError::InvalidInheritedLayer {
                reason: "materialization layer index must exist",
            },
        )?;
        let higher_precedence_rows = self.higher_precedence_materialization_rows(layer_index)?;
        let mut target_keys = BTreeSet::<TableInternalKeyBytes>::new();
        let mut rows = Vec::new();
        let mut skipped_post_fork_rows = 0u64;
        let mut skipped_exact_duplicate_rows = 0u64;

        for table in layer.owned_levels().iter().flatten() {
            for row in table.rows() {
                if row.commit_version().as_u64() > layer.fork_version().as_u64() {
                    skipped_post_fork_rows = skipped_post_fork_rows.saturating_add(1);
                    continue;
                }
                let rewritten =
                    rewrite_row_branch(row.row(), layer.source_branch_id(), self.branch_id)
                        .map_err(|_| BranchRuntimeError::InvalidInheritedLayer {
                            reason: "materialization row branch rewrite failed",
                        })?;
                let key = TableInternalKeyBytes::from_row(&rewritten);
                if !target_keys.insert(key.clone()) {
                    return Err(BranchRuntimeError::InvalidInheritedLayer {
                        reason:
                            "materialized inherited rows must not contain duplicate internal keys",
                    });
                }
                if let Some(rows) = higher_precedence_rows.get(&key) {
                    if rows.iter().any(|existing| existing == &rewritten) {
                        skipped_exact_duplicate_rows =
                            skipped_exact_duplicate_rows.saturating_add(1);
                        continue;
                    }
                    return Err(BranchRuntimeError::InvalidInheritedLayer {
                        reason: "materialized inherited rows must not collide with higher-precedence rows",
                    });
                }
                rows.push(rewritten);
            }
        }

        rows.sort_by(|left, right| {
            TableInternalKeyBytes::from_row(left).cmp(&TableInternalKeyBytes::from_row(right))
        });

        Ok(MaterializedRows {
            rows,
            skipped_post_fork_rows,
            skipped_exact_duplicate_rows,
        })
    }

    fn higher_precedence_materialization_rows(
        &self,
        target_layer_index: usize,
    ) -> BranchRuntimeResult<BTreeMap<TableInternalKeyBytes, Vec<StorageRow>>> {
        let mut rows_by_key = BTreeMap::<TableInternalKeyBytes, Vec<StorageRow>>::new();
        for row in self.active.iter() {
            rows_by_key
                .entry(row.key().clone())
                .or_default()
                .push(row.row().clone());
        }
        for table in &self.frozen {
            for row in table.iter() {
                rows_by_key
                    .entry(row.key().clone())
                    .or_default()
                    .push(row.row().clone());
            }
        }
        for table in self.owned_levels.iter().flatten() {
            for row in table.rows() {
                rows_by_key
                    .entry(row.key().clone())
                    .or_default()
                    .push(row.row().clone());
            }
        }
        for layer in self.inherited_layers.iter().take(target_layer_index) {
            match layer.status() {
                InheritedLayerStatus::Active | InheritedLayerStatus::Materializing => {}
                InheritedLayerStatus::Materialized => continue,
                InheritedLayerStatus::Unavailable => {
                    return Err(BranchRuntimeError::InvalidInheritedLayer {
                        reason:
                            "unavailable closer inherited layers cannot be used for materialization",
                    });
                }
            }
            for table in layer.owned_levels().iter().flatten() {
                for row in table.rows() {
                    if row.commit_version().as_u64() > layer.fork_version().as_u64() {
                        continue;
                    }
                    let rewritten =
                        rewrite_row_branch(row.row(), layer.source_branch_id(), self.branch_id)
                            .map_err(|_| BranchRuntimeError::InvalidInheritedLayer {
                                reason: "closer inherited row branch rewrite failed",
                            })?;
                    rows_by_key
                        .entry(TableInternalKeyBytes::from_row(&rewritten))
                        .or_default()
                        .push(rewritten);
                }
            }
        }
        Ok(rows_by_key)
    }

    fn build_materialized_l0_artifacts(
        layer_index: usize,
        output_identity_prefix: &str,
        output_index_start: usize,
        rows: &[StorageRow],
    ) -> BranchRuntimeResult<Vec<BuiltTableArtifact>> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }

        let builder = ImmutableTableBuilder::new(TableBuilderConfig::default())
            .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
        let mut artifacts = Vec::new();
        for (output_index, chunk) in rows
            .chunks(MATERIALIZATION_ROWS_PER_OUTPUT_TABLE)
            .enumerate()
        {
            let identity = materialized_table_identity(
                output_identity_prefix,
                layer_index,
                output_index_start.saturating_add(output_index),
            )?;
            let artifact = builder
                .build_from_storage_rows(identity.clone(), chunk)
                .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
            artifacts.push(artifact);
        }
        Ok(artifacts)
    }

    pub(crate) fn materialization_tables_from_artifacts(
        &self,
        layer_index: usize,
        materialization_source: BranchMaterializationSource,
        artifacts: Vec<BuiltTableArtifact>,
    ) -> BranchRuntimeResult<Vec<BranchOwnedTable>> {
        if self.inherited_layers.get(layer_index).is_none() {
            return Err(BranchRuntimeError::InvalidInheritedLayer {
                reason: "materialization layer index must exist",
            });
        }
        let mut tables = Vec::new();
        for artifact in artifacts {
            let identity = artifact.facts().identity().clone();
            let reader = ImmutableTableReader::open_bytes(
                identity.clone(),
                artifact.into_bytes(),
                TableReaderConfig::default(),
            )
            .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
            let descriptor =
                BranchTableDescriptor::new(identity, reader.facts().clone(), BranchLevel::ZERO)?;
            tables.push(BranchOwnedTable::new_materialization_replacement(
                self.branch_id,
                descriptor,
                reader,
                materialization_source,
            )?);
        }
        Ok(tables)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MaterializedRows {
    rows: Vec<StorageRow>,
    skipped_post_fork_rows: u64,
    skipped_exact_duplicate_rows: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ExistingMaterializationReplacementSummary {
    rows: u64,
    tables: usize,
    next_output_index: usize,
}

fn validate_materialization_output_identities(
    output_identities: &[TableIdentity],
    owned_levels: &[Vec<BranchOwnedTable>],
    inherited_layers: &[crate::branch::read::BranchInheritedLayer],
) -> BranchRuntimeResult<()> {
    let mut output_seen = BTreeSet::<&str>::new();
    for identity in output_identities {
        if !output_seen.insert(identity.as_str()) {
            return Err(BranchRuntimeError::InvalidInheritedLayer {
                reason: "materialization output identities must be unique",
            });
        }
        if branch_reachable_table_identity_exists(identity, owned_levels, inherited_layers) {
            return Err(BranchRuntimeError::InvalidInheritedLayer {
                reason:
                    "materialization output identity must not collide with existing reachable table",
            });
        }
    }
    Ok(())
}

fn materialized_table_identity(
    output_identity_prefix: &str,
    layer_index: usize,
    output_index: usize,
) -> BranchRuntimeResult<TableIdentity> {
    TableIdentity::new(format!(
        "{output_identity_prefix}-layer-{layer_index}-table-{output_index}"
    ))
    .map_err(|source| BranchRuntimeError::TableRuntime { source })
}

fn materialized_table_output_index(
    identity: &TableIdentity,
    output_identity_prefix: &str,
    layer_index: usize,
) -> Option<usize> {
    let prefix = format!("{output_identity_prefix}-layer-{layer_index}-table-");
    let suffix = identity.as_str().strip_prefix(&prefix)?;
    if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    suffix.parse().ok()
}
