use super::background::{BackgroundDrainLimits, BackgroundDrainRound};
use super::diagnostics::require_valid_branch_identifier;
use super::error::{map_lifecycle_error, map_recovery_health};
use super::{
    perf_trace, ApiTimestampSource, Arc, BackgroundTaskPriority, CacheBackgroundMaintenanceStep,
    DurableBackgroundMaintenanceStep, LifecycleCacheRuntime, LifecycleCheckpointOutcome,
    LifecycleDurableLocalRuntime, LifecycleError, LifecycleMaintenanceOutcome,
    LifecycleMaintenanceOutcomeReasonClass, LifecycleMaintenanceOutcomeStatus,
    LifecycleMaintenanceTaskKind, LifecycleMaintenanceTaskPolicy, LifecycleMaintenanceTaskPriority,
    LifecycleMaintenanceTaskRequest, LifecycleMaintenanceTaskScope, LifecycleWalGrowthOutcome,
    LifecycleWalGrowthStatus, LifecycleWalGrowthTrigger, MaintenanceCheckpointOptions,
    MaintenanceClock, MaintenanceExecutorStats, MaintenanceExecutorStatus, MaintenanceQueueSummary,
    MaintenanceReasonClass, MaintenanceRequest, MaintenanceScope, MaintenanceSummary,
    MaintenanceSummaryStatus, MaintenanceTask, MaintenanceWalGrowthStatus,
    MaintenanceWalGrowthSummary, MaintenanceWalGrowthTrigger, ParkingMutex, PreparedPublishStep,
    StorageApiError, StorageApiErrorClass, StorageApiResult, StorageRuntime,
};

pub(super) fn validate_maintenance_request(request: &MaintenanceRequest) -> StorageApiResult<()> {
    let valid = match request.task() {
        MaintenanceTask::Flush
        | MaintenanceTask::Compact
        | MaintenanceTask::Materialize
        | MaintenanceTask::Reclaim
        | MaintenanceTask::Purge => matches!(request.scope(), MaintenanceScope::Branch(_)),
        MaintenanceTask::Retain
        | MaintenanceTask::SnapshotPruning
        | MaintenanceTask::Quarantine
        | MaintenanceTask::WalGrowth => matches!(request.scope(), MaintenanceScope::Global),
        MaintenanceTask::Checkpoint | MaintenanceTask::Repair => true,
    };
    if valid {
        Ok(())
    } else {
        Err(StorageApiError::InvalidArgument {
            field: "scope",
            reason: "maintenance task does not support the requested scope",
        })
    }
}

pub(super) fn map_maintenance_summary(
    request: MaintenanceRequest,
    outcome: &LifecycleMaintenanceOutcome,
) -> MaintenanceSummary {
    let (source_error_code, source_error_class) = outcome
        .source_error()
        .map_or((None, None), lifecycle_error_facts);
    MaintenanceSummary::new(
        request.task(),
        request.scope(),
        map_maintenance_status(outcome.status()),
    )
    .with_reason(
        outcome.reason_class().map(map_maintenance_reason_class),
        outcome.reason(),
    )
    .with_affected_objects(
        outcome.affected_object_names().to_vec(),
        outcome.affected_objects(),
    )
    .with_effects(
        outcome.bytes_reclaimed(),
        outcome.retryable(),
        outcome.checkpoint_required(),
        outcome.state_changes(),
    )
    .with_health(outcome.recovery_health().map(map_recovery_health))
    .with_source_error(source_error_code, source_error_class)
}

pub(super) fn map_checkpoint_summary(
    request: &MaintenanceRequest,
    outcome: &LifecycleCheckpointOutcome,
) -> MaintenanceSummary {
    let wal_truncated = outcome
        .wal_truncation()
        .is_some_and(|truncation| truncation.deleted_segments() > 0);
    map_maintenance_summary(*request, &outcome.maintenance_outcome()).with_checkpoint_facts(
        outcome.checkpoint_watermark(),
        outcome.snapshot_id(),
        outcome.row_count(),
        wal_truncated,
    )
}

pub(super) fn map_wal_growth_maintenance_summary(
    request: MaintenanceRequest,
    outcome: &LifecycleWalGrowthOutcome,
) -> MaintenanceSummary {
    let growth = map_wal_growth_summary(outcome);
    let status = match outcome.status() {
        LifecycleWalGrowthStatus::Deferred => MaintenanceSummaryStatus::Deferred,
        _ => MaintenanceSummaryStatus::Completed,
    };
    let (source_error_code, source_error_class) = outcome
        .source_error()
        .map_or((None, None), lifecycle_error_facts);
    MaintenanceSummary::new(request.task(), request.scope(), status)
        .with_health(outcome.recovery_health().map(map_recovery_health))
        .with_source_error(source_error_code, source_error_class)
        .with_wal_growth(growth)
}

pub(super) fn map_wal_growth_summary(
    outcome: &LifecycleWalGrowthOutcome,
) -> MaintenanceWalGrowthSummary {
    let facts = outcome.facts();
    let (source_error_code, source_error_class) = outcome
        .source_error()
        .map_or((None, None), lifecycle_error_facts);
    MaintenanceWalGrowthSummary::new(
        map_wal_growth_status(outcome.status()),
        facts.retained_bytes(),
        u64::try_from(facts.retained_segments()).unwrap_or(u64::MAX),
        outcome.commits_since_checkpoint(),
        outcome.trigger().map(map_wal_growth_trigger),
        outcome.enqueue().is_some(),
        outcome.recovery_health().map(map_recovery_health),
        source_error_code,
        source_error_class,
    )
}

pub(super) const fn map_maintenance_status(
    status: LifecycleMaintenanceOutcomeStatus,
) -> MaintenanceSummaryStatus {
    match status {
        LifecycleMaintenanceOutcomeStatus::Completed => MaintenanceSummaryStatus::Completed,
        LifecycleMaintenanceOutcomeStatus::Deferred => MaintenanceSummaryStatus::Deferred,
        LifecycleMaintenanceOutcomeStatus::Failed => MaintenanceSummaryStatus::Failed,
        LifecycleMaintenanceOutcomeStatus::Canceled => MaintenanceSummaryStatus::Canceled,
    }
}

pub(super) const fn map_maintenance_reason_class(
    reason_class: LifecycleMaintenanceOutcomeReasonClass,
) -> MaintenanceReasonClass {
    match reason_class {
        LifecycleMaintenanceOutcomeReasonClass::Deferred => MaintenanceReasonClass::Deferred,
        LifecycleMaintenanceOutcomeReasonClass::Failed => MaintenanceReasonClass::Failed,
        LifecycleMaintenanceOutcomeReasonClass::Canceled => MaintenanceReasonClass::Canceled,
    }
}

pub(super) const fn map_wal_growth_status(
    status: LifecycleWalGrowthStatus,
) -> MaintenanceWalGrowthStatus {
    match status {
        LifecycleWalGrowthStatus::Disabled => MaintenanceWalGrowthStatus::Disabled,
        LifecycleWalGrowthStatus::BelowThreshold => MaintenanceWalGrowthStatus::BelowThreshold,
        LifecycleWalGrowthStatus::MaintenanceEnqueued => {
            MaintenanceWalGrowthStatus::MaintenanceEnqueued
        }
        LifecycleWalGrowthStatus::MaintenanceCoalesced => {
            MaintenanceWalGrowthStatus::MaintenanceCoalesced
        }
        LifecycleWalGrowthStatus::Deferred => MaintenanceWalGrowthStatus::Deferred,
        LifecycleWalGrowthStatus::NoDurableAction => MaintenanceWalGrowthStatus::NoDurableAction,
    }
}

pub(super) const fn map_wal_growth_trigger(
    trigger: LifecycleWalGrowthTrigger,
) -> MaintenanceWalGrowthTrigger {
    match trigger {
        LifecycleWalGrowthTrigger::RetainedBytes => MaintenanceWalGrowthTrigger::RetainedBytes,
        LifecycleWalGrowthTrigger::RetainedSegments => {
            MaintenanceWalGrowthTrigger::RetainedSegments
        }
        LifecycleWalGrowthTrigger::CommitsSinceCheckpoint => {
            MaintenanceWalGrowthTrigger::CommitsSinceCheckpoint
        }
    }
}

pub(super) fn map_maintenance_queue_summary(
    executor_status: MaintenanceExecutorStatus,
    background_stats: Option<MaintenanceExecutorStats>,
) -> MaintenanceQueueSummary {
    let stats = executor_status.stats();
    let (
        background_worker_count,
        background_queue_depth,
        background_active_tasks,
        background_tasks_completed,
    ) = background_stats.map_or((0, 0, 0, 0), |stats| {
        (
            stats.worker_count,
            stats.queue_depth,
            stats.active_tasks,
            stats.tasks_completed,
        )
    });
    MaintenanceQueueSummary::new(
        executor_status.pending_tasks(),
        executor_status
            .active_task()
            .map(crate::lifecycle::MaintenanceTaskId::get),
        stats.enqueued(),
        stats.coalesced(),
        stats.max_pending_tasks(),
        stats.started(),
        stats.completed(),
        stats.deferred(),
        stats.failed(),
        stats.canceled(),
        stats.drained(),
        stats.queue_full(),
        background_worker_count,
        background_queue_depth,
        background_active_tasks,
        background_tasks_completed,
    )
}

pub(super) fn unsupported_maintenance_summary(
    request: &MaintenanceRequest,
    reason: &'static str,
) -> MaintenanceSummary {
    MaintenanceSummary::new(
        request.task(),
        request.scope(),
        MaintenanceSummaryStatus::Deferred,
    )
    .with_reason(Some(MaintenanceReasonClass::Deferred), Some(reason))
    .with_effects(0, false, false, 0)
}

pub(super) fn lifecycle_error_facts(
    error: &LifecycleError,
) -> (Option<&'static str>, Option<StorageApiErrorClass>) {
    let error = map_lifecycle_error(error.clone());
    (Some(error.code()), Some(error.class()))
}

pub(super) fn request_for_outcome(outcome: &LifecycleMaintenanceOutcome) -> MaintenanceRequest {
    let task_scope = outcome.task_scope();
    let task = match (outcome.task_kind(), task_scope) {
        (
            LifecycleMaintenanceTaskKind::Retention,
            Some(LifecycleMaintenanceTaskScope::Branch(_)),
        ) => MaintenanceTask::Reclaim,
        (LifecycleMaintenanceTaskKind::Checkpoint, _) => MaintenanceTask::Checkpoint,
        (LifecycleMaintenanceTaskKind::Flush, _) => MaintenanceTask::Flush,
        (LifecycleMaintenanceTaskKind::Compaction, _) => MaintenanceTask::Compact,
        (LifecycleMaintenanceTaskKind::Materialization, _) => MaintenanceTask::Materialize,
        (LifecycleMaintenanceTaskKind::SnapshotPruning, _) => MaintenanceTask::SnapshotPruning,
        (LifecycleMaintenanceTaskKind::Retention, _) => MaintenanceTask::Retain,
        (LifecycleMaintenanceTaskKind::Quarantine, _) => MaintenanceTask::Quarantine,
        (LifecycleMaintenanceTaskKind::Purge, _) => MaintenanceTask::Purge,
        (LifecycleMaintenanceTaskKind::Repair, _) => MaintenanceTask::Repair,
        (
            LifecycleMaintenanceTaskKind::WalTruncation
            | LifecycleMaintenanceTaskKind::FlushWatermark
            | LifecycleMaintenanceTaskKind::HealthCollection,
            _,
        ) => MaintenanceTask::WalGrowth,
    };
    let scope = match task_scope {
        Some(
            LifecycleMaintenanceTaskScope::Branch(branch_id)
            | LifecycleMaintenanceTaskScope::TableLevel { branch_id, .. }
            | LifecycleMaintenanceTaskScope::InheritedLayer { branch_id, .. },
        ) => MaintenanceScope::Branch(branch_id),
        Some(
            LifecycleMaintenanceTaskScope::Global
            | LifecycleMaintenanceTaskScope::Wal
            | LifecycleMaintenanceTaskScope::Checkpoint
            | LifecycleMaintenanceTaskScope::Quarantine
            | LifecycleMaintenanceTaskScope::Retention,
        )
        | None => MaintenanceScope::Global,
    };
    MaintenanceRequest::new(task, scope)
}

pub(super) fn map_maintenance_task_request(
    runtime: &StorageRuntime<'_>,
    request: &MaintenanceRequest,
) -> StorageApiResult<LifecycleMaintenanceTaskRequest> {
    match request.task() {
        MaintenanceTask::Checkpoint => {
            Ok(LifecycleMaintenanceTaskRequest::checkpoint_with_options(
                MaintenanceCheckpointOptions::new(None, false),
            ))
        }
        MaintenanceTask::Flush => Ok(LifecycleMaintenanceTaskRequest::flush(
            runtime.branch_for_maintenance_scope(request.scope())?,
        )),
        MaintenanceTask::Compact => Ok(LifecycleMaintenanceTaskRequest::compaction(
            runtime.branch_for_maintenance_scope(request.scope())?,
            0,
        )),
        MaintenanceTask::Materialize => Ok(LifecycleMaintenanceTaskRequest::materialization(
            runtime.branch_for_maintenance_scope(request.scope())?,
        )),
        MaintenanceTask::Retain => Ok(LifecycleMaintenanceTaskRequest::retention(1)),
        MaintenanceTask::SnapshotPruning => {
            Ok(LifecycleMaintenanceTaskRequest::snapshot_pruning(1))
        }
        MaintenanceTask::Reclaim => {
            let branch_id = runtime.branch_for_maintenance_scope(request.scope())?;
            LifecycleMaintenanceTaskRequest::new(
                LifecycleMaintenanceTaskKind::Retention,
                LifecycleMaintenanceTaskPriority::Low,
                LifecycleMaintenanceTaskScope::Branch(branch_id),
                LifecycleMaintenanceTaskPolicy::coalescing(),
            )
            .map_err(map_lifecycle_error)
        }
        MaintenanceTask::Quarantine => Ok(LifecycleMaintenanceTaskRequest::quarantine()),
        MaintenanceTask::Purge => Ok(LifecycleMaintenanceTaskRequest::purge_quarantine(
            runtime.branch_for_maintenance_scope(request.scope())?,
        )),
        MaintenanceTask::Repair => match request.scope() {
            MaintenanceScope::Global => {
                Ok(LifecycleMaintenanceTaskRequest::repair_quarantine_family())
            }
            MaintenanceScope::Branch(branch_id) => {
                require_valid_branch_identifier(branch_id, "branch_id")?;
                Ok(LifecycleMaintenanceTaskRequest::repair_quarantine(
                    branch_id,
                ))
            }
        },
        MaintenanceTask::WalGrowth => Err(StorageApiError::InvalidArgument {
            field: "task",
            reason: "WAL growth policy cannot be enqueued directly",
        }),
    }
}

pub(super) const fn background_priority_for_task_request(
    request: LifecycleMaintenanceTaskRequest,
) -> BackgroundTaskPriority {
    match request.kind() {
        LifecycleMaintenanceTaskKind::Flush
        | LifecycleMaintenanceTaskKind::Checkpoint
        | LifecycleMaintenanceTaskKind::FlushWatermark
        | LifecycleMaintenanceTaskKind::WalTruncation => BackgroundTaskPriority::High,
        LifecycleMaintenanceTaskKind::Compaction
        | LifecycleMaintenanceTaskKind::Materialization => BackgroundTaskPriority::Normal,
        LifecycleMaintenanceTaskKind::HealthCollection
        | LifecycleMaintenanceTaskKind::Retention
        | LifecycleMaintenanceTaskKind::SnapshotPruning
        | LifecycleMaintenanceTaskKind::Quarantine
        | LifecycleMaintenanceTaskKind::Purge
        | LifecycleMaintenanceTaskKind::Repair => BackgroundTaskPriority::Low,
    }
}

pub(super) fn run_next_cache_maintenance(
    runtime: &mut LifecycleCacheRuntime<ApiTimestampSource>,
) -> StorageApiResult<Option<LifecycleMaintenanceOutcome>> {
    if let Some(outcome) = runtime
        .run_next_flush_maintenance()
        .map_err(map_lifecycle_error)?
    {
        return Ok(Some(outcome));
    }
    runtime
        .run_next_table_rewrite_maintenance()
        .map_err(map_lifecycle_error)
}

pub(super) fn drain_cache_background_round(
    runtime: &Arc<ParkingMutex<LifecycleCacheRuntime<ApiTimestampSource>>>,
    limits: BackgroundDrainLimits,
    clock: &Arc<dyn MaintenanceClock>,
) -> BackgroundDrainRound {
    let start = clock.now();
    let mut tasks_completed = 0;
    let mut made_progress = false;
    while tasks_completed < limits.max_tasks
        && clock.now().saturating_duration_since(start) < limits.max_runtime
    {
        let task_start = perf_trace::start_timer();
        let snapshot_start = perf_trace::start_timer();
        let (step, pending_before) = {
            let mut runtime = runtime.lock();
            let pending_before = runtime.maintenance_status().pending_tasks();
            let step = match runtime.start_next_background_flush_maintenance() {
                Ok(Some(step)) => Ok(Some(step)),
                Ok(None) => match runtime.run_next_flush_maintenance() {
                    Ok(Some(outcome)) => Ok(Some(CacheBackgroundMaintenanceStep::Completed(
                        Box::new(outcome),
                    ))),
                    Ok(None) => runtime.start_next_background_table_rewrite_maintenance(),
                    Err(error) => Err(error),
                },
                Err(error) => Err(error),
            };
            (step, pending_before)
        };
        perf_trace::record_lifecycle_background_task_snapshot_lock(perf_trace::timer_elapsed(
            snapshot_start,
        ));
        let Ok(step) = step else {
            perf_trace::record_lifecycle_background_task_start_failure();
            let pending_after = runtime.lock().maintenance_status().pending_tasks();
            made_progress |= pending_after < pending_before;
            break;
        };
        let Some(step) = step else {
            let coverage_scheduled = {
                let mut runtime = runtime.lock();
                runtime.schedule_background_maintenance_coverage()
            };
            if coverage_scheduled {
                made_progress = true;
                continue;
            }
            break;
        };
        match step {
            CacheBackgroundMaintenanceStep::Completed(_outcome) => {
                perf_trace::record_lifecycle_background_task_total(perf_trace::timer_elapsed(
                    task_start,
                ));
                tasks_completed += 1;
                made_progress = true;
            }
            CacheBackgroundMaintenanceStep::Build(pending_build) => {
                let task = pending_build.task();
                let build_start = perf_trace::start_timer();
                let build_result = (*pending_build).build();
                perf_trace::record_lifecycle_background_task_unlocked_build(
                    perf_trace::timer_elapsed(build_start),
                );
                let publish_start = perf_trace::start_timer();
                let publish = {
                    let mut runtime = runtime.lock();
                    match build_result {
                        Ok(prepared_build) => runtime.finish_background_maintenance(prepared_build),
                        Err(error) => runtime.finish_background_build_error(task, error),
                    }
                };
                perf_trace::record_lifecycle_background_task_publish_lock(
                    perf_trace::timer_elapsed(publish_start),
                );
                perf_trace::record_lifecycle_background_task_total(perf_trace::timer_elapsed(
                    task_start,
                ));
                if let Ok(_outcome) = publish {
                    tasks_completed += 1;
                    made_progress = true;
                } else {
                    perf_trace::record_lifecycle_background_task_publish_failure();
                    break;
                }
            }
        }
    }
    let mut pending_tasks = runtime.lock().maintenance_status().pending_tasks();
    if tasks_completed > 0 && pending_tasks == 0 {
        let coverage_scheduled = {
            let mut runtime = runtime.lock();
            runtime.schedule_background_maintenance_coverage()
        };
        if coverage_scheduled {
            made_progress = true;
            pending_tasks = runtime.lock().maintenance_status().pending_tasks();
        }
    }
    BackgroundDrainRound {
        tasks_completed,
        pending_tasks,
        made_progress,
    }
}

pub(super) fn run_next_durable_maintenance(
    runtime: &mut LifecycleDurableLocalRuntime<'_, ApiTimestampSource>,
) -> StorageApiResult<Option<LifecycleMaintenanceOutcome>> {
    if let Some(outcome) = runtime
        .run_next_flush_maintenance()
        .map_err(map_lifecycle_error)?
    {
        return Ok(Some(outcome));
    }
    if let Some(outcome) = runtime
        .run_next_checkpoint_maintenance()
        .map_err(map_lifecycle_error)?
    {
        return Ok(Some(outcome));
    }
    if let Some(outcome) = runtime
        .run_next_flush_watermark_maintenance()
        .map_err(map_lifecycle_error)?
    {
        return Ok(Some(outcome));
    }
    if let Some(outcome) = runtime
        .run_next_wal_truncation_maintenance()
        .map_err(map_lifecycle_error)?
    {
        return Ok(Some(outcome));
    }
    if let Some(outcome) = runtime
        .run_next_table_rewrite_maintenance()
        .map_err(map_lifecycle_error)?
    {
        return Ok(Some(outcome));
    }
    if let Some(outcome) = runtime
        .run_next_retention_maintenance()
        .map_err(map_lifecycle_error)?
    {
        return Ok(Some(outcome));
    }
    if let Some(outcome) = runtime
        .run_next_purge_maintenance()
        .map_err(map_lifecycle_error)?
    {
        return Ok(Some(outcome));
    }
    if let Some(outcome) = runtime
        .run_next_quarantine_maintenance()
        .map_err(map_lifecycle_error)?
    {
        return Ok(Some(outcome));
    }
    runtime
        .run_next_quarantine_repair_maintenance()
        .map_err(map_lifecycle_error)
}

pub(super) fn run_next_background_durable_maintenance(
    runtime: &mut LifecycleDurableLocalRuntime<'_, ApiTimestampSource>,
) -> StorageApiResult<Option<LifecycleMaintenanceOutcome>> {
    if let Some(outcome) = runtime
        .run_next_retention_maintenance()
        .map_err(map_lifecycle_error)?
    {
        return Ok(Some(outcome));
    }
    if let Some(outcome) = runtime
        .run_next_purge_maintenance()
        .map_err(map_lifecycle_error)?
    {
        return Ok(Some(outcome));
    }
    if let Some(outcome) = runtime
        .run_next_quarantine_maintenance()
        .map_err(map_lifecycle_error)?
    {
        return Ok(Some(outcome));
    }
    runtime
        .run_next_quarantine_repair_maintenance()
        .map_err(map_lifecycle_error)
}

/// How many upper-tier tasks a drain round may complete before it services one pending
/// low-tier task (retention/purge/quarantine/repair). Without this interleave the strict
/// tier fall-through starves reclaim forever under sustained load — space debt then grows
/// unboundedly (~9× the live dataset observed at 10M). The wake priority mapping is
/// untouched: low-tier work still never *wakes* a worker ahead of durability work; this
/// only bounds how long an already-awake round may ignore it.
const LOW_TIER_SERVICE_INTERVAL: usize = 4;

/// The strict upper-tier dispatch ladder of the durable background round: flush → checkpoint →
/// flush-watermark → WAL truncation → table rewrite → (all empty) low-tier maintenance.
fn start_next_background_step(
    runtime: &mut LifecycleDurableLocalRuntime<'static, ApiTimestampSource>,
    flush_watermark_exhausted: bool,
) -> StorageApiResult<Option<DurableBackgroundMaintenanceStep<'static>>> {
    match runtime.start_next_background_flush_maintenance() {
        Ok(Some(step)) => Ok(Some(step)),
        Ok(None) => match runtime.start_next_background_checkpoint_maintenance() {
            Ok(Some(step)) => Ok(Some(step)),
            Ok(None) => {
                let flush_watermark_step = if flush_watermark_exhausted {
                    Ok(None)
                } else {
                    runtime.start_next_background_flush_watermark_maintenance()
                };
                match flush_watermark_step {
                    Ok(Some(step)) => Ok(Some(step)),
                    Ok(None) => match runtime.start_next_background_wal_truncation_maintenance() {
                        Ok(Some(step)) => Ok(Some(step)),
                        Ok(None) => {
                            match runtime.start_next_background_table_rewrite_maintenance() {
                                Ok(Some(step)) => Ok(Some(step)),
                                Ok(None) => run_next_background_durable_maintenance(runtime).map(
                                    |outcome| {
                                        outcome.map(DurableBackgroundMaintenanceStep::completed)
                                    },
                                ),
                                Err(error) => Err(map_lifecycle_error(error)),
                            }
                        }
                        Err(error) => Err(map_lifecycle_error(error)),
                    },
                    Err(error) => Err(map_lifecycle_error(error)),
                }
            }
            Err(error) => Err(map_lifecycle_error(error)),
        },
        Err(error) => Err(map_lifecycle_error(error)),
    }
}

#[allow(
    clippy::too_many_lines,
    reason = "durable background drain is an explicit start/build/publish state machine"
)]
pub(super) fn drain_durable_background_round(
    runtime: &Arc<ParkingMutex<LifecycleDurableLocalRuntime<'static, ApiTimestampSource>>>,
    limits: BackgroundDrainLimits,
    clock: &Arc<dyn MaintenanceClock>,
) -> BackgroundDrainRound {
    let start = clock.now();
    let mut tasks_completed = 0;
    let mut made_progress = false;
    // D.2b-2: once an off-lock flush-watermark coverage scan finds nothing coverable this
    // round, skip flush-watermark for the rest of the round so the maintenance that runs
    // after it (WAL truncation, table rewrite, durable) is not starved — mirrors the
    // pre-D.2b under-lock fall-through.
    let mut flush_watermark_exhausted = false;
    // Anti-starvation interleave: upper-tier completions since the last low-tier service.
    let mut upper_tier_since_low = 0usize;
    while tasks_completed < limits.max_tasks
        && clock.now().saturating_duration_since(start) < limits.max_runtime
    {
        let task_start = perf_trace::start_timer();
        let snapshot_start = perf_trace::start_timer();
        let mut serviced_low_tier = false;
        let (step, pending_before) = {
            let mut runtime = runtime.lock();
            let pending_before = runtime.maintenance_status().pending_tasks();
            // Guaranteed low-tier progress: after every LOW_TIER_SERVICE_INTERVAL upper-tier
            // tasks with low-tier work pending, service one low-tier task before the ladder.
            // Falls through to the ladder when the low tier has nothing startable.
            let step = if upper_tier_since_low >= LOW_TIER_SERVICE_INTERVAL
                && runtime.has_pending_low_tier_maintenance()
            {
                match run_next_background_durable_maintenance(&mut runtime) {
                    Ok(Some(outcome)) => {
                        serviced_low_tier = true;
                        Ok(Some(DurableBackgroundMaintenanceStep::completed(outcome)))
                    }
                    Ok(None) => start_next_background_step(&mut runtime, flush_watermark_exhausted),
                    Err(error) => Err(error),
                }
            } else {
                start_next_background_step(&mut runtime, flush_watermark_exhausted)
            };
            (step, pending_before)
        };
        perf_trace::record_lifecycle_background_task_snapshot_lock(perf_trace::timer_elapsed(
            snapshot_start,
        ));
        let Ok(step) = step else {
            perf_trace::record_lifecycle_background_task_start_failure();
            let pending_after = runtime.lock().maintenance_status().pending_tasks();
            made_progress |= pending_after < pending_before;
            break;
        };
        let Some(step) = step else {
            let coverage_scheduled = {
                let mut runtime = runtime.lock();
                runtime.schedule_background_maintenance_coverage()
            };
            if coverage_scheduled {
                made_progress = true;
                continue;
            }
            break;
        };
        match step {
            DurableBackgroundMaintenanceStep::Completed(_outcome) => {
                perf_trace::record_lifecycle_background_task_total(perf_trace::timer_elapsed(
                    task_start,
                ));
                tasks_completed += 1;
                made_progress = true;
                if serviced_low_tier {
                    upper_tier_since_low = 0;
                } else {
                    upper_tier_since_low += 1;
                }
            }
            DurableBackgroundMaintenanceStep::Build(pending_build) => {
                let pending_build = *pending_build;
                let task = pending_build.task();
                let build_start = perf_trace::start_timer();
                let build_result = pending_build.build();
                perf_trace::record_lifecycle_background_task_unlocked_build(
                    perf_trace::timer_elapsed(build_start),
                );
                // Phase one: install in memory, reserve the manifest sequence, and claim the
                // per-branch publish slot under the global lock. The manifest fsync itself is
                // deferred to phase two so it runs off-lock.
                let publish_start = perf_trace::start_timer();
                let begin = {
                    let mut runtime = runtime.lock();
                    match build_result {
                        Ok(prepared_build) => runtime.begin_publish_phase(prepared_build),
                        Err(error) => Ok(PreparedPublishStep::Done(
                            runtime.finish_background_build_error(task, error),
                        )),
                    }
                };
                perf_trace::record_lifecycle_background_task_publish_lock(
                    perf_trace::timer_elapsed(publish_start),
                );
                let publish = match begin {
                    Ok(PreparedPublishStep::Done(result)) => result,
                    Ok(PreparedPublishStep::OffLock(prepared)) => {
                        // Phase two: durable manifest fsync with the global lock RELEASED. The
                        // per-branch publish slot stays held so no concurrent same-branch publish
                        // can interleave between the reserved sequence and the catalog record.
                        let (prepared, write_result) = prepared.persist_off_lock();
                        // Phase three: fold the persisted manifest back into the catalog and
                        // finish the task under the global lock again.
                        let finish_start = perf_trace::start_timer();
                        let finish = {
                            let mut runtime = runtime.lock();
                            runtime.finish_publish_phase(prepared, write_result)
                        };
                        perf_trace::record_lifecycle_background_task_publish_lock(
                            perf_trace::timer_elapsed(finish_start),
                        );
                        finish
                    }
                    Err(error) => Err(error),
                };
                perf_trace::record_lifecycle_background_task_total(perf_trace::timer_elapsed(
                    task_start,
                ));
                if let Ok(_outcome) = publish {
                    tasks_completed += 1;
                    made_progress = true;
                    upper_tier_since_low += 1;
                } else {
                    perf_trace::record_lifecycle_background_task_publish_failure();
                    break;
                }
            }
            DurableBackgroundMaintenanceStep::FlushWatermarkCompute(inputs) => {
                // The O(rows) flush-watermark coverage scan runs with the runtime lock
                // RELEASED (D.2b-2); only the O(1) capture and apply take the lock.
                let compute_start = perf_trace::start_timer();
                let computed = inputs.compute_coverage();
                perf_trace::record_lifecycle_background_task_unlocked_build(
                    perf_trace::timer_elapsed(compute_start),
                );
                let apply = {
                    let mut runtime = runtime.lock();
                    runtime.apply_flush_watermark_coverage(&inputs, computed)
                };
                perf_trace::record_lifecycle_background_task_total(perf_trace::timer_elapsed(
                    task_start,
                ));
                match apply {
                    Ok(Some(_outcome)) => {
                        tasks_completed += 1;
                        made_progress = true;
                        upper_tier_since_low += 1;
                    }
                    Ok(None) => {
                        // Nothing coverable this round; skip flush-watermark for the rest
                        // of the round so later maintenance is not starved.
                        flush_watermark_exhausted = true;
                    }
                    Err(_error) => {
                        perf_trace::record_lifecycle_background_task_publish_failure();
                        break;
                    }
                }
            }
        }
    }
    let mut pending_tasks = runtime.lock().maintenance_status().pending_tasks();
    if tasks_completed > 0 && pending_tasks == 0 {
        let coverage_scheduled = {
            let mut runtime = runtime.lock();
            runtime.schedule_background_maintenance_coverage()
        };
        if coverage_scheduled {
            made_progress = true;
            pending_tasks = runtime.lock().maintenance_status().pending_tasks();
        }
    }
    BackgroundDrainRound {
        tasks_completed,
        pending_tasks,
        made_progress,
    }
}
