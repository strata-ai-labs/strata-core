//! Branch preview promotion — the engine workflow behind `BranchService::preview`.
//!
//! Preview derives the branch point from lineage, runs a three-way comparison
//! (branch point → source, branch point → target), and reports the conflicts a
//! promotion would hit — without mutating either branch (contract §Preview
//! Promotion, conformance #5).
//!
//! M12C1 supports a direct fork lineage: one branch forked from the other. The
//! branch point is the ancestor's state at the fork. Sibling, transitive, and
//! unrelated lineages are rejected here and land in a follow-on; per the
//! contract, callers may never inject a synthetic branch point.

use std::collections::{BTreeMap, BTreeSet};

use strata_core::{BranchId, CommitVersion, Timestamp};

use crate::api::{
    BranchPreview, ComparedCapability, ConflictKind, ConflictStrategyResult, PreviewConflict,
    PromotionStrategy,
};
use crate::branch::adapter::{CapabilityBranchAdapter, DerivedDisposition, EntitySummary};
use crate::branch::catalog::BranchCatalogRecord;
use crate::control::space::registered_spaces;
use crate::data::event::EventBranchAdapter;
use crate::data::graph::{
    GraphEdgeBranchAdapter, GraphNodeBranchAdapter, GraphOntologyBranchAdapter,
};
use crate::data::json::JsonBranchAdapter;
use crate::data::kv::{KvBranchAdapter, ProductSpace};
use crate::data::vector::VectorBranchAdapter;
use crate::diagnostics::{EngineError, EngineResult};
use crate::persistence::{ReadSelector, StoragePersistence};

/// The authored capabilities a branch workflow enumerates, in report order.
/// Comparison covers all of them; promotion additionally filters to those whose
/// adapter reports `supports_promotion` (see `three_way`).
const AUTHORED_CAPABILITIES: [ComparedCapability; 7] = [
    ComparedCapability::KeyValue,
    ComparedCapability::Json,
    ComparedCapability::Vector,
    ComparedCapability::Event,
    ComparedCapability::GraphNode,
    ComparedCapability::GraphEdge,
    ComparedCapability::GraphOntology,
];

/// The branch adapter for one capability. Single source of truth for the
/// capability→adapter mapping, shared by compare, preview, and promotion.
pub(crate) fn adapter_for(capability: ComparedCapability) -> Box<dyn CapabilityBranchAdapter> {
    match capability {
        ComparedCapability::KeyValue => Box::new(KvBranchAdapter),
        ComparedCapability::Json => Box::new(JsonBranchAdapter),
        ComparedCapability::Vector => Box::new(VectorBranchAdapter),
        ComparedCapability::Event => Box::new(EventBranchAdapter),
        ComparedCapability::GraphNode => Box::new(GraphNodeBranchAdapter),
        ComparedCapability::GraphEdge => Box::new(GraphEdgeBranchAdapter),
        ComparedCapability::GraphOntology => Box::new(GraphOntologyBranchAdapter),
    }
}

/// The authored capabilities in report order, each with its adapter. The single
/// registry shared by compare, preview, and promotion — register a capability
/// once here and all three cover it.
pub(crate) fn capability_adapters() -> Vec<(ComparedCapability, Box<dyn CapabilityBranchAdapter>)> {
    AUTHORED_CAPABILITIES
        .iter()
        .map(|&capability| (capability, adapter_for(capability)))
        .collect()
}

/// The branch point of a direct fork lineage: the ancestor's storage branch and
/// the read selector that reproduces its state at the fork.
struct BasePoint {
    storage_branch_id: BranchId,
    selector: ReadSelector,
    version: CommitVersion,
}

/// Derives the branch point for a direct fork lineage (M12C1): the target
/// forked from the source, or the source forked from the target, with an
/// intact generation edge. Any other relationship is rejected.
fn resolve_base_point(
    source: &BranchCatalogRecord,
    target: &BranchCatalogRecord,
) -> EngineResult<BasePoint> {
    if let Some(parent) = target.parent() {
        if parent.branch_id() == source.branch_id() && parent.generation() == source.generation() {
            return Ok(base_point_from(
                source,
                parent.fork_version(),
                parent.fork_timestamp(),
            ));
        }
    }
    if let Some(parent) = source.parent() {
        if parent.branch_id() == target.branch_id() && parent.generation() == target.generation() {
            return Ok(base_point_from(
                target,
                parent.fork_version(),
                parent.fork_timestamp(),
            ));
        }
    }
    Err(EngineError::invalid_input(
        "invalid_argument.engine.branch_point",
        "no branch point: the two branches are not in a direct fork lineage",
    ))
}

fn base_point_from(
    ancestor: &BranchCatalogRecord,
    fork_version: CommitVersion,
    fork_timestamp: Option<Timestamp>,
) -> BasePoint {
    let selector = fork_timestamp.map_or(
        ReadSelector::AtVersion(fork_version),
        ReadSelector::AtTimestamp,
    );
    BasePoint {
        storage_branch_id: ancestor.storage_branch_id(),
        selector,
        version: fork_version,
    }
}

/// Every entity of one capability in one space at a branch state, keyed by
/// identity, including tombstones (as `EntitySummary::Absent`) so a three-way
/// diff can see deletions.
fn entity_states(
    persistence: &mut StoragePersistence,
    storage_branch_id: BranchId,
    adapter: &dyn CapabilityBranchAdapter,
    space: &ProductSpace,
    selector: ReadSelector,
) -> EngineResult<BTreeMap<Vec<u8>, EntitySummary>> {
    let rows = persistence.scan_prefix(
        storage_branch_id,
        adapter.row_class(),
        adapter.space_prefix(space),
        selector,
        None,
    )?;
    let mut states = BTreeMap::new();
    for row in &rows {
        let entity = adapter.interpret_row(space, row)?;
        states.insert(entity.identity().to_vec(), entity.summary().clone());
    }
    Ok(states)
}

pub(crate) fn value_of(summary: Option<&EntitySummary>) -> Option<Vec<u8>> {
    match summary {
        Some(EntitySummary::Present(bytes)) => Some(bytes.clone()),
        _ => None,
    }
}

/// Whether a side's state differs from the branch-point state for an entity. An
/// entity absent from a side's map is absent on that side (its inherited rows
/// are visible in the scan, so a missing key is a genuine absence).
pub(crate) fn changed(side: Option<&EntitySummary>, base: Option<&EntitySummary>) -> bool {
    normalized(side) != normalized(base)
}

pub(crate) fn normalized(summary: Option<&EntitySummary>) -> &EntitySummary {
    summary.unwrap_or(&EntitySummary::Absent)
}

/// One entity's three-way state across a promotion's branch point, source, and
/// target — emitted for every identity that changed on at least one side since
/// the branch point. Shared by preview (which reports conflicts) and promotion
/// (which turns source changes into target mutations).
pub(crate) struct ThreeWayEntity {
    pub(crate) capability: ComparedCapability,
    pub(crate) space: ProductSpace,
    pub(crate) identity: Vec<u8>,
    pub(crate) base: Option<EntitySummary>,
    pub(crate) source: Option<EntitySummary>,
    pub(crate) target: Option<EntitySummary>,
}

/// Runs the three-way scan (branch point → source, branch point → target) over
/// every authored capability and space, returning the branch-point version and,
/// for each identity that changed on at least one side, its three summaries.
///
/// The branch point is derived from lineage (a direct fork edge in M12C1);
/// unrelated branches are rejected with `invalid_argument.engine.branch_point`.
pub(crate) fn three_way(
    persistence: &mut StoragePersistence,
    source: &BranchCatalogRecord,
    target: &BranchCatalogRecord,
) -> EngineResult<(CommitVersion, Vec<ThreeWayEntity>)> {
    let base = resolve_base_point(source, target)?;

    let mut spaces = registered_spaces(persistence, source)?;
    for space in registered_spaces(persistence, target)? {
        if !spaces.contains(&space) {
            spaces.push(space);
        }
    }
    spaces.sort();

    let adapters = capability_adapters();
    let mut entities = Vec::new();
    for space in &spaces {
        for (capability, adapter) in &adapters {
            // The three-way scan drives promotion and its preview, so it covers
            // only capabilities that support promotion. Comparison (compare.rs)
            // uses the same registry but includes every authored capability, so
            // compare-only capabilities such as event streams still diff.
            if adapter.derived_disposition() != DerivedDisposition::Authored
                || !adapter.supports_promotion()
            {
                continue;
            }
            let base_states = entity_states(
                persistence,
                base.storage_branch_id,
                adapter.as_ref(),
                space,
                base.selector,
            )?;
            let source_states = entity_states(
                persistence,
                source.storage_branch_id(),
                adapter.as_ref(),
                space,
                ReadSelector::Latest,
            )?;
            let target_states = entity_states(
                persistence,
                target.storage_branch_id(),
                adapter.as_ref(),
                space,
                ReadSelector::Latest,
            )?;

            let mut identities: BTreeSet<&Vec<u8>> = BTreeSet::new();
            identities.extend(source_states.keys());
            identities.extend(target_states.keys());
            identities.extend(base_states.keys());

            for identity in identities {
                let base_value = base_states.get(identity);
                let source_value = source_states.get(identity);
                let target_value = target_states.get(identity);

                if !(changed(source_value, base_value) || changed(target_value, base_value)) {
                    continue;
                }

                entities.push(ThreeWayEntity {
                    capability: *capability,
                    space: space.clone(),
                    identity: identity.clone(),
                    base: base_value.cloned(),
                    source: source_value.cloned(),
                    target: target_value.cloned(),
                });
            }
        }
    }

    Ok((base.version, entities))
}

/// Previews promoting `source` into `target`: derives the branch point and runs
/// the three-way comparison, reporting conflicts without mutating either branch.
pub(crate) fn preview_branches(
    persistence: &mut StoragePersistence,
    source: &BranchCatalogRecord,
    target: &BranchCatalogRecord,
    strategy: PromotionStrategy,
) -> EngineResult<BranchPreview> {
    let strategy_result = match strategy {
        PromotionStrategy::Strict => ConflictStrategyResult::Refused,
        PromotionStrategy::SourceWins => ConflictStrategyResult::SourceWins,
    };
    let (branch_point, entities) = three_way(persistence, source, target)?;

    let mut conflicts = Vec::new();
    for entity in &entities {
        let source_value = entity.source.as_ref();
        let target_value = entity.target.as_ref();
        let base_value = entity.base.as_ref();

        if !(changed(source_value, base_value) && changed(target_value, base_value)) {
            continue;
        }
        if normalized(source_value) == normalized(target_value) {
            continue; // both sides converged on the same change
        }

        let source_present = matches!(entity.source, Some(EntitySummary::Present(_)));
        let target_present = matches!(entity.target, Some(EntitySummary::Present(_)));
        let kind = if source_present && target_present {
            ConflictKind::ValueDivergence
        } else {
            ConflictKind::ModifyDeleteDivergence
        };

        conflicts.push(PreviewConflict::new(
            entity.capability,
            entity.space.clone(),
            entity.identity.clone(),
            value_of(source_value),
            value_of(target_value),
            kind,
            strategy_result,
        ));
    }

    Ok(BranchPreview::new(
        source.name().clone(),
        target.name().clone(),
        branch_point,
        strategy,
        conflicts,
    ))
}
