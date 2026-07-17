//! Commit guard tokens for branch admission and quiesce.
//!
//! These guards are crate-local, in-process admission tokens. At the guard
//! layer they serialize mutating work for one branch, allow independent
//! branches to proceed at the same time, and provide a nonblocking quiesce
//! token for maintenance and recovery orchestration. Cache and durable runtimes
//! may still add broader admission constraints above this guard set.
//!
//! Commit-runtime lock order:
//! 1. The unresolved-durable gate may be admitted first by cache/durable
//!    runtimes, but it never holds its mutex while branch admission runs.
//! 2. Branch registry validation runs without taking this guard mutex.
//! 3. `try_acquire_branch_guard` or `try_begin_quiesce` takes this guard mutex
//!    briefly and returns an RAII token; no WAL, branch-state, visible-version,
//!    or read-view lock may be acquired while the guard mutex is held.
//! 4. Commit work may run while holding logical admission tokens, including the
//!    unresolved-durable admission token and the RAII branch token, but no gate
//!    or guard mutex remains held. The branch-token drop path reacquires this
//!    mutex only to remove its branch from the active set.
//!
//! Both branch contention and quiesce contention are intentionally fail-fast in
//! the commit runtime. Callers that want wait, retry, or deadline behavior must
//! build it above this primitive with the typed `BranchGuardUnavailable` or
//! `CommitQuiesceUnavailable` errors.

use super::lock_order::{CommitLockOrderGuard, COMMIT_GUARD_RANK};
use super::{CommitRuntimeError, CommitRuntimeResult};
use crate::observability::perf_trace;
use std::fmt;
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, Mutex, MutexGuard};
use strata_core::BranchId;

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
        perf_trace::record_commit_branch_guard_attempt();
        let mut state = self.lock_state()?;
        if state.quiescing {
            perf_trace::record_commit_branch_guard_rejected();
            return Err(CommitRuntimeError::CommitQuiesceUnavailable {
                reason: "commit quiesce is active",
            });
        }
        if state.active_branches.contains(&branch_id) {
            perf_trace::record_commit_branch_guard_rejected();
            return Err(CommitRuntimeError::BranchGuardUnavailable {
                branch_id,
                reason: "branch commit guard is already active",
            });
        }
        state.active_branches.push(branch_id);
        perf_trace::record_commit_branch_guard_acquired();
        Ok(CommitBranchGuard {
            inner: Arc::clone(&self.inner),
            branch_id,
        })
    }

    pub(crate) fn require_branch_guard_available(
        &self,
        branch_id: BranchId,
    ) -> CommitRuntimeResult<()> {
        let state = self.lock_state()?;
        if state.quiescing {
            perf_trace::record_commit_branch_guard_attempt();
            perf_trace::record_commit_branch_guard_rejected();
            return Err(CommitRuntimeError::CommitQuiesceUnavailable {
                reason: "commit quiesce is active",
            });
        }
        if state.active_branches.contains(&branch_id) {
            perf_trace::record_commit_branch_guard_attempt();
            perf_trace::record_commit_branch_guard_rejected();
            return Err(CommitRuntimeError::BranchGuardUnavailable {
                branch_id,
                reason: "branch commit guard is already active",
            });
        }
        Ok(())
    }

    /// Attempts to begin V1 quiesce.
    ///
    /// V1 deliberately fast-fails instead of waiting: the maintenance layer
    /// owns retry/deadline policy above this crate-private token. Once a token
    /// is returned, new mutating branch guards are rejected until the token is
    /// dropped.
    pub(crate) fn try_begin_quiesce(&self) -> CommitRuntimeResult<CommitQuiesceGuard> {
        perf_trace::record_commit_quiesce_attempt();
        let mut state = self.lock_state()?;
        if state.quiescing {
            perf_trace::record_commit_quiesce_rejected();
            return Err(CommitRuntimeError::CommitQuiesceUnavailable {
                reason: "commit quiesce is already active",
            });
        }
        if !state.active_branches.is_empty() {
            perf_trace::record_commit_quiesce_rejected();
            return Err(CommitRuntimeError::CommitQuiesceUnavailable {
                reason: "commit quiesce cannot start while branch guards are active",
            });
        }
        state.quiescing = true;
        perf_trace::record_commit_quiesce_acquired();
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

    /// Lock the guard state under the commit lock-order bracket. The returned
    /// value holds both the mutex guard and the lock-order token; dropping it
    /// releases the mutex first, then the tracker (see field order below).
    fn lock_state(&self) -> CommitRuntimeResult<TrackedBranchGuardState<'_>> {
        let order = CommitLockOrderGuard::acquire(COMMIT_GUARD_RANK);
        let state = self
            .inner
            .lock()
            .map_err(|_| CommitRuntimeError::InvalidCommitState {
                reason: "commit branch guard state lock is poisoned",
            })?;
        Ok(TrackedBranchGuardState {
            state,
            _order: order,
        })
    }
}

/// A held guard-state lock, bracketed by the commit lock-order tracker. Field
/// order is load-bearing: `state` (the mutex guard) drops before `_order`, so
/// the mutex is released before the tracker records the release.
struct TrackedBranchGuardState<'a> {
    state: MutexGuard<'a, CommitBranchGuardState>,
    _order: CommitLockOrderGuard,
}

impl Deref for TrackedBranchGuardState<'_> {
    type Target = CommitBranchGuardState;
    fn deref(&self) -> &Self::Target {
        &self.state
    }
}

impl DerefMut for TrackedBranchGuardState<'_> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.state
    }
}

impl CommitBranchGuard {
    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }
}

impl Drop for CommitBranchGuard {
    fn drop(&mut self) {
        let _order = CommitLockOrderGuard::acquire(COMMIT_GUARD_RANK);
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
        let _order = CommitLockOrderGuard::acquire(COMMIT_GUARD_RANK);
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

#[cfg(test)]
mod lock_order_tests {
    use super::CommitBranchGuardSet;
    use strata_core::BranchId;

    fn branch(byte: u8) -> BranchId {
        BranchId::from_bytes([byte; BranchId::BYTE_LEN])
    }

    /// Driving test for the commit lock-order instrumentation (#2636): the real
    /// guard API — held RAII tokens across branches, quiesce, and token drops —
    /// must not trip the lock-order tracker. The tracker's own unit tests
    /// (`commit::lock_order::tests`) prove it *fires* on a nested acquisition;
    /// this proves it does *not* false-positive on legitimate use, so the
    /// instrumentation is correctly bracketed around each brief mutex hold.
    #[test]
    fn legitimate_guard_use_never_trips_the_lock_order_tracker() {
        let guard_set = CommitBranchGuardSet::new();

        // Independent-branch tokens held simultaneously: each acquisition takes
        // and releases the guard mutex as a leaf, so holding both tokens at once
        // is legal (the tokens are logical, not mutex guards).
        let first = guard_set
            .try_acquire_branch_guard(branch(0x01))
            .expect("first branch guard");
        let second = guard_set
            .try_acquire_branch_guard(branch(0x02))
            .expect("second branch guard");

        // Introspection while tokens are held re-locks the mutex briefly — a
        // legal sequential leaf acquisition, not a nested one.
        assert_eq!(guard_set.active_guard_count().expect("count"), 2);
        assert!(!guard_set.is_quiescing().expect("quiescing"));

        // Dropping a token re-locks the mutex in Drop: still a leaf.
        drop(first);
        assert_eq!(guard_set.active_guard_count().expect("count"), 1);
        drop(second);

        // Quiesce takes the mutex, returns a token, releases the mutex; the
        // token drop re-locks. All leaves.
        let quiesce = guard_set.try_begin_quiesce().expect("begin quiesce");
        assert!(guard_set.is_quiescing().expect("quiescing"));
        drop(quiesce);
        assert!(!guard_set.is_quiescing().expect("quiescing"));
    }
}
