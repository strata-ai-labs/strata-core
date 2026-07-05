//! Durable table rewrite publication.

use super::compaction::{
    branch_error, table_compaction_config_with_storage_budget, LifecycleCompactionIoFacts,
    LifecycleCompactionOutcome, LifecycleCompactionRequest, LifecycleMaterializationOutcome,
    LifecycleMaterializationRequest, LifecycleTableRewriteDurability,
};
use super::{
    publish_table_manifest_for_branch_with_budget, require_generated_artifact_budget,
    require_table_reader_budget, subcompaction_cap, LifecycleDurableTableCatalog, LifecycleError,
    LifecycleResult, StorageBudgetLedger,
};
use crate::backend::{PublishError, PublishFailureKind};
use crate::branch::error::BranchRuntimeError;
use crate::branch::facts::{BranchLevel, BranchTableDescriptor};
use crate::branch::read::{BranchMaterializationSource, BranchOwnedTable};
use crate::branch::state::compaction::{
    BranchCompactionKind, BranchCompactionPlan, BranchCompactionRequest,
};
use crate::branch::state::materialization::{
    BranchMaterializationHandle, BranchMaterializationOutcome, BranchMaterializationPreparedOutput,
    BranchMaterializationRecovery, BranchMaterializationRequest,
};
use crate::branch::state::BranchLocalState;
use crate::format::TableManifestTableProvenance;
use crate::observability::perf_trace;
use crate::service::{
    TableManifestService, TableObjectFacts, TableObjectReadError, TableObjectReaderService,
    TableObjectService, TableObjectServiceError,
};
use crate::table::{BuiltTableArtifact, TableCompactionReport, TableKeyBounds, TableReaderConfig};
use strata_core_next::BranchId;

pub(crate) struct PreparedDurableCompaction {
    request: LifecycleCompactionRequest,
    branch_request: BranchCompactionRequest,
    plan: BranchCompactionPlan,
    io_facts: LifecycleCompactionIoFacts,
    output: PreparedDurableCompactionOutput,
    elapsed: std::time::Duration,
}

enum PreparedDurableCompactionOutput {
    MetadataOnly,
    Published {
        report: TableCompactionReport,
        published: Vec<PublishedRewriteTable>,
    },
}

pub(crate) enum DurableMaterializationBegin {
    Deferred(Box<LifecycleMaterializationOutcome>),
    Build(Box<DurableMaterializationBuild>),
}

pub(crate) struct DurableMaterializationBuild {
    request: LifecycleMaterializationRequest,
    materialization_handle: BranchMaterializationHandle,
    reachability_snapshot: crate::branch::facts::BranchReachabilitySnapshot,
    branch_request: BranchMaterializationRequest,
    branch_snapshot: BranchLocalState,
}

pub(crate) struct PreparedDurableMaterialization {
    request: LifecycleMaterializationRequest,
    materialization_handle: BranchMaterializationHandle,
    reachability_snapshot: crate::branch::facts::BranchReachabilitySnapshot,
    branch_request: BranchMaterializationRequest,
    prepared: Option<BranchMaterializationPreparedOutput>,
    published: Vec<PublishedRewriteTable>,
}

pub(crate) fn compact_durable_branch_manifest_backed(
    branch: &mut BranchLocalState,
    table_service: &TableObjectService<'_>,
    reader_service: &TableObjectReaderService<'_>,
    manifest_service: &TableManifestService<'_>,
    catalog: &mut LifecycleDurableTableCatalog,
    request: &LifecycleCompactionRequest,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<LifecycleCompactionOutcome> {
    let prepared = prepare_durable_compaction_publication(
        branch,
        table_service,
        reader_service,
        request,
        budget,
    )?;
    install_prepared_durable_compaction(branch, manifest_service, catalog, prepared, budget)
}

pub(crate) fn prepare_durable_compaction_publication(
    branch: &BranchLocalState,
    table_service: &TableObjectService<'_>,
    reader_service: &TableObjectReaderService<'_>,
    request: &LifecycleCompactionRequest,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<PreparedDurableCompaction> {
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
    let ranges = subcompaction_ranges_for_publication(branch, &branch_request, &plan)?;
    let output = match build_and_publish_compaction(
        branch,
        &branch_request,
        &plan,
        table_service,
        reader_service,
        budget,
        &ranges,
    )? {
        Some((published, report)) => {
            PreparedDurableCompactionOutput::Published { report, published }
        }
        None => PreparedDurableCompactionOutput::MetadataOnly,
    };
    Ok(PreparedDurableCompaction {
        request,
        branch_request,
        plan,
        io_facts,
        output,
        elapsed: started.elapsed(),
    })
}

/// The subcompaction key ranges for one compaction: `vec![None]` (a single serial build) unless
/// the candidate is an L0-to-L1 table rewrite large enough to split, in which case up to
/// `subcompaction_cap()` disjoint half-open physical-key ranges.
fn subcompaction_ranges_for_publication(
    branch: &BranchLocalState,
    branch_request: &BranchCompactionRequest,
    plan: &BranchCompactionPlan,
) -> LifecycleResult<Vec<Option<TableKeyBounds>>> {
    let Some(candidate) = plan.candidate() else {
        return Ok(vec![None]);
    };
    if candidate.is_metadata_promotion()
        || branch_request.kind() != BranchCompactionKind::CompactL0ToLevelOne
    {
        return Ok(vec![None]);
    }
    let n = subcompaction_cap();
    if n <= 1 {
        return Ok(vec![None]);
    }
    let target_output_bytes = branch_request
        .table_compaction_config()
        .target_output_bytes();
    branch
        .subcompaction_ranges_for_candidate(candidate, n, target_output_bytes)
        .map_err(branch_error)
}

/// Build and publish a compaction as `ranges.len()` parallel subcompactions (or one serial build
/// when `ranges` is a single unbounded range), then aggregate their published output tables and
/// reports into one result. Returns `None` for a metadata promotion (no build). Any subcompaction
/// failure aborts the whole compaction and cleans up already-published objects as an orphaned
/// partial publish.
fn build_and_publish_compaction(
    branch: &BranchLocalState,
    branch_request: &BranchCompactionRequest,
    plan: &BranchCompactionPlan,
    table_service: &TableObjectService<'_>,
    reader_service: &TableObjectReaderService<'_>,
    budget: Option<&StorageBudgetLedger>,
    ranges: &[Option<TableKeyBounds>],
) -> SubcompactionBuildResult {
    let build_range = |index: usize, bounds: Option<&TableKeyBounds>| -> SubcompactionBuildResult {
        let Some((artifacts, report)) = branch
            .prepare_branch_compaction_plan_bounded(branch_request, plan, bounds, index)
            .map_err(branch_error)?
        else {
            return Ok(None);
        };
        let output_level = plan
            .output_level()
            .ok_or(LifecycleError::RewritePublicationFailed {
                reason: "prepared compaction output requires a candidate plan",
                source: None,
            })?;
        let published = publish_compaction_outputs(
            branch.branch_id(),
            output_level,
            plan.materialization_source(),
            table_service,
            reader_service,
            artifacts,
            budget,
        )?;
        Ok(Some((published, report)))
    };

    if ranges.len() <= 1 {
        return build_range(0, ranges.first().and_then(Option::as_ref));
    }

    // BS3.1 (G23 / constraint C1): the subcompaction fan-out spawns threads, unsupported on wasm32.
    // `mod lifecycle` is compiled for wasm (only `localfs` is forbidden — see lib.rs), so the spawn
    // must be cfg'd out of the wasm build, not merely left unreached. On wasm the ranges build
    // serially (n_eff = 1); env vars don't exist there, so `subcompaction_cap()` is already 1 and
    // this arm is effectively dead, but it stays correct if `ranges.len() > 1` is ever forced.
    #[cfg(not(target_arch = "wasm32"))]
    let results: Vec<SubcompactionBuildResult> = std::thread::scope(|scope| {
        let handles: Vec<_> = ranges
            .iter()
            .enumerate()
            .map(|(index, bounds)| scope.spawn(move || build_range(index, bounds.as_ref())))
            .collect();
        handles
            .into_iter()
            .map(|handle| {
                handle.join().unwrap_or_else(|_| {
                    Err(LifecycleError::RewritePublicationFailed {
                        reason: "subcompaction build thread panicked",
                        source: None,
                    })
                })
            })
            .collect()
    });
    #[cfg(target_arch = "wasm32")]
    let results: Vec<SubcompactionBuildResult> = ranges
        .iter()
        .enumerate()
        .map(|(index, bounds)| build_range(index, bounds.as_ref()))
        .collect();

    let mut all_published: Vec<PublishedRewriteTable> = Vec::new();
    let mut merged_report: Option<TableCompactionReport> = None;
    let mut first_error: Option<LifecycleError> = None;
    for result in results {
        match result {
            Ok(Some((published, report))) => {
                all_published.extend(published);
                match &mut merged_report {
                    Some(existing) => existing.accumulate(&report),
                    None => merged_report = Some(report),
                }
            }
            Ok(None) => {}
            Err(error) => {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
    }
    if let Some(error) = first_error {
        return Err(partial_publish_error(&all_published, error));
    }
    Ok(merged_report.map(|report| (all_published, report)))
}

pub(crate) fn install_prepared_durable_compaction(
    branch: &mut BranchLocalState,
    manifest_service: &TableManifestService<'_>,
    catalog: &mut LifecycleDurableTableCatalog,
    prepared: PreparedDurableCompaction,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<LifecycleCompactionOutcome> {
    let outcome =
        install_prepared_durable_compaction_without_publish(branch, catalog, prepared, budget)?;
    Ok(publish_compaction_outcome_manifest(
        branch,
        manifest_service,
        catalog,
        outcome,
        budget,
    ))
}

/// Install a prepared compaction's in-memory and catalog state without writing the table
/// manifest. The returned outcome describes a completed install whose durable manifest is not
/// yet published; the caller must follow with a manifest publish (synchronous under the runtime
/// lock, or off-lock via the three-phase background publish). On a `MetadataOnly` no-candidate
/// plan the returned outcome carries no rewrite, so no publish is required.
pub(crate) fn install_prepared_durable_compaction_without_publish(
    branch: &mut BranchLocalState,
    catalog: &mut LifecycleDurableTableCatalog,
    prepared: PreparedDurableCompaction,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<LifecycleCompactionOutcome> {
    let PreparedDurableCompaction {
        request,
        branch_request,
        plan,
        io_facts,
        output,
        elapsed,
    } = prepared;
    match output {
        PreparedDurableCompactionOutput::MetadataOnly => {
            let branch_outcome = branch
                .install_branch_compaction_plan(&branch_request, &plan)
                .map_err(branch_error)?;
            if plan.is_metadata_promotion() {
                let retained_input_objects = branch_outcome
                    .removed_refs()
                    .iter()
                    .map(|table_ref| table_ref.table_identity().as_str().to_owned())
                    .collect::<Vec<_>>();
                return Ok(LifecycleCompactionOutcome::completed_durable(
                    plan,
                    branch_outcome,
                    io_facts,
                    elapsed,
                    Vec::new(),
                    retained_input_objects,
                ));
            }
            Ok(LifecycleCompactionOutcome::new(
                &request,
                plan,
                branch_outcome,
                io_facts,
                elapsed,
            ))
        }
        PreparedDurableCompactionOutput::Published { report, published } => {
            install_published_durable_compaction(
                branch,
                catalog,
                &branch_request,
                plan,
                io_facts,
                elapsed,
                report,
                &published,
                budget,
            )
        }
    }
}

/// Publish the table manifest for an already-installed compaction outcome. Returns the outcome
/// unchanged on success, or stamped with manifest debt on publish failure. Compaction outcomes
/// that did not rewrite anything (`DeferredNoCandidate`) require no manifest publish.
pub(crate) fn publish_compaction_outcome_manifest(
    branch: &BranchLocalState,
    manifest_service: &TableManifestService<'_>,
    catalog: &mut LifecycleDurableTableCatalog,
    outcome: LifecycleCompactionOutcome,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleCompactionOutcome {
    if outcome.status() == crate::lifecycle::LifecycleCompactionStatus::DeferredNoCandidate {
        return outcome;
    }
    match publish_table_manifest_for_branch_with_budget(branch, manifest_service, catalog, budget) {
        Ok(_) => outcome,
        Err(error) => outcome.manifest_debt(error),
    }
}

fn install_published_durable_compaction(
    branch: &mut BranchLocalState,
    catalog: &mut LifecycleDurableTableCatalog,
    branch_request: &BranchCompactionRequest,
    plan: BranchCompactionPlan,
    io_facts: LifecycleCompactionIoFacts,
    elapsed: std::time::Duration,
    report: TableCompactionReport,
    published: &[PublishedRewriteTable],
    _budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<LifecycleCompactionOutcome> {
    let mut next_catalog = catalog.clone();
    record_published_outputs(&mut next_catalog, published).map_err(|source| {
        LifecycleError::rewrite_publication_orphaned_with(
            published_object_names(published),
            "table rewrite published output before catalog update failed",
            source,
        )
    })?;
    let output_tables = published
        .iter()
        .map(|output| published_table(output).clone())
        .collect::<Vec<_>>();
    let branch_outcome = branch
        .install_branch_compaction_prepared_plan(branch_request, &plan, output_tables, report)
        .map_err(|source| {
            LifecycleError::rewrite_publication_orphaned_with(
                published_object_names(published),
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
    Ok(LifecycleCompactionOutcome::completed_durable(
        plan,
        branch_outcome,
        io_facts,
        elapsed,
        output_objects,
        retained_input_objects,
    ))
}

fn compaction_request_with_durable_budget_target(
    request: BranchCompactionRequest,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<BranchCompactionRequest> {
    let budget = budget.map(StorageBudgetLedger::budget);
    let config =
        table_compaction_config_with_storage_budget(request.table_compaction_config(), budget)?;
    Ok(request.with_table_compaction_config(config))
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
    let build = match begin_durable_materialization_build(branch, request)? {
        DurableMaterializationBegin::Deferred(outcome) => return Ok(*outcome),
        DurableMaterializationBegin::Build(build) => *build,
    };
    let prepared = build.build(table_service, reader_service, budget)?;
    install_prepared_durable_materialization(branch, manifest_service, catalog, prepared, budget)
}

pub(crate) fn begin_durable_materialization_build(
    branch: &mut BranchLocalState,
    request: &LifecycleMaterializationRequest,
) -> LifecycleResult<DurableMaterializationBegin> {
    let request = request
        .clone()
        .with_durability(LifecycleTableRewriteDurability::DurableTableManifestBacked);
    if branch
        .inherited_layers()
        .get(request.layer_index())
        .is_none()
        && request.handle().is_none()
    {
        return Ok(DurableMaterializationBegin::Deferred(Box::new(
            LifecycleMaterializationOutcome::deferred(&request),
        )));
    }
    let (materialization_handle, reachability_snapshot, branch_request) =
        materialization_binding_and_request(branch, &request)?;
    Ok(DurableMaterializationBegin::Build(Box::new(
        DurableMaterializationBuild {
            request,
            materialization_handle,
            reachability_snapshot,
            branch_request,
            branch_snapshot: branch.clone(),
        },
    )))
}

impl DurableMaterializationBuild {
    pub(crate) fn build(
        self,
        table_service: &TableObjectService<'_>,
        reader_service: &TableObjectReaderService<'_>,
        budget: Option<&StorageBudgetLedger>,
    ) -> LifecycleResult<PreparedDurableMaterialization> {
        let prepared = self
            .branch_snapshot
            .prepare_materialization_output(&self.branch_request)
            .map_err(branch_error)?;
        let published = match prepared.as_ref() {
            Some(prepared) if !prepared.artifacts().is_empty() => publish_materialization_outputs(
                self.branch_snapshot.branch_id(),
                table_service,
                reader_service,
                prepared,
                budget,
            )?,
            _ => Vec::new(),
        };
        Ok(PreparedDurableMaterialization {
            request: self.request,
            materialization_handle: self.materialization_handle,
            reachability_snapshot: self.reachability_snapshot,
            branch_request: self.branch_request,
            prepared,
            published,
        })
    }
}

pub(crate) fn install_prepared_durable_materialization(
    branch: &mut BranchLocalState,
    manifest_service: &TableManifestService<'_>,
    catalog: &mut LifecycleDurableTableCatalog,
    prepared: PreparedDurableMaterialization,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<LifecycleMaterializationOutcome> {
    let outcome = install_prepared_durable_materialization_without_publish(
        branch, catalog, prepared, budget,
    )?;
    Ok(publish_materialization_outcome_manifest(
        branch,
        manifest_service,
        catalog,
        outcome,
        budget,
    ))
}

/// Install a prepared materialization's in-memory and catalog state without writing the table
/// manifest. The returned outcome describes a completed install whose durable manifest is not yet
/// published; the caller must follow with a manifest publish (synchronous under the runtime lock,
/// or off-lock via the three-phase background publish). An already-materialized layer requires no
/// manifest publish.
pub(crate) fn install_prepared_durable_materialization_without_publish(
    branch: &mut BranchLocalState,
    catalog: &mut LifecycleDurableTableCatalog,
    prepared: PreparedDurableMaterialization,
    _budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<LifecycleMaterializationOutcome> {
    let PreparedDurableMaterialization {
        request,
        materialization_handle,
        reachability_snapshot,
        branch_request,
        prepared,
        published,
    } = prepared;
    let Some(prepared_output) = prepared else {
        let branch_outcome = branch
            .materialize_inherited_layer(&branch_request)
            .map_err(branch_error)?;
        return Ok(materialization_outcome_after_install(
            &request,
            materialization_handle,
            reachability_snapshot,
            branch_outcome,
            Vec::new(),
        ));
    };
    if published.is_empty() {
        let branch_outcome = branch
            .install_materialization_prepared_output(&branch_request, &prepared_output, Vec::new())
            .map_err(branch_error)?;
        return Ok(materialization_outcome_after_install(
            &request,
            materialization_handle,
            reachability_snapshot,
            branch_outcome,
            Vec::new(),
        ));
    }
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
        .install_materialization_prepared_output(&branch_request, &prepared_output, output_tables)
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
    Ok(materialization_outcome_after_install(
        &request,
        materialization_handle,
        reachability_snapshot,
        branch_outcome,
        output_objects,
    ))
}

/// Publish the table manifest for an already-installed materialization outcome. Returns the
/// outcome unchanged on success, or stamped with manifest debt on publish failure.
pub(crate) fn publish_materialization_outcome_manifest(
    branch: &BranchLocalState,
    manifest_service: &TableManifestService<'_>,
    catalog: &mut LifecycleDurableTableCatalog,
    outcome: LifecycleMaterializationOutcome,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleMaterializationOutcome {
    match publish_table_manifest_for_branch_with_budget(branch, manifest_service, catalog, budget) {
        Ok(_) => outcome,
        Err(error) => outcome.manifest_debt(error),
    }
}

fn materialization_outcome_after_install(
    request: &LifecycleMaterializationRequest,
    materialization_handle: BranchMaterializationHandle,
    reachability_snapshot: crate::branch::facts::BranchReachabilitySnapshot,
    branch_outcome: BranchMaterializationOutcome,
    output_objects: Vec<crate::object::ObjectName>,
) -> LifecycleMaterializationOutcome {
    if matches!(
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
    }
}

type PublishedRewriteTable = (
    BranchOwnedTable,
    TableObjectFacts,
    TableManifestTableProvenance,
);

/// One subcompaction's published output tables plus its compaction report.
type SubcompactionBuildOutput = (Vec<PublishedRewriteTable>, TableCompactionReport);
/// Result of building one subcompaction range (`None` = a metadata promotion, no build).
type SubcompactionBuildResult = LifecycleResult<Option<SubcompactionBuildOutput>>;

struct PublishedRewriteObject {
    facts: TableObjectFacts,
    exact_bytes_validated: bool,
}

fn publish_compaction_outputs(
    branch_id: BranchId,
    output_level: BranchLevel,
    materialization_source: Option<BranchMaterializationSource>,
    table_service: &TableObjectService<'_>,
    reader_service: &TableObjectReaderService<'_>,
    artifacts: Vec<BuiltTableArtifact>,
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
            artifact.clone(),
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
    artifact: BuiltTableArtifact,
    materialization_source: Option<BranchMaterializationSource>,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<PublishedRewriteTable> {
    let extras = artifact.extras().clone();
    let (bytes, table_facts, rows) = artifact.into_parts_with_rows();
    require_optional_rewrite_generated_budget(budget, table_facts.byte_count())?;
    require_optional_rewrite_reader_budget(budget, table_facts.byte_count())?;
    let identity = table_facts.identity().clone();
    let branch_component = branch_id.to_string();
    let object = publish_or_load_rewrite_output(
        table_service,
        reader_service,
        &branch_component,
        u32::from(level.raw()),
        identity.as_str(),
        &bytes,
        &table_facts,
    )?;
    let object_facts = object.facts;
    if !object.exact_bytes_validated {
        reader_service
            .require_exact_bytes(&object_facts, &bytes)
            .map_err(|source| {
                orphaned_published_object_error(
                    &object_facts,
                    "table rewrite published output before byte-exact validation failed",
                    source,
                )
            })?;
    }
    let reader = reader_service
        .open_reader_from_validated_rows(
            &object_facts,
            table_facts,
            &bytes,
            rows,
            TableReaderConfig::default().with_eager_filter_unavailable(),
        )
        .map_err(|source| {
            orphaned_published_object_error(
                &object_facts,
                "table rewrite published output before in-memory reader handoff failed",
                source,
            )
        })?;
    perf_trace::record_table_rewrite_reader_reopen_avoided();
    perf_trace::record_table_rewrite_reader_rows_reused();
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
                branch_id, descriptor, reader, extras, source,
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
            BranchOwnedTable::new(branch_id, descriptor, reader, extras).map_err(|error| {
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
) -> LifecycleResult<PublishedRewriteObject> {
    match table_service.publish_create_prevalidated(
        branch_component,
        level,
        object_id,
        bytes,
        table_facts,
    ) {
        Ok(facts) => {
            perf_trace::record_table_rewrite_redundant_fact_decode_avoided();
            Ok(PublishedRewriteObject {
                facts,
                exact_bytes_validated: false,
            })
        }
        Err(TableObjectServiceError::Publish { source, .. })
            if source.kind() == PublishFailureKind::PreconditionFailed =>
        {
            perf_trace::record_table_rewrite_redundant_fact_decode_avoided();
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
            Ok(PublishedRewriteObject {
                facts: object_facts,
                exact_bytes_validated: true,
            })
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
