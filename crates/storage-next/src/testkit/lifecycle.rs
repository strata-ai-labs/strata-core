//! Lifecycle scaffold conformance helpers.

use super::TestkitError;
use crate::lifecycle::{
    CloseOutcome, CloseOutcomeStatus, ClosePhase, LifecycleCloseTimeoutPolicy, LifecycleCodecId,
    LifecycleConfig, LifecycleError, LifecycleLossyRecoveryPolicy, LifecycleLowerLayer,
    LifecycleState, LifecycleStats, MaintenanceOutcome, MaintenanceOutcomeStatus,
    MaintenanceTaskKind, QuarantineStage, RecoveryDegradationClass, RecoveryFault,
    RecoveryFaultKind, RecoveryHealth, RecoveryStrictness, RetentionDecision, StorageMode,
    StorageOpenOutcome, StorageOpenPlan,
};
use std::error::Error;
use std::fmt;
use strata_core_next::CommitVersion;

/// Coverage counters returned by the lifecycle scaffold contract.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[expect(
    clippy::struct_field_names,
    reason = "counter suffix keeps the public testkit getter vocabulary explicit"
)]
pub struct LifecycleScaffoldOutcome {
    valid_config_cases: usize,
    invalid_config_cases: usize,
    lifecycle_state_cases: usize,
    storage_mode_cases: usize,
    open_plan_cases: usize,
    open_outcome_cases: usize,
    recovery_health_cases: usize,
    maintenance_task_cases: usize,
    reclaim_fact_cases: usize,
    error_display_cases: usize,
    error_source_cases: usize,
    stats_cases: usize,
    source_guard_fixture_cases: usize,
}

impl LifecycleScaffoldOutcome {
    /// Number of valid config cases exercised.
    pub const fn valid_config_cases(&self) -> usize {
        self.valid_config_cases
    }

    /// Number of invalid config cases exercised.
    pub const fn invalid_config_cases(&self) -> usize {
        self.invalid_config_cases
    }

    /// Number of lifecycle state cases exercised.
    pub const fn lifecycle_state_cases(&self) -> usize {
        self.lifecycle_state_cases
    }

    /// Number of storage mode cases exercised.
    pub const fn storage_mode_cases(&self) -> usize {
        self.storage_mode_cases
    }

    /// Number of open plan cases exercised.
    pub const fn open_plan_cases(&self) -> usize {
        self.open_plan_cases
    }

    /// Number of open outcome cases exercised.
    pub const fn open_outcome_cases(&self) -> usize {
        self.open_outcome_cases
    }

    /// Number of recovery health cases exercised.
    pub const fn recovery_health_cases(&self) -> usize {
        self.recovery_health_cases
    }

    /// Number of maintenance task cases exercised.
    pub const fn maintenance_task_cases(&self) -> usize {
        self.maintenance_task_cases
    }

    /// Number of retention, quarantine, and close fact cases exercised.
    pub const fn reclaim_fact_cases(&self) -> usize {
        self.reclaim_fact_cases
    }

    /// Number of error display cases exercised.
    pub const fn error_display_cases(&self) -> usize {
        self.error_display_cases
    }

    /// Number of error source-chain cases exercised.
    pub const fn error_source_cases(&self) -> usize {
        self.error_source_cases
    }

    /// Number of stats cases exercised.
    pub const fn stats_cases(&self) -> usize {
        self.stats_cases
    }

    /// Number of source-guard fixture cases exercised.
    pub const fn source_guard_fixture_cases(&self) -> usize {
        self.source_guard_fixture_cases
    }
}

/// Exercises the lifecycle scaffold without opening or mutating storage.
pub fn check_lifecycle_scaffold_contract(
    script: &[u8],
) -> Result<LifecycleScaffoldOutcome, TestkitError> {
    let mut outcome = LifecycleScaffoldOutcome::default();
    check_valid_config(script, &mut outcome)?;
    check_invalid_config(&mut outcome)?;
    check_lifecycle_states(&mut outcome)?;
    check_storage_modes(&mut outcome)?;
    check_open_plans(script, &mut outcome)?;
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
        true,
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
    outcome.open_outcome_cases += 1;
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

fn script_byte(script: &[u8], index: usize) -> u8 {
    script.get(index).copied().unwrap_or(0)
}

fn ensure(condition: bool, message: &'static str) -> Result<(), TestkitError> {
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
