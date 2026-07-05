//! Inherited-layer materialization helpers for branch-local state.

use std::sync::Arc;

use super::{branch_reachable_table_identity_exists, BranchLocalState};
use crate::branch::error::{BranchRuntimeError, BranchRuntimeResult};
use crate::branch::facts::{
    BranchLevel, BranchReachabilitySnapshot, BranchTableDescriptor, InheritedLayerStatus,
};
use crate::branch::identity::rewrite_row_branch;
use crate::branch::read::{BranchMaterializationSource, BranchOwnedTable};
use crate::observability::perf_trace;
use crate::row::StorageRow;
use crate::table::{
    BuiltTableArtifact, ImmutableTableReader, TableBuilderConfig, TableCompactionInput,
    TableCompactionMergeCursor, TableCompactionSourceId, TableCursor, TableIdentity,
    TableInternalKeyBytes, TableReaderConfig, TableRow, TableRuntimeError, TableRuntimeResult,
    TableStreamingArtifactBuilder, TableStreamingArtifactReport,
};
use std::cell::Cell;
use std::cmp::Ordering;
use std::collections::BTreeSet;
use strata_core_next::{BranchId, CommitVersion};

const MATERIALIZATION_ROWS_PER_OUTPUT_TABLE: usize = 4_096;
const _: () = assert!(MATERIALIZATION_ROWS_PER_OUTPUT_TABLE > 0);
const MATERIALIZATION_HASH_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const MATERIALIZATION_HASH_PRIME: u64 = 0x0000_0100_0000_01b3;

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
    materialized_rows: MaterializedRowsSummary,
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
    pub(crate) const fn layer_index(&self) -> usize {
        self.layer_index
    }

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
    ) -> BranchRuntimeResult<(BranchMaterializationHandle, BranchReachabilitySnapshot)> {
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
                self.materialization_binding(handle)
            }
            InheritedLayerStatus::Materializing | InheritedLayerStatus::Materialized => {
                self.materialization_binding(handle)
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
                    materialized_rows: MaterializedRowsSummary::default(),
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
        let existing_summary = self.existing_materialization_replacement_summary(
            materialization_source,
            request.output_identity_prefix(),
            request.layer_index(),
        );
        let materialized = self.stream_materialization_rows(
            request.layer_index(),
            MaterializationOutputMode::Build {
                output_identity_prefix: request.output_identity_prefix(),
                output_index_start: existing_summary.map_or(0, |summary| summary.next_output_index),
            },
        )?;
        Ok(Some(BranchMaterializationPreparedOutput {
            child_branch_id: self.branch_id,
            source_branch_id: layer.source_branch_id(),
            fork_version: layer.fork_version(),
            layer_index: request.layer_index(),
            materialized_rows: materialized.summary,
            artifacts: materialized.artifacts,
            skipped_post_fork_rows: materialized.summary.skipped_post_fork_rows,
            skipped_exact_duplicate_rows: materialized.summary.skipped_exact_duplicate_rows,
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
        validate_prepared_materialization_replacement_tables(prepared, &replacement_tables)?;
        let materialized = self.stream_materialization_rows(
            request.layer_index(),
            MaterializationOutputMode::Verify {
                artifacts: &prepared.artifacts,
            },
        )?;
        if materialized.summary != prepared.materialized_rows {
            return Err(BranchRuntimeError::InvalidInheritedLayer {
                reason: "materialization candidate changed after output publication",
            });
        }
        if let Some(outcome) = self.try_finish_existing_materialization_retry(
            request,
            prepared.materialization_source,
            &materialized.summary,
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
        let rows_materialized = prepared
            .existing_rows
            .saturating_add(prepared.materialized_rows.rows);
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
        materialized: &MaterializedRowsSummary,
        existing_summary: Option<ExistingMaterializationReplacementSummary>,
    ) -> BranchRuntimeResult<Option<BranchMaterializationOutcome>> {
        if materialized.rows != 0 {
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

    fn materialization_binding(
        &self,
        handle: BranchMaterializationHandle,
    ) -> BranchRuntimeResult<(BranchMaterializationHandle, BranchReachabilitySnapshot)> {
        Ok((handle, self.reachability_snapshot()?))
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
            self.owned_levels(),
            &self.inherited_layers,
        )?;
        // Materialized rows represent an inherited snapshot, so they sit behind
        // ordinary child-owned L0 tables even though they become owned by the
        // child branch.
        let level_index = usize::from(BranchLevel::ZERO.raw());
        for table in replacement_tables {
            self.validate_install(BranchLevel::ZERO, &table)?;
            Arc::make_mut(&mut self.layout).levels_mut()[level_index].push(table);
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
        for table in self.owned_levels().iter().flatten() {
            if table.materialization_source() == Some(source) {
                // BS4.4a-i: the row count is on facts (u64) — no materialization needed.
                let row_count = table.facts().row_count();
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

    fn stream_materialization_rows(
        &self,
        layer_index: usize,
        output_mode: MaterializationOutputMode<'_>,
    ) -> BranchRuntimeResult<MaterializedRows> {
        let layer = self.inherited_layers.get(layer_index).ok_or(
            BranchRuntimeError::InvalidInheritedLayer {
                reason: "materialization layer index must exist",
            },
        )?;
        let stats = MaterializationStreamStats::default();
        let sources = materialization_table_sources(
            layer.owned_levels().iter().flatten(),
            layer.source_branch_id(),
            self.branch_id,
            layer.fork_version(),
            &stats,
        )?;
        let source_refs = sources
            .iter()
            .map(|source| source as &dyn TableCompactionInput)
            .collect::<Vec<_>>();
        let mut target_keys = BTreeSet::<TableInternalKeyBytes>::new();
        let mut summary = MaterializedRowsSummary::default();
        let mut artifact_builder = materialization_artifact_builder(output_mode, layer_index)?;
        let mut artifact_verifier = materialization_artifact_verifier(output_mode);

        {
            let mut merged = TableCompactionMergeCursor::new(&source_refs)
                .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
            while let Some(row) = merged.current().map(|current| current.row().clone()) {
                let key = row.key().clone();
                if !target_keys.insert(key.clone()) {
                    return Err(BranchRuntimeError::InvalidInheritedLayer {
                        reason:
                            "materialized inherited rows must not contain duplicate internal keys",
                    });
                }
                // BS4.4e: probe the higher-precedence sources for this exact internal key instead of a
                // precomputed whole-branch map (O(dataset) RAM). The gathered set equals the old map's
                // `Vec` for `key`, so the shadow-skip/collision rules are unchanged.
                let higher_precedence = self.higher_precedence_rows_for_row(layer_index, &row)?;
                if !higher_precedence.is_empty() {
                    if higher_precedence
                        .iter()
                        .any(|existing| existing == row.row())
                    {
                        summary.record_shadow_skip();
                        stats.record_shadow_skip();
                        merged
                            .advance()
                            .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
                        continue;
                    }
                    return Err(BranchRuntimeError::InvalidInheritedLayer {
                        reason: "materialized inherited rows must not collide with higher-precedence rows",
                    });
                }
                summary.record_row(&row);
                if let Some(verifier) = artifact_verifier.as_mut() {
                    verifier.verify_row(&row)?;
                }
                if let Some(builder) = artifact_builder.as_mut() {
                    builder
                        .push(row)
                        .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
                }
                merged
                    .advance()
                    .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
            }
        }

        finish_materialization_stream(summary, artifact_builder, artifact_verifier, &stats)
    }

    /// BS4.4e: gather every higher-precedence row sharing `target`'s exact internal key, in the same
    /// source order the old whole-branch map used (active → frozen → owned levels → closer inherited),
    /// via per-key probes (index+filter accelerated). Replaces `higher_precedence_materialization_rows`
    /// on the hot path; that fn survives under `cfg(test)` as the differential oracle.
    fn higher_precedence_rows_for_row(
        &self,
        target_layer_index: usize,
        target: &TableRow,
    ) -> BranchRuntimeResult<Vec<StorageRow>> {
        let key = target.key();
        let mut rows = Vec::new();
        if let Some(row) = self.active.get(key) {
            rows.push(row.row().clone());
        }
        for table in &self.frozen {
            if let Some(row) = table.get(key) {
                rows.push(row.row().clone());
            }
        }
        for table in self.owned_levels().iter().flatten() {
            if let Some(hit) = table
                .reader()
                .try_get_exact(key)
                .map_err(|source| BranchRuntimeError::TableRuntime { source })?
            {
                rows.push(hit.row().clone());
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
            // Rewrite the target row to the source branch to get the internal key to probe there, then
            // rewrite each hit (commit ≤ fork) back to the child branch so its key matches `key`.
            let source_probe =
                rewrite_row_branch(target.row(), self.branch_id, layer.source_branch_id())
                    .map_err(|_| BranchRuntimeError::InvalidInheritedLayer {
                        reason: "closer inherited row branch rewrite failed",
                    })?;
            let source_key = TableInternalKeyBytes::from_row(&source_probe);
            for table in layer.owned_levels().iter().flatten() {
                if let Some(hit) = table
                    .reader()
                    .try_get_exact(&source_key)
                    .map_err(|source| BranchRuntimeError::TableRuntime { source })?
                {
                    if hit.row().commit_version().as_u64() > layer.fork_version().as_u64() {
                        continue;
                    }
                    let rewritten =
                        rewrite_row_branch(hit.row(), layer.source_branch_id(), self.branch_id)
                            .map_err(|_| BranchRuntimeError::InvalidInheritedLayer {
                                reason: "closer inherited row branch rewrite failed",
                            })?;
                    rows.push(rewritten);
                }
            }
        }
        Ok(rows)
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
    summary: MaterializedRowsSummary,
    artifacts: Vec<BuiltTableArtifact>,
}

type MaterializationArtifactBuilder<'a> =
    TableStreamingArtifactBuilder<BoxedMaterializationIdentityFn<'a>>;
type BoxedMaterializationIdentityFn<'a> =
    Box<dyn FnMut(usize, &[TableRow]) -> TableRuntimeResult<TableIdentity> + 'a>;

#[derive(Clone, Copy)]
enum MaterializationOutputMode<'a> {
    Build {
        output_identity_prefix: &'a str,
        output_index_start: usize,
    },
    Verify {
        artifacts: &'a [BuiltTableArtifact],
    },
}

fn materialization_artifact_builder<'a>(
    output_mode: MaterializationOutputMode<'a>,
    layer_index: usize,
) -> BranchRuntimeResult<Option<MaterializationArtifactBuilder<'a>>> {
    let MaterializationOutputMode::Build {
        output_identity_prefix,
        output_index_start,
    } = output_mode
    else {
        return Ok(None);
    };
    let identity_for_output: BoxedMaterializationIdentityFn<'a> =
        Box::new(move |output_index, _rows| {
            TableIdentity::new(format!(
                "{output_identity_prefix}-layer-{layer_index}-table-{}",
                output_index_start.saturating_add(output_index)
            ))
        });
    TableStreamingArtifactBuilder::new(
        TableBuilderConfig::default(),
        MATERIALIZATION_ROWS_PER_OUTPUT_TABLE,
        identity_for_output,
    )
    .map(Some)
    .map_err(|source| BranchRuntimeError::TableRuntime { source })
}

fn materialization_artifact_verifier(
    output_mode: MaterializationOutputMode<'_>,
) -> Option<PreparedMaterializationArtifactVerifier<'_>> {
    let MaterializationOutputMode::Verify { artifacts } = output_mode else {
        return None;
    };
    Some(PreparedMaterializationArtifactVerifier::new(artifacts))
}

fn finish_materialization_stream(
    mut summary: MaterializedRowsSummary,
    artifact_builder: Option<MaterializationArtifactBuilder<'_>>,
    mut artifact_verifier: Option<PreparedMaterializationArtifactVerifier<'_>>,
    stats: &MaterializationStreamStats,
) -> BranchRuntimeResult<MaterializedRows> {
    if let Some(verifier) = artifact_verifier.as_mut() {
        verifier.finish()?;
    }
    summary.skipped_post_fork_rows = stats.rows_skipped_by_fork();
    let (artifacts, output_report) = match artifact_builder {
        Some(builder) => builder
            .finish()
            .map_err(|source| BranchRuntimeError::TableRuntime { source })?,
        None => (Vec::new(), TableStreamingArtifactReport::default()),
    };
    record_materialization_stream_stats(stats, &output_report);
    Ok(MaterializedRows { summary, artifacts })
}

fn record_materialization_stream_stats(
    stats: &MaterializationStreamStats,
    output_report: &TableStreamingArtifactReport,
) {
    perf_trace::record_branch_materialization_source_opens(stats.source_table_opens());
    perf_trace::record_branch_materialization_rows_rewritten(stats.rows_rewritten());
    perf_trace::record_branch_materialization_rows_skipped_by_fork(stats.rows_skipped_by_fork());
    perf_trace::record_branch_materialization_rows_skipped_by_shadowing(
        stats.rows_skipped_by_shadowing(),
    );
    perf_trace::record_branch_materialization_output_tables(output_report.output_tables());
    perf_trace::record_branch_materialization_peak_buffered_rows(
        output_report.peak_buffered_rows(),
    );
}

struct PreparedMaterializationArtifactVerifier<'a> {
    artifacts: &'a [BuiltTableArtifact],
    artifact_index: usize,
    rows: Vec<TableRow>,
    row_index: usize,
}

impl<'a> PreparedMaterializationArtifactVerifier<'a> {
    const fn new(artifacts: &'a [BuiltTableArtifact]) -> Self {
        Self {
            artifacts,
            artifact_index: 0,
            rows: Vec::new(),
            row_index: 0,
        }
    }

    fn verify_row(&mut self, row: &TableRow) -> BranchRuntimeResult<()> {
        while self.row_index >= self.rows.len() {
            if self.artifact_index >= self.artifacts.len() {
                return Err(materialization_candidate_changed());
            }
            self.load_next_artifact()?;
        }
        let expected = self
            .rows
            .get(self.row_index)
            .ok_or_else(materialization_candidate_changed)?;
        if expected.row() != row.row() {
            return Err(materialization_candidate_changed());
        }
        self.row_index = self.row_index.saturating_add(1);
        Ok(())
    }

    fn finish(&mut self) -> BranchRuntimeResult<()> {
        loop {
            if self.row_index < self.rows.len() {
                return Err(materialization_candidate_changed());
            }
            if self.artifact_index >= self.artifacts.len() {
                return Ok(());
            }
            self.load_next_artifact()?;
        }
    }

    fn load_next_artifact(&mut self) -> BranchRuntimeResult<()> {
        let artifact = self
            .artifacts
            .get(self.artifact_index)
            .ok_or_else(materialization_candidate_changed)?;
        let reader = ImmutableTableReader::open_bytes(
            artifact.facts().identity().clone(),
            artifact.bytes().to_vec(),
            TableReaderConfig::default(),
        )
        .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
        self.rows = reader.rows().to_vec();
        self.row_index = 0;
        self.artifact_index = self.artifact_index.saturating_add(1);
        Ok(())
    }
}

fn materialization_candidate_changed() -> BranchRuntimeError {
    BranchRuntimeError::InvalidInheritedLayer {
        reason: "materialization candidate changed after output publication",
    }
}

fn materialization_replacement_mismatch() -> BranchRuntimeError {
    BranchRuntimeError::InvalidInheritedLayer {
        reason: "materialization replacement tables must match prepared output",
    }
}

fn validate_prepared_materialization_replacement_tables(
    prepared: &BranchMaterializationPreparedOutput,
    replacement_tables: &[BranchOwnedTable],
) -> BranchRuntimeResult<()> {
    if replacement_tables.len() != prepared.artifacts.len() {
        return Err(materialization_replacement_mismatch());
    }
    for (artifact, table) in prepared.artifacts.iter().zip(replacement_tables) {
        if table.branch_id() != prepared.child_branch_id
            || table.level() != BranchLevel::ZERO
            || table.materialization_source() != Some(prepared.materialization_source)
            || table.facts() != artifact.facts()
        {
            return Err(materialization_replacement_mismatch());
        }
        let reader = ImmutableTableReader::open_bytes(
            artifact.facts().identity().clone(),
            artifact.bytes().to_vec(),
            TableReaderConfig::default(),
        )
        .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
        // BS4.4a-ii-a: `reader` is a fresh eager reader (open_bytes); pair its rows against the owned
        // table's cursor so the owned side streams in ascending internal-key order instead of
        // materializing. Row counts are compared via facts (no scan) before the paired walk.
        if reader.rows().len() as u64 != table.facts().row_count() {
            return Err(materialization_replacement_mismatch());
        }
        let mut actual = table.reader().cursor();
        actual
            .seek_to_first()
            .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
        for expected in reader.rows() {
            match actual.current() {
                Some(row) if expected.row() == row.row() => {}
                _ => return Err(materialization_replacement_mismatch()),
            }
            actual
                .advance()
                .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct MaterializedRowsSummary {
    rows: u64,
    fingerprint: u64,
    skipped_post_fork_rows: u64,
    skipped_exact_duplicate_rows: u64,
}

impl MaterializedRowsSummary {
    fn record_row(&mut self, row: &TableRow) {
        if self.rows == 0 {
            self.fingerprint = MATERIALIZATION_HASH_OFFSET;
        }
        hash_materialized_row(&mut self.fingerprint, row);
        self.rows = self.rows.saturating_add(1);
    }

    fn record_shadow_skip(&mut self) {
        self.skipped_exact_duplicate_rows = self.skipped_exact_duplicate_rows.saturating_add(1);
    }
}

#[derive(Debug, Default)]
struct MaterializationStreamStats {
    source_table_opens: Cell<u64>,
    rows_rewritten: Cell<u64>,
    rows_skipped_by_fork: Cell<u64>,
    rows_skipped_by_shadowing: Cell<u64>,
}

impl MaterializationStreamStats {
    fn record_source_table_open(&self) {
        self.source_table_opens
            .set(self.source_table_opens.get().saturating_add(1));
    }

    fn record_row_rewritten(&self) {
        self.rows_rewritten
            .set(self.rows_rewritten.get().saturating_add(1));
    }

    fn record_fork_skip(&self) {
        self.rows_skipped_by_fork
            .set(self.rows_skipped_by_fork.get().saturating_add(1));
    }

    fn record_shadow_skip(&self) {
        self.rows_skipped_by_shadowing
            .set(self.rows_skipped_by_shadowing.get().saturating_add(1));
    }

    fn source_table_opens(&self) -> u64 {
        self.source_table_opens.get()
    }

    fn rows_rewritten(&self) -> u64 {
        self.rows_rewritten.get()
    }

    fn rows_skipped_by_fork(&self) -> u64 {
        self.rows_skipped_by_fork.get()
    }

    fn rows_skipped_by_shadowing(&self) -> u64 {
        self.rows_skipped_by_shadowing.get()
    }
}

struct MaterializationTableSource<'a> {
    id: TableCompactionSourceId,
    table: &'a BranchOwnedTable,
    source_branch_id: BranchId,
    target_branch_id: BranchId,
    fork_version: CommitVersion,
    stats: &'a MaterializationStreamStats,
}

impl<'a> MaterializationTableSource<'a> {
    fn new(
        source_index: usize,
        table: &'a BranchOwnedTable,
        source_branch_id: BranchId,
        target_branch_id: BranchId,
        fork_version: CommitVersion,
        stats: &'a MaterializationStreamStats,
    ) -> BranchRuntimeResult<Self> {
        let id = TableCompactionSourceId::new(format!("materialization-source-{source_index}"))
            .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
        Ok(Self {
            id,
            table,
            source_branch_id,
            target_branch_id,
            fork_version,
            stats,
        })
    }
}

impl TableCompactionInput for MaterializationTableSource<'_> {
    fn id(&self) -> &TableCompactionSourceId {
        &self.id
    }

    fn open_cursor(&self) -> crate::table::TableRuntimeResult<Box<dyn TableCursor + '_>> {
        self.stats.record_source_table_open();
        Ok(Box::new(MaterializationRewriteCursor::new(
            Box::new(self.table.reader().cursor()),
            self.source_branch_id,
            self.target_branch_id,
            self.fork_version,
            self.stats,
        )))
    }

    fn requires_source_order_validation(&self) -> bool {
        false
    }
}

struct MaterializationRewriteCursor<'a> {
    inner: Box<dyn TableCursor + 'a>,
    source_branch_id: BranchId,
    target_branch_id: BranchId,
    fork_version: CommitVersion,
    stats: &'a MaterializationStreamStats,
    current: Option<TableRow>,
}

impl<'a> MaterializationRewriteCursor<'a> {
    fn new(
        inner: Box<dyn TableCursor + 'a>,
        source_branch_id: BranchId,
        target_branch_id: BranchId,
        fork_version: CommitVersion,
        stats: &'a MaterializationStreamStats,
    ) -> Self {
        Self {
            inner,
            source_branch_id,
            target_branch_id,
            fork_version,
            stats,
            current: None,
        }
    }

    fn position_at_next_rewritten_row(&mut self) -> crate::table::TableRuntimeResult<()> {
        self.current = None;
        while let Some(row) = self.inner.current() {
            if row.commit_version().as_u64() > self.fork_version.as_u64() {
                self.stats.record_fork_skip();
                self.inner.advance()?;
                continue;
            }
            let rewritten =
                rewrite_row_branch(row.row(), self.source_branch_id, self.target_branch_id)
                    .map_err(|_| TableRuntimeError::InvalidConfig {
                        field: "materialization_row_branch",
                        reason: "rewrite failed",
                    })?;
            self.stats.record_row_rewritten();
            self.current = Some(TableRow::new(rewritten));
            return Ok(());
        }
        Ok(())
    }
}

impl TableCursor for MaterializationRewriteCursor<'_> {
    fn seek_to_first(&mut self) -> crate::table::TableRuntimeResult<()> {
        self.inner.seek_to_first()?;
        self.position_at_next_rewritten_row()
    }

    fn seek(&mut self, target: &TableInternalKeyBytes) -> crate::table::TableRuntimeResult<()> {
        self.seek_to_first()?;
        while let Some(row) = &self.current {
            match row.key().cmp(target) {
                Ordering::Less => self.advance()?,
                Ordering::Equal | Ordering::Greater => return Ok(()),
            }
        }
        Ok(())
    }

    fn advance(&mut self) -> crate::table::TableRuntimeResult<()> {
        self.inner.advance()?;
        self.position_at_next_rewritten_row()
    }

    fn current(&self) -> Option<&TableRow> {
        self.current.as_ref()
    }
}

fn materialization_table_sources<'a>(
    tables: impl Iterator<Item = &'a BranchOwnedTable>,
    source_branch_id: BranchId,
    target_branch_id: BranchId,
    fork_version: CommitVersion,
    stats: &'a MaterializationStreamStats,
) -> BranchRuntimeResult<Vec<MaterializationTableSource<'a>>> {
    tables
        .enumerate()
        .map(|(source_index, table)| {
            MaterializationTableSource::new(
                source_index,
                table,
                source_branch_id,
                target_branch_id,
                fork_version,
                stats,
            )
        })
        .collect()
}

fn hash_materialized_row(hash: &mut u64, row: &TableRow) {
    hash_bytes(hash, row.encoded_key());
    hash_u64(hash, row.commit_timestamp().as_micros());
    hash_u64(hash, row.expires_at().as_micros());
    hash_bytes(hash, &[u8::from(row.is_tombstone())]);
    hash_bytes(hash, row.value());
}

fn hash_u64(hash: &mut u64, value: u64) {
    hash_bytes(hash, &value.to_le_bytes());
}

fn hash_bytes(hash: &mut u64, bytes: &[u8]) {
    hash_u64_raw(hash, bytes.len() as u64);
    for byte in bytes {
        *hash ^= u64::from(*byte);
        *hash = hash.wrapping_mul(MATERIALIZATION_HASH_PRIME);
    }
}

fn hash_u64_raw(hash: &mut u64, value: u64) {
    for byte in value.to_le_bytes() {
        *hash ^= u64::from(byte);
        *hash = hash.wrapping_mul(MATERIALIZATION_HASH_PRIME);
    }
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
