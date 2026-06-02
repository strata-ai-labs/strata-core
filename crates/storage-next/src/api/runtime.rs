//! API runtime handle.

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
    collect_storage_pressure, CloseOutcome, CloseOutcomeStatus, FlushFrozenRequest,
    FlushTableIdentitySeed, FlushTableObjectId, LifecycleBranchCatalog, LifecycleBranchDescriptor,
    LifecycleBranchStatus, LifecycleCacheOpenRequest, LifecycleCacheRuntime,
    LifecycleCheckpointOutcome, LifecycleCodecId, LifecycleCompactionRequest, LifecycleConfig,
    LifecycleDurableLocalOpenRequest, LifecycleDurableLocalRuntime, LifecycleDurableLocalShell,
    LifecycleError, LifecycleRecoveryRuntime, LifecycleRetentionRequest, LifecycleRetentionScope,
    LifecycleStoragePressureReason, LifecycleStoragePressureSeverity, LifecycleWalGrowthOutcome,
    LifecycleWalGrowthPolicy, LifecycleWalGrowthStatus, LifecycleWalGrowthTrigger,
    MaintenanceCheckpointOptions, MaintenanceExecutorStatus,
    MaintenanceOutcome as LifecycleMaintenanceOutcome,
    MaintenanceOutcomeReasonClass as LifecycleMaintenanceOutcomeReasonClass,
    MaintenanceOutcomeStatus as LifecycleMaintenanceOutcomeStatus,
    MaintenanceTaskKind as LifecycleMaintenanceTaskKind,
    MaintenanceTaskPolicy as LifecycleMaintenanceTaskPolicy,
    MaintenanceTaskPriority as LifecycleMaintenanceTaskPriority,
    MaintenanceTaskRequest as LifecycleMaintenanceTaskRequest,
    MaintenanceTaskScope as LifecycleMaintenanceTaskScope, RecoveryDegradationClass,
    RecoveryFaultKind, RecoveryHealth, RecoveryStrictness, StorageBudgetPool,
    StorageBudgetPressureSeverity, StorageBudgetSnapshot, StorageMode as LifecycleStorageMode,
    StorageOpenOutcome as LifecycleStorageOpenOutcome, StorageOpenPlan, StorageRuntimeBudget,
};
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId as RowStorageSpaceId};
use crate::service::WalServiceConfig;
use strata_core_next::{BranchId, CommitVersion, Timestamp};

use super::{
    BranchAction, BranchCleanupSummary, BranchGeneration, BranchOperation, BranchOutcome,
    BranchParentSummary, BranchRequest, BranchStatus, BranchSummary, CommitBatch, CommitDurability,
    CommitDurabilitySummary, CommitExpectedVersion, CommitSummary, DiagnosticsBranchCatalogReport,
    DiagnosticsBudgetPool, DiagnosticsBudgetPressure, DiagnosticsBudgetReport,
    DiagnosticsBudgetUsage, DiagnosticsCheckpointReport, DiagnosticsOutcome,
    DiagnosticsQuarantineReport, DiagnosticsReadActivityReport, DiagnosticsRecoveryClass,
    DiagnosticsRecoveryFault, DiagnosticsRecoveryFaultKind, DiagnosticsRecoveryReport,
    DiagnosticsRequest, DiagnosticsRetentionReport, DiagnosticsScope,
    DiagnosticsStoragePressureReason, DiagnosticsStoragePressureReport,
    DiagnosticsStoragePressureSeverity, DiagnosticsTableReachabilityReport,
    DiagnosticsTimelineReport, DiagnosticsWalGrowthReport, HistoryReadOutcome, HistoryReadRequest,
    MaintenanceDrainSummary, MaintenanceQueueSummary, MaintenanceReasonClass, MaintenanceRequest,
    MaintenanceScope, MaintenanceSummary, MaintenanceSummaryStatus, MaintenanceTask,
    MaintenanceWalGrowthStatus, MaintenanceWalGrowthSummary, MaintenanceWalGrowthTrigger,
    PointReadOutcome, PointReadRequest, PrefixScanReadRequest, ReadBound, ReadLimit,
    RecoveryHealthSummary, ScanReadOutcome, ScanReadRequest, StorageApiError, StorageApiErrorClass,
    StorageApiLowerLayer, StorageApiResult, StorageBackend, StorageBudgetPolicy,
    StorageCloseSummary, StorageDurabilityPolicy, StorageKey, StorageMode, StorageOpenDisposition,
    StorageOpenOptions, StorageOpenOutcome, StorageOpenSummary, StorageReadRow,
    StorageRuntimeState, StorageSpaceId, StorageValue, StorageWalGrowthPolicy,
    TimelineBoundsOutcome, TimelineBoundsRequest, TimestampLookupMiss, TimestampLookupOutcome,
    TimestampLookupRequest, VersionLookupOutcome, VersionLookupRequest,
};
use crate::api::outcome::StorageCloseEffects;
use std::time::Duration;

const DEFAULT_DATABASE_ID: [u8; 16] = [0x53; 16];
const DEFAULT_BRANCH_ID: BranchId = BranchId::from_bytes([0x01; BranchId::BYTE_LEN]);
const DEFAULT_BRANCH_GENERATION: u64 = 1;
const DEFAULT_TIMESTAMP: Timestamp = Timestamp::from_micros(1);
const API_PHYSICAL_SPACE: &str = "api";

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
    Cache(Box<LifecycleCacheRuntime<ApiTimestampSource>>),
    Durable(Box<LifecycleDurableLocalRuntime<'a, ApiTimestampSource>>),
    Closed,
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct StorageCloseOptions {
    _private: (),
}

impl StorageCloseOptions {
    #[must_use]
    pub const fn graceful() -> Self {
        Self { _private: () }
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

    /// Open a cache-mode runtime for cache-specific tests and previews.
    pub fn open_cache() -> StorageApiResult<StorageOpenOutcome<'static>> {
        Self::open(StorageOpenOptions::cache())
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
}

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
            StorageMode::DurableLocal { .. } => Self::open_durable_with_backend(options, backend),
            StorageMode::ObjectDurableCandidate | StorageMode::DistributedCandidate => {
                unreachable!("unsupported modes are rejected during validation")
            }
        }
    }

    #[must_use]
    pub const fn state(&self) -> StorageRuntimeState {
        match self.inner {
            StorageRuntimeInner::Cache(_) | StorageRuntimeInner::Durable(_) => {
                StorageRuntimeState::Open
            }
            StorageRuntimeInner::Closed => StorageRuntimeState::Closed,
        }
    }

    #[must_use]
    pub const fn is_open(&self) -> bool {
        matches!(
            self.inner,
            StorageRuntimeInner::Cache(_) | StorageRuntimeInner::Durable(_)
        )
    }

    pub fn close(&mut self) -> StorageApiResult<StorageCloseSummary> {
        self.close_with_options(StorageCloseOptions::graceful())
    }

    pub fn close_with_options(
        &mut self,
        _options: StorageCloseOptions,
    ) -> StorageApiResult<StorageCloseSummary> {
        match &mut self.inner {
            StorageRuntimeInner::Cache(runtime) => {
                let recovery = map_diagnostics_recovery(runtime.open_outcome().recovery_health());
                let close = runtime.close().map_err(map_lifecycle_error)?;
                let summary = map_close_summary(close, false);
                self.inner = StorageRuntimeInner::Closed;
                self.last_recovery = Some(recovery);
                self.last_close = Some(summary);
                Ok(summary)
            }
            StorageRuntimeInner::Durable(runtime) => {
                let close = runtime.close().map_err(map_lifecycle_error)?;
                let recovery = map_diagnostics_recovery(runtime.current_recovery_health());
                let summary = map_close_summary(close, false);
                self.inner = StorageRuntimeInner::Closed;
                self.last_recovery = Some(recovery);
                self.last_close = Some(summary);
                Ok(summary)
            }
            StorageRuntimeInner::Closed => {
                let summary = self.last_close.unwrap_or_else(|| {
                    StorageCloseSummary::with_close_facts(
                        StorageRuntimeState::Closed,
                        true,
                        StorageCloseEffects::empty(),
                    )
                });
                let idempotent =
                    StorageCloseSummary::with_close_facts(summary.state(), true, summary.effects());
                Ok(idempotent)
            }
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
            StorageRuntimeInner::Cache(runtime) => {
                Ok(map_maintenance_queue_summary(runtime.maintenance_status()))
            }
            StorageRuntimeInner::Durable(runtime) => {
                Ok(map_maintenance_queue_summary(runtime.maintenance_status()))
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
        runtime: &LifecycleCacheRuntime<S>,
    ) -> StorageApiResult<DiagnosticsOutcome> {
        let branch_id = branch_for_diagnostics_scope(request.scope());
        let branches = self.list_branches(true)?;
        let wal_growth = runtime.evaluate_wal_growth_policy();
        Ok(DiagnosticsOutcome::new(
            request.scope(),
            StorageRuntimeState::Open,
            Some(diagnostics_mode_from_plan(
                self.open_summary,
                runtime.open_plan(),
            )),
            current_visible(self),
            map_diagnostics_recovery(runtime.open_outcome().recovery_health()),
            Some(map_maintenance_queue_summary(runtime.maintenance_status())),
            map_budget_report(&runtime.budget_snapshot()),
            diagnostics_pressure_report(
                runtime.branch_catalog(),
                branch_id,
                runtime.maintenance_status(),
            ),
            DiagnosticsReadActivityReport::unknown(),
            DiagnosticsTableReachabilityReport::unsupported(),
            DiagnosticsRetentionReport::unsupported(),
            DiagnosticsQuarantineReport::unsupported(),
            DiagnosticsCheckpointReport::unsupported(),
            map_wal_growth_report(
                runtime.open_plan().lifecycle_config().wal_growth_policy(),
                Some(map_wal_growth_summary(&wal_growth)),
            ),
            map_branch_catalog_report(&branches),
            self.diagnostics_timeline(branch_id),
        ))
    }

    fn durable_diagnostics<S>(
        &self,
        request: DiagnosticsRequest,
        runtime: &LifecycleDurableLocalRuntime<'_, S>,
    ) -> StorageApiResult<DiagnosticsOutcome> {
        let branch_id = branch_for_diagnostics_scope(request.scope());
        let branches = self.list_branches(true)?;
        let table_catalog = runtime.table_catalog();
        Ok(DiagnosticsOutcome::new(
            request.scope(),
            StorageRuntimeState::Open,
            Some(diagnostics_mode_from_plan(
                self.open_summary,
                runtime.open_plan(),
            )),
            current_visible(self),
            map_diagnostics_recovery(runtime.current_recovery_health()),
            Some(map_maintenance_queue_summary(runtime.maintenance_status())),
            map_budget_report(&runtime.budget_snapshot()),
            diagnostics_pressure_report(
                runtime.branch_catalog(),
                branch_id,
                runtime.maintenance_status(),
            ),
            DiagnosticsReadActivityReport::unknown(),
            DiagnosticsTableReachabilityReport::known(
                table_catalog.entry_count(),
                table_catalog.object_count(),
                Some(table_catalog.next_manifest_sequence()),
            ),
            DiagnosticsRetentionReport::known(None, Some(runtime.pending_releases().len()), None),
            DiagnosticsQuarantineReport::unknown(),
            durable_checkpoint_report(runtime),
            map_wal_growth_report(
                runtime.open_plan().lifecycle_config().wal_growth_policy(),
                runtime
                    .last_wal_growth_outcome()
                    .map(map_wal_growth_summary),
            ),
            map_branch_catalog_report(&branches),
            self.diagnostics_timeline(branch_id),
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
            StorageRuntimeInner::Cache(runtime) => {
                runtime
                    .enqueue_maintenance(task)
                    .map_err(map_lifecycle_error)?;
                Ok(map_maintenance_queue_summary(runtime.maintenance_status()))
            }
            StorageRuntimeInner::Durable(runtime) => {
                runtime
                    .enqueue_maintenance(task)
                    .map_err(map_lifecycle_error)?;
                Ok(map_maintenance_queue_summary(runtime.maintenance_status()))
            }
            StorageRuntimeInner::Closed => Err(StorageApiError::InvalidRuntimeState {
                reason: "maintenance enqueue requires an open runtime",
            }),
        }
    }

    pub fn run_next_maintenance(&mut self) -> StorageApiResult<Option<MaintenanceSummary>> {
        let outcome = match &mut self.inner {
            StorageRuntimeInner::Cache(runtime) => run_next_cache_maintenance(runtime)?,
            StorageRuntimeInner::Durable(runtime) => run_next_durable_maintenance(runtime)?,
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
        let view = self.read_view_for_branch(request.branch_id())?;
        let prefix = physical_key(
            request.branch_id(),
            request.storage_space(),
            request.prefix(),
        )?;
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
        let view = self.read_view_for_branch(request.branch_id())?;
        let storage_space = map_storage_space(request.storage_space())?;
        let resolved = resolve_read_bound(&view, request.bound())?;
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
            StorageRuntimeInner::Durable(runtime) => {
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
            StorageRuntimeInner::Cache(runtime) => {
                runtime
                    .rotate_active_for_branch_for_maintenance(branch_id)
                    .map_err(map_lifecycle_error)?;
                runtime.flush_frozen(&flush_request)
            }
            StorageRuntimeInner::Durable(runtime) => {
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
        Ok(
            map_maintenance_summary(*request, &outcome.maintenance_outcome())
                .with_rows_processed(outcome.rows_flushed()),
        )
    }

    fn compaction_maintenance(
        &mut self,
        request: &MaintenanceRequest,
    ) -> StorageApiResult<MaintenanceSummary> {
        let branch_id = self.branch_for_maintenance_scope(request.scope())?;
        let compaction = LifecycleCompactionRequest::new(
            branch_id,
            crate::branch::state::compaction::BranchCompactionKind::CompactL0ToLevelOne,
            format!("storage-boundary-compaction-{branch_id}"),
        )
        .map_err(map_lifecycle_error)?;
        let outcome = match &mut self.inner {
            StorageRuntimeInner::Cache(runtime) => runtime.compact_branch_tables(&compaction),
            StorageRuntimeInner::Durable(runtime) => runtime.compact_branch_tables(&compaction),
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

    fn materialization_maintenance(
        &mut self,
        request: &MaintenanceRequest,
    ) -> StorageApiResult<MaintenanceSummary> {
        let branch_id = self.branch_for_maintenance_scope(request.scope())?;
        let task = LifecycleMaintenanceTaskRequest::materialization(branch_id);
        let outcome = match &mut self.inner {
            StorageRuntimeInner::Cache(runtime) => {
                let enqueue = runtime
                    .enqueue_maintenance(task)
                    .map_err(map_lifecycle_error)?;
                runtime
                    .run_materialization_maintenance_task(enqueue.task_id())
                    .map_err(map_lifecycle_error)?
            }
            StorageRuntimeInner::Durable(runtime) => {
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
            StorageRuntimeInner::Durable(runtime) => {
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
            StorageRuntimeInner::Durable(runtime) => {
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
            StorageRuntimeInner::Durable(runtime) => {
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
            StorageRuntimeInner::Durable(runtime) => {
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
            StorageRuntimeInner::Durable(runtime) => {
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
            StorageRuntimeInner::Durable(runtime) => {
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
            StorageRuntimeInner::Cache(runtime) => Ok(runtime.evaluate_wal_growth_policy()),
            StorageRuntimeInner::Durable(runtime) => runtime.evaluate_wal_growth_policy(),
            StorageRuntimeInner::Closed => {
                return Err(StorageApiError::InvalidRuntimeState {
                    reason: "WAL growth maintenance requires an open runtime",
                });
            }
        }
        .map_err(map_lifecycle_error)?;
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
            StorageRuntimeInner::Cache(runtime) => {
                runtime.create_branch(branch_id, generation, created_at)
            }
            StorageRuntimeInner::Durable(runtime) => {
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
            StorageRuntimeInner::Cache(runtime) => runtime.list_branches(include_deleted),
            StorageRuntimeInner::Durable(runtime) => runtime.list_branches(include_deleted),
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
            StorageRuntimeInner::Cache(runtime) => runtime.branch_catalog().lookup(branch_id),
            StorageRuntimeInner::Durable(runtime) => runtime.branch_catalog().lookup(branch_id),
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
            StorageRuntimeInner::Cache(runtime) => runtime.fork_at_retained_version(
                source,
                request.branch_id(),
                generation,
                version,
                retained_floor,
            ),
            StorageRuntimeInner::Durable(runtime) => runtime.fork_at_retained_version(
                source,
                request.branch_id(),
                generation,
                version,
                retained_floor,
            ),
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
            StorageRuntimeInner::Cache(runtime) => runtime.clear_branch(request.branch_id(), guard),
            StorageRuntimeInner::Durable(runtime) => {
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
            StorageRuntimeInner::Cache(runtime) => {
                runtime.delete_branch(request.branch_id(), guard, deleted_at)
            }
            StorageRuntimeInner::Durable(runtime) => {
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
        let summary = map_open_summary(runtime.open_outcome(), options.mode());
        let recovery = map_diagnostics_recovery(runtime.open_outcome().recovery_health());
        Ok(StorageOpenOutcome::new(
            Self {
                inner: StorageRuntimeInner::Cache(Box::new(runtime)),
                open_summary: Some(summary),
                last_recovery: Some(recovery),
                last_close: None,
            },
            summary,
        ))
    }

    fn open_durable_with_backend(
        options: StorageOpenOptions,
        backend: &'a StorageBackend,
    ) -> StorageApiResult<StorageOpenOutcome<'a>> {
        let plan = lifecycle_plan(options)?;
        let request = LifecycleDurableLocalOpenRequest::new(
            plan,
            DEFAULT_DATABASE_ID,
            DEFAULT_BRANCH_ID,
            default_branch_generation()?,
            BranchRuntimeConfig::default(),
            CommitRuntimeConfig::default(),
            WalServiceConfig::default(),
        )
        .map_err(map_lifecycle_error)?;
        let mut shell = LifecycleDurableLocalShell::assemble(
            request,
            backend.as_backend(),
            default_timestamp_source(),
        )
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
        let summary = map_open_summary(runtime.open_outcome(), options.mode());
        let recovery_report = map_diagnostics_recovery(runtime.current_recovery_health());
        Ok(StorageOpenOutcome::new(
            Self {
                inner: StorageRuntimeInner::Durable(Box::new(runtime)),
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
            StorageRuntimeInner::Durable(runtime) => runtime.release_writer_guard_for_test(),
            StorageRuntimeInner::Cache(_) | StorageRuntimeInner::Closed => false,
        }
    }

    fn read_view_for_branch(&self, branch_id: BranchId) -> StorageApiResult<BranchReadView> {
        match &self.inner {
            StorageRuntimeInner::Cache(runtime) => runtime
                .read_view_for_branch(branch_id)
                .map_err(map_lifecycle_error),
            StorageRuntimeInner::Durable(runtime) => runtime
                .read_view_for_branch(branch_id)
                .map_err(map_lifecycle_error),
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
    pub(crate) fn record_recovery_health_for_test(
        &mut self,
        health: &RecoveryHealth,
    ) -> StorageApiResult<()> {
        match &mut self.inner {
            StorageRuntimeInner::Durable(runtime) => {
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
        let runtime_batch =
            map_api_commit_batch(batch, timestamp_base, timestamp_policy, durability)?;
        let outcome = match &mut self.inner {
            StorageRuntimeInner::Cache(runtime) => {
                runtime.execute_cache_commit(runtime_batch, generation_guard)
            }
            StorageRuntimeInner::Durable(runtime) => {
                runtime.execute_durable_commit(runtime_batch, generation_guard)
            }
            StorageRuntimeInner::Closed => {
                return Err(StorageApiError::InvalidRuntimeState {
                    reason: "commit requires an open runtime",
                });
            }
        }
        .map_err(map_lifecycle_error)?;
        map_commit_summary(&outcome)
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
            StorageRuntimeInner::Durable(runtime) => {
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
            StorageRuntimeInner::Cache(runtime) => {
                runtime.allocator().timestamp_guard().last_allocated()
            }
            StorageRuntimeInner::Durable(runtime) => {
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
            StorageRuntimeInner::Cache(runtime) => {
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
            StorageRuntimeInner::Durable(runtime) => {
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
            StorageRuntimeInner::Cache(runtime) => runtime
                .fork_current(DEFAULT_BRANCH_ID, destination, destination_generation)
                .map(|_| ())
                .map_err(map_lifecycle_error),
            StorageRuntimeInner::Durable(runtime) => runtime
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
    pub(crate) fn flush_branch_for_test(&mut self, branch_id: BranchId) -> StorageApiResult<()> {
        match &mut self.inner {
            StorageRuntimeInner::Cache(runtime) => {
                runtime
                    .rotate_active_for_maintenance()
                    .map_err(map_lifecycle_error)?;
                runtime
                    .flush_frozen(&flush_request_for_boundary(branch_id)?)
                    .map(|_| ())
                    .map_err(map_lifecycle_error)
            }
            StorageRuntimeInner::Durable(runtime) => {
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
            StorageRuntimeInner::Cache(runtime) => runtime
                .branch_catalog_mut_for_test()
                .pin_reachability(branch_id)
                .map(|_| ())
                .map_err(map_lifecycle_error),
            StorageRuntimeInner::Durable(runtime) => runtime
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
            StorageRuntimeInner::Cache(runtime) => {
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
            StorageRuntimeInner::Durable(runtime) => {
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
    StorageOpenPlan::new(mode, LifecycleCodecId::identity(), recovery, config)
        .map_err(map_lifecycle_error)
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

fn map_open_summary(
    outcome: &LifecycleStorageOpenOutcome,
    mode: StorageMode,
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
        LifecycleWalGrowthStatus::CheckpointEnqueued => {
            MaintenanceWalGrowthStatus::CheckpointEnqueued
        }
        LifecycleWalGrowthStatus::CheckpointCoalesced => {
            MaintenanceWalGrowthStatus::CheckpointCoalesced
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
) -> MaintenanceQueueSummary {
    let stats = executor_status.stats();
    MaintenanceQueueSummary::new(
        executor_status.pending_tasks(),
        executor_status
            .active_task()
            .map(crate::lifecycle::MaintenanceTaskId::get),
        stats.enqueued(),
        stats.coalesced(),
        stats.started(),
        stats.completed(),
        stats.deferred(),
        stats.failed(),
        stats.canceled(),
        stats.drained(),
        stats.queue_full(),
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

fn run_next_cache_maintenance(
    runtime: &mut LifecycleCacheRuntime<ApiTimestampSource>,
) -> StorageApiResult<Option<LifecycleMaintenanceOutcome>> {
    if let Some(outcome) = runtime
        .run_next_flush_maintenance()
        .map_err(map_lifecycle_error)?
    {
        return Ok(Some(outcome));
    }
    if let Some(outcome) = runtime
        .run_next_compaction_maintenance()
        .map_err(map_lifecycle_error)?
    {
        return Ok(Some(outcome));
    }
    runtime
        .run_next_materialization_maintenance()
        .map_err(map_lifecycle_error)
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
        .run_next_wal_truncation_maintenance()
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
        .run_next_compaction_maintenance()
        .map_err(map_lifecycle_error)?
    {
        return Ok(Some(outcome));
    }
    if let Some(outcome) = runtime
        .run_next_materialization_maintenance()
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

fn current_visible(runtime: &StorageRuntime<'_>) -> Option<CommitVersion> {
    let version = match &runtime.inner {
        StorageRuntimeInner::Cache(runtime) => runtime.visible_version(),
        StorageRuntimeInner::Durable(runtime) => runtime.visible_version(),
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
) -> DiagnosticsStoragePressureReport {
    let Ok(branch) = catalog.branch_state(branch_id) else {
        return DiagnosticsStoragePressureReport::unknown();
    };
    let pressure = collect_storage_pressure(branch, maintenance);
    DiagnosticsStoragePressureReport::known(
        branch_id,
        map_storage_pressure_severity(pressure.severity()),
        map_storage_pressure_reason(pressure.reason()),
        pressure.active_rows(),
        pressure.frozen_tables(),
        pressure.level_zero_tables(),
        pressure.owned_tables(),
        pressure.inherited_layers(),
        pressure.pending_maintenance(),
    )
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
        LifecycleStoragePressureReason::FrozenBacklog => {
            DiagnosticsStoragePressureReason::FrozenBacklog
        }
        LifecycleStoragePressureReason::LevelZeroTableBacklog => {
            DiagnosticsStoragePressureReason::LevelZeroTableBacklog
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
    last_status: Option<MaintenanceWalGrowthSummary>,
) -> DiagnosticsWalGrowthReport {
    DiagnosticsWalGrowthReport::known(
        policy.enabled(),
        Some(policy.max_retained_wal_bytes()),
        Some(policy.max_retained_wal_segments()),
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

fn map_commit_summary(outcome: &crate::commit::CommitOutcome) -> StorageApiResult<CommitSummary> {
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
    ))
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
