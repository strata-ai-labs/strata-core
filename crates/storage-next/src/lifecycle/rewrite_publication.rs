//! Durable table rewrite publication.

use super::compaction::{
    branch_error, LifecycleCompactionIoFacts, LifecycleCompactionOutcome,
    LifecycleCompactionRequest, LifecycleMaterializationOutcome, LifecycleMaterializationRequest,
    LifecycleTableRewriteDurability,
};
use super::{
    publish_table_manifest_for_branch_with_budget, require_generated_artifact_budget,
    require_table_reader_budget, LifecycleDurableTableCatalog, LifecycleError, LifecycleLowerLayer,
    LifecycleResult, StorageBudgetLedger, StorageBudgetPool,
};
use crate::backend::{PublishError, PublishFailureKind};
use crate::branch::error::BranchRuntimeError;
use crate::branch::facts::{BranchLevel, BranchTableDescriptor};
use crate::branch::read::{BranchMaterializationSource, BranchOwnedTable};
use crate::branch::state::compaction::{
    BranchCompactionOutcome, BranchCompactionPlan, BranchCompactionRequest,
};
use crate::branch::state::materialization::{
    BranchMaterializationHandle, BranchMaterializationOutcome, BranchMaterializationPreparedOutput,
    BranchMaterializationRecovery, BranchMaterializationRequest,
};
use crate::branch::state::BranchLocalState;
use crate::format::TableManifestTableProvenance;
use crate::service::{
    TableManifestService, TableObjectFacts, TableObjectReadError, TableObjectReaderService,
    TableObjectService, TableObjectServiceError,
};
use crate::table::{BuiltTableArtifact, TableCompactionConfig, TableReaderConfig};
use strata_core_next::BranchId;

pub(crate) fn compact_durable_branch_manifest_backed(
    branch: &mut BranchLocalState,
    table_service: &TableObjectService<'_>,
    reader_service: &TableObjectReaderService<'_>,
    manifest_service: &TableManifestService<'_>,
    catalog: &mut LifecycleDurableTableCatalog,
    request: &LifecycleCompactionRequest,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<LifecycleCompactionOutcome> {
    let started = std::time::Instant::now();
    let request = request
        .clone()
        .with_durability(LifecycleTableRewriteDurability::DurableTableManifestBacked);
    let branch_request =
        compaction_request_with_durable_budget_target(request.branch_request()?, budget)?;
    let plan = branch
        .plan_branch_compaction(&branch_request)
        .map_err(branch_error)?;
    let io_facts = LifecycleCompactionIoFacts::from_plan(branch, &plan);
    let Some((artifacts, report)) = branch
        .prepare_branch_compaction_plan(&branch_request, &plan)
        .map_err(branch_error)?
    else {
        let branch_outcome = branch
            .install_branch_compaction_plan(&branch_request, &plan)
            .map_err(branch_error)?;
        if plan.is_metadata_promotion() {
            return Ok(finish_metadata_promotion_compaction(
                branch,
                manifest_service,
                catalog,
                plan,
                branch_outcome,
                io_facts,
                started.elapsed(),
                budget,
            ));
        }
        return Ok(LifecycleCompactionOutcome::new(
            &request,
            plan,
            branch_outcome,
            io_facts,
            started.elapsed(),
        ));
    };
    let Some(output_level) = plan.output_level() else {
        return Err(LifecycleError::RewritePublicationFailed {
            reason: "prepared compaction output requires a candidate plan",
            source: None,
        });
    };
    let published = publish_compaction_outputs(
        branch.branch_id(),
        output_level,
        plan.materialization_source(),
        table_service,
        reader_service,
        &artifacts,
        budget,
    )?;
    let mut next_catalog = catalog.clone();
    record_published_outputs(&mut next_catalog, &published).map_err(|source| {
        LifecycleError::rewrite_publication_orphaned_with(
            published_object_names(&published),
            "table rewrite published output before catalog update failed",
            source,
        )
    })?;
    let output_tables = published
        .iter()
        .map(|output| published_table(output).clone())
        .collect::<Vec<_>>();
    let branch_outcome = branch
        .install_branch_compaction_prepared_plan(&branch_request, &plan, output_tables, report)
        .map_err(|source| {
            LifecycleError::rewrite_publication_orphaned_with(
                published_object_names(&published),
                "table rewrite published output before branch install failed",
                source,
            )
        })?;
    *catalog = next_catalog;
    let retained_input_objects = branch_outcome
        .removed_refs()
        .iter()
        .map(|table_ref| table_ref.table_identity().as_str().to_owned())
        .collect::<Vec<_>>();
    let output_objects = published
        .iter()
        .map(|output| published_object_facts(output).object().clone())
        .collect::<Vec<_>>();
    let outcome = LifecycleCompactionOutcome::completed_durable(
        plan,
        branch_outcome,
        io_facts,
        started.elapsed(),
        output_objects,
        retained_input_objects,
    );
    match publish_table_manifest_for_branch_with_budget(branch, manifest_service, catalog, budget) {
        Ok(_) => Ok(outcome),
        Err(error) => Ok(outcome.manifest_debt(error)),
    }
}

fn compaction_request_with_durable_budget_target(
    request: BranchCompactionRequest,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<BranchCompactionRequest> {
    let Some(budget) = budget else {
        return Ok(request);
    };
    let config = table_compaction_config_with_generated_artifact_budget(
        request.table_compaction_config(),
        budget,
    )?;
    Ok(request.with_table_compaction_config(config))
}

fn table_compaction_config_with_generated_artifact_budget(
    config: TableCompactionConfig,
    budget: &StorageBudgetLedger,
) -> LifecycleResult<TableCompactionConfig> {
    let generated_limit = budget
        .budget()
        .pool_limit_bytes(StorageBudgetPool::GeneratedArtifact);
    let reader_limit = budget
        .budget()
        .pool_limit_bytes(StorageBudgetPool::TableReader);
    let limit = generated_limit.min(reader_limit);
    if limit == 0 {
        return Ok(config);
    }
    let capped_target = config.target_output_bytes().min((limit / 2).max(1));
    if capped_target == config.target_output_bytes() {
        return Ok(config);
    }
    TableCompactionConfig::new(capped_target, config.max_output_tables()).map_err(|source| {
        LifecycleError::lower_layer_with(
            LifecycleLowerLayer::TableRuntime,
            "table runtime failed",
            source,
        )
    })
}

fn finish_metadata_promotion_compaction(
    branch: &BranchLocalState,
    manifest_service: &TableManifestService<'_>,
    catalog: &mut LifecycleDurableTableCatalog,
    plan: BranchCompactionPlan,
    branch_outcome: BranchCompactionOutcome,
    io_facts: LifecycleCompactionIoFacts,
    elapsed: std::time::Duration,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleCompactionOutcome {
    let retained_input_objects = branch_outcome
        .removed_refs()
        .iter()
        .map(|table_ref| table_ref.table_identity().as_str().to_owned())
        .collect::<Vec<_>>();
    let outcome = LifecycleCompactionOutcome::completed_durable(
        plan,
        branch_outcome,
        io_facts,
        elapsed,
        Vec::new(),
        retained_input_objects,
    );
    match publish_table_manifest_for_branch_with_budget(branch, manifest_service, catalog, budget) {
        Ok(_) => outcome,
        Err(error) => outcome.manifest_debt(error),
    }
}

pub(crate) fn materialize_durable_branch_manifest_backed(
    branch: &mut BranchLocalState,
    table_service: &TableObjectService<'_>,
    reader_service: &TableObjectReaderService<'_>,
    manifest_service: &TableManifestService<'_>,
    catalog: &mut LifecycleDurableTableCatalog,
    request: &LifecycleMaterializationRequest,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<LifecycleMaterializationOutcome> {
    let request = request
        .clone()
        .with_durability(LifecycleTableRewriteDurability::DurableTableManifestBacked);
    if branch
        .inherited_layers()
        .get(request.layer_index())
        .is_none()
        && request.handle().is_none()
    {
        return Ok(LifecycleMaterializationOutcome::deferred(&request));
    }
    let (materialization_handle, reachability_snapshot, branch_request) =
        materialization_binding_and_request(branch, &request)?;
    let Some(prepared) = branch
        .prepare_materialization_output(&branch_request)
        .map_err(branch_error)?
    else {
        let branch_outcome = branch
            .materialize_inherited_layer(&branch_request)
            .map_err(branch_error)?;
        return Ok(finish_materialization_after_install(
            branch,
            manifest_service,
            catalog,
            &request,
            materialization_handle,
            reachability_snapshot,
            branch_outcome,
            Vec::new(),
            budget,
        ));
    };
    if prepared.artifacts().is_empty() {
        let branch_outcome = branch
            .install_materialization_prepared_output(&branch_request, &prepared, Vec::new())
            .map_err(branch_error)?;
        return Ok(finish_materialization_after_install(
            branch,
            manifest_service,
            catalog,
            &request,
            materialization_handle,
            reachability_snapshot,
            branch_outcome,
            Vec::new(),
            budget,
        ));
    }
    let published = publish_materialization_outputs(
        branch.branch_id(),
        table_service,
        reader_service,
        &prepared,
        budget,
    )?;
    let mut next_catalog = catalog.clone();
    record_published_outputs(&mut next_catalog, &published).map_err(|source| {
        LifecycleError::rewrite_publication_orphaned_with(
            published_object_names(&published),
            "table rewrite published replacement before catalog update failed",
            source,
        )
    })?;
    let output_tables = published
        .iter()
        .map(|output| published_table(output).clone())
        .collect::<Vec<_>>();
    let branch_outcome = branch
        .install_materialization_prepared_output(&branch_request, &prepared, output_tables)
        .map_err(|source| {
            LifecycleError::rewrite_publication_orphaned_with(
                published_object_names(&published),
                "table rewrite published replacement before materialization install failed",
                source,
            )
        })?;
    *catalog = next_catalog;
    let output_objects = published
        .iter()
        .map(|output| published_object_facts(output).object().clone())
        .collect::<Vec<_>>();
    Ok(finish_materialization_after_install(
        branch,
        manifest_service,
        catalog,
        &request,
        materialization_handle,
        reachability_snapshot,
        branch_outcome,
        output_objects,
        budget,
    ))
}

fn finish_materialization_after_install(
    branch: &BranchLocalState,
    manifest_service: &TableManifestService<'_>,
    catalog: &mut LifecycleDurableTableCatalog,
    request: &LifecycleMaterializationRequest,
    materialization_handle: BranchMaterializationHandle,
    reachability_snapshot: crate::branch::facts::BranchReachabilitySnapshot,
    branch_outcome: BranchMaterializationOutcome,
    output_objects: Vec<crate::object::ObjectName>,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleMaterializationOutcome {
    let outcome = if matches!(
        branch_outcome.recovery(),
        BranchMaterializationRecovery::LayerAlreadyMaterialized,
    ) {
        LifecycleMaterializationOutcome::completed(
            request,
            materialization_handle,
            reachability_snapshot,
            branch_outcome,
        )
    } else {
        LifecycleMaterializationOutcome::completed_durable(
            materialization_handle,
            reachability_snapshot,
            branch_outcome,
            output_objects,
        )
    };
    match publish_table_manifest_for_branch_with_budget(branch, manifest_service, catalog, budget) {
        Ok(_) => outcome,
        Err(error) => outcome.manifest_debt(error),
    }
}

type PublishedRewriteTable = (
    BranchOwnedTable,
    TableObjectFacts,
    TableManifestTableProvenance,
);

fn publish_compaction_outputs(
    branch_id: BranchId,
    output_level: BranchLevel,
    materialization_source: Option<BranchMaterializationSource>,
    table_service: &TableObjectService<'_>,
    reader_service: &TableObjectReaderService<'_>,
    artifacts: &[BuiltTableArtifact],
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<Vec<PublishedRewriteTable>> {
    let mut published = Vec::new();
    for artifact in artifacts {
        match publish_rewrite_artifact(
            branch_id,
            output_level,
            table_service,
            reader_service,
            artifact,
            materialization_source,
            budget,
        ) {
            Ok(output) => published.push(output),
            Err(error) => return Err(partial_publish_error(&published, error)),
        }
    }
    Ok(published)
}

fn publish_materialization_outputs(
    branch_id: BranchId,
    table_service: &TableObjectService<'_>,
    reader_service: &TableObjectReaderService<'_>,
    prepared: &BranchMaterializationPreparedOutput,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<Vec<PublishedRewriteTable>> {
    let mut published = Vec::new();
    for artifact in prepared.artifacts() {
        match publish_rewrite_artifact(
            branch_id,
            BranchLevel::ZERO,
            table_service,
            reader_service,
            artifact,
            Some(prepared.materialization_source()),
            budget,
        ) {
            Ok(output) => published.push(output),
            Err(error) => return Err(partial_publish_error(&published, error)),
        }
    }
    Ok(published)
}

fn publish_rewrite_artifact(
    branch_id: BranchId,
    level: BranchLevel,
    table_service: &TableObjectService<'_>,
    reader_service: &TableObjectReaderService<'_>,
    artifact: &BuiltTableArtifact,
    materialization_source: Option<BranchMaterializationSource>,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<PublishedRewriteTable> {
    require_optional_rewrite_generated_budget(budget, artifact.byte_count())?;
    require_optional_rewrite_reader_budget(budget, artifact.byte_count())?;
    let identity = artifact.facts().identity().clone();
    let branch_component = branch_id.to_string();
    let object_facts = publish_or_load_rewrite_output(
        table_service,
        reader_service,
        &branch_component,
        u32::from(level.raw()),
        identity.as_str(),
        artifact.bytes(),
        artifact.facts(),
    )?;
    let reader = reader_service
        .open_reader(
            identity.clone(),
            &object_facts,
            TableReaderConfig::default(),
        )
        .map_err(|source| {
            orphaned_published_object_error(
                &object_facts,
                "table rewrite published output before reopen failed",
                source,
            )
        })?;
    let descriptor =
        BranchTableDescriptor::new(identity, reader.facts().clone(), level).map_err(|source| {
            orphaned_published_object_error(
                &object_facts,
                "table rewrite published output before descriptor validation failed",
                source,
            )
        })?;
    let (table, provenance) = if let Some(source) = materialization_source {
        (
            BranchOwnedTable::new_materialization_replacement(
                branch_id, descriptor, reader, source,
            )
            .map_err(|error| {
                orphaned_published_object_error(
                    &object_facts,
                    "table rewrite published replacement before branch table validation failed",
                    error,
                )
            })?,
            TableManifestTableProvenance::materialization_replacement(
                source.source_branch_id(),
                source.fork_version(),
            )
            .map_err(|error| {
                orphaned_published_object_error(
                    &object_facts,
                    "table rewrite published replacement before provenance validation failed",
                    error,
                )
            })?,
        )
    } else {
        (
            BranchOwnedTable::new(branch_id, descriptor, reader).map_err(|error| {
                orphaned_published_object_error(
                    &object_facts,
                    "table rewrite published output before branch table validation failed",
                    error,
                )
            })?,
            TableManifestTableProvenance::Compaction,
        )
    };
    Ok((table, object_facts, provenance))
}

fn require_optional_rewrite_generated_budget(
    budget: Option<&StorageBudgetLedger>,
    bytes: u64,
) -> LifecycleResult<()> {
    if let Some(budget) = budget {
        require_generated_artifact_budget(
            budget,
            bytes,
            "table rewrite artifact exceeds generated artifact budget",
        )?;
    }
    Ok(())
}

fn require_optional_rewrite_reader_budget(
    budget: Option<&StorageBudgetLedger>,
    bytes: u64,
) -> LifecycleResult<()> {
    if let Some(budget) = budget {
        require_table_reader_budget(budget, bytes, "table rewrite reader exceeds storage budget")?;
    }
    Ok(())
}

fn publish_or_load_rewrite_output(
    table_service: &TableObjectService<'_>,
    reader_service: &TableObjectReaderService<'_>,
    branch_component: &str,
    level: u32,
    object_id: &str,
    bytes: &[u8],
    table_facts: &crate::table::TableRuntimeFacts,
) -> LifecycleResult<TableObjectFacts> {
    match table_service.publish_create(branch_component, level, object_id, bytes) {
        Ok(facts) => Ok(facts),
        Err(TableObjectServiceError::Publish { source, .. })
            if source.kind() == PublishFailureKind::PreconditionFailed =>
        {
            let object_facts = TableObjectService::facts_for_table(
                branch_component,
                level,
                object_id,
                table_facts,
            )
            .map_err(rewrite_table_service_error)?;
            reader_service
                .require_exact_bytes(&object_facts, bytes)
                .map_err(rewrite_existing_table_error)?;
            Ok(object_facts)
        }
        Err(error) => Err(rewrite_table_service_error(error)),
    }
}

fn published_object_names(published: &[PublishedRewriteTable]) -> Vec<String> {
    published
        .iter()
        .map(|output| published_object_facts(output).object().as_str().to_owned())
        .collect()
}

fn record_published_outputs(
    catalog: &mut LifecycleDurableTableCatalog,
    published: &[PublishedRewriteTable],
) -> LifecycleResult<()> {
    for output in published {
        catalog.record_table_with_provenance(
            published_table(output).descriptor().identity().clone(),
            published_object_facts(output).clone(),
            published_provenance(output).clone(),
        )?;
    }
    Ok(())
}

fn published_table(output: &PublishedRewriteTable) -> &BranchOwnedTable {
    &output.0
}

fn published_object_facts(output: &PublishedRewriteTable) -> &TableObjectFacts {
    &output.1
}

fn published_provenance(output: &PublishedRewriteTable) -> &TableManifestTableProvenance {
    &output.2
}

fn partial_publish_error(
    published: &[PublishedRewriteTable],
    error: LifecycleError,
) -> LifecycleError {
    let previous_objects = published_object_names(published);
    if previous_objects.is_empty() {
        return error;
    }
    match error {
        LifecycleError::RewritePublicationOrphaned {
            mut objects,
            reason,
            source,
        } => {
            let mut all_objects = previous_objects;
            all_objects.append(&mut objects);
            LifecycleError::RewritePublicationOrphaned {
                objects: all_objects,
                reason,
                source,
            }
        }
        other => LifecycleError::rewrite_publication_orphaned_with(
            previous_objects,
            "table rewrite partially published outputs before failure",
            other,
        ),
    }
}

fn orphaned_published_object_error(
    object_facts: &TableObjectFacts,
    reason: &'static str,
    source: impl std::error::Error + Send + Sync + 'static,
) -> LifecycleError {
    LifecycleError::rewrite_publication_orphaned_with(
        vec![object_facts.object().as_str().to_owned()],
        reason,
        source,
    )
}

fn materialization_binding_and_request(
    branch: &mut BranchLocalState,
    request: &LifecycleMaterializationRequest,
) -> LifecycleResult<(
    BranchMaterializationHandle,
    crate::branch::facts::BranchReachabilitySnapshot,
    BranchMaterializationRequest,
)> {
    if branch.branch_id() != request.child_branch_id() {
        return Err(branch_error(BranchRuntimeError::InvalidBranchState {
            reason: "materialization request branch id must match branch state",
        }));
    }
    if let Some(handle) = request.handle() {
        if let Some(layer_index) = materialization_layer_index_for_handle(branch, handle) {
            let (bound_handle, snapshot) = branch
                .mark_inherited_layer_materializing(layer_index)
                .map_err(branch_error)?;
            let branch_request = BranchMaterializationRequest::from_handle(
                bound_handle,
                request.output_identity_prefix().to_owned(),
            )
            .map_err(branch_error)?;
            Ok((bound_handle, snapshot, branch_request))
        } else {
            let snapshot = branch.reachability_snapshot().map_err(branch_error)?;
            Ok((handle, snapshot, request.branch_request()?))
        }
    } else {
        let (handle, snapshot) = branch
            .mark_inherited_layer_materializing(request.layer_index())
            .map_err(branch_error)?;
        let branch_request = BranchMaterializationRequest::from_handle(
            handle,
            request.output_identity_prefix().to_owned(),
        )
        .map_err(branch_error)?;
        Ok((handle, snapshot, branch_request))
    }
}

fn materialization_layer_index_for_handle(
    branch: &BranchLocalState,
    handle: BranchMaterializationHandle,
) -> Option<usize> {
    branch.inherited_layers().iter().position(|layer| {
        layer.source_branch_id() == handle.source_branch_id()
            && layer.fork_version() == handle.fork_version()
    })
}

fn rewrite_table_service_error(error: TableObjectServiceError) -> LifecycleError {
    if let TableObjectServiceError::Publish { object, source } = &error {
        if matches!(
            source.kind(),
            PublishFailureKind::VisibilityUnknown
                | PublishFailureKind::VisibleDurabilityUnconfirmed
        ) {
            return LifecycleError::rewrite_publication_uncertain_with_objects(
                vec![object.as_str().to_owned()],
                rewrite_publish_reason(source),
                error,
            );
        }
    }
    let reason = match &error {
        TableObjectServiceError::Layout { .. } => "table rewrite object layout failed",
        TableObjectServiceError::List { .. } => "table rewrite object list failed",
        TableObjectServiceError::Metadata { .. } => "table rewrite object metadata failed",
        TableObjectServiceError::Decode { .. } => "table rewrite object decode failed",
        TableObjectServiceError::Publish { source, .. } => rewrite_publish_reason(source),
        TableObjectServiceError::InvalidPublishMetadata { .. } => {
            "table rewrite object publish metadata invalid"
        }
    };
    LifecycleError::rewrite_publication_failed_with(reason, error)
}

fn rewrite_existing_table_error(error: TableObjectReadError) -> LifecycleError {
    LifecycleError::rewrite_publication_failed_with(
        "table rewrite existing output does not match expected bytes",
        error,
    )
}

fn rewrite_publish_reason(error: &PublishError) -> &'static str {
    match error.kind() {
        PublishFailureKind::Unsupported => "table rewrite output publish unsupported",
        PublishFailureKind::PreconditionFailed => "table rewrite output already exists",
        PublishFailureKind::FailedBeforeVisibility => {
            "table rewrite output publish failed before visibility"
        }
        PublishFailureKind::VisibilityUnknown => "table rewrite output publish visibility unknown",
        PublishFailureKind::VisibleDurabilityUnconfirmed => {
            "table rewrite output publish durability unconfirmed"
        }
    }
}
