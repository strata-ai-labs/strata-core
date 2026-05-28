//! API branch request shells.

use strata_core_next::{BranchId, CommitVersion, Timestamp};

use super::BranchGeneration;

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BranchAction {
    Create,
    Describe,
    List,
    ForkCurrent {
        source: BranchId,
    },
    ForkAtVersion {
        source: BranchId,
        version: CommitVersion,
    },
    ForkAtTimestamp {
        source: BranchId,
        timestamp: Timestamp,
    },
    Clear,
    Delete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BranchRequest {
    branch_id: BranchId,
    action: BranchAction,
    expected_generation: Option<BranchGeneration>,
}

impl BranchRequest {
    #[must_use]
    pub const fn new(
        branch_id: BranchId,
        action: BranchAction,
        expected_generation: Option<BranchGeneration>,
    ) -> Self {
        Self {
            branch_id,
            action,
            expected_generation,
        }
    }

    #[must_use]
    pub const fn branch_id(self) -> BranchId {
        self.branch_id
    }

    #[must_use]
    pub const fn action(self) -> BranchAction {
        self.action
    }

    #[must_use]
    pub const fn expected_generation(self) -> Option<BranchGeneration> {
        self.expected_generation
    }
}
