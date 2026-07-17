//! Commit-runtime lock-order enforcement (commit runtime, #2636).
//!
//! The commit runtime's lock discipline is documented in `guard.rs`: the
//! branch-admission guard mutex and the unresolved-durable gate mutex are each
//! held only briefly as *leaves* — never nested under one another, and never
//! while a WAL, branch-state, visible-version, or read-view lock is acquired.
//! Before this module that discipline was prose only; an out-of-order
//! acquisition in a future change would be a production deadlock no test
//! catches.
//!
//! This tracks the commit locks a thread holds and, under `debug_assertions`,
//! asserts each new acquisition respects the order. It is compiled out entirely
//! in release builds (the guard becomes a zero-sized no-op), so it costs the
//! hot commit path nothing while making the invariant a debug-time trap and a
//! driving-test target.
//!
//! Ranks are strictly increasing along the *legal* acquisition order. Acquiring
//! a lock whose rank is not strictly greater than every rank the thread already
//! holds is a violation. The two commit mutexes share a rank because the
//! documented rule is that they are mutually exclusive — neither may be held
//! while the other is.

/// Rank of the branch-admission guard mutex (`CommitBranchGuardSet`) and the
/// unresolved-durable gate mutex (`CommitUnresolvedDurableGate`). They share a
/// rank: the documented invariant is that at most one is held at a time, and
/// neither is re-entered while held.
pub(crate) const COMMIT_GUARD_RANK: u8 = 10;

#[cfg(debug_assertions)]
mod tracking {
    use std::cell::RefCell;

    thread_local! {
        /// Ranks of the commit locks the current thread holds, in acquisition
        /// order. A stack because a *legal* chain is strictly increasing.
        static HELD: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    }

    pub(super) fn enter(rank: u8) {
        HELD.with_borrow_mut(|held| {
            if let Some(&top) = held.last() {
                assert!(
                    rank > top,
                    "commit lock-order violation: acquiring a rank-{rank} commit lock while \
                     holding rank-{top} (locks of equal or higher rank are mutually exclusive; \
                     see commit/guard.rs and the commit-runtime contract)"
                );
            }
            held.push(rank);
        });
    }

    pub(super) fn exit(rank: u8) {
        HELD.with_borrow_mut(|held| {
            let popped = held.pop();
            debug_assert_eq!(
                popped,
                Some(rank),
                "commit lock-order tracker imbalance: released a rank-{rank} lock that was not \
                 the most recently acquired"
            );
        });
    }
}

/// RAII bracket asserting the commit lock at `rank` may be acquired now. Hold it
/// for exactly the duration the underlying mutex guard is held (declare it
/// *before* acquiring the guard so the guard drops first). A no-op in release.
#[must_use = "the lock-order guard must live for the mutex-hold scope"]
pub(crate) struct CommitLockOrderGuard {
    #[cfg(debug_assertions)]
    rank: u8,
}

impl CommitLockOrderGuard {
    #[cfg(debug_assertions)]
    pub(crate) fn acquire(rank: u8) -> Self {
        tracking::enter(rank);
        Self { rank }
    }

    #[cfg(not(debug_assertions))]
    pub(crate) fn acquire(_rank: u8) -> Self {
        Self {}
    }
}

impl Drop for CommitLockOrderGuard {
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        tracking::exit(self.rank);
    }
}

#[cfg(test)]
mod tests {
    use super::{CommitLockOrderGuard, COMMIT_GUARD_RANK};

    /// Acquiring the commit guard as a leaf, releasing it, and acquiring it
    /// again sequentially is legal — the tracker only trips on nesting.
    #[test]
    fn sequential_leaf_acquisitions_are_allowed() {
        {
            let _g = CommitLockOrderGuard::acquire(COMMIT_GUARD_RANK);
        }
        {
            let _g = CommitLockOrderGuard::acquire(COMMIT_GUARD_RANK);
        }
    }

    /// Acquiring a same-rank commit lock while already holding one is the
    /// deadlock-shaped violation the tracker exists to catch (the two commit
    /// mutexes are mutually exclusive; re-entrancy is likewise forbidden).
    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "commit lock-order violation")]
    fn nested_same_rank_acquisition_panics() {
        let _outer = CommitLockOrderGuard::acquire(COMMIT_GUARD_RANK);
        let _inner = CommitLockOrderGuard::acquire(COMMIT_GUARD_RANK);
    }

    /// A lower rank may nest under nothing, but a strictly-increasing chain is
    /// legal — this pins the ordering direction so a future second commit lock
    /// added *below* the guard is caught.
    #[cfg(debug_assertions)]
    #[test]
    fn strictly_increasing_chain_is_allowed() {
        let _low = CommitLockOrderGuard::acquire(COMMIT_GUARD_RANK - 1);
        let _high = CommitLockOrderGuard::acquire(COMMIT_GUARD_RANK);
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "commit lock-order violation")]
    fn decreasing_chain_panics() {
        let _high = CommitLockOrderGuard::acquire(COMMIT_GUARD_RANK);
        let _low = CommitLockOrderGuard::acquire(COMMIT_GUARD_RANK - 1);
    }
}
