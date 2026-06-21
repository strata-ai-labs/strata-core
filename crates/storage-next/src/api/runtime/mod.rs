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
    ModeLifecyclePolicy, PreparedPublishStep, RealMaintenanceClock, RecoveryDegradationClass,
    RecoveryFaultKind, RecoveryHealth, RecoveryStrictness, StorageBudgetPool,
    StorageBudgetPressureSeverity, StorageBudgetSnapshot, StorageMode as LifecycleStorageMode,
    StorageOpenOutcome as LifecycleStorageOpenOutcome, StorageOpenPlan, StorageRuntimeBudget,
    ThreadedMaintenanceExecutor,
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
    DiagnosticsBudgetAccuracy, DiagnosticsBudgetPool, DiagnosticsBudgetPressure,
    DiagnosticsBudgetReport, DiagnosticsBudgetUsage, DiagnosticsCheckpointReport,
    DiagnosticsOutcome, DiagnosticsQuarantineReport, DiagnosticsReadActivityReport,
    DiagnosticsRecoveryClass, DiagnosticsRecoveryFault, DiagnosticsRecoveryFaultKind,
    DiagnosticsRecoveryReport, DiagnosticsRequest, DiagnosticsRetentionReport, DiagnosticsScope,
    DiagnosticsSourceLayoutReport, DiagnosticsSourceLevelTableCount,
    DiagnosticsStoragePressureReason, DiagnosticsStoragePressureReport,
    DiagnosticsStoragePressureSeverity, DiagnosticsTableReachabilityReport,
    DiagnosticsTimelineReport, DiagnosticsWalGrowthReport, HistoryReadOutcome, HistoryReadRequest,
    ImmutableSourceScanReadOutcome, ImmutableSourceScanReadRequest, MaintenanceDrainSummary,
    MaintenanceQueueSummary, MaintenanceReasonClass, MaintenanceRequest, MaintenanceScope,
    MaintenanceSummary, MaintenanceSummaryStatus, MaintenanceTask, MaintenanceWalGrowthStatus,
    MaintenanceWalGrowthSummary, MaintenanceWalGrowthTrigger, PointReadOutcome, PointReadRequest,
    PrefixScanReadRequest, ReadBound, ReadLimit, RecoveryHealthSummary, ScanReadOutcome,
    ScanReadRequest, StorageApiError, StorageApiErrorClass, StorageApiLowerLayer, StorageApiResult,
    StorageBackend, StorageBackgroundMaintenanceOptions, StorageBudgetPolicy, StorageCloseSummary,
    StorageDurabilityPolicy, StorageKey, StorageMaintenanceSchedulingPolicy, StorageMode,
    StorageOpenDisposition, StorageOpenOptions, StorageOpenOutcome, StorageOpenSummary,
    StorageReadRow, StorageRuntimeState, StorageSpaceId, StorageValue, StorageWalGrowthPolicy,
    TimelineBoundsOutcome, TimelineBoundsRequest, TimestampLookupMiss, TimestampLookupOutcome,
    TimestampLookupRequest, VersionLookupOutcome, VersionLookupRequest,
};
use crate::api::outcome::StorageCloseEffects;
use parking_lot::{Mutex as ParkingMutex, MutexGuard as ParkingMutexGuard};
use std::fmt;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

mod background;
mod data;
mod diagnostics;
mod error;
mod maintenance;
mod open_close;

use background::{
    BackgroundBlockWaitConfig, BackgroundPressureSnapshot, BackgroundWalGrowthSnapshot, RuntimeSlot,
};

use data::{
    flush_request_for_boundary, map_api_commit_batch, map_commit_summary, map_immutable_sources,
    map_scan_rows, map_storage_space, physical_key, read_row_from_storage,
    read_row_from_storage_if_visible, require_version_retained, resolve_read_bound,
    visible_tombstone_at_bound,
};
use diagnostics::{
    branch_for_diagnostics_scope, branch_generation_or_default, current_visible,
    diagnostics_mode_from_plan, diagnostics_pressure_report, diagnostics_source_layout_report,
    durable_checkpoint_report, map_branch_catalog_report, map_branch_cleanup,
    map_branch_descriptor, map_budget_report, map_diagnostics_recovery, map_generation_guard,
    map_wal_growth_report, require_valid_branch_identifier,
};
use error::{branch_error, commit_error, default_branch_generation, map_lifecycle_error};
#[cfg(any(test, feature = "testkit"))]
pub(crate) use error::{
    map_commit_error_for_test, map_lifecycle_error_for_test, map_maintenance_outcome_for_test,
};
use maintenance::{
    background_priority_for_task_request, drain_cache_background_round,
    drain_durable_background_round, map_checkpoint_summary, map_maintenance_queue_summary,
    map_maintenance_summary, map_maintenance_task_request, map_wal_growth_maintenance_summary,
    map_wal_growth_summary, request_for_outcome, run_next_cache_maintenance,
    run_next_durable_maintenance, unsupported_maintenance_summary, validate_maintenance_request,
};
use open_close::{
    background_executor_mode, background_shutdown_panic_error, durable_backend_handle_for_open,
    lifecycle_plan, map_close_summary, map_open_summary, record_background_close_maintenance_facts,
    with_background_close_facts,
};

const DEFAULT_DATABASE_ID: [u8; 16] = [0x53; 16];
const DEFAULT_BRANCH_ID: BranchId = BranchId::from_bytes([0x01; BranchId::BYTE_LEN]);
const DEFAULT_BRANCH_GENERATION: u64 = 1;
const DEFAULT_TIMESTAMP: Timestamp = Timestamp::from_micros(1);
const API_PHYSICAL_SPACE: &str = "api";
const DEFAULT_BACKGROUND_BLOCK_WAIT_SLICE: Duration = Duration::from_millis(250);
const DEFAULT_BACKGROUND_BLOCK_STALL_DEADLINE: Duration = Duration::from_secs(30);
const DEFAULT_BACKGROUND_BLOCK_NO_RELIEF_ROUNDS: usize = 4;
const DEFAULT_BACKGROUND_CLOSE_SHUTDOWN_TIMEOUT: Duration = Duration::from_millis(250);

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

enum DurableBackendHandleForOpen<'a> {
    Borrowed(BackendHandle<'a>),
    #[cfg(feature = "localfs")]
    Owned(BackendHandle<'static>),
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

    /// Open durable local storage at `root` with explicit open options.
    ///
    /// Like [`Self::open_durable_local`], but accepts a full
    /// [`StorageOpenOptions`] so callers can set an explicit memory budget. The
    /// options must select a durable-local mode; other modes are rejected.
    pub fn open_durable_local_with_options(
        root: impl Into<std::path::PathBuf>,
        options: StorageOpenOptions,
    ) -> StorageApiResult<StorageOpenOutcome<'static>> {
        open_durable_local_owned_with_options(root, options)
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
    #[cfg_attr(not(feature = "perf-trace"), allow(dead_code))]
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
    #[cfg_attr(not(feature = "perf-trace"), allow(dead_code))]
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
    #[cfg_attr(not(feature = "perf-trace"), allow(dead_code))]
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
    #[allow(dead_code)]
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
    #[cfg_attr(not(feature = "perf-trace"), allow(dead_code))]
    pub(crate) fn pending_flush_watermark_candidate_for_test(&self) -> Option<CommitVersion> {
        match &self.inner {
            StorageRuntimeInner::Cache(_) | StorageRuntimeInner::Closed => None,
            StorageRuntimeInner::Durable(slot) | StorageRuntimeInner::DurableOwned(slot) => {
                slot.lock().pending_flush_watermark_candidate_for_test()
            }
        }
    }

    #[cfg(test)]
    #[cfg_attr(not(feature = "perf-trace"), allow(dead_code))]
    pub(crate) fn background_now_for_test(&self) -> Option<MaintenanceInstant> {
        self.background_now_for_current_runtime()
    }

    #[cfg(test)]
    #[cfg_attr(not(feature = "perf-trace"), allow(dead_code))]
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
    #[cfg_attr(not(feature = "perf-trace"), allow(dead_code))]
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
    #[cfg_attr(not(feature = "perf-trace"), allow(dead_code))]
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
    #[cfg_attr(not(feature = "perf-trace"), allow(dead_code))]
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
    let mode_policy = runtime.open_plan().lifecycle_policy();
    Ok(StorageOpenOutcome::new(
        StorageRuntime {
            inner: StorageRuntimeInner::DurableOwned(Box::new(
                RuntimeSlot::new_with_background_arc_drain(
                    runtime,
                    config,
                    background_config,
                    executor_mode,
                    mode_policy,
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
    open_durable_local_owned_with_options(root, StorageOpenOptions::durable_local(policy))
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

#[cfg(feature = "localfs")]
fn open_durable_local_owned_with_options(
    root: impl Into<std::path::PathBuf>,
    options: StorageOpenOptions,
) -> StorageApiResult<StorageOpenOutcome<'static>> {
    options.validate()?;
    if !matches!(options.mode(), StorageMode::DurableLocal { .. }) {
        return Err(StorageApiError::InvalidArgument {
            field: "mode",
            reason: "durable local open requires a durable-local mode",
        });
    }
    let backend = StorageBackend::local_fs(root);
    open_durable_with_owned_backend_handle(options, backend.into_backend_handle())
}

#[cfg(not(feature = "localfs"))]
fn open_durable_local_owned_with_options(
    _root: impl Into<std::path::PathBuf>,
    _options: StorageOpenOptions,
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
            map_budget_report(
                &runtime.budget_snapshot(),
                runtime.budget_total_used_bytes(),
                runtime.budget_global_pressure(),
            ),
            diagnostics_pressure_report(
                runtime.branch_catalog(),
                branch_id,
                runtime.maintenance_status(),
                runtime.open_plan().lifecycle_config().storage_budget(),
                runtime.open_plan().lifecycle_policy(),
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
            map_budget_report(
                &runtime.budget_snapshot(),
                runtime.budget_total_used_bytes(),
                runtime.budget_global_pressure(),
            ),
            diagnostics_pressure_report(
                runtime.branch_catalog(),
                branch_id,
                runtime.maintenance_status(),
                runtime.open_plan().lifecycle_config().storage_budget(),
                runtime.open_plan().lifecycle_policy(),
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
        // The Latest fast path does not apply a version lower bound, so route `after_version`
        // reads through the resolving view path (which honors the bound at selection).
        if matches!(request.bound(), ReadBound::Latest) && request.after_version().is_none() {
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
            .scan_prefix_including_tombstones(
                &bounds,
                resolved.branch_bound,
                request.after_version(),
            )
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

    pub fn scan_immutable_sources(
        &self,
        request: &ImmutableSourceScanReadRequest,
    ) -> StorageApiResult<ImmutableSourceScanReadOutcome> {
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
        let view = self.read_view_for_branch(request.branch_id())?;
        let resolved = resolve_read_bound(&view, request.bound())?;
        let sources = view
            .scan_immutable_sources(&bounds, resolved.branch_bound)
            .map_err(branch_error)?;
        Ok(ImmutableSourceScanReadOutcome::new(map_immutable_sources(
            &sources,
            resolved.selected_timestamp,
        )?))
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
        let mode_policy = runtime.open_plan().lifecycle_policy();
        Ok(StorageOpenOutcome::new(
            Self {
                inner: StorageRuntimeInner::Cache(Box::new(
                    RuntimeSlot::new_with_background_arc_drain(
                        runtime,
                        config,
                        background_config,
                        executor_mode,
                        mode_policy,
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
                    self.background_wait_after_wal_growth_enqueue(wal_growth.as_ref());
                    perf_trace::record_api_commit_runtime_elapsed(runtime_timer);
                    return map_commit_summary(&outcome, admission);
                }
                Err(error)
                    if self.background_wait_after_pressure_rejection(
                        &error,
                        &mut pressure_wait_deadline,
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

    /// Advance the injected manual maintenance clock (deterministic-inline runtimes
    /// only); returns whether a manual clock was reached. Test / `fault-injection`-only
    /// — the seam the simulation driver uses to drive time deterministically. Lives in
    /// the lifetime-generic impl so it is callable on a borrowed-backend runtime.
    #[cfg(any(test, feature = "fault-injection"))]
    pub(crate) fn advance_maintenance_clock_for_test(&self, by: std::time::Duration) -> bool {
        self.advance_maintenance_clock_for_current_runtime(by)
    }

    #[cfg(any(test, feature = "fault-injection"))]
    fn advance_maintenance_clock_for_current_runtime(&self, by: std::time::Duration) -> bool {
        match &self.inner {
            StorageRuntimeInner::Cache(slot) => slot.advance_maintenance_clock(by),
            StorageRuntimeInner::Durable(slot) => slot.advance_maintenance_clock(by),
            StorageRuntimeInner::DurableOwned(slot) => slot.advance_maintenance_clock(by),
            StorageRuntimeInner::Closed => false,
        }
    }

    fn background_wait_after_pressure_rejection(
        &mut self,
        error: &LifecycleError,
        deadline: &mut Option<MaintenanceInstant>,
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
        // Ensure the maintenance this pressure needs is enqueued before we wait
        // on it (forced flush for FrozenBacklog, forced compaction for
        // LevelZeroTableBacklog); the writer is then paced on its progress
        // rather than rejected for lack of immediately-visible work.
        self.enqueue_pressure_maintenance_for_background_wait(*branch_id, *pressure_reason);
        let Some(stats_before_wait) = self.background_stats_for_current_runtime() else {
            return false;
        };
        let pressure_before_wait = self.background_pressure_snapshot_for_branch(*branch_id);
        let completed_before_wait = stats_before_wait.tasks_completed;
        let lifecycle_completed_before = self.background_lifecycle_completed_for_current_runtime();
        let wait_start = self.background_now_for_current_runtime().unwrap_or(now);
        self.notify_background_drain_for_current_runtime(BackgroundTaskPriority::High);
        // Drive the background drain for one bounded slice (and advance the
        // manual clock under deterministic simulation). The executor-level
        // "progressed" flag is intentionally discarded: it reports true when the
        // executor is merely idle, which is not real maintenance progress. The
        // watchdog reset below is gated on the lifecycle maintenance completion
        // count and the pressure snapshot instead.
        let _drove_drain = match &self.inner {
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
        let backlog_reduced = pressure_before_wait
            .zip(pressure_after_wait)
            .is_some_and(|(before, after)| after.relieved_since(before, *pressure_reason));
        let maintenance_completed_task =
            self.background_lifecycle_completed_for_current_runtime() > lifecycle_completed_before;
        if backlog_reduced || maintenance_completed_task {
            // The executor is alive and making real maintenance progress (it
            // completed a maintenance task, or the backlog shrank this slice).
            // Reset the stall watchdog so a sustained overload that maintenance
            // can service keeps pacing the writer instead of timing out on an
            // absolute clock. The top-of-function `now >= stall_deadline` check
            // then fires only after a full window with zero maintenance
            // completions and no backlog reduction — a provably dead or stuck
            // executor (the bounded liveness backstop).
            *deadline = None;
            perf_trace::record_lifecycle_write_admission_wait_progress_reset();
        }
        // Keep pacing the writer in wait-slices; the sole give-up is the
        // top-of-function watchdog. Backlog that maintenance is still working
        // through is throttled, never converted into a rejection.
        true
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
                // Symmetric to the forced flush above: an L0 backlog that blocks
                // admission must have its L0->L1 compaction enqueued before the
                // wait path can give up, so the writer is paced on real
                // maintenance progress rather than rejected for lack of a task.
                if pressure_reason == LifecycleStoragePressureReason::LevelZeroTableBacklog
                    && runtime.maintenance_status().pending_tasks() == 0
                {
                    let _ = runtime.enqueue_maintenance(
                        LifecycleMaintenanceTaskRequest::compaction(branch_id, 0),
                    );
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
                // Symmetric to the forced flush above: an L0 backlog that blocks
                // admission must have its L0->L1 compaction enqueued before the
                // wait path can give up, so the writer is paced on real
                // maintenance progress rather than rejected for lack of a task.
                if pressure_reason == LifecycleStoragePressureReason::LevelZeroTableBacklog
                    && runtime.maintenance_status().pending_tasks() == 0
                {
                    let _ = runtime.enqueue_maintenance(
                        LifecycleMaintenanceTaskRequest::compaction(branch_id, 0),
                    );
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

const fn default_timestamp_source() -> ApiTimestampSource {
    ApiTimestampSource::new(DEFAULT_TIMESTAMP)
}
