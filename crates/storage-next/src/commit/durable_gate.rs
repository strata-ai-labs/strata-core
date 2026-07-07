//! Unresolved durable commit gate.
//!
//! This gate is the first mutating-commit admission point in the commit
//! runtime. Its mutex is held only while checking or updating gate state; the
//! returned admission token is a logical global commit token and does not carry
//! a mutex guard. Cache and durable runtimes must therefore acquire this gate
//! before branch admission, then proceed to registry validation and branch guard
//! acquisition after the gate mutex has been released.

use super::{
    CommitDurabilityClass, CommitRuntimeError, CommitRuntimeResult, CommitStamp,
    CommitVisibilityFacts,
};
use crate::observability::perf_trace;
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
    /// First commit version the unresolved state covers. Equal to `commit_version` for a
    /// single commit; a write group's group-fatal failure records the whole group's
    /// contiguous version block `first_commit_version..=commit_version` (BS5.1 D1), and
    /// recovery replays every version in the range before the gate clears.
    first_commit_version: CommitVersion,
    commit_version: CommitVersion,
    commit_timestamp: Timestamp,
    durability: CommitDurabilityClass,
    kind: CommitUnresolvedDurableKind,
    visibility_facts: CommitVisibilityFacts,
    reason: &'static str,
}

#[derive(Debug, Default)]
pub(crate) struct CommitUnresolvedDurableGate {
    state: Mutex<CommitUnresolvedDurableGateState>,
}

#[derive(Debug)]
pub(crate) struct CommitUnresolvedDurableAdmission<'a> {
    gate: &'a CommitUnresolvedDurableGate,
    active: bool,
}

#[derive(Debug, Default)]
struct CommitUnresolvedDurableGateState {
    unresolved: Option<CommitUnresolvedDurable>,
    /// Open admission spans (BS5.2). The pipelined commit path keeps a span
    /// open across its off-lock covering fsync, so a later group (or a solo
    /// commit) legitimately admits while earlier spans are still in flight;
    /// admission is refused only while an unresolved fact is recorded.
    active_admissions: usize,
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
            first_commit_version: stamp.commit_version(),
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

    /// Widen this fact to cover a write group's contiguous version block, starting at
    /// `first_commit_version` (BS5.1 D1). The stamp fields stay keyed to the group's LAST member
    /// (the range end) so recovery's replay of the final version clears the gate; a group of one
    /// leaves the fact byte-identical to the single-commit shape.
    pub(crate) fn covering_group_from(
        mut self,
        first_commit_version: CommitVersion,
    ) -> CommitRuntimeResult<Self> {
        self.first_commit_version = first_commit_version;
        self.validate()?;
        Ok(self)
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
        let durable_version = if durability == CommitDurabilityClass::NotDurable {
            None
        } else {
            Some(stamp.commit_version())
        };
        Self::new(
            stamp,
            durability,
            CommitUnresolvedDurableKind::AppliedNotVisible,
            CommitVisibilityFacts::new(
                Some(stamp.commit_version()),
                durable_version,
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

    pub(crate) const fn first_commit_version(self) -> CommitVersion {
        self.first_commit_version
    }

    /// Whether `version` falls inside the covered range (a single commit's range is itself).
    pub(crate) fn covers_version(self, version: CommitVersion) -> bool {
        version >= self.first_commit_version && version <= self.commit_version
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
        if self.first_commit_version > self.commit_version {
            return Err(CommitRuntimeError::InvalidCommitState {
                reason: "unresolved durable range must not start after its end",
            });
        }
        if self.visibility_facts.allocated_version() != Some(self.commit_version) {
            return Err(CommitRuntimeError::InvalidVisibilityFacts {
                reason: "unresolved durable commit must preserve allocated version",
            });
        }
        match self.kind {
            CommitUnresolvedDurableKind::DurableNotApplied => {
                if !matches!(
                    self.durability,
                    CommitDurabilityClass::Standard | CommitDurabilityClass::Always
                ) {
                    return Err(CommitRuntimeError::InvalidCommitState {
                        reason:
                            "durable-not-applied unresolved commit must claim durable WAL success",
                    });
                }
                if self.visibility_facts.durable_version() != Some(self.commit_version) {
                    return Err(CommitRuntimeError::InvalidVisibilityFacts {
                        reason: "unresolved durable commit must preserve durable version",
                    });
                }
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
                if matches!(self.durability, CommitDurabilityClass::Uncertain) {
                    return Err(CommitRuntimeError::InvalidCommitState {
                        reason:
                            "applied-not-visible unresolved commit cannot have uncertain durability",
                    });
                }
                if matches!(
                    self.durability,
                    CommitDurabilityClass::Standard | CommitDurabilityClass::Always
                ) && self.visibility_facts.durable_version() != Some(self.commit_version)
                {
                    return Err(CommitRuntimeError::InvalidVisibilityFacts {
                        reason: "unresolved durable commit must preserve durable version",
                    });
                }
                if self.durability == CommitDurabilityClass::NotDurable
                    && self.visibility_facts.durable_version().is_some()
                {
                    return Err(CommitRuntimeError::InvalidVisibilityFacts {
                        reason:
                            "not-durable applied-not-visible commit must not claim durable progress",
                    });
                }
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
            state: Mutex::new(CommitUnresolvedDurableGateState {
                unresolved: None,
                active_admissions: 0,
            }),
        }
    }

    pub(crate) fn unresolved(&self) -> CommitRuntimeResult<Option<CommitUnresolvedDurable>> {
        Ok(self.lock()?.unresolved)
    }

    /// Whether the gate currently holds no unresolved durable commit. Debug-assert helper for the
    /// BS2.2 visibility-bound equivalence check (type-checked in release, evaluated only in
    /// debug); a poisoned lock relaxes to `true` so a panic in flight is not compounded by a
    /// spurious assert.
    pub(crate) fn is_clean(&self) -> bool {
        self.lock()
            .map(|state| state.unresolved.is_none())
            .unwrap_or(true)
    }

    pub(crate) fn require_admission_available(&self) -> CommitRuntimeResult<()> {
        let state = self.lock()?;
        if let Some(unresolved) = state.unresolved {
            perf_trace::record_commit_unresolved_gate_admission_attempt();
            perf_trace::record_commit_unresolved_gate_rejected_unresolved();
            return Err(CommitRuntimeError::UnresolvedDurableCommit {
                branch_id: unresolved.branch_id(),
                commit_version: unresolved.commit_version(),
                reason: "durable commit must be replayed or reconciled first",
            });
        }
        Ok(())
    }

    pub(crate) fn require_open_for_mutation(&self) -> CommitRuntimeResult<()> {
        self.admit_mutating_commit().map(|_| ())
    }

    /// Leader-scoped admission span for a write group (BS5.1): identical
    /// admission rules to [`admit_mutating_commit`], but the span is released
    /// explicitly with [`end_group_admission`] instead of through the
    /// borrowing RAII token, which cannot live across the leader's `&mut self`
    /// bootstrap calls. A panic mid-span leaves admission active — equivalent
    /// to today's panic-under-runtime-lock exposure, which is already
    /// unrecoverable in-process.
    pub(crate) fn begin_group_admission(&self) -> CommitRuntimeResult<()> {
        let mut admission = self.admit_mutating_commit()?;
        // Defuse the RAII reset: the caller now owns the span.
        admission.active = false;
        Ok(())
    }

    /// Release the leader's group admission span (see [`begin_group_admission`]).
    pub(crate) fn end_group_admission(&self) {
        if let Ok(mut state) = self.state.lock() {
            state.active_admissions = state.active_admissions.saturating_sub(1);
        }
    }

    /// Open admission spans (BS5.2): >1 means other commits are mid-pipeline
    /// right now — the sync chain uses this to decide whether a batching beat
    /// is worth waiting before it captures.
    pub(crate) fn open_admission_spans(&self) -> usize {
        self.state.lock().map_or(0, |state| state.active_admissions)
    }

    /// Group-member admission check (BS5.1): the leader's admission span is
    /// active by construction while members execute, so a member verifies only
    /// that no unresolved fact has been recorded (a mid-group failure records
    /// one and makes the group fatal).
    pub(crate) fn require_no_unresolved_fact(&self) -> CommitRuntimeResult<()> {
        let state = self.lock()?;
        if let Some(unresolved) = state.unresolved {
            perf_trace::record_commit_unresolved_gate_admission_attempt();
            perf_trace::record_commit_unresolved_gate_rejected_unresolved();
            return Err(CommitRuntimeError::UnresolvedDurableCommit {
                branch_id: unresolved.branch_id(),
                commit_version: unresolved.commit_version(),
                reason: "durable commit must be replayed or reconciled first",
            });
        }
        Ok(())
    }

    pub(crate) fn admit_mutating_commit(
        &self,
    ) -> CommitRuntimeResult<CommitUnresolvedDurableAdmission<'_>> {
        perf_trace::record_commit_unresolved_gate_admission_attempt();
        let mut state = self.lock()?;
        if let Some(unresolved) = state.unresolved {
            perf_trace::record_commit_unresolved_gate_rejected_unresolved();
            return Err(CommitRuntimeError::UnresolvedDurableCommit {
                branch_id: unresolved.branch_id(),
                commit_version: unresolved.commit_version(),
                reason: "durable commit must be replayed or reconciled first",
            });
        }
        // BS5.2: concurrent spans are legal — the pipelined commit path keeps a
        // span open across its off-lock fsync while later commits admit.
        state.active_admissions = state.active_admissions.saturating_add(1);
        perf_trace::record_commit_unresolved_gate_admission_acquired();
        Ok(CommitUnresolvedDurableAdmission {
            gate: self,
            active: true,
        })
    }

    pub(crate) fn record_unresolved(
        &self,
        unresolved: CommitUnresolvedDurable,
    ) -> CommitRuntimeResult<()> {
        unresolved.validate()?;
        let mut state = self.lock()?;
        match state.unresolved {
            Some(existing) if existing == unresolved => Ok(()),
            Some(_) => Err(CommitRuntimeError::InvalidCommitState {
                reason: "different unresolved durable commit is already recorded",
            }),
            None => {
                state.unresolved = Some(unresolved);
                perf_trace::record_commit_unresolved_record();
                match unresolved.kind() {
                    CommitUnresolvedDurableKind::DurableNotApplied => {
                        perf_trace::record_commit_unresolved_durable_not_applied_record();
                    }
                    CommitUnresolvedDurableKind::AppliedNotVisible => {
                        perf_trace::record_commit_unresolved_applied_not_visible_record();
                    }
                }
                Ok(())
            }
        }
    }

    pub(crate) fn clear_exact(
        &self,
        unresolved: CommitUnresolvedDurable,
    ) -> CommitRuntimeResult<()> {
        let mut state = self.lock()?;
        match state.unresolved {
            Some(existing) if existing == unresolved => {
                state.unresolved = None;
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

    pub(crate) fn replace_exact(
        &self,
        expected: CommitUnresolvedDurable,
        replacement: CommitUnresolvedDurable,
    ) -> CommitRuntimeResult<()> {
        replacement.validate()?;
        let mut state = self.lock()?;
        match state.unresolved {
            Some(existing) if existing == expected => {
                state.unresolved = Some(replacement);
                Ok(())
            }
            Some(_) => Err(CommitRuntimeError::InvalidCommitState {
                reason: "cannot replace different unresolved durable commit",
            }),
            None => Err(CommitRuntimeError::InvalidCommitState {
                reason: "cannot replace empty unresolved durable gate",
            }),
        }
    }

    fn lock(&self) -> CommitRuntimeResult<MutexGuard<'_, CommitUnresolvedDurableGateState>> {
        self.state
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

impl Drop for CommitUnresolvedDurableAdmission<'_> {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        if let Ok(mut state) = self.gate.state.lock() {
            state.active_admissions = state.active_admissions.saturating_sub(1);
        }
        self.active = false;
    }
}
