//! Cache/no-WAL commit execution.

use super::{
    admit_mutating_commit, validate_commit_conflicts, CommitBatch, CommitBatchKind,
    CommitBranchApplyTarget, CommitBranchGenerationGuard, CommitBranchGuardSet,
    CommitBranchReadViewConflictSource, CommitBranchRegistry, CommitDurabilityClass,
    CommitDurabilityMode, CommitFactAllocation, CommitFactAllocator, CommitMutationCounts,
    CommitOutcome, CommitRuntimeConfig, CommitRuntimeError, CommitRuntimeResult, CommitStamp,
    CommitTimelineEntry, CommitTimelineRows, CommitTimestampSource, CommitUnresolvedDurable,
    CommitUnresolvedDurableGate, CommitVisibilityFacts, CommitVisiblePublisher,
    ValidatedCommitBatch,
};
use crate::row::StorageRow;

#[derive(Debug)]
pub(crate) struct CommitCacheRuntime<'a, S, B, V> {
    config: &'a CommitRuntimeConfig,
    registry: &'a CommitBranchRegistry,
    guard_set: &'a CommitBranchGuardSet,
    allocator: &'a mut CommitFactAllocator<S>,
    branch: &'a mut B,
    visible: &'a mut V,
    durable_gate: &'a CommitUnresolvedDurableGate,
}

impl<'a, S, B, V> CommitCacheRuntime<'a, S, B, V> {
    pub(crate) fn new(
        config: &'a CommitRuntimeConfig,
        registry: &'a CommitBranchRegistry,
        guard_set: &'a CommitBranchGuardSet,
        allocator: &'a mut CommitFactAllocator<S>,
        branch: &'a mut B,
        visible: &'a mut V,
        durable_gate: &'a CommitUnresolvedDurableGate,
    ) -> Self {
        Self {
            config,
            registry,
            guard_set,
            allocator,
            branch,
            visible,
            durable_gate,
        }
    }
}

impl<S, B, V> CommitCacheRuntime<'_, S, B, V>
where
    S: CommitTimestampSource,
    B: CommitBranchApplyTarget,
    V: CommitVisiblePublisher,
{
    pub(crate) fn execute(
        &mut self,
        batch: CommitBatch,
        generation_guard: CommitBranchGenerationGuard,
    ) -> CommitRuntimeResult<CommitOutcome> {
        let batch = batch.validate(self.config)?;
        require_cache_mutating_batch(&batch)?;
        if batch.batch().branch_id() != self.branch.branch_id() {
            return Err(CommitRuntimeError::BranchMismatch {
                expected: batch.batch().branch_id(),
                actual: self.branch.branch_id(),
            });
        }
        // The unresolved durable gate is global for V1 visible-version safety,
        // so check it before target-branch admission or allocation.
        let mut unresolved_admission = self.durable_gate.admit_mutating_commit()?;

        // Keep this guard alive through conflict validation, apply, and
        // visible publication. That is the single-process safety window for
        // target-branch read-set/CAS checks.
        let _admission_guard =
            admit_mutating_commit(self.registry, self.guard_set, &batch, generation_guard)?;
        let current_visible_version = self.visible.visible_version();
        require_branch_not_ahead_of_visible(
            self.branch.max_commit_version(),
            current_visible_version,
        )?;

        let read_view = self.branch.capture_read_view()?;
        let conflict_source =
            CommitBranchReadViewConflictSource::new_at_version(&read_view, current_visible_version);
        validate_commit_conflicts(&batch, &conflict_source)?;

        let allocation = self.allocator.allocate_for_batch(&batch)?;
        let stamp = require_mutating_allocation(allocation)?;
        require_allocated_after_visible(stamp, current_visible_version)?;
        let (combined_rows, mutation_counts) = prepare_commit_rows(&batch, stamp, self.config)?;
        let facts = visible_cache_facts(stamp)?;
        self.branch
            .validate_committed_rows_before_apply(&combined_rows)?;

        self.branch
            .append_committed_rows_atomically(combined_rows)?;

        if let Err(error) = self.visible.publish_from_facts(facts) {
            // Cache mode has no WAL replay path. If publication fails after
            // apply, block all later mutations through the global gate so a
            // cross-branch visible-version advance cannot expose these rows by
            // side effect.
            //
            // Note: the applied row stays in branch state by design. A
            // same-branch `latest()` read after this failure still returns
            // the applied row — this preserves read-your-writes for cache
            // mode. The cross-branch leak is closed by the unresolved gate
            // recorded just below; OTHER branches cannot advance their
            // visible version past this in-flight row until the gate is
            // resolved. See `cache_applied_not_visible_row_is_visible_to_same_branch_read_your_writes`
            // for the pinning test.
            let reason = applied_not_visible_reason(&error);
            let unresolved = CommitUnresolvedDurable::applied_not_visible(
                stamp,
                CommitDurabilityClass::NotDurable,
                reason,
            )?;
            unresolved_admission.record_unresolved(unresolved)?;
            return Err(CommitRuntimeError::AppliedButNotVisible {
                branch_id: batch.batch().branch_id(),
                commit_version: stamp.commit_version(),
                reason,
            });
        }

        CommitOutcome::visible(
            batch.batch().branch_id(),
            stamp,
            CommitDurabilityClass::NotDurable,
            mutation_counts,
            facts,
        )
    }
}

pub(crate) fn prepare_commit_rows(
    batch: &ValidatedCommitBatch,
    stamp: CommitStamp,
    config: &CommitRuntimeConfig,
) -> CommitRuntimeResult<(Vec<StorageRow>, CommitMutationCounts)> {
    let user_rows = batch.stamp_user_rows(stamp)?;
    let timeline_entry = CommitTimelineEntry::from_stamp(stamp)?;
    let timeline_rows = CommitTimelineRows::from_entry(timeline_entry)?;
    let user_counts = CommitMutationCounts::from_validated_batch(batch)?;
    let mutation_counts = CommitMutationCounts::new(
        user_counts.puts(),
        user_counts.deletes(),
        CommitTimelineRows::timeline_row_count(),
        config,
    )?;

    let mut rows = Vec::with_capacity(
        user_rows
            .len()
            .saturating_add(CommitTimelineRows::timeline_row_count()),
    );
    rows.extend(user_rows);
    rows.extend(timeline_rows.rows().into_iter().cloned());
    Ok((rows, mutation_counts))
}

fn require_cache_mutating_batch(batch: &ValidatedCommitBatch) -> CommitRuntimeResult<()> {
    if batch.batch().kind() != CommitBatchKind::Mutating {
        return Err(CommitRuntimeError::InvalidBatch {
            reason: "cache commit executor requires mutating batch",
        });
    }
    if batch.batch().options().durability() != CommitDurabilityMode::Cache {
        return Err(CommitRuntimeError::DurabilityUnavailable {
            reason: "cache commit executor requires cache durability mode",
        });
    }
    Ok(())
}

fn require_mutating_allocation(
    allocation: CommitFactAllocation,
) -> CommitRuntimeResult<CommitStamp> {
    match allocation {
        CommitFactAllocation::Mutating { stamp, .. } => Ok(stamp),
        CommitFactAllocation::ReadOnly { .. } => Err(CommitRuntimeError::InvalidCommitState {
            reason: "cache commit executor expected mutating allocation",
        }),
    }
}

fn require_allocated_after_visible(
    stamp: CommitStamp,
    current_visible_version: strata_core_next::CommitVersion,
) -> CommitRuntimeResult<()> {
    if stamp.commit_version() <= current_visible_version {
        return Err(CommitRuntimeError::InvalidCommitState {
            reason: "allocated commit version must be greater than current visible version",
        });
    }
    Ok(())
}

fn require_branch_not_ahead_of_visible(
    branch_max_commit_version: Option<strata_core_next::CommitVersion>,
    current_visible_version: strata_core_next::CommitVersion,
) -> CommitRuntimeResult<()> {
    // The V1 visible-version tracker is global, but this executor owns only the
    // target branch state. Cross-branch applied-not-visible rows are excluded by
    // the global unresolved gate; this local check fails closed for the branch
    // this commit can actually expose.
    if branch_max_commit_version.is_some_and(|version| version > current_visible_version) {
        return Err(CommitRuntimeError::InvalidCommitState {
            reason: "branch has applied rows above current visible version",
        });
    }
    Ok(())
}

fn visible_cache_facts(stamp: CommitStamp) -> CommitRuntimeResult<CommitVisibilityFacts> {
    CommitVisibilityFacts::new(
        Some(stamp.commit_version()),
        None,
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
    )
}

fn applied_not_visible_reason(error: &CommitRuntimeError) -> &'static str {
    match error {
        CommitRuntimeError::InvalidVisibilityFacts { reason }
        | CommitRuntimeError::InvalidCommitState { reason } => reason,
        _ => "visible publication failed after branch apply",
    }
}
