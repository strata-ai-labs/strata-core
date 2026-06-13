//! API open options.

use super::{StorageApiError, StorageApiResult};
use std::time::Duration;

const DEFAULT_BACKGROUND_WORKER_COUNT: usize = 4;
const DEFAULT_BACKGROUND_QUEUE_DEPTH: usize = 4096;
const DEFAULT_BACKGROUND_MAX_TASKS_PER_WAKE: usize = 8;
const DEFAULT_BACKGROUND_MAX_RUNTIME_PER_WAKE: Duration = Duration::from_millis(25);

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageDurabilityPolicy {
    Standard,
    Always,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageMode {
    /// Volatile in-memory storage for tests, demos, and explicit
    /// ephemeral sessions. This mode does not persist data across process
    /// lifetime.
    Cache,
    /// Directory-backed local durable storage. Callers must provide an
    /// explicit durable backend handle when opening this mode.
    DurableLocal {
        policy: StorageDurabilityPolicy,
    },
    ObjectDurableCandidate,
    DistributedCandidate,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageBudgetPolicy {
    Default,
    LowMemory,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageWalGrowthPolicy {
    Default,
    Disabled,
    Thresholds {
        max_retained_wal_bytes: u64,
        max_retained_wal_segments: usize,
        max_commits_since_checkpoint: u64,
    },
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageMaintenanceSchedulingPolicy {
    Background,
    DeterministicInline,
    EvaluateAndEnqueue,
    Disabled,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageOpenOptions {
    mode: StorageMode,
    strict_recovery: bool,
    budget_policy: StorageBudgetPolicy,
    wal_growth_policy: StorageWalGrowthPolicy,
    maintenance_scheduling_policy: StorageMaintenanceSchedulingPolicy,
    background_maintenance: StorageBackgroundMaintenanceOptions,
    wal_segment_size_for_test: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageBackgroundMaintenanceOptions {
    worker_count: usize,
    scheduler_queue_depth: usize,
    max_tasks_per_wake: usize,
    max_runtime_per_wake: Duration,
}

impl StorageBackgroundMaintenanceOptions {
    #[must_use]
    pub const fn product_default() -> Self {
        Self {
            worker_count: DEFAULT_BACKGROUND_WORKER_COUNT,
            scheduler_queue_depth: DEFAULT_BACKGROUND_QUEUE_DEPTH,
            max_tasks_per_wake: DEFAULT_BACKGROUND_MAX_TASKS_PER_WAKE,
            max_runtime_per_wake: DEFAULT_BACKGROUND_MAX_RUNTIME_PER_WAKE,
        }
    }

    #[must_use]
    pub const fn with_worker_count(mut self, worker_count: usize) -> Self {
        self.worker_count = worker_count;
        self
    }

    #[must_use]
    pub const fn with_scheduler_queue_depth(mut self, scheduler_queue_depth: usize) -> Self {
        self.scheduler_queue_depth = scheduler_queue_depth;
        self
    }

    #[must_use]
    pub const fn with_max_tasks_per_wake(mut self, max_tasks_per_wake: usize) -> Self {
        self.max_tasks_per_wake = max_tasks_per_wake;
        self
    }

    #[must_use]
    pub const fn with_max_runtime_per_wake(mut self, max_runtime_per_wake: Duration) -> Self {
        self.max_runtime_per_wake = max_runtime_per_wake;
        self
    }

    #[must_use]
    pub const fn worker_count(self) -> usize {
        self.worker_count
    }

    #[must_use]
    pub const fn scheduler_queue_depth(self) -> usize {
        self.scheduler_queue_depth
    }

    #[must_use]
    pub const fn max_tasks_per_wake(self) -> usize {
        self.max_tasks_per_wake
    }

    #[must_use]
    pub const fn max_runtime_per_wake(self) -> Duration {
        self.max_runtime_per_wake
    }

    fn validate(self) -> StorageApiResult<()> {
        if self.worker_count == 0 {
            return Err(StorageApiError::InvalidArgument {
                field: "background_worker_count",
                reason: "background maintenance worker count must be greater than zero",
            });
        }
        if self.scheduler_queue_depth == 0 {
            return Err(StorageApiError::InvalidArgument {
                field: "background_scheduler_queue_depth",
                reason: "background maintenance scheduler queue depth must be greater than zero",
            });
        }
        if self.max_tasks_per_wake == 0 {
            return Err(StorageApiError::InvalidArgument {
                field: "background_max_tasks_per_wake",
                reason: "background maintenance max tasks per wake must be greater than zero",
            });
        }
        if self.max_runtime_per_wake.is_zero() {
            return Err(StorageApiError::InvalidArgument {
                field: "background_max_runtime_per_wake",
                reason: "background maintenance max runtime per wake must be greater than zero",
            });
        }
        Ok(())
    }
}

impl StorageOpenOptions {
    /// Open volatile cache storage.
    ///
    /// Cache mode is intentionally non-durable. Use this for tests, demos,
    /// and explicitly ephemeral sessions rather than normal database opens.
    #[must_use]
    pub const fn cache() -> Self {
        Self {
            mode: StorageMode::Cache,
            strict_recovery: true,
            budget_policy: StorageBudgetPolicy::Default,
            wal_growth_policy: StorageWalGrowthPolicy::Default,
            maintenance_scheduling_policy: StorageMaintenanceSchedulingPolicy::Background,
            background_maintenance: StorageBackgroundMaintenanceOptions::product_default(),
            wal_segment_size_for_test: None,
        }
    }

    /// Open explicitly ephemeral storage.
    ///
    /// This is an intent-revealing alias for [`Self::cache`].
    #[must_use]
    pub const fn ephemeral() -> Self {
        Self::cache()
    }

    /// Open durable local storage through a caller-provided backend handle.
    #[must_use]
    pub const fn durable_local(policy: StorageDurabilityPolicy) -> Self {
        Self {
            mode: StorageMode::DurableLocal { policy },
            strict_recovery: true,
            budget_policy: StorageBudgetPolicy::Default,
            wal_growth_policy: StorageWalGrowthPolicy::Default,
            maintenance_scheduling_policy: StorageMaintenanceSchedulingPolicy::Background,
            background_maintenance: StorageBackgroundMaintenanceOptions::product_default(),
            wal_segment_size_for_test: None,
        }
    }

    #[must_use]
    pub const fn object_durable_candidate() -> Self {
        Self {
            mode: StorageMode::ObjectDurableCandidate,
            strict_recovery: true,
            budget_policy: StorageBudgetPolicy::Default,
            wal_growth_policy: StorageWalGrowthPolicy::Default,
            maintenance_scheduling_policy: StorageMaintenanceSchedulingPolicy::Background,
            background_maintenance: StorageBackgroundMaintenanceOptions::product_default(),
            wal_segment_size_for_test: None,
        }
    }

    #[must_use]
    pub const fn distributed_candidate() -> Self {
        Self {
            mode: StorageMode::DistributedCandidate,
            strict_recovery: true,
            budget_policy: StorageBudgetPolicy::Default,
            wal_growth_policy: StorageWalGrowthPolicy::Default,
            maintenance_scheduling_policy: StorageMaintenanceSchedulingPolicy::Background,
            background_maintenance: StorageBackgroundMaintenanceOptions::product_default(),
            wal_segment_size_for_test: None,
        }
    }

    #[must_use]
    pub const fn with_strict_recovery(mut self, strict_recovery: bool) -> Self {
        self.strict_recovery = strict_recovery;
        self
    }

    #[must_use]
    pub const fn with_budget_policy(mut self, budget_policy: StorageBudgetPolicy) -> Self {
        self.budget_policy = budget_policy;
        self
    }

    #[must_use]
    pub const fn with_wal_growth_policy(
        mut self,
        wal_growth_policy: StorageWalGrowthPolicy,
    ) -> Self {
        self.wal_growth_policy = wal_growth_policy;
        self
    }

    #[must_use]
    pub const fn with_maintenance_scheduling_policy(
        mut self,
        maintenance_scheduling_policy: StorageMaintenanceSchedulingPolicy,
    ) -> Self {
        self.maintenance_scheduling_policy = maintenance_scheduling_policy;
        self
    }

    #[must_use]
    pub const fn with_background_maintenance(
        mut self,
        background_maintenance: StorageBackgroundMaintenanceOptions,
    ) -> Self {
        self.background_maintenance = background_maintenance;
        self
    }

    #[must_use]
    pub const fn with_background_worker_count(mut self, worker_count: usize) -> Self {
        self.background_maintenance = self.background_maintenance.with_worker_count(worker_count);
        self
    }

    #[must_use]
    pub const fn with_background_scheduler_queue_depth(
        mut self,
        scheduler_queue_depth: usize,
    ) -> Self {
        self.background_maintenance = self
            .background_maintenance
            .with_scheduler_queue_depth(scheduler_queue_depth);
        self
    }

    #[must_use]
    pub const fn with_background_max_tasks_per_wake(mut self, max_tasks_per_wake: usize) -> Self {
        self.background_maintenance = self
            .background_maintenance
            .with_max_tasks_per_wake(max_tasks_per_wake);
        self
    }

    #[must_use]
    pub const fn with_background_max_runtime_per_wake(
        mut self,
        max_runtime_per_wake: Duration,
    ) -> Self {
        self.background_maintenance = self
            .background_maintenance
            .with_max_runtime_per_wake(max_runtime_per_wake);
        self
    }

    #[cfg(any(test, feature = "testkit"))]
    #[must_use]
    pub const fn with_wal_segment_size_for_test(mut self, segment_size: u64) -> Self {
        self.wal_segment_size_for_test = Some(segment_size);
        self
    }

    pub fn validate(&self) -> StorageApiResult<()> {
        self.wal_growth_policy.validate()?;
        self.background_maintenance.validate()?;
        if let Some(segment_size) = self.wal_segment_size_for_test {
            crate::service::WalServiceConfig::new(segment_size)
                .validate()
                .map_err(|_| StorageApiError::InvalidArgument {
                    field: "wal_segment_size",
                    reason: "test WAL segment size is invalid",
                })?;
        }
        match self.mode {
            StorageMode::Cache if !self.strict_recovery => Err(StorageApiError::InvalidArgument {
                field: "strict_recovery",
                reason: "cache mode cannot request lossy durable recovery fallback",
            }),
            StorageMode::Cache | StorageMode::DurableLocal { .. } => Ok(()),
            StorageMode::ObjectDurableCandidate => Err(StorageApiError::UnsupportedCapability {
                capability: "object_durable",
                reason: "object-durable storage is not a V1 production mode",
            }),
            StorageMode::DistributedCandidate => Err(StorageApiError::UnsupportedCapability {
                capability: "distributed_writer",
                reason: "distributed writer coordination is not a V1 production mode",
            }),
        }
    }

    #[must_use]
    pub const fn requires_backend(self) -> bool {
        matches!(self.mode, StorageMode::DurableLocal { .. })
    }

    #[must_use]
    pub const fn mode(&self) -> StorageMode {
        self.mode
    }

    #[must_use]
    pub const fn strict_recovery(&self) -> bool {
        self.strict_recovery
    }

    #[must_use]
    pub const fn budget_policy(&self) -> StorageBudgetPolicy {
        self.budget_policy
    }

    #[must_use]
    pub const fn wal_growth_policy(&self) -> StorageWalGrowthPolicy {
        self.wal_growth_policy
    }

    #[must_use]
    pub const fn maintenance_scheduling_policy(&self) -> StorageMaintenanceSchedulingPolicy {
        self.maintenance_scheduling_policy
    }

    #[must_use]
    pub const fn background_maintenance(&self) -> StorageBackgroundMaintenanceOptions {
        self.background_maintenance
    }

    pub(crate) const fn wal_segment_size_for_test(&self) -> Option<u64> {
        self.wal_segment_size_for_test
    }
}

impl StorageWalGrowthPolicy {
    #[must_use]
    pub const fn thresholds(
        max_retained_wal_bytes: u64,
        max_retained_wal_segments: usize,
        max_commits_since_checkpoint: u64,
    ) -> Self {
        Self::Thresholds {
            max_retained_wal_bytes,
            max_retained_wal_segments,
            max_commits_since_checkpoint,
        }
    }

    fn validate(self) -> StorageApiResult<()> {
        match self {
            Self::Thresholds {
                max_retained_wal_bytes: 0,
                ..
            } => Err(StorageApiError::InvalidArgument {
                field: "max_retained_wal_bytes",
                reason: "WAL growth byte limit must be greater than zero",
            }),
            Self::Thresholds {
                max_retained_wal_segments: 0,
                ..
            } => Err(StorageApiError::InvalidArgument {
                field: "max_retained_wal_segments",
                reason: "WAL growth segment limit must be greater than zero",
            }),
            Self::Thresholds {
                max_commits_since_checkpoint: 0,
                ..
            } => Err(StorageApiError::InvalidArgument {
                field: "max_commits_since_checkpoint",
                reason: "WAL growth commit limit must be greater than zero",
            }),
            Self::Default | Self::Disabled | Self::Thresholds { .. } => Ok(()),
        }
    }
}
