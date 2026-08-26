//! Branch promotion (merge) — the engine planner behind `BranchService::promote`.
//!
//! Promotion applies a source branch's completed changes into a target branch as
//! a single atomic commit (contract §Promotion). This module derives the plan:
//! it reuses the three-way scan behind preview, then turns every source-side
//! change since the branch point into a target mutation, resolving conflicts per
//! the selected strategy. It does not commit — the branch service coordinates
//! the single target commit and records the promotion lineage.
//!
//! M12D1 covers KV and JSON in cache/durable happy paths; the recoverable
//! workflow intent and crash/reopen recovery land in M12D2.

use strata_core::CommitVersion;

use crate::api::{
    ComparedCapability, ConflictKind, ConflictStrategyResult, PreviewConflict, PromotedEntity,
    PromotionStrategy,
};
use crate::branch::adapter::EntitySummary;
use crate::branch::catalog::BranchCatalogRecord;
use crate::branch::preview::{adapter_for, changed, normalized, three_way, value_of};
use crate::control::space::{registered_spaces, registration_mutations_for_many};
use crate::data::vector::plan_collection_promotion;
use crate::diagnostics::EngineResult;
use crate::persistence::{RowMutation, StoragePersistence};

/// The mutations and reporting a promotion of `source` into `target` would
/// produce. `mutations` apply every winning source change onto the target in one
/// commit; `applied`/`deleted` report them for the outcome; `conflicts` records
/// entities that diverged on both sides (empty when the merge is clean).
pub(crate) struct PromotionPlan {
    pub(crate) branch_point: CommitVersion,
    pub(crate) mutations: Vec<RowMutation>,
    pub(crate) applied: Vec<PromotedEntity>,
    pub(crate) deleted: Vec<PromotedEntity>,
    pub(crate) conflicts: Vec<PreviewConflict>,
}

/// Plans promoting `source` into `target` under `strategy`, without mutating
/// either branch. The caller decides whether to commit: a strict strategy
/// refuses when `conflicts` is non-empty; otherwise `mutations` are the target
/// write-set (empty when the merge is a no-op).
pub(crate) fn plan_promotion(
    persistence: &mut StoragePersistence,
    source: &BranchCatalogRecord,
    target: &BranchCatalogRecord,
    strategy: PromotionStrategy,
) -> EngineResult<PromotionPlan> {
    let strategy_result = match strategy {
        PromotionStrategy::Strict => ConflictStrategyResult::Refused,
        PromotionStrategy::SourceWins => ConflictStrategyResult::SourceWins,
    };
    let (branch_point, entities) = three_way(persistence, source, target)?;
    let target_branch = target.storage_branch_id();

    let mut mutations = Vec::new();
    let mut applied = Vec::new();
    let mut deleted = Vec::new();
    let mut conflicts = Vec::new();

    for entity in entities {
        let source_value = entity.source.as_ref();
        let target_value = entity.target.as_ref();
        let base_value = entity.base.as_ref();

        // Only source-side changes propagate; a target-only change is kept.
        if !changed(source_value, base_value) {
            continue;
        }
        // Source and target already agree (both converged on the same change) —
        // nothing to write. Keeps a clean promotion a no-op with no commit.
        if normalized(source_value) == normalized(target_value) {
            continue;
        }

        // Both sides changed this identity to different states: a conflict.
        if changed(target_value, base_value) {
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

        // Apply the source's resolved state onto the target.
        let resolved = normalized(source_value);
        let adapter = adapter_for(entity.capability);
        mutations.push(adapter.write_mutation(
            target_branch,
            &entity.space,
            &entity.identity,
            resolved,
        ));
        record_applied_or_deleted(
            &mut applied,
            &mut deleted,
            entity.capability,
            &entity,
            resolved,
        );
    }

    // Carry source-only spaces so promoted rows land in a space the target's
    // catalog registers, rather than orphaned outside it — a visible data change
    // must carry the branch-control metadata that explains it (contract Binding
    // Decision #8). The registration commits atomically with the data mutations,
    // so a strict refusal (which never commits the plan) leaves the target
    // untouched, spaces included.
    let source_spaces = registered_spaces(persistence, source)?;
    mutations.extend(registration_mutations_for_many(
        persistence,
        target,
        &source_spaces,
    )?);

    // Carry vector collection configs so promoted vectors are usable on the
    // target rather than orphaned behind a missing collection config (contract
    // Vector minimum). An incompatible dimension/metric surfaces as a conflict.
    let (collection_mutations, collection_conflicts) =
        plan_collection_promotion(persistence, source, target, &source_spaces, strategy_result)?;
    mutations.extend(collection_mutations);
    conflicts.extend(collection_conflicts);

    Ok(PromotionPlan {
        branch_point,
        mutations,
        applied,
        deleted,
        conflicts,
    })
}

fn record_applied_or_deleted(
    applied: &mut Vec<PromotedEntity>,
    deleted: &mut Vec<PromotedEntity>,
    capability: ComparedCapability,
    entity: &crate::branch::preview::ThreeWayEntity,
    resolved: &EntitySummary,
) {
    match resolved {
        EntitySummary::Present(value) => applied.push(PromotedEntity::new(
            capability,
            entity.space.clone(),
            entity.identity.clone(),
            Some(value.clone()),
        )),
        EntitySummary::Absent => deleted.push(PromotedEntity::new(
            capability,
            entity.space.clone(),
            entity.identity.clone(),
            None,
        )),
    }
}
