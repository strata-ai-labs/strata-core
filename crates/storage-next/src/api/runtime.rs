//! API runtime handle.

use crate::branch::BranchRuntimeConfig;
use crate::commit::{CommitBranchGeneration, CommitManualTimestampSource, CommitRuntimeConfig};
use crate::lifecycle::{
    CloseOutcome, CloseOutcomeStatus, LifecycleCacheOpenRequest, LifecycleCacheRuntime,
    LifecycleCodecId, LifecycleConfig, LifecycleDurableLocalOpenRequest,
    LifecycleDurableLocalRuntime, LifecycleDurableLocalShell, LifecycleError,
    LifecycleRecoveryRuntime, LifecycleWalGrowthPolicy, RecoveryHealth, RecoveryStrictness,
    StorageMode as LifecycleStorageMode, StorageOpenOutcome as LifecycleStorageOpenOutcome,
    StorageOpenPlan, StorageRuntimeBudget,
};
use crate::service::WalServiceConfig;
use strata_core_next::{BranchId, Timestamp};

use super::{
    StorageApiError, StorageApiLowerLayer, StorageApiResult, StorageBackend, StorageBudgetPolicy,
    StorageCloseSummary, StorageDurabilityPolicy, StorageMode, StorageOpenDisposition,
    StorageOpenOptions, StorageOpenOutcome, StorageOpenSummary, StorageRuntimeState,
    StorageWalGrowthPolicy,
};
use crate::api::outcome::StorageCloseEffects;

const DEFAULT_DATABASE_ID: [u8; 16] = [0x53; 16];
const DEFAULT_BRANCH_ID: BranchId = BranchId::from_bytes([0x01; BranchId::BYTE_LEN]);
const DEFAULT_BRANCH_GENERATION: u64 = 1;
const DEFAULT_TIMESTAMP: Timestamp = Timestamp::from_micros(1);

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
                options.validate()?;
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
            options.validate()?;
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
