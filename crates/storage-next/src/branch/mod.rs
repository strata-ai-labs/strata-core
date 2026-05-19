//! Branch-aware visibility and inheritance mechanics.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "branch runtime scaffolding is consumed by later M4 branch slices"
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
pub(crate) use error::{BranchRuntimeError, BranchRuntimeResult};
#[cfg_attr(
    not(test),
    allow(
        unused_imports,
        reason = "branch scaffold exports define the local surface for later slices"
    )
)]
pub(crate) use facts::{
    BranchLevel, BranchReachabilityFacts, BranchRuntimeStats, BranchStateFacts,
    BranchTableDescriptor, InheritedLayerDescriptor, InheritedLayerStatus,
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
    BranchOwnedTable, BranchReadBound, BranchReadView, BranchRowBoundMatch,
    BranchRowCandidateFacts, BranchRowSource, BranchScanBounds, BranchUserKeyBound,
    BranchVisibleRow,
};
#[cfg_attr(
    not(test),
    allow(
        unused_imports,
        reason = "branch scaffold exports define the local surface for later slices"
    )
)]
pub(crate) use state::{BranchAppendOutcome, BranchForkOutcome, BranchImmutableInstallOutcome};
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
