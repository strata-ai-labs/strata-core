//! Internal commit pipeline and timeline mechanics.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "commit runtime scaffolding is consumed by later commit-layer work"
    )
)]

mod allocator;
mod batch;
mod branch_registry;
mod config;
mod error;
mod facts;
mod guard;
mod outcome;
mod result;
mod visibility;

#[allow(
    unused_imports,
    reason = "commit scaffold exports define the local surface for later slices"
)]
pub(crate) use allocator::{
    CommitFactAllocation, CommitFactAllocator, CommitManualTimestampSource,
    CommitTimestampAllocationSource, CommitTimestampGuard, CommitTimestampSource,
    CommitVersionAllocator,
};
#[allow(
    unused_imports,
    reason = "commit scaffold exports define the local surface for later slices"
)]
pub(crate) use batch::{
    CommitBatch, CommitBatchKind, CommitBatchOptions, CommitCasFact, CommitConflictValidationMode,
    CommitDuplicateKeyPolicy, CommitDurabilityMode, CommitExpiry, CommitMutation,
    CommitObservedVersion, CommitOrigin, CommitReadFact, CommitRetentionHint, CommitStamp,
    CommitTimestampPolicy, CommitValidationFacts, StampedCommitRows, ValidatedCommitBatch,
};
#[allow(
    unused_imports,
    reason = "commit scaffold exports define the local surface for later slices"
)]
pub(crate) use branch_registry::{
    admit_mutating_commit, CommitBranchAdmission, CommitBranchAdmissionGuard,
    CommitBranchDescriptor, CommitBranchGeneration, CommitBranchGenerationGuard,
    CommitBranchRegistry, CommitBranchState,
};
#[cfg_attr(
    not(test),
    allow(
        unused_imports,
        reason = "commit scaffold exports define the local surface for later slices"
    )
)]
pub(crate) use config::{CommitReadOnlyDiagnostics, CommitRuntimeConfig};
#[cfg_attr(
    not(test),
    allow(
        unused_imports,
        reason = "commit scaffold exports define the local surface for later slices"
    )
)]
pub(crate) use error::{CommitLowerLayer, CommitRuntimeError};
#[cfg_attr(
    not(test),
    allow(
        unused_imports,
        reason = "commit scaffold exports define the local surface for later slices"
    )
)]
pub(crate) use facts::{
    CommitDurabilityClass, CommitPhase, CommitRuntimeStats, CommitVisibilityFacts,
};
#[allow(
    unused_imports,
    reason = "commit scaffold exports define the local surface for later slices"
)]
pub(crate) use guard::{CommitBranchGuard, CommitBranchGuardSet, CommitQuiesceGuard};
#[allow(
    unused_imports,
    reason = "commit scaffold exports define the local surface for later slices"
)]
pub(crate) use outcome::{
    execute_read_only_diagnostic, CommitMutationCounts, CommitOutcome, CommitOutcomeKind,
    CommitReadSnapshot,
};
#[cfg_attr(
    not(test),
    allow(
        unused_imports,
        reason = "commit scaffold exports define the local surface for later slices"
    )
)]
pub(crate) use result::CommitRuntimeResult;
#[allow(
    unused_imports,
    reason = "commit scaffold exports define the local surface for later slices"
)]
pub(crate) use visibility::{VisibleVersionPublish, VisibleVersionTracker};

#[cfg(test)]
mod tests;
