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

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BranchStatus {
    Active,
    Deleted,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BranchOperation {
    Created,
    Listed,
    Described,
    Forked,
    Cleared,
    Deleted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BranchParentSummary {
    source_branch_id: BranchId,
    fork_version: CommitVersion,
}

impl BranchParentSummary {
    #[must_use]
    pub const fn new(source_branch_id: BranchId, fork_version: CommitVersion) -> Self {
        Self {
            source_branch_id,
            fork_version,
        }
    }

    #[must_use]
    pub const fn source_branch_id(self) -> BranchId {
        self.source_branch_id
    }

    #[must_use]
    pub const fn fork_version(self) -> CommitVersion {
        self.fork_version
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BranchSummary {
    branch_id: BranchId,
    generation: BranchGeneration,
    status: BranchStatus,
    parent: Option<BranchParentSummary>,
    created_at: Option<CommitVersion>,
    deleted_at: Option<CommitVersion>,
    state_revision: u64,
}

impl BranchSummary {
    #[must_use]
    pub(crate) const fn new(
        branch_id: BranchId,
        generation: BranchGeneration,
        status: BranchStatus,
        parent: Option<BranchParentSummary>,
        created_at: Option<CommitVersion>,
        deleted_at: Option<CommitVersion>,
        state_revision: u64,
    ) -> Self {
        Self {
            branch_id,
            generation,
            status,
            parent,
            created_at,
            deleted_at,
            state_revision,
        }
    }

    #[must_use]
    pub const fn branch_id(self) -> BranchId {
        self.branch_id
    }

    #[must_use]
    pub const fn generation(self) -> BranchGeneration {
        self.generation
    }

    #[must_use]
    pub const fn status(self) -> BranchStatus {
        self.status
    }

    #[must_use]
    pub const fn parent(self) -> Option<BranchParentSummary> {
        self.parent
    }

    #[must_use]
    pub const fn created_at(self) -> Option<CommitVersion> {
        self.created_at
    }

    #[must_use]
    pub const fn deleted_at(self) -> Option<CommitVersion> {
        self.deleted_at
    }

    #[must_use]
    pub const fn state_revision(self) -> u64 {
        self.state_revision
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BranchCleanupSummary {
    removed_refs: usize,
    releasable_tables: usize,
    protected_tables: usize,
}

impl BranchCleanupSummary {
    #[must_use]
    pub(crate) const fn new(
        removed_refs: usize,
        releasable_tables: usize,
        protected_tables: usize,
    ) -> Self {
        Self {
            removed_refs,
            releasable_tables,
            protected_tables,
        }
    }

    #[must_use]
    pub const fn removed_refs(self) -> usize {
        self.removed_refs
    }

    #[must_use]
    pub const fn releasable_tables(self) -> usize {
        self.releasable_tables
    }

    #[must_use]
    pub const fn protected_tables(self) -> usize {
        self.protected_tables
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BranchOutcome {
    operation: BranchOperation,
    branches: Vec<BranchSummary>,
    generation_before: Option<BranchGeneration>,
    generation_after: Option<BranchGeneration>,
    source_branch_id: Option<BranchId>,
    fork_version: Option<CommitVersion>,
    fork_timestamp: Option<Timestamp>,
    cleanup: Option<BranchCleanupSummary>,
}

impl BranchOutcome {
    #[must_use]
    pub(crate) fn new(operation: BranchOperation, branches: Vec<BranchSummary>) -> Self {
        Self {
            operation,
            branches,
            generation_before: None,
            generation_after: None,
            source_branch_id: None,
            fork_version: None,
            fork_timestamp: None,
            cleanup: None,
        }
    }

    #[must_use]
    pub(crate) const fn with_generations(
        mut self,
        generation_before: Option<BranchGeneration>,
        generation_after: Option<BranchGeneration>,
    ) -> Self {
        self.generation_before = generation_before;
        self.generation_after = generation_after;
        self
    }

    #[must_use]
    pub(crate) const fn with_fork_facts(
        mut self,
        source_branch_id: BranchId,
        fork_version: CommitVersion,
        fork_timestamp: Option<Timestamp>,
    ) -> Self {
        self.source_branch_id = Some(source_branch_id);
        self.fork_version = Some(fork_version);
        self.fork_timestamp = fork_timestamp;
        self
    }

    #[must_use]
    pub(crate) const fn with_cleanup(mut self, cleanup: BranchCleanupSummary) -> Self {
        self.cleanup = Some(cleanup);
        self
    }

    #[must_use]
    pub const fn operation(&self) -> BranchOperation {
        self.operation
    }

    #[must_use]
    pub fn branches(&self) -> &[BranchSummary] {
        &self.branches
    }

    #[must_use]
    pub fn branch(&self) -> Option<BranchSummary> {
        self.branches.first().copied()
    }

    #[must_use]
    pub const fn generation_before(&self) -> Option<BranchGeneration> {
        self.generation_before
    }

    #[must_use]
    pub const fn generation_after(&self) -> Option<BranchGeneration> {
        self.generation_after
    }

    #[must_use]
    pub const fn source_branch_id(&self) -> Option<BranchId> {
        self.source_branch_id
    }

    #[must_use]
    pub const fn fork_version(&self) -> Option<CommitVersion> {
        self.fork_version
    }

    #[must_use]
    pub const fn fork_timestamp(&self) -> Option<Timestamp> {
        self.fork_timestamp
    }

    #[must_use]
    pub const fn cleanup(&self) -> Option<BranchCleanupSummary> {
        self.cleanup
    }
}
