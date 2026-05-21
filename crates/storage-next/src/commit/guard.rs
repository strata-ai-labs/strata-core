//! Commit guard tokens for branch admission and quiesce.
//!
//! These guards are crate-local, in-process admission tokens. They serialize
//! mutating work for one branch, allow independent branches to proceed at the
//! same time, and provide a nonblocking quiesce token for L8 maintenance and
//! recovery orchestration.

use super::{CommitRuntimeError, CommitRuntimeResult};
use std::fmt;
use std::sync::{Arc, Mutex, MutexGuard};
use strata_core_next::BranchId;

#[derive(Clone, Default)]
pub(crate) struct CommitBranchGuardSet {
    inner: Arc<Mutex<CommitBranchGuardState>>,
}

#[derive(Debug, Default)]
struct CommitBranchGuardState {
    active_branches: Vec<BranchId>,
    quiescing: bool,
}

pub(crate) struct CommitBranchGuard {
    inner: Arc<Mutex<CommitBranchGuardState>>,
    branch_id: BranchId,
}

pub(crate) struct CommitQuiesceGuard {
    inner: Arc<Mutex<CommitBranchGuardState>>,
}

impl CommitBranchGuardSet {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn try_acquire_branch_guard(
        &self,
        branch_id: BranchId,
    ) -> CommitRuntimeResult<CommitBranchGuard> {
        let mut state = self.lock_state()?;
        if state.quiescing {
            return Err(CommitRuntimeError::CommitQuiesceUnavailable {
                reason: "commit quiesce is active",
            });
        }
        if state.active_branches.contains(&branch_id) {
            return Err(CommitRuntimeError::BranchGuardUnavailable {
                branch_id,
                reason: "branch commit guard is already active",
            });
        }
        state.active_branches.push(branch_id);
        Ok(CommitBranchGuard {
            inner: Arc::clone(&self.inner),
            branch_id,
        })
    }

    /// Attempts to begin V1 quiesce.
    ///
    /// V1 deliberately fast-fails instead of waiting: L8 owns retry/deadline
    /// policy above this crate-private token. Once a token is returned, new
    /// mutating branch guards are rejected until the token is dropped.
    pub(crate) fn try_begin_quiesce(&self) -> CommitRuntimeResult<CommitQuiesceGuard> {
        let mut state = self.lock_state()?;
        if state.quiescing {
            return Err(CommitRuntimeError::CommitQuiesceUnavailable {
                reason: "commit quiesce is already active",
            });
        }
        if !state.active_branches.is_empty() {
            return Err(CommitRuntimeError::CommitQuiesceUnavailable {
                reason: "commit quiesce cannot start while branch guards are active",
            });
        }
        state.quiescing = true;
        Ok(CommitQuiesceGuard {
            inner: Arc::clone(&self.inner),
        })
    }

    pub(crate) fn active_guard_count(&self) -> CommitRuntimeResult<usize> {
        Ok(self.lock_state()?.active_branches.len())
    }

    pub(crate) fn is_quiescing(&self) -> CommitRuntimeResult<bool> {
        Ok(self.lock_state()?.quiescing)
    }

    fn lock_state(&self) -> CommitRuntimeResult<MutexGuard<'_, CommitBranchGuardState>> {
        self.inner
            .lock()
            .map_err(|_| CommitRuntimeError::InvalidCommitState {
                reason: "commit branch guard state lock is poisoned",
            })
    }
}

impl CommitBranchGuard {
    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }
}

impl Drop for CommitBranchGuard {
    fn drop(&mut self) {
        match self.inner.lock() {
            Ok(mut state) => {
                remove_branch(&mut state.active_branches, self.branch_id);
            }
            Err(poisoned) => {
                let mut state = poisoned.into_inner();
                remove_branch(&mut state.active_branches, self.branch_id);
            }
        }
    }
}

impl Drop for CommitQuiesceGuard {
    fn drop(&mut self) {
        match self.inner.lock() {
            Ok(mut state) => {
                state.quiescing = false;
            }
            Err(poisoned) => {
                let mut state = poisoned.into_inner();
                state.quiescing = false;
            }
        }
    }
}

impl fmt::Debug for CommitBranchGuardSet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.inner.lock() {
            Ok(state) => formatter
                .debug_struct("CommitBranchGuardSet")
                .field("active_guard_count", &state.active_branches.len())
                .field("quiescing", &state.quiescing)
                .finish(),
            Err(_) => formatter
                .debug_struct("CommitBranchGuardSet")
                .field("poisoned", &true)
                .finish(),
        }
    }
}

impl fmt::Debug for CommitBranchGuard {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CommitBranchGuard")
            .field("branch_id", &self.branch_id)
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for CommitQuiesceGuard {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CommitQuiesceGuard")
            .finish_non_exhaustive()
    }
}

fn remove_branch(active_branches: &mut Vec<BranchId>, branch_id: BranchId) {
    if let Some(index) = active_branches
        .iter()
        .position(|active| *active == branch_id)
    {
        active_branches.swap_remove(index);
    }
}
