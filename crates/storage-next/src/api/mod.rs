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
pub use branch::{BranchAction, BranchRequest};
pub use commit::{CommitBatch, CommitMutation, CommitOptions};
pub use diagnostics::{DiagnosticsRequest, DiagnosticsScope};
pub use error::{StorageApiError, StorageApiErrorClass, StorageApiLowerLayer};
pub use maintenance::{MaintenanceRequest, MaintenanceScope, MaintenanceTask};
pub use options::{
    StorageBudgetPolicy, StorageDurabilityPolicy, StorageMode, StorageOpenOptions,
    StorageWalGrowthPolicy,
};
pub use outcome::{
    CommitSummary, RecoveryHealthSummary, StorageCloseSummary, StorageOpenDisposition,
    StorageOpenOutcome, StorageOpenSummary, StorageRuntimeState,
};
pub use read::{PointReadRequest, ReadBound, ScanReadRequest};
pub use result::StorageApiResult;
pub use runtime::{StorageCloseOptions, StorageRuntime};
pub use strata_core_next::{BranchId, CommitVersion, Timestamp};

#[cfg(test)]
mod tests;
