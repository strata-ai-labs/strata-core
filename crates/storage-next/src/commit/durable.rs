//! Durable WAL-backed commit execution.

use super::{
    admit_mutating_commit, validate_commit_conflicts, CacheCommitRows, CommitBatch,
    CommitBatchKind, CommitBranchGenerationGuard, CommitBranchGuardSet,
    CommitBranchReadViewConflictSource, CommitBranchRegistry, CommitDurabilityClass,
    CommitDurabilityMode, CommitFactAllocation, CommitFactAllocator, CommitLowerLayer,
    CommitOutcome, CommitRuntimeConfig, CommitRuntimeError, CommitRuntimeResult, CommitStamp,
    CommitTimestampSource, CommitVisibilityFacts, ValidatedCommitBatch, VisibleVersionTracker,
};
use crate::branch::BranchLocalState;
use crate::config::mode::DurabilityPolicy;
use crate::format::{WalCommitPayload, WalRecord};
use crate::service::{WalAppend, WalService, WalServiceError};

#[derive(Debug)]
pub(crate) struct CommitDurableRuntime<'a, S, W> {
    config: &'a CommitRuntimeConfig,
    registry: &'a CommitBranchRegistry,
    guard_set: &'a CommitBranchGuardSet,
    allocator: &'a mut CommitFactAllocator<S>,
    branch: &'a mut BranchLocalState,
    visible: &'a mut VisibleVersionTracker,
    wal: &'a mut W,
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
    kind: CommitWalAppendErrorKind,
    error: CommitRuntimeError,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CommitWalAppendErrorKind {
    Clean,
    Uncertain,
}

pub(crate) trait CommitWalAppender {
    fn durability_policy(&self) -> DurabilityPolicy;
    fn append_commit_record(
        &mut self,
        record: &WalRecord,
    ) -> Result<CommitWalAppendFacts, CommitWalAppendError>;
}

impl<'a, S, W> CommitDurableRuntime<'a, S, W> {
    pub(crate) fn new(
        config: &'a CommitRuntimeConfig,
        registry: &'a CommitBranchRegistry,
        guard_set: &'a CommitBranchGuardSet,
        allocator: &'a mut CommitFactAllocator<S>,
        branch: &'a mut BranchLocalState,
        visible: &'a mut VisibleVersionTracker,
        wal: &'a mut W,
    ) -> Self {
        Self {
            config,
            registry,
            guard_set,
            allocator,
            branch,
            visible,
            wal,
        }
    }
}

impl<S: CommitTimestampSource, W: CommitWalAppender> CommitDurableRuntime<'_, S, W> {
    pub(crate) fn execute(
        &mut self,
        batch: CommitBatch,
        generation_guard: CommitBranchGenerationGuard,
    ) -> CommitRuntimeResult<CommitOutcome> {
        let batch = batch.validate(self.config)?;
        let (required_policy, durability) = require_durable_mutating_batch(&batch)?;
        if self.wal.durability_policy() != required_policy {
            return Err(CommitRuntimeError::DurabilityUnavailable {
                reason: "durable commit executor requires matching WAL durability policy",
            });
        }
        if batch.batch().branch_id() != self.branch.branch_id() {
            return Err(CommitRuntimeError::BranchMismatch {
                expected: batch.batch().branch_id(),
                actual: self.branch.branch_id(),
            });
        }

        let _admission_guard =
            admit_mutating_commit(self.registry, self.guard_set, &batch, generation_guard)?;
        let current_visible_version = self.visible.visible_version();
        require_branch_not_ahead_of_visible(
            self.branch.max_commit_version(),
            current_visible_version,
        )?;

        let read_view = self.branch.capture_read_view().map_err(|source| {
            CommitRuntimeError::lower_layer_with(
                CommitLowerLayer::BranchRuntime,
                "branch read view capture failed",
                source,
            )
        })?;
        let conflict_source =
            CommitBranchReadViewConflictSource::new_at_version(&read_view, current_visible_version);
        validate_commit_conflicts(&batch, &conflict_source)?;

        let allocation = self.allocator.allocate_for_batch(&batch)?;
        let stamp = require_mutating_allocation(allocation)?;
        require_allocated_after_visible(stamp, current_visible_version)?;
        let rows = CacheCommitRows::prepare(&batch, stamp, self.config)?;
        let combined_rows = rows.combined_rows();
        let record = build_wal_record(stamp, combined_rows.clone())?;
        let append = self.wal.append_commit_record(&record).map_err(|error| {
            error.into_commit_error(batch.batch().branch_id(), stamp.commit_version())
        })?;
        require_append_satisfies_policy(
            required_policy,
            append,
            batch.batch().branch_id(),
            stamp.commit_version(),
        )?;

        self.branch
            .append_committed_rows_atomically(combined_rows)
            .map_err(|source| {
                CommitRuntimeError::durable_but_not_visible_with(
                    batch.batch().branch_id(),
                    stamp.commit_version(),
                    "branch state rejected durable commit rows after WAL append",
                    source,
                )
            })?;

        let facts = visible_durable_facts(stamp)?;
        self.visible.publish_from_facts(facts).map_err(|error| {
            CommitRuntimeError::durable_but_not_visible_with(
                batch.batch().branch_id(),
                stamp.commit_version(),
                durable_visible_reason(&error),
                error,
            )
        })?;

        CommitOutcome::visible(
            batch.batch().branch_id(),
            stamp,
            durability,
            rows.mutation_counts(),
            facts,
        )
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
            kind: CommitWalAppendErrorKind::Clean,
            error,
        }
    }

    pub(crate) const fn uncertain(error: CommitRuntimeError) -> Self {
        Self {
            kind: CommitWalAppendErrorKind::Uncertain,
            error,
        }
    }

    fn into_commit_error(
        self,
        branch_id: strata_core_next::BranchId,
        version: strata_core_next::CommitVersion,
    ) -> CommitRuntimeError {
        match self.kind {
            CommitWalAppendErrorKind::Clean => self.error,
            CommitWalAppendErrorKind::Uncertain => CommitRuntimeError::durability_uncertain_with(
                branch_id,
                version,
                "WAL append durability is uncertain",
                self.error,
            ),
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
