//! Deterministic storage-fault injection for engine boundary tests.
//!
//! [`FaultOp`] names the persistence operations a test can target. The schedule
//! and the public [`StorageFaultKind`] selector are test-only: in production
//! builds `guard_fault` compiles to a no-op, so the seam adds no behavior to a
//! shipped engine. Faults are fabricated as storage errors and flow through the
//! real `map_storage_error`, so tests exercise the genuine error-translation
//! path rather than a stand-in.

/// Persistence operation a fault can target.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum FaultOp {
    Commit,
    Read,
    Scan,
    Branch,
}

#[cfg(any(test, feature = "testkit"))]
pub(crate) use injection::FaultSchedule;
#[cfg(any(test, feature = "testkit"))]
pub use injection::StorageFaultKind;

#[cfg(any(test, feature = "testkit"))]
mod injection {
    use strata_core::BranchId;
    use strata_storage::api::{StorageApiError, StorageApiLowerLayer};

    use super::FaultOp;

    /// The storage failure a test wants the next matching operation to surface.
    ///
    /// Each variant maps to a concrete `StorageApiError`, so the engine's real
    /// `map_storage_error` decides the resulting engine status.
    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub enum StorageFaultKind {
        /// Retryable resource-budget exhaustion.
        ResourceExhausted,
        /// Commit whose durability could not be proven.
        AmbiguousCommit,
        /// Degraded recovery (corruption-class) signal.
        RecoveryDegraded,
        /// Retryable lower-layer unavailability.
        Unavailable,
        /// Target was not found.
        NotFound,
        /// Request conflicted with existing state.
        Conflict,
    }

    impl StorageFaultKind {
        fn into_storage_error(self) -> StorageApiError {
            match self {
                Self::ResourceExhausted => StorageApiError::ResourceExhausted {
                    resource: "memory",
                    requested_bytes: 0,
                    used_bytes: 0,
                    limit_bytes: 0,
                    reason: "injected storage fault",
                },
                Self::AmbiguousCommit => {
                    StorageApiError::durable_uncertain("injected storage fault")
                }
                Self::RecoveryDegraded => StorageApiError::RecoveryDegraded {
                    reason: "injected storage fault",
                },
                Self::Unavailable => StorageApiError::lower_layer_with(
                    StorageApiLowerLayer::Service,
                    "injected storage fault",
                    std::io::Error::other("injected storage fault"),
                ),
                Self::NotFound => StorageApiError::BranchNotFound {
                    branch_id: BranchId::from_bytes([0; BranchId::BYTE_LEN]),
                },
                Self::Conflict => StorageApiError::Conflict {
                    branch_id: BranchId::from_bytes([0; BranchId::BYTE_LEN]),
                    storage_space: None,
                    key_fingerprint: None,
                    user_key_len: None,
                    reason: "injected storage fault",
                },
            }
        }
    }

    struct ScheduledFault {
        op: FaultOp,
        error: StorageApiError,
        /// Number of matching operations that pass before this fault fires.
        skip: usize,
    }

    /// FIFO schedule of pending faults, matched and consumed per operation.
    #[derive(Default)]
    pub(crate) struct FaultSchedule {
        entries: Vec<ScheduledFault>,
    }

    impl FaultSchedule {
        /// Arms a fault that fires after `skip` matching operations pass.
        pub(crate) fn arm(&mut self, op: FaultOp, kind: StorageFaultKind, skip: usize) {
            self.entries.push(ScheduledFault {
                op,
                error: kind.into_storage_error(),
                skip,
            });
        }

        /// Returns the fault for `op` if one is due, letting `skip` occurrences
        /// pass through to real storage first.
        pub(crate) fn take(&mut self, op: FaultOp) -> Option<StorageApiError> {
            let position = self.entries.iter().position(|entry| entry.op == op)?;
            if self.entries[position].skip > 0 {
                self.entries[position].skip -= 1;
                return None;
            }
            Some(self.entries.remove(position).error)
        }
    }
}
