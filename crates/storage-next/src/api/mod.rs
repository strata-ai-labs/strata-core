//! Engine-facing storage API boundary.

#![allow(
    missing_docs,
    reason = "the storage API scaffold exposes boundary vocabulary before behavior slices add full rustdoc"
)]

mod atoms;
mod backend;
mod branch;
mod commit;
mod diagnostics;
mod error;
mod maintenance;
mod options;
mod outcome;
mod read;
mod result;
mod runtime;

pub use atoms::{BranchGeneration, ReadLimit, ScanRange, StorageKey, StorageSpaceId, StorageValue};
pub use backend::StorageBackend;
pub use branch::{
    BranchAction, BranchCleanupSummary, BranchOperation, BranchOutcome, BranchParentSummary,
    BranchRequest, BranchStatus, BranchSummary,
};
pub use commit::{
    CommitBatch, CommitCondition, CommitDurability, CommitDurabilitySummary, CommitExpectedVersion,
    CommitMutation, CommitOptions,
};
pub use diagnostics::{DiagnosticsRequest, DiagnosticsScope};
pub use error::{StorageApiError, StorageApiErrorClass, StorageApiLowerLayer};
pub use maintenance::{
    MaintenanceDrainSummary, MaintenanceQueueSummary, MaintenanceReasonClass, MaintenanceRequest,
    MaintenanceScope, MaintenanceSummary, MaintenanceSummaryStatus, MaintenanceTask,
    MaintenanceWalGrowthStatus, MaintenanceWalGrowthSummary, MaintenanceWalGrowthTrigger,
};
pub use options::{
    StorageBudgetPolicy, StorageDurabilityPolicy, StorageMode, StorageOpenOptions,
    StorageWalGrowthPolicy,
};
pub use outcome::{
    CommitSummary, RecoveryHealthSummary, StorageCloseSummary, StorageOpenDisposition,
    StorageOpenOutcome, StorageOpenSummary, StorageRuntimeState,
};
pub use read::{
    HistoryReadOutcome, HistoryReadRequest, PointReadOutcome, PointReadRequest,
    PrefixScanReadRequest, ReadBound, ScanReadOutcome, ScanReadRequest, StorageReadRow,
    TimelineBoundsOutcome, TimelineBoundsRequest, TimestampLookupMiss, TimestampLookupOutcome,
    TimestampLookupRequest, VersionLookupOutcome, VersionLookupRequest,
};
pub use result::StorageApiResult;
#[cfg(any(test, feature = "testkit"))]
pub(crate) use runtime::{
    map_commit_error_for_test, map_lifecycle_error_for_test, map_maintenance_outcome_for_test,
};
pub use runtime::{StorageCloseOptions, StorageRuntime};
pub use strata_core_next::{BranchId, CommitVersion, Timestamp};

#[cfg(test)]
mod tests;
