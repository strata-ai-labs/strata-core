//! Branch admission registry for commit-runtime targets.

use super::{
    CommitBatchKind, CommitBranchGuard, CommitBranchGuardSet, CommitRuntimeError,
    CommitRuntimeResult, ValidatedCommitBatch,
};
use std::num::NonZeroU64;
use strata_core_next::BranchId;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct CommitBranchGeneration(NonZeroU64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommitBranchGenerationGuard {
    NotSupplied,
    Exact(CommitBranchGeneration),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommitBranchState {
    Active,
    Deleting,
    Deleted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CommitBranchDescriptor {
    branch_id: BranchId,
    generation: CommitBranchGeneration,
    state: CommitBranchState,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct CommitBranchRegistry {
    // BranchId intentionally does not expose Ord, so this registry keeps a
    // small explicit list instead of the BTreeMap shape from the planning doc.
    descriptors: Vec<CommitBranchDescriptor>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CommitBranchAdmission {
    branch_id: BranchId,
    generation: CommitBranchGeneration,
    state: CommitBranchState,
}

#[derive(Debug)]
pub(crate) struct CommitBranchAdmissionGuard {
    admission: CommitBranchAdmission,
    _guard: CommitBranchGuard,
}

impl CommitBranchGeneration {
    pub(crate) fn new(value: u64) -> CommitRuntimeResult<Self> {
        NonZeroU64::new(value)
            .map(Self)
            .ok_or(CommitRuntimeError::InvalidCommitState {
                reason: "branch generation must be nonzero",
            })
    }

    pub(crate) const fn get(self) -> u64 {
        self.0.get()
    }
}

impl CommitBranchGenerationGuard {
    pub(crate) const fn not_supplied() -> Self {
        Self::NotSupplied
    }

    pub(crate) const fn exact(generation: CommitBranchGeneration) -> Self {
        Self::Exact(generation)
    }

    fn validate(
        self,
        branch_id: BranchId,
        current: CommitBranchGeneration,
    ) -> CommitRuntimeResult<()> {
        match self {
            Self::NotSupplied => Ok(()),
            Self::Exact(supplied) if supplied == current => Ok(()),
            Self::Exact(supplied) => Err(CommitRuntimeError::BranchGenerationMismatch {
                branch_id,
                expected: current.get(),
                actual: supplied.get(),
            }),
        }
    }
}

impl CommitBranchState {
    const fn rejection_reason(self) -> Option<&'static str> {
        match self {
            Self::Active => None,
            Self::Deleting => Some("branch is deleting"),
            Self::Deleted => Some("branch is deleted"),
        }
    }
}

impl CommitBranchDescriptor {
    pub(crate) fn active(branch_id: BranchId, generation: CommitBranchGeneration) -> Self {
        Self::new(branch_id, generation, CommitBranchState::Active)
    }

    pub(crate) const fn new(
        branch_id: BranchId,
        generation: CommitBranchGeneration,
        state: CommitBranchState,
    ) -> Self {
        Self {
            branch_id,
            generation,
            state,
        }
    }

    pub(crate) const fn branch_id(self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn generation(self) -> CommitBranchGeneration {
        self.generation
    }

    pub(crate) const fn state(self) -> CommitBranchState {
        self.state
    }

    fn require_writable(self) -> CommitRuntimeResult<()> {
        match self.state.rejection_reason() {
            Some(reason) => Err(CommitRuntimeError::BranchNotWritable {
                branch_id: self.branch_id,
                reason,
            }),
            None => Ok(()),
        }
    }
}

impl CommitBranchRegistry {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn register_active(
        &mut self,
        branch_id: BranchId,
        generation: CommitBranchGeneration,
    ) -> CommitRuntimeResult<CommitBranchDescriptor> {
        self.register_descriptor(
            branch_id,
            CommitBranchDescriptor::active(branch_id, generation),
        )
    }

    pub(crate) fn register_descriptor(
        &mut self,
        branch_id: BranchId,
        descriptor: CommitBranchDescriptor,
    ) -> CommitRuntimeResult<CommitBranchDescriptor> {
        if descriptor.branch_id() != branch_id {
            return Err(CommitRuntimeError::BranchMismatch {
                expected: branch_id,
                actual: descriptor.branch_id(),
            });
        }
        if self
            .descriptors
            .iter()
            .any(|entry| entry.branch_id() == branch_id)
        {
            return Err(CommitRuntimeError::BranchAlreadyExists { branch_id });
        }
        self.descriptors.push(descriptor);
        Ok(descriptor)
    }

    pub(crate) fn lookup(
        &self,
        branch_id: BranchId,
    ) -> CommitRuntimeResult<CommitBranchDescriptor> {
        self.descriptors
            .iter()
            .find(|descriptor| descriptor.branch_id() == branch_id)
            .copied()
            .ok_or(CommitRuntimeError::BranchNotFound { branch_id })
    }

    pub(crate) fn mark_deleting(
        &mut self,
        branch_id: BranchId,
    ) -> CommitRuntimeResult<CommitBranchDescriptor> {
        self.set_state(branch_id, CommitBranchState::Deleting)
    }

    pub(crate) fn mark_deleted(
        &mut self,
        branch_id: BranchId,
    ) -> CommitRuntimeResult<CommitBranchDescriptor> {
        self.set_state(branch_id, CommitBranchState::Deleted)
    }

    pub(crate) fn recreate_active(
        &mut self,
        branch_id: BranchId,
        generation: CommitBranchGeneration,
    ) -> CommitRuntimeResult<CommitBranchDescriptor> {
        let current = self.lookup(branch_id)?;
        match current.state() {
            CommitBranchState::Deleted if generation > current.generation() => {
                let descriptor = CommitBranchDescriptor::active(branch_id, generation);
                self.replace_descriptor(descriptor);
                Ok(descriptor)
            }
            CommitBranchState::Deleted if current.generation().get() == u64::MAX => {
                Err(CommitRuntimeError::BranchGenerationExhausted {
                    branch_id,
                    generation: current.generation().get(),
                })
            }
            CommitBranchState::Deleted => Err(CommitRuntimeError::BranchGenerationMismatch {
                branch_id,
                expected: current.generation().get().saturating_add(1),
                actual: generation.get(),
            }),
            CommitBranchState::Active | CommitBranchState::Deleting => {
                Err(CommitRuntimeError::BranchAlreadyExists { branch_id })
            }
        }
    }

    pub(crate) fn validate_target(
        &self,
        branch_id: BranchId,
        generation_guard: CommitBranchGenerationGuard,
    ) -> CommitRuntimeResult<CommitBranchAdmission> {
        let descriptor = self.lookup(branch_id)?;
        descriptor.require_writable()?;
        generation_guard.validate(branch_id, descriptor.generation())?;
        Ok(CommitBranchAdmission {
            branch_id,
            generation: descriptor.generation(),
            state: descriptor.state(),
        })
    }

    pub(crate) fn len(&self) -> usize {
        self.descriptors.len()
    }

    pub(crate) fn active_branch_ids(&self) -> Vec<BranchId> {
        let mut branches = self
            .descriptors
            .iter()
            .filter(|descriptor| descriptor.state() == CommitBranchState::Active)
            .map(|descriptor| descriptor.branch_id())
            .collect::<Vec<_>>();
        branches.sort_by(|left, right| left.as_bytes().cmp(right.as_bytes()));
        branches
    }

    fn set_state(
        &mut self,
        branch_id: BranchId,
        state: CommitBranchState,
    ) -> CommitRuntimeResult<CommitBranchDescriptor> {
        let current = self.lookup(branch_id)?;
        let descriptor = CommitBranchDescriptor::new(branch_id, current.generation(), state);
        self.replace_descriptor(descriptor);
        Ok(descriptor)
    }

    fn replace_descriptor(&mut self, descriptor: CommitBranchDescriptor) {
        let current = self
            .descriptors
            .iter_mut()
            .find(|entry| entry.branch_id() == descriptor.branch_id())
            .expect("replace_descriptor callers must look up the branch first");
        *current = descriptor;
    }
}

impl CommitBranchAdmission {
    pub(crate) const fn branch_id(self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn generation(self) -> CommitBranchGeneration {
        self.generation
    }

    pub(crate) const fn state(self) -> CommitBranchState {
        self.state
    }
}

impl CommitBranchAdmissionGuard {
    pub(crate) const fn admission(&self) -> CommitBranchAdmission {
        self.admission
    }
}

pub(crate) fn admit_mutating_commit(
    registry: &CommitBranchRegistry,
    guard_set: &CommitBranchGuardSet,
    batch: &ValidatedCommitBatch,
    generation_guard: CommitBranchGenerationGuard,
) -> CommitRuntimeResult<CommitBranchAdmissionGuard> {
    if batch.batch().kind() != CommitBatchKind::Mutating {
        return Err(CommitRuntimeError::InvalidBatch {
            reason: "mutating branch admission requires mutating batch",
        });
    }
    let branch_id = batch.batch().branch_id();
    // Validate branch lifecycle and generation before acquiring the branch
    // guard. A guard-acquisition failure therefore cannot mutate registry
    // descriptors, and later commit phases hold the guard by RAII.
    let admission = registry.validate_target(branch_id, generation_guard)?;
    let guard = guard_set.try_acquire_branch_guard(branch_id)?;
    Ok(CommitBranchAdmissionGuard {
        admission,
        _guard: guard,
    })
}
