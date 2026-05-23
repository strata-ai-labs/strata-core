//! Storage lifecycle coordination.

#![cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "lifecycle scaffold is consumed by later lifecycle slices"
    )
)]

mod cache;
mod capability;
mod config;
mod durable;
mod error;
mod facts;
mod health;
mod outcome;
mod recovery;
mod result;
mod state;

#[allow(
    unused_imports,
    reason = "lifecycle cache runtime exports define the local surface for later slices"
)]
pub(crate) use cache::{LifecycleCacheOpenRequest, LifecycleCacheRuntime};
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
    reason = "durable lifecycle assembly exports define the local surface for recovery slices"
)]
pub(crate) use durable::{
    LifecycleDurableAssemblyFacts, LifecycleDurableLocalOpenRequest, LifecycleDurableLocalRuntime,
    LifecycleDurableLocalServices, LifecycleDurableLocalShell, LifecycleRecoveryBootstrapReport,
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
    CloseOutcome, CloseOutcomeEffects, CloseOutcomeStatus, MaintenanceOutcome,
    MaintenanceOutcomeStatus, StorageOpenDisposition, StorageOpenOutcome,
};
#[allow(
    unused_imports,
    reason = "lifecycle recovery exports define the local surface for L8G bootstrap"
)]
pub(crate) use recovery::{
    encode_checkpoint_row_section, LifecycleRecoveredCheckpoint, LifecycleRecoveredQuarantine,
    LifecycleRecoveredTable, LifecycleRecoveredTables, LifecycleRecoveredWal,
    LifecycleRecoveryOutcome, LifecycleRecoveryRequest, LifecycleRecoveryRuntime,
    LifecycleRecoveryTableObject, SNAPSHOT_ROW_SECTION_KIND,
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
