//! Lifecycle scaffold conformance helpers.

mod bootstrap;
mod cache;
mod capability;
mod checkpoint;
mod close;
mod crash;
mod durable;
mod fault;
mod flush;
mod maintenance;
mod outcome;
mod quarantine;
mod recovery;
mod retention;
mod rewrite;
mod script;

pub use bootstrap::{check_lifecycle_bootstrap_contract, LifecycleBootstrapContractOutcome};
pub use checkpoint::{check_lifecycle_checkpoint_contract, LifecycleCheckpointContractOutcome};
pub use close::{check_lifecycle_close_contract, LifecycleCloseContractOutcome};
pub use crash::{check_lifecycle_crash_contract, LifecycleCrashContractOutcome};
pub use fault::{check_lifecycle_fault_contract, LifecycleFaultContractOutcome};
pub use flush::{check_lifecycle_flush_contract, LifecycleFlushContractOutcome};
pub use maintenance::{check_lifecycle_maintenance_contract, LifecycleMaintenanceContractOutcome};
pub use outcome::LifecycleScaffoldOutcome;
pub use quarantine::{check_lifecycle_quarantine_contract, LifecycleQuarantineContractOutcome};
pub use recovery::{check_lifecycle_recovery_contract, LifecycleRecoveryContractOutcome};
pub use retention::{check_lifecycle_retention_contract, LifecycleRetentionContractOutcome};
pub use rewrite::{check_lifecycle_table_rewrite_contract, LifecycleTableRewriteContractOutcome};
pub use script::{
    check_lifecycle_generated_script_contract, check_lifecycle_maintenance_fuzz_contract,
    check_lifecycle_recovery_fuzz_contract, check_lifecycle_retention_fuzz_contract,
    LifecycleGeneratedScriptOutcome,
};

use super::TestkitError;
use crate::lifecycle::{
    CloseOutcome, CloseOutcomeStatus, ClosePhase, LifecycleCloseTimeoutPolicy, LifecycleCodecId,
    LifecycleConfig, LifecycleError, LifecycleLossyRecoveryPolicy, LifecycleLowerLayer,
    LifecycleOperationKind, LifecycleState, LifecycleStateMachine, LifecycleStats,
    LifecycleTransitionTrigger, MaintenanceOutcome, MaintenanceOutcomeStatus, MaintenanceTaskKind,
    QuarantineStage, RecoveryDegradationClass, RecoveryFault, RecoveryFaultKind, RecoveryHealth,
    RecoveryStrictness, RetentionDecision, StorageMode, StorageOpenDisposition, StorageOpenOutcome,
    StorageOpenPlan,
};
use std::error::Error;
use std::fmt;
use strata_core_next::CommitVersion;

/// Exercises the lifecycle scaffold without opening or mutating storage.
pub fn check_lifecycle_scaffold_contract(
    script: &[u8],
) -> Result<LifecycleScaffoldOutcome, TestkitError> {
    let mut outcome = LifecycleScaffoldOutcome::default();
    check_valid_config(script, &mut outcome)?;
    check_invalid_config(&mut outcome)?;
    check_lifecycle_states(&mut outcome)?;
    check_storage_modes(&mut outcome)?;
    check_lifecycle_state_transitions(script, &mut outcome)?;
    check_lifecycle_operation_admission(script, &mut outcome)?;
    check_open_plans(script, &mut outcome)?;
    capability::check_lifecycle_capability_contract(script, &mut outcome)?;
    cache::check_lifecycle_cache_contract(script, &mut outcome)?;
    durable::check_lifecycle_durable_contract(script, &mut outcome)?;
    let close = close::check_lifecycle_close_contract(script)?;
    ensure(
        close.close_requested_cases() > 0
            && close.cache_close_completed_cases() > 0
            && close.durable_close_completed_cases() > 0,
        "close contract did not exercise required close routes",
    )?;
    check_open_outcomes(script, &mut outcome)?;
    check_recovery_health(&mut outcome)?;
    check_maintenance_tasks(&mut outcome)?;
    check_reclaim_facts(&mut outcome)?;
    check_error_display(&mut outcome)?;
    check_error_source(&mut outcome)?;
    check_stats(script, &mut outcome)?;
    check_source_guard_fixtures(&mut outcome)?;
    Ok(outcome)
}

fn check_valid_config(
    script: &[u8],
    outcome: &mut LifecycleScaffoldOutcome,
) -> Result<(), TestkitError> {
    let queue_depth = usize::from(script_byte(script, 0) % 16) + 1;
    let recovery_faults = usize::from(script_byte(script, 1) % 16) + 1;
    let config = LifecycleConfig::new(
        queue_depth,
        recovery_faults,
        LifecycleCloseTimeoutPolicy::ReturnTypedTimeout,
        LifecycleLossyRecoveryPolicy::Disabled,
    )
    .map_err(|error| testkit_error(&error))?;
    ensure(
        config.max_maintenance_queue_depth() == queue_depth,
        "valid lifecycle config did not preserve queue depth",
    )?;
    ensure(
        config.max_recovery_faults() == recovery_faults,
        "valid lifecycle config did not preserve recovery fault limit",
    )?;
    outcome.valid_config_cases += 1;
    Ok(())
}

fn check_invalid_config(outcome: &mut LifecycleScaffoldOutcome) -> Result<(), TestkitError> {
    let error = LifecycleConfig::new(
        0,
        1,
        LifecycleCloseTimeoutPolicy::ReturnTypedTimeout,
        LifecycleLossyRecoveryPolicy::Disabled,
    )
    .expect_err("zero queue depth rejects");
    ensure(
        matches!(
            error,
            LifecycleError::InvalidConfig {
                field: "max_maintenance_queue_depth",
                ..
            }
        ),
        "zero queue depth did not return invalid-config",
    )?;
    outcome.invalid_config_cases += 1;
    Ok(())
}

fn check_lifecycle_states(outcome: &mut LifecycleScaffoldOutcome) -> Result<(), TestkitError> {
    let states = [
        LifecycleState::New,
        LifecycleState::Opening,
        LifecycleState::Recovering,
        LifecycleState::Open,
        LifecycleState::Closing,
        LifecycleState::Closed,
        LifecycleState::Failed,
    ];
    ensure(states.len() == 7, "lifecycle state fixture incomplete")?;
    outcome.lifecycle_state_cases += states.len();
    Ok(())
}

fn check_storage_modes(outcome: &mut LifecycleScaffoldOutcome) -> Result<(), TestkitError> {
    let modes = [
        StorageMode::Cache,
        StorageMode::DurableLocalStandard,
        StorageMode::DurableLocalAlways,
        StorageMode::ObjectDurableCandidate,
    ];
    ensure(modes.len() == 4, "storage mode fixture incomplete")?;
    outcome.storage_mode_cases += modes.len();
    Ok(())
}

fn check_lifecycle_state_transitions(
    script: &[u8],
    outcome: &mut LifecycleScaffoldOutcome,
) -> Result<(), TestkitError> {
    check_valid_lifecycle_transitions(outcome)?;
    check_failed_lifecycle_transitions(outcome)?;
    check_invalid_lifecycle_transitions(outcome)?;
    check_input_derived_lifecycle_transition(script, outcome)?;
    Ok(())
}

fn check_valid_lifecycle_transitions(
    outcome: &mut LifecycleScaffoldOutcome,
) -> Result<(), TestkitError> {
    for (start, trigger, expected) in [
        (
            LifecycleState::New,
            LifecycleTransitionTrigger::OpenRequested,
            LifecycleState::Opening,
        ),
        (
            LifecycleState::Opening,
            LifecycleTransitionTrigger::CacheOpenReady,
            LifecycleState::Open,
        ),
        (
            LifecycleState::Opening,
            LifecycleTransitionTrigger::DurableRecoveryRequired,
            LifecycleState::Recovering,
        ),
        (
            LifecycleState::Recovering,
            LifecycleTransitionTrigger::RecoveryAccepted,
            LifecycleState::Open,
        ),
        (
            LifecycleState::Open,
            LifecycleTransitionTrigger::CloseRequested,
            LifecycleState::Closing,
        ),
        (
            LifecycleState::Closing,
            LifecycleTransitionTrigger::CloseCompleted,
            LifecycleState::Closed,
        ),
        (
            LifecycleState::Closing,
            LifecycleTransitionTrigger::CloseRetried,
            LifecycleState::Closing,
        ),
        (
            LifecycleState::Closed,
            LifecycleTransitionTrigger::CloseRetried,
            LifecycleState::Closed,
        ),
    ] {
        let mut machine = lifecycle_machine_in_state(start)?;
        let transition = machine
            .transition(trigger)
            .map_err(|error| testkit_error(&error))?;
        ensure(transition.from() == start, "transition lost source state")?;
        ensure(
            transition.to() == expected,
            "transition reached wrong state",
        )?;
        ensure(machine.state() == expected, "machine state was not updated")?;
        if matches!(trigger, LifecycleTransitionTrigger::CloseRetried) {
            outcome.close_retry_cases += 1;
        }
        if start == LifecycleState::Closed {
            ensure(
                transition.is_idempotent(),
                "closed close retry was not idempotent",
            )?;
            outcome.closed_idempotence_cases += 1;
        }
        outcome.valid_transition_cases += 1;
    }
    Ok(())
}

fn check_failed_lifecycle_transitions(
    outcome: &mut LifecycleScaffoldOutcome,
) -> Result<(), TestkitError> {
    for start in [
        LifecycleState::Opening,
        LifecycleState::Recovering,
        LifecycleState::Closing,
    ] {
        let mut machine = lifecycle_machine_in_state(start)?;
        let transition = machine
            .transition(LifecycleTransitionTrigger::PhaseFailed {
                reason: "phase failed",
            })
            .map_err(|error| testkit_error(&error))?;
        let failure = transition
            .failure()
            .ok_or_else(|| TestkitError::new("failure transition did not record failure fact"))?;
        ensure(
            failure.failed_state() == start,
            "failure transition lost failed state",
        )?;
        ensure(
            machine.state() == LifecycleState::Failed,
            "failure transition did not enter failed state",
        )?;
        ensure(
            LifecycleStateMachine::admit_state(
                LifecycleState::Failed,
                LifecycleOperationKind::HealthQuery,
            )
            .is_allowed(),
            "failed state rejected health query",
        )?;
        ensure(
            !LifecycleStateMachine::admit_state(
                LifecycleState::Failed,
                LifecycleOperationKind::Commit,
            )
            .is_allowed(),
            "failed state admitted commit",
        )?;
        outcome.valid_transition_cases += 1;
        outcome.failed_state_sticky_cases += 1;
    }
    Ok(())
}

fn check_invalid_lifecycle_transitions(
    outcome: &mut LifecycleScaffoldOutcome,
) -> Result<(), TestkitError> {
    for (start, trigger) in [
        (
            LifecycleState::New,
            LifecycleTransitionTrigger::CacheOpenReady,
        ),
        (
            LifecycleState::New,
            LifecycleTransitionTrigger::RecoveryAccepted,
        ),
        (
            LifecycleState::Opening,
            LifecycleTransitionTrigger::CloseCompleted,
        ),
        (
            LifecycleState::Recovering,
            LifecycleTransitionTrigger::CloseRequested,
        ),
        (
            LifecycleState::Open,
            LifecycleTransitionTrigger::DurableRecoveryRequired,
        ),
        (
            LifecycleState::Closed,
            LifecycleTransitionTrigger::OpenRequested,
        ),
        (
            LifecycleState::Closed,
            LifecycleTransitionTrigger::RecoveryAccepted,
        ),
        (
            LifecycleState::Failed,
            LifecycleTransitionTrigger::OpenRequested,
        ),
        (
            LifecycleState::Failed,
            LifecycleTransitionTrigger::CloseCompleted,
        ),
        (
            LifecycleState::Failed,
            LifecycleTransitionTrigger::CloseRetried,
        ),
        (
            LifecycleState::New,
            LifecycleTransitionTrigger::PhaseFailed { reason: "no phase" },
        ),
        (
            LifecycleState::Open,
            LifecycleTransitionTrigger::PhaseFailed {
                reason: "open ordinary phase",
            },
        ),
        (
            LifecycleState::Closed,
            LifecycleTransitionTrigger::PhaseFailed {
                reason: "already closed",
            },
        ),
        (
            LifecycleState::Failed,
            LifecycleTransitionTrigger::PhaseFailed {
                reason: "already failed",
            },
        ),
    ] {
        let mut machine = lifecycle_machine_in_state(start)?;
        let before = machine.state();
        let error = machine
            .transition(trigger)
            .expect_err("invalid transition rejects");
        ensure(
            matches!(error, LifecycleError::InvalidLifecycleState { .. }),
            "invalid transition did not return lifecycle-state error",
        )?;
        ensure(
            machine.state() == before,
            "invalid transition mutated state",
        )?;
        outcome.invalid_transition_cases += 1;
    }
    Ok(())
}

fn check_input_derived_lifecycle_transition(
    script: &[u8],
    outcome: &mut LifecycleScaffoldOutcome,
) -> Result<(), TestkitError> {
    let mut dynamic = lifecycle_machine_in_state(state_from_script(script_byte(script, 9)))?;
    let trigger = trigger_from_script(script_byte(script, 10));
    match dynamic.transition(trigger) {
        Ok(_) => outcome.valid_transition_cases += 1,
        Err(LifecycleError::InvalidLifecycleState { .. }) => {
            outcome.invalid_transition_cases += 1;
        }
        Err(error) => return Err(testkit_error(&error)),
    }
    outcome.input_derived_state_cases += 1;
    Ok(())
}

fn check_lifecycle_operation_admission(
    script: &[u8],
    outcome: &mut LifecycleScaffoldOutcome,
) -> Result<(), TestkitError> {
    let states = [
        LifecycleState::New,
        LifecycleState::Opening,
        LifecycleState::Recovering,
        LifecycleState::Open,
        LifecycleState::Closing,
        LifecycleState::Closed,
        LifecycleState::Failed,
    ];
    let operations = [
        LifecycleOperationKind::Open,
        LifecycleOperationKind::OrdinaryRead,
        LifecycleOperationKind::Commit,
        LifecycleOperationKind::RecoveryStep,
        LifecycleOperationKind::OrdinaryMaintenance,
        LifecycleOperationKind::CloseRequiredDrain,
        LifecycleOperationKind::HealthQuery,
        LifecycleOperationKind::Close,
        LifecycleOperationKind::CloseRetry,
    ];
    for state in states {
        for operation in operations {
            let admission = LifecycleStateMachine::admit_state(state, operation);
            if admission.is_allowed() {
                outcome.operation_admission_accept_cases += 1;
            } else {
                ensure(
                    admission.rejection_reason().is_some(),
                    "rejected admission did not preserve reason",
                )?;
                outcome.operation_admission_reject_cases += 1;
            }
        }
    }

    let state = state_from_script(script_byte(script, 11));
    let operation = operation_from_script(script_byte(script, 12));
    let admission = LifecycleStateMachine::admit_state(state, operation);
    if admission.is_allowed() {
        outcome.operation_admission_accept_cases += 1;
    } else {
        outcome.operation_admission_reject_cases += 1;
    }
    outcome.input_derived_state_cases += 1;
    Ok(())
}

fn check_open_plans(
    script: &[u8],
    outcome: &mut LifecycleScaffoldOutcome,
) -> Result<(), TestkitError> {
    let codec_id = LifecycleCodecId::new(format!("identity-{}", script_byte(script, 2)))
        .map_err(|error| testkit_error(&error))?;
    let plan = StorageOpenPlan::new(
        StorageMode::Cache,
        codec_id,
        RecoveryStrictness::Strict,
        LifecycleConfig::default(),
    )
    .map_err(|error| testkit_error(&error))?;
    ensure(
        plan.storage_mode() == StorageMode::Cache,
        "cache open plan did not preserve mode",
    )?;
    ensure(
        plan.codec_id().as_str().starts_with("identity-"),
        "open plan did not preserve codec id",
    )?;

    let lossy_config = LifecycleConfig::new(
        2,
        2,
        LifecycleCloseTimeoutPolicy::WaitForStorageDrain,
        LifecycleLossyRecoveryPolicy::ExplicitlyAllowed,
    )
    .map_err(|error| testkit_error(&error))?;
    StorageOpenPlan::new(
        StorageMode::DurableLocalAlways,
        LifecycleCodecId::identity(),
        RecoveryStrictness::AllowExplicitLossyFallback,
        lossy_config,
    )
    .map_err(|error| testkit_error(&error))?;
    outcome.open_plan_cases += 2;
    Ok(())
}

fn check_open_outcomes(
    script: &[u8],
    outcome: &mut LifecycleScaffoldOutcome,
) -> Result<(), TestkitError> {
    let version = CommitVersion::new(u64::from(script_byte(script, 3)) + 1);
    let open = StorageOpenOutcome::new(
        StorageMode::DurableLocalStandard,
        StorageOpenDisposition::OpenedExisting,
        Some(version),
        RecoveryHealth::Healthy,
        true,
    )
    .map_err(|error| testkit_error(&error))?;
    ensure(
        open.recovered_visible_version() == Some(version),
        "open outcome did not preserve visible version",
    )?;
    ensure(
        open.maintenance_ready(),
        "open outcome did not preserve readiness",
    )?;
    ensure(
        open.opened_existing(),
        "open outcome did not preserve open disposition",
    )?;
    let degraded = RecoveryHealth::degraded(
        RecoveryDegradationClass::Telemetry,
        vec![
            RecoveryFault::new(RecoveryFaultKind::IoFailure, "cache recovery")
                .map_err(|error| testkit_error(&error))?,
        ],
    )
    .map_err(|error| testkit_error(&error))?;
    let error = StorageOpenOutcome::new(
        StorageMode::Cache,
        StorageOpenDisposition::Created,
        None,
        degraded,
        false,
    )
    .expect_err("cache degraded recovery rejects");
    ensure(
        matches!(error, LifecycleError::InvalidOpenPlan { .. }),
        "cache degraded recovery did not return invalid open plan",
    )?;
    outcome.open_outcome_cases += 2;
    Ok(())
}

fn check_recovery_health(outcome: &mut LifecycleScaffoldOutcome) -> Result<(), TestkitError> {
    let fault = RecoveryFault::new(RecoveryFaultKind::TimelineMismatch, "timeline mismatch")
        .map_err(|error| testkit_error(&error))?;
    let degraded = RecoveryHealth::degraded(
        RecoveryDegradationClass::PolicyDowngrade,
        vec![fault.clone()],
    )
    .map_err(|error| testkit_error(&error))?;
    ensure(
        matches!(
            degraded,
            RecoveryHealth::Degraded {
                class: RecoveryDegradationClass::PolicyDowngrade,
                ..
            }
        ),
        "degraded recovery health lost class",
    )?;
    ensure(
        matches!(RecoveryHealth::failed(fault), RecoveryHealth::Failed { .. }),
        "failed recovery health not constructible",
    )?;
    outcome.recovery_health_cases += 3;
    Ok(())
}

fn check_maintenance_tasks(outcome: &mut LifecycleScaffoldOutcome) -> Result<(), TestkitError> {
    let tasks = [
        MaintenanceTaskKind::Flush,
        MaintenanceTaskKind::Checkpoint,
        MaintenanceTaskKind::WalTruncation,
        MaintenanceTaskKind::Compaction,
        MaintenanceTaskKind::Materialization,
        MaintenanceTaskKind::SnapshotPruning,
        MaintenanceTaskKind::Retention,
        MaintenanceTaskKind::Quarantine,
        MaintenanceTaskKind::Purge,
        MaintenanceTaskKind::Repair,
        MaintenanceTaskKind::HealthCollection,
    ];
    let maintenance = MaintenanceOutcome::new(tasks[0], MaintenanceOutcomeStatus::Completed);
    ensure(
        maintenance.status() == MaintenanceOutcomeStatus::Completed,
        "maintenance outcome did not preserve status",
    )?;
    outcome.maintenance_task_cases += tasks.len();
    Ok(())
}

fn check_reclaim_facts(outcome: &mut LifecycleScaffoldOutcome) -> Result<(), TestkitError> {
    let decisions = [
        RetentionDecision::Retain,
        RetentionDecision::PruneCandidate,
        RetentionDecision::QuarantineCandidate,
        RetentionDecision::PurgeCandidate,
        RetentionDecision::SkipUntilProof,
    ];
    let stages = [
        QuarantineStage::Candidate,
        QuarantineStage::InventoryPublished,
        QuarantineStage::Quarantined,
        QuarantineStage::PurgeEligible,
    ];
    let close = CloseOutcome::new(ClosePhase::ReleaseGuards, CloseOutcomeStatus::Complete);
    ensure(
        close.status() == CloseOutcomeStatus::Complete,
        "close outcome did not preserve status",
    )?;
    outcome.reclaim_fact_cases += decisions.len() + stages.len() + 1;
    Ok(())
}

fn check_error_display(outcome: &mut LifecycleScaffoldOutcome) -> Result<(), TestkitError> {
    let error = LifecycleError::InvalidOpenPlan {
        reason: "mode not supported",
    };
    ensure(
        error.to_string() == "invalid storage open plan: mode not supported",
        "lifecycle error display changed",
    )?;
    outcome.error_display_cases += 1;
    Ok(())
}

fn check_error_source(outcome: &mut LifecycleScaffoldOutcome) -> Result<(), TestkitError> {
    let error = LifecycleError::lower_layer_with(
        LifecycleLowerLayer::CommitRuntime,
        "replay rejected",
        TestSourceError,
    );
    ensure(
        error.source().is_some(),
        "lower-layer source was not preserved",
    )?;
    outcome.error_source_cases += 1;
    Ok(())
}

fn check_stats(script: &[u8], outcome: &mut LifecycleScaffoldOutcome) -> Result<(), TestkitError> {
    let stats = LifecycleStats::new(
        usize::from(script_byte(script, 4)),
        usize::from(script_byte(script, 5)),
        usize::from(script_byte(script, 6)),
        usize::from(script_byte(script, 7)),
        usize::from(script_byte(script, 8)),
    );
    ensure(
        stats.open_attempts() == usize::from(script_byte(script, 4)),
        "stats did not preserve open attempts",
    )?;
    ensure(
        LifecycleStats::default().close_attempts() == 0,
        "default stats are nonzero",
    )?;
    outcome.stats_cases += 2;
    Ok(())
}

fn check_source_guard_fixtures(outcome: &mut LifecycleScaffoldOutcome) -> Result<(), TestkitError> {
    for allowed in [
        "RecoveryHealth MaintenanceTask CommitVersion",
        "BranchId StorageRow WalService",
    ] {
        ensure(
            !contains_forbidden_lifecycle_vocabulary(allowed),
            "storage-owned lifecycle vocabulary was rejected",
        )?;
        outcome.source_guard_fixture_cases += 1;
    }
    for forbidden in [
        "Database::open",
        "database::open",
        "VersionedValue",
        "versionedvalue",
        "StrataHub",
        "stratahub",
        "Follower",
        "follower mode",
        "manual maintenance command",
        "refresh follower",
        "EntityRef",
        "JsonValue",
        "Graph",
        "Vector",
        "Search",
        "event module",
        "Embedding",
        "Inference",
        "TransactionContext",
        "begin_transaction",
    ] {
        ensure(
            contains_forbidden_lifecycle_vocabulary(forbidden),
            "product lifecycle vocabulary was not rejected",
        )?;
        outcome.source_guard_fixture_cases += 1;
    }
    Ok(())
}

fn lifecycle_machine_in_state(
    state: LifecycleState,
) -> Result<LifecycleStateMachine, TestkitError> {
    let mut machine = LifecycleStateMachine::new();
    match state {
        LifecycleState::New => {}
        LifecycleState::Opening => {
            transition_machine(&mut machine, LifecycleTransitionTrigger::OpenRequested)?;
        }
        LifecycleState::Recovering => {
            transition_machine(&mut machine, LifecycleTransitionTrigger::OpenRequested)?;
            transition_machine(
                &mut machine,
                LifecycleTransitionTrigger::DurableRecoveryRequired,
            )?;
        }
        LifecycleState::Open => {
            transition_machine(&mut machine, LifecycleTransitionTrigger::OpenRequested)?;
            transition_machine(&mut machine, LifecycleTransitionTrigger::CacheOpenReady)?;
        }
        LifecycleState::Closing => {
            machine = lifecycle_machine_in_state(LifecycleState::Open)?;
            transition_machine(&mut machine, LifecycleTransitionTrigger::CloseRequested)?;
        }
        LifecycleState::Closed => {
            machine = lifecycle_machine_in_state(LifecycleState::Closing)?;
            transition_machine(&mut machine, LifecycleTransitionTrigger::CloseCompleted)?;
        }
        LifecycleState::Failed => {
            machine = lifecycle_machine_in_state(LifecycleState::Opening)?;
            transition_machine(
                &mut machine,
                LifecycleTransitionTrigger::PhaseFailed {
                    reason: "phase failed",
                },
            )?;
        }
    }
    Ok(machine)
}

fn transition_machine(
    machine: &mut LifecycleStateMachine,
    trigger: LifecycleTransitionTrigger,
) -> Result<(), TestkitError> {
    machine
        .transition(trigger)
        .map(|_| ())
        .map_err(|error| testkit_error(&error))
}

fn state_from_script(byte: u8) -> LifecycleState {
    match byte % 7 {
        0 => LifecycleState::New,
        1 => LifecycleState::Opening,
        2 => LifecycleState::Recovering,
        3 => LifecycleState::Open,
        4 => LifecycleState::Closing,
        5 => LifecycleState::Closed,
        _ => LifecycleState::Failed,
    }
}

fn trigger_from_script(byte: u8) -> LifecycleTransitionTrigger {
    match byte % 8 {
        0 => LifecycleTransitionTrigger::OpenRequested,
        1 => LifecycleTransitionTrigger::CacheOpenReady,
        2 => LifecycleTransitionTrigger::DurableRecoveryRequired,
        3 => LifecycleTransitionTrigger::RecoveryAccepted,
        4 => LifecycleTransitionTrigger::CloseRequested,
        5 => LifecycleTransitionTrigger::CloseCompleted,
        6 => LifecycleTransitionTrigger::CloseRetried,
        _ => LifecycleTransitionTrigger::PhaseFailed {
            reason: "script failure",
        },
    }
}

fn operation_from_script(byte: u8) -> LifecycleOperationKind {
    match byte % 9 {
        0 => LifecycleOperationKind::Open,
        1 => LifecycleOperationKind::OrdinaryRead,
        2 => LifecycleOperationKind::Commit,
        3 => LifecycleOperationKind::RecoveryStep,
        4 => LifecycleOperationKind::OrdinaryMaintenance,
        5 => LifecycleOperationKind::CloseRequiredDrain,
        6 => LifecycleOperationKind::HealthQuery,
        7 => LifecycleOperationKind::Close,
        _ => LifecycleOperationKind::CloseRetry,
    }
}

pub(super) fn script_byte(script: &[u8], index: usize) -> u8 {
    script.get(index).copied().unwrap_or(0)
}

pub(super) fn ensure(condition: bool, message: &'static str) -> Result<(), TestkitError> {
    if condition {
        Ok(())
    } else {
        Err(TestkitError::new(message))
    }
}

fn testkit_error(error: &LifecycleError) -> TestkitError {
    TestkitError::new(error.to_string())
}

fn contains_forbidden_lifecycle_vocabulary(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        "database::open",
        "versionedvalue",
        "stratahub",
        "follower",
        "manual maintenance command",
        "refresh follower",
        "entityref",
        "jsonvalue",
        "graph",
        "vector",
        "search",
        "event module",
        "embedding",
        "inference",
        "transactioncontext",
        "begin_transaction",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

#[derive(Debug)]
struct TestSourceError;

impl fmt::Display for TestSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("test source")
    }
}

impl Error for TestSourceError {}
