//! Storage lifecycle coordination.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "lifecycle scaffold is consumed by later lifecycle slices"
    )
)]

mod capability;
mod config;
mod error;
mod facts;
mod health;
mod outcome;
mod result;
mod state;

#[allow(
    unused_imports,
    reason = "lifecycle scaffold exports define the local surface for later slices"
)]
pub(crate) use capability::{
    validate_backend_capabilities_for_open, validate_storage_mode_capabilities,
    LifecycleCapabilityOutcome, ObjectDurableFenceMode,
};
#[allow(
    unused_imports,
    reason = "lifecycle scaffold exports define the local surface for later slices"
)]
pub(crate) use config::{
    LifecycleCloseTimeoutPolicy, LifecycleConfig, LifecycleLossyRecoveryPolicy,
};
#[allow(
    unused_imports,
    reason = "lifecycle scaffold exports define the local surface for later slices"
)]
pub(crate) use error::{LifecycleError, LifecycleLowerLayer};
#[allow(
    unused_imports,
    reason = "lifecycle scaffold exports define the local surface for later slices"
)]
pub(crate) use facts::{
    ClosePhase, LifecycleCodecId, LifecycleState, LifecycleStats, MaintenanceTaskKind,
    QuarantineStage, RecoveryStrictness, RetentionDecision, StorageMode, StorageOpenPlan,
};
#[allow(
    unused_imports,
    reason = "lifecycle scaffold exports define the local surface for later slices"
)]
pub(crate) use health::{
    RecoveryDegradationClass, RecoveryFault, RecoveryFaultKind, RecoveryHealth,
};
#[allow(
    unused_imports,
    reason = "lifecycle scaffold exports define the local surface for later slices"
)]
pub(crate) use outcome::{
    CloseOutcome, CloseOutcomeStatus, MaintenanceOutcome, MaintenanceOutcomeStatus,
    StorageOpenDisposition, StorageOpenOutcome,
};
#[allow(
    unused_imports,
    reason = "lifecycle scaffold exports define the local surface for later slices"
)]
pub(crate) use result::LifecycleResult;
#[allow(
    unused_imports,
    reason = "lifecycle state exports define the local surface for later slices"
)]
pub(crate) use state::{
    LifecycleAdmissionEffect, LifecycleCloseFact, LifecycleFailureFact,
    LifecycleOperationAdmission, LifecycleOperationKind, LifecycleStateMachine,
    LifecycleTransitionEffect, LifecycleTransitionOutcome, LifecycleTransitionTrigger,
};

#[cfg(test)]
mod tests;
