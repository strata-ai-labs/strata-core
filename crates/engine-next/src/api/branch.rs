//! Branch API DTOs.

use strata_core_next::{BranchId, CommitVersion, Timestamp};

use crate::branch::catalog::{
    BranchCatalogRecord, BranchParentRecord as CatalogBranchParentRecord,
    BranchStatus as CatalogBranchStatus,
};
use crate::branch::BranchName;

/// Product branch status exposed by the engine.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BranchStatus {
    /// Branch accepts reads and writes.
    Active,
    /// Branch was deleted and is no longer returned by normal listing.
    Deleted,
}

/// Fork parent facts for a product branch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BranchParentSummary {
    name: BranchName,
    branch_id: BranchId,
    generation: u64,
    fork_version: CommitVersion,
    fork_timestamp: Option<Timestamp>,
}

impl BranchParentSummary {
    pub(crate) fn from_catalog(record: &CatalogBranchParentRecord) -> Self {
        Self {
            name: record.name().clone(),
            branch_id: record.branch_id(),
            generation: record.generation(),
            fork_version: record.fork_version(),
            fork_timestamp: record.fork_timestamp(),
        }
    }

    #[must_use]
    /// Returns the parent branch name.
    pub fn name(&self) -> &BranchName {
        &self.name
    }

    #[must_use]
    /// Returns the parent branch id.
    pub const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    #[must_use]
    /// Returns the parent branch generation at fork time.
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    /// Returns the source commit version used as the fork point.
    pub const fn fork_version(&self) -> CommitVersion {
        self.fork_version
    }

    #[must_use]
    /// Returns the source timestamp used to resolve the fork point, when any.
    pub const fn fork_timestamp(&self) -> Option<Timestamp> {
        self.fork_timestamp
    }
}

/// Product branch summary exposed to executor layers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BranchSummary {
    name: BranchName,
    branch_id: BranchId,
    generation: u64,
    status: BranchStatus,
    parent: Option<BranchParentSummary>,
    created_at: Option<CommitVersion>,
    deleted_at: Option<CommitVersion>,
    state_revision: u64,
}

impl BranchSummary {
    pub(crate) fn from_catalog(record: &BranchCatalogRecord) -> Self {
        Self {
            name: record.name().clone(),
            branch_id: record.branch_id(),
            generation: record.generation(),
            status: branch_status_from_catalog(record.status()),
            parent: record.parent().map(BranchParentSummary::from_catalog),
            created_at: record.created_at(),
            deleted_at: record.deleted_at(),
            state_revision: record.state_revision(),
        }
    }

    #[must_use]
    /// Returns the product branch name.
    pub fn name(&self) -> &BranchName {
        &self.name
    }

    #[must_use]
    /// Returns the engine branch id.
    pub const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    #[must_use]
    /// Returns the branch generation tracked by the engine catalog.
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    #[must_use]
    /// Returns the status tracked by the engine catalog.
    pub const fn status(&self) -> BranchStatus {
        self.status
    }

    #[must_use]
    /// Returns fork parent facts, when this branch was forked.
    pub const fn parent(&self) -> Option<&BranchParentSummary> {
        self.parent.as_ref()
    }

    #[must_use]
    /// Returns the storage creation version when storage reports it.
    pub const fn created_at(&self) -> Option<CommitVersion> {
        self.created_at
    }

    #[must_use]
    /// Returns the storage deletion version when storage reports it.
    pub const fn deleted_at(&self) -> Option<CommitVersion> {
        self.deleted_at
    }

    #[must_use]
    /// Returns the storage state revision captured with this summary.
    pub const fn state_revision(&self) -> u64 {
        self.state_revision
    }
}

const fn branch_status_from_catalog(value: CatalogBranchStatus) -> BranchStatus {
    match value {
        CatalogBranchStatus::Active => BranchStatus::Active,
        CatalogBranchStatus::Deleted => BranchStatus::Deleted,
    }
}

/// Outcome returned after creating a product branch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BranchCreateOutcome {
    branch: BranchSummary,
}

/// Branch cleanup facts returned after deletion.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct BranchCleanupSummary {
    removed_refs: usize,
    releasable_tables: usize,
    protected_tables: usize,
}

impl BranchCleanupSummary {
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
    /// Returns the number of removed storage references.
    pub const fn removed_refs(self) -> usize {
        self.removed_refs
    }

    #[must_use]
    /// Returns the number of tables storage can release.
    pub const fn releasable_tables(self) -> usize {
        self.releasable_tables
    }

    #[must_use]
    /// Returns the number of tables protected by retained readers.
    pub const fn protected_tables(self) -> usize {
        self.protected_tables
    }
}

/// Outcome returned after deleting a product branch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BranchDeleteOutcome {
    branch: BranchSummary,
    generation_before: Option<u64>,
    generation_after: Option<u64>,
    cleanup: Option<BranchCleanupSummary>,
}

impl BranchDeleteOutcome {
    pub(crate) const fn new(
        branch: BranchSummary,
        generation_before: Option<u64>,
        generation_after: Option<u64>,
        cleanup: Option<BranchCleanupSummary>,
    ) -> Self {
        Self {
            branch,
            generation_before,
            generation_after,
            cleanup,
        }
    }

    #[must_use]
    /// Returns the deleted branch summary.
    pub const fn branch(&self) -> &BranchSummary {
        &self.branch
    }

    #[must_use]
    /// Returns the generation before delete, when storage reported it.
    pub const fn generation_before(&self) -> Option<u64> {
        self.generation_before
    }

    #[must_use]
    /// Returns the generation after delete, when storage reported it.
    pub const fn generation_after(&self) -> Option<u64> {
        self.generation_after
    }

    #[must_use]
    /// Returns storage cleanup facts.
    pub const fn cleanup(&self) -> Option<BranchCleanupSummary> {
        self.cleanup
    }
}

impl BranchCreateOutcome {
    pub(crate) const fn new(branch: BranchSummary) -> Self {
        Self { branch }
    }

    #[must_use]
    /// Returns the created branch summary.
    pub const fn branch(&self) -> &BranchSummary {
        &self.branch
    }
}
