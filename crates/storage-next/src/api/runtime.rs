//! API runtime handle.

use crate::branch::{
    BranchHistoryOptions, BranchReadBound, BranchReadView, BranchRuntimeConfig, BranchScanBounds,
    BranchUserKeyBound,
};
#[cfg(any(test, feature = "testkit"))]
use crate::commit::CommitBranchGenerationGuard;
use crate::commit::{
    CommitBranchGeneration, CommitManualTimestampSource, CommitRuntimeConfig, CommitTimelineMiss,
    CommitTimelineView, COMMIT_TIMELINE_SPACE,
};
use crate::lifecycle::{
    CloseOutcome, CloseOutcomeStatus, LifecycleCacheOpenRequest, LifecycleCacheRuntime,
    LifecycleCodecId, LifecycleConfig, LifecycleDurableLocalOpenRequest,
    LifecycleDurableLocalRuntime, LifecycleDurableLocalShell, LifecycleError,
    LifecycleRecoveryRuntime, LifecycleWalGrowthPolicy, RecoveryHealth, RecoveryStrictness,
    StorageMode as LifecycleStorageMode, StorageOpenOutcome as LifecycleStorageOpenOutcome,
    StorageOpenPlan, StorageRuntimeBudget,
};
#[cfg(test)]
use crate::lifecycle::{FlushFrozenRequest, FlushTableIdentitySeed, FlushTableObjectId};
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId as RowStorageSpaceId};
use crate::service::WalServiceConfig;
use strata_core_next::{BranchId, CommitVersion, Timestamp};

use super::{
    HistoryReadOutcome, HistoryReadRequest, PointReadOutcome, PointReadRequest,
    PrefixScanReadRequest, ReadBound, ReadLimit, ScanReadOutcome, ScanReadRequest, StorageApiError,
    StorageApiLowerLayer, StorageApiResult, StorageBackend, StorageBudgetPolicy,
    StorageCloseSummary, StorageDurabilityPolicy, StorageKey, StorageMode, StorageOpenDisposition,
    StorageOpenOptions, StorageOpenOutcome, StorageOpenSummary, StorageReadRow,
    StorageRuntimeState, StorageSpaceId, StorageValue, StorageWalGrowthPolicy,
    TimelineBoundsOutcome, TimelineBoundsRequest, TimestampLookupMiss, TimestampLookupOutcome,
    TimestampLookupRequest, VersionLookupOutcome, VersionLookupRequest,
};
use crate::api::outcome::StorageCloseEffects;

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
    last_close: Option<StorageCloseSummary>,
}

#[derive(Debug)]
enum StorageRuntimeInner<'a> {
    Cache(Box<LifecycleCacheRuntime>),
    Durable(Box<LifecycleDurableLocalRuntime<'a>>),
    Closed,
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
            last_close: None,
        }
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
                let close = runtime.close().map_err(map_lifecycle_error)?;
                let summary = map_close_summary(close, false);
                self.inner = StorageRuntimeInner::Closed;
                self.last_close = Some(summary);
                Ok(summary)
            }
            StorageRuntimeInner::Durable(runtime) => {
                let close = runtime.close().map_err(map_lifecycle_error)?;
                let summary = map_close_summary(close, false);
                self.inner = StorageRuntimeInner::Closed;
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
            rows.iter().map(crate::branch::BranchHistoryRow::row),
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
            rows.iter().map(crate::branch::BranchHistoryRow::row),
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
        Ok(StorageOpenOutcome::new(
            Self {
                inner: StorageRuntimeInner::Cache(Box::new(runtime)),
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
        Ok(StorageOpenOutcome::new(
            Self {
                inner: StorageRuntimeInner::Durable(Box::new(runtime)),
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
                .map(crate::branch::BranchHistoryRow::row),
        )
        .map_err(commit_error)
    }

    #[cfg(test)]
    pub(crate) const fn default_branch_id_for_test() -> BranchId {
        DEFAULT_BRANCH_ID
    }

    #[cfg(any(test, feature = "testkit"))]
    pub(crate) fn commit_for_test(
        &mut self,
        batch: &super::CommitBatch,
        timestamp: Timestamp,
    ) -> StorageApiResult<super::CommitSummary> {
        let durability = match &self.inner {
            StorageRuntimeInner::Cache(_) => crate::commit::CommitDurabilityMode::Cache,
            StorageRuntimeInner::Durable(_) => crate::commit::CommitDurabilityMode::Standard,
            StorageRuntimeInner::Closed => {
                return Err(StorageApiError::InvalidRuntimeState {
                    reason: "commit requires an open runtime",
                });
            }
        };
        let runtime_batch = map_api_commit_batch(batch, timestamp, durability)?;
        let guard = CommitBranchGenerationGuard::exact(default_branch_generation()?);
        let outcome = match &mut self.inner {
            StorageRuntimeInner::Cache(runtime) => {
                runtime.execute_cache_commit(runtime_batch, guard)
            }
            StorageRuntimeInner::Durable(runtime) => {
                runtime.execute_durable_commit(runtime_batch, guard)
            }
            StorageRuntimeInner::Closed => unreachable!("closed runtime returned above"),
        }
        .map_err(map_lifecycle_error)?;
        let commit_version =
            outcome
                .commit_version()
                .ok_or(StorageApiError::InvalidRuntimeState {
                    reason: "test commit did not allocate a commit version",
                })?;
        let commit_timestamp =
            outcome
                .commit_timestamp()
                .ok_or(StorageApiError::InvalidRuntimeState {
                    reason: "test commit did not allocate a commit timestamp",
                })?;
        Ok(super::CommitSummary::new(
            outcome.branch_id(),
            commit_version,
            commit_timestamp,
        ))
    }

    #[cfg(test)]
    pub(crate) fn set_timestamp_coverage_for_test(
        &mut self,
        branch_id: BranchId,
        coverage: crate::branch::BranchTimestampCoverage,
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
        match &mut self.inner {
            StorageRuntimeInner::Cache(runtime) => {
                runtime
                    .rotate_active_for_maintenance()
                    .map_err(map_lifecycle_error)?;
                runtime
                    .flush_frozen(&flush_request_for_test(DEFAULT_BRANCH_ID)?)
                    .map(|_| ())
                    .map_err(map_lifecycle_error)
            }
            StorageRuntimeInner::Durable(runtime) => {
                runtime
                    .rotate_active_for_maintenance()
                    .map_err(map_lifecycle_error)?;
                runtime
                    .flush_frozen(&flush_request_for_test(DEFAULT_BRANCH_ID)?)
                    .map(|_| ())
                    .map_err(map_lifecycle_error)
            }
            StorageRuntimeInner::Closed => Err(StorageApiError::InvalidRuntimeState {
                reason: "flush requires an open runtime",
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
            .map(crate::branch::BranchHistoryRow::row),
    )
    .map_err(commit_error)
}

#[cfg(any(test, feature = "testkit"))]
fn map_api_commit_batch(
    batch: &super::CommitBatch,
    timestamp: Timestamp,
    durability: crate::commit::CommitDurabilityMode,
) -> StorageApiResult<crate::commit::CommitBatch> {
    let mut mutations = Vec::with_capacity(batch.mutations().len());
    for mutation in batch.mutations() {
        match mutation {
            super::CommitMutation::Put {
                storage_space,
                key,
                value,
                ttl,
            } => mutations.push(crate::commit::CommitMutation::put(
                physical_key(batch.branch_id(), storage_space, key)?,
                value.as_bytes().to_vec(),
                map_expiry(timestamp, *ttl)?,
                crate::commit::CommitRetentionHint::Append,
            )),
            super::CommitMutation::Delete { storage_space, key } => {
                mutations.push(crate::commit::CommitMutation::delete(physical_key(
                    batch.branch_id(),
                    storage_space,
                    key,
                )?));
            }
        }
    }

    let conflict_validation = if batch.options().conflict_check_required() {
        crate::commit::CommitConflictValidationMode::Validate
    } else {
        crate::commit::CommitConflictValidationMode::Skip
    };
    let options = crate::commit::CommitBatchOptions::new(
        durability,
        conflict_validation,
        crate::commit::CommitDuplicateKeyPolicy::Reject,
        crate::commit::CommitTimestampPolicy::Explicit(timestamp),
        crate::commit::CommitOrigin::StorageRuntime,
    );
    Ok(crate::commit::CommitBatch::mutating(
        batch.branch_id(),
        mutations,
        crate::commit::CommitValidationFacts::empty(),
        options,
    ))
}

#[cfg(any(test, feature = "testkit"))]
fn map_expiry(
    timestamp: Timestamp,
    ttl: Option<std::time::Duration>,
) -> StorageApiResult<crate::commit::CommitExpiry> {
    let Some(ttl) = ttl else {
        return Ok(crate::commit::CommitExpiry::None);
    };
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

#[cfg(test)]
fn flush_request_for_test(branch_id: BranchId) -> StorageApiResult<FlushFrozenRequest> {
    FlushFrozenRequest::new(
        branch_id,
        None,
        FlushTableIdentitySeed::new("api-test-flush").map_err(map_lifecycle_error)?,
        FlushTableObjectId::new("api-test-flush").map_err(map_lifecycle_error)?,
    )
    .map_err(map_lifecycle_error)
}

fn branch_error(error: crate::branch::BranchRuntimeError) -> StorageApiError {
    match error {
        crate::branch::BranchRuntimeError::InsufficientTimestampHistory { branch_id, .. } => {
            StorageApiError::TimestampHistoryUnavailable {
                branch_id,
                reason: "timestamp is outside retained branch history",
            }
        }
        other => StorageApiError::lower_layer_with(
            StorageApiLowerLayer::Branch,
            "branch read failed",
            other,
        ),
    }
}

fn commit_error(error: crate::commit::CommitRuntimeError) -> StorageApiError {
    match error {
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

fn map_lifecycle_error(error: LifecycleError) -> StorageApiError {
    match error {
        LifecycleError::InvalidConfig { field, reason } => {
            StorageApiError::InvalidArgument { field, reason }
        }
        LifecycleError::InvalidOpenPlan { reason } => StorageApiError::InvalidArgument {
            field: "open_options",
            reason,
        },
        LifecycleError::InvalidLifecycleState { reason } => {
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
        LifecycleError::BranchHistoryUnavailable { branch_id, reason } => {
            StorageApiError::RetainedHistoryUnavailable { branch_id, reason }
        }
        LifecycleError::InsufficientTimestampHistory { branch_id, reason } => {
            StorageApiError::TimestampHistoryUnavailable { branch_id, reason }
        }
        LifecycleError::CapabilityMismatch { .. } => StorageApiError::UnsupportedCapability {
            capability: "backend",
            reason: "backend capabilities do not satisfy storage mode",
        },
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

const fn default_timestamp_source() -> CommitManualTimestampSource {
    CommitManualTimestampSource::new(DEFAULT_TIMESTAMP)
}
