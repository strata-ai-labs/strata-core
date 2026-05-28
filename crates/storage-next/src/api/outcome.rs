//! API outcome summaries.

use strata_core_next::{BranchId, CommitVersion, Timestamp};

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageOpenDisposition {
    Created,
    OpenedExisting,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryHealthSummary {
    Healthy,
    Degraded,
    Failed,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageRuntimeState {
    Open,
    Closed,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageOpenSummary {
    disposition: StorageOpenDisposition,
    recovery_health: RecoveryHealthSummary,
    recovered_visible_version: Option<CommitVersion>,
}

impl StorageOpenSummary {
    #[must_use]
    pub const fn new(
        disposition: StorageOpenDisposition,
        recovery_health: RecoveryHealthSummary,
        recovered_visible_version: Option<CommitVersion>,
    ) -> Self {
        Self {
            disposition,
            recovery_health,
            recovered_visible_version,
        }
    }

    #[must_use]
    pub const fn disposition(self) -> StorageOpenDisposition {
        self.disposition
    }

    #[must_use]
    pub const fn recovery_health(self) -> RecoveryHealthSummary {
        self.recovery_health
    }

    #[must_use]
    pub const fn recovered_visible_version(self) -> Option<CommitVersion> {
        self.recovered_visible_version
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageCloseSummary {
    state: StorageRuntimeState,
    idempotent: bool,
}

impl StorageCloseSummary {
    #[must_use]
    pub const fn new(state: StorageRuntimeState, idempotent: bool) -> Self {
        Self { state, idempotent }
    }

    #[must_use]
    pub const fn state(self) -> StorageRuntimeState {
        self.state
    }

    #[must_use]
    pub const fn idempotent(self) -> bool {
        self.idempotent
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommitSummary {
    branch_id: BranchId,
    commit_version: CommitVersion,
    commit_timestamp: Timestamp,
}

impl CommitSummary {
    #[must_use]
    pub const fn new(
        branch_id: BranchId,
        commit_version: CommitVersion,
        commit_timestamp: Timestamp,
    ) -> Self {
        Self {
            branch_id,
            commit_version,
            commit_timestamp,
        }
    }

    #[must_use]
    pub const fn branch_id(self) -> BranchId {
        self.branch_id
    }

    #[must_use]
    pub const fn commit_version(self) -> CommitVersion {
        self.commit_version
    }

    #[must_use]
    pub const fn commit_timestamp(self) -> Timestamp {
        self.commit_timestamp
    }
}
