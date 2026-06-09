//! Durable WAL-backed commit execution.

use super::cache::prepare_commit_rows;
use super::{
    admit_mutating_commit, commit_conflict_validation_needs_source, validate_commit_conflicts,
    validate_commit_conflicts_without_source, CommitBatch, CommitBatchKind,
    CommitBranchGenerationGuard, CommitBranchGuardSet, CommitBranchReadViewConflictSource,
    CommitBranchRegistry, CommitDurabilityClass, CommitDurabilityMode, CommitFactAllocation,
    CommitFactAllocator, CommitLowerLayer, CommitOutcome, CommitRuntimeConfig, CommitRuntimeError,
    CommitRuntimeResult, CommitStamp, CommitTimestampSource, CommitUnresolvedDurable,
    CommitUnresolvedDurableGate, CommitVisibilityFacts, ValidatedCommitBatch,
    VisibleVersionPublish, VisibleVersionTracker,
};
use crate::branch::read::BranchReadView;
use crate::branch::state::BranchLocalState;
use crate::config::mode::DurabilityPolicy;
use crate::format::{WalCommitPayload, WalRecord};
use crate::observability::perf_trace;
use crate::row::StorageRow;
use crate::service::{WalAppend, WalService, WalServiceError};
use strata_core_next::{BranchId, CommitVersion};

#[derive(Debug)]
pub(crate) struct CommitDurableRuntime<'a, S, W, B = BranchLocalState, V = VisibleVersionTracker> {
    config: &'a CommitRuntimeConfig,
    registry: &'a CommitBranchRegistry,
    guard_set: &'a CommitBranchGuardSet,
    allocator: &'a mut CommitFactAllocator<S>,
    branch: &'a mut B,
    visible: &'a mut V,
    wal: &'a mut W,
    durable_gate: &'a CommitUnresolvedDurableGate,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CommitWalAppendFacts {
    segment_id: u64,
    start_offset: u64,
    bytes_written: u64,
    dirty_bytes: u64,
    forced_durable: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct CommitWalAppendError {
    durability_uncertain: bool,
    error: CommitRuntimeError,
}

pub(crate) trait CommitWalAppender {
    fn durability_policy(&self) -> DurabilityPolicy;
    fn append_commit_record(
        &mut self,
        record: &WalRecord,
    ) -> Result<CommitWalAppendFacts, CommitWalAppendError>;
}

pub(crate) trait CommitBranchApplyTarget {
    fn branch_id(&self) -> BranchId;
    fn max_commit_version(&self) -> Option<CommitVersion>;
    fn capture_read_view(&self) -> CommitRuntimeResult<BranchReadView>;
    fn validate_committed_rows_before_apply(
        &self,
        _rows: &[StorageRow],
    ) -> CommitRuntimeResult<()> {
        Ok(())
    }
    fn append_committed_rows_atomically(
        &mut self,
        rows: Vec<StorageRow>,
    ) -> CommitRuntimeResult<()>;
}

pub(crate) trait CommitVisiblePublisher {
    fn visible_version(&self) -> CommitVersion;
    fn publish_from_facts(
        &mut self,
        facts: CommitVisibilityFacts,
    ) -> CommitRuntimeResult<VisibleVersionPublish>;
}

impl CommitBranchApplyTarget for BranchLocalState {
    fn branch_id(&self) -> BranchId {
        BranchLocalState::branch_id(self)
    }

    fn max_commit_version(&self) -> Option<CommitVersion> {
        BranchLocalState::max_commit_version(self)
    }

    fn capture_read_view(&self) -> CommitRuntimeResult<BranchReadView> {
        BranchLocalState::capture_read_view(self).map_err(|source| {
            CommitRuntimeError::lower_layer_with(
                CommitLowerLayer::BranchRuntime,
                "branch read view capture failed",
                source,
            )
        })
    }

    fn validate_committed_rows_before_apply(
        &self,
        _rows: &[StorageRow],
    ) -> CommitRuntimeResult<()> {
        Ok(())
    }

    fn append_committed_rows_atomically(
        &mut self,
        rows: Vec<StorageRow>,
    ) -> CommitRuntimeResult<()> {
        BranchLocalState::append_committed_rows_atomically(self, rows)
            .map(|_| ())
            .map_err(|source| {
                CommitRuntimeError::lower_layer_with(
                    CommitLowerLayer::BranchRuntime,
                    "branch state rejected commit rows",
                    source,
                )
            })
    }
}

impl CommitVisiblePublisher for VisibleVersionTracker {
    fn visible_version(&self) -> CommitVersion {
        VisibleVersionTracker::visible_version(*self)
    }

    fn publish_from_facts(
        &mut self,
        facts: CommitVisibilityFacts,
    ) -> CommitRuntimeResult<VisibleVersionPublish> {
        VisibleVersionTracker::publish_from_facts(self, facts)
    }
}

impl<'a, S, W, B, V> CommitDurableRuntime<'a, S, W, B, V> {
    pub(crate) fn new(
        config: &'a CommitRuntimeConfig,
        registry: &'a CommitBranchRegistry,
        guard_set: &'a CommitBranchGuardSet,
        allocator: &'a mut CommitFactAllocator<S>,
        branch: &'a mut B,
        visible: &'a mut V,
        wal: &'a mut W,
        durable_gate: &'a CommitUnresolvedDurableGate,
    ) -> Self {
        Self {
            config,
            registry,
            guard_set,
            allocator,
            branch,
            visible,
            wal,
            durable_gate,
        }
    }
}

impl<S, W, B, V> CommitDurableRuntime<'_, S, W, B, V>
where
    S: CommitTimestampSource,
    W: CommitWalAppender,
    B: CommitBranchApplyTarget,
    V: CommitVisiblePublisher,
{
    pub(crate) fn execute(
        &mut self,
        batch: CommitBatch,
        generation_guard: CommitBranchGenerationGuard,
    ) -> CommitRuntimeResult<CommitOutcome> {
        let batch = batch.validate(self.config)?;
        let (required_policy, durability) = require_durable_mutating_batch(&batch)?;
        let branch_id = batch.batch().branch_id();
        if self.wal.durability_policy() != required_policy {
            return Err(CommitRuntimeError::DurabilityUnavailable {
                reason: "durable commit executor requires matching WAL durability policy",
            });
        }
        if branch_id != self.branch.branch_id() {
            return Err(CommitRuntimeError::BranchMismatch {
                expected: branch_id,
                actual: self.branch.branch_id(),
            });
        }
        // The unresolved durable gate is global for V1 visible-version safety,
        // so check it before target-branch admission, allocation, or WAL work.
        let mut unresolved_admission = self.durable_gate.admit_mutating_commit()?;

        // Keep this guard alive through WAL append, apply, gate recording, and
        // visible publication. Same-branch commits cannot observe a stale
        // conflict window while this token is held.
        let _admission_guard =
            admit_mutating_commit(self.registry, self.guard_set, &batch, generation_guard)?;
        let current_visible_version = self.visible.visible_version();
        require_branch_not_ahead_of_visible(
            self.branch.max_commit_version(),
            current_visible_version,
        )?;

        if commit_conflict_validation_needs_source(&batch) {
            let read_view = self.branch.capture_read_view()?;
            perf_trace::record_conflict_source_built();
            let conflict_source = CommitBranchReadViewConflictSource::new_at_version(
                &read_view,
                current_visible_version,
            );
            validate_commit_conflicts(&batch, &conflict_source)?;
        } else {
            validate_commit_conflicts_without_source(&batch)?;
        }

        let allocation = self.allocator.allocate_for_batch(&batch)?;
        let stamp = require_mutating_allocation(allocation)?;
        require_allocated_after_visible(stamp, current_visible_version)?;
        let (combined_rows, mutation_counts) = prepare_commit_rows(batch, stamp, self.config)?;
        self.branch
            .validate_committed_rows_before_apply(&combined_rows)?;
        let record = build_wal_record(stamp, combined_rows)?;
        let append = self
            .wal
            .append_commit_record(&record)
            .map_err(|error| error.into_commit_error(branch_id, stamp.commit_version()))?;
        require_append_satisfies_policy(
            required_policy,
            append,
            branch_id,
            stamp.commit_version(),
        )?;

        let combined_rows = record.into_commit_payload().into_rows();
        if let Err(source) = self.branch.append_committed_rows_atomically(combined_rows) {
            let reason = "branch state rejected durable commit rows after WAL append";
            unresolved_admission.record_unresolved(
                CommitUnresolvedDurable::durable_not_applied_with_facts(stamp, durability, reason)?,
            )?;
            return Err(CommitRuntimeError::durable_but_not_visible_with(
                branch_id,
                stamp.commit_version(),
                reason,
                source,
            ));
        }

        let facts = visible_durable_facts(stamp)?;
        if let Err(error) = self.visible.publish_from_facts(facts) {
            let reason = durable_visible_reason(&error);
            unresolved_admission.record_unresolved(
                CommitUnresolvedDurable::applied_not_visible(stamp, durability, reason)?,
            )?;
            return Err(CommitRuntimeError::durable_but_not_visible_with(
                branch_id,
                stamp.commit_version(),
                reason,
                error,
            ));
        }

        CommitOutcome::visible(branch_id, stamp, durability, mutation_counts, facts)
    }
}

impl CommitWalAppendFacts {
    pub(crate) const fn new(
        segment_id: u64,
        start_offset: u64,
        bytes_written: u64,
        dirty_bytes: u64,
        forced_durable: bool,
    ) -> Self {
        Self {
            segment_id,
            start_offset,
            bytes_written,
            dirty_bytes,
            forced_durable,
        }
    }

    pub(crate) fn from_wal_append(append: &WalAppend) -> Self {
        Self::new(
            append.segment_id(),
            append.start_offset(),
            append.bytes_written(),
            append.dirty_bytes(),
            append.forced_durable(),
        )
    }

    pub(crate) const fn forced_durable(self) -> bool {
        self.forced_durable
    }
}

impl CommitWalAppendError {
    pub(crate) const fn clean(error: CommitRuntimeError) -> Self {
        Self {
            durability_uncertain: false,
            error,
        }
    }

    pub(crate) const fn uncertain(error: CommitRuntimeError) -> Self {
        Self {
            durability_uncertain: true,
            error,
        }
    }

    fn into_commit_error(
        self,
        branch_id: strata_core_next::BranchId,
        version: strata_core_next::CommitVersion,
    ) -> CommitRuntimeError {
        if self.durability_uncertain {
            CommitRuntimeError::durability_uncertain_with(
                branch_id,
                version,
                "WAL append durability is uncertain",
                self.error,
            )
        } else {
            self.error
        }
    }
}

impl CommitWalAppender for WalService<'_> {
    fn durability_policy(&self) -> DurabilityPolicy {
        self.durability_policy()
    }

    fn append_commit_record(
        &mut self,
        record: &WalRecord,
    ) -> Result<CommitWalAppendFacts, CommitWalAppendError> {
        self.append(record)
            .map(|append| CommitWalAppendFacts::from_wal_append(&append))
            .map_err(map_wal_service_append_error)
    }
}

fn require_durable_mutating_batch(
    batch: &ValidatedCommitBatch,
) -> CommitRuntimeResult<(DurabilityPolicy, CommitDurabilityClass)> {
    if batch.batch().kind() != CommitBatchKind::Mutating {
        return Err(CommitRuntimeError::InvalidBatch {
            reason: "durable commit executor requires mutating batch",
        });
    }
    match batch.batch().options().durability() {
        CommitDurabilityMode::Standard => {
            Ok((DurabilityPolicy::Standard, CommitDurabilityClass::Standard))
        }
        CommitDurabilityMode::Always => {
            Ok((DurabilityPolicy::Always, CommitDurabilityClass::Always))
        }
        CommitDurabilityMode::Cache => Err(CommitRuntimeError::DurabilityUnavailable {
            reason: "durable commit executor requires durable mode",
        }),
    }
}

fn require_mutating_allocation(
    allocation: CommitFactAllocation,
) -> CommitRuntimeResult<CommitStamp> {
    match allocation {
        CommitFactAllocation::Mutating { stamp, .. } => Ok(stamp),
        CommitFactAllocation::ReadOnly { .. } => Err(CommitRuntimeError::InvalidCommitState {
            reason: "durable commit executor expected mutating allocation",
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
    // target branch state. Cross-branch applied-not-visible durable rows are
    // excluded by the global unresolved durable gate and recovery ownership;
    // this local check fails closed for the branch this commit can expose.
    if branch_max_commit_version.is_some_and(|version| version > current_visible_version) {
        return Err(CommitRuntimeError::InvalidCommitState {
            reason: "branch has applied rows above current visible version",
        });
    }
    Ok(())
}

fn build_wal_record(
    stamp: CommitStamp,
    rows: Vec<crate::row::StorageRow>,
) -> CommitRuntimeResult<WalRecord> {
    let payload = WalCommitPayload::new(rows).map_err(|source| {
        CommitRuntimeError::lower_layer_with(
            CommitLowerLayer::WalFormat,
            "WAL commit payload rejected durable rows",
            source,
        )
    })?;
    WalRecord::new(
        stamp.commit_version(),
        stamp.branch_id(),
        stamp.commit_timestamp(),
        payload,
    )
    .map_err(|source| {
        CommitRuntimeError::lower_layer_with(
            CommitLowerLayer::WalFormat,
            "WAL record rejected durable row facts",
            source,
        )
    })
}

fn require_append_satisfies_policy(
    policy: DurabilityPolicy,
    append: CommitWalAppendFacts,
    branch_id: strata_core_next::BranchId,
    version: strata_core_next::CommitVersion,
) -> CommitRuntimeResult<()> {
    if policy == DurabilityPolicy::Always && !append.forced_durable() {
        return Err(CommitRuntimeError::DurabilityUncertain {
            branch_id,
            commit_version: version,
            reason: "always durability requires a forced WAL append",
            source: None,
        });
    }
    Ok(())
}

fn visible_durable_facts(stamp: CommitStamp) -> CommitRuntimeResult<CommitVisibilityFacts> {
    CommitVisibilityFacts::new(
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
        Some(stamp.commit_version()),
    )
}

fn durable_visible_reason(error: &CommitRuntimeError) -> &'static str {
    match error {
        CommitRuntimeError::InvalidVisibilityFacts { reason }
        | CommitRuntimeError::InvalidCommitState { reason } => reason,
        _ => "visible publication failed after durable branch apply",
    }
}

fn map_wal_service_append_error(error: WalServiceError) -> CommitWalAppendError {
    if error.is_writer_halted_append_failure() {
        CommitWalAppendError::clean(CommitRuntimeError::DurabilityUnavailable {
            reason: "WAL writer is halted; reopen before appending",
        })
    } else if is_uncertain_wal_append_error(&error) {
        CommitWalAppendError::uncertain(CommitRuntimeError::lower_layer_with(
            CommitLowerLayer::WalService,
            "WAL append durability is uncertain",
            error,
        ))
    } else {
        CommitWalAppendError::clean(CommitRuntimeError::lower_layer_with(
            CommitLowerLayer::WalService,
            "WAL append failed before commit visibility",
            error,
        ))
    }
}

fn is_uncertain_wal_append_error(error: &WalServiceError) -> bool {
    error.is_durability_uncertain_append_failure()
}
