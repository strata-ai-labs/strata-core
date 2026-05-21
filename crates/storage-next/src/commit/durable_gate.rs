//! Unresolved durable commit gate.

use super::{
    CommitDurabilityClass, CommitRuntimeError, CommitRuntimeResult, CommitStamp,
    CommitVisibilityFacts,
};
use std::sync::{Mutex, MutexGuard};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommitUnresolvedDurableKind {
    DurableNotApplied,
    AppliedNotVisible,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CommitUnresolvedDurable {
    branch_id: BranchId,
    commit_version: CommitVersion,
    commit_timestamp: Timestamp,
    durability: CommitDurabilityClass,
    kind: CommitUnresolvedDurableKind,
    visibility_facts: CommitVisibilityFacts,
    reason: &'static str,
}

#[derive(Debug, Default)]
pub(crate) struct CommitUnresolvedDurableGate {
    unresolved: Mutex<Option<CommitUnresolvedDurable>>,
}

#[derive(Debug)]
pub(crate) struct CommitUnresolvedDurableAdmission<'a> {
    gate: &'a CommitUnresolvedDurableGate,
}

impl CommitUnresolvedDurable {
    pub(crate) fn new(
        stamp: CommitStamp,
        durability: CommitDurabilityClass,
        kind: CommitUnresolvedDurableKind,
        visibility_facts: CommitVisibilityFacts,
        reason: &'static str,
    ) -> CommitRuntimeResult<Self> {
        let fact = Self {
            branch_id: stamp.branch_id(),
            commit_version: stamp.commit_version(),
            commit_timestamp: stamp.commit_timestamp(),
            durability,
            kind,
            visibility_facts,
            reason,
        };
        fact.validate()?;
        Ok(fact)
    }

    pub(crate) fn durable_not_applied_with_facts(
        stamp: CommitStamp,
        durability: CommitDurabilityClass,
        reason: &'static str,
    ) -> CommitRuntimeResult<Self> {
        Self::new(
            stamp,
            durability,
            CommitUnresolvedDurableKind::DurableNotApplied,
            CommitVisibilityFacts::new(
                Some(stamp.commit_version()),
                Some(stamp.commit_version()),
                None,
                None,
                None,
            )?,
            reason,
        )
    }

    pub(crate) fn applied_not_visible(
        stamp: CommitStamp,
        durability: CommitDurabilityClass,
        reason: &'static str,
    ) -> CommitRuntimeResult<Self> {
        Self::new(
            stamp,
            durability,
            CommitUnresolvedDurableKind::AppliedNotVisible,
            CommitVisibilityFacts::new(
                Some(stamp.commit_version()),
                Some(stamp.commit_version()),
                Some(stamp.commit_version()),
                None,
                Some(stamp.commit_version()),
            )?,
            reason,
        )
    }

    pub(crate) const fn branch_id(self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn commit_version(self) -> CommitVersion {
        self.commit_version
    }

    pub(crate) const fn commit_timestamp(self) -> Timestamp {
        self.commit_timestamp
    }

    pub(crate) const fn durability(self) -> CommitDurabilityClass {
        self.durability
    }

    pub(crate) const fn kind(self) -> CommitUnresolvedDurableKind {
        self.kind
    }

    pub(crate) const fn visibility_facts(self) -> CommitVisibilityFacts {
        self.visibility_facts
    }

    pub(crate) const fn reason(self) -> &'static str {
        self.reason
    }

    pub(crate) fn validate(self) -> CommitRuntimeResult<()> {
        self.visibility_facts.validate()?;
        if !matches!(
            self.durability,
            CommitDurabilityClass::Standard | CommitDurabilityClass::Always
        ) {
            return Err(CommitRuntimeError::InvalidCommitState {
                reason: "unresolved durable commit must claim durable WAL success",
            });
        }
        if self.visibility_facts.allocated_version() != Some(self.commit_version) {
            return Err(CommitRuntimeError::InvalidVisibilityFacts {
                reason: "unresolved durable commit must preserve allocated version",
            });
        }
        if self.visibility_facts.durable_version() != Some(self.commit_version) {
            return Err(CommitRuntimeError::InvalidVisibilityFacts {
                reason: "unresolved durable commit must preserve durable version",
            });
        }
        match self.kind {
            CommitUnresolvedDurableKind::DurableNotApplied => {
                if self.visibility_facts.applied_version().is_some()
                    || self.visibility_facts.timeline_version().is_some()
                    || self.visibility_facts.visible_version().is_some()
                {
                    return Err(CommitRuntimeError::InvalidVisibilityFacts {
                        reason: "durable-not-applied unresolved commit must not claim applied or visible progress",
                    });
                }
            }
            CommitUnresolvedDurableKind::AppliedNotVisible => {
                if self.visibility_facts.applied_version() != Some(self.commit_version) {
                    return Err(CommitRuntimeError::InvalidVisibilityFacts {
                        reason:
                            "applied-not-visible unresolved commit must preserve applied version",
                    });
                }
                if self.visibility_facts.timeline_version() != Some(self.commit_version) {
                    return Err(CommitRuntimeError::InvalidVisibilityFacts {
                        reason:
                            "applied-not-visible unresolved commit must preserve timeline version",
                    });
                }
                if self.visibility_facts.visible_version().is_some() {
                    return Err(CommitRuntimeError::InvalidVisibilityFacts {
                        reason: "applied-not-visible unresolved commit must not claim visibility",
                    });
                }
            }
        }
        Ok(())
    }
}

impl CommitUnresolvedDurableGate {
    pub(crate) const fn new() -> Self {
        Self {
            unresolved: Mutex::new(None),
        }
    }

    pub(crate) fn unresolved(&self) -> CommitRuntimeResult<Option<CommitUnresolvedDurable>> {
        Ok(*self.lock()?)
    }

    pub(crate) fn require_open_for_mutation(&self) -> CommitRuntimeResult<()> {
        self.admit_mutating_commit().map(|_| ())
    }

    pub(crate) fn admit_mutating_commit(
        &self,
    ) -> CommitRuntimeResult<CommitUnresolvedDurableAdmission<'_>> {
        let slot = self.lock()?;
        if let Some(unresolved) = *slot {
            return Err(CommitRuntimeError::UnresolvedDurableCommit {
                branch_id: unresolved.branch_id(),
                commit_version: unresolved.commit_version(),
                reason: "durable commit must be replayed or reconciled first",
            });
        }
        Ok(CommitUnresolvedDurableAdmission { gate: self })
    }

    pub(crate) fn record_unresolved(
        &self,
        unresolved: CommitUnresolvedDurable,
    ) -> CommitRuntimeResult<()> {
        unresolved.validate()?;
        let mut slot = self.lock()?;
        match *slot {
            Some(existing) if existing == unresolved => Ok(()),
            Some(_) => Err(CommitRuntimeError::InvalidCommitState {
                reason: "different unresolved durable commit is already recorded",
            }),
            None => {
                *slot = Some(unresolved);
                Ok(())
            }
        }
    }

    pub(crate) fn clear_exact(
        &self,
        unresolved: CommitUnresolvedDurable,
    ) -> CommitRuntimeResult<()> {
        let mut slot = self.lock()?;
        match *slot {
            Some(existing) if existing == unresolved => {
                *slot = None;
                Ok(())
            }
            Some(_) => Err(CommitRuntimeError::InvalidCommitState {
                reason: "cannot clear different unresolved durable commit",
            }),
            None => Err(CommitRuntimeError::InvalidCommitState {
                reason: "cannot clear empty unresolved durable gate",
            }),
        }
    }

    fn lock(&self) -> CommitRuntimeResult<MutexGuard<'_, Option<CommitUnresolvedDurable>>> {
        self.unresolved
            .lock()
            .map_err(|_| CommitRuntimeError::InvalidCommitState {
                reason: "unresolved durable gate lock poisoned",
            })
    }
}

impl CommitUnresolvedDurableAdmission<'_> {
    pub(crate) fn record_unresolved(
        &mut self,
        unresolved: CommitUnresolvedDurable,
    ) -> CommitRuntimeResult<()> {
        self.gate.record_unresolved(unresolved)
    }
}
