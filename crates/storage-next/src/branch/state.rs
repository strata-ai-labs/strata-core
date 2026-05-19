//! Branch-local state and descriptor shells.

use super::{
    read::{inherited_table_count, table_ranges_overlap},
    require_row_branch, rewrite_row_branch, BranchInheritedLayer, BranchLevel, BranchOwnedTable,
    BranchReachabilitySnapshot, BranchReadView, BranchRuntimeConfig, BranchRuntimeError,
    BranchRuntimeResult, BranchStateFacts, BranchTableDescriptor, BranchTableRef,
    InheritedLayerDescriptor, InheritedLayerStatus,
};
use crate::row::StorageRow;
use crate::table::{
    FrozenTable, ImmutableTableBuilder, ImmutableTableReader, MutableTable, TableBuilderConfig,
    TableIdentity, TableInternalKeyBytes, TableReaderConfig, TableRuntimeError,
};
use std::collections::{BTreeMap, BTreeSet};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

const MATERIALIZATION_ROWS_PER_OUTPUT_TABLE: usize = 4_096;
const _: () = assert!(MATERIALIZATION_ROWS_PER_OUTPUT_TABLE > 0);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchStateDescriptor {
    branch_id: BranchId,
    facts: BranchStateFacts,
}

impl BranchStateDescriptor {
    pub(crate) fn new(branch_id: BranchId, facts: BranchStateFacts) -> BranchRuntimeResult<Self> {
        if branch_id != facts.branch_id() {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "state descriptor branch id must match branch facts",
            });
        }
        Ok(Self { branch_id, facts })
    }

    pub(crate) const fn branch_id(self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn facts(self) -> BranchStateFacts {
        self.facts
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchViewDescriptor {
    branch_id: BranchId,
    facts: BranchStateFacts,
}

impl BranchViewDescriptor {
    pub(crate) fn new(branch_id: BranchId, facts: BranchStateFacts) -> BranchRuntimeResult<Self> {
        if branch_id != facts.branch_id() {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "view descriptor branch id must match branch facts",
            });
        }
        Ok(Self { branch_id, facts })
    }

    pub(crate) const fn branch_id(self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn facts(self) -> BranchStateFacts {
        self.facts
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchAppendOutcome {
    branch_id: BranchId,
    commit_version: CommitVersion,
    commit_timestamp: Timestamp,
    is_tombstone: bool,
    active_rows: usize,
    approximate_active_bytes: usize,
}

impl BranchAppendOutcome {
    pub(crate) const fn branch_id(self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn commit_version(self) -> CommitVersion {
        self.commit_version
    }

    pub(crate) const fn commit_timestamp(self) -> Timestamp {
        self.commit_timestamp
    }

    pub(crate) const fn is_tombstone(self) -> bool {
        self.is_tombstone
    }

    pub(crate) const fn active_rows(self) -> usize {
        self.active_rows
    }

    pub(crate) const fn approximate_active_bytes(self) -> usize {
        self.approximate_active_bytes
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BranchRotationSkipReason {
    EmptyActive,
    FrozenLimitReached,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BranchRotationOutcome {
    Rotated {
        frozen_index: usize,
        frozen_rows: usize,
        frozen_tables: usize,
    },
    Skipped {
        reason: BranchRotationSkipReason,
    },
}

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
pub(crate) struct BranchForkOutcome {
    source_branch_id: BranchId,
    destination_branch_id: BranchId,
    fork_version: CommitVersion,
    inherited_layer_count: usize,
    inherited_table_count: usize,
}

impl BranchForkOutcome {
    pub(crate) const fn source_branch_id(self) -> BranchId {
        self.source_branch_id
    }

    pub(crate) const fn destination_branch_id(self) -> BranchId {
        self.destination_branch_id
    }

    pub(crate) const fn fork_version(self) -> CommitVersion {
        self.fork_version
    }

    pub(crate) const fn inherited_layer_count(self) -> usize {
        self.inherited_layer_count
    }

    pub(crate) const fn inherited_table_count(self) -> usize {
        self.inherited_table_count
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BranchMaterializationRecovery {
    ReplacementVisibleLayerRemoved,
    LayerAlreadyMaterialized,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchMaterializationRequest {
    child_branch_id: BranchId,
    layer_index: usize,
    output_identity_prefix: String,
}

impl BranchMaterializationRequest {
    pub(crate) fn new(
        child_branch_id: BranchId,
        layer_index: usize,
        output_identity_prefix: impl Into<String>,
    ) -> BranchRuntimeResult<Self> {
        let output_identity_prefix = output_identity_prefix.into();
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
            output_identity_prefix,
        })
    }

    pub(crate) const fn child_branch_id(&self) -> BranchId {
        self.child_branch_id
    }

    pub(crate) const fn layer_index(&self) -> usize {
        self.layer_index
    }

    pub(crate) fn output_identity_prefix(&self) -> &str {
        &self.output_identity_prefix
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchMaterializationOutcome {
    child_branch_id: BranchId,
    source_branch_id: BranchId,
    fork_version: CommitVersion,
    layer_index: usize,
    rows_materialized: u64,
    tables_created: usize,
    skipped_post_fork_rows: u64,
    skipped_exact_duplicate_rows: u64,
    inherited_layers_remaining: usize,
    replacement_owned_table_count: usize,
    recovery: BranchMaterializationRecovery,
}

impl BranchMaterializationOutcome {
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

    pub(crate) const fn rows_materialized(self) -> u64 {
        self.rows_materialized
    }

    pub(crate) const fn tables_created(self) -> usize {
        self.tables_created
    }

    pub(crate) const fn skipped_post_fork_rows(self) -> u64 {
        self.skipped_post_fork_rows
    }

    pub(crate) const fn skipped_exact_duplicate_rows(self) -> u64 {
        self.skipped_exact_duplicate_rows
    }

    pub(crate) const fn inherited_layers_remaining(self) -> usize {
        self.inherited_layers_remaining
    }

    pub(crate) const fn replacement_owned_table_count(self) -> usize {
        self.replacement_owned_table_count
    }

    pub(crate) const fn recovery(self) -> BranchMaterializationRecovery {
        self.recovery
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
    put_rows: u64,
    tombstone_rows: u64,
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
            put_rows: 0,
            tombstone_rows: 0,
        })
    }

    pub(crate) fn empty(branch_id: BranchId) -> Self {
        Self::new(branch_id, BranchRuntimeConfig::default())
            .expect("default branch-local state configuration is valid")
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn config(&self) -> BranchRuntimeConfig {
        self.config
    }

    pub(crate) const fn active(&self) -> &MutableTable {
        &self.active
    }

    pub(crate) fn frozen(&self) -> &[FrozenTable] {
        &self.frozen
    }

    pub(crate) fn owned_levels(&self) -> &[Vec<BranchOwnedTable>] {
        &self.owned_levels
    }

    pub(crate) fn inherited_layers(&self) -> &[BranchInheritedLayer] {
        &self.inherited_layers
    }

    pub(crate) fn active_row_count(&self) -> usize {
        self.active.len()
    }

    pub(crate) fn frozen_table_count(&self) -> usize {
        self.frozen.len()
    }

    pub(crate) fn owned_table_count(&self) -> usize {
        self.owned_levels.iter().map(Vec::len).sum()
    }

    pub(crate) fn inherited_layer_count(&self) -> usize {
        self.inherited_layers.len()
    }

    pub(crate) fn inherited_table_count(&self) -> usize {
        inherited_table_count(&self.inherited_layers)
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.active.is_empty()
            && self.frozen.is_empty()
            && self.owned_table_count() == 0
            && self.inherited_layers.is_empty()
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

    pub(crate) const fn put_rows(&self) -> u64 {
        self.put_rows
    }

    pub(crate) const fn tombstone_rows(&self) -> u64 {
        self.tombstone_rows
    }

    pub(crate) fn facts(&self) -> BranchRuntimeResult<BranchStateFacts> {
        let observed = self.observe_rows();
        BranchStateFacts::new(
            self.branch_id,
            u64::try_from(self.active.len()).expect("active row count fits in u64"),
            self.frozen.len(),
            self.owned_table_count(),
            self.inherited_layers.len(),
            observed.max_commit_version,
            observed.timestamp_min,
            observed.timestamp_max,
        )
    }

    pub(crate) fn capture_read_view(&self) -> BranchRuntimeResult<BranchReadView> {
        BranchReadView::new_with_inherited(
            self.branch_id,
            self.active.clone(),
            self.frozen.clone(),
            self.owned_levels.clone(),
            self.inherited_layers.clone(),
            self.facts()?,
        )
    }

    pub(crate) fn reachability_snapshot(&self) -> BranchRuntimeResult<BranchReachabilitySnapshot> {
        let mut table_refs = Vec::new();
        for tables in &self.owned_levels {
            for (table_index, table) in tables.iter().enumerate() {
                let table_ref = if let Some(materialization_layer_index) =
                    table.materialization_layer_index()
                {
                    BranchTableRef::replacement(
                        self.branch_id,
                        materialization_layer_index,
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

    pub(crate) fn append_committed_row(
        &mut self,
        row: StorageRow,
    ) -> BranchRuntimeResult<BranchAppendOutcome> {
        let identity = require_row_branch(self.branch_id, &row)?;
        let key = TableInternalKeyBytes::from_row(&row);
        self.require_absent_internal_key(&key)?;

        let commit_version = identity.commit_version();
        let commit_timestamp = identity.commit_timestamp();
        let is_tombstone = row.is_tombstone();
        self.active
            .insert_row(row)
            .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
        self.track_committed_row(commit_version, commit_timestamp, is_tombstone);

        Ok(BranchAppendOutcome {
            branch_id: self.branch_id,
            commit_version,
            commit_timestamp,
            is_tombstone,
            active_rows: self.active.len(),
            approximate_active_bytes: self.active.approximate_size_bytes(),
        })
    }

    pub(crate) fn rotate_active(&mut self) -> BranchRotationOutcome {
        if self.active.is_empty() {
            return BranchRotationOutcome::Skipped {
                reason: BranchRotationSkipReason::EmptyActive,
            };
        }

        if self.frozen.len() >= self.config.max_frozen_tables() {
            return BranchRotationOutcome::Skipped {
                reason: BranchRotationSkipReason::FrozenLimitReached,
            };
        }

        let active = std::mem::replace(&mut self.active, MutableTable::new());
        let frozen_rows = active.len();
        self.frozen.insert(0, active.freeze());
        BranchRotationOutcome::Rotated {
            frozen_index: 0,
            frozen_rows,
            frozen_tables: self.frozen.len(),
        }
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
            insert_sorted_by_range(&mut self.owned_levels[level_index], table)
        };
        self.refresh_observed_row_facts();
        Ok(self.install_outcome(level, table_index, None))
    }

    pub(crate) fn replace_frozen_with_l0_table(
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

    pub(crate) fn attach_inherited_layers(
        &mut self,
        layers: Vec<BranchInheritedLayer>,
    ) -> BranchRuntimeResult<BranchForkOutcome> {
        self.validate_inherited_attach(&layers)?;
        let inherited_layer_count = layers.len();
        let inherited_table_count = inherited_table_count(&layers);
        self.inherited_layers = layers;
        self.refresh_observed_row_facts();
        Ok(BranchForkOutcome {
            source_branch_id: self
                .inherited_layers
                .first()
                .map_or(self.branch_id, BranchInheritedLayer::source_branch_id),
            destination_branch_id: self.branch_id,
            fork_version: self.max_commit_version.unwrap_or(CommitVersion::ZERO),
            inherited_layer_count,
            inherited_table_count,
        })
    }

    pub(crate) fn fork_into_empty_child(
        &self,
        destination_branch_id: BranchId,
    ) -> BranchRuntimeResult<(Self, BranchForkOutcome)> {
        if destination_branch_id == self.branch_id {
            return Err(BranchRuntimeError::InvalidInheritedLayer {
                reason: "fork source and destination branches must differ",
            });
        }

        let fork_version = self.max_commit_version.unwrap_or(CommitVersion::ZERO);
        let mut layers = Vec::with_capacity(self.inherited_layers.len() + 1);
        layers.push(BranchInheritedLayer::new(
            InheritedLayerDescriptor::new(
                self.branch_id,
                fork_version,
                InheritedLayerStatus::Active,
                self.owned_table_count(),
            ),
            self.owned_levels.clone(),
        )?);
        for layer in &self.inherited_layers {
            if let Some(layer) = layer.clone_active_for_fork()? {
                layers.push(layer);
            }
        }

        let mut child = Self::new(destination_branch_id, self.config)?;
        let attach_outcome = child.attach_inherited_layers(layers)?;
        let outcome = BranchForkOutcome {
            source_branch_id: self.branch_id,
            destination_branch_id,
            fork_version,
            inherited_layer_count: attach_outcome.inherited_layer_count(),
            inherited_table_count: attach_outcome.inherited_table_count(),
        };
        Ok((child, outcome))
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
        let layer = self.inherited_layers.get(request.layer_index()).ok_or(
            BranchRuntimeError::InvalidInheritedLayer {
                reason: "materialization layer index must exist",
            },
        )?;
        let source_branch_id = layer.source_branch_id();
        let fork_version = layer.fork_version();

        match layer.status() {
            InheritedLayerStatus::Active | InheritedLayerStatus::Materializing => {}
            InheritedLayerStatus::Materialized => {
                return Ok(BranchMaterializationOutcome {
                    child_branch_id: self.branch_id,
                    source_branch_id,
                    fork_version,
                    layer_index: request.layer_index(),
                    rows_materialized: 0,
                    tables_created: 0,
                    skipped_post_fork_rows: 0,
                    skipped_exact_duplicate_rows: 0,
                    inherited_layers_remaining: self.inherited_layers.len(),
                    replacement_owned_table_count: 0,
                    recovery: BranchMaterializationRecovery::LayerAlreadyMaterialized,
                });
            }
            InheritedLayerStatus::Unavailable => {
                return Err(BranchRuntimeError::InvalidInheritedLayer {
                    reason: "unavailable inherited layers cannot be materialized",
                });
            }
        }

        let materialized = self.collect_materialization_rows(request.layer_index())?;
        let replacement_tables = self.build_materialized_l0_tables(
            request.layer_index(),
            request.output_identity_prefix(),
            &materialized.rows,
        )?;
        let rows_materialized = u64::try_from(materialized.rows.len()).map_err(|_| {
            BranchRuntimeError::InvalidBranchState {
                reason: "materialized row count must fit in u64",
            }
        })?;
        let tables_created = replacement_tables.len();

        let level_index = usize::from(BranchLevel::ZERO.raw());
        for table in replacement_tables.into_iter().rev() {
            self.owned_levels[level_index].insert(0, table);
        }
        self.inherited_layers.remove(request.layer_index());
        self.refresh_observed_row_facts();

        Ok(BranchMaterializationOutcome {
            child_branch_id: self.branch_id,
            source_branch_id,
            fork_version,
            layer_index: request.layer_index(),
            rows_materialized,
            tables_created,
            skipped_post_fork_rows: materialized.skipped_post_fork_rows,
            skipped_exact_duplicate_rows: materialized.skipped_exact_duplicate_rows,
            inherited_layers_remaining: self.inherited_layers.len(),
            replacement_owned_table_count: tables_created,
            recovery: BranchMaterializationRecovery::ReplacementVisibleLayerRemoved,
        })
    }

    fn require_absent_internal_key(&self, key: &TableInternalKeyBytes) -> BranchRuntimeResult<()> {
        self.require_absent_internal_key_except_frozen(key, None)
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
                if higher_precedence_rows
                    .get(&key)
                    .is_some_and(|rows| rows.iter().any(|existing| existing == &rewritten))
                {
                    skipped_exact_duplicate_rows = skipped_exact_duplicate_rows.saturating_add(1);
                    continue;
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

    fn build_materialized_l0_tables(
        &self,
        layer_index: usize,
        output_identity_prefix: &str,
        rows: &[StorageRow],
    ) -> BranchRuntimeResult<Vec<BranchOwnedTable>> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }

        let builder = ImmutableTableBuilder::new(TableBuilderConfig::default())
            .map_err(|source| BranchRuntimeError::TableRuntime { source })?;
        let mut tables = Vec::new();
        for (output_index, chunk) in rows
            .chunks(MATERIALIZATION_ROWS_PER_OUTPUT_TABLE)
            .enumerate()
        {
            let identity =
                materialized_table_identity(output_identity_prefix, layer_index, output_index)?;
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
            tables.push(BranchOwnedTable::new_materialization_replacement(
                self.branch_id,
                descriptor,
                reader,
                layer_index,
            )?);
        }
        Ok(tables)
    }

    fn validate_inherited_attach(
        &self,
        layers: &[BranchInheritedLayer],
    ) -> BranchRuntimeResult<()> {
        if !self.inherited_layers.is_empty() {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "branch already has inherited layers",
            });
        }
        if !self.active.is_empty() || !self.frozen.is_empty() || self.owned_table_count() != 0 {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "inherited layers can only attach to an empty own branch state",
            });
        }
        if layers.len() > self.config.max_inherited_layers() {
            return Err(BranchRuntimeError::InvalidInheritedLayer {
                reason: "inherited layer count exceeds branch runtime configuration",
            });
        }
        for layer in layers {
            if layer.source_branch_id() == self.branch_id {
                return Err(BranchRuntimeError::InvalidInheritedLayer {
                    reason: "inherited layer source branch must differ from child branch",
                });
            }
            if layer.status() == InheritedLayerStatus::Unavailable {
                return Err(BranchRuntimeError::InvalidInheritedLayer {
                    reason: "unavailable inherited layers cannot attach",
                });
            }
        }
        Ok(())
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
        let range = table.facts().key_range();
        if self.owned_levels[level_index]
            .iter()
            .any(|existing| table_ranges_overlap(existing.facts().key_range(), range))
        {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "branch-owned nonzero level tables must not overlap",
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

    fn observe_rows(&self) -> ObservedBranchRows {
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
        for layer in &self.inherited_layers {
            observed.record_inherited_layer(layer);
        }
        observed
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MaterializedRows {
    rows: Vec<StorageRow>,
    skipped_post_fork_rows: u64,
    skipped_exact_duplicate_rows: u64,
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
        self.record_commit_version(layer.fork_version());
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

fn insert_sorted_by_range(tables: &mut Vec<BranchOwnedTable>, table: BranchOwnedTable) -> usize {
    let first_key = table.facts().key_range().first_key();
    let index =
        tables.partition_point(|existing| existing.facts().key_range().first_key() < first_key);
    tables.insert(index, table);
    index
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
