//! Branch API DTOs.

use strata_core::{BranchId, CommitVersion, Timestamp};

use crate::branch::catalog::{
    BranchCatalogRecord, BranchParentRecord as CatalogBranchParentRecord,
    BranchStatus as CatalogBranchStatus,
};
use crate::branch::BranchName;
use crate::data::kv::ProductSpace;

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

/// Selects the branch state a comparison reads. `Current` reads the live head of
/// each branch; `AtTimestamp` reads each branch as of the commit frontier
/// at-or-before the timestamp (a timestamp after a branch's latest raises rather
/// than clamping). Version selectors are intentionally absent: a commit version
/// resolves against a single branch's timeline, so it is not meaningful across
/// two branches.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum BranchStateSelector {
    /// The live head of each branch.
    Current,
    /// A timestamp resolved to each branch's retained version frontier.
    AtTimestamp(Timestamp),
}

/// A data capability covered by a branch comparison.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ComparedCapability {
    /// The key-value capability.
    KeyValue,
    /// The JSON document capability.
    Json,
}

/// One entity that differs between two branches, identified by its
/// space-relative logical key and the commit version observed on the reported
/// side.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComparedEntity {
    identity: Vec<u8>,
    version: CommitVersion,
}

impl ComparedEntity {
    pub(crate) fn new(identity: Vec<u8>, version: CommitVersion) -> Self {
        Self { identity, version }
    }

    #[must_use]
    /// Returns the entity's space-relative logical key.
    pub fn identity(&self) -> &[u8] {
        &self.identity
    }

    #[must_use]
    /// Returns the commit version observed on the reported side.
    pub const fn version(&self) -> CommitVersion {
        self.version
    }
}

/// The differing entities for one capability within one space.
///
/// The comparison is directional from branch A to branch B: `added` are present
/// on B but not A, `removed` are present on A but not B, and `modified` are
/// present on both with differing values.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SpaceComparison {
    space: ProductSpace,
    capability: ComparedCapability,
    added: Vec<ComparedEntity>,
    removed: Vec<ComparedEntity>,
    modified: Vec<ComparedEntity>,
}

impl SpaceComparison {
    pub(crate) fn new(
        space: ProductSpace,
        capability: ComparedCapability,
        added: Vec<ComparedEntity>,
        removed: Vec<ComparedEntity>,
        modified: Vec<ComparedEntity>,
    ) -> Self {
        Self {
            space,
            capability,
            added,
            removed,
            modified,
        }
    }

    #[must_use]
    /// Returns the space this comparison covers.
    pub fn space(&self) -> &ProductSpace {
        &self.space
    }

    #[must_use]
    /// Returns the capability this comparison covers.
    pub const fn capability(&self) -> ComparedCapability {
        self.capability
    }

    #[must_use]
    /// Entities present on branch B but not branch A.
    pub fn added(&self) -> &[ComparedEntity] {
        &self.added
    }

    #[must_use]
    /// Entities present on branch A but not branch B.
    pub fn removed(&self) -> &[ComparedEntity] {
        &self.removed
    }

    #[must_use]
    /// Entities present on both branches with differing values.
    pub fn modified(&self) -> &[ComparedEntity] {
        &self.modified
    }
}

/// The result of comparing two branches: the entities that differ, grouped by
/// capability and space. Derived rows are omitted by default.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BranchComparison {
    branch_a: BranchName,
    branch_b: BranchName,
    spaces: Vec<SpaceComparison>,
}

impl BranchComparison {
    pub(crate) fn new(
        branch_a: BranchName,
        branch_b: BranchName,
        spaces: Vec<SpaceComparison>,
    ) -> Self {
        Self {
            branch_a,
            branch_b,
            spaces,
        }
    }

    #[must_use]
    /// The first branch of the comparison (the `A` side).
    pub fn branch_a(&self) -> &BranchName {
        &self.branch_a
    }

    #[must_use]
    /// The second branch of the comparison (the `B` side).
    pub fn branch_b(&self) -> &BranchName {
        &self.branch_b
    }

    #[must_use]
    /// The per-capability, per-space comparisons that contain at least one
    /// difference.
    pub fn comparisons(&self) -> &[SpaceComparison] {
        &self.spaces
    }

    #[must_use]
    /// Whether the two branches have no differing authored entities.
    pub fn is_empty(&self) -> bool {
        self.spaces.is_empty()
    }
}

/// A conflict-resolution strategy for previewing or promoting a branch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PromotionStrategy {
    /// Refuse the promotion when any conflict exists.
    Strict,
    /// Apply the source side's value or tombstone for each conflict.
    SourceWins,
}

/// How two branches diverged on one entity since their branch point.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConflictKind {
    /// Both sides changed the entity to different present values.
    ValueDivergence,
    /// One side changed the value while the other deleted the entity.
    ModifyDeleteDivergence,
}

/// What the selected strategy would do with a conflict.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConflictStrategyResult {
    /// The conflict blocks the promotion (`Strict`).
    Refused,
    /// The source value or tombstone would overwrite the target (`SourceWins`).
    SourceWins,
}

/// One conflicting entity between two branches, relative to their branch point.
///
/// `source_value` / `target_value` are the current values on each side, with
/// `None` meaning the entity is absent (a deletion/tombstone).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PreviewConflict {
    capability: ComparedCapability,
    space: ProductSpace,
    identity: Vec<u8>,
    source_value: Option<Vec<u8>>,
    target_value: Option<Vec<u8>>,
    kind: ConflictKind,
    strategy_result: ConflictStrategyResult,
}

impl PreviewConflict {
    pub(crate) fn new(
        capability: ComparedCapability,
        space: ProductSpace,
        identity: Vec<u8>,
        source_value: Option<Vec<u8>>,
        target_value: Option<Vec<u8>>,
        kind: ConflictKind,
        strategy_result: ConflictStrategyResult,
    ) -> Self {
        Self {
            capability,
            space,
            identity,
            source_value,
            target_value,
            kind,
            strategy_result,
        }
    }

    /// The capability the conflicting entity belongs to.
    #[must_use]
    pub const fn capability(&self) -> ComparedCapability {
        self.capability
    }

    /// The space the conflicting entity belongs to.
    #[must_use]
    pub fn space(&self) -> &ProductSpace {
        &self.space
    }

    /// The capability's space-relative logical key.
    #[must_use]
    pub fn identity(&self) -> &[u8] {
        &self.identity
    }

    /// The source side's current value, or `None` if deleted.
    #[must_use]
    pub fn source_value(&self) -> Option<&[u8]> {
        self.source_value.as_deref()
    }

    /// The target side's current value, or `None` if deleted.
    #[must_use]
    pub fn target_value(&self) -> Option<&[u8]> {
        self.target_value.as_deref()
    }

    /// How the two sides diverged.
    #[must_use]
    pub const fn kind(&self) -> ConflictKind {
        self.kind
    }

    /// What the selected strategy would do with this conflict.
    #[must_use]
    pub const fn strategy_result(&self) -> ConflictStrategyResult {
        self.strategy_result
    }
}

/// The result of previewing a promotion of `source` into `target`.
///
/// Preview is read-only: it derives the branch point from lineage, runs a
/// three-way comparison, and reports the conflicts that would arise, without
/// mutating either branch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BranchPreview {
    source: BranchName,
    target: BranchName,
    branch_point: CommitVersion,
    strategy: PromotionStrategy,
    conflicts: Vec<PreviewConflict>,
}

impl BranchPreview {
    pub(crate) fn new(
        source: BranchName,
        target: BranchName,
        branch_point: CommitVersion,
        strategy: PromotionStrategy,
        conflicts: Vec<PreviewConflict>,
    ) -> Self {
        Self {
            source,
            target,
            branch_point,
            strategy,
            conflicts,
        }
    }

    /// The branch whose changes would be promoted.
    #[must_use]
    pub fn source(&self) -> &BranchName {
        &self.source
    }

    /// The branch that would receive the promotion.
    #[must_use]
    pub fn target(&self) -> &BranchName {
        &self.target
    }

    /// The derived branch point (shared commit version) the preview compared against.
    #[must_use]
    pub const fn branch_point(&self) -> CommitVersion {
        self.branch_point
    }

    /// The strategy the preview was evaluated under.
    #[must_use]
    pub const fn strategy(&self) -> PromotionStrategy {
        self.strategy
    }

    /// The conflicts a promotion would encounter.
    #[must_use]
    pub fn conflicts(&self) -> &[PreviewConflict] {
        &self.conflicts
    }

    /// Whether the promotion is conflict-free.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.conflicts.is_empty()
    }
}
