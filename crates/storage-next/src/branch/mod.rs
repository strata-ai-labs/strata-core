//! Branch-aware visibility and inheritance mechanics.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "branch runtime scaffolding is consumed by later branch-layer work"
    )
)]

mod config;
mod error;
mod facts;
mod identity;
mod read;
mod state;

#[cfg_attr(
    not(test),
    allow(
        unused_imports,
        reason = "branch scaffold exports define the local surface for later slices"
    )
)]
pub(crate) use config::BranchRuntimeConfig;
#[cfg_attr(
    not(test),
    allow(
        unused_imports,
        reason = "branch scaffold exports define the local surface for later slices"
    )
)]
pub(crate) use error::{BranchRuntimeError, BranchRuntimeResult, BranchTimestampHistorySource};
#[allow(
    unused_imports,
    reason = "branch scaffold exports define the local surface for later slices"
)]
pub(crate) use facts::{
    BranchLevel, BranchProtectedTable, BranchProtectionReason, BranchReachabilityAggregate,
    BranchReachabilityFacts, BranchReachabilitySnapshot, BranchReleasePlan, BranchRuntimeStats,
    BranchStateFacts, BranchTableDescriptor, BranchTableProtection, BranchTableRef,
    BranchTableReferenceKind, InheritedLayerDescriptor, InheritedLayerStatus, SharedTableRegistry,
};
#[cfg_attr(
    not(test),
    allow(
        unused_imports,
        reason = "branch scaffold exports define the local surface for later slices"
    )
)]
pub(crate) use identity::{
    require_physical_key_branch, require_row_branch, rewrite_physical_key_branch,
    rewrite_row_branch, row_matches_branch, BranchRowIdentity,
};
#[cfg_attr(
    not(test),
    allow(
        unused_imports,
        reason = "branch scaffold exports define the local surface for later slices"
    )
)]
pub(crate) use read::{
    BranchEffectiveReadBound, BranchHistoryOptions, BranchHistoryRow, BranchInheritedLayer,
    BranchMaterializationSource, BranchOwnedTable, BranchReadBound, BranchReadView,
    BranchRowBoundMatch, BranchRowCandidateFacts, BranchRowSource, BranchScanBounds,
    BranchTimestampCoverage, BranchUserKeyBound, BranchVisibleRow,
};
#[cfg_attr(
    not(test),
    allow(
        unused_imports,
        reason = "branch scaffold exports define the local surface for later slices"
    )
)]
pub(crate) use state::{
    install_snapshot_rows_into_branches, BranchAppendOutcome, BranchCompactionCandidate,
    BranchCompactionKind, BranchCompactionNoopReason, BranchCompactionOutcome,
    BranchCompactionPlan, BranchCompactionRecovery, BranchCompactionRequest,
    BranchCompactionRetentionPolicy, BranchForkOutcome, BranchImmutableInstallOutcome,
    BranchMaterializationOutcome, BranchMaterializationRecovery, BranchMaterializationRequest,
    BranchSnapshotInstallBranchOutcome, BranchSnapshotInstallGroup, BranchSnapshotInstallOutcome,
    BranchSnapshotInstallRecovery, BranchSnapshotInstallRequest, BranchSnapshotMissingBranchPolicy,
    BranchSnapshotTargetStatePolicy,
};
#[cfg_attr(
    all(not(test), not(feature = "testkit")),
    allow(
        unused_imports,
        reason = "branch scaffold exports define the local surface for later slices"
    )
)]
pub(crate) use state::{
    BranchLocalState, BranchRotationOutcome, BranchRotationSkipReason, BranchStateDescriptor,
    BranchViewDescriptor,
};

#[cfg(test)]
mod tests;
