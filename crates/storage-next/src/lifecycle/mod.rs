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
mod checkpoint;
mod compaction;
mod config;
mod durable;
mod error;
mod facts;
mod flush;
mod health;
mod maintenance;
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
    reason = "checkpoint maintenance exports define the local surface for later slices"
)]
pub(crate) use checkpoint::{
    checkpoint_durable_branch, checkpoint_request_from_maintenance_task, persist_flush_watermark,
    truncate_wal, wal_truncation_request_from_maintenance_task, LifecycleCheckpointOutcome,
    LifecycleCheckpointRequest, LifecycleCheckpointStatus, LifecycleFlushWatermarkOutcome,
    LifecycleFlushWatermarkProof, LifecycleFlushWatermarkRequest, LifecycleFlushWatermarkStatus,
    LifecycleWalTruncationOutcome, LifecycleWalTruncationRequest, LifecycleWalTruncationStatus,
};
#[allow(
    unused_imports,
    reason = "table rewrite maintenance exports define the local surface for later slices"
)]
pub(crate) use compaction::{
    collect_storage_pressure, compact_cache_branch, compact_durable_branch,
    compaction_request_from_maintenance_task, materialization_request_from_maintenance_task,
    materialize_cache_branch, materialize_durable_branch, LifecycleCompactionOutcome,
    LifecycleCompactionRequest, LifecycleCompactionStatus, LifecycleMaterializationOutcome,
    LifecycleMaterializationRequest, LifecycleMaterializationStatus, LifecycleStoragePressure,
    LifecycleStoragePressureReason, LifecycleStoragePressureSeverity,
    LifecycleTableRewriteDurability,
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
    reason = "flush maintenance exports define the local surface for later slices"
)]
pub(crate) use flush::{
    flush_cache_branch, flush_durable_branch, FlushFrozenOutcome, FlushFrozenRequest,
    FlushFrozenStatus, FlushTableIdentitySeed, FlushTableObjectId,
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
    reason = "maintenance executor exports define the local surface for later slices"
)]
pub(crate) use maintenance::{
    maintenance_ready_for_recovery_health, telemetry_health_debt, LifecycleMaintenanceExecutor,
    LifecycleMaintenanceStats, MaintenanceCancelOutcome, MaintenanceCheckpointOptions,
    MaintenanceClosePolicy, MaintenanceCoalesceKey, MaintenanceDrainOutcome,
    MaintenanceEnqueueOutcome, MaintenanceEnqueueStatus, MaintenanceExecutorStatus,
    MaintenanceFaultHook, MaintenanceFaultPoint, MaintenanceTask, MaintenanceTaskId,
    MaintenanceTaskPolicy, MaintenanceTaskPriority, MaintenanceTaskRequest, MaintenanceTaskRunner,
    MaintenanceTaskScope, NoopMaintenanceFaultHook,
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
    reason = "lifecycle recovery exports define the local surface for bootstrap"
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
