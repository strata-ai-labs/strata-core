//! Branch table-manifest publication and recovery helpers.

use super::{
    require_manifest_catalog_budget, require_table_reader_budget, telemetry_health_debt,
    LifecycleError, LifecycleLowerLayer, LifecycleResult, MaintenanceOutcome,
    MaintenanceOutcomeStatus, StorageBudgetLedger,
};
use crate::backend::{BackendErrorKind, PublishError, PublishFailureKind};
use crate::branch::facts::{
    BranchLevel, BranchTableDescriptor, InheritedLayerDescriptor, InheritedLayerStatus,
};
use crate::branch::read::{BranchInheritedLayer, BranchMaterializationSource, BranchOwnedTable};
use crate::branch::state::manifest_recovery::{
    BranchTableManifestRecoveryOutcome, BranchTableManifestRecoveryRequest,
};
use crate::branch::state::BranchLocalState;
use crate::format::{
    encode_table_manifest, FormatError, TableManifest, TableManifestInheritedLayer,
    TableManifestInheritedLayerStatus, TableManifestLevel, TableManifestTableBounds,
    TableManifestTableFacts, TableManifestTableProvenance, TableManifestTableRef,
};
use crate::object::ObjectName;
use crate::service::{
    ManifestRole, ManifestServiceError, TableManifestService, TableManifestWrite, TableObjectFacts,
    TableObjectReadError, TableObjectReaderService,
};
use crate::table::{
    ImmutableTableReader, TableIdentity, TablePhysicalKeyBytes, TableReaderConfig, TableRow,
    TableRuntimeFacts,
};
use std::collections::BTreeMap;
use strata_core_next::{BranchId, Timestamp};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleDurableTableCatalog {
    entries: BTreeMap<String, LifecycleDurableTableCatalogEntry>,
    objects: BTreeMap<String, String>,
    next_manifest_sequence: u64,
    // Set when a table is installed in-memory (`record_table`) and cleared when a manifest is
    // durably recorded (`record_manifest` / `record_reserved_manifest`). A manifest publish
    // that fails leaves this set: the in-memory branch state then owns L0 rows the durable
    // manifest does not cover, so a checkpoint must defer rather than advance the WAL-replay
    // floor past them. In-memory bookkeeping only — recovery rebuilds the catalog via
    // `record_manifest`, which clears it, so a reopen always starts settled (`false`).
    //
    // Catalog-global, not per-branch — correct for a single flushing branch (the regime the
    // fault soak exercises). Multiple branches can flush (the maintenance coverage scan in
    // `durable/maintenance.rs` enqueues flush per-branch under pressure), so branch B's
    // `record_manifest` could clear the flag branch A's install set, masking branch A's debt from
    // the checkpoint defer. The multi-branch checkpoint guard covers this in practice — a checkpoint
    // also defers while any non-seeded branch holds a durable base, and a branch carrying publish
    // debt owns exactly such a base, so the masked-debt snapshot is not recorded. A per-branch debt
    // set is the deferred precise fix; see multi-branch-orphaned-delta-recovery-gap.md.
    manifest_publish_pending: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleDurableTableCatalogEntry {
    identity: TableIdentity,
    object_facts: TableObjectFacts,
    provenance: TableManifestTableProvenance,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleTableManifestRecoveryOutcome {
    manifest_object: Option<ObjectName>,
    manifest_sequence: Option<u64>,
    table_count: usize,
    install_outcome: Option<BranchTableManifestRecoveryOutcome>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleTableManifestRecoveryStage {
    branch: BranchLocalState,
    catalog: LifecycleDurableTableCatalog,
    outcome: LifecycleTableManifestRecoveryOutcome,
}

impl LifecycleDurableTableCatalog {
    pub(crate) fn new() -> Self {
        Self {
            entries: BTreeMap::new(),
            objects: BTreeMap::new(),
            next_manifest_sequence: 1,
            manifest_publish_pending: false,
        }
    }

    pub(crate) fn record_table(
        &mut self,
        identity: TableIdentity,
        object_facts: TableObjectFacts,
    ) -> LifecycleResult<()> {
        self.record_table_with_provenance(
            identity,
            object_facts,
            TableManifestTableProvenance::Flush,
        )
    }

    pub(crate) fn record_table_with_provenance(
        &mut self,
        identity: TableIdentity,
        object_facts: TableObjectFacts,
        provenance: TableManifestTableProvenance,
    ) -> LifecycleResult<()> {
        let key = identity.as_str().to_owned();
        let object_key = object_facts.object().as_str().to_owned();
        let entry = LifecycleDurableTableCatalogEntry {
            identity,
            object_facts,
            provenance,
        };
        if let Some(existing) = self.entries.get(&key) {
            if existing == &entry {
                return Ok(());
            }
            return Err(LifecycleError::TableManifestPublicationFailed {
                reason: "table identity has conflicting durable object facts",
                source: None,
            });
        }
        if self
            .objects
            .get(&object_key)
            .is_some_and(|existing_identity| existing_identity != &key)
        {
            return Err(LifecycleError::TableManifestPublicationFailed {
                reason: "table object has conflicting durable table identity",
                source: None,
            });
        }
        self.objects.insert(object_key, key.clone());
        self.entries.insert(key, entry);
        // A new table is installed in-memory; the durable manifest no longer covers every
        // owned L0 row until the next successful manifest publish records it.
        self.manifest_publish_pending = true;
        Ok(())
    }

    pub(crate) fn record_manifest(&mut self, manifest: &TableManifest) -> LifecycleResult<()> {
        if manifest.manifest_sequence() < self.next_manifest_sequence {
            return Err(LifecycleError::TableManifestPublicationFailed {
                reason: "table manifest sequence regresses below catalog state",
                source: None,
            });
        }
        self.record_manifest_tables(manifest)?;
        self.next_manifest_sequence = manifest.manifest_sequence().checked_add(1).ok_or(
            LifecycleError::TableManifestPublicationFailed {
                reason: "table manifest sequence overflow",
                source: None,
            },
        )?;
        // The manifest is durably recorded: the catalog and the durable manifest agree again.
        self.manifest_publish_pending = false;
        Ok(())
    }

    /// Records a manifest whose sequence was already reserved under the runtime lock via
    /// [`reserve_manifest_sequence`](Self::reserve_manifest_sequence) (the off-lock publish
    /// path). Only the manifest's tables are folded into the catalog; the sequence marker was
    /// advanced at reservation time, so it is not advanced again here.
    pub(crate) fn record_reserved_manifest(
        &mut self,
        manifest: &TableManifest,
    ) -> LifecycleResult<()> {
        self.record_manifest_tables(manifest)?;
        // The off-lock publish is confirmed durable: clear the pending-manifest debt.
        self.manifest_publish_pending = false;
        Ok(())
    }

    fn record_manifest_tables(&mut self, manifest: &TableManifest) -> LifecycleResult<()> {
        for table in manifest_table_refs(manifest) {
            self.record_table_with_provenance(
                table.table_identity().clone(),
                TableObjectFacts::from_table_manifest_ref(table),
                table.provenance().clone(),
            )?;
        }
        Ok(())
    }

    /// Reserves the next manifest sequence under the runtime lock. The caller stamps the
    /// reserved value into the manifest it persists off-lock; advancing the counter here (rather
    /// than at record time) keeps cross-branch sequences unique and monotonic when the durable
    /// write runs off-lock under a per-branch publish lock.
    pub(crate) fn reserve_manifest_sequence(&mut self) -> LifecycleResult<u64> {
        let reserved = self.next_manifest_sequence;
        self.next_manifest_sequence =
            reserved
                .checked_add(1)
                .ok_or(LifecycleError::TableManifestPublicationFailed {
                    reason: "table manifest sequence overflow",
                    source: None,
                })?;
        Ok(reserved)
    }

    pub(crate) fn build_manifest(
        &self,
        branch: &BranchLocalState,
    ) -> LifecycleResult<TableManifest> {
        self.build_manifest_with_sequence(branch, self.next_manifest_sequence)
    }

    pub(crate) fn build_manifest_with_sequence(
        &self,
        branch: &BranchLocalState,
        manifest_sequence: u64,
    ) -> LifecycleResult<TableManifest> {
        let levels = manifest_levels_from_owned(branch.owned_levels(), self)?;
        let inherited_layers = manifest_inherited_layers(branch.inherited_layers(), self)?;
        let mut extensions = Vec::new();
        if let Some(facts) =
            super::retained_history_extension::RetainedHistoryFacts::from_timestamp_coverage(
                branch.timestamp_coverage(),
                branch
                    .max_commit_version()
                    .unwrap_or(strata_core_next::CommitVersion::ZERO),
            )
        {
            extensions.push(facts.to_extension_section().map_err(format_error)?);
        }
        TableManifest::new(
            branch.branch_id(),
            None,
            manifest_sequence,
            levels,
            inherited_layers,
            extensions,
        )
        .map_err(format_error)
    }

    #[allow(
        dead_code,
        reason = "manifest sequence is asserted by table-manifest tests"
    )]
    pub(crate) const fn next_manifest_sequence(&self) -> u64 {
        self.next_manifest_sequence
    }

    pub(crate) fn entry_count(&self) -> usize {
        self.entries.len()
    }

    /// Whether a table has been installed in-memory without a subsequent successful manifest
    /// publish — i.e. the durable manifest does not yet cover every owned L0 row. A checkpoint
    /// must defer while this holds so it cannot advance the WAL-replay floor past rows a clean
    /// reopen could not recover.
    pub(crate) const fn has_outstanding_manifest_debt(&self) -> bool {
        self.manifest_publish_pending
    }

    pub(crate) fn object_count(&self) -> usize {
        self.objects.len()
    }

    fn entry_for(
        &self,
        identity: &TableIdentity,
    ) -> LifecycleResult<&LifecycleDurableTableCatalogEntry> {
        self.entries
            .get(identity.as_str())
            .ok_or(LifecycleError::TableManifestPublicationFailed {
            reason:
                "branch table state contains volatile rewrite output without durable catalog entry",
            source: None,
        })
    }
}

impl Default for LifecycleDurableTableCatalog {
    fn default() -> Self {
        Self::new()
    }
}

impl LifecycleTableManifestRecoveryOutcome {
    pub(crate) const fn absent() -> Self {
        Self {
            manifest_object: None,
            manifest_sequence: None,
            table_count: 0,
            install_outcome: None,
        }
    }

    fn installed(
        manifest_object: ObjectName,
        manifest: &TableManifest,
        install_outcome: BranchTableManifestRecoveryOutcome,
    ) -> Self {
        Self {
            manifest_object: Some(manifest_object),
            manifest_sequence: Some(manifest.manifest_sequence()),
            table_count: install_outcome.total_table_count(),
            install_outcome: Some(install_outcome),
        }
    }

    #[allow(
        dead_code,
        reason = "table-manifest recovery facts are consumed by lifecycle tests"
    )]
    pub(crate) const fn manifest_object(&self) -> Option<&ObjectName> {
        self.manifest_object.as_ref()
    }

    #[allow(
        dead_code,
        reason = "table-manifest recovery facts are consumed by lifecycle tests"
    )]
    pub(crate) const fn manifest_sequence(&self) -> Option<u64> {
        self.manifest_sequence
    }

    #[allow(
        dead_code,
        reason = "table-manifest recovery facts are consumed by lifecycle tests"
    )]
    pub(crate) const fn table_count(&self) -> usize {
        self.table_count
    }

    #[allow(
        dead_code,
        reason = "table-manifest recovery facts are consumed by lifecycle tests"
    )]
    pub(crate) const fn install_outcome(&self) -> Option<&BranchTableManifestRecoveryOutcome> {
        self.install_outcome.as_ref()
    }
}

impl LifecycleTableManifestRecoveryStage {
    fn new(
        branch: BranchLocalState,
        catalog: LifecycleDurableTableCatalog,
        outcome: LifecycleTableManifestRecoveryOutcome,
    ) -> Self {
        Self {
            branch,
            catalog,
            outcome,
        }
    }

    pub(crate) const fn outcome(&self) -> &LifecycleTableManifestRecoveryOutcome {
        &self.outcome
    }

    pub(crate) const fn staged_branch(&self) -> &BranchLocalState {
        &self.branch
    }

    pub(crate) fn into_parts(
        self,
    ) -> (
        BranchLocalState,
        LifecycleDurableTableCatalog,
        LifecycleTableManifestRecoveryOutcome,
    ) {
        (self.branch, self.catalog, self.outcome)
    }
}

pub(crate) fn preflight_table_manifest_with_checkpoint(
    checkpoint_branch: &BranchLocalState,
    staged_branch: &BranchLocalState,
) -> LifecycleResult<bool> {
    let mut checkpoint_rows = BTreeMap::<&[u8], &TableRow>::new();
    for table in checkpoint_branch.owned_levels().iter().flatten() {
        for row in table.rows() {
            checkpoint_rows.insert(row.key().as_slice(), row);
        }
    }
    let mut any_overlap = false;
    for table in staged_branch.owned_levels().iter().flatten() {
        for row in table.rows() {
            if let Some(checkpoint_row) = checkpoint_rows.get(row.key().as_slice()) {
                if checkpoint_row.row() != row.row() {
                    return Err(LifecycleError::table_manifest_checkpoint_conflict(
                        "manifest row bytes diverge from checkpoint at internal key",
                    ));
                }
                any_overlap = true;
            }
        }
    }
    Ok(any_overlap)
}

#[allow(
    dead_code,
    reason = "budget-free wrapper preserves the table-manifest helper surface for direct tests"
)]
pub(crate) fn publish_table_manifest_for_branch(
    branch: &BranchLocalState,
    service: &TableManifestService<'_>,
    catalog: &mut LifecycleDurableTableCatalog,
) -> LifecycleResult<TableManifestWrite> {
    publish_table_manifest_for_branch_with_budget(branch, service, catalog, None)
}

pub(crate) fn publish_table_manifest_for_branch_with_budget(
    branch: &BranchLocalState,
    service: &TableManifestService<'_>,
    catalog: &mut LifecycleDurableTableCatalog,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<TableManifestWrite> {
    let manifest = catalog.build_manifest(branch)?;
    let write = persist_reserved_manifest(service, branch.branch_id(), &manifest, budget)?;
    catalog.record_manifest(write.manifest())?;
    Ok(write)
}

/// Persists an already-built (and, on the off-lock path, sequence-reserved) table manifest:
/// the budget check + encode + durable `publish_replace_manifest` (the fsync). This is the only
/// step the off-lock publish phase runs while the global runtime lock is released; it touches no
/// catalog state, so it is safe to call between the under-lock reserve and record phases.
pub(crate) fn persist_reserved_manifest(
    service: &TableManifestService<'_>,
    branch_id: BranchId,
    manifest: &TableManifest,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<TableManifestWrite> {
    if let Some(budget) = budget {
        let bytes = encode_table_manifest(manifest).map_err(format_error)?;
        require_manifest_catalog_budget(
            budget,
            bytes.len() as u64,
            1,
            "table manifest exceeds manifest catalog budget",
        )?;
    }
    let persist_start = crate::observability::perf_trace::start_timer();
    let write_result = service.publish_replace_manifest(branch_id, manifest);
    crate::observability::perf_trace::record_lifecycle_background_publish_manifest_persist(
        crate::observability::perf_trace::timer_elapsed(persist_start),
    );
    write_result.map_err(table_manifest_service_error)
}

pub(crate) fn stage_table_manifest_for_branch(
    branch: &BranchLocalState,
    service: &TableManifestService<'_>,
    reader_service: &TableObjectReaderService<'_>,
    catalog: &LifecycleDurableTableCatalog,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<LifecycleTableManifestRecoveryStage> {
    let mut staged_branch = branch.clone();
    let mut staged_catalog = catalog.clone();
    let outcome = recover_table_manifest_for_branch(
        &mut staged_branch,
        service,
        reader_service,
        &mut staged_catalog,
        budget,
    )?;
    Ok(LifecycleTableManifestRecoveryStage::new(
        staged_branch,
        staged_catalog,
        outcome,
    ))
}

pub(crate) fn table_manifest_debt_outcome(
    mut outcome: MaintenanceOutcome,
    error: LifecycleError,
) -> MaintenanceOutcome {
    outcome = outcome
        .with_status(MaintenanceOutcomeStatus::Completed)
        .with_reason("table manifest publication needs recovery")
        .with_source_error(error);
    if let Ok(health) = telemetry_health_debt("table manifest publication needs recovery") {
        outcome = outcome.with_recovery_health(health);
    }
    outcome
}

pub(crate) fn recover_table_manifest_for_branch(
    branch: &mut BranchLocalState,
    service: &TableManifestService<'_>,
    reader_service: &TableObjectReaderService<'_>,
    catalog: &mut LifecycleDurableTableCatalog,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<LifecycleTableManifestRecoveryOutcome> {
    let Some(manifest) = service
        .load_current(branch.branch_id())
        .map_err(table_manifest_service_error)?
    else {
        return Ok(LifecycleTableManifestRecoveryOutcome::absent());
    };
    apply_loaded_table_manifest_to_branch(branch, &manifest, reader_service, catalog, budget)
}

pub(crate) fn apply_loaded_table_manifest_to_branch(
    branch: &mut BranchLocalState,
    manifest: &TableManifest,
    reader_service: &TableObjectReaderService<'_>,
    catalog: &mut LifecycleDurableTableCatalog,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<LifecycleTableManifestRecoveryOutcome> {
    if manifest.branch_id() != branch.branch_id() {
        return Err(LifecycleError::RecoveryFailed {
            reason: "table manifest branch id does not match branch state",
        });
    }
    let manifest_object =
        crate::layout::ObjectLayout::branch_table_manifest(&branch.branch_id().to_string())
            .map_err(|source| {
                LifecycleError::lower_layer_with(
                    LifecycleLowerLayer::Layout,
                    "table manifest object layout failed",
                    source,
                )
            })?;
    let request = recovery_request_from_manifest(manifest, reader_service, catalog, budget)?;
    let install_outcome = branch
        .install_table_manifest_recovery(request)
        .map_err(branch_error)?;
    catalog.record_manifest(manifest)?;
    Ok(LifecycleTableManifestRecoveryOutcome::installed(
        manifest_object,
        manifest,
        install_outcome,
    ))
}

fn recovery_request_from_manifest(
    manifest: &TableManifest,
    reader_service: &TableObjectReaderService<'_>,
    catalog: &mut LifecycleDurableTableCatalog,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<BranchTableManifestRecoveryRequest> {
    let owned_levels = recover_manifest_levels(
        manifest.branch_id(),
        manifest.levels(),
        reader_service,
        catalog,
        budget,
    )?;
    let inherited_layers = manifest
        .inherited_layers()
        .iter()
        .map(|layer| recover_manifest_inherited_layer(layer, reader_service, catalog, budget))
        .collect::<LifecycleResult<Vec<_>>>()?;
    let request = BranchTableManifestRecoveryRequest::new(
        manifest.branch_id(),
        owned_levels,
        inherited_layers,
    )
    .map_err(branch_error)?;
    let retained =
        super::retained_history_extension::RetainedHistoryFacts::from_extension_sections(
            manifest.extension_sections(),
        )
        .map_err(format_error)?;
    Ok(match retained {
        Some(facts) => request.with_timestamp_coverage(facts.to_timestamp_coverage()),
        None => request,
    })
}

fn recover_manifest_inherited_layer(
    layer: &TableManifestInheritedLayer,
    reader_service: &TableObjectReaderService<'_>,
    catalog: &mut LifecycleDurableTableCatalog,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<BranchInheritedLayer> {
    let owned_levels = recover_manifest_levels(
        layer.source_branch_id(),
        layer.levels(),
        reader_service,
        catalog,
        budget,
    )?;
    let table_count = owned_levels.iter().map(Vec::len).sum();
    BranchInheritedLayer::new(
        InheritedLayerDescriptor::new(
            layer.source_branch_id(),
            layer.fork_version(),
            inherited_status_from_manifest(layer.status()),
            table_count,
        ),
        owned_levels,
    )
    .map_err(branch_error)
}

fn recover_manifest_levels(
    branch_id: BranchId,
    levels: &[TableManifestLevel],
    reader_service: &TableObjectReaderService<'_>,
    catalog: &mut LifecycleDurableTableCatalog,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<Vec<Vec<BranchOwnedTable>>> {
    let mut owned_levels = Vec::<Vec<BranchOwnedTable>>::new();
    for level in levels {
        let level_index = usize::from(level.level().raw());
        if owned_levels.len() <= level_index {
            owned_levels.resize_with(level_index + 1, Vec::new);
        }
        let tables = level
            .tables()
            .iter()
            .map(|table| {
                recover_manifest_table(
                    branch_id,
                    level.level(),
                    table,
                    reader_service,
                    catalog,
                    budget,
                )
            })
            .collect::<LifecycleResult<Vec<_>>>()?;
        owned_levels[level_index] = tables;
    }
    Ok(owned_levels)
}

fn recover_manifest_table(
    branch_id: BranchId,
    level: BranchLevel,
    table: &TableManifestTableRef,
    reader_service: &TableObjectReaderService<'_>,
    catalog: &mut LifecycleDurableTableCatalog,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<BranchOwnedTable> {
    let object_facts = TableObjectFacts::from_table_manifest_ref(table);
    if let Some(budget) = budget {
        require_table_reader_budget(
            budget,
            manifest_reader_materialized_budget_bytes(&object_facts),
            "table manifest recovery reader exceeds storage budget",
        )?;
    }
    let reader = reader_service
        .open_reader(
            table.table_identity().clone(),
            &object_facts,
            TableReaderConfig::default(),
        )
        .map_err(table_read_error)?;
    validate_manifest_reader_facts(table, &reader)?;
    catalog.record_table_with_provenance(
        table.table_identity().clone(),
        object_facts,
        table.provenance().clone(),
    )?;
    branch_table_from_reader(branch_id, level, table.provenance(), reader)
}

fn manifest_reader_materialized_budget_bytes(object_facts: &TableObjectFacts) -> u64 {
    object_facts.byte_count()
}

fn branch_table_from_reader(
    branch_id: BranchId,
    level: BranchLevel,
    provenance: &TableManifestTableProvenance,
    reader: ImmutableTableReader<'_>,
) -> LifecycleResult<BranchOwnedTable> {
    let identity = reader.facts().identity().clone();
    let descriptor = BranchTableDescriptor::new(identity, reader.facts().clone(), level)
        .map_err(branch_error)?;
    match provenance {
        TableManifestTableProvenance::MaterializationReplacement {
            source_branch_id,
            fork_version,
        } => BranchOwnedTable::new_materialization_replacement(
            branch_id,
            descriptor,
            reader,
            BranchMaterializationSource::new(*source_branch_id, *fork_version),
        )
        .map_err(branch_error),
        TableManifestTableProvenance::Flush
        | TableManifestTableProvenance::SnapshotInstall
        | TableManifestTableProvenance::Compaction
        | TableManifestTableProvenance::Recovered => {
            BranchOwnedTable::new(branch_id, descriptor, reader).map_err(branch_error)
        }
    }
}

fn manifest_levels_from_owned(
    owned_levels: &[Vec<BranchOwnedTable>],
    catalog: &LifecycleDurableTableCatalog,
) -> LifecycleResult<Vec<TableManifestLevel>> {
    let mut levels = Vec::new();
    for (level_index, tables) in owned_levels.iter().enumerate() {
        if tables.is_empty() {
            continue;
        }
        let level = BranchLevel::new(u8::try_from(level_index).map_err(|_| {
            LifecycleError::TableManifestPublicationFailed {
                reason: "branch level index does not fit table manifest level",
                source: None,
            }
        })?);
        let manifest_tables = tables
            .iter()
            .enumerate()
            .map(|(order, table)| table_ref_from_branch_table(table, order, catalog))
            .collect::<LifecycleResult<Vec<_>>>()?;
        levels.push(TableManifestLevel::new(level, manifest_tables).map_err(format_error)?);
    }
    Ok(levels)
}

fn manifest_inherited_layers(
    inherited_layers: &[BranchInheritedLayer],
    catalog: &LifecycleDurableTableCatalog,
) -> LifecycleResult<Vec<TableManifestInheritedLayer>> {
    inherited_layers
        .iter()
        .enumerate()
        .map(|(order, layer)| {
            let levels = manifest_levels_from_owned(layer.owned_levels(), catalog)?;
            TableManifestInheritedLayer::new(
                u32::try_from(order).map_err(|_| {
                    LifecycleError::TableManifestPublicationFailed {
                        reason: "inherited layer order does not fit table manifest",
                        source: None,
                    }
                })?,
                layer.source_branch_id(),
                None,
                layer.fork_version(),
                manifest_status_from_inherited(layer.status()),
                levels,
            )
            .map_err(format_error)
        })
        .collect()
}

fn table_ref_from_branch_table(
    table: &BranchOwnedTable,
    order: usize,
    catalog: &LifecycleDurableTableCatalog,
) -> LifecycleResult<TableManifestTableRef> {
    let entry = catalog.entry_for(table.descriptor().identity())?;
    validate_catalog_entry_matches_table(entry, table.facts())?;
    let facts = table.facts();
    let extras = table.extras();
    // Read the per-table summary cached at seal time instead of rescanning the
    // table's rows. Internal-key bounds come from the table facts' key range;
    // timestamp and physical-key bounds come from the cached summary. The values
    // are identical to a full row scan (recovery validation cross-checks them),
    // so the manifest bytes are unchanged.
    let manifest_facts = TableManifestTableFacts::new(
        facts.byte_count(),
        facts.row_count(),
        facts.data_block_count(),
        facts.commit_range().min(),
        facts.commit_range().max(),
        extras.timestamp_min(),
        extras.timestamp_max(),
    )
    .map_err(format_error)?;
    let manifest_bounds = TableManifestTableBounds::new(
        extras.physical_key_min().to_vec(),
        extras.physical_key_max().to_vec(),
        facts.key_range().first_key().to_vec(),
        facts.key_range().last_key().to_vec(),
    )
    .map_err(format_error)?;
    TableManifestTableRef::new(
        table.descriptor().identity().clone(),
        entry.object_facts.object().clone(),
        u32::try_from(order).map_err(|_| LifecycleError::TableManifestPublicationFailed {
            reason: "table order does not fit table manifest",
            source: None,
        })?,
        manifest_facts,
        manifest_bounds,
        manifest_table_provenance(table, entry)?,
    )
    .map_err(format_error)
}

fn validate_catalog_entry_matches_table(
    entry: &LifecycleDurableTableCatalogEntry,
    facts: &TableRuntimeFacts,
) -> LifecycleResult<()> {
    if entry.identity != *facts.identity()
        || entry.object_facts.byte_count() != facts.byte_count()
        || entry.object_facts.row_count() != facts.row_count()
        || entry.object_facts.data_block_count() != facts.data_block_count()
        || entry.object_facts.commit_min() != facts.commit_range().min()
        || entry.object_facts.commit_max() != facts.commit_range().max()
    {
        return Err(LifecycleError::TableManifestPublicationFailed {
            reason: "catalog table object facts do not match reachable table",
            source: None,
        });
    }
    Ok(())
}

fn manifest_table_facts(
    facts: &TableRuntimeFacts,
    rows: &[TableRow],
) -> LifecycleResult<TableManifestTableFacts> {
    let (timestamp_min, timestamp_max) = timestamp_bounds(rows);
    TableManifestTableFacts::new(
        facts.byte_count(),
        facts.row_count(),
        facts.data_block_count(),
        facts.commit_range().min(),
        facts.commit_range().max(),
        timestamp_min,
        timestamp_max,
    )
    .map_err(format_error)
}

fn manifest_table_bounds(rows: &[TableRow]) -> LifecycleResult<TableManifestTableBounds> {
    let Some(first_row) = rows.first() else {
        return Err(LifecycleError::TableManifestPublicationFailed {
            reason: "reachable table must not be empty",
            source: None,
        });
    };
    let mut physical_first = TablePhysicalKeyBytes::from_row(first_row.row());
    let mut physical_last = physical_first.clone();
    let mut internal_first = first_row.key().clone();
    let mut internal_last = internal_first.clone();
    for row in rows.iter().skip(1) {
        let physical = TablePhysicalKeyBytes::from_row(row.row());
        if physical < physical_first {
            physical_first = physical.clone();
        }
        if physical > physical_last {
            physical_last = physical;
        }
        if row.key() < &internal_first {
            internal_first = row.key().clone();
        }
        if row.key() > &internal_last {
            internal_last = row.key().clone();
        }
    }
    TableManifestTableBounds::new(
        physical_first.as_slice().to_vec(),
        physical_last.as_slice().to_vec(),
        internal_first.as_slice().to_vec(),
        internal_last.as_slice().to_vec(),
    )
    .map_err(format_error)
}

fn manifest_table_provenance(
    table: &BranchOwnedTable,
    entry: &LifecycleDurableTableCatalogEntry,
) -> LifecycleResult<TableManifestTableProvenance> {
    if let Some(source) = table.materialization_source() {
        TableManifestTableProvenance::materialization_replacement(
            source.source_branch_id(),
            source.fork_version(),
        )
        .map_err(format_error)
    } else {
        Ok(entry.provenance.clone())
    }
}

fn validate_manifest_reader_facts(
    table: &TableManifestTableRef,
    reader: &ImmutableTableReader,
) -> LifecycleResult<()> {
    let facts = manifest_table_facts(reader.facts(), reader.rows())?;
    if &facts != table.facts() {
        return Err(LifecycleError::TableManifestRecoveryMismatch {
            reason: "table manifest facts do not match table object",
            source: None,
        });
    }
    let bounds = manifest_table_bounds(reader.rows())?;
    if &bounds != table.bounds() {
        return Err(LifecycleError::TableManifestRecoveryMismatch {
            reason: "table manifest bounds do not match table object",
            source: None,
        });
    }
    Ok(())
}

fn timestamp_bounds(rows: &[TableRow]) -> (Option<Timestamp>, Option<Timestamp>) {
    let mut timestamps = rows.iter().map(TableRow::commit_timestamp);
    let Some(first) = timestamps.next() else {
        return (None, None);
    };
    let (min, max) = timestamps.fold((first, first), |(min, max), timestamp| {
        (min.min(timestamp), max.max(timestamp))
    });
    (Some(min), Some(max))
}

fn manifest_table_refs(manifest: &TableManifest) -> impl Iterator<Item = &TableManifestTableRef> {
    manifest
        .levels()
        .iter()
        .flat_map(TableManifestLevel::tables)
        .chain(
            manifest
                .inherited_layers()
                .iter()
                .flat_map(TableManifestInheritedLayer::levels)
                .flat_map(TableManifestLevel::tables),
        )
}

const fn manifest_status_from_inherited(
    status: InheritedLayerStatus,
) -> TableManifestInheritedLayerStatus {
    match status {
        InheritedLayerStatus::Active => TableManifestInheritedLayerStatus::Active,
        InheritedLayerStatus::Materializing => TableManifestInheritedLayerStatus::Materializing,
        InheritedLayerStatus::Materialized => TableManifestInheritedLayerStatus::Materialized,
        InheritedLayerStatus::Unavailable => TableManifestInheritedLayerStatus::Unavailable,
    }
}

const fn inherited_status_from_manifest(
    status: TableManifestInheritedLayerStatus,
) -> InheritedLayerStatus {
    match status {
        TableManifestInheritedLayerStatus::Active => InheritedLayerStatus::Active,
        TableManifestInheritedLayerStatus::Materializing => InheritedLayerStatus::Materializing,
        TableManifestInheritedLayerStatus::Materialized => InheritedLayerStatus::Materialized,
        TableManifestInheritedLayerStatus::Unavailable => InheritedLayerStatus::Unavailable,
    }
}

fn format_error(error: FormatError) -> LifecycleError {
    LifecycleError::lower_layer_with(
        LifecycleLowerLayer::Format,
        "table manifest format failed",
        error,
    )
}

fn table_manifest_service_error(error: ManifestServiceError) -> LifecycleError {
    let reason = match &error {
        ManifestServiceError::Publish {
            role: ManifestRole::Table,
            source,
        } if matches!(
            source.kind(),
            PublishFailureKind::VisibilityUnknown
                | PublishFailureKind::VisibleDurabilityUnconfirmed
        ) =>
        {
            return LifecycleError::table_manifest_publication_uncertain_with(
                table_manifest_publish_reason(source),
                error,
            );
        }
        ManifestServiceError::Publish {
            role: ManifestRole::Table,
            source,
        } => table_manifest_publish_reason(source),
        ManifestServiceError::Missing {
            role: ManifestRole::Table,
            ..
        } => "table manifest missing",
        ManifestServiceError::Decode {
            role: ManifestRole::Table,
            ..
        } => "table manifest decode failed",
        ManifestServiceError::Encode {
            role: ManifestRole::Table,
            ..
        } => "table manifest encode failed",
        ManifestServiceError::Read {
            role: ManifestRole::Table,
            ..
        } => "table manifest read failed",
        ManifestServiceError::Layout {
            role: ManifestRole::Table,
            ..
        } => "table manifest layout failed",
        ManifestServiceError::InvalidPublishMetadata {
            role: ManifestRole::Table,
            ..
        } => "table manifest publish metadata invalid",
        ManifestServiceError::InvalidRecoveryFact {
            role: ManifestRole::Table,
            ..
        } => "table manifest recovery fact invalid",
        _ => "manifest service role mismatch",
    };
    match error {
        ManifestServiceError::Decode { .. } | ManifestServiceError::InvalidRecoveryFact { .. } => {
            LifecycleError::table_manifest_recovery_mismatch_with(reason, error)
        }
        ManifestServiceError::Publish { .. } => {
            LifecycleError::table_manifest_publication_failed_with(reason, error)
        }
        other => LifecycleError::lower_layer_with(LifecycleLowerLayer::Service, reason, other),
    }
}

fn table_manifest_publish_reason(error: &PublishError) -> &'static str {
    match error.kind() {
        PublishFailureKind::Unsupported => "table manifest publish unsupported",
        PublishFailureKind::PreconditionFailed => "table manifest publish precondition failed",
        PublishFailureKind::FailedBeforeVisibility => {
            "table manifest publish failed before visibility"
        }
        PublishFailureKind::VisibilityUnknown => "table manifest publish visibility unknown",
        PublishFailureKind::VisibleDurabilityUnconfirmed => {
            "table manifest publish durability unconfirmed"
        }
    }
}

fn table_read_error(error: TableObjectReadError) -> LifecycleError {
    match error {
        TableObjectReadError::Backend { object, source }
            if source.kind() == BackendErrorKind::NotFound =>
        {
            LifecycleError::table_manifest_recovery_mismatch_with(
                "table manifest listed table object is missing",
                TableObjectReadError::Backend { object, source },
            )
        }
        TableObjectReadError::FactMismatch { .. } => {
            LifecycleError::table_manifest_recovery_mismatch_with(
                "table manifest facts do not match table object",
                error,
            )
        }
        TableObjectReadError::Source { .. } | TableObjectReadError::Table { .. } => {
            LifecycleError::table_manifest_recovery_mismatch_with(
                "table manifest listed table object failed validation",
                error,
            )
        }
        other => LifecycleError::lower_layer_with(
            LifecycleLowerLayer::Service,
            "table manifest listed table object read failed",
            other,
        ),
    }
}

fn branch_error(error: impl std::error::Error + Send + Sync + 'static) -> LifecycleError {
    LifecycleError::table_manifest_branch_install_failed_with(
        "table manifest branch recovery failed",
        error,
    )
}
