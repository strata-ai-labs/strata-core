use super::background::{BackgroundExecutorMode, BackgroundShutdownStats};
use super::error::{map_lifecycle_error, map_recovery_health};
use super::{
    perf_trace, CloseOutcome, CloseOutcomeStatus, LifecycleCodecId, LifecycleConfig,
    LifecycleError, LifecycleMaintenanceSchedulingPolicy, LifecycleMaintenanceStats,
    LifecycleStorageMode, LifecycleStorageOpenOutcome, LifecycleWalGrowthPolicy,
    MaintenanceExecutorStats, RecoveryStrictness, StorageApiError, StorageApiResult,
    StorageBackend, StorageBudgetPolicy, StorageCloseEffects, StorageCloseSummary,
    StorageDurabilityPolicy, StorageMaintenanceSchedulingPolicy, StorageMode,
    StorageOpenDisposition, StorageOpenOptions, StorageOpenPlan, StorageOpenSummary,
    StorageRuntimeBudget, StorageRuntimeState, StorageWalGrowthPolicy,
};
use crate::backend::BackendHandle;

/// The storage budget a fresh open uses when no test override is supplied.
///
/// An explicit `memory_budget`, when set, takes precedence (storage derives its per-pool split from
/// the total). Otherwise both cache and durable opens use the configured budget policy: cache obeys
/// the same budget as durable, failing with typed resource errors rather than growing until host
/// memory is exhausted.
pub(super) fn default_open_storage_budget(
    options: &StorageOpenOptions,
) -> StorageApiResult<StorageRuntimeBudget> {
    if let Some(memory_budget) = options.memory_budget() {
        // Cache mode holds the whole working set in the mutable pools (no
        // durable tables), so it derives a cache-shaped split — the durable
        // profile capped a cache database's effective capacity at total/8.
        let budget = if options.mode() == StorageMode::Cache {
            StorageRuntimeBudget::from_total_bytes_for_cache(memory_budget.bytes())
        } else {
            StorageRuntimeBudget::from_total_bytes(memory_budget.bytes())
        };
        return budget.map_err(map_lifecycle_error);
    }
    Ok(map_budget_policy(options.budget_policy()))
}

pub(super) fn lifecycle_plan(options: StorageOpenOptions) -> StorageApiResult<StorageOpenPlan> {
    let mode = match options.mode() {
        StorageMode::Cache => LifecycleStorageMode::Cache,
        StorageMode::DurableLocal {
            policy: StorageDurabilityPolicy::Standard,
        } => LifecycleStorageMode::DurableLocalStandard,
        StorageMode::DurableLocal {
            policy: StorageDurabilityPolicy::Always,
        } => LifecycleStorageMode::DurableLocalAlways,
        StorageMode::ObjectDurableCandidate | StorageMode::DistributedCandidate => {
            unreachable!("unsupported modes are rejected during validation")
        }
    };
    let recovery = if options.strict_recovery() {
        RecoveryStrictness::Strict
    } else {
        RecoveryStrictness::AllowExplicitLossyFallback
    };
    let mut config = LifecycleConfig::default();
    if !options.strict_recovery() {
        config = LifecycleConfig::new(
            config.max_maintenance_queue_depth(),
            config.max_recovery_faults(),
            config.close_timeout_policy(),
            crate::lifecycle::LifecycleLossyRecoveryPolicy::ExplicitlyAllowed,
        )
        .map_err(map_lifecycle_error)?;
    }
    #[cfg(any(test, feature = "testkit"))]
    let storage_budget = match options.storage_budget_for_test() {
        Some(budget) => budget,
        None => default_open_storage_budget(&options)?,
    };
    #[cfg(not(any(test, feature = "testkit")))]
    let storage_budget = default_open_storage_budget(&options)?;
    config = config
        .with_storage_budget(storage_budget)
        .map_err(map_lifecycle_error)?;
    config = config
        .with_wal_growth_policy(map_wal_growth_policy(options.wal_growth_policy()))
        .map_err(map_lifecycle_error)?;
    config = config
        .with_maintenance_scheduling_policy(map_maintenance_scheduling_policy(
            options.maintenance_scheduling_policy(),
        ))
        .map_err(map_lifecycle_error)?;
    StorageOpenPlan::new(mode, LifecycleCodecId::identity(), recovery, config)
        .map_err(map_lifecycle_error)
}

pub(super) fn durable_backend_handle_for_open(
    options: StorageOpenOptions,
    backend: &StorageBackend,
) -> StorageApiResult<BackendHandle<'static>> {
    // BS4.4i: durable runtimes are uniformly owned/`'static` — the borrowed `Durable` variant is gone.
    // Every real durable backend produces an owned handle (localfs directly; the fault/reordering test
    // backends via their `Arc`-shared state). A backend with no owned handle is in-memory, which cannot
    // back durable-local storage; surface the same policy-appropriate errors the borrowed path used to
    // (Background/DeterministicInline reject the policy — a background worker cannot hold a borrowed
    // backend; EvaluateAndEnqueue/Disabled reject the capability — matching the durable assembler's
    // in-memory rejection), without ever constructing a borrowed runtime.
    #[cfg(feature = "localfs")]
    if let Some(handle) = backend.to_owned_backend_handle() {
        return Ok(handle);
    }
    #[cfg(not(feature = "localfs"))]
    let _ = backend;

    match options.maintenance_scheduling_policy() {
        StorageMaintenanceSchedulingPolicy::Background
        | StorageMaintenanceSchedulingPolicy::DeterministicInline => {
            Err(StorageApiError::InvalidArgument {
                field: "maintenance_scheduling_policy",
                reason: "background and deterministic-inline durable opens require an owned backend handle",
            })
        }
        StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue
        | StorageMaintenanceSchedulingPolicy::Disabled => {
            Err(StorageApiError::UnsupportedCapability {
                capability: "durable_local",
                reason: "an in-memory backend cannot satisfy durable-local mode",
            })
        }
    }
}

pub(super) fn map_budget_policy(policy: StorageBudgetPolicy) -> StorageRuntimeBudget {
    match policy {
        StorageBudgetPolicy::Default => StorageRuntimeBudget::default(),
    }
}

pub(super) fn map_wal_growth_policy(policy: StorageWalGrowthPolicy) -> LifecycleWalGrowthPolicy {
    match policy {
        StorageWalGrowthPolicy::Default => LifecycleWalGrowthPolicy::default(),
        StorageWalGrowthPolicy::Disabled => LifecycleWalGrowthPolicy::disabled(),
        StorageWalGrowthPolicy::Thresholds {
            max_retained_wal_bytes,
            max_retained_wal_segments,
            max_commits_since_checkpoint,
        } => LifecycleWalGrowthPolicy::new(
            max_retained_wal_bytes,
            max_retained_wal_segments,
            Some(max_commits_since_checkpoint),
        ),
    }
}

pub(super) const fn map_maintenance_scheduling_policy(
    policy: StorageMaintenanceSchedulingPolicy,
) -> LifecycleMaintenanceSchedulingPolicy {
    match policy {
        StorageMaintenanceSchedulingPolicy::Background
        | StorageMaintenanceSchedulingPolicy::DeterministicInline => {
            LifecycleMaintenanceSchedulingPolicy::Background
        }
        StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue => {
            LifecycleMaintenanceSchedulingPolicy::EvaluateAndEnqueue
        }
        StorageMaintenanceSchedulingPolicy::Disabled => {
            LifecycleMaintenanceSchedulingPolicy::Disabled
        }
    }
}

pub(super) const fn background_executor_mode(
    policy: StorageMaintenanceSchedulingPolicy,
) -> BackgroundExecutorMode {
    match policy {
        StorageMaintenanceSchedulingPolicy::DeterministicInline => BackgroundExecutorMode::Inline,
        StorageMaintenanceSchedulingPolicy::Background
        | StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue
        | StorageMaintenanceSchedulingPolicy::Disabled => BackgroundExecutorMode::Threaded,
    }
}

pub(super) fn map_open_summary(
    outcome: &LifecycleStorageOpenOutcome,
    mode: StorageMode,
    options: StorageOpenOptions,
) -> StorageOpenSummary {
    StorageOpenSummary::with_open_facts(
        mode,
        match outcome.disposition() {
            crate::lifecycle::StorageOpenDisposition::Created => StorageOpenDisposition::Created,
            crate::lifecycle::StorageOpenDisposition::OpenedExisting => {
                StorageOpenDisposition::OpenedExisting
            }
        },
        map_recovery_health(outcome.recovery_health()),
        outcome.recovered_visible_version(),
        outcome.maintenance_ready(),
        options.maintenance_scheduling_policy(),
        outcome.bootstrap().is_some()
            || outcome.checkpoint().is_some()
            || outcome.wal().is_some()
            || outcome.tables().is_some()
            || outcome.quarantine().is_some(),
        outcome.backend_capabilities().is_some(),
    )
}

pub(super) fn map_close_summary(outcome: CloseOutcome, idempotent: bool) -> StorageCloseSummary {
    StorageCloseSummary::with_close_facts(
        match outcome.status() {
            CloseOutcomeStatus::Complete | CloseOutcomeStatus::Idempotent => {
                StorageRuntimeState::Closed
            }
            CloseOutcomeStatus::Timeout | CloseOutcomeStatus::Failed => StorageRuntimeState::Failed,
        },
        idempotent || matches!(outcome.status(), CloseOutcomeStatus::Idempotent),
        map_close_effects(outcome),
    )
}

pub(super) fn with_background_close_facts(
    summary: StorageCloseSummary,
    background_stats: Option<&MaintenanceExecutorStats>,
) -> StorageCloseSummary {
    background_stats.map_or(summary, |stats| {
        summary.with_background_facts(
            stats.worker_count,
            stats.queue_depth,
            stats.active_tasks,
            stats.tasks_completed,
        )
    })
}

pub(super) fn background_shutdown_panic_error(
    background_shutdown: Option<&BackgroundShutdownStats>,
) -> Option<StorageApiError> {
    let shutdown = background_shutdown?;
    if !shutdown.first_shutdown || shutdown.stats.worker_panics_after_shutdown == 0 {
        return None;
    }
    Some(map_lifecycle_error(LifecycleError::MaintenanceTaskFailed {
        reason: "background maintenance task panicked during close shutdown",
    }))
}

pub(super) fn record_background_close_maintenance_facts(
    before: LifecycleMaintenanceStats,
    after: LifecycleMaintenanceStats,
) {
    perf_trace::record_lifecycle_background_shutdown_canceled_tasks(
        after.canceled().saturating_sub(before.canceled()),
    );
    perf_trace::record_lifecycle_background_shutdown_drained_tasks(
        u64::try_from(after.drained().saturating_sub(before.drained())).unwrap_or(u64::MAX),
    );
}

pub(super) fn map_close_effects(outcome: CloseOutcome) -> StorageCloseEffects {
    let mut effects = StorageCloseEffects::empty();
    if outcome.commits_quiesced() {
        effects = effects.with_commits_quiesced();
    }
    if outcome.maintenance_drained() {
        effects = effects.with_maintenance_drained();
    }
    if outcome.durable_synced() {
        effects = effects.with_durable_synced();
    }
    if outcome.guards_released() {
        effects = effects.with_guards_released();
    }
    effects
}
