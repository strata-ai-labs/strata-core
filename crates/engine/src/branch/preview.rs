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
use crate::data::json::JsonBranchAdapter;
use crate::data::kv::{KvBranchAdapter, ProductSpace};
use crate::diagnostics::{EngineError, EngineResult};
use crate::persistence::{ReadSelector, StoragePersistence};

/// The capabilities previewed today, in report order, each with its adapter.
fn capability_adapters() -> Vec<(ComparedCapability, Box<dyn CapabilityBranchAdapter>)> {
    vec![
        (ComparedCapability::KeyValue, Box::new(KvBranchAdapter)),
        (ComparedCapability::Json, Box::new(JsonBranchAdapter)),
    ]
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

fn value_of(summary: Option<&EntitySummary>) -> Option<Vec<u8>> {
    match summary {
        Some(EntitySummary::Present(bytes)) => Some(bytes.clone()),
        _ => None,
    }
}

/// Whether a side's state differs from the branch-point state for an entity. An
/// entity absent from a side's map is absent on that side (its inherited rows
/// are visible in the scan, so a missing key is a genuine absence).
fn changed(side: Option<&EntitySummary>, base: Option<&EntitySummary>) -> bool {
    normalized(side) != normalized(base)
}

fn normalized(summary: Option<&EntitySummary>) -> &EntitySummary {
    summary.unwrap_or(&EntitySummary::Absent)
}

/// Previews promoting `source` into `target`: derives the branch point and runs
/// the three-way comparison, reporting conflicts without mutating either branch.
pub(crate) fn preview_branches(
    persistence: &mut StoragePersistence,
    source: &BranchCatalogRecord,
    target: &BranchCatalogRecord,
    strategy: PromotionStrategy,
) -> EngineResult<BranchPreview> {
    let base = resolve_base_point(source, target)?;
    let strategy_result = match strategy {
        PromotionStrategy::Strict => ConflictStrategyResult::Refused,
        PromotionStrategy::SourceWins => ConflictStrategyResult::SourceWins,
    };

    let mut spaces = registered_spaces(persistence, source)?;
    for space in registered_spaces(persistence, target)? {
        if !spaces.contains(&space) {
            spaces.push(space);
        }
    }
    spaces.sort();

    let adapters = capability_adapters();
    let mut conflicts = Vec::new();
    for space in &spaces {
        for (capability, adapter) in &adapters {
            if adapter.derived_disposition() != DerivedDisposition::Authored {
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

                let source_changed = changed(source_value, base_value);
                let target_changed = changed(target_value, base_value);
                if !(source_changed && target_changed) {
                    continue;
                }
                if normalized(source_value) == normalized(target_value) {
                    continue; // both sides converged on the same change
                }

                let source_present = matches!(source_value, Some(EntitySummary::Present(_)));
                let target_present = matches!(target_value, Some(EntitySummary::Present(_)));
                let kind = if source_present && target_present {
                    ConflictKind::ValueDivergence
                } else {
                    ConflictKind::ModifyDeleteDivergence
                };

                conflicts.push(PreviewConflict::new(
                    *capability,
                    space.clone(),
                    identity.clone(),
                    value_of(source_value),
                    value_of(target_value),
                    kind,
                    strategy_result,
                ));
            }
        }
    }

    Ok(BranchPreview::new(
        source.name().clone(),
        target.name().clone(),
        base.version,
        strategy,
        conflicts,
    ))
}
