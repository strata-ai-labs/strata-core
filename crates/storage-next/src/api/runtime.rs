//! API runtime handle.

use crate::backend::BackendHandle;
use crate::branch::config::BranchRuntimeConfig;
use crate::branch::facts::BranchReleasePlan;
use crate::branch::read::{
    BranchHistoryOptions, BranchReadBound, BranchReadView, BranchScanBounds, BranchUserKeyBound,
};
use crate::commit::{
    CommitBranchGeneration, CommitBranchGenerationGuard, CommitDurabilityClass,
    CommitRuntimeConfig, CommitTimelineMiss, CommitTimelineView, CommitTimestampSource,
    COMMIT_TIMELINE_SPACE,
};
use crate::lifecycle::{
    collect_storage_pressure_with_budget, BackgroundBackpressureError, BackgroundTaskPriority,
    CacheBackgroundMaintenanceStep, CloseOutcome, CloseOutcomeStatus,
    DurableBackgroundMaintenanceStep, FlushFrozenRequest, FlushTableIdentitySeed,
    FlushTableObjectId, InlineMaintenanceExecutor, LifecycleBranchCatalog,
    LifecycleBranchDescriptor, LifecycleBranchStatus, LifecycleCacheOpenRequest,
    LifecycleCacheRuntime, LifecycleCheckpointOutcome, LifecycleCodecId,
    LifecycleCompactionDrainRequest, LifecycleConfig, LifecycleDurableLocalOpenRequest,
    LifecycleDurableLocalRuntime, LifecycleDurableLocalShell, LifecycleError,
    LifecycleMaintenanceSchedulingPolicy, LifecycleMaintenanceStats, LifecycleRecoveryRuntime,
    LifecycleRetentionRequest, LifecycleRetentionScope, LifecycleStoragePressure,
    LifecycleStoragePressureReason, LifecycleStoragePressureSeverity, LifecycleWalGrowthOutcome,
    LifecycleWalGrowthPolicy, LifecycleWalGrowthStatus, LifecycleWalGrowthTrigger,
    LifecycleWriteAdmissionOutcome, LifecycleWriteAdmissionStatus, MaintenanceCheckpointOptions,
    MaintenanceClock, MaintenanceExecutor, MaintenanceExecutorStats, MaintenanceExecutorStatus,
    MaintenanceInstant, MaintenanceOutcome as LifecycleMaintenanceOutcome,
    MaintenanceOutcomeReasonClass as LifecycleMaintenanceOutcomeReasonClass,
    MaintenanceOutcomeStatus as LifecycleMaintenanceOutcomeStatus,
    MaintenanceTaskKind as LifecycleMaintenanceTaskKind,
    MaintenanceTaskPolicy as LifecycleMaintenanceTaskPolicy,
    MaintenanceTaskPriority as LifecycleMaintenanceTaskPriority,
    MaintenanceTaskRequest as LifecycleMaintenanceTaskRequest,
    MaintenanceTaskScope as LifecycleMaintenanceTaskScope, ManualMaintenanceClock,
    RealMaintenanceClock, RecoveryDegradationClass, RecoveryFaultKind, RecoveryHealth,
    RecoveryStrictness, StorageBudgetPool, StorageBudgetPressureSeverity, StorageBudgetSnapshot,
    StorageMode as LifecycleStorageMode, StorageOpenOutcome as LifecycleStorageOpenOutcome,
    StorageOpenPlan, StorageRuntimeBudget, ThreadedMaintenanceExecutor,
};
use crate::observability::perf_trace;
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId as RowStorageSpaceId};
use crate::service::{WalGrowthFacts, WalServiceConfig};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

use super::{
    BranchAction, BranchCleanupSummary, BranchGeneration, BranchOperation, BranchOutcome,
    BranchParentSummary, BranchRequest, BranchStatus, BranchSummary, CommitAdmissionPressureReason,
    CommitAdmissionPressureSeverity, CommitAdmissionSummary, CommitBatch, CommitDurability,
    CommitDurabilitySummary, CommitExpectedVersion, CommitSummary, DiagnosticsBranchCatalogReport,
    DiagnosticsBudgetPool, DiagnosticsBudgetPressure, DiagnosticsBudgetReport,
    DiagnosticsBudgetUsage, DiagnosticsCheckpointReport, DiagnosticsOutcome,
    DiagnosticsQuarantineReport, DiagnosticsReadActivityReport, DiagnosticsRecoveryClass,
    DiagnosticsRecoveryFault, DiagnosticsRecoveryFaultKind, DiagnosticsRecoveryReport,
    DiagnosticsRequest, DiagnosticsRetentionReport, DiagnosticsScope,
    DiagnosticsSourceLayoutReport, DiagnosticsSourceLevelTableCount,
    DiagnosticsStoragePressureReason, DiagnosticsStoragePressureReport,
    DiagnosticsStoragePressureSeverity, DiagnosticsTableReachabilityReport,
    DiagnosticsTimelineReport, DiagnosticsWalGrowthReport, HistoryReadOutcome, HistoryReadRequest,
    MaintenanceDrainSummary, MaintenanceQueueSummary, MaintenanceReasonClass, MaintenanceRequest,
    MaintenanceScope, MaintenanceSummary, MaintenanceSummaryStatus, MaintenanceTask,
    MaintenanceWalGrowthStatus, MaintenanceWalGrowthSummary, MaintenanceWalGrowthTrigger,
    PointReadOutcome, PointReadRequest, PrefixScanReadRequest, ReadBound, ReadLimit,
    RecoveryHealthSummary, ScanReadOutcome, ScanReadRequest, StorageApiError, StorageApiErrorClass,
    StorageApiLowerLayer, StorageApiResult, StorageBackend, StorageBackgroundMaintenanceOptions,
    StorageBudgetPolicy, StorageCloseSummary, StorageDurabilityPolicy, StorageKey,
    StorageMaintenanceSchedulingPolicy, StorageMode, StorageOpenDisposition, StorageOpenOptions,
    StorageOpenOutcome, StorageOpenSummary, StorageReadRow, StorageRuntimeState, StorageSpaceId,
    StorageValue, StorageWalGrowthPolicy, TimelineBoundsOutcome, TimelineBoundsRequest,
    TimestampLookupMiss, TimestampLookupOutcome, TimestampLookupRequest, VersionLookupOutcome,
    VersionLookupRequest,
};
use crate::api::outcome::StorageCloseEffects;
use parking_lot::{Mutex as ParkingMutex, MutexGuard as ParkingMutexGuard};
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

const DEFAULT_DATABASE_ID: [u8; 16] = [0x53; 16];
const DEFAULT_BRANCH_ID: BranchId = BranchId::from_bytes([0x01; BranchId::BYTE_LEN]);
const DEFAULT_BRANCH_GENERATION: u64 = 1;
const DEFAULT_TIMESTAMP: Timestamp = Timestamp::from_micros(1);
const API_PHYSICAL_SPACE: &str = "api";
const DEFAULT_BACKGROUND_URGENT_BASE_SLOWDOWN: Duration = Duration::from_micros(100);
const DEFAULT_BACKGROUND_URGENT_MAX_SLOWDOWN: Duration = Duration::from_millis(25);
const DEFAULT_BACKGROUND_URGENT_NO_RELIEF_ROUNDS: usize = 2;
const DEFAULT_BACKGROUND_BLOCK_WAIT_SLICE: Duration = Duration::from_millis(250);
const DEFAULT_BACKGROUND_BLOCK_STALL_DEADLINE: Duration = Duration::from_secs(30);
const DEFAULT_BACKGROUND_BLOCK_NO_RELIEF_ROUNDS: usize = 4;
const DEFAULT_BACKGROUND_CLOSE_SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(250);
pub(crate) const BACKGROUND_LEVEL_ZERO_URGENT_START_TABLES: usize = 8;
pub(crate) const BACKGROUND_LEVEL_ZERO_BLOCK_TABLES: usize = 16;
pub(crate) const BACKGROUND_LEVEL_ZERO_URGENT_UNIT_MULTIPLIER: usize = 32;

fn background_pressure_byte_units(bytes: u64) -> usize {
    let units = bytes / (4 * 1024 * 1024);
    usize::try_from(units).unwrap_or(usize::MAX)
}

pub(crate) fn background_level_zero_pressure_units(
    reason: LifecycleStoragePressureReason,
    level_zero_tables: usize,
) -> usize {
    if reason != LifecycleStoragePressureReason::LevelZeroTableBacklog {
        return level_zero_tables / 4;
    }
    let urgent_distance = level_zero_tables
        .saturating_add(1)
        .saturating_sub(BACKGROUND_LEVEL_ZERO_URGENT_START_TABLES)
        .min(
            BACKGROUND_LEVEL_ZERO_BLOCK_TABLES
                .saturating_sub(BACKGROUND_LEVEL_ZERO_URGENT_START_TABLES),
        );
    urgent_distance.saturating_mul(BACKGROUND_LEVEL_ZERO_URGENT_UNIT_MULTIPLIER)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ResolvedReadBound {
    branch_bound: BranchReadBound,
    selected_timestamp: Option<Timestamp>,
}

#[derive(Debug)]
pub struct StorageRuntime<'a> {
    inner: StorageRuntimeInner<'a>,
    open_summary: Option<StorageOpenSummary>,
    last_recovery: Option<DiagnosticsRecoveryReport>,
    last_close: Option<StorageCloseSummary>,
}

#[derive(Debug)]
enum StorageRuntimeInner<'a> {
    Cache(Box<RuntimeSlot<LifecycleCacheRuntime<ApiTimestampSource>>>),
    Durable(Box<RuntimeSlot<LifecycleDurableLocalRuntime<'a, ApiTimestampSource>>>),
    DurableOwned(Box<RuntimeSlot<LifecycleDurableLocalRuntime<'static, ApiTimestampSource>>>),
    Closed,
}

struct RuntimeSlot<R> {
    runtime: Arc<ParkingMutex<R>>,
    background: Option<BackgroundRuntimeController>,
    background_drain: Option<BackgroundDrainFn>,
    #[cfg(test)]
    background_block_wait: BackgroundBlockWaitConfig,
}

type BackgroundDrainFn = Arc<
    dyn Fn(BackgroundDrainLimits, Arc<dyn MaintenanceClock>) -> BackgroundDrainRound + Send + Sync,
>;
type BackgroundArcDrain<R> = fn(
    &Arc<ParkingMutex<R>>,
    BackgroundDrainLimits,
    &Arc<dyn MaintenanceClock>,
) -> BackgroundDrainRound;

impl<R> fmt::Debug for RuntimeSlot<R>
where
    R: fmt::Debug,
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut debug = formatter.debug_struct("RuntimeSlot");
        debug
            .field("runtime", &self.runtime)
            .field("background", &self.background)
            .field("background_drain", &self.background_drain.is_some());
        #[cfg(test)]
        debug.field("background_block_wait", &self.background_block_wait);
        debug.finish()
    }
}

struct BackgroundRuntimeController {
    executor: Arc<dyn MaintenanceExecutor>,
    clock: Arc<dyn MaintenanceClock>,
    admission_throttle: Arc<ParkingMutex<BackgroundAdmissionThrottle>>,
    close_requested: Arc<AtomicBool>,
    wake_scheduled_mask: Arc<AtomicUsize>,
    wake_requested_mask: Arc<AtomicUsize>,
    max_tasks_per_wake: usize,
    max_runtime_per_wake: Duration,
    drain_immediately: bool,
}

impl fmt::Debug for BackgroundRuntimeController {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("BackgroundRuntimeController")
            .field("stats", &self.stats())
            .field("admission_throttle", &self.admission_throttle)
            .field("close_requested", &self.close_requested)
            .field("wake_scheduled_mask", &self.wake_scheduled_mask)
            .field("wake_requested_mask", &self.wake_requested_mask)
            .field("max_tasks_per_wake", &self.max_tasks_per_wake)
            .field("max_runtime_per_wake", &self.max_runtime_per_wake)
            .field("drain_immediately", &self.drain_immediately)
            .finish_non_exhaustive()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BackgroundDrainRound {
    tasks_completed: usize,
    pending_tasks: usize,
    made_progress: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BackgroundDrainLimits {
    max_tasks: usize,
    max_runtime: Duration,
}

#[derive(Clone, Debug, PartialEq)]
struct BackgroundShutdownStats {
    stats: MaintenanceExecutorStats,
    first_shutdown: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BackgroundExecutorMode {
    Threaded,
    Inline,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BackgroundBlockWaitConfig {
    wait_slice: Duration,
    stall_deadline: Duration,
    no_relief_rounds: usize,
}

impl Default for BackgroundBlockWaitConfig {
    fn default() -> Self {
        Self {
            wait_slice: DEFAULT_BACKGROUND_BLOCK_WAIT_SLICE,
            stall_deadline: DEFAULT_BACKGROUND_BLOCK_STALL_DEADLINE,
            no_relief_rounds: DEFAULT_BACKGROUND_BLOCK_NO_RELIEF_ROUNDS,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BackgroundPressureSnapshot {
    severity: LifecycleStoragePressureSeverity,
    active_bytes: u64,
    frozen_tables: usize,
    frozen_bytes: u64,
    level_zero_tables: usize,
    owned_tables: usize,
    table_rewrite_bytes: u64,
    inherited_layers: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BackgroundAdmissionThrottleObservation {
    branch_id: BranchId,
    reason: LifecycleStoragePressureReason,
    severity: LifecycleStoragePressureSeverity,
    pressure_units: usize,
    pressure_snapshot: BackgroundPressureSnapshot,
    completed_tasks: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BackgroundAdmissionThrottleDecision {
    slowdown: Option<Duration>,
    no_relief_escalated: bool,
    relief_reset: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BackgroundAdmissionThrottle {
    last: Option<BackgroundAdmissionThrottleObservation>,
    consecutive_no_relief_rounds: usize,
    current_slowdown: Duration,
    last_slowdown_at: Option<MaintenanceInstant>,
}

impl Default for BackgroundAdmissionThrottle {
    fn default() -> Self {
        Self {
            last: None,
            consecutive_no_relief_rounds: 0,
            current_slowdown: Duration::ZERO,
            last_slowdown_at: None,
        }
    }
}

impl BackgroundAdmissionThrottle {
    fn reset(&mut self) -> bool {
        let had_pressure = self.last.is_some()
            || self.consecutive_no_relief_rounds != 0
            || !self.current_slowdown.is_zero()
            || self.last_slowdown_at.is_some();
        *self = Self::default();
        had_pressure
    }

    fn observe_urgent(
        &mut self,
        observation: BackgroundAdmissionThrottleObservation,
        base_slowdown: Duration,
        now: MaintenanceInstant,
    ) -> BackgroundAdmissionThrottleDecision {
        let Some(last) = self.last else {
            self.last = Some(observation);
            return BackgroundAdmissionThrottleDecision {
                slowdown: None,
                no_relief_escalated: false,
                relief_reset: false,
            };
        };

        let completed_tasks_advanced = observation.completed_tasks > last.completed_tasks;
        let pressure_units_improved = observation.pressure_units < last.pressure_units;
        let pressure_units_stable = observation.pressure_units == last.pressure_units;
        let pressure_snapshot_relieved = observation
            .pressure_snapshot
            .relieved_since(last.pressure_snapshot, observation.reason);
        if observation.branch_id != last.branch_id
            || observation.reason != last.reason
            || (completed_tasks_advanced && (pressure_units_stable || pressure_units_improved))
            || lifecycle_storage_pressure_severity_rank(observation.severity)
                < lifecycle_storage_pressure_severity_rank(last.severity)
            || pressure_units_improved
            || pressure_snapshot_relieved
        {
            let relief_reset =
                self.consecutive_no_relief_rounds != 0 || !self.current_slowdown.is_zero();
            self.consecutive_no_relief_rounds = 0;
            self.current_slowdown = Duration::ZERO;
            self.last_slowdown_at = None;
            self.last = Some(observation);
            return BackgroundAdmissionThrottleDecision {
                slowdown: None,
                no_relief_escalated: false,
                relief_reset,
            };
        }

        self.consecutive_no_relief_rounds = self.consecutive_no_relief_rounds.saturating_add(1);
        self.last = Some(observation);
        if self.consecutive_no_relief_rounds < DEFAULT_BACKGROUND_URGENT_NO_RELIEF_ROUNDS {
            return BackgroundAdmissionThrottleDecision {
                slowdown: None,
                no_relief_escalated: false,
                relief_reset: false,
            };
        }

        let should_escalate = self.last_slowdown_at.is_none_or(|last_slowdown_at| {
            now > last_slowdown_at || self.current_slowdown.is_zero()
        });
        if should_escalate {
            self.current_slowdown = if self.current_slowdown.is_zero() {
                base_slowdown.max(DEFAULT_BACKGROUND_URGENT_BASE_SLOWDOWN)
            } else {
                self.current_slowdown.saturating_mul(2)
            }
            .min(DEFAULT_BACKGROUND_URGENT_MAX_SLOWDOWN);
        }
        self.last_slowdown_at = Some(now);
        BackgroundAdmissionThrottleDecision {
            slowdown: Some(self.current_slowdown),
            no_relief_escalated: should_escalate,
            relief_reset: false,
        }
    }
}

impl BackgroundPressureSnapshot {
    fn from_pressure(pressure: LifecycleStoragePressure) -> Self {
        Self {
            severity: pressure.severity(),
            active_bytes: pressure.active_bytes(),
            frozen_tables: pressure.frozen_tables(),
            frozen_bytes: pressure.frozen_bytes(),
            level_zero_tables: pressure.level_zero_tables(),
            owned_tables: pressure.owned_tables(),
            table_rewrite_bytes: pressure.table_rewrite_bytes(),
            inherited_layers: pressure.inherited_layers(),
        }
    }

    fn relieved_since(self, before: Self, reason: LifecycleStoragePressureReason) -> bool {
        if reason == LifecycleStoragePressureReason::FrozenBacklog {
            return self.active_bytes < before.active_bytes
                || self.frozen_tables < before.frozen_tables
                || self.frozen_bytes < before.frozen_bytes;
        }
        lifecycle_storage_pressure_severity_rank(self.severity)
            < lifecycle_storage_pressure_severity_rank(before.severity)
            || self.active_bytes < before.active_bytes
            || self.frozen_tables < before.frozen_tables
            || self.frozen_bytes < before.frozen_bytes
            || self.level_zero_tables < before.level_zero_tables
            || self.owned_tables < before.owned_tables
            || self.table_rewrite_bytes < before.table_rewrite_bytes
            || self.inherited_layers < before.inherited_layers
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct BackgroundWalGrowthSnapshot {
    retained_bytes: u64,
    retained_segments: usize,
    commits_since_checkpoint: u64,
    exceeds_backpressure: bool,
}

impl BackgroundWalGrowthSnapshot {
    fn from_parts(
        facts: WalGrowthFacts,
        commits_since_checkpoint: u64,
        trigger: Option<LifecycleWalGrowthTrigger>,
    ) -> Self {
        Self {
            retained_bytes: facts.retained_bytes(),
            retained_segments: facts.retained_segments(),
            commits_since_checkpoint,
            exceeds_backpressure: trigger.is_some(),
        }
    }

    fn relieved_since(self, before: Self) -> bool {
        !self.exceeds_backpressure
            || self.retained_bytes < before.retained_bytes
            || self.retained_segments < before.retained_segments
            || self.commits_since_checkpoint < before.commits_since_checkpoint
    }
}

const fn lifecycle_storage_pressure_severity_rank(
    severity: LifecycleStoragePressureSeverity,
) -> u8 {
    match severity {
        LifecycleStoragePressureSeverity::None => 0,
        LifecycleStoragePressureSeverity::Background => 1,
        LifecycleStoragePressureSeverity::Urgent => 2,
        LifecycleStoragePressureSeverity::BlockMutatingAdmission => 3,
    }
}

enum DurableBackendHandleForOpen<'a> {
    Borrowed(BackendHandle<'a>),
    #[cfg(feature = "localfs")]
    Owned(BackendHandle<'static>),
}

impl<R> RuntimeSlot<R> {
    fn new(runtime: R, _config: LifecycleConfig) -> Self {
        Self {
            runtime: Arc::new(ParkingMutex::new(runtime)),
            background: None,
            background_drain: None,
            #[cfg(test)]
            background_block_wait: BackgroundBlockWaitConfig::default(),
        }
    }

    fn lock(&self) -> ParkingMutexGuard<'_, R> {
        let started = perf_trace::start_timer();
        let guard = self.runtime.lock();
        perf_trace::record_lifecycle_foreground_wait_background_lock(perf_trace::timer_elapsed(
            started,
        ));
        guard
    }

    #[allow(
        dead_code,
        reason = "background lifecycle drains submit work through this handle"
    )]
    fn runtime_handle(&self) -> Arc<ParkingMutex<R>> {
        Arc::clone(&self.runtime)
    }

    #[allow(
        dead_code,
        reason = "background lifecycle queue wakeups are wired through this hook"
    )]
    fn submit_background(
        &self,
        priority: BackgroundTaskPriority,
        work: impl FnOnce(Arc<ParkingMutex<R>>) + Send + 'static,
    ) -> Result<(), BackgroundBackpressureError>
    where
        R: Send + 'static,
    {
        let runtime = self.runtime_handle();
        self.background
            .as_ref()
            .ok_or(BackgroundBackpressureError)?
            .submit(priority, move || work(runtime))
    }

    fn notify_background_drain(&self, priority: BackgroundTaskPriority) {
        if let (Some(background), Some(drain)) = (&self.background, &self.background_drain) {
            background.notify_drain(priority, Arc::clone(drain));
        }
    }

    fn wait_background_progress_until(
        &self,
        completed_before_wait: u64,
        deadline: MaintenanceInstant,
    ) -> bool {
        self.background.as_ref().is_some_and(|background| {
            background.wait_for_progress_until(completed_before_wait, deadline)
        })
    }

    fn background_now(&self) -> Option<MaintenanceInstant> {
        self.background
            .as_ref()
            .map(BackgroundRuntimeController::now)
    }

    fn sleep_background_duration(&self, duration: Duration) {
        if let Some(background) = &self.background {
            background.sleep(duration);
        }
    }

    fn background_admission_slowdown_duration(
        &self,
        pressure: LifecycleStoragePressure,
        pressure_units: usize,
        completed_tasks: u64,
    ) -> Option<BackgroundAdmissionThrottleDecision> {
        self.background.as_ref().map(|background| {
            background.admission_slowdown_duration(pressure, pressure_units, completed_tasks)
        })
    }

    fn reset_background_admission_throttle(&self) -> bool {
        self.background
            .as_ref()
            .is_some_and(BackgroundRuntimeController::reset_admission_throttle)
    }

    fn has_background(&self) -> bool {
        self.background.is_some()
    }

    fn background_stats(&self) -> Option<MaintenanceExecutorStats> {
        self.background
            .as_ref()
            .map(BackgroundRuntimeController::stats)
    }

    fn shutdown_background(&self, timeout: Option<Duration>) -> Option<BackgroundShutdownStats> {
        self.background
            .as_ref()
            .map(|background| background.shutdown(timeout))
    }

    fn request_background_shutdown(&self) -> Option<MaintenanceExecutorStats> {
        self.background
            .as_ref()
            .map(BackgroundRuntimeController::request_shutdown)
    }

    #[cfg(test)]
    fn background_shutdown_requested_flag(&self) -> Option<Arc<AtomicBool>> {
        self.background
            .as_ref()
            .map(BackgroundRuntimeController::close_requested_flag)
    }

    #[cfg(test)]
    fn wait_background_idle(&self) {
        if let Some(background) = &self.background {
            background.drain_scheduler();
        }
    }

    #[cfg(test)]
    fn wait_background_idle_until(&self, timeout: Duration) -> Option<MaintenanceExecutorStats> {
        let background = self.background.as_ref()?;
        let deadline = std::time::Instant::now()
            .checked_add(timeout)
            .unwrap_or_else(std::time::Instant::now);
        loop {
            if background.drain_immediately {
                let completed_before_wait = background.stats().tasks_completed;
                let _ = background
                    .executor
                    .wait_for_progress(completed_before_wait, Duration::from_millis(1));
            }
            let stats = background.stats();
            if stats.queue_depth == 0 && stats.active_tasks == 0 {
                return Some(stats);
            }
            if std::time::Instant::now() >= deadline {
                return Some(stats);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[cfg(test)]
    fn set_background_drain_limits(&mut self, max_tasks: usize, max_runtime: Duration) -> bool {
        let Some(background) = &mut self.background else {
            return false;
        };
        background.max_tasks_per_wake = max_tasks;
        background.max_runtime_per_wake = max_runtime;
        true
    }

    #[cfg(test)]
    fn set_background_block_wait_for_test(
        &mut self,
        wait_slice: Duration,
        stall_deadline: Duration,
        no_relief_rounds: usize,
    ) -> bool {
        if self.background.is_none() {
            return false;
        }
        self.background_block_wait = BackgroundBlockWaitConfig {
            wait_slice,
            stall_deadline,
            no_relief_rounds,
        };
        true
    }
}

impl<R> RuntimeSlot<R>
where
    R: Send + 'static,
{
    fn new_with_background_arc_drain(
        runtime: R,
        config: LifecycleConfig,
        background_config: StorageBackgroundMaintenanceOptions,
        executor_mode: BackgroundExecutorMode,
        drain: BackgroundArcDrain<R>,
    ) -> Self {
        let runtime = Arc::new(ParkingMutex::new(runtime));
        let background = if config.maintenance_scheduling_policy()
            == LifecycleMaintenanceSchedulingPolicy::Background
        {
            Some(BackgroundRuntimeController::new(
                background_config,
                executor_mode,
            ))
        } else {
            None
        };
        let background_drain = background.as_ref().map(|_| {
            let runtime = Arc::clone(&runtime);
            Arc::new(move |limits, clock| drain(&runtime, limits, &clock)) as BackgroundDrainFn
        });
        Self {
            runtime,
            background,
            background_drain,
            #[cfg(test)]
            background_block_wait: BackgroundBlockWaitConfig::default(),
        }
    }
}

impl<R> Drop for RuntimeSlot<R> {
    fn drop(&mut self) {
        let _ = self.request_background_shutdown();
    }
}

impl BackgroundRuntimeController {
    fn new(
        background_config: StorageBackgroundMaintenanceOptions,
        executor_mode: BackgroundExecutorMode,
    ) -> Self {
        let (executor, clock, worker_count, drain_immediately): (
            Arc<dyn MaintenanceExecutor>,
            Arc<dyn MaintenanceClock>,
            usize,
            bool,
        ) = match executor_mode {
            BackgroundExecutorMode::Threaded => (
                Arc::new(ThreadedMaintenanceExecutor::new(
                    background_config.worker_count(),
                    background_config.scheduler_queue_depth(),
                )),
                Arc::new(RealMaintenanceClock::new()),
                background_config.worker_count(),
                false,
            ),
            BackgroundExecutorMode::Inline => {
                let clock: Arc<dyn MaintenanceClock> = Arc::new(ManualMaintenanceClock::default());
                (
                    Arc::new(InlineMaintenanceExecutor::new(
                        background_config.scheduler_queue_depth(),
                        Arc::clone(&clock),
                    )) as Arc<dyn MaintenanceExecutor>,
                    clock,
                    0,
                    true,
                )
            }
        };
        perf_trace::record_lifecycle_background_runtime_created(worker_count);
        Self {
            executor,
            clock,
            admission_throttle: Arc::new(ParkingMutex::new(BackgroundAdmissionThrottle::default())),
            close_requested: Arc::new(AtomicBool::new(false)),
            wake_scheduled_mask: Arc::new(AtomicUsize::new(0)),
            wake_requested_mask: Arc::new(AtomicUsize::new(0)),
            max_tasks_per_wake: background_config.max_tasks_per_wake(),
            max_runtime_per_wake: background_config.max_runtime_per_wake(),
            drain_immediately,
        }
    }

    fn stats(&self) -> MaintenanceExecutorStats {
        self.executor.stats()
    }

    fn now(&self) -> MaintenanceInstant {
        self.clock.now()
    }

    fn sleep(&self, duration: Duration) {
        self.clock.sleep(duration);
    }

    fn admission_slowdown_duration(
        &self,
        pressure: LifecycleStoragePressure,
        pressure_units: usize,
        completed_tasks: u64,
    ) -> BackgroundAdmissionThrottleDecision {
        let observation = BackgroundAdmissionThrottleObservation {
            branch_id: pressure.branch_id(),
            reason: pressure.reason(),
            severity: pressure.severity(),
            pressure_units,
            pressure_snapshot: BackgroundPressureSnapshot::from_pressure(pressure),
            completed_tasks,
        };
        let capped_pressure_units =
            u32::try_from(pressure_units.min(250)).expect("pressure unit cap fits in u32");
        let base_slowdown = DEFAULT_BACKGROUND_URGENT_BASE_SLOWDOWN
            .saturating_mul(capped_pressure_units)
            .min(DEFAULT_BACKGROUND_URGENT_MAX_SLOWDOWN);
        self.admission_throttle
            .lock()
            .observe_urgent(observation, base_slowdown, self.clock.now())
    }

    fn reset_admission_throttle(&self) -> bool {
        self.admission_throttle.lock().reset()
    }

    fn submit(
        &self,
        priority: BackgroundTaskPriority,
        work: impl FnOnce() + Send + 'static,
    ) -> Result<(), BackgroundBackpressureError> {
        let capture_enabled = perf_trace::test_capture_enabled_for_current_thread();
        self.executor.submit(
            priority,
            Box::new(move || {
                perf_trace::with_test_capture_enabled_for_current_thread(capture_enabled, work);
            }),
        )
    }

    fn wait_for_progress_until(
        &self,
        completed_before_wait: u64,
        deadline: MaintenanceInstant,
    ) -> bool {
        let now = self.clock.now();
        if now >= deadline {
            return false;
        }
        let timeout = deadline.saturating_duration_since(now);
        let progressed = self
            .executor
            .wait_for_progress(completed_before_wait, timeout);
        if self.drain_immediately {
            let elapsed = self.clock.now().saturating_duration_since(now);
            let simulated_wait = timeout.min(self.max_runtime_per_wake);
            if elapsed < simulated_wait {
                self.clock.sleep(simulated_wait.saturating_sub(elapsed));
            }
        }
        progressed
    }

    #[cfg(test)]
    fn drain_scheduler(&self) {
        self.executor.wait_for_idle();
    }

    fn notify_drain(&self, priority: BackgroundTaskPriority, drain: BackgroundDrainFn) {
        if self.close_requested.load(Ordering::Acquire) {
            perf_trace::record_lifecycle_background_submit_after_shutdown_rejected();
            perf_trace::record_lifecycle_background_wake_rejected();
            return;
        }
        let wake_bit = background_priority_bit(priority);
        let scheduled = self
            .wake_scheduled_mask
            .fetch_or(wake_bit, Ordering::AcqRel);
        if scheduled & wake_bit != 0 {
            self.wake_requested_mask
                .fetch_or(wake_bit, Ordering::AcqRel);
            perf_trace::record_lifecycle_background_wake_coalesced();
            return;
        }
        self.submit_drain(priority, drain, wake_bit);
    }

    fn submit_drain(
        &self,
        priority: BackgroundTaskPriority,
        drain: BackgroundDrainFn,
        wake_bit: usize,
    ) {
        let controller = self.clone();
        let capture_enabled = perf_trace::test_capture_enabled_for_current_thread();
        let submit = self.executor.submit(
            priority,
            Box::new(move || {
                perf_trace::with_test_capture_enabled_for_current_thread(capture_enabled, || {
                    let limits = BackgroundDrainLimits {
                        max_tasks: controller.max_tasks_per_wake,
                        max_runtime: controller.max_runtime_per_wake,
                    };
                    let round = drain(limits, Arc::clone(&controller.clock));
                    perf_trace::record_lifecycle_background_drain_round(round.tasks_completed);
                    if round.tasks_completed > 0 {
                        perf_trace::record_lifecycle_pressure_clear_wake();
                    }
                    controller
                        .wake_scheduled_mask
                        .fetch_and(!wake_bit, Ordering::AcqRel);
                    let requested = controller
                        .wake_requested_mask
                        .fetch_and(!wake_bit, Ordering::AcqRel)
                        & wake_bit
                        != 0;
                    if (round.pending_tasks > 0 && round.made_progress) || requested {
                        controller.notify_drain(priority, drain);
                    }
                });
            }),
        );
        if let Ok(()) = submit {
            perf_trace::record_lifecycle_background_wake_submitted();
            if self.drain_immediately && self.executor.stats().active_tasks == 0 {
                let completed_before_wait = self.executor.stats().tasks_completed;
                let _ = self
                    .executor
                    .wait_for_progress(completed_before_wait, self.max_runtime_per_wake);
            }
        } else {
            self.wake_scheduled_mask
                .fetch_and(!wake_bit, Ordering::AcqRel);
            self.wake_requested_mask
                .fetch_and(!wake_bit, Ordering::AcqRel);
            perf_trace::record_lifecycle_background_wake_rejected();
        }
    }

    fn shutdown(&self, timeout: Option<Duration>) -> BackgroundShutdownStats {
        let first_shutdown = !self.close_requested.swap(true, Ordering::AcqRel);
        if first_shutdown {
            self.executor.shutdown(timeout);
        }
        BackgroundShutdownStats {
            stats: self.executor.stats(),
            first_shutdown,
        }
    }

    fn request_shutdown(&self) -> MaintenanceExecutorStats {
        if !self.close_requested.swap(true, Ordering::AcqRel) {
            let _ = self.executor.request_shutdown();
        }
        self.executor.stats()
    }

    #[cfg(test)]
    fn close_requested_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.close_requested)
    }
}

fn background_pressure_wait_should_continue_for_progress(
    progressed: bool,
    lifecycle_pending_tasks: usize,
    lifecycle_active_task: bool,
    no_relief_rounds: &mut usize,
) -> bool {
    if lifecycle_active_task || (progressed && lifecycle_pending_tasks > 0) {
        *no_relief_rounds = 0;
        return true;
    }
    false
}

impl Clone for BackgroundRuntimeController {
    fn clone(&self) -> Self {
        Self {
            executor: Arc::clone(&self.executor),
            clock: Arc::clone(&self.clock),
            admission_throttle: Arc::clone(&self.admission_throttle),
            close_requested: Arc::clone(&self.close_requested),
            wake_scheduled_mask: Arc::clone(&self.wake_scheduled_mask),
            wake_requested_mask: Arc::clone(&self.wake_requested_mask),
            max_tasks_per_wake: self.max_tasks_per_wake,
            max_runtime_per_wake: self.max_runtime_per_wake,
            drain_immediately: self.drain_immediately,
        }
    }
}

const fn background_priority_value(priority: BackgroundTaskPriority) -> usize {
    match priority {
        BackgroundTaskPriority::Low => 0,
        BackgroundTaskPriority::Normal => 1,
        BackgroundTaskPriority::High => 2,
    }
}

const fn background_priority_bit(priority: BackgroundTaskPriority) -> usize {
    1usize << background_priority_value(priority)
}

#[cfg(test)]
mod background_controller_tests {
    use super::*;
    use std::sync::atomic::{AtomicBool as TestAtomicBool, Ordering as TestOrdering};
    use std::sync::{Arc as TestArc, Barrier};
    use std::time::Instant;

    #[test]
    fn controller_submits_high_priority_wake_while_normal_drain_is_active() {
        let controller = BackgroundRuntimeController::new(
            StorageBackgroundMaintenanceOptions::product_default()
                .with_worker_count(2)
                .with_scheduler_queue_depth(8),
            BackgroundExecutorMode::Threaded,
        );
        let normal_started = TestArc::new(Barrier::new(2));
        let normal_release = TestArc::new(Barrier::new(2));
        let high_ran = TestArc::new(TestAtomicBool::new(false));

        let normal_drain: BackgroundDrainFn = {
            let normal_started = TestArc::clone(&normal_started);
            let normal_release = TestArc::clone(&normal_release);
            TestArc::new(move |_limits, _clock| {
                normal_started.wait();
                normal_release.wait();
                BackgroundDrainRound {
                    tasks_completed: 1,
                    pending_tasks: 0,
                    made_progress: true,
                }
            })
        };
        controller.notify_drain(BackgroundTaskPriority::Normal, normal_drain);
        normal_started.wait();

        let high_drain: BackgroundDrainFn = {
            let high_ran = TestArc::clone(&high_ran);
            TestArc::new(move |_limits, _clock| {
                high_ran.store(true, TestOrdering::Release);
                BackgroundDrainRound {
                    tasks_completed: 1,
                    pending_tasks: 0,
                    made_progress: true,
                }
            })
        };
        controller.notify_drain(BackgroundTaskPriority::High, high_drain);

        let deadline = Instant::now() + Duration::from_secs(1);
        while !high_ran.load(TestOrdering::Acquire) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
        let ran_while_normal_active = high_ran.load(TestOrdering::Acquire);
        normal_release.wait();
        controller.shutdown(Some(Duration::from_secs(1)));

        assert!(
            ran_while_normal_active,
            "high-priority wake should be submitted while normal-priority drain is still active"
        );
    }

    #[test]
    fn pressure_wait_keeps_waiting_when_progress_leaves_pending_work() {
        let mut no_relief_rounds = usize::MAX;
        assert!(background_pressure_wait_should_continue_for_progress(
            true,
            1,
            false,
            &mut no_relief_rounds,
        ));
        assert_eq!(no_relief_rounds, 0);

        no_relief_rounds = usize::MAX;
        assert!(background_pressure_wait_should_continue_for_progress(
            false,
            0,
            true,
            &mut no_relief_rounds,
        ));
        assert_eq!(no_relief_rounds, 0);

        assert!(!background_pressure_wait_should_continue_for_progress(
            true,
            0,
            false,
            &mut no_relief_rounds,
        ));
        assert!(!background_pressure_wait_should_continue_for_progress(
            false,
            1,
            false,
            &mut no_relief_rounds,
        ));
    }

    fn throttle_observation(
        level_zero_tables: usize,
        pressure_units: usize,
        completed_tasks: u64,
    ) -> BackgroundAdmissionThrottleObservation {
        BackgroundAdmissionThrottleObservation {
            branch_id: DEFAULT_BRANCH_ID,
            reason: LifecycleStoragePressureReason::LevelZeroTableBacklog,
            severity: LifecycleStoragePressureSeverity::Urgent,
            pressure_units,
            pressure_snapshot: BackgroundPressureSnapshot {
                severity: LifecycleStoragePressureSeverity::Urgent,
                active_bytes: 0,
                frozen_tables: 0,
                frozen_bytes: 0,
                level_zero_tables,
                owned_tables: level_zero_tables,
                table_rewrite_bytes: 0,
                inherited_layers: 0,
            },
            completed_tasks,
        }
    }

    #[test]
    fn admission_throttle_treats_progress_as_relief_only_when_pressure_stops_worsening() {
        let mut throttle = BackgroundAdmissionThrottle::default();
        let base_slowdown = Duration::from_millis(1);

        let first = throttle.observe_urgent(
            throttle_observation(8, 32, 0),
            base_slowdown,
            MaintenanceInstant::from_elapsed(Duration::ZERO),
        );
        assert_eq!(first.slowdown, None);
        assert!(!first.no_relief_escalated);
        assert!(!first.relief_reset);

        let still_worse = throttle.observe_urgent(
            throttle_observation(10, 96, 1),
            base_slowdown,
            MaintenanceInstant::from_elapsed(Duration::from_millis(1)),
        );
        assert_eq!(still_worse.slowdown, None);
        assert!(!still_worse.no_relief_escalated);
        assert!(!still_worse.relief_reset);

        let escalated = throttle.observe_urgent(
            throttle_observation(11, 128, 2),
            base_slowdown,
            MaintenanceInstant::from_elapsed(Duration::from_millis(2)),
        );
        assert_eq!(escalated.slowdown, Some(base_slowdown));
        assert!(escalated.no_relief_escalated);
        assert!(!escalated.relief_reset);

        let stable_after_progress = throttle.observe_urgent(
            throttle_observation(11, 128, 3),
            base_slowdown,
            MaintenanceInstant::from_elapsed(Duration::from_millis(3)),
        );
        assert_eq!(stable_after_progress.slowdown, None);
        assert!(!stable_after_progress.no_relief_escalated);
        assert!(stable_after_progress.relief_reset);

        let improved_after_progress = throttle.observe_urgent(
            throttle_observation(10, 96, 4),
            base_slowdown,
            MaintenanceInstant::from_elapsed(Duration::from_millis(4)),
        );
        assert_eq!(improved_after_progress.slowdown, None);
        assert!(!improved_after_progress.no_relief_escalated);
        assert!(!improved_after_progress.relief_reset);
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ApiTimestampSource {
    next_timestamp: Timestamp,
}

impl ApiTimestampSource {
    const fn new(next_timestamp: Timestamp) -> Self {
        Self { next_timestamp }
    }
}

impl CommitTimestampSource for ApiTimestampSource {
    fn next_timestamp(&mut self) -> crate::commit::CommitRuntimeResult<Timestamp> {
        let timestamp = self.next_timestamp;
        self.next_timestamp = timestamp.saturating_add(Duration::from_micros(1));
        Ok(timestamp)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageCloseOptions {
    background_shutdown_timeout: Duration,
}

impl StorageCloseOptions {
    #[must_use]
    pub const fn graceful() -> Self {
        Self {
            background_shutdown_timeout: DEFAULT_BACKGROUND_CLOSE_SHUTDOWN_TIMEOUT,
        }
    }

    #[must_use]
    pub const fn with_background_shutdown_timeout(
        mut self,
        background_shutdown_timeout: Duration,
    ) -> Self {
        self.background_shutdown_timeout = background_shutdown_timeout;
        self
    }

    const fn background_shutdown_timeout(self) -> Duration {
        self.background_shutdown_timeout
    }
}

impl Default for StorageCloseOptions {
    fn default() -> Self {
        Self::graceful()
    }
}

impl StorageRuntime<'static> {
    #[must_use]
    pub const fn closed() -> Self {
        Self {
            inner: StorageRuntimeInner::Closed,
            open_summary: None,
            last_recovery: None,
            last_close: None,
        }
    }

    /// Open an explicit volatile runtime backed by in-memory cache storage.
    pub fn open_ephemeral() -> StorageApiResult<StorageOpenOutcome<'static>> {
        Self::open_cache()
    }

    /// Open a cache-mode runtime for cache-specific tests and demos.
    pub fn open_cache() -> StorageApiResult<StorageOpenOutcome<'static>> {
        Self::open(StorageOpenOptions::cache())
    }

    /// Open durable local storage at `root` with standard durability.
    ///
    /// This is the native product-facing open helper. It never falls back to
    /// cache mode; builds without the `localfs` feature return an explicit
    /// unsupported-capability error instead.
    pub fn open_local(
        root: impl Into<std::path::PathBuf>,
    ) -> StorageApiResult<StorageOpenOutcome<'static>> {
        Self::open_durable_local(root, StorageDurabilityPolicy::Standard)
    }

    /// Open durable local storage at `root` with an explicit durability policy.
    pub fn open_durable_local(
        root: impl Into<std::path::PathBuf>,
        policy: StorageDurabilityPolicy,
    ) -> StorageApiResult<StorageOpenOutcome<'static>> {
        open_durable_local_owned(root, policy)
    }

    pub fn open(options: StorageOpenOptions) -> StorageApiResult<StorageOpenOutcome<'static>> {
        options.validate()?;
        match options.mode() {
            StorageMode::Cache => {
                let backend = StorageBackend::memory();
                Self::open_cache_with_backend(options, &backend)
            }
            StorageMode::DurableLocal { .. } => Err(StorageApiError::InvalidArgument {
                field: "backend",
                reason: "durable local open requires an explicit storage backend handle",
            }),
            StorageMode::ObjectDurableCandidate | StorageMode::DistributedCandidate => {
                unreachable!("unsupported modes are rejected during validation")
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn submit_runtime_state_background_probe_for_test(
        &self,
        ready: std::sync::Arc<std::sync::Barrier>,
        release: std::sync::Arc<std::sync::Barrier>,
        observed_open: std::sync::Arc<std::sync::atomic::AtomicBool>,
    ) -> bool {
        match &self.inner {
            StorageRuntimeInner::Cache(slot) => slot
                .submit_background(BackgroundTaskPriority::High, move |runtime| {
                    ready.wait();
                    release.wait();
                    let runtime = runtime.lock();
                    observed_open.store(
                        runtime.state() == crate::lifecycle::LifecycleState::Open,
                        std::sync::atomic::Ordering::Release,
                    );
                })
                .is_ok(),
            StorageRuntimeInner::Durable(slot) => slot
                .submit_background(BackgroundTaskPriority::High, move |runtime| {
                    ready.wait();
                    release.wait();
                    let runtime = runtime.lock();
                    observed_open.store(
                        runtime.state() == crate::lifecycle::LifecycleState::Open,
                        std::sync::atomic::Ordering::Release,
                    );
                })
                .is_ok(),
            StorageRuntimeInner::DurableOwned(slot) => slot
                .submit_background(BackgroundTaskPriority::High, move |runtime| {
                    ready.wait();
                    release.wait();
                    let runtime = runtime.lock();
                    observed_open.store(
                        runtime.state() == crate::lifecycle::LifecycleState::Open,
                        std::sync::atomic::Ordering::Release,
                    );
                })
                .is_ok(),
            StorageRuntimeInner::Closed => false,
        }
    }

    #[cfg(test)]
    pub(crate) fn submit_panicking_background_task_for_test(
        &self,
        ready: std::sync::Arc<std::sync::Barrier>,
        release: std::sync::Arc<std::sync::Barrier>,
    ) -> bool {
        match &self.inner {
            StorageRuntimeInner::Cache(slot) => slot
                .submit_background(BackgroundTaskPriority::High, move |_runtime| {
                    ready.wait();
                    release.wait();
                    panic!("intentional background close panic test");
                })
                .is_ok(),
            StorageRuntimeInner::Durable(slot) => slot
                .submit_background(BackgroundTaskPriority::High, move |_runtime| {
                    ready.wait();
                    release.wait();
                    panic!("intentional background close panic test");
                })
                .is_ok(),
            StorageRuntimeInner::DurableOwned(slot) => slot
                .submit_background(BackgroundTaskPriority::High, move |_runtime| {
                    ready.wait();
                    release.wait();
                    panic!("intentional background close panic test");
                })
                .is_ok(),
            StorageRuntimeInner::Closed => false,
        }
    }

    #[cfg(test)]
    #[allow(
        clippy::match_same_arms,
        reason = "borrowed and owned durable slots have different concrete lifetimes"
    )]
    pub(crate) fn background_shutdown_requested_flag_for_test(
        &self,
    ) -> Option<std::sync::Arc<std::sync::atomic::AtomicBool>> {
        match &self.inner {
            StorageRuntimeInner::Cache(slot) => slot.background_shutdown_requested_flag(),
            StorageRuntimeInner::Durable(slot) => slot.background_shutdown_requested_flag(),
            StorageRuntimeInner::DurableOwned(slot) => slot.background_shutdown_requested_flag(),
            StorageRuntimeInner::Closed => None,
        }
    }

    #[cfg(test)]
    #[allow(
        clippy::match_same_arms,
        reason = "borrowed and owned durable slots have different concrete lifetimes"
    )]
    pub(crate) fn wait_background_idle_for_test(&self) {
        match &self.inner {
            StorageRuntimeInner::Cache(slot) => slot.wait_background_idle(),
            StorageRuntimeInner::Durable(slot) => slot.wait_background_idle(),
            StorageRuntimeInner::DurableOwned(slot) => slot.wait_background_idle(),
            StorageRuntimeInner::Closed => {}
        }
    }

    #[cfg(test)]
    #[allow(
        clippy::match_same_arms,
        reason = "borrowed and owned durable slots have different concrete lifetimes"
    )]
    pub(crate) fn wait_background_idle_until_for_test(
        &self,
        timeout: Duration,
    ) -> Option<MaintenanceExecutorStats> {
        match &self.inner {
            StorageRuntimeInner::Cache(slot) => slot.wait_background_idle_until(timeout),
            StorageRuntimeInner::Durable(slot) => slot.wait_background_idle_until(timeout),
            StorageRuntimeInner::DurableOwned(slot) => slot.wait_background_idle_until(timeout),
            StorageRuntimeInner::Closed => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn pending_lifecycle_maintenance_kinds_for_test(
        &self,
    ) -> Vec<LifecycleMaintenanceTaskKind> {
        match &self.inner {
            StorageRuntimeInner::Cache(_) | StorageRuntimeInner::Closed => Vec::new(),
            StorageRuntimeInner::Durable(slot) => slot.lock().pending_maintenance_kinds_for_test(),
            StorageRuntimeInner::DurableOwned(slot) => {
                slot.lock().pending_maintenance_kinds_for_test()
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn pending_flush_watermark_candidate_for_test(&self) -> Option<CommitVersion> {
        match &self.inner {
            StorageRuntimeInner::Cache(_) | StorageRuntimeInner::Closed => None,
            StorageRuntimeInner::Durable(slot) | StorageRuntimeInner::DurableOwned(slot) => {
                slot.lock().pending_flush_watermark_candidate_for_test()
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn background_now_for_test(&self) -> Option<MaintenanceInstant> {
        self.background_now_for_current_runtime()
    }

    #[cfg(test)]
    #[allow(
        clippy::match_same_arms,
        reason = "borrowed and owned durable slots have different concrete lifetimes"
    )]
    pub(crate) fn set_background_drain_limits_for_test(
        &mut self,
        max_tasks: usize,
        max_runtime: Duration,
    ) -> bool {
        match &mut self.inner {
            StorageRuntimeInner::Cache(slot) => {
                slot.set_background_drain_limits(max_tasks, max_runtime)
            }
            StorageRuntimeInner::Durable(slot) => {
                slot.set_background_drain_limits(max_tasks, max_runtime)
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                slot.set_background_drain_limits(max_tasks, max_runtime)
            }
            StorageRuntimeInner::Closed => false,
        }
    }

    #[cfg(test)]
    #[allow(
        clippy::match_same_arms,
        reason = "borrowed and owned durable slots have different concrete lifetimes"
    )]
    pub(crate) fn set_background_block_wait_for_test(
        &mut self,
        wait_slice: Duration,
        stall_deadline: Duration,
        no_relief_rounds: usize,
    ) -> bool {
        match &mut self.inner {
            StorageRuntimeInner::Cache(slot) => slot.set_background_block_wait_for_test(
                wait_slice,
                stall_deadline,
                no_relief_rounds,
            ),
            StorageRuntimeInner::Durable(slot) => slot.set_background_block_wait_for_test(
                wait_slice,
                stall_deadline,
                no_relief_rounds,
            ),
            StorageRuntimeInner::DurableOwned(slot) => slot.set_background_block_wait_for_test(
                wait_slice,
                stall_deadline,
                no_relief_rounds,
            ),
            StorageRuntimeInner::Closed => false,
        }
    }

    #[cfg(test)]
    #[allow(
        clippy::match_same_arms,
        reason = "borrowed and owned durable slots have different concrete lifetimes"
    )]
    pub(crate) fn shutdown_background_for_test(&self) {
        match &self.inner {
            StorageRuntimeInner::Cache(slot) => {
                let _ = slot.shutdown_background(Some(DEFAULT_BACKGROUND_CLOSE_SHUTDOWN_TIMEOUT));
            }
            StorageRuntimeInner::Durable(slot) => {
                let _ = slot.shutdown_background(Some(DEFAULT_BACKGROUND_CLOSE_SHUTDOWN_TIMEOUT));
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let _ = slot.shutdown_background(Some(DEFAULT_BACKGROUND_CLOSE_SHUTDOWN_TIMEOUT));
            }
            StorageRuntimeInner::Closed => {}
        }
    }

    #[cfg(test)]
    pub(crate) fn submit_stale_background_wake_for_test(&self) {
        self.notify_background_drain_for_current_runtime(BackgroundTaskPriority::Low);
    }

    #[cfg(test)]
    #[allow(
        clippy::match_same_arms,
        reason = "borrowed and owned durable slots have different concrete lifetimes"
    )]
    pub(crate) fn enqueue_lifecycle_maintenance_for_test(
        &mut self,
        task: LifecycleMaintenanceTaskRequest,
    ) -> StorageApiResult<MaintenanceQueueSummary> {
        self.require_open("maintenance enqueue requires an open runtime")?;
        match &mut self.inner {
            StorageRuntimeInner::Cache(slot) => {
                let status = {
                    let mut runtime = slot.lock();
                    runtime
                        .enqueue_maintenance(task)
                        .map_err(map_lifecycle_error)?;
                    runtime.maintenance_status()
                };
                slot.notify_background_drain(background_priority_for_task_request(task));
                Ok(map_maintenance_queue_summary(
                    status,
                    slot.background_stats(),
                ))
            }
            StorageRuntimeInner::Durable(slot) => {
                let status = {
                    let mut runtime = slot.lock();
                    runtime
                        .enqueue_maintenance(task)
                        .map_err(map_lifecycle_error)?;
                    runtime.maintenance_status()
                };
                slot.notify_background_drain(background_priority_for_task_request(task));
                Ok(map_maintenance_queue_summary(
                    status,
                    slot.background_stats(),
                ))
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let status = {
                    let mut runtime = slot.lock();
                    runtime
                        .enqueue_maintenance(task)
                        .map_err(map_lifecycle_error)?;
                    runtime.maintenance_status()
                };
                slot.notify_background_drain(background_priority_for_task_request(task));
                Ok(map_maintenance_queue_summary(
                    status,
                    slot.background_stats(),
                ))
            }
            StorageRuntimeInner::Closed => Err(StorageApiError::InvalidRuntimeState {
                reason: "maintenance enqueue requires an open runtime",
            }),
        }
    }
}

fn assemble_durable_runtime(
    options: StorageOpenOptions,
    backend: BackendHandle<'_>,
) -> StorageApiResult<(
    LifecycleDurableLocalRuntime<'_, ApiTimestampSource>,
    StorageOpenSummary,
    DiagnosticsRecoveryReport,
    LifecycleConfig,
)> {
    let plan = lifecycle_plan(options)?;
    let wal_config = wal_service_config(options)?;
    let request = LifecycleDurableLocalOpenRequest::new(
        plan,
        DEFAULT_DATABASE_ID,
        DEFAULT_BRANCH_ID,
        default_branch_generation()?,
        BranchRuntimeConfig::default(),
        CommitRuntimeConfig::default(),
        wal_config,
    )
    .map_err(map_lifecycle_error)?;
    let mut shell =
        LifecycleDurableLocalShell::assemble(request, backend, default_timestamp_source())
            .map_err(map_lifecycle_error)?;
    let recovery_request =
        crate::lifecycle::LifecycleRecoveryRequest::from_open_plan(shell.open_plan())
            .map_err(map_lifecycle_error)?;
    let recovery = LifecycleRecoveryRuntime::new(&mut shell)
        .recover(&recovery_request)
        .map_err(map_lifecycle_error)?;
    let runtime = shell
        .complete_recovery(&recovery)
        .map_err(map_lifecycle_error)?;
    let summary = map_open_summary(runtime.open_outcome(), options.mode(), options);
    let recovery_report = map_diagnostics_recovery(runtime.current_recovery_health());
    let config = runtime.open_plan().lifecycle_config();
    Ok((runtime, summary, recovery_report, config))
}

fn wal_service_config(options: StorageOpenOptions) -> StorageApiResult<WalServiceConfig> {
    let config = options
        .wal_segment_size_for_test()
        .map_or_else(WalServiceConfig::default, WalServiceConfig::new);
    config
        .validate()
        .map_err(|_| StorageApiError::InvalidArgument {
            field: "wal_segment_size",
            reason: "WAL segment size is invalid",
        })?;
    Ok(config)
}

fn open_durable_with_owned_backend_handle<'runtime>(
    options: StorageOpenOptions,
    backend: BackendHandle<'static>,
) -> StorageApiResult<StorageOpenOutcome<'runtime>> {
    let executor_mode = background_executor_mode(options.maintenance_scheduling_policy());
    let background_config = options.background_maintenance();
    let (runtime, summary, recovery_report, config) = assemble_durable_runtime(options, backend)?;
    Ok(StorageOpenOutcome::new(
        StorageRuntime {
            inner: StorageRuntimeInner::DurableOwned(Box::new(
                RuntimeSlot::new_with_background_arc_drain(
                    runtime,
                    config,
                    background_config,
                    executor_mode,
                    drain_durable_background_round,
                ),
            )),
            open_summary: Some(summary),
            last_recovery: Some(recovery_report),
            last_close: None,
        },
        summary,
    ))
}

#[cfg(feature = "localfs")]
fn open_durable_local_owned(
    root: impl Into<std::path::PathBuf>,
    policy: StorageDurabilityPolicy,
) -> StorageApiResult<StorageOpenOutcome<'static>> {
    let backend = StorageBackend::local_fs(root);
    open_durable_with_owned_backend_handle(
        StorageOpenOptions::durable_local(policy),
        backend.into_backend_handle(),
    )
}

#[cfg(not(feature = "localfs"))]
fn open_durable_local_owned(
    _root: impl Into<std::path::PathBuf>,
    _policy: StorageDurabilityPolicy,
) -> StorageApiResult<StorageOpenOutcome<'static>> {
    Err(StorageApiError::UnsupportedCapability {
        capability: "localfs",
        reason: "durable local storage requires the localfs feature",
    })
}

#[allow(
    clippy::match_same_arms,
    reason = "borrowed and owned durable runtime variants share behavior but carry different backend lifetimes"
)]
impl<'a> StorageRuntime<'a> {
    /// Open durable local storage with an explicit backend handle.
    ///
    /// The returned runtime borrows `backend`; keep the backend alive for at
    /// least as long as the runtime.
    pub fn open_durable_local_with_backend(
        policy: StorageDurabilityPolicy,
        backend: &'a StorageBackend,
    ) -> StorageApiResult<StorageOpenOutcome<'a>> {
        Self::open_with_backend(StorageOpenOptions::durable_local(policy), backend)
    }

    pub fn open_with_backend(
        options: StorageOpenOptions,
        backend: &'a StorageBackend,
    ) -> StorageApiResult<StorageOpenOutcome<'a>> {
        options.validate()?;
        match options.mode() {
            StorageMode::Cache => Self::open_cache_with_backend(options, backend),
            StorageMode::DurableLocal { .. } => {
                match durable_backend_handle_for_open(options, backend)? {
                    DurableBackendHandleForOpen::Borrowed(handle) => {
                        Self::open_durable_with_backend_handle(options, handle)
                    }
                    #[cfg(feature = "localfs")]
                    DurableBackendHandleForOpen::Owned(handle) => {
                        open_durable_with_owned_backend_handle(options, handle)
                    }
                }
            }
            StorageMode::ObjectDurableCandidate | StorageMode::DistributedCandidate => {
                unreachable!("unsupported modes are rejected during validation")
            }
        }
    }

    #[must_use]
    pub const fn state(&self) -> StorageRuntimeState {
        match self.inner {
            StorageRuntimeInner::Cache(_)
            | StorageRuntimeInner::Durable(_)
            | StorageRuntimeInner::DurableOwned(_) => StorageRuntimeState::Open,
            StorageRuntimeInner::Closed => StorageRuntimeState::Closed,
        }
    }

    #[must_use]
    pub const fn is_open(&self) -> bool {
        matches!(
            self.inner,
            StorageRuntimeInner::Cache(_)
                | StorageRuntimeInner::Durable(_)
                | StorageRuntimeInner::DurableOwned(_)
        )
    }

    pub fn close(&mut self) -> StorageApiResult<StorageCloseSummary> {
        self.close_with_options(StorageCloseOptions::graceful())
    }

    /// Closes the runtime using the supplied close policy.
    ///
    /// If a background worker panic is discovered during shutdown, this method
    /// returns that error and leaves the runtime open so callers can inspect the
    /// failure and retry close. Dropping the runtime after such an error only
    /// requests background shutdown; durable runtimes rely on recovery at the
    /// next open rather than a clean close summary.
    pub fn close_with_options(
        &mut self,
        options: StorageCloseOptions,
    ) -> StorageApiResult<StorageCloseSummary> {
        let background_shutdown_timeout = Some(options.background_shutdown_timeout());
        match &mut self.inner {
            StorageRuntimeInner::Cache(runtime) => {
                let background_shutdown = runtime.shutdown_background(background_shutdown_timeout);
                if let Some(error) = background_shutdown_panic_error(background_shutdown.as_ref()) {
                    return Err(error);
                }
                let mut runtime = runtime.lock();
                let maintenance_before_close = runtime.maintenance_status().stats();
                let recovery = map_diagnostics_recovery(runtime.open_outcome().recovery_health());
                let close = runtime.close().map_err(map_lifecycle_error)?;
                let maintenance_after_close = runtime.maintenance_status().stats();
                record_background_close_maintenance_facts(
                    maintenance_before_close,
                    maintenance_after_close,
                );
                let summary = with_background_close_facts(
                    map_close_summary(close, false),
                    background_shutdown.as_ref().map(|shutdown| &shutdown.stats),
                );
                drop(runtime);
                self.inner = StorageRuntimeInner::Closed;
                self.last_recovery = Some(recovery);
                self.last_close = Some(summary);
                Ok(summary)
            }
            StorageRuntimeInner::Durable(runtime) => {
                let background_shutdown = runtime.shutdown_background(background_shutdown_timeout);
                if let Some(error) = background_shutdown_panic_error(background_shutdown.as_ref()) {
                    return Err(error);
                }
                let mut runtime = runtime.lock();
                let maintenance_before_close = runtime.maintenance_status().stats();
                let close = runtime.close().map_err(map_lifecycle_error)?;
                let maintenance_after_close = runtime.maintenance_status().stats();
                record_background_close_maintenance_facts(
                    maintenance_before_close,
                    maintenance_after_close,
                );
                let recovery = map_diagnostics_recovery(runtime.current_recovery_health());
                let summary = with_background_close_facts(
                    map_close_summary(close, false),
                    background_shutdown.as_ref().map(|shutdown| &shutdown.stats),
                );
                drop(runtime);
                self.inner = StorageRuntimeInner::Closed;
                self.last_recovery = Some(recovery);
                self.last_close = Some(summary);
                Ok(summary)
            }
            StorageRuntimeInner::DurableOwned(runtime) => {
                let background_shutdown = runtime.shutdown_background(background_shutdown_timeout);
                if let Some(error) = background_shutdown_panic_error(background_shutdown.as_ref()) {
                    return Err(error);
                }
                let mut runtime = runtime.lock();
                let maintenance_before_close = runtime.maintenance_status().stats();
                let close = runtime.close().map_err(map_lifecycle_error)?;
                let maintenance_after_close = runtime.maintenance_status().stats();
                record_background_close_maintenance_facts(
                    maintenance_before_close,
                    maintenance_after_close,
                );
                let recovery = map_diagnostics_recovery(runtime.current_recovery_health());
                let summary = with_background_close_facts(
                    map_close_summary(close, false),
                    background_shutdown.as_ref().map(|shutdown| &shutdown.stats),
                );
                drop(runtime);
                self.inner = StorageRuntimeInner::Closed;
                self.last_recovery = Some(recovery);
                self.last_close = Some(summary);
                Ok(summary)
            }
            StorageRuntimeInner::Closed => Ok(self.last_close.map_or_else(
                || {
                    StorageCloseSummary::with_close_facts(
                        StorageRuntimeState::Closed,
                        true,
                        StorageCloseEffects::empty(),
                    )
                },
                |summary| summary.with_idempotent(true),
            )),
        }
    }

    pub fn require_open(&self, operation: &'static str) -> StorageApiResult<()> {
        if self.is_open() {
            Ok(())
        } else {
            Err(StorageApiError::InvalidRuntimeState { reason: operation })
        }
    }

    pub fn commit(&mut self, batch: &CommitBatch) -> StorageApiResult<CommitSummary> {
        self.execute_commit(batch, None)
    }

    pub fn branch(&mut self, request: &BranchRequest) -> StorageApiResult<BranchOutcome> {
        match request.action() {
            BranchAction::Create => self.create_branch_request(request),
            BranchAction::Describe => self.describe_branch_request(request),
            BranchAction::List => self.list_branch_request(),
            BranchAction::ForkCurrent { source } => {
                require_valid_branch_identifier(request.branch_id(), "branch_id")?;
                require_valid_branch_identifier(source, "source_branch_id")?;
                let version = self.current_branch_version(source)?;
                self.fork_branch_at_version(request, source, version, None)
            }
            BranchAction::ForkAtVersion { source, version } => {
                require_valid_branch_identifier(request.branch_id(), "branch_id")?;
                require_valid_branch_identifier(source, "source_branch_id")?;
                self.require_retained_version_watermark(source, version)?;
                self.fork_branch_at_version(request, source, version, None)
            }
            BranchAction::ForkAtTimestamp { source, timestamp } => {
                require_valid_branch_identifier(request.branch_id(), "branch_id")?;
                require_valid_branch_identifier(source, "source_branch_id")?;
                let timeline = self.timeline_view(source)?;
                let lookup = timeline.version_at_or_before(timestamp);
                let version = match lookup.miss() {
                    CommitTimelineMiss::Matched => lookup.matched_version().ok_or(
                        StorageApiError::RetainedHistoryUnavailable {
                            branch_id: source,
                            reason: "timestamp lookup did not return a retained version",
                        },
                    )?,
                    CommitTimelineMiss::BeforeRetainedHistory | CommitTimelineMiss::Empty => {
                        return Err(StorageApiError::TimestampHistoryUnavailable {
                            branch_id: source,
                            reason: "timestamp is outside retained timeline history",
                        });
                    }
                    CommitTimelineMiss::AfterLatestRetained => {
                        return Err(StorageApiError::TimestampHistoryUnavailable {
                            branch_id: source,
                            reason: "timestamp is newer than retained timeline history",
                        });
                    }
                };
                self.fork_branch_at_version(request, source, version, Some(timestamp))
            }
            BranchAction::Clear => self.clear_branch_request(request),
            BranchAction::Delete => self.delete_branch_request(request),
        }
    }

    pub fn maintenance(
        &mut self,
        request: &MaintenanceRequest,
    ) -> StorageApiResult<MaintenanceSummary> {
        self.require_open("maintenance requires an open runtime")?;
        validate_maintenance_request(request)?;
        match request.task() {
            MaintenanceTask::Checkpoint => self.checkpoint_maintenance(request),
            MaintenanceTask::Flush => self.flush_maintenance(request),
            MaintenanceTask::Compact => self.compaction_maintenance(request),
            MaintenanceTask::Materialize => self.materialization_maintenance(request),
            MaintenanceTask::Retain => self.retention_maintenance(request),
            MaintenanceTask::SnapshotPruning => self.snapshot_pruning_maintenance(request),
            MaintenanceTask::Reclaim => self.reclaim_maintenance(request),
            MaintenanceTask::Quarantine => self.quarantine_maintenance(request),
            MaintenanceTask::Purge => self.purge_maintenance(request),
            MaintenanceTask::Repair => self.repair_maintenance(request),
            MaintenanceTask::WalGrowth => self.wal_growth_maintenance(request),
        }
    }

    pub fn maintenance_status(&self) -> StorageApiResult<MaintenanceQueueSummary> {
        match &self.inner {
            StorageRuntimeInner::Cache(slot) => {
                let runtime = slot.lock();
                Ok(map_maintenance_queue_summary(
                    runtime.maintenance_status(),
                    slot.background_stats(),
                ))
            }
            StorageRuntimeInner::Durable(slot) => {
                let runtime = slot.lock();
                Ok(map_maintenance_queue_summary(
                    runtime.maintenance_status(),
                    slot.background_stats(),
                ))
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let runtime = slot.lock();
                Ok(map_maintenance_queue_summary(
                    runtime.maintenance_status(),
                    slot.background_stats(),
                ))
            }
            StorageRuntimeInner::Closed => Err(StorageApiError::InvalidRuntimeState {
                reason: "maintenance status requires an open runtime",
            }),
        }
    }

    pub fn diagnostics(&self, request: DiagnosticsRequest) -> StorageApiResult<DiagnosticsOutcome> {
        match &self.inner {
            StorageRuntimeInner::Cache(runtime) => self.cache_diagnostics(request, runtime),
            StorageRuntimeInner::Durable(runtime) => self.durable_diagnostics(request, runtime),
            StorageRuntimeInner::DurableOwned(runtime) => {
                self.durable_diagnostics(request, runtime)
            }
            StorageRuntimeInner::Closed => Ok(DiagnosticsOutcome::new(
                request.scope(),
                StorageRuntimeState::Closed,
                self.open_summary.map(StorageOpenSummary::mode),
                None,
                self.last_recovery
                    .clone()
                    .unwrap_or_else(DiagnosticsRecoveryReport::unknown),
                None,
                DiagnosticsBudgetReport::unknown(),
                DiagnosticsStoragePressureReport::unknown(),
                DiagnosticsSourceLayoutReport::unknown(),
                DiagnosticsReadActivityReport::unknown(),
                DiagnosticsTableReachabilityReport::unknown(),
                DiagnosticsRetentionReport::unknown(),
                DiagnosticsQuarantineReport::unknown(),
                DiagnosticsCheckpointReport::unknown(),
                DiagnosticsWalGrowthReport::unknown(),
                DiagnosticsBranchCatalogReport::unknown(),
                DiagnosticsTimelineReport::unknown(),
            )),
        }
    }

    fn cache_diagnostics<S>(
        &self,
        request: DiagnosticsRequest,
        slot: &RuntimeSlot<LifecycleCacheRuntime<S>>,
    ) -> StorageApiResult<DiagnosticsOutcome> {
        let branch_id = branch_for_diagnostics_scope(request.scope());
        let branches = self.list_branches(true)?;
        let visible = current_visible(self);
        let timeline = self.diagnostics_timeline(branch_id);
        let runtime = slot.lock();
        let wal_growth = runtime.evaluate_wal_growth_policy();
        Ok(DiagnosticsOutcome::new(
            request.scope(),
            StorageRuntimeState::Open,
            Some(diagnostics_mode_from_plan(
                self.open_summary,
                runtime.open_plan(),
            )),
            visible,
            map_diagnostics_recovery(runtime.open_outcome().recovery_health()),
            Some(map_maintenance_queue_summary(
                runtime.maintenance_status(),
                slot.background_stats(),
            )),
            map_budget_report(&runtime.budget_snapshot()),
            diagnostics_pressure_report(
                runtime.branch_catalog(),
                branch_id,
                runtime.maintenance_status(),
                runtime.open_plan().lifecycle_config().storage_budget(),
            ),
            diagnostics_source_layout_report(runtime.branch_catalog(), branch_id),
            DiagnosticsReadActivityReport::unknown(),
            DiagnosticsTableReachabilityReport::unsupported(),
            DiagnosticsRetentionReport::unsupported(),
            DiagnosticsQuarantineReport::unsupported(),
            DiagnosticsCheckpointReport::unsupported(),
            map_wal_growth_report(
                runtime.open_plan().lifecycle_config().wal_growth_policy(),
                Some(wal_growth.facts()),
                Some(map_wal_growth_summary(&wal_growth)),
            ),
            map_branch_catalog_report(&branches),
            timeline,
        ))
    }

    fn durable_diagnostics<S>(
        &self,
        request: DiagnosticsRequest,
        slot: &RuntimeSlot<LifecycleDurableLocalRuntime<'_, S>>,
    ) -> StorageApiResult<DiagnosticsOutcome> {
        let branch_id = branch_for_diagnostics_scope(request.scope());
        let branches = self.list_branches(true)?;
        let visible = current_visible(self);
        let timeline = self.diagnostics_timeline(branch_id);
        let runtime = slot.lock();
        let table_catalog = runtime.table_catalog();
        Ok(DiagnosticsOutcome::new(
            request.scope(),
            StorageRuntimeState::Open,
            Some(diagnostics_mode_from_plan(
                self.open_summary,
                runtime.open_plan(),
            )),
            visible,
            map_diagnostics_recovery(runtime.current_recovery_health()),
            Some(map_maintenance_queue_summary(
                runtime.maintenance_status(),
                slot.background_stats(),
            )),
            map_budget_report(&runtime.budget_snapshot()),
            diagnostics_pressure_report(
                runtime.branch_catalog(),
                branch_id,
                runtime.maintenance_status(),
                runtime.open_plan().lifecycle_config().storage_budget(),
            ),
            diagnostics_source_layout_report(runtime.branch_catalog(), branch_id),
            DiagnosticsReadActivityReport::unknown(),
            DiagnosticsTableReachabilityReport::known(
                table_catalog.entry_count(),
                table_catalog.object_count(),
                Some(table_catalog.next_manifest_sequence()),
            ),
            DiagnosticsRetentionReport::known(None, Some(runtime.pending_releases().len()), None),
            DiagnosticsQuarantineReport::unknown(),
            durable_checkpoint_report(&runtime),
            map_wal_growth_report(
                runtime.open_plan().lifecycle_config().wal_growth_policy(),
                runtime.current_wal_growth_facts().ok(),
                runtime
                    .last_wal_growth_outcome()
                    .map(map_wal_growth_summary),
            ),
            map_branch_catalog_report(&branches),
            timeline,
        ))
    }

    pub fn enqueue_maintenance(
        &mut self,
        request: &MaintenanceRequest,
    ) -> StorageApiResult<MaintenanceQueueSummary> {
        self.require_open("maintenance enqueue requires an open runtime")?;
        validate_maintenance_request(request)?;
        let task = map_maintenance_task_request(self, request)?;
        match &mut self.inner {
            StorageRuntimeInner::Cache(slot) => {
                let status = {
                    let mut runtime = slot.lock();
                    runtime
                        .enqueue_maintenance(task)
                        .map_err(map_lifecycle_error)?;
                    runtime.maintenance_status()
                };
                slot.notify_background_drain(background_priority_for_task_request(task));
                Ok(map_maintenance_queue_summary(
                    status,
                    slot.background_stats(),
                ))
            }
            StorageRuntimeInner::Durable(slot) => {
                let status = {
                    let mut runtime = slot.lock();
                    runtime
                        .enqueue_maintenance(task)
                        .map_err(map_lifecycle_error)?;
                    runtime.maintenance_status()
                };
                slot.notify_background_drain(background_priority_for_task_request(task));
                Ok(map_maintenance_queue_summary(
                    status,
                    slot.background_stats(),
                ))
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let status = {
                    let mut runtime = slot.lock();
                    runtime
                        .enqueue_maintenance(task)
                        .map_err(map_lifecycle_error)?;
                    runtime.maintenance_status()
                };
                slot.notify_background_drain(background_priority_for_task_request(task));
                Ok(map_maintenance_queue_summary(
                    status,
                    slot.background_stats(),
                ))
            }
            StorageRuntimeInner::Closed => Err(StorageApiError::InvalidRuntimeState {
                reason: "maintenance enqueue requires an open runtime",
            }),
        }
    }

    pub fn run_next_maintenance(&mut self) -> StorageApiResult<Option<MaintenanceSummary>> {
        let outcome = match &mut self.inner {
            StorageRuntimeInner::Cache(slot) => {
                let mut runtime = slot.lock();
                run_next_cache_maintenance(&mut runtime)?
            }
            StorageRuntimeInner::Durable(slot) => {
                let mut runtime = slot.lock();
                run_next_durable_maintenance(&mut runtime)?
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let mut runtime = slot.lock();
                run_next_durable_maintenance(&mut runtime)?
            }
            StorageRuntimeInner::Closed => {
                return Err(StorageApiError::InvalidRuntimeState {
                    reason: "maintenance run requires an open runtime",
                });
            }
        };
        Ok(outcome.map(|outcome| map_maintenance_summary(request_for_outcome(&outcome), &outcome)))
    }

    pub fn drain_maintenance(&mut self) -> StorageApiResult<MaintenanceDrainSummary> {
        self.require_open("maintenance drain requires an open runtime")?;
        let mut outcomes = Vec::new();
        while let Some(outcome) = self.run_next_maintenance()? {
            outcomes.push(outcome);
        }
        let queue = self.maintenance_status()?;
        let drained_tasks = outcomes.len();
        Ok(MaintenanceDrainSummary::new(drained_tasks, outcomes, queue))
    }

    pub fn read_point(&self, request: &PointReadRequest) -> StorageApiResult<PointReadOutcome> {
        if request.bound() == ReadBound::Latest {
            return self.read_latest_point(request);
        }
        let view = self.read_view_for_branch(request.branch_id())?;
        let key = physical_key(request.branch_id(), request.storage_space(), request.key())?;
        let resolved = resolve_read_bound(&view, request.bound())?;
        let row = match view
            .read_point(&key, resolved.branch_bound)
            .map_err(branch_error)?
        {
            Some(row) => read_row_from_storage_if_visible(row.row(), resolved.selected_timestamp)?,
            None => visible_tombstone_at_bound(&view, &key, resolved)?,
        };
        Ok(PointReadOutcome::new(row))
    }

    fn read_latest_point(&self, request: &PointReadRequest) -> StorageApiResult<PointReadOutcome> {
        let key = physical_key(request.branch_id(), request.storage_space(), request.key())?;
        let row = match &self.inner {
            StorageRuntimeInner::Cache(slot) => {
                let runtime = slot.lock();
                runtime
                    .read_latest_point_or_tombstone_for_branch(request.branch_id(), &key)
                    .map_err(map_lifecycle_error)?
            }
            StorageRuntimeInner::Durable(slot) => {
                let runtime = slot.lock();
                runtime
                    .read_latest_point_or_tombstone_for_branch(request.branch_id(), &key)
                    .map_err(map_lifecycle_error)?
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let runtime = slot.lock();
                runtime
                    .read_latest_point_or_tombstone_for_branch(request.branch_id(), &key)
                    .map_err(map_lifecycle_error)?
            }
            StorageRuntimeInner::Closed => {
                return Err(StorageApiError::InvalidRuntimeState {
                    reason: "read requires an open runtime",
                });
            }
        };
        let row = row
            .as_ref()
            .map(|row| read_row_from_storage(row.row()))
            .transpose()?;
        Ok(PointReadOutcome::new(row))
    }

    pub fn read_history(
        &self,
        request: &HistoryReadRequest,
    ) -> StorageApiResult<HistoryReadOutcome> {
        let view = self.read_view_for_branch(request.branch_id())?;
        let key = physical_key(request.branch_id(), request.storage_space(), request.key())?;
        if let Some(version) = request.before_version_bound() {
            require_version_retained(&view, version)?;
        }
        let mut options =
            BranchHistoryOptions::all().include_tombstones(request.includes_tombstones());
        if let Some(version) = request.before_version_bound() {
            options = options.before_version(version);
        }
        if let Some(limit) = request.limit_bound() {
            options = options.limit(limit.get());
        }
        let rows = view
            .history(&key, options)
            .map_err(branch_error)?
            .iter()
            .map(|row| read_row_from_storage(row.row()))
            .collect::<StorageApiResult<Vec<_>>>()?;
        Ok(HistoryReadOutcome::new(rows))
    }

    pub fn scan_prefix(
        &self,
        request: &PrefixScanReadRequest,
    ) -> StorageApiResult<ScanReadOutcome> {
        let prefix = physical_key(
            request.branch_id(),
            request.storage_space(),
            request.prefix(),
        )?;
        if matches!(request.bound(), ReadBound::Latest) {
            let bounds = BranchScanBounds::prefix(&prefix);
            let scan_timer = perf_trace::start_timer();
            let rows = self.scan_latest_including_tombstones_for_branch(
                request.branch_id(),
                &bounds,
                request.limit().map(ReadLimit::get),
            )?;
            perf_trace::record_api_scan_runtime_elapsed(scan_timer);
            let map_timer = perf_trace::start_timer();
            let outcome = map_scan_rows(
                rows.iter().map(crate::branch::read::BranchHistoryRow::row),
                request.limit(),
                None,
            );
            perf_trace::record_api_scan_map_elapsed(map_timer);
            return outcome;
        }

        let view = self.read_view_for_branch(request.branch_id())?;
        let resolved = resolve_read_bound(&view, request.bound())?;
        let bounds = BranchScanBounds::prefix(&prefix);
        let rows = view
            .scan_prefix_including_tombstones(&bounds, resolved.branch_bound)
            .map_err(branch_error)?;
        map_scan_rows(
            rows.iter().map(crate::branch::read::BranchHistoryRow::row),
            request.limit(),
            resolved.selected_timestamp,
        )
    }

    pub fn scan_range(&self, request: &ScanReadRequest) -> StorageApiResult<ScanReadOutcome> {
        let bounds_timer = perf_trace::start_timer();
        let storage_space = map_storage_space(request.storage_space())?;
        let bounds = BranchScanBounds::range(
            request.branch_id(),
            API_PHYSICAL_SPACE,
            storage_space,
            request
                .range()
                .start()
                .map_or(BranchUserKeyBound::Unbounded, |key| {
                    BranchUserKeyBound::included(key.as_bytes())
                }),
            request
                .range()
                .end()
                .map_or(BranchUserKeyBound::Unbounded, |key| {
                    BranchUserKeyBound::excluded(key.as_bytes())
                }),
        )
        .map_err(branch_error)?;
        perf_trace::record_api_scan_bounds_elapsed(bounds_timer);
        if matches!(request.bound(), ReadBound::Latest) {
            let scan_timer = perf_trace::start_timer();
            let rows = self.scan_latest_including_tombstones_for_branch(
                request.branch_id(),
                &bounds,
                request.limit().map(ReadLimit::get),
            )?;
            perf_trace::record_api_scan_runtime_elapsed(scan_timer);
            let map_timer = perf_trace::start_timer();
            let outcome = map_scan_rows(
                rows.iter().map(crate::branch::read::BranchHistoryRow::row),
                request.limit(),
                None,
            );
            perf_trace::record_api_scan_map_elapsed(map_timer);
            return outcome;
        }

        let view = self.read_view_for_branch(request.branch_id())?;
        let resolved = resolve_read_bound(&view, request.bound())?;
        let rows = view
            .scan_range_including_tombstones(&bounds, resolved.branch_bound)
            .map_err(branch_error)?;
        map_scan_rows(
            rows.iter().map(crate::branch::read::BranchHistoryRow::row),
            request.limit(),
            resolved.selected_timestamp,
        )
    }

    pub fn lookup_version_at_or_before_timestamp(
        &self,
        request: TimestampLookupRequest,
    ) -> StorageApiResult<TimestampLookupOutcome> {
        let timeline = self.timeline_view(request.branch_id())?;
        let lookup = timeline.version_at_or_before(request.timestamp());
        match lookup.miss() {
            CommitTimelineMiss::Matched | CommitTimelineMiss::AfterLatestRetained => {
                let matched_version = lookup.matched_version().ok_or(
                    StorageApiError::RetainedHistoryUnavailable {
                        branch_id: request.branch_id(),
                        reason: "timeline lookup did not return a retained version",
                    },
                )?;
                let matched_timestamp = lookup.matched_timestamp().ok_or(
                    StorageApiError::RetainedHistoryUnavailable {
                        branch_id: request.branch_id(),
                        reason: "timeline lookup did not return a retained timestamp",
                    },
                )?;
                Ok(TimestampLookupOutcome::new(
                    lookup.query_timestamp(),
                    matched_version,
                    matched_timestamp,
                    (lookup.miss() == CommitTimelineMiss::AfterLatestRetained)
                        .then_some(TimestampLookupMiss::AfterLatestRetained),
                ))
            }
            CommitTimelineMiss::BeforeRetainedHistory | CommitTimelineMiss::Empty => {
                Err(StorageApiError::TimestampHistoryUnavailable {
                    branch_id: request.branch_id(),
                    reason: "timestamp is outside retained timeline history",
                })
            }
        }
    }

    pub fn lookup_timestamp_for_version(
        &self,
        request: VersionLookupRequest,
    ) -> StorageApiResult<VersionLookupOutcome> {
        let timeline = self.timeline_view(request.branch_id())?;
        let timestamp = timeline.timestamp_for_version(request.version()).ok_or(
            StorageApiError::RetainedHistoryUnavailable {
                branch_id: request.branch_id(),
                reason: "commit version is outside retained timeline history",
            },
        )?;
        Ok(VersionLookupOutcome::new(request.version(), timestamp))
    }

    pub fn timeline_bounds(
        &self,
        request: TimelineBoundsRequest,
    ) -> StorageApiResult<TimelineBoundsOutcome> {
        let bounds = self.timeline_view(request.branch_id())?.bounds();
        Ok(TimelineBoundsOutcome::new(
            bounds.min_timestamp(),
            bounds.max_timestamp(),
            bounds.min_version(),
            bounds.max_version(),
        ))
    }

    fn checkpoint_maintenance(
        &mut self,
        request: &MaintenanceRequest,
    ) -> StorageApiResult<MaintenanceSummary> {
        let branch_id = self.branch_for_maintenance_scope(request.scope())?;
        match &mut self.inner {
            StorageRuntimeInner::Cache(_) => Ok(unsupported_maintenance_summary(
                request,
                "cache runtime does not support durable checkpoint maintenance",
            )),
            StorageRuntimeInner::Durable(slot) => {
                let mut runtime = slot.lock();
                let outcome = runtime
                    .checkpoint_for_explicit_maintenance(branch_id, false)
                    .map_err(map_lifecycle_error)?;
                Ok(map_checkpoint_summary(request, &outcome))
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let mut runtime = slot.lock();
                let outcome = runtime
                    .checkpoint_for_explicit_maintenance(branch_id, false)
                    .map_err(map_lifecycle_error)?;
                Ok(map_checkpoint_summary(request, &outcome))
            }
            StorageRuntimeInner::Closed => Err(StorageApiError::InvalidRuntimeState {
                reason: "checkpoint maintenance requires an open runtime",
            }),
        }
    }

    fn flush_maintenance(
        &mut self,
        request: &MaintenanceRequest,
    ) -> StorageApiResult<MaintenanceSummary> {
        let branch_id = self.branch_for_maintenance_scope(request.scope())?;
        let flush_request = flush_request_for_boundary(branch_id)?;
        let outcome = match &mut self.inner {
            StorageRuntimeInner::Cache(slot) => {
                let mut runtime = slot.lock();
                runtime
                    .rotate_active_for_branch_for_maintenance(branch_id)
                    .map_err(map_lifecycle_error)?;
                runtime.flush_frozen(&flush_request)
            }
            StorageRuntimeInner::Durable(slot) => {
                let mut runtime = slot.lock();
                runtime
                    .rotate_active_for_branch_for_maintenance(branch_id)
                    .map_err(map_lifecycle_error)?;
                runtime.flush_frozen(&flush_request)
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let mut runtime = slot.lock();
                runtime
                    .rotate_active_for_branch_for_maintenance(branch_id)
                    .map_err(map_lifecycle_error)?;
                runtime.flush_frozen(&flush_request)
            }
            StorageRuntimeInner::Closed => {
                return Err(StorageApiError::InvalidRuntimeState {
                    reason: "flush maintenance requires an open runtime",
                });
            }
        }
        .map_err(map_lifecycle_error)?;
        self.run_flush_followup_compaction(branch_id)?;
        Ok(
            map_maintenance_summary(*request, &outcome.maintenance_outcome())
                .with_rows_processed(outcome.rows_flushed()),
        )
    }

    fn run_flush_followup_compaction(&mut self, branch_id: BranchId) -> StorageApiResult<()> {
        if !self.storage_pressure_suggests_compaction(branch_id)? {
            return Ok(());
        }
        let task = LifecycleMaintenanceTaskRequest::compaction(branch_id, 0);
        let outcome = match &mut self.inner {
            StorageRuntimeInner::Cache(slot) => {
                let mut runtime = slot.lock();
                let enqueue = runtime
                    .enqueue_maintenance(task)
                    .map_err(map_lifecycle_error)?;
                runtime
                    .run_compaction_maintenance_task(enqueue.task_id())
                    .map_err(map_lifecycle_error)?
            }
            StorageRuntimeInner::Durable(slot) => {
                let mut runtime = slot.lock();
                let enqueue = runtime
                    .enqueue_maintenance(task)
                    .map_err(map_lifecycle_error)?;
                runtime
                    .run_compaction_maintenance_task(enqueue.task_id())
                    .map_err(map_lifecycle_error)?
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let mut runtime = slot.lock();
                let enqueue = runtime
                    .enqueue_maintenance(task)
                    .map_err(map_lifecycle_error)?;
                runtime
                    .run_compaction_maintenance_task(enqueue.task_id())
                    .map_err(map_lifecycle_error)?
            }
            StorageRuntimeInner::Closed => {
                return Err(StorageApiError::InvalidRuntimeState {
                    reason: "flush follow-up compaction requires an open runtime",
                });
            }
        }
        .ok_or(StorageApiError::InvalidRuntimeState {
            reason: "flush follow-up compaction task was not runnable",
        })?;
        match outcome.status() {
            LifecycleMaintenanceOutcomeStatus::Completed
            | LifecycleMaintenanceOutcomeStatus::Deferred => Ok(()),
            LifecycleMaintenanceOutcomeStatus::Failed
            | LifecycleMaintenanceOutcomeStatus::Canceled => {
                Err(StorageApiError::InvalidRuntimeState {
                    reason: "flush follow-up compaction did not complete",
                })
            }
        }
    }

    fn storage_pressure_suggests_compaction(&self, branch_id: BranchId) -> StorageApiResult<bool> {
        let suggested_task = match &self.inner {
            StorageRuntimeInner::Cache(slot) => {
                let runtime = slot.lock();
                runtime.storage_pressure().suggested_task()
            }
            StorageRuntimeInner::Durable(slot) => {
                let runtime = slot.lock();
                runtime.storage_pressure().suggested_task()
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let runtime = slot.lock();
                runtime.storage_pressure().suggested_task()
            }
            StorageRuntimeInner::Closed => {
                return Err(StorageApiError::InvalidRuntimeState {
                    reason: "flush follow-up compaction requires an open runtime",
                });
            }
        };
        Ok(suggested_task.is_some_and(|task| {
            task.kind() == LifecycleMaintenanceTaskKind::Compaction
                && matches!(
                    task.scope(),
                    LifecycleMaintenanceTaskScope::TableLevel {
                        branch_id: task_branch_id,
                        level: 0,
                    } if task_branch_id == branch_id
                )
        }))
    }

    fn compaction_maintenance(
        &mut self,
        request: &MaintenanceRequest,
    ) -> StorageApiResult<MaintenanceSummary> {
        let branch_id = self.branch_for_maintenance_scope(request.scope())?;
        let compaction = LifecycleCompactionDrainRequest::new(
            branch_id,
            format!("storage-boundary-compaction-{branch_id}"),
        )
        .map_err(map_lifecycle_error)?;
        let outcome = match &mut self.inner {
            StorageRuntimeInner::Cache(slot) => {
                let mut runtime = slot.lock();
                runtime.compact_branch_tables_to_fixed_point(&compaction)
            }
            StorageRuntimeInner::Durable(slot) => {
                let mut runtime = slot.lock();
                runtime.compact_branch_tables_to_fixed_point(&compaction)
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let mut runtime = slot.lock();
                runtime.compact_branch_tables_to_fixed_point(&compaction)
            }
            StorageRuntimeInner::Closed => {
                return Err(StorageApiError::InvalidRuntimeState {
                    reason: "compaction maintenance requires an open runtime",
                });
            }
        }
        .map_err(map_lifecycle_error)?;
        Ok(map_maintenance_summary(
            *request,
            &outcome.maintenance_outcome(),
        ))
    }

    #[cfg(test)]
    pub(crate) fn branch_source_layout_for_test(
        &self,
        branch_id: BranchId,
    ) -> StorageApiResult<crate::branch::facts::BranchSourceLayout> {
        match &self.inner {
            StorageRuntimeInner::Cache(slot) => {
                let runtime = slot.lock();
                Ok(runtime
                    .branch_catalog()
                    .branch_state(branch_id)
                    .map_err(map_lifecycle_error)?
                    .source_layout())
            }
            StorageRuntimeInner::Durable(slot) => {
                let runtime = slot.lock();
                Ok(runtime
                    .branch_catalog()
                    .branch_state(branch_id)
                    .map_err(map_lifecycle_error)?
                    .source_layout())
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let runtime = slot.lock();
                Ok(runtime
                    .branch_catalog()
                    .branch_state(branch_id)
                    .map_err(map_lifecycle_error)?
                    .source_layout())
            }
            StorageRuntimeInner::Closed => Err(StorageApiError::InvalidRuntimeState {
                reason: "source layout requires an open runtime",
            }),
        }
    }

    fn materialization_maintenance(
        &mut self,
        request: &MaintenanceRequest,
    ) -> StorageApiResult<MaintenanceSummary> {
        let branch_id = self.branch_for_maintenance_scope(request.scope())?;
        let task = LifecycleMaintenanceTaskRequest::materialization(branch_id);
        let outcome = match &mut self.inner {
            StorageRuntimeInner::Cache(slot) => {
                let mut runtime = slot.lock();
                let enqueue = runtime
                    .enqueue_maintenance(task)
                    .map_err(map_lifecycle_error)?;
                runtime
                    .run_materialization_maintenance_task(enqueue.task_id())
                    .map_err(map_lifecycle_error)?
            }
            StorageRuntimeInner::Durable(slot) => {
                let mut runtime = slot.lock();
                let enqueue = runtime
                    .enqueue_maintenance(task)
                    .map_err(map_lifecycle_error)?;
                runtime
                    .run_materialization_maintenance_task(enqueue.task_id())
                    .map_err(map_lifecycle_error)?
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let mut runtime = slot.lock();
                let enqueue = runtime
                    .enqueue_maintenance(task)
                    .map_err(map_lifecycle_error)?;
                runtime
                    .run_materialization_maintenance_task(enqueue.task_id())
                    .map_err(map_lifecycle_error)?
            }
            StorageRuntimeInner::Closed => {
                return Err(StorageApiError::InvalidRuntimeState {
                    reason: "materialization maintenance requires an open runtime",
                });
            }
        };
        outcome.map_or_else(
            || {
                Ok(unsupported_maintenance_summary(
                    request,
                    "materialization maintenance was deferred",
                ))
            },
            |outcome| Ok(map_maintenance_summary(*request, &outcome)),
        )
    }

    fn retention_maintenance(
        &mut self,
        request: &MaintenanceRequest,
    ) -> StorageApiResult<MaintenanceSummary> {
        match &mut self.inner {
            StorageRuntimeInner::Cache(_) => Ok(unsupported_maintenance_summary(
                request,
                "cache runtime does not support durable retention maintenance",
            )),
            StorageRuntimeInner::Durable(slot) => {
                let mut runtime = slot.lock();
                let outcome = runtime
                    .prove_retention(&LifecycleRetentionRequest::global(1))
                    .map_err(map_lifecycle_error)?;
                Ok(map_maintenance_summary(
                    *request,
                    &outcome.maintenance_outcome(),
                ))
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let mut runtime = slot.lock();
                let outcome = runtime
                    .prove_retention(&LifecycleRetentionRequest::global(1))
                    .map_err(map_lifecycle_error)?;
                Ok(map_maintenance_summary(
                    *request,
                    &outcome.maintenance_outcome(),
                ))
            }
            StorageRuntimeInner::Closed => Err(StorageApiError::InvalidRuntimeState {
                reason: "retention maintenance requires an open runtime",
            }),
        }
    }

    fn snapshot_pruning_maintenance(
        &mut self,
        request: &MaintenanceRequest,
    ) -> StorageApiResult<MaintenanceSummary> {
        match &mut self.inner {
            StorageRuntimeInner::Cache(_) => Ok(unsupported_maintenance_summary(
                request,
                "cache runtime does not support durable snapshot pruning maintenance",
            )),
            StorageRuntimeInner::Durable(slot) => {
                let mut runtime = slot.lock();
                let retention = LifecycleRetentionRequest::snapshot_pruning(1);
                let outcome = runtime
                    .prune_snapshots(&retention)
                    .map_err(map_lifecycle_error)?;
                Ok(map_maintenance_summary(
                    *request,
                    &outcome.maintenance_outcome(),
                ))
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let mut runtime = slot.lock();
                let retention = LifecycleRetentionRequest::snapshot_pruning(1);
                let outcome = runtime
                    .prune_snapshots(&retention)
                    .map_err(map_lifecycle_error)?;
                Ok(map_maintenance_summary(
                    *request,
                    &outcome.maintenance_outcome(),
                ))
            }
            StorageRuntimeInner::Closed => Err(StorageApiError::InvalidRuntimeState {
                reason: "snapshot pruning maintenance requires an open runtime",
            }),
        }
    }

    fn reclaim_maintenance(
        &mut self,
        request: &MaintenanceRequest,
    ) -> StorageApiResult<MaintenanceSummary> {
        let branch_id = self.branch_for_maintenance_scope(request.scope())?;
        match &mut self.inner {
            StorageRuntimeInner::Cache(_) => Ok(unsupported_maintenance_summary(
                request,
                "cache runtime does not support durable reclaim maintenance",
            )),
            StorageRuntimeInner::Durable(slot) => {
                let mut runtime = slot.lock();
                let retention = LifecycleRetentionRequest::new(
                    LifecycleRetentionScope::TableObjects { branch_id },
                    1,
                );
                let outcome = runtime
                    .prove_retention(&retention)
                    .map_err(map_lifecycle_error)?;
                Ok(map_maintenance_summary(
                    *request,
                    &outcome.maintenance_outcome(),
                ))
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let mut runtime = slot.lock();
                let retention = LifecycleRetentionRequest::new(
                    LifecycleRetentionScope::TableObjects { branch_id },
                    1,
                );
                let outcome = runtime
                    .prove_retention(&retention)
                    .map_err(map_lifecycle_error)?;
                Ok(map_maintenance_summary(
                    *request,
                    &outcome.maintenance_outcome(),
                ))
            }
            StorageRuntimeInner::Closed => Err(StorageApiError::InvalidRuntimeState {
                reason: "reclaim maintenance requires an open runtime",
            }),
        }
    }

    fn quarantine_maintenance(
        &mut self,
        request: &MaintenanceRequest,
    ) -> StorageApiResult<MaintenanceSummary> {
        let task = map_maintenance_task_request(self, request)?;
        match &mut self.inner {
            StorageRuntimeInner::Cache(_) => Ok(unsupported_maintenance_summary(
                request,
                "cache runtime does not support durable quarantine maintenance",
            )),
            StorageRuntimeInner::Durable(slot) => {
                let mut runtime = slot.lock();
                let enqueue = runtime
                    .enqueue_maintenance(task)
                    .map_err(map_lifecycle_error)?;
                let outcome = runtime
                    .run_quarantine_maintenance_task(enqueue.task_id())
                    .map_err(map_lifecycle_error)?;
                Ok(outcome.map_or_else(
                    || {
                        unsupported_maintenance_summary(
                            request,
                            "quarantine maintenance was deferred",
                        )
                    },
                    |outcome| map_maintenance_summary(*request, &outcome),
                ))
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let mut runtime = slot.lock();
                let enqueue = runtime
                    .enqueue_maintenance(task)
                    .map_err(map_lifecycle_error)?;
                let outcome = runtime
                    .run_quarantine_maintenance_task(enqueue.task_id())
                    .map_err(map_lifecycle_error)?;
                Ok(outcome.map_or_else(
                    || {
                        unsupported_maintenance_summary(
                            request,
                            "quarantine maintenance was deferred",
                        )
                    },
                    |outcome| map_maintenance_summary(*request, &outcome),
                ))
            }
            StorageRuntimeInner::Closed => Err(StorageApiError::InvalidRuntimeState {
                reason: "quarantine maintenance requires an open runtime",
            }),
        }
    }

    fn purge_maintenance(
        &mut self,
        request: &MaintenanceRequest,
    ) -> StorageApiResult<MaintenanceSummary> {
        let task = map_maintenance_task_request(self, request)?;
        match &mut self.inner {
            StorageRuntimeInner::Cache(_) => Ok(unsupported_maintenance_summary(
                request,
                "cache runtime does not support durable purge maintenance",
            )),
            StorageRuntimeInner::Durable(slot) => {
                let mut runtime = slot.lock();
                let enqueue = runtime
                    .enqueue_maintenance(task)
                    .map_err(map_lifecycle_error)?;
                let outcome = runtime
                    .run_purge_maintenance_task(enqueue.task_id())
                    .map_err(map_lifecycle_error)?;
                Ok(outcome.map_or_else(
                    || unsupported_maintenance_summary(request, "purge maintenance was deferred"),
                    |outcome| map_maintenance_summary(*request, &outcome),
                ))
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let mut runtime = slot.lock();
                let enqueue = runtime
                    .enqueue_maintenance(task)
                    .map_err(map_lifecycle_error)?;
                let outcome = runtime
                    .run_purge_maintenance_task(enqueue.task_id())
                    .map_err(map_lifecycle_error)?;
                Ok(outcome.map_or_else(
                    || unsupported_maintenance_summary(request, "purge maintenance was deferred"),
                    |outcome| map_maintenance_summary(*request, &outcome),
                ))
            }
            StorageRuntimeInner::Closed => Err(StorageApiError::InvalidRuntimeState {
                reason: "purge maintenance requires an open runtime",
            }),
        }
    }

    fn repair_maintenance(
        &mut self,
        request: &MaintenanceRequest,
    ) -> StorageApiResult<MaintenanceSummary> {
        let task = map_maintenance_task_request(self, request)?;
        match &mut self.inner {
            StorageRuntimeInner::Cache(_) => Ok(unsupported_maintenance_summary(
                request,
                "cache runtime does not support quarantine repair maintenance",
            )),
            StorageRuntimeInner::Durable(slot) => {
                let mut runtime = slot.lock();
                let enqueue = runtime
                    .enqueue_maintenance(task)
                    .map_err(map_lifecycle_error)?;
                let outcome = runtime
                    .run_quarantine_repair_maintenance_task(enqueue.task_id())
                    .map_err(map_lifecycle_error)?;
                Ok(outcome.map_or_else(
                    || unsupported_maintenance_summary(request, "repair maintenance was deferred"),
                    |outcome| map_maintenance_summary(*request, &outcome),
                ))
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let mut runtime = slot.lock();
                let enqueue = runtime
                    .enqueue_maintenance(task)
                    .map_err(map_lifecycle_error)?;
                let outcome = runtime
                    .run_quarantine_repair_maintenance_task(enqueue.task_id())
                    .map_err(map_lifecycle_error)?;
                Ok(outcome.map_or_else(
                    || unsupported_maintenance_summary(request, "repair maintenance was deferred"),
                    |outcome| map_maintenance_summary(*request, &outcome),
                ))
            }
            StorageRuntimeInner::Closed => Err(StorageApiError::InvalidRuntimeState {
                reason: "repair maintenance requires an open runtime",
            }),
        }
    }

    fn wal_growth_maintenance(
        &mut self,
        request: &MaintenanceRequest,
    ) -> StorageApiResult<MaintenanceSummary> {
        let outcome = match &mut self.inner {
            StorageRuntimeInner::Cache(slot) => {
                let runtime = slot.lock();
                Ok(runtime.evaluate_wal_growth_policy())
            }
            StorageRuntimeInner::Durable(slot) => {
                let mut runtime = slot.lock();
                runtime.evaluate_wal_growth_policy()
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let mut runtime = slot.lock();
                runtime.evaluate_wal_growth_policy()
            }
            StorageRuntimeInner::Closed => {
                return Err(StorageApiError::InvalidRuntimeState {
                    reason: "WAL growth maintenance requires an open runtime",
                });
            }
        }
        .map_err(map_lifecycle_error)?;
        if matches!(
            outcome.status(),
            LifecycleWalGrowthStatus::MaintenanceEnqueued
                | LifecycleWalGrowthStatus::MaintenanceCoalesced
        ) {
            self.notify_background_drain_for_current_runtime(BackgroundTaskPriority::High);
        }
        Ok(map_wal_growth_maintenance_summary(*request, &outcome))
    }

    fn branch_for_maintenance_scope(&self, scope: MaintenanceScope) -> StorageApiResult<BranchId> {
        match scope {
            MaintenanceScope::Global => Ok(DEFAULT_BRANCH_ID),
            MaintenanceScope::Branch(branch_id) => {
                require_valid_branch_identifier(branch_id, "branch_id")?;
                self.describe_branch(branch_id).map(|_| branch_id)
            }
        }
    }

    fn create_branch(
        &mut self,
        branch_id: BranchId,
        generation: CommitBranchGeneration,
        created_at: Option<CommitVersion>,
    ) -> StorageApiResult<crate::lifecycle::LifecycleBranchCreateOutcome> {
        match &mut self.inner {
            StorageRuntimeInner::Cache(slot) => {
                let mut runtime = slot.lock();
                runtime.create_branch(branch_id, generation, created_at)
            }
            StorageRuntimeInner::Durable(slot) => {
                let mut runtime = slot.lock();
                runtime.create_branch(branch_id, generation, created_at)
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let mut runtime = slot.lock();
                runtime.create_branch(branch_id, generation, created_at)
            }
            StorageRuntimeInner::Closed => {
                return Err(StorageApiError::InvalidRuntimeState {
                    reason: "branch operation requires an open runtime",
                });
            }
        }
        .map_err(map_lifecycle_error)
    }

    fn create_branch_request(
        &mut self,
        request: &BranchRequest,
    ) -> StorageApiResult<BranchOutcome> {
        require_valid_branch_identifier(request.branch_id(), "branch_id")?;
        let generation_before = self.recreate_generation_before(request.branch_id())?;
        let generation = branch_generation_or_default(request.expected_generation())?;
        let created_at = current_visible(self);
        let outcome = self.create_branch(request.branch_id(), generation, created_at)?;
        let branch = map_branch_descriptor(outcome.descriptor());
        Ok(BranchOutcome::new(BranchOperation::Created, vec![branch])
            .with_generations(generation_before, Some(branch.generation())))
    }

    fn describe_branch_request(&self, request: &BranchRequest) -> StorageApiResult<BranchOutcome> {
        require_valid_branch_identifier(request.branch_id(), "branch_id")?;
        let branch = self.describe_branch(request.branch_id())?;
        Ok(BranchOutcome::new(BranchOperation::Described, vec![branch])
            .with_generations(Some(branch.generation()), Some(branch.generation())))
    }

    fn list_branch_request(&self) -> StorageApiResult<BranchOutcome> {
        let branches = self.list_branches(false)?;
        Ok(BranchOutcome::new(BranchOperation::Listed, branches))
    }

    fn list_branches(&self, include_deleted: bool) -> StorageApiResult<Vec<BranchSummary>> {
        let descriptors = match &self.inner {
            StorageRuntimeInner::Cache(slot) => {
                let runtime = slot.lock();
                runtime.list_branches(include_deleted)
            }
            StorageRuntimeInner::Durable(slot) => {
                let runtime = slot.lock();
                runtime.list_branches(include_deleted)
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let runtime = slot.lock();
                runtime.list_branches(include_deleted)
            }
            StorageRuntimeInner::Closed => {
                return Err(StorageApiError::InvalidRuntimeState {
                    reason: "branch operation requires an open runtime",
                });
            }
        };
        Ok(descriptors.into_iter().map(map_branch_descriptor).collect())
    }

    fn describe_branch(&self, branch_id: BranchId) -> StorageApiResult<BranchSummary> {
        let descriptor = match &self.inner {
            StorageRuntimeInner::Cache(slot) => {
                let runtime = slot.lock();
                runtime.branch_catalog().lookup(branch_id)
            }
            StorageRuntimeInner::Durable(slot) => {
                let runtime = slot.lock();
                runtime.branch_catalog().lookup(branch_id)
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let runtime = slot.lock();
                runtime.branch_catalog().lookup(branch_id)
            }
            StorageRuntimeInner::Closed => {
                return Err(StorageApiError::InvalidRuntimeState {
                    reason: "branch operation requires an open runtime",
                });
            }
        }
        .map_err(map_lifecycle_error)?;
        Ok(map_branch_descriptor(descriptor))
    }

    fn active_branch_count(&self) -> StorageApiResult<usize> {
        Ok(self.list_branches(false)?.len())
    }

    fn retained_floor(&self, branch_id: BranchId) -> StorageApiResult<CommitVersion> {
        self.timeline_view(branch_id)?.bounds().min_version().ok_or(
            StorageApiError::RetainedHistoryUnavailable {
                branch_id,
                reason: "branch has no retained commit history",
            },
        )
    }

    fn current_branch_version(&self, branch_id: BranchId) -> StorageApiResult<CommitVersion> {
        self.timeline_view(branch_id)?.bounds().max_version().ok_or(
            StorageApiError::RetainedHistoryUnavailable {
                branch_id,
                reason: "branch has no retained commit history",
            },
        )
    }

    fn require_retained_version_watermark(
        &self,
        branch_id: BranchId,
        version: CommitVersion,
    ) -> StorageApiResult<()> {
        if version == CommitVersion::ZERO {
            return Err(StorageApiError::RetainedHistoryUnavailable {
                branch_id,
                reason: "commit version is outside retained branch history",
            });
        }
        let bounds = self.timeline_view(branch_id)?.bounds();
        let Some(min_version) = bounds.min_version() else {
            return Err(StorageApiError::RetainedHistoryUnavailable {
                branch_id,
                reason: "branch has no retained commit history",
            });
        };
        let Some(max_version) = bounds.max_version() else {
            return Err(StorageApiError::RetainedHistoryUnavailable {
                branch_id,
                reason: "branch has no retained commit history",
            });
        };
        if version < min_version || version > max_version {
            return Err(StorageApiError::RetainedHistoryUnavailable {
                branch_id,
                reason: "commit version is outside retained branch history",
            });
        }
        Ok(())
    }

    fn recreate_generation_before(
        &self,
        branch_id: BranchId,
    ) -> StorageApiResult<Option<BranchGeneration>> {
        match self.describe_branch(branch_id) {
            Ok(branch) if branch.status() == BranchStatus::Deleted => Ok(Some(branch.generation())),
            Ok(_) | Err(StorageApiError::BranchNotFound { .. }) => Ok(None),
            Err(error) => Err(error),
        }
    }

    fn fork_branch_at_version(
        &mut self,
        request: &BranchRequest,
        source: BranchId,
        version: CommitVersion,
        timestamp: Option<Timestamp>,
    ) -> StorageApiResult<BranchOutcome> {
        let generation = branch_generation_or_default(request.expected_generation())?;
        let retained_floor = self.retained_floor(source)?;
        let outcome = match &mut self.inner {
            StorageRuntimeInner::Cache(slot) => {
                let mut runtime = slot.lock();
                runtime.fork_at_retained_version(
                    source,
                    request.branch_id(),
                    generation,
                    version,
                    retained_floor,
                )
            }
            StorageRuntimeInner::Durable(slot) => {
                let mut runtime = slot.lock();
                runtime.fork_at_retained_version(
                    source,
                    request.branch_id(),
                    generation,
                    version,
                    retained_floor,
                )
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let mut runtime = slot.lock();
                runtime.fork_at_retained_version(
                    source,
                    request.branch_id(),
                    generation,
                    version,
                    retained_floor,
                )
            }
            StorageRuntimeInner::Closed => {
                return Err(StorageApiError::InvalidRuntimeState {
                    reason: "branch operation requires an open runtime",
                });
            }
        }
        .map_err(map_lifecycle_error)?;
        let branch = map_branch_descriptor(outcome.descriptor());
        Ok(BranchOutcome::new(BranchOperation::Forked, vec![branch])
            .with_generations(None, Some(branch.generation()))
            .with_fork_facts(
                outcome.source_branch_id(),
                outcome.fork_version(),
                timestamp,
            ))
    }

    fn clear_branch_request(&mut self, request: &BranchRequest) -> StorageApiResult<BranchOutcome> {
        require_valid_branch_identifier(request.branch_id(), "branch_id")?;
        let before = self.describe_branch(request.branch_id())?;
        let guard = map_generation_guard(request.expected_generation())?;
        let outcome = match &mut self.inner {
            StorageRuntimeInner::Cache(slot) => {
                let mut runtime = slot.lock();
                runtime.clear_branch(request.branch_id(), guard)
            }
            StorageRuntimeInner::Durable(slot) => {
                let mut runtime = slot.lock();
                runtime.clear_branch(request.branch_id(), guard)
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let mut runtime = slot.lock();
                runtime.clear_branch(request.branch_id(), guard)
            }
            StorageRuntimeInner::Closed => {
                return Err(StorageApiError::InvalidRuntimeState {
                    reason: "branch operation requires an open runtime",
                });
            }
        }
        .map_err(map_lifecycle_error)?;
        let branch = map_branch_descriptor(outcome.descriptor());
        Ok(BranchOutcome::new(BranchOperation::Cleared, vec![branch])
            .with_generations(Some(before.generation()), Some(branch.generation()))
            .with_cleanup(map_branch_cleanup(outcome.release_plan())))
    }

    fn delete_branch_request(
        &mut self,
        request: &BranchRequest,
    ) -> StorageApiResult<BranchOutcome> {
        require_valid_branch_identifier(request.branch_id(), "branch_id")?;
        let before = self.describe_branch(request.branch_id())?;
        if before.status() == BranchStatus::Active && self.active_branch_count()? <= 1 {
            return Err(StorageApiError::InvalidRuntimeState {
                reason: "delete would remove the last active branch",
            });
        }
        let guard = map_generation_guard(request.expected_generation())?;
        let deleted_at = current_visible(self);
        let outcome = match &mut self.inner {
            StorageRuntimeInner::Cache(slot) => {
                let mut runtime = slot.lock();
                runtime.delete_branch(request.branch_id(), guard, deleted_at)
            }
            StorageRuntimeInner::Durable(slot) => {
                let mut runtime = slot.lock();
                runtime.delete_branch(request.branch_id(), guard, deleted_at)
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let mut runtime = slot.lock();
                runtime.delete_branch(request.branch_id(), guard, deleted_at)
            }
            StorageRuntimeInner::Closed => {
                return Err(StorageApiError::InvalidRuntimeState {
                    reason: "branch operation requires an open runtime",
                });
            }
        }
        .map_err(map_lifecycle_error)?;
        let branch = map_branch_descriptor(outcome.descriptor());
        Ok(BranchOutcome::new(BranchOperation::Deleted, vec![branch])
            .with_generations(Some(before.generation()), Some(branch.generation()))
            .with_cleanup(map_branch_cleanup(outcome.release_plan())))
    }

    fn open_cache_with_backend(
        options: StorageOpenOptions,
        backend: &StorageBackend,
    ) -> StorageApiResult<StorageOpenOutcome<'a>> {
        let executor_mode = background_executor_mode(options.maintenance_scheduling_policy());
        let background_config = options.background_maintenance();
        let request = LifecycleCacheOpenRequest::new(
            lifecycle_plan(options)?,
            DEFAULT_BRANCH_ID,
            default_branch_generation()?,
        )
        .map_err(map_lifecycle_error)?;
        let runtime = LifecycleCacheRuntime::open(
            request,
            backend.as_backend(),
            BranchRuntimeConfig::default(),
            CommitRuntimeConfig::default(),
            default_timestamp_source(),
        )
        .map_err(map_lifecycle_error)?;
        let summary = map_open_summary(runtime.open_outcome(), options.mode(), options);
        let recovery = map_diagnostics_recovery(runtime.open_outcome().recovery_health());
        let config = runtime.open_plan().lifecycle_config();
        Ok(StorageOpenOutcome::new(
            Self {
                inner: StorageRuntimeInner::Cache(Box::new(
                    RuntimeSlot::new_with_background_arc_drain(
                        runtime,
                        config,
                        background_config,
                        executor_mode,
                        drain_cache_background_round,
                    ),
                )),
                open_summary: Some(summary),
                last_recovery: Some(recovery),
                last_close: None,
            },
            summary,
        ))
    }

    fn open_durable_with_backend_handle(
        options: StorageOpenOptions,
        backend: BackendHandle<'a>,
    ) -> StorageApiResult<StorageOpenOutcome<'a>> {
        let (runtime, summary, recovery_report, config) =
            assemble_durable_runtime(options, backend)?;
        Ok(StorageOpenOutcome::new(
            Self {
                inner: StorageRuntimeInner::Durable(Box::new(RuntimeSlot::new(runtime, config))),
                open_summary: Some(summary),
                last_recovery: Some(recovery_report),
                last_close: None,
            },
            summary,
        ))
    }

    #[cfg(all(test, feature = "localfs"))]
    pub(crate) fn release_writer_guard_for_test(&mut self) -> bool {
        match &mut self.inner {
            StorageRuntimeInner::Durable(slot) => {
                let mut runtime = slot.lock();
                runtime.release_writer_guard_for_test()
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let mut runtime = slot.lock();
                runtime.release_writer_guard_for_test()
            }
            StorageRuntimeInner::Cache(_) | StorageRuntimeInner::Closed => false,
        }
    }

    fn read_view_for_branch(&self, branch_id: BranchId) -> StorageApiResult<BranchReadView> {
        match &self.inner {
            StorageRuntimeInner::Cache(slot) => {
                let runtime = slot.lock();
                runtime
                    .read_view_for_branch(branch_id)
                    .map_err(map_lifecycle_error)
            }
            StorageRuntimeInner::Durable(slot) => {
                let runtime = slot.lock();
                runtime
                    .read_view_for_branch(branch_id)
                    .map_err(map_lifecycle_error)
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let runtime = slot.lock();
                runtime
                    .read_view_for_branch(branch_id)
                    .map_err(map_lifecycle_error)
            }
            StorageRuntimeInner::Closed => Err(StorageApiError::InvalidRuntimeState {
                reason: "read requires an open runtime",
            }),
        }
    }

    fn scan_latest_including_tombstones_for_branch(
        &self,
        branch_id: BranchId,
        bounds: &BranchScanBounds,
        visible_limit: Option<usize>,
    ) -> StorageApiResult<Vec<crate::branch::read::BranchHistoryRow>> {
        match &self.inner {
            StorageRuntimeInner::Cache(slot) => {
                let runtime = slot.lock();
                runtime
                    .scan_latest_including_tombstones_for_branch(branch_id, bounds, visible_limit)
                    .map_err(map_lifecycle_error)
            }
            StorageRuntimeInner::Durable(slot) => {
                let runtime = slot.lock();
                runtime
                    .scan_latest_including_tombstones_for_branch(branch_id, bounds, visible_limit)
                    .map_err(map_lifecycle_error)
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let runtime = slot.lock();
                runtime
                    .scan_latest_including_tombstones_for_branch(branch_id, bounds, visible_limit)
                    .map_err(map_lifecycle_error)
            }
            StorageRuntimeInner::Closed => Err(StorageApiError::InvalidRuntimeState {
                reason: "read requires an open runtime",
            }),
        }
    }

    fn timeline_view(&self, branch_id: BranchId) -> StorageApiResult<CommitTimelineView> {
        let view = self.read_view_for_branch(branch_id)?;
        let bounds = BranchScanBounds::unbounded(
            branch_id,
            COMMIT_TIMELINE_SPACE,
            RowStorageSpaceId::COMMIT_TIMELINE,
        )
        .map_err(branch_error)?;
        let timeline_rows = view
            .scan_range_including_tombstones(&bounds, BranchReadBound::Latest)
            .map_err(branch_error)?;
        CommitTimelineView::from_rows(
            branch_id,
            timeline_rows
                .iter()
                .map(crate::branch::read::BranchHistoryRow::row),
        )
        .map_err(commit_error)
    }

    fn diagnostics_timeline(&self, branch_id: BranchId) -> DiagnosticsTimelineReport {
        match self.timeline_view(branch_id) {
            Ok(timeline) => {
                let bounds = timeline.bounds();
                DiagnosticsTimelineReport::known(
                    bounds.min_version(),
                    bounds.max_version(),
                    bounds.min_timestamp(),
                    bounds.max_timestamp(),
                )
            }
            Err(_) => DiagnosticsTimelineReport::unknown(),
        }
    }

    #[cfg(test)]
    pub(crate) const fn default_branch_id_for_test() -> BranchId {
        DEFAULT_BRANCH_ID
    }

    #[cfg(test)]
    pub(crate) fn maintenance_scheduling_policy_for_test(
        &self,
    ) -> LifecycleMaintenanceSchedulingPolicy {
        match &self.inner {
            StorageRuntimeInner::Cache(slot) => {
                let runtime = slot.lock();
                runtime
                    .open_plan()
                    .lifecycle_config()
                    .maintenance_scheduling_policy()
            }
            StorageRuntimeInner::Durable(slot) => {
                let runtime = slot.lock();
                runtime
                    .open_plan()
                    .lifecycle_config()
                    .maintenance_scheduling_policy()
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let runtime = slot.lock();
                runtime
                    .open_plan()
                    .lifecycle_config()
                    .maintenance_scheduling_policy()
            }
            StorageRuntimeInner::Closed => LifecycleMaintenanceSchedulingPolicy::Disabled,
        }
    }

    #[cfg(any(test, feature = "testkit"))]
    pub(crate) fn commit_for_test(
        &mut self,
        batch: &CommitBatch,
        timestamp: Timestamp,
    ) -> StorageApiResult<CommitSummary> {
        self.execute_commit(batch, Some(timestamp))
    }

    #[cfg(test)]
    pub(crate) fn diagnostics_recovery_report_for_test(
        health: &RecoveryHealth,
    ) -> DiagnosticsRecoveryReport {
        map_diagnostics_recovery(health)
    }

    #[cfg(test)]
    #[cfg_attr(
        not(feature = "localfs"),
        expect(
            dead_code,
            reason = "durable recovery health hook is exercised by localfs diagnostics tests"
        )
    )]
    pub(crate) fn record_recovery_health_for_test(
        &mut self,
        health: &RecoveryHealth,
    ) -> StorageApiResult<()> {
        match &mut self.inner {
            StorageRuntimeInner::Durable(slot) => {
                let mut runtime = slot.lock();
                runtime.record_recovery_health_for_test(health);
                self.last_recovery = Some(map_diagnostics_recovery(health));
                Ok(())
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let mut runtime = slot.lock();
                runtime.record_recovery_health_for_test(health);
                self.last_recovery = Some(map_diagnostics_recovery(health));
                Ok(())
            }
            StorageRuntimeInner::Cache(_) | StorageRuntimeInner::Closed => {
                Err(StorageApiError::InvalidRuntimeState {
                    reason: "durable recovery health test hook requires an open durable runtime",
                })
            }
        }
    }

    fn execute_commit(
        &mut self,
        batch: &CommitBatch,
        explicit_timestamp: Option<Timestamp>,
    ) -> StorageApiResult<CommitSummary> {
        let timestamp_base = explicit_timestamp.unwrap_or_else(|| self.next_commit_timestamp());
        // The API computes the timestamp before mapping TTL so lower commit
        // stamping and expiry facts use the same monotonic frontier.
        let timestamp_policy = crate::commit::CommitTimestampPolicy::Explicit(timestamp_base);
        let durability = self.resolve_commit_durability(batch.options().durability())?;
        let generation_guard = map_generation_guard(batch.options().expected_generation())?;
        let map_timer = perf_trace::start_timer();
        let runtime_batch_result =
            map_api_commit_batch(batch, timestamp_base, timestamp_policy, durability);
        perf_trace::record_api_commit_map_elapsed(map_timer);
        let runtime_batch = runtime_batch_result?;

        let runtime_timer = perf_trace::start_timer();
        let mut pressure_wait_deadline = None;
        let mut pressure_no_relief_rounds = 0;
        loop {
            let (outcome_result, admission, pending_tasks, wal_growth) = match &mut self.inner {
                StorageRuntimeInner::Cache(slot) => {
                    let mut runtime = slot.lock();
                    let result =
                        runtime.execute_cache_commit(runtime_batch.clone(), generation_guard);
                    (
                        result,
                        runtime.last_write_admission(),
                        runtime.maintenance_status().pending_tasks(),
                        None,
                    )
                }
                StorageRuntimeInner::Durable(slot) => {
                    let mut runtime = slot.lock();
                    let result =
                        runtime.execute_durable_commit(runtime_batch.clone(), generation_guard);
                    (
                        result,
                        runtime.last_write_admission(),
                        runtime.maintenance_status().pending_tasks(),
                        runtime.last_wal_growth_outcome().cloned(),
                    )
                }
                StorageRuntimeInner::DurableOwned(slot) => {
                    let mut runtime = slot.lock();
                    let result =
                        runtime.execute_durable_commit(runtime_batch.clone(), generation_guard);
                    (
                        result,
                        runtime.last_write_admission(),
                        runtime.maintenance_status().pending_tasks(),
                        runtime.last_wal_growth_outcome().cloned(),
                    )
                }
                StorageRuntimeInner::Closed => {
                    return Err(StorageApiError::InvalidRuntimeState {
                        reason: "commit requires an open runtime",
                    });
                }
            };
            if pending_tasks > 0 {
                self.notify_background_drain_for_current_runtime(BackgroundTaskPriority::High);
            }
            match outcome_result {
                Ok(outcome) => {
                    if let Some(slowdown) = self.background_admission_slowdown(admission) {
                        self.sleep_background_duration(slowdown);
                        perf_trace::record_lifecycle_write_admission_slowdown(slowdown);
                    }
                    self.background_wait_after_wal_growth_enqueue(wal_growth.as_ref());
                    perf_trace::record_api_commit_runtime_elapsed(runtime_timer);
                    return map_commit_summary(&outcome, admission);
                }
                Err(error)
                    if self.background_wait_after_pressure_rejection(
                        &error,
                        &mut pressure_wait_deadline,
                        &mut pressure_no_relief_rounds,
                    ) => {}
                Err(error) => {
                    perf_trace::record_api_commit_runtime_elapsed(runtime_timer);
                    return Err(map_lifecycle_error(error));
                }
            }
        }
    }

    fn notify_background_drain_for_current_runtime(&self, priority: BackgroundTaskPriority) {
        match &self.inner {
            StorageRuntimeInner::Cache(slot) => {
                slot.notify_background_drain(priority);
            }
            StorageRuntimeInner::Durable(slot) => {
                slot.notify_background_drain(priority);
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                slot.notify_background_drain(priority);
            }
            StorageRuntimeInner::Closed => {}
        }
    }

    fn background_admission_slowdown(
        &self,
        admission: Option<LifecycleWriteAdmissionOutcome>,
    ) -> Option<Duration> {
        if !self.has_background_runtime() {
            return None;
        }
        let admission = admission?;
        if admission.status() != LifecycleWriteAdmissionStatus::AcceptedUnderPressure
            || admission.pressure().severity() != LifecycleStoragePressureSeverity::Urgent
            || admission.inline_maintenance_driven()
        {
            if self.reset_background_admission_throttle_for_current_runtime() {
                perf_trace::record_lifecycle_write_admission_throttle_relief_reset();
            }
            return None;
        }
        let pressure = admission.pressure();
        self.notify_background_drain_for_current_runtime(BackgroundTaskPriority::High);
        let queue_depth = self
            .background_stats_for_current_runtime()
            .map_or(0, |stats| {
                stats.queue_depth.saturating_add(stats.active_tasks)
            });
        let pressure_units = 1usize
            .saturating_add(pressure.pending_maintenance())
            .saturating_add(pressure.frozen_tables())
            .saturating_add(background_pressure_byte_units(
                pressure
                    .active_bytes()
                    .saturating_add(pressure.frozen_bytes())
                    .saturating_add(pressure.table_rewrite_bytes()),
            ))
            .saturating_add(background_level_zero_pressure_units(
                pressure.reason(),
                pressure.level_zero_tables(),
            ))
            .saturating_add(pressure.owned_tables() / 8)
            .saturating_add(queue_depth);
        let completed_tasks = self.background_lifecycle_completed_for_current_runtime();
        let decision = self.background_admission_slowdown_for_current_runtime(
            pressure,
            pressure_units,
            completed_tasks,
        )?;
        if decision.relief_reset {
            perf_trace::record_lifecycle_write_admission_throttle_relief_reset();
        }
        if decision.no_relief_escalated {
            perf_trace::record_lifecycle_write_admission_throttle_no_relief_escalation();
        }
        decision.slowdown
    }

    fn has_background_runtime(&self) -> bool {
        match &self.inner {
            StorageRuntimeInner::Cache(slot) => slot.has_background(),
            StorageRuntimeInner::Durable(slot) => slot.has_background(),
            StorageRuntimeInner::DurableOwned(slot) => slot.has_background(),
            StorageRuntimeInner::Closed => false,
        }
    }

    fn background_stats_for_current_runtime(&self) -> Option<MaintenanceExecutorStats> {
        match &self.inner {
            StorageRuntimeInner::Cache(slot) => slot.background_stats(),
            StorageRuntimeInner::Durable(slot) => slot.background_stats(),
            StorageRuntimeInner::DurableOwned(slot) => slot.background_stats(),
            StorageRuntimeInner::Closed => None,
        }
    }

    fn background_admission_slowdown_for_current_runtime(
        &self,
        pressure: LifecycleStoragePressure,
        pressure_units: usize,
        completed_tasks: u64,
    ) -> Option<BackgroundAdmissionThrottleDecision> {
        match &self.inner {
            StorageRuntimeInner::Cache(slot) => slot.background_admission_slowdown_duration(
                pressure,
                pressure_units,
                completed_tasks,
            ),
            StorageRuntimeInner::Durable(slot) => slot.background_admission_slowdown_duration(
                pressure,
                pressure_units,
                completed_tasks,
            ),
            StorageRuntimeInner::DurableOwned(slot) => slot.background_admission_slowdown_duration(
                pressure,
                pressure_units,
                completed_tasks,
            ),
            StorageRuntimeInner::Closed => None,
        }
    }

    fn reset_background_admission_throttle_for_current_runtime(&self) -> bool {
        match &self.inner {
            StorageRuntimeInner::Cache(slot) => slot.reset_background_admission_throttle(),
            StorageRuntimeInner::Durable(slot) => slot.reset_background_admission_throttle(),
            StorageRuntimeInner::DurableOwned(slot) => slot.reset_background_admission_throttle(),
            StorageRuntimeInner::Closed => false,
        }
    }

    #[cfg(test)]
    fn background_block_wait_for_current_runtime(&self) -> BackgroundBlockWaitConfig {
        match &self.inner {
            StorageRuntimeInner::Cache(slot) => slot.background_block_wait,
            StorageRuntimeInner::Durable(slot) => slot.background_block_wait,
            StorageRuntimeInner::DurableOwned(slot) => slot.background_block_wait,
            StorageRuntimeInner::Closed => BackgroundBlockWaitConfig::default(),
        }
    }

    fn background_pressure_snapshot_for_branch(
        &self,
        branch_id: BranchId,
    ) -> Option<BackgroundPressureSnapshot> {
        match &self.inner {
            StorageRuntimeInner::Cache(slot) => Some(BackgroundPressureSnapshot::from_pressure(
                slot.lock().storage_pressure_for_branch(branch_id),
            )),
            StorageRuntimeInner::Durable(slot) => Some(BackgroundPressureSnapshot::from_pressure(
                slot.lock().storage_pressure_for_branch(branch_id),
            )),
            StorageRuntimeInner::DurableOwned(slot) => {
                Some(BackgroundPressureSnapshot::from_pressure(
                    slot.lock().storage_pressure_for_branch(branch_id),
                ))
            }
            StorageRuntimeInner::Closed => None,
        }
    }

    fn background_lifecycle_work_for_current_runtime(&self) -> Option<(usize, bool)> {
        match &self.inner {
            StorageRuntimeInner::Cache(slot) => {
                let status = slot.lock().maintenance_status();
                Some((status.pending_tasks(), status.active_tasks() > 0))
            }
            StorageRuntimeInner::Durable(slot) => {
                let status = slot.lock().maintenance_status();
                Some((status.pending_tasks(), status.active_tasks() > 0))
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let status = slot.lock().maintenance_status();
                Some((status.pending_tasks(), status.active_tasks() > 0))
            }
            StorageRuntimeInner::Closed => None,
        }
    }

    fn background_lifecycle_completed_for_current_runtime(&self) -> u64 {
        match &self.inner {
            StorageRuntimeInner::Cache(slot) => {
                u64::try_from(slot.lock().maintenance_status().stats().completed())
                    .unwrap_or(u64::MAX)
            }
            StorageRuntimeInner::Durable(slot) => {
                u64::try_from(slot.lock().maintenance_status().stats().completed())
                    .unwrap_or(u64::MAX)
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                u64::try_from(slot.lock().maintenance_status().stats().completed())
                    .unwrap_or(u64::MAX)
            }
            StorageRuntimeInner::Closed => 0,
        }
    }

    fn background_now_for_current_runtime(&self) -> Option<MaintenanceInstant> {
        match &self.inner {
            StorageRuntimeInner::Cache(slot) => slot.background_now(),
            StorageRuntimeInner::Durable(slot) => slot.background_now(),
            StorageRuntimeInner::DurableOwned(slot) => slot.background_now(),
            StorageRuntimeInner::Closed => None,
        }
    }

    fn sleep_background_duration(&self, duration: Duration) {
        match &self.inner {
            StorageRuntimeInner::Cache(slot) => slot.sleep_background_duration(duration),
            StorageRuntimeInner::Durable(slot) => slot.sleep_background_duration(duration),
            StorageRuntimeInner::DurableOwned(slot) => slot.sleep_background_duration(duration),
            StorageRuntimeInner::Closed => {}
        }
    }

    fn background_wait_after_pressure_rejection(
        &mut self,
        error: &LifecycleError,
        deadline: &mut Option<MaintenanceInstant>,
        no_relief_rounds: &mut usize,
    ) -> bool {
        let LifecycleError::StoragePressureRejected {
            branch_id,
            pressure_reason,
            retryable: true,
            ..
        } = error
        else {
            return false;
        };
        if !self.has_background_runtime() {
            return false;
        }
        #[cfg(test)]
        let block_wait = self.background_block_wait_for_current_runtime();
        #[cfg(not(test))]
        let block_wait = BackgroundBlockWaitConfig::default();
        let Some(now) = self.background_now_for_current_runtime() else {
            return false;
        };
        perf_trace::record_lifecycle_write_admission_wait_attempt();
        let stall_deadline =
            *deadline.get_or_insert_with(|| now.saturating_add(block_wait.stall_deadline));
        if now >= stall_deadline {
            perf_trace::record_lifecycle_write_admission_wait_timeout();
            return false;
        }
        let wait_deadline = now
            .saturating_add(block_wait.wait_slice)
            .min(stall_deadline);
        let pending_tasks =
            self.enqueue_pressure_maintenance_for_background_wait(*branch_id, *pressure_reason);
        let Some(stats_before_wait) = self.background_stats_for_current_runtime() else {
            return false;
        };
        if pending_tasks
            .saturating_add(stats_before_wait.queue_depth)
            .saturating_add(stats_before_wait.active_tasks)
            == 0
        {
            return false;
        }
        let pressure_before_wait = self.background_pressure_snapshot_for_branch(*branch_id);
        let completed_before_wait = stats_before_wait.tasks_completed;
        let wait_start = self.background_now_for_current_runtime().unwrap_or(now);
        self.notify_background_drain_for_current_runtime(BackgroundTaskPriority::High);
        let progressed = match &self.inner {
            StorageRuntimeInner::Cache(slot) => {
                slot.wait_background_progress_until(completed_before_wait, wait_deadline)
            }
            StorageRuntimeInner::Durable(slot) => {
                slot.wait_background_progress_until(completed_before_wait, wait_deadline)
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                slot.wait_background_progress_until(completed_before_wait, wait_deadline)
            }
            StorageRuntimeInner::Closed => return false,
        };
        let wait_elapsed = self
            .background_now_for_current_runtime()
            .unwrap_or(wait_start)
            .saturating_duration_since(wait_start);
        perf_trace::record_lifecycle_write_admission_block_wait(wait_elapsed);
        let pressure_after_wait = self.background_pressure_snapshot_for_branch(*branch_id);
        if pressure_before_wait
            .zip(pressure_after_wait)
            .is_some_and(|(before, after)| after.relieved_since(before, *pressure_reason))
        {
            *deadline = None;
            *no_relief_rounds = 0;
            return true;
        }
        let (lifecycle_pending_tasks, lifecycle_active_task) = self
            .background_lifecycle_work_for_current_runtime()
            .unwrap_or((0, false));
        if background_pressure_wait_should_continue_for_progress(
            progressed,
            lifecycle_pending_tasks,
            lifecycle_active_task,
            no_relief_rounds,
        ) {
            return true;
        }
        if progressed {
            perf_trace::record_lifecycle_write_admission_wait_timeout();
            return false;
        }
        perf_trace::record_lifecycle_write_admission_wait_timeout();
        false
    }

    fn background_wait_after_wal_growth_enqueue(
        &mut self,
        wal_growth: Option<&LifecycleWalGrowthOutcome>,
    ) {
        if wal_growth.is_none() {
            return;
        }
        if !self.has_background_runtime() {
            return;
        }
        let Some(now) = self.background_now_for_current_runtime() else {
            return;
        };
        #[cfg(test)]
        let block_wait = self.background_block_wait_for_current_runtime();
        #[cfg(not(test))]
        let block_wait = BackgroundBlockWaitConfig::default();
        let stall_deadline = now.saturating_add(block_wait.stall_deadline);
        let mut no_relief_rounds = 0usize;
        while self.current_wal_growth_exceeds_backpressure() {
            let Some(now) = self.background_now_for_current_runtime() else {
                return;
            };
            if now >= stall_deadline {
                return;
            }
            let snapshot_before_wait = self.background_wal_growth_snapshot_for_current_runtime();
            let wait_deadline = now
                .saturating_add(block_wait.wait_slice)
                .min(stall_deadline);
            self.evaluate_wal_growth_policy_for_background_wait();
            self.notify_background_drain_for_current_runtime(BackgroundTaskPriority::High);
            let Some(stats) = self.background_stats_for_current_runtime() else {
                return;
            };
            let (lifecycle_pending_tasks, lifecycle_active_task) = self
                .background_lifecycle_work_for_current_runtime()
                .unwrap_or((0, false));
            if stats
                .queue_depth
                .saturating_add(stats.active_tasks)
                .saturating_add(lifecycle_pending_tasks)
                .saturating_add(usize::from(lifecycle_active_task))
                == 0
            {
                return;
            }
            let completed_before_wait = stats.tasks_completed;
            let progressed = match &self.inner {
                StorageRuntimeInner::Cache(slot) => {
                    slot.wait_background_progress_until(completed_before_wait, wait_deadline)
                }
                StorageRuntimeInner::Durable(slot) => {
                    slot.wait_background_progress_until(completed_before_wait, wait_deadline)
                }
                StorageRuntimeInner::DurableOwned(slot) => {
                    slot.wait_background_progress_until(completed_before_wait, wait_deadline)
                }
                StorageRuntimeInner::Closed => return,
            };
            let snapshot_after_wait = self.background_wal_growth_snapshot_for_current_runtime();
            if snapshot_before_wait
                .zip(snapshot_after_wait)
                .is_some_and(|(before, after)| after.relieved_since(before))
            {
                no_relief_rounds = 0;
                continue;
            }
            let (lifecycle_pending_tasks, lifecycle_active_task) = self
                .background_lifecycle_work_for_current_runtime()
                .unwrap_or((0, false));
            if lifecycle_active_task {
                no_relief_rounds = 0;
                continue;
            }
            if progressed && lifecycle_pending_tasks > 0 {
                no_relief_rounds = no_relief_rounds.saturating_add(1);
                if no_relief_rounds < block_wait.no_relief_rounds {
                    continue;
                }
            }
            if !progressed || no_relief_rounds >= block_wait.no_relief_rounds {
                return;
            }
        }
    }

    fn evaluate_wal_growth_policy_for_background_wait(&mut self) {
        match &mut self.inner {
            StorageRuntimeInner::Cache(_) | StorageRuntimeInner::Closed => {}
            StorageRuntimeInner::Durable(slot) => {
                slot.lock().evaluate_and_record_wal_growth_policy();
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                slot.lock().evaluate_and_record_wal_growth_policy();
            }
        }
    }

    fn current_wal_growth_exceeds_backpressure(&self) -> bool {
        self.background_wal_growth_snapshot_for_current_runtime()
            .is_some_and(|snapshot| snapshot.exceeds_backpressure)
    }

    fn background_wal_growth_snapshot_for_current_runtime(
        &self,
    ) -> Option<BackgroundWalGrowthSnapshot> {
        match &self.inner {
            StorageRuntimeInner::Cache(_) | StorageRuntimeInner::Closed => None,
            StorageRuntimeInner::Durable(slot) => slot
                .lock()
                .current_wal_growth_backpressure_snapshot()
                .ok()
                .map(|(facts, commits_since_checkpoint, trigger)| {
                    BackgroundWalGrowthSnapshot::from_parts(
                        facts,
                        commits_since_checkpoint,
                        trigger,
                    )
                }),
            StorageRuntimeInner::DurableOwned(slot) => slot
                .lock()
                .current_wal_growth_backpressure_snapshot()
                .ok()
                .map(|(facts, commits_since_checkpoint, trigger)| {
                    BackgroundWalGrowthSnapshot::from_parts(
                        facts,
                        commits_since_checkpoint,
                        trigger,
                    )
                }),
        }
    }

    fn enqueue_pressure_maintenance_for_background_wait(
        &mut self,
        branch_id: BranchId,
        pressure_reason: LifecycleStoragePressureReason,
    ) -> usize {
        match &mut self.inner {
            StorageRuntimeInner::Cache(slot) => {
                let mut runtime = slot.lock();
                let _ = runtime.schedule_post_commit_maintenance_for_branch(branch_id);
                if pressure_reason == LifecycleStoragePressureReason::FrozenBacklog
                    && runtime.maintenance_status().pending_tasks() == 0
                {
                    let _ = runtime
                        .enqueue_maintenance(LifecycleMaintenanceTaskRequest::flush(branch_id));
                }
                runtime.maintenance_status().pending_tasks()
            }
            StorageRuntimeInner::Durable(slot) => {
                let mut runtime = slot.lock();
                let _ = runtime.schedule_post_commit_maintenance_for_branch(branch_id);
                if pressure_reason == LifecycleStoragePressureReason::FrozenBacklog
                    && runtime.maintenance_status().pending_tasks() == 0
                {
                    let _ = runtime
                        .enqueue_maintenance(LifecycleMaintenanceTaskRequest::flush(branch_id));
                }
                runtime.maintenance_status().pending_tasks()
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let mut runtime = slot.lock();
                let _ = runtime.schedule_post_commit_maintenance_for_branch(branch_id);
                if pressure_reason == LifecycleStoragePressureReason::FrozenBacklog
                    && runtime.maintenance_status().pending_tasks() == 0
                {
                    let _ = runtime
                        .enqueue_maintenance(LifecycleMaintenanceTaskRequest::flush(branch_id));
                }
                runtime.maintenance_status().pending_tasks()
            }
            StorageRuntimeInner::Closed => 0,
        }
    }

    fn resolve_commit_durability(
        &self,
        requested: CommitDurability,
    ) -> StorageApiResult<crate::commit::CommitDurabilityMode> {
        match &self.inner {
            StorageRuntimeInner::Cache(_) => match requested {
                CommitDurability::RuntimeDefault | CommitDurability::NotDurable => {
                    Ok(crate::commit::CommitDurabilityMode::Cache)
                }
                CommitDurability::Standard | CommitDurability::Always => {
                    Err(StorageApiError::UnsupportedCapability {
                        capability: "commit_durability",
                        reason: "cache runtime cannot satisfy durable commit requests",
                    })
                }
            },
            StorageRuntimeInner::Durable(slot) => {
                let runtime = slot.lock();
                match (runtime.open_plan().storage_mode(), requested) {
                    (
                        LifecycleStorageMode::DurableLocalStandard,
                        CommitDurability::RuntimeDefault | CommitDurability::Standard,
                    ) => Ok(crate::commit::CommitDurabilityMode::Standard),
                    (
                        LifecycleStorageMode::DurableLocalAlways,
                        CommitDurability::RuntimeDefault | CommitDurability::Always,
                    ) => Ok(crate::commit::CommitDurabilityMode::Always),
                    (_, CommitDurability::NotDurable) => {
                        Err(StorageApiError::UnsupportedCapability {
                            capability: "commit_durability",
                            reason: "durable runtime cannot accept cache-only commit requests",
                        })
                    }
                    (LifecycleStorageMode::DurableLocalStandard, CommitDurability::Always) => {
                        Err(StorageApiError::UnsupportedCapability {
                            capability: "commit_durability",
                            reason: "always commit durability requires an always-durable runtime",
                        })
                    }
                    (LifecycleStorageMode::DurableLocalAlways, CommitDurability::Standard) => {
                        Err(StorageApiError::UnsupportedCapability {
                            capability: "commit_durability",
                            reason:
                                "standard commit durability cannot weaken an always-durable runtime",
                        })
                    }
                    _ => Err(StorageApiError::UnsupportedCapability {
                        capability: "commit_durability",
                        reason: "commit durability is unsupported for this runtime mode",
                    }),
                }
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let runtime = slot.lock();
                match (runtime.open_plan().storage_mode(), requested) {
                    (
                        LifecycleStorageMode::DurableLocalStandard,
                        CommitDurability::RuntimeDefault | CommitDurability::Standard,
                    ) => Ok(crate::commit::CommitDurabilityMode::Standard),
                    (
                        LifecycleStorageMode::DurableLocalAlways,
                        CommitDurability::RuntimeDefault | CommitDurability::Always,
                    ) => Ok(crate::commit::CommitDurabilityMode::Always),
                    (_, CommitDurability::NotDurable) => {
                        Err(StorageApiError::UnsupportedCapability {
                            capability: "commit_durability",
                            reason: "durable runtime cannot accept cache-only commit requests",
                        })
                    }
                    (LifecycleStorageMode::DurableLocalStandard, CommitDurability::Always) => {
                        Err(StorageApiError::UnsupportedCapability {
                            capability: "commit_durability",
                            reason: "always commit durability requires an always-durable runtime",
                        })
                    }
                    (LifecycleStorageMode::DurableLocalAlways, CommitDurability::Standard) => {
                        Err(StorageApiError::UnsupportedCapability {
                            capability: "commit_durability",
                            reason:
                                "standard commit durability cannot weaken an always-durable runtime",
                        })
                    }
                    _ => Err(StorageApiError::UnsupportedCapability {
                        capability: "commit_durability",
                        reason: "commit durability is unsupported for this runtime mode",
                    }),
                }
            }
            StorageRuntimeInner::Closed => Err(StorageApiError::InvalidRuntimeState {
                reason: "commit requires an open runtime",
            }),
        }
    }

    fn next_commit_timestamp(&self) -> Timestamp {
        let last_allocated = match &self.inner {
            StorageRuntimeInner::Cache(slot) => {
                let runtime = slot.lock();
                runtime.allocator().timestamp_guard().last_allocated()
            }
            StorageRuntimeInner::Durable(slot) => {
                let runtime = slot.lock();
                runtime.allocator().timestamp_guard().last_allocated()
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let runtime = slot.lock();
                runtime.allocator().timestamp_guard().last_allocated()
            }
            StorageRuntimeInner::Closed => None,
        };
        match last_allocated {
            Some(timestamp) if timestamp >= DEFAULT_TIMESTAMP => {
                timestamp.saturating_add(Duration::from_micros(1))
            }
            Some(_) | None => DEFAULT_TIMESTAMP,
        }
    }

    #[cfg(test)]
    pub(crate) fn set_timestamp_coverage_for_test(
        &mut self,
        branch_id: BranchId,
        coverage: crate::branch::read::BranchTimestampCoverage,
    ) -> StorageApiResult<()> {
        match &mut self.inner {
            StorageRuntimeInner::Cache(slot) => {
                let mut runtime = slot.lock();
                let generation = runtime
                    .branch_catalog()
                    .registry()
                    .lookup(branch_id)
                    .map_err(commit_error)?
                    .generation();
                runtime
                    .branch_catalog_mut_for_test()
                    .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))
                    .map_err(map_lifecycle_error)?
                    .set_timestamp_coverage(coverage);
                Ok(())
            }
            StorageRuntimeInner::Durable(slot) => {
                let mut runtime = slot.lock();
                let generation = runtime
                    .branch_catalog()
                    .registry()
                    .lookup(branch_id)
                    .map_err(commit_error)?
                    .generation();
                runtime
                    .branch_catalog_mut_for_test()
                    .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))
                    .map_err(map_lifecycle_error)?
                    .set_timestamp_coverage(coverage);
                Ok(())
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let mut runtime = slot.lock();
                let generation = runtime
                    .branch_catalog()
                    .registry()
                    .lookup(branch_id)
                    .map_err(commit_error)?
                    .generation();
                runtime
                    .branch_catalog_mut_for_test()
                    .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))
                    .map_err(map_lifecycle_error)?
                    .set_timestamp_coverage(coverage);
                Ok(())
            }
            StorageRuntimeInner::Closed => Err(StorageApiError::InvalidRuntimeState {
                reason: "timestamp coverage update requires an open runtime",
            }),
        }
    }

    #[cfg(test)]
    pub(crate) fn fork_default_branch_for_test(
        &mut self,
        destination: BranchId,
    ) -> StorageApiResult<()> {
        let destination_generation =
            crate::commit::CommitBranchGeneration::new(DEFAULT_BRANCH_GENERATION)
                .map_err(commit_error)?;
        match &mut self.inner {
            StorageRuntimeInner::Cache(slot) => slot
                .lock()
                .fork_current(DEFAULT_BRANCH_ID, destination, destination_generation)
                .map(|_| ())
                .map_err(map_lifecycle_error),
            StorageRuntimeInner::Durable(slot) => slot
                .lock()
                .fork_current(DEFAULT_BRANCH_ID, destination, destination_generation)
                .map(|_| ())
                .map_err(map_lifecycle_error),
            StorageRuntimeInner::DurableOwned(slot) => slot
                .lock()
                .fork_current(DEFAULT_BRANCH_ID, destination, destination_generation)
                .map(|_| ())
                .map_err(map_lifecycle_error),
            StorageRuntimeInner::Closed => Err(StorageApiError::InvalidRuntimeState {
                reason: "fork requires an open runtime",
            }),
        }
    }

    #[cfg(test)]
    pub(crate) fn flush_default_branch_for_test(&mut self) -> StorageApiResult<()> {
        self.flush_branch_for_test(DEFAULT_BRANCH_ID)
    }

    #[cfg(test)]
    pub(crate) fn rotate_default_branch_for_test(&mut self) -> StorageApiResult<()> {
        self.rotate_branch_for_test(DEFAULT_BRANCH_ID)
    }

    #[cfg(test)]
    pub(crate) fn rotate_branch_for_test(&mut self, branch_id: BranchId) -> StorageApiResult<()> {
        match &mut self.inner {
            StorageRuntimeInner::Cache(slot) => {
                let mut runtime = slot.lock();
                runtime
                    .rotate_active_for_branch_for_maintenance(branch_id)
                    .map(|_| ())
                    .map_err(map_lifecycle_error)
            }
            StorageRuntimeInner::Durable(slot) => {
                let mut runtime = slot.lock();
                runtime
                    .rotate_active_for_branch_for_maintenance(branch_id)
                    .map(|_| ())
                    .map_err(map_lifecycle_error)
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let mut runtime = slot.lock();
                runtime
                    .rotate_active_for_branch_for_maintenance(branch_id)
                    .map(|_| ())
                    .map_err(map_lifecycle_error)
            }
            StorageRuntimeInner::Closed => Err(StorageApiError::InvalidRuntimeState {
                reason: "rotation requires an open runtime",
            }),
        }
    }

    #[cfg(test)]
    pub(crate) fn flush_branch_for_test(&mut self, branch_id: BranchId) -> StorageApiResult<()> {
        match &mut self.inner {
            StorageRuntimeInner::Cache(slot) => {
                let mut runtime = slot.lock();
                runtime
                    .rotate_active_for_maintenance()
                    .map_err(map_lifecycle_error)?;
                runtime
                    .flush_frozen(&flush_request_for_boundary(branch_id)?)
                    .map(|_| ())
                    .map_err(map_lifecycle_error)
            }
            StorageRuntimeInner::Durable(slot) => {
                let mut runtime = slot.lock();
                runtime
                    .rotate_active_for_maintenance()
                    .map_err(map_lifecycle_error)?;
                runtime
                    .flush_frozen(&flush_request_for_boundary(branch_id)?)
                    .map(|_| ())
                    .map_err(map_lifecycle_error)
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let mut runtime = slot.lock();
                runtime
                    .rotate_active_for_maintenance()
                    .map_err(map_lifecycle_error)?;
                runtime
                    .flush_frozen(&flush_request_for_boundary(branch_id)?)
                    .map(|_| ())
                    .map_err(map_lifecycle_error)
            }
            StorageRuntimeInner::Closed => Err(StorageApiError::InvalidRuntimeState {
                reason: "flush requires an open runtime",
            }),
        }
    }

    #[cfg(test)]
    pub(crate) fn pin_branch_reachability_for_test(
        &mut self,
        branch_id: BranchId,
    ) -> StorageApiResult<()> {
        match &mut self.inner {
            StorageRuntimeInner::Cache(slot) => slot
                .lock()
                .branch_catalog_mut_for_test()
                .pin_reachability(branch_id)
                .map(|_| ())
                .map_err(map_lifecycle_error),
            StorageRuntimeInner::Durable(slot) => slot
                .lock()
                .branch_catalog_mut_for_test()
                .pin_reachability(branch_id)
                .map(|_| ())
                .map_err(map_lifecycle_error),
            StorageRuntimeInner::DurableOwned(slot) => slot
                .lock()
                .branch_catalog_mut_for_test()
                .pin_reachability(branch_id)
                .map(|_| ())
                .map_err(map_lifecycle_error),
            StorageRuntimeInner::Closed => Err(StorageApiError::InvalidRuntimeState {
                reason: "pin requires an open runtime",
            }),
        }
    }

    #[cfg(test)]
    pub(crate) fn append_raw_row_for_test(&mut self, row: StorageRow) -> StorageApiResult<()> {
        let branch_id = row.physical_key().branch_id();
        match &mut self.inner {
            StorageRuntimeInner::Cache(slot) => {
                let mut runtime = slot.lock();
                let generation = runtime
                    .branch_catalog()
                    .registry()
                    .lookup(branch_id)
                    .map_err(commit_error)?
                    .generation();
                runtime
                    .branch_catalog_mut_for_test()
                    .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))
                    .map_err(map_lifecycle_error)?
                    .append_committed_row(row)
                    .map(|_| ())
                    .map_err(branch_error)
            }
            StorageRuntimeInner::Durable(slot) => {
                let mut runtime = slot.lock();
                let generation = runtime
                    .branch_catalog()
                    .registry()
                    .lookup(branch_id)
                    .map_err(commit_error)?
                    .generation();
                runtime
                    .branch_catalog_mut_for_test()
                    .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))
                    .map_err(map_lifecycle_error)?
                    .append_committed_row(row)
                    .map(|_| ())
                    .map_err(branch_error)
            }
            StorageRuntimeInner::DurableOwned(slot) => {
                let mut runtime = slot.lock();
                let generation = runtime
                    .branch_catalog()
                    .registry()
                    .lookup(branch_id)
                    .map_err(commit_error)?
                    .generation();
                runtime
                    .branch_catalog_mut_for_test()
                    .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))
                    .map_err(map_lifecycle_error)?
                    .append_committed_row(row)
                    .map(|_| ())
                    .map_err(branch_error)
            }
            StorageRuntimeInner::Closed => Err(StorageApiError::InvalidRuntimeState {
                reason: "raw row append requires an open runtime",
            }),
        }
    }
}

fn lifecycle_plan(options: StorageOpenOptions) -> StorageApiResult<StorageOpenPlan> {
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
    config = config
        .with_storage_budget(map_budget_policy(options.budget_policy()))
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

fn durable_backend_handle_for_open(
    options: StorageOpenOptions,
    backend: &StorageBackend,
) -> StorageApiResult<DurableBackendHandleForOpen<'_>> {
    match options.maintenance_scheduling_policy() {
        StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue
        | StorageMaintenanceSchedulingPolicy::Disabled => {
            return Ok(DurableBackendHandleForOpen::Borrowed(
                backend.as_backend_handle(),
            ));
        }
        StorageMaintenanceSchedulingPolicy::Background
        | StorageMaintenanceSchedulingPolicy::DeterministicInline => {}
    }

    #[cfg(feature = "localfs")]
    if let Some(handle) = backend.to_owned_backend_handle() {
        return Ok(DurableBackendHandleForOpen::Owned(handle));
    }

    Err(StorageApiError::InvalidArgument {
        field: "maintenance_scheduling_policy",
        reason: "background durable opens with borrowed backend handles require evaluate-and-enqueue; background and deterministic-inline durable opens require an owned backend handle",
    })
}

fn map_budget_policy(policy: StorageBudgetPolicy) -> StorageRuntimeBudget {
    match policy {
        StorageBudgetPolicy::Default => StorageRuntimeBudget::default(),
        StorageBudgetPolicy::LowMemory => StorageRuntimeBudget::low_memory_test_profile(),
    }
}

fn map_wal_growth_policy(policy: StorageWalGrowthPolicy) -> LifecycleWalGrowthPolicy {
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
            max_commits_since_checkpoint,
        ),
    }
}

const fn map_maintenance_scheduling_policy(
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

const fn background_executor_mode(
    policy: StorageMaintenanceSchedulingPolicy,
) -> BackgroundExecutorMode {
    match policy {
        StorageMaintenanceSchedulingPolicy::DeterministicInline => BackgroundExecutorMode::Inline,
        StorageMaintenanceSchedulingPolicy::Background
        | StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue
        | StorageMaintenanceSchedulingPolicy::Disabled => BackgroundExecutorMode::Threaded,
    }
}

fn map_open_summary(
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

fn map_close_summary(outcome: CloseOutcome, idempotent: bool) -> StorageCloseSummary {
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

fn with_background_close_facts(
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

fn background_shutdown_panic_error(
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

fn record_background_close_maintenance_facts(
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

fn map_close_effects(outcome: CloseOutcome) -> StorageCloseEffects {
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

fn validate_maintenance_request(request: &MaintenanceRequest) -> StorageApiResult<()> {
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

fn map_maintenance_summary(
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

fn map_checkpoint_summary(
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

fn map_wal_growth_maintenance_summary(
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

fn map_wal_growth_summary(outcome: &LifecycleWalGrowthOutcome) -> MaintenanceWalGrowthSummary {
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

const fn map_maintenance_status(
    status: LifecycleMaintenanceOutcomeStatus,
) -> MaintenanceSummaryStatus {
    match status {
        LifecycleMaintenanceOutcomeStatus::Completed => MaintenanceSummaryStatus::Completed,
        LifecycleMaintenanceOutcomeStatus::Deferred => MaintenanceSummaryStatus::Deferred,
        LifecycleMaintenanceOutcomeStatus::Failed => MaintenanceSummaryStatus::Failed,
        LifecycleMaintenanceOutcomeStatus::Canceled => MaintenanceSummaryStatus::Canceled,
    }
}

const fn map_maintenance_reason_class(
    reason_class: LifecycleMaintenanceOutcomeReasonClass,
) -> MaintenanceReasonClass {
    match reason_class {
        LifecycleMaintenanceOutcomeReasonClass::Deferred => MaintenanceReasonClass::Deferred,
        LifecycleMaintenanceOutcomeReasonClass::Failed => MaintenanceReasonClass::Failed,
        LifecycleMaintenanceOutcomeReasonClass::Canceled => MaintenanceReasonClass::Canceled,
    }
}

const fn map_wal_growth_status(status: LifecycleWalGrowthStatus) -> MaintenanceWalGrowthStatus {
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

const fn map_wal_growth_trigger(trigger: LifecycleWalGrowthTrigger) -> MaintenanceWalGrowthTrigger {
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

fn map_maintenance_queue_summary(
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

fn unsupported_maintenance_summary(
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

fn lifecycle_error_facts(
    error: &LifecycleError,
) -> (Option<&'static str>, Option<StorageApiErrorClass>) {
    let error = map_lifecycle_error(error.clone());
    (Some(error.code()), Some(error.class()))
}

fn request_for_outcome(outcome: &LifecycleMaintenanceOutcome) -> MaintenanceRequest {
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

fn map_maintenance_task_request(
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

const fn background_priority_for_task_request(
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

fn run_next_cache_maintenance(
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

fn drain_cache_background_round(
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

fn run_next_durable_maintenance(
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

fn run_next_background_durable_maintenance(
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

#[allow(
    clippy::too_many_lines,
    reason = "durable background drain is an explicit start/build/publish state machine"
)]
fn drain_durable_background_round(
    runtime: &Arc<ParkingMutex<LifecycleDurableLocalRuntime<'static, ApiTimestampSource>>>,
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
                Ok(None) => match runtime.start_next_background_checkpoint_maintenance() {
                    Ok(Some(step)) => Ok(Some(step)),
                    Ok(None) => match runtime.run_next_background_flush_watermark_maintenance() {
                        Ok(Some(outcome)) => {
                            Ok(Some(DurableBackgroundMaintenanceStep::completed(outcome)))
                        }
                        Ok(None) => {
                            match runtime.start_next_background_wal_truncation_maintenance() {
                                Ok(Some(step)) => Ok(Some(step)),
                                Ok(None) => {
                                    match runtime.start_next_background_table_rewrite_maintenance()
                                    {
                                        Ok(Some(step)) => Ok(Some(step)),
                                        Ok(None) => {
                                            run_next_background_durable_maintenance(&mut runtime)
                                                .map(|outcome| {
                                                    outcome.map(
                                                        DurableBackgroundMaintenanceStep::completed,
                                                    )
                                                })
                                        }
                                        Err(error) => Err(map_lifecycle_error(error)),
                                    }
                                }
                                Err(error) => Err(map_lifecycle_error(error)),
                            }
                        }
                        Err(error) => Err(map_lifecycle_error(error)),
                    },
                    Err(error) => Err(map_lifecycle_error(error)),
                },
                Err(error) => Err(map_lifecycle_error(error)),
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
            }
            DurableBackgroundMaintenanceStep::Build(pending_build) => {
                let pending_build = *pending_build;
                let task = pending_build.task();
                let build_start = perf_trace::start_timer();
                let build_result = pending_build.build();
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

fn map_generation_guard(
    generation: Option<BranchGeneration>,
) -> StorageApiResult<CommitBranchGenerationGuard> {
    match generation {
        Some(generation) if generation == BranchGeneration::ZERO => {
            Err(StorageApiError::InvalidArgument {
                field: "branch_generation",
                reason: "expected branch generation must be nonzero",
            })
        }
        Some(generation) => CommitBranchGeneration::new(generation.as_u64())
            .map(CommitBranchGenerationGuard::exact)
            .map_err(commit_error),
        None => Ok(CommitBranchGenerationGuard::not_supplied()),
    }
}

fn branch_generation_or_default(
    generation: Option<BranchGeneration>,
) -> StorageApiResult<CommitBranchGeneration> {
    match generation {
        Some(generation) if generation == BranchGeneration::ZERO => {
            Err(StorageApiError::InvalidArgument {
                field: "branch_generation",
                reason: "branch generation must be nonzero",
            })
        }
        Some(generation) => CommitBranchGeneration::new(generation.as_u64()).map_err(commit_error),
        None => default_branch_generation(),
    }
}

fn require_valid_branch_identifier(
    branch_id: BranchId,
    field: &'static str,
) -> StorageApiResult<()> {
    if branch_id.as_bytes().iter().all(|byte| *byte == 0) {
        Err(StorageApiError::InvalidArgument {
            field,
            reason: "branch id must not be all zero",
        })
    } else {
        Ok(())
    }
}

#[allow(
    clippy::match_same_arms,
    reason = "borrowed and owned durable runtime variants share behavior but carry different backend lifetimes"
)]
fn current_visible(runtime: &StorageRuntime<'_>) -> Option<CommitVersion> {
    let version = match &runtime.inner {
        StorageRuntimeInner::Cache(slot) => {
            let runtime = slot.lock();
            runtime.visible_version()
        }
        StorageRuntimeInner::Durable(slot) => {
            let runtime = slot.lock();
            runtime.visible_version()
        }
        StorageRuntimeInner::DurableOwned(slot) => {
            let runtime = slot.lock();
            runtime.visible_version()
        }
        StorageRuntimeInner::Closed => CommitVersion::ZERO,
    };
    (version != CommitVersion::ZERO).then_some(version)
}

fn branch_for_diagnostics_scope(scope: DiagnosticsScope) -> BranchId {
    match scope {
        DiagnosticsScope::Global => DEFAULT_BRANCH_ID,
        DiagnosticsScope::Branch(branch_id) => branch_id,
    }
}

fn diagnostics_mode_from_plan(
    open_summary: Option<StorageOpenSummary>,
    plan: &StorageOpenPlan,
) -> StorageMode {
    open_summary.map_or_else(
        || map_lifecycle_storage_mode(plan.storage_mode()),
        StorageOpenSummary::mode,
    )
}

const fn map_lifecycle_storage_mode(mode: LifecycleStorageMode) -> StorageMode {
    match mode {
        LifecycleStorageMode::Cache => StorageMode::Cache,
        LifecycleStorageMode::DurableLocalStandard => StorageMode::DurableLocal {
            policy: StorageDurabilityPolicy::Standard,
        },
        LifecycleStorageMode::DurableLocalAlways => StorageMode::DurableLocal {
            policy: StorageDurabilityPolicy::Always,
        },
        LifecycleStorageMode::ObjectDurableCandidate => StorageMode::ObjectDurableCandidate,
    }
}

fn durable_checkpoint_report<S>(
    runtime: &LifecycleDurableLocalRuntime<'_, S>,
) -> DiagnosticsCheckpointReport {
    let Some(manifest) = runtime.services().manifest().load_current().ok().flatten() else {
        return DiagnosticsCheckpointReport::unknown();
    };
    DiagnosticsCheckpointReport::known(
        manifest.snapshot_id(),
        manifest.snapshot_watermark().map(CommitVersion::new),
        manifest.flushed_through_commit_id(),
    )
}

fn map_diagnostics_recovery(health: &RecoveryHealth) -> DiagnosticsRecoveryReport {
    match health {
        RecoveryHealth::Healthy => DiagnosticsRecoveryReport::healthy(),
        RecoveryHealth::Degraded { class, faults } => DiagnosticsRecoveryReport::new(
            RecoveryHealthSummary::Degraded,
            Some(map_diagnostics_recovery_class(*class)),
            faults.iter().map(map_diagnostics_recovery_fault).collect(),
        ),
        RecoveryHealth::Failed { fault } => DiagnosticsRecoveryReport::new(
            RecoveryHealthSummary::Failed,
            Some(map_failed_diagnostics_recovery_class(fault.kind())),
            vec![map_diagnostics_recovery_fault(fault)],
        ),
    }
}

const fn map_diagnostics_recovery_class(
    class: RecoveryDegradationClass,
) -> DiagnosticsRecoveryClass {
    match class {
        RecoveryDegradationClass::DataLoss => DiagnosticsRecoveryClass::Corruption,
        RecoveryDegradationClass::PolicyDowngrade => DiagnosticsRecoveryClass::Policy,
        RecoveryDegradationClass::Telemetry => DiagnosticsRecoveryClass::Telemetry,
    }
}

const fn map_failed_diagnostics_recovery_class(
    kind: RecoveryFaultKind,
) -> DiagnosticsRecoveryClass {
    match kind {
        RecoveryFaultKind::IoFailure | RecoveryFaultKind::WalTailRepairFailed => {
            DiagnosticsRecoveryClass::Io
        }
        RecoveryFaultKind::NoManifestFallback => DiagnosticsRecoveryClass::Policy,
        RecoveryFaultKind::CorruptManifest
        | RecoveryFaultKind::CorruptSnapshot
        | RecoveryFaultKind::CorruptWal
        | RecoveryFaultKind::MissingManifestObject
        | RecoveryFaultKind::MissingSnapshotObject
        | RecoveryFaultKind::MissingTableObject
        | RecoveryFaultKind::InheritedLayerLoss
        | RecoveryFaultKind::QuarantineInventoryMismatch
        | RecoveryFaultKind::TimelineMismatch => DiagnosticsRecoveryClass::Corruption,
    }
}

fn map_diagnostics_recovery_fault(
    fault: &crate::lifecycle::RecoveryFault,
) -> DiagnosticsRecoveryFault {
    DiagnosticsRecoveryFault::new(
        map_diagnostics_recovery_fault_kind(fault.kind()),
        fault.reason(),
        fault.affected_branch(),
    )
}

const fn map_diagnostics_recovery_fault_kind(
    kind: RecoveryFaultKind,
) -> DiagnosticsRecoveryFaultKind {
    match kind {
        RecoveryFaultKind::CorruptManifest => DiagnosticsRecoveryFaultKind::CorruptManifest,
        RecoveryFaultKind::CorruptSnapshot => DiagnosticsRecoveryFaultKind::CorruptSnapshot,
        RecoveryFaultKind::CorruptWal => DiagnosticsRecoveryFaultKind::CorruptWal,
        RecoveryFaultKind::MissingManifestObject => {
            DiagnosticsRecoveryFaultKind::MissingManifestObject
        }
        RecoveryFaultKind::MissingSnapshotObject => {
            DiagnosticsRecoveryFaultKind::MissingSnapshotObject
        }
        RecoveryFaultKind::MissingTableObject => DiagnosticsRecoveryFaultKind::MissingTableObject,
        RecoveryFaultKind::InheritedLayerLoss => DiagnosticsRecoveryFaultKind::InheritedLayerLoss,
        RecoveryFaultKind::NoManifestFallback => DiagnosticsRecoveryFaultKind::NoManifestFallback,
        RecoveryFaultKind::IoFailure => DiagnosticsRecoveryFaultKind::IoFailure,
        RecoveryFaultKind::QuarantineInventoryMismatch => {
            DiagnosticsRecoveryFaultKind::QuarantineInventoryMismatch
        }
        RecoveryFaultKind::TimelineMismatch => DiagnosticsRecoveryFaultKind::TimelineMismatch,
        RecoveryFaultKind::WalTailRepairFailed => DiagnosticsRecoveryFaultKind::WalTailRepairFailed,
    }
}

fn map_budget_report(snapshot: &StorageBudgetSnapshot) -> DiagnosticsBudgetReport {
    let usages = snapshot
        .usages()
        .iter()
        .map(|usage| {
            DiagnosticsBudgetUsage::new(
                map_budget_pool(usage.pool()),
                usage.used_bytes(),
                usage.limit_bytes(),
                usage.used_count(),
                usage.limit_count(),
                map_budget_pressure(snapshot.pressure(usage.pool())),
            )
        })
        .collect();
    DiagnosticsBudgetReport::known(snapshot.budget().total_bytes(), usages)
}

const fn map_budget_pool(pool: StorageBudgetPool) -> DiagnosticsBudgetPool {
    match pool {
        StorageBudgetPool::BlockCache => DiagnosticsBudgetPool::BlockCache,
        StorageBudgetPool::TableReader => DiagnosticsBudgetPool::TableReader,
        StorageBudgetPool::ActiveMutable => DiagnosticsBudgetPool::ActiveMutable,
        StorageBudgetPool::FrozenMutable => DiagnosticsBudgetPool::FrozenMutable,
        StorageBudgetPool::MaintenanceQueue => DiagnosticsBudgetPool::MaintenanceQueue,
        StorageBudgetPool::GeneratedArtifact => DiagnosticsBudgetPool::GeneratedArtifact,
        StorageBudgetPool::ManifestCatalog => DiagnosticsBudgetPool::ManifestCatalog,
    }
}

const fn map_budget_pressure(severity: StorageBudgetPressureSeverity) -> DiagnosticsBudgetPressure {
    match severity {
        StorageBudgetPressureSeverity::Normal => DiagnosticsBudgetPressure::Normal,
        StorageBudgetPressureSeverity::Evicting => DiagnosticsBudgetPressure::Evicting,
        StorageBudgetPressureSeverity::DeferOptionalMaintenance => {
            DiagnosticsBudgetPressure::DeferOptionalMaintenance
        }
        StorageBudgetPressureSeverity::RejectOptionalWork => {
            DiagnosticsBudgetPressure::RejectOptionalWork
        }
        StorageBudgetPressureSeverity::RejectMutatingAdmission => {
            DiagnosticsBudgetPressure::RejectMutatingAdmission
        }
    }
}

fn diagnostics_pressure_report(
    catalog: &LifecycleBranchCatalog,
    branch_id: BranchId,
    maintenance: MaintenanceExecutorStatus,
    budget: StorageRuntimeBudget,
) -> DiagnosticsStoragePressureReport {
    let Ok(branch) = catalog.branch_state(branch_id) else {
        return DiagnosticsStoragePressureReport::unknown();
    };
    let pressure = collect_storage_pressure_with_budget(branch, maintenance, Some(budget));
    DiagnosticsStoragePressureReport::known(
        branch_id,
        map_storage_pressure_severity(pressure.severity()),
        map_storage_pressure_reason(pressure.reason()),
        pressure.active_rows(),
        pressure.active_bytes(),
        pressure.frozen_tables(),
        pressure.frozen_bytes(),
        pressure.level_zero_tables(),
        pressure.owned_tables(),
        pressure.inherited_layers(),
        pressure.pending_maintenance(),
    )
}

fn diagnostics_source_layout_report(
    catalog: &LifecycleBranchCatalog,
    branch_id: BranchId,
) -> DiagnosticsSourceLayoutReport {
    let Ok(branch) = catalog.branch_state(branch_id) else {
        return DiagnosticsSourceLayoutReport::unknown();
    };
    let layout = branch.source_layout();
    DiagnosticsSourceLayoutReport::known(
        layout.active_rows(),
        layout.frozen_table_count(),
        layout.frozen_rows(),
        layout.owned_l0_tables(),
        map_source_level_table_counts(layout.owned_nonzero_level_table_counts()),
        layout.owned_total_tables(),
        layout.inherited_layers(),
        layout.inherited_l0_tables(),
        map_source_level_table_counts(layout.inherited_nonzero_level_table_counts()),
        layout.inherited_total_tables(),
    )
}

fn map_source_level_table_counts(
    counts: &[crate::branch::facts::BranchLevelTableCount],
) -> Vec<DiagnosticsSourceLevelTableCount> {
    counts
        .iter()
        .map(|count| {
            DiagnosticsSourceLevelTableCount::new(count.level().raw(), count.table_count())
        })
        .collect()
}

const fn map_storage_pressure_severity(
    severity: LifecycleStoragePressureSeverity,
) -> DiagnosticsStoragePressureSeverity {
    match severity {
        LifecycleStoragePressureSeverity::None => DiagnosticsStoragePressureSeverity::None,
        LifecycleStoragePressureSeverity::Background => {
            DiagnosticsStoragePressureSeverity::Background
        }
        LifecycleStoragePressureSeverity::Urgent => DiagnosticsStoragePressureSeverity::Urgent,
        LifecycleStoragePressureSeverity::BlockMutatingAdmission => {
            DiagnosticsStoragePressureSeverity::BlockMutatingAdmission
        }
    }
}

const fn map_storage_pressure_reason(
    reason: LifecycleStoragePressureReason,
) -> DiagnosticsStoragePressureReason {
    match reason {
        LifecycleStoragePressureReason::None => DiagnosticsStoragePressureReason::None,
        LifecycleStoragePressureReason::ActiveMutableBytes => {
            DiagnosticsStoragePressureReason::ActiveMutableBytes
        }
        LifecycleStoragePressureReason::FrozenBacklog => {
            DiagnosticsStoragePressureReason::FrozenBacklog
        }
        LifecycleStoragePressureReason::LevelZeroTableBacklog => {
            DiagnosticsStoragePressureReason::LevelZeroTableBacklog
        }
        LifecycleStoragePressureReason::NonZeroLevelTableBacklog => {
            DiagnosticsStoragePressureReason::NonZeroLevelTableBacklog
        }
        LifecycleStoragePressureReason::InheritedLayerBacklog => {
            DiagnosticsStoragePressureReason::InheritedLayerBacklog
        }
        LifecycleStoragePressureReason::MaintenanceQueueBacklog => {
            DiagnosticsStoragePressureReason::MaintenanceQueueBacklog
        }
    }
}

fn map_wal_growth_report(
    policy: LifecycleWalGrowthPolicy,
    current_facts: Option<WalGrowthFacts>,
    last_status: Option<MaintenanceWalGrowthSummary>,
) -> DiagnosticsWalGrowthReport {
    DiagnosticsWalGrowthReport::known_with_current_retention(
        policy.enabled(),
        Some(policy.max_retained_wal_bytes()),
        Some(policy.max_retained_wal_segments()),
        current_facts.map(WalGrowthFacts::retained_bytes),
        current_facts.map(WalGrowthFacts::retained_segments),
        Some(policy.max_commits_since_checkpoint()),
        last_status,
    )
}

fn map_branch_catalog_report(branches: &[BranchSummary]) -> DiagnosticsBranchCatalogReport {
    let mut active_branches = 0;
    let mut deleted_branches = 0;
    let mut min_generation = None;
    let mut max_generation = None;
    for branch in branches {
        match branch.status() {
            BranchStatus::Active => {
                active_branches += 1;
                min_generation = Some(
                    min_generation.map_or(branch.generation(), |generation: BranchGeneration| {
                        generation.min(branch.generation())
                    }),
                );
                max_generation = Some(
                    max_generation.map_or(branch.generation(), |generation: BranchGeneration| {
                        generation.max(branch.generation())
                    }),
                );
            }
            BranchStatus::Deleted => deleted_branches += 1,
        }
    }
    DiagnosticsBranchCatalogReport::known(
        active_branches,
        deleted_branches,
        min_generation,
        max_generation,
    )
}

fn map_branch_descriptor(descriptor: LifecycleBranchDescriptor) -> BranchSummary {
    let status = match descriptor.status() {
        LifecycleBranchStatus::Active => BranchStatus::Active,
        LifecycleBranchStatus::Deleted => BranchStatus::Deleted,
    };
    let parent = descriptor
        .parent()
        .map(|parent| BranchParentSummary::new(parent.source_branch_id(), parent.fork_version()));
    BranchSummary::new(
        descriptor.branch_id(),
        BranchGeneration::new(descriptor.generation().get()),
        status,
        parent,
        descriptor.created_at(),
        descriptor.deleted_at(),
        descriptor.state_revision(),
    )
}

fn map_branch_cleanup(release_plan: &BranchReleasePlan) -> BranchCleanupSummary {
    BranchCleanupSummary::new(
        release_plan.removed_refs().len(),
        release_plan.releasable_tables().len(),
        release_plan.protected_tables().len(),
    )
}

fn map_commit_summary(
    outcome: &crate::commit::CommitOutcome,
    admission: Option<LifecycleWriteAdmissionOutcome>,
) -> StorageApiResult<CommitSummary> {
    let commit_version = outcome
        .commit_version()
        .ok_or(StorageApiError::InvalidRuntimeState {
            reason: "commit did not allocate a commit version",
        })?;
    let commit_timestamp =
        outcome
            .commit_timestamp()
            .ok_or(StorageApiError::InvalidRuntimeState {
                reason: "commit did not allocate a commit timestamp",
            })?;
    let counts = outcome.mutation_counts();
    Ok(CommitSummary::with_commit_facts(
        outcome.branch_id(),
        commit_version,
        commit_timestamp,
        map_commit_durability(outcome.durability()),
        counts.puts(),
        counts.deletes(),
        counts.timeline_rows(),
        matches!(outcome.kind(), crate::commit::CommitOutcomeKind::Visible),
    )
    .with_admission_summary(map_commit_admission_summary(admission)))
}

const fn map_commit_admission_summary(
    admission: Option<LifecycleWriteAdmissionOutcome>,
) -> CommitAdmissionSummary {
    let Some(admission) = admission else {
        return CommitAdmissionSummary::accepted_clean(
            CommitAdmissionPressureSeverity::None,
            CommitAdmissionPressureReason::None,
            false,
        );
    };
    match admission.status() {
        LifecycleWriteAdmissionStatus::AcceptedClean => CommitAdmissionSummary::accepted_clean(
            map_commit_admission_pressure_severity(admission.pressure().severity()),
            map_commit_admission_pressure_reason(admission.pressure().reason()),
            admission.cleared_prior_rejection(),
        ),
        LifecycleWriteAdmissionStatus::AcceptedUnderPressure => {
            CommitAdmissionSummary::accepted_under_pressure(
                map_commit_admission_pressure_reason(admission.pressure().reason()),
                admission.cleared_prior_rejection(),
            )
            .with_inline_maintenance_driven(admission.inline_maintenance_driven())
        }
    }
}

const fn map_commit_admission_pressure_severity(
    severity: LifecycleStoragePressureSeverity,
) -> CommitAdmissionPressureSeverity {
    match severity {
        LifecycleStoragePressureSeverity::None => CommitAdmissionPressureSeverity::None,
        LifecycleStoragePressureSeverity::Background => CommitAdmissionPressureSeverity::Background,
        LifecycleStoragePressureSeverity::Urgent => CommitAdmissionPressureSeverity::Urgent,
        LifecycleStoragePressureSeverity::BlockMutatingAdmission => {
            CommitAdmissionPressureSeverity::Blocking
        }
    }
}

const fn map_commit_admission_pressure_reason(
    reason: LifecycleStoragePressureReason,
) -> CommitAdmissionPressureReason {
    match reason {
        LifecycleStoragePressureReason::None => CommitAdmissionPressureReason::None,
        LifecycleStoragePressureReason::ActiveMutableBytes => {
            CommitAdmissionPressureReason::ActiveMutableBytes
        }
        LifecycleStoragePressureReason::FrozenBacklog => {
            CommitAdmissionPressureReason::FrozenBacklog
        }
        LifecycleStoragePressureReason::LevelZeroTableBacklog => {
            CommitAdmissionPressureReason::LevelZeroTableBacklog
        }
        LifecycleStoragePressureReason::NonZeroLevelTableBacklog => {
            CommitAdmissionPressureReason::NonZeroLevelTableBacklog
        }
        LifecycleStoragePressureReason::InheritedLayerBacklog => {
            CommitAdmissionPressureReason::InheritedLayerBacklog
        }
        LifecycleStoragePressureReason::MaintenanceQueueBacklog => {
            CommitAdmissionPressureReason::MaintenanceQueueBacklog
        }
    }
}

const fn map_commit_durability(durability: CommitDurabilityClass) -> CommitDurabilitySummary {
    match durability {
        CommitDurabilityClass::NotDurable => CommitDurabilitySummary::NotDurable,
        CommitDurabilityClass::Standard => CommitDurabilitySummary::Standard,
        CommitDurabilityClass::Always => CommitDurabilitySummary::Always,
        CommitDurabilityClass::Uncertain => CommitDurabilitySummary::Uncertain,
    }
}

fn physical_key(
    branch_id: BranchId,
    storage_space: &StorageSpaceId,
    key: &StorageKey,
) -> StorageApiResult<PhysicalKey> {
    PhysicalKey::new(
        branch_id,
        API_PHYSICAL_SPACE,
        map_storage_space(storage_space)?,
        key.as_bytes().to_vec(),
    )
    .map_err(|error| {
        StorageApiError::lower_layer_with(
            StorageApiLowerLayer::Branch,
            "physical key construction failed",
            error,
        )
    })
}

fn map_storage_space(storage_space: &StorageSpaceId) -> StorageApiResult<RowStorageSpaceId> {
    let bytes = storage_space.as_bytes();
    let [raw] = bytes else {
        return Err(StorageApiError::InvalidArgument {
            field: "storage_space",
            reason: "storage space must be a single engine-owned byte",
        });
    };
    RowStorageSpaceId::engine(*raw).map_err(|_| StorageApiError::InvalidArgument {
        field: "storage_space",
        reason: "storage space must use an engine-owned id",
    })
}

fn read_row_from_storage(row: &StorageRow) -> StorageApiResult<StorageReadRow> {
    let storage_space = StorageSpaceId::new(vec![row.physical_key().storage_space_id().raw()])?;
    let key = StorageKey::new(row.physical_key().user_key().to_vec())?;
    let expires_at = (row.expires_at() != Timestamp::EPOCH).then_some(row.expires_at());
    let value = if row.is_tombstone() {
        None
    } else {
        Some(StorageValue::new(row.value().to_vec()))
    };
    Ok(StorageReadRow::new(
        storage_space,
        key,
        value,
        row.commit_version(),
        row.commit_timestamp(),
        expires_at,
        row.is_tombstone(),
    ))
}

fn read_row_from_storage_if_visible(
    row: &StorageRow,
    selected_timestamp: Option<Timestamp>,
) -> StorageApiResult<Option<StorageReadRow>> {
    if row_is_expired_at_selected_frontier(row, selected_timestamp) {
        Ok(None)
    } else {
        read_row_from_storage(row).map(Some)
    }
}

fn row_is_expired_at_selected_frontier(
    row: &StorageRow,
    selected_timestamp: Option<Timestamp>,
) -> bool {
    selected_timestamp.is_some_and(|timestamp| {
        !row.is_tombstone() && row.expires_at() != Timestamp::EPOCH && row.expires_at() <= timestamp
    })
}

fn visible_tombstone_at_bound(
    view: &BranchReadView,
    key: &PhysicalKey,
    resolved: ResolvedReadBound,
) -> StorageApiResult<Option<StorageReadRow>> {
    let rows = view
        .history(key, BranchHistoryOptions::all())
        .map_err(branch_error)?;
    for row in rows {
        if !row_matches_read_bound(row.row(), resolved.branch_bound) {
            continue;
        }
        if row.row().is_tombstone() {
            return read_row_from_storage(row.row()).map(Some);
        }
        return Ok(None);
    }
    Ok(None)
}

fn row_matches_read_bound(row: &StorageRow, bound: BranchReadBound) -> bool {
    match bound {
        BranchReadBound::Latest => true,
        BranchReadBound::AtVersion(version) => row.commit_version() <= version,
        BranchReadBound::AtTimestamp(timestamp) => row.commit_timestamp() <= timestamp,
    }
}

fn map_scan_rows<'a>(
    rows: impl Iterator<Item = &'a StorageRow>,
    limit: Option<ReadLimit>,
    selected_timestamp: Option<Timestamp>,
) -> StorageApiResult<ScanReadOutcome> {
    let mut mapped = Vec::new();
    for row in rows {
        if limit.is_some_and(|limit| mapped.len() >= limit.get()) {
            break;
        }
        if let Some(read_row) = read_row_from_storage_if_visible(row, selected_timestamp)? {
            mapped.push(read_row);
        }
    }
    Ok(ScanReadOutcome::new(mapped))
}

fn require_version_retained(view: &BranchReadView, version: CommitVersion) -> StorageApiResult<()> {
    let timeline = timeline_view_from_read_view(view)?;
    if timeline
        .bounds()
        .min_version()
        .is_some_and(|min_version| version < min_version)
    {
        return Err(StorageApiError::RetainedHistoryUnavailable {
            branch_id: view.branch_id(),
            reason: "commit version is outside retained history",
        });
    }
    Ok(())
}

fn resolve_read_bound(
    view: &BranchReadView,
    bound: ReadBound,
) -> StorageApiResult<ResolvedReadBound> {
    match bound {
        ReadBound::Latest => Ok(ResolvedReadBound {
            branch_bound: BranchReadBound::Latest,
            selected_timestamp: None,
        }),
        ReadBound::AtVersion(version) => {
            let timeline = timeline_view_from_read_view(view)?;
            let selected_timestamp = timeline.timestamp_for_version(version).ok_or(
                StorageApiError::RetainedHistoryUnavailable {
                    branch_id: view.branch_id(),
                    reason: "commit version is outside retained timeline history",
                },
            )?;
            Ok(ResolvedReadBound {
                branch_bound: BranchReadBound::AtVersion(version),
                selected_timestamp: Some(selected_timestamp),
            })
        }
        ReadBound::AtTimestamp(timestamp) => {
            let lookup = timeline_view_from_read_view(view)?.version_at_or_before(timestamp);
            match lookup.miss() {
                CommitTimelineMiss::Matched => Ok(ResolvedReadBound {
                    branch_bound: BranchReadBound::AtVersion(lookup.matched_version().ok_or(
                        StorageApiError::TimestampHistoryUnavailable {
                            branch_id: view.branch_id(),
                            reason: "timestamp lookup did not return a retained version",
                        },
                    )?),
                    selected_timestamp: Some(lookup.matched_timestamp().ok_or(
                        StorageApiError::TimestampHistoryUnavailable {
                            branch_id: view.branch_id(),
                            reason: "timestamp lookup did not return a retained timestamp",
                        },
                    )?),
                }),
                CommitTimelineMiss::BeforeRetainedHistory | CommitTimelineMiss::Empty => {
                    Err(StorageApiError::TimestampHistoryUnavailable {
                        branch_id: view.branch_id(),
                        reason: "timestamp is before retained timeline history",
                    })
                }
                CommitTimelineMiss::AfterLatestRetained => {
                    Err(StorageApiError::TimestampHistoryUnavailable {
                        branch_id: view.branch_id(),
                        reason: "timestamp is after latest retained timeline history",
                    })
                }
            }
        }
    }
}

fn timeline_view_from_read_view(view: &BranchReadView) -> StorageApiResult<CommitTimelineView> {
    // This intentionally rebuilds the timeline from branch rows today. The public
    // boundary should grow a retained timeline index/cache before high-cardinality
    // timestamp reads become a hot path.
    let bounds = BranchScanBounds::unbounded(
        view.branch_id(),
        COMMIT_TIMELINE_SPACE,
        RowStorageSpaceId::COMMIT_TIMELINE,
    )
    .map_err(branch_error)?;
    let timeline_rows = view
        .scan_range_including_tombstones(&bounds, BranchReadBound::Latest)
        .map_err(branch_error)?;
    CommitTimelineView::from_rows(
        view.branch_id(),
        timeline_rows
            .iter()
            .map(crate::branch::read::BranchHistoryRow::row),
    )
    .map_err(commit_error)
}

fn map_api_commit_batch(
    batch: &CommitBatch,
    timestamp_base: Timestamp,
    timestamp_policy: crate::commit::CommitTimestampPolicy,
    durability: crate::commit::CommitDurabilityMode,
) -> StorageApiResult<crate::commit::CommitBatch> {
    let mut mutations = Vec::with_capacity(batch.mutations().len());
    for mutation in batch.mutations() {
        match mutation {
            crate::api::CommitMutation::Put {
                storage_space,
                key,
                value,
                ttl,
            } => mutations.push(crate::commit::CommitMutation::put(
                physical_key(batch.branch_id(), storage_space, key)?,
                value.as_bytes().to_vec(),
                map_expiry(timestamp_base, *ttl)?,
                crate::commit::CommitRetentionHint::Append,
            )),
            crate::api::CommitMutation::Delete { storage_space, key } => {
                mutations.push(crate::commit::CommitMutation::delete(physical_key(
                    batch.branch_id(),
                    storage_space,
                    key,
                )?));
            }
        }
    }

    let mut cas_set = Vec::with_capacity(batch.conditions().len());
    for condition in batch.conditions() {
        let expected = match condition.expected() {
            CommitExpectedVersion::Absent => crate::commit::CommitObservedVersion::Missing,
            CommitExpectedVersion::Present(version) => {
                crate::commit::CommitObservedVersion::Present(version)
            }
        };
        cas_set.push(crate::commit::CommitCasFact::new(
            physical_key(
                batch.branch_id(),
                condition.storage_space(),
                condition.key(),
            )?,
            expected,
        ));
    }

    let conflict_validation =
        if batch.options().conflict_check_required() || !batch.conditions().is_empty() {
            crate::commit::CommitConflictValidationMode::Validate
        } else {
            crate::commit::CommitConflictValidationMode::Skip
        };
    let options = crate::commit::CommitBatchOptions::new(
        durability,
        conflict_validation,
        crate::commit::CommitDuplicateKeyPolicy::Reject,
        timestamp_policy,
        crate::commit::CommitOrigin::StorageRuntime,
    );
    Ok(crate::commit::CommitBatch::mutating(
        batch.branch_id(),
        mutations,
        crate::commit::CommitValidationFacts::new(Vec::new(), cas_set),
        options,
    ))
}

fn map_expiry(
    timestamp: Timestamp,
    ttl: Option<std::time::Duration>,
) -> StorageApiResult<crate::commit::CommitExpiry> {
    let Some(ttl) = ttl else {
        return Ok(crate::commit::CommitExpiry::None);
    };
    if ttl.is_zero() {
        return Err(StorageApiError::InvalidArgument {
            field: "ttl",
            reason: "ttl duration must be greater than zero",
        });
    }
    let ttl_micros =
        u64::try_from(ttl.as_micros()).map_err(|_| StorageApiError::InvalidArgument {
            field: "ttl",
            reason: "ttl duration is too large",
        })?;
    let expires_at = timestamp
        .as_micros()
        .checked_add(ttl_micros)
        .map(Timestamp::from_micros)
        .ok_or(StorageApiError::InvalidArgument {
            field: "ttl",
            reason: "ttl expiration overflows timestamp",
        })?;
    Ok(crate::commit::CommitExpiry::At(expires_at))
}

fn flush_request_for_boundary(branch_id: BranchId) -> StorageApiResult<FlushFrozenRequest> {
    FlushFrozenRequest::new(
        branch_id,
        None,
        FlushTableIdentitySeed::new(format!("storage-boundary-flush-{branch_id}"))
            .map_err(map_lifecycle_error)?,
        FlushTableObjectId::new(format!("storage-boundary-flush-{branch_id}"))
            .map_err(map_lifecycle_error)?,
    )
    .map_err(map_lifecycle_error)
}

fn branch_error(error: crate::branch::error::BranchRuntimeError) -> StorageApiError {
    match error {
        crate::branch::error::BranchRuntimeError::InsufficientTimestampHistory {
            branch_id,
            ..
        } => StorageApiError::TimestampHistoryUnavailable {
            branch_id,
            reason: "timestamp is outside retained branch history",
        },
        other => StorageApiError::lower_layer_with(
            StorageApiLowerLayer::Branch,
            "branch read failed",
            other,
        ),
    }
}

fn commit_error(error: crate::commit::CommitRuntimeError) -> StorageApiError {
    match error {
        crate::commit::CommitRuntimeError::InvalidBatch { reason }
        | crate::commit::CommitRuntimeError::InvalidMutation { reason }
        | crate::commit::CommitRuntimeError::InvalidValidationFacts { reason }
        | crate::commit::CommitRuntimeError::InvalidTimestampPolicy { reason } => {
            StorageApiError::InvalidArgument {
                field: "commit",
                reason,
            }
        }
        crate::commit::CommitRuntimeError::DuplicateMutationKey { .. } => {
            StorageApiError::InvalidArgument {
                field: "mutations",
                reason: "commit batch must not contain duplicate keys",
            }
        }
        crate::commit::CommitRuntimeError::StorageOwnedMutationSpace { .. } => {
            StorageApiError::InvalidArgument {
                field: "storage_space",
                reason: "storage-owned commit spaces are not accepted by the API",
            }
        }
        crate::commit::CommitRuntimeError::BranchNotFound { branch_id } => {
            StorageApiError::BranchNotFound { branch_id }
        }
        crate::commit::CommitRuntimeError::BranchAlreadyExists { branch_id } => {
            StorageApiError::BranchAlreadyExists { branch_id }
        }
        crate::commit::CommitRuntimeError::BranchGenerationMismatch {
            branch_id,
            expected,
            actual,
        } => StorageApiError::BranchGenerationMismatch {
            branch_id,
            expected,
            actual,
        },
        crate::commit::CommitRuntimeError::BranchNotWritable { reason, .. }
        | crate::commit::CommitRuntimeError::BranchGuardUnavailable { reason, .. }
        | crate::commit::CommitRuntimeError::CommitQuiesceUnavailable { reason }
        | crate::commit::CommitRuntimeError::BranchUnavailable { reason } => {
            StorageApiError::InvalidRuntimeState { reason }
        }
        crate::commit::CommitRuntimeError::InvalidCommitPhase {
            reason: "read-only diagnostics are disabled",
        } => StorageApiError::UnsupportedCapability {
            capability: "read_only_diagnostics",
            reason: "read-only diagnostics are disabled",
        },
        crate::commit::CommitRuntimeError::CommitConflict { conflict } => {
            StorageApiError::Conflict {
                branch_id: conflict.branch_id(),
                storage_space: Some(conflict.storage_space_id().raw()),
                key_fingerprint: Some(conflict.key_fingerprint()),
                user_key_len: Some(conflict.user_key_len()),
                reason: "commit condition was not satisfied",
            }
        }
        crate::commit::CommitRuntimeError::DurabilityUnavailable { reason } => {
            StorageApiError::UnsupportedCapability {
                capability: "commit_durability",
                reason,
            }
        }
        crate::commit::CommitRuntimeError::DurabilityUncertain { reason, source, .. }
        | crate::commit::CommitRuntimeError::DurableButNotVisible { reason, source, .. } => {
            StorageApiError::DurableUncertain { reason, source }
        }
        crate::commit::CommitRuntimeError::UnresolvedDurableCommit { reason, .. }
        | crate::commit::CommitRuntimeError::AppliedButNotVisible { reason, .. } => {
            StorageApiError::durable_uncertain(reason)
        }
        crate::commit::CommitRuntimeError::InvalidTimelineFact { .. }
        | crate::commit::CommitRuntimeError::TimelineConflict { .. } => {
            StorageApiError::lower_layer_with(
                StorageApiLowerLayer::Commit,
                "commit timeline facts are invalid",
                error,
            )
        }
        other => StorageApiError::lower_layer_with(
            StorageApiLowerLayer::Commit,
            "commit runtime failed",
            other,
        ),
    }
}

fn map_recovery_health(health: &RecoveryHealth) -> super::RecoveryHealthSummary {
    match health {
        RecoveryHealth::Healthy => super::RecoveryHealthSummary::Healthy,
        RecoveryHealth::Degraded { .. } => super::RecoveryHealthSummary::Degraded,
        RecoveryHealth::Failed { .. } => super::RecoveryHealthSummary::Failed,
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "storage API keeps lifecycle error mapping in one exhaustive registry"
)]
fn map_lifecycle_error(error: LifecycleError) -> StorageApiError {
    match error {
        LifecycleError::InvalidConfig { field, reason } => {
            StorageApiError::InvalidArgument { field, reason }
        }
        LifecycleError::InvalidOpenPlan { reason } => StorageApiError::InvalidArgument {
            field: "open_options",
            reason,
        },
        LifecycleError::InvalidLifecycleState { reason }
        | LifecycleError::PinnedViewReleaseBlocked { reason, .. } => {
            StorageApiError::InvalidRuntimeState { reason }
        }
        LifecycleError::BranchNotFound { branch_id } => {
            StorageApiError::BranchNotFound { branch_id }
        }
        LifecycleError::BranchAlreadyExists { branch_id } => {
            StorageApiError::BranchAlreadyExists { branch_id }
        }
        LifecycleError::BranchGenerationMismatch {
            branch_id,
            expected,
            actual,
        } => StorageApiError::BranchGenerationMismatch {
            branch_id,
            expected,
            actual,
        },
        LifecycleError::BranchNotWritable { state, .. } => {
            StorageApiError::InvalidRuntimeState { reason: state }
        }
        LifecycleError::BranchGenerationExhausted { .. } => StorageApiError::InvalidRuntimeState {
            reason: "branch generation is exhausted",
        },
        LifecycleError::BranchHistoryUnavailable { branch_id, reason } => {
            StorageApiError::RetainedHistoryUnavailable { branch_id, reason }
        }
        LifecycleError::InsufficientTimestampHistory { branch_id, reason } => {
            StorageApiError::TimestampHistoryUnavailable { branch_id, reason }
        }
        LifecycleError::SourceHasUnflushedRows { .. } => StorageApiError::InvalidRuntimeState {
            reason: "source branch has unflushed rows",
        },
        LifecycleError::CapabilityMismatch { .. } => StorageApiError::UnsupportedCapability {
            capability: "backend",
            reason: "backend capabilities do not satisfy storage mode",
        },
        LifecycleError::MaintenanceFailed { reason }
        | LifecycleError::MaintenanceQueueFull { reason }
        | LifecycleError::MaintenanceTaskFailed { reason }
        | LifecycleError::RetentionBlocked { reason }
        | LifecycleError::QuarantineProofBlocked { reason }
        | LifecycleError::PurgeProofBlocked { reason }
        | LifecycleError::WalRetentionProofIncomplete { reason }
        | LifecycleError::FlushPublicationFailed { reason }
        | LifecycleError::CheckpointPublicationFailed { reason }
        | LifecycleError::CheckpointSnapshotOrphaned { reason, .. } => {
            StorageApiError::MaintenanceRejected { reason }
        }
        LifecycleError::StoragePressureRejected {
            branch_id,
            severity,
            pressure_reason,
            retryable,
            reason,
            ..
        } => StorageApiError::StoragePressure {
            branch_id,
            severity: map_commit_admission_pressure_severity(severity),
            pressure_reason: map_commit_admission_pressure_reason(pressure_reason),
            reason,
            retryable,
        },
        LifecycleError::FlushPublicationUncertain { reason, source }
        | LifecycleError::FlushPublicationOrphaned { reason, source, .. }
        | LifecycleError::RewritePublicationUncertain { reason, source, .. }
        | LifecycleError::RewritePublicationOrphaned { reason, source, .. }
        | LifecycleError::TableManifestPublicationUncertain { reason, source } => {
            StorageApiError::DurableUncertain { reason, source }
        }
        LifecycleError::RewritePublicationFailed { reason, source }
        | LifecycleError::TableManifestPublicationFailed { reason, source } => {
            StorageApiError::LowerLayer {
                layer: StorageApiLowerLayer::Lifecycle,
                reason,
                source,
            }
        }
        LifecycleError::LowerLayer {
            layer: crate::lifecycle::LifecycleLowerLayer::CommitRuntime,
            source: Some(source),
            ..
        } => source
            .as_ref()
            .downcast_ref::<crate::commit::CommitRuntimeError>()
            .cloned()
            .map_or_else(
                || {
                    StorageApiError::lower_layer_with(
                        StorageApiLowerLayer::Lifecycle,
                        "lifecycle commit runtime failed",
                        LifecycleError::LowerLayer {
                            layer: crate::lifecycle::LifecycleLowerLayer::CommitRuntime,
                            reason: "commit runtime failed",
                            source: Some(source),
                        },
                    )
                },
                commit_error,
            ),
        other => StorageApiError::lower_layer_with(
            StorageApiLowerLayer::Lifecycle,
            "lifecycle runtime failed",
            other,
        ),
    }
}

fn default_branch_generation() -> StorageApiResult<CommitBranchGeneration> {
    CommitBranchGeneration::new(DEFAULT_BRANCH_GENERATION).map_err(|error| {
        StorageApiError::lower_layer_with(
            StorageApiLowerLayer::Commit,
            "commit branch generation failed",
            error,
        )
    })
}

#[cfg(any(test, feature = "testkit"))]
pub(crate) fn map_commit_error_for_test(
    error: crate::commit::CommitRuntimeError,
) -> StorageApiError {
    commit_error(error)
}

#[cfg(any(test, feature = "testkit"))]
pub(crate) fn map_lifecycle_error_for_test(error: LifecycleError) -> StorageApiError {
    map_lifecycle_error(error)
}

#[cfg(any(test, feature = "testkit"))]
pub(crate) fn map_maintenance_outcome_for_test(
    request: MaintenanceRequest,
    outcome: &LifecycleMaintenanceOutcome,
) -> MaintenanceSummary {
    map_maintenance_summary(request, outcome)
}

const fn default_timestamp_source() -> ApiTimestampSource {
    ApiTimestampSource::new(DEFAULT_TIMESTAMP)
}
