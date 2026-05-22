//! Storage lifecycle coordination.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "lifecycle scaffold is consumed by later lifecycle slices"
    )
)]

mod config;
mod error;
mod facts;
mod health;
mod outcome;
mod result;

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
    StorageOpenOutcome,
};
#[allow(
    unused_imports,
    reason = "lifecycle scaffold exports define the local surface for later slices"
)]
pub(crate) use result::LifecycleResult;

#[cfg(test)]
mod tests;
