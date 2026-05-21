//! Commit outcome and read-only diagnostic facts.

use super::{
    CommitBatchKind, CommitDurabilityClass, CommitMutation, CommitPhase, CommitReadOnlyDiagnostics,
    CommitRuntimeConfig, CommitRuntimeError, CommitRuntimeResult, CommitStamp,
    CommitVisibilityFacts, ValidatedCommitBatch, VisibleVersionTracker,
};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CommitReadSnapshot {
    branch_id: BranchId,
    visible_version: CommitVersion,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct CommitMutationCounts {
    puts: usize,
    deletes: usize,
    timeline_rows: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommitOutcomeKind {
    ReadOnly,
    Visible,
    NotVisible,
    DurableButNotVisible,
    Replay,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CommitOutcome {
    branch_id: BranchId,
    kind: CommitOutcomeKind,
    phase: CommitPhase,
    durability: CommitDurabilityClass,
    commit_version: Option<CommitVersion>,
    commit_timestamp: Option<Timestamp>,
    mutation_counts: CommitMutationCounts,
    visibility_facts: CommitVisibilityFacts,
    read_snapshot: Option<CommitReadSnapshot>,
}

impl CommitReadSnapshot {
    pub(crate) const fn new(branch_id: BranchId, visible_version: CommitVersion) -> Self {
        Self {
            branch_id,
            visible_version,
        }
    }

    pub(crate) const fn branch_id(self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn visible_version(self) -> CommitVersion {
        self.visible_version
    }
}

impl CommitMutationCounts {
    const fn from_parts_unchecked(puts: usize, deletes: usize, timeline_rows: usize) -> Self {
        Self {
            puts,
            deletes,
            timeline_rows,
        }
    }

    pub(crate) fn new(
        puts: usize,
        deletes: usize,
        timeline_rows: usize,
        config: &CommitRuntimeConfig,
    ) -> CommitRuntimeResult<Self> {
        config.validate()?;
        let mutation_rows =
            puts.checked_add(deletes)
                .ok_or(CommitRuntimeError::InvalidCommitState {
                    reason: "mutation count overflow",
                })?;
        let total_rows = mutation_rows.checked_add(timeline_rows).ok_or(
            CommitRuntimeError::InvalidCommitState {
                reason: "commit row count overflow",
            },
        )?;
        if mutation_rows > config.max_mutations_per_batch() {
            return Err(CommitRuntimeError::InvalidBatch {
                reason: "mutation count exceeds configured limit",
            });
        }
        if total_rows > config.max_commit_rows_per_batch() {
            return Err(CommitRuntimeError::InvalidBatch {
                reason: "commit row count exceeds configured limit",
            });
        }
        Ok(Self::from_parts_unchecked(puts, deletes, timeline_rows))
    }

    pub(crate) const fn from_replayed_rows(
        puts: usize,
        deletes: usize,
        timeline_rows: usize,
    ) -> Self {
        Self::from_parts_unchecked(puts, deletes, timeline_rows)
    }

    pub(crate) const fn read_only() -> Self {
        Self::from_parts_unchecked(0, 0, 0)
    }

    pub(crate) fn from_validated_batch(batch: &ValidatedCommitBatch) -> CommitRuntimeResult<Self> {
        if batch.batch().kind() != CommitBatchKind::Mutating {
            return Ok(Self::read_only());
        }

        let mut puts = 0usize;
        let mut deletes = 0usize;
        for mutation in batch.batch().mutations() {
            match mutation {
                CommitMutation::Put { .. } => {
                    puts = puts
                        .checked_add(1)
                        .ok_or(CommitRuntimeError::InvalidCommitState {
                            reason: "put count overflow",
                        })?;
                }
                CommitMutation::Delete { .. } => {
                    deletes =
                        deletes
                            .checked_add(1)
                            .ok_or(CommitRuntimeError::InvalidCommitState {
                                reason: "delete count overflow",
                            })?;
                }
            }
        }
        Ok(Self::from_parts_unchecked(puts, deletes, 0))
    }

    pub(crate) const fn puts(self) -> usize {
        self.puts
    }

    pub(crate) const fn deletes(self) -> usize {
        self.deletes
    }

    pub(crate) const fn timeline_rows(self) -> usize {
        self.timeline_rows
    }

    const fn is_empty(self) -> bool {
        self.puts == 0 && self.deletes == 0 && self.timeline_rows == 0
    }
}

impl CommitOutcome {
    pub(crate) fn new(
        branch_id: BranchId,
        kind: CommitOutcomeKind,
        phase: CommitPhase,
        durability: CommitDurabilityClass,
        commit_stamp: Option<CommitStamp>,
        mutation_counts: CommitMutationCounts,
        visibility_facts: CommitVisibilityFacts,
        read_snapshot: Option<CommitReadSnapshot>,
    ) -> CommitRuntimeResult<Self> {
        if let Some(stamp) = commit_stamp {
            if stamp.branch_id() != branch_id {
                return Err(CommitRuntimeError::BranchMismatch {
                    expected: branch_id,
                    actual: stamp.branch_id(),
                });
            }
        }
        let outcome = Self {
            branch_id,
            kind,
            phase,
            durability,
            commit_version: commit_stamp.map(CommitStamp::commit_version),
            commit_timestamp: commit_stamp.map(CommitStamp::commit_timestamp),
            mutation_counts,
            visibility_facts,
            read_snapshot,
        };
        outcome.validate()?;
        Ok(outcome)
    }

    pub(crate) fn read_only(snapshot: CommitReadSnapshot) -> CommitRuntimeResult<Self> {
        Self::new(
            snapshot.branch_id(),
            CommitOutcomeKind::ReadOnly,
            CommitPhase::RejectedBeforeAllocation,
            CommitDurabilityClass::NotDurable,
            None,
            CommitMutationCounts::read_only(),
            CommitVisibilityFacts::empty(),
            Some(snapshot),
        )
    }

    pub(crate) fn visible(
        branch_id: BranchId,
        stamp: CommitStamp,
        durability: CommitDurabilityClass,
        mutation_counts: CommitMutationCounts,
        visibility_facts: CommitVisibilityFacts,
    ) -> CommitRuntimeResult<Self> {
        Self::new(
            branch_id,
            CommitOutcomeKind::Visible,
            CommitPhase::Visible,
            durability,
            Some(stamp),
            mutation_counts,
            visibility_facts,
            None,
        )
    }

    pub(crate) fn durable_but_not_visible(
        branch_id: BranchId,
        stamp: CommitStamp,
        durability: CommitDurabilityClass,
        mutation_counts: CommitMutationCounts,
        visibility_facts: CommitVisibilityFacts,
    ) -> CommitRuntimeResult<Self> {
        Self::new(
            branch_id,
            CommitOutcomeKind::DurableButNotVisible,
            CommitPhase::DurableNotApplied,
            durability,
            Some(stamp),
            mutation_counts,
            visibility_facts,
            None,
        )
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn kind(&self) -> CommitOutcomeKind {
        self.kind
    }

    pub(crate) const fn phase(&self) -> CommitPhase {
        self.phase
    }

    pub(crate) const fn durability(&self) -> CommitDurabilityClass {
        self.durability
    }

    pub(crate) const fn commit_version(&self) -> Option<CommitVersion> {
        self.commit_version
    }

    pub(crate) const fn commit_timestamp(&self) -> Option<Timestamp> {
        self.commit_timestamp
    }

    pub(crate) const fn mutation_counts(&self) -> CommitMutationCounts {
        self.mutation_counts
    }

    pub(crate) const fn visibility_facts(&self) -> CommitVisibilityFacts {
        self.visibility_facts
    }

    pub(crate) const fn read_snapshot(&self) -> Option<CommitReadSnapshot> {
        self.read_snapshot
    }

    fn validate(&self) -> CommitRuntimeResult<()> {
        self.visibility_facts.validate()?;
        match self.kind {
            CommitOutcomeKind::ReadOnly => self.validate_read_only(),
            CommitOutcomeKind::Visible => self.validate_visible(),
            CommitOutcomeKind::NotVisible => self.validate_not_visible(),
            CommitOutcomeKind::DurableButNotVisible => self.validate_durable_but_not_visible(),
            CommitOutcomeKind::Replay => self.validate_replay(),
        }
    }

    fn validate_read_only(&self) -> CommitRuntimeResult<()> {
        if self.phase != CommitPhase::RejectedBeforeAllocation {
            return Err(CommitRuntimeError::InvalidCommitPhase {
                reason: "read-only outcome must use rejected-before-allocation phase",
            });
        }
        if self.commit_version.is_some() || self.commit_timestamp.is_some() {
            return Err(CommitRuntimeError::InvalidCommitState {
                reason: "read-only outcome must not carry commit facts",
            });
        }
        if self.durability != CommitDurabilityClass::NotDurable {
            return Err(CommitRuntimeError::InvalidCommitState {
                reason: "read-only outcome must not claim durability",
            });
        }
        if !self.mutation_counts.is_empty() {
            return Err(CommitRuntimeError::InvalidCommitState {
                reason: "read-only outcome must not report mutations",
            });
        }
        match self.read_snapshot {
            Some(snapshot) if snapshot.branch_id() == self.branch_id => {}
            Some(_) => {
                return Err(CommitRuntimeError::InvalidCommitState {
                    reason: "read-only snapshot branch must match outcome branch",
                });
            }
            None => {
                return Err(CommitRuntimeError::InvalidCommitState {
                    reason: "read-only outcome must carry a read snapshot",
                });
            }
        }
        if self.visibility_facts != CommitVisibilityFacts::empty() {
            return Err(CommitRuntimeError::InvalidCommitState {
                reason: "read-only outcome must not publish visibility",
            });
        }
        Ok(())
    }

    fn validate_visible(&self) -> CommitRuntimeResult<()> {
        self.require_commit_stamp("visible outcome must carry commit facts")?;
        if self.phase != CommitPhase::Visible {
            return Err(CommitRuntimeError::InvalidCommitPhase {
                reason: "visible outcome must use visible phase",
            });
        }
        self.require_durability_matches_facts()?;
        if self.read_snapshot.is_some() {
            return Err(CommitRuntimeError::InvalidCommitState {
                reason: "mutating outcome must not carry a read snapshot",
            });
        }
        if self.visibility_facts.visible_version() != self.commit_version {
            return Err(CommitRuntimeError::InvalidVisibilityFacts {
                reason: "visible outcome must publish its commit version",
            });
        }
        Ok(())
    }

    fn validate_not_visible(&self) -> CommitRuntimeResult<()> {
        self.require_commit_stamp("not-visible outcome must carry commit facts")?;
        if self.visibility_facts.allocated_version() != self.commit_version {
            return Err(CommitRuntimeError::InvalidVisibilityFacts {
                reason: "not-visible outcome must preserve allocated version",
            });
        }
        if !matches!(
            self.phase,
            CommitPhase::AllocatedNotDurable | CommitPhase::AppliedNotVisible
        ) {
            return Err(CommitRuntimeError::InvalidCommitPhase {
                reason: "not-visible outcome must use an allocated or applied non-visible phase",
            });
        }
        if self.durability != CommitDurabilityClass::NotDurable {
            return Err(CommitRuntimeError::InvalidCommitState {
                reason: "not-visible outcome must not claim durability",
            });
        }
        if self.visibility_facts.durable_version().is_some() {
            return Err(CommitRuntimeError::InvalidVisibilityFacts {
                reason: "not-visible outcome must not preserve durable version",
            });
        }
        if self.phase == CommitPhase::AppliedNotVisible
            && self.visibility_facts.applied_version() != self.commit_version
        {
            return Err(CommitRuntimeError::InvalidVisibilityFacts {
                reason: "applied not-visible outcome must preserve applied version",
            });
        }
        if self.read_snapshot.is_some() {
            return Err(CommitRuntimeError::InvalidCommitState {
                reason: "mutating outcome must not carry a read snapshot",
            });
        }
        if self.visibility_facts.visible_version() == self.commit_version {
            return Err(CommitRuntimeError::InvalidVisibilityFacts {
                reason: "not-visible outcome must not publish its commit version",
            });
        }
        Ok(())
    }

    fn validate_durable_but_not_visible(&self) -> CommitRuntimeResult<()> {
        self.require_commit_stamp("durable-but-not-visible outcome must carry commit facts")?;
        if !matches!(
            self.phase,
            CommitPhase::DurableNotApplied | CommitPhase::AppliedNotVisible
        ) {
            return Err(CommitRuntimeError::InvalidCommitPhase {
                reason: "durable-but-not-visible outcome must use a durable non-visible phase",
            });
        }
        if !matches!(
            self.durability,
            CommitDurabilityClass::Standard | CommitDurabilityClass::Always
        ) {
            return Err(CommitRuntimeError::InvalidCommitState {
                reason: "durable-but-not-visible outcome must claim durable WAL success",
            });
        }
        if self.visibility_facts.allocated_version() != self.commit_version {
            return Err(CommitRuntimeError::InvalidVisibilityFacts {
                reason: "durable-but-not-visible outcome must preserve allocated version",
            });
        }
        if self.visibility_facts.durable_version() != self.commit_version {
            return Err(CommitRuntimeError::InvalidVisibilityFacts {
                reason: "durable-but-not-visible outcome must preserve durable version",
            });
        }
        match self.phase {
            CommitPhase::DurableNotApplied if self.visibility_facts.applied_version().is_some() => {
                return Err(CommitRuntimeError::InvalidVisibilityFacts {
                    reason: "durable-not-applied outcome must not preserve applied version",
                });
            }
            CommitPhase::AppliedNotVisible
                if self.visibility_facts.applied_version() != self.commit_version =>
            {
                return Err(CommitRuntimeError::InvalidVisibilityFacts {
                    reason: "applied durable outcome must preserve applied version",
                });
            }
            _ => {}
        }
        if self.visibility_facts.visible_version() == self.commit_version {
            return Err(CommitRuntimeError::InvalidVisibilityFacts {
                reason: "durable-but-not-visible outcome must not publish visibility",
            });
        }
        if self.read_snapshot.is_some() {
            return Err(CommitRuntimeError::InvalidCommitState {
                reason: "mutating outcome must not carry a read snapshot",
            });
        }
        Ok(())
    }

    fn validate_replay(&self) -> CommitRuntimeResult<()> {
        self.require_commit_stamp("replay outcome must carry commit facts")?;
        if self.phase != CommitPhase::Replay {
            return Err(CommitRuntimeError::InvalidCommitPhase {
                reason: "replay outcome must use replay phase",
            });
        }
        self.require_durability_matches_facts()?;
        if self.read_snapshot.is_some() {
            return Err(CommitRuntimeError::InvalidCommitState {
                reason: "replay outcome must not carry a read snapshot",
            });
        }
        Ok(())
    }

    fn require_commit_stamp(&self, reason: &'static str) -> CommitRuntimeResult<()> {
        match (self.commit_version, self.commit_timestamp) {
            (Some(version), Some(_)) if version != CommitVersion::ZERO => Ok(()),
            _ => Err(CommitRuntimeError::InvalidCommitState { reason }),
        }
    }

    fn require_durability_matches_facts(&self) -> CommitRuntimeResult<()> {
        match self.durability {
            CommitDurabilityClass::NotDurable
                if self.visibility_facts.durable_version().is_some() =>
            {
                Err(CommitRuntimeError::InvalidVisibilityFacts {
                    reason: "non-durable outcome must not preserve durable version",
                })
            }
            CommitDurabilityClass::NotDurable => Ok(()),
            CommitDurabilityClass::Standard | CommitDurabilityClass::Always
                if self.visibility_facts.durable_version() == self.commit_version =>
            {
                Ok(())
            }
            CommitDurabilityClass::Standard | CommitDurabilityClass::Always => {
                Err(CommitRuntimeError::InvalidVisibilityFacts {
                    reason: "durable outcome must preserve durable version",
                })
            }
            CommitDurabilityClass::Uncertain => Err(CommitRuntimeError::InvalidCommitState {
                reason: "outcome must not claim uncertain durability",
            }),
        }
    }
}

pub(crate) fn execute_read_only_diagnostic(
    batch: &ValidatedCommitBatch,
    config: &CommitRuntimeConfig,
    visible: VisibleVersionTracker,
) -> CommitRuntimeResult<CommitOutcome> {
    config.validate()?;
    if batch.batch().kind() != CommitBatchKind::ReadOnlyDiagnostic {
        return Err(CommitRuntimeError::InvalidBatch {
            reason: "read-only diagnostic executor requires read-only batch",
        });
    }
    if config.read_only_diagnostics() != CommitReadOnlyDiagnostics::Enabled {
        return Err(CommitRuntimeError::InvalidCommitPhase {
            reason: "read-only diagnostics are disabled",
        });
    }

    CommitOutcome::read_only(CommitReadSnapshot::new(
        batch.batch().branch_id(),
        visible.visible_version(),
    ))
}
