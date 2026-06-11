//! Lifecycle configuration facts.

use super::{LifecycleError, LifecycleResult, StorageRuntimeBudget};

const DEFAULT_MAX_MAINTENANCE_QUEUE_DEPTH: usize = 1024;
const DEFAULT_MAX_RECOVERY_FAULTS: usize = 64;
const DEFAULT_WAL_GROWTH_MAX_BYTES: u64 = 256 * 1024 * 1024;
const DEFAULT_WAL_GROWTH_MAX_SEGMENTS: usize = 64;
const DEFAULT_WAL_GROWTH_MAX_COMMITS: u64 = 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleConfig {
    max_maintenance_queue_depth: usize,
    max_recovery_faults: usize,
    close_timeout_policy: LifecycleCloseTimeoutPolicy,
    lossy_recovery: LifecycleLossyRecoveryPolicy,
    storage_budget: StorageRuntimeBudget,
    wal_growth_policy: LifecycleWalGrowthPolicy,
    maintenance_scheduling_policy: LifecycleMaintenanceSchedulingPolicy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LifecycleCloseTimeoutPolicy {
    ReturnTypedTimeout,
    WaitForStorageDrain,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LifecycleLossyRecoveryPolicy {
    Disabled,
    ExplicitlyAllowed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleWalGrowthPolicy {
    enabled: bool,
    max_retained_wal_bytes: u64,
    max_retained_wal_segments: usize,
    max_commits_since_checkpoint: u64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum LifecycleMaintenanceSchedulingPolicy {
    Disabled,
    #[default]
    EvaluateAndEnqueue,
    DeterministicInline,
}

impl LifecycleConfig {
    pub(crate) fn new(
        max_maintenance_queue_depth: usize,
        max_recovery_faults: usize,
        close_timeout_policy: LifecycleCloseTimeoutPolicy,
        lossy_recovery: LifecycleLossyRecoveryPolicy,
    ) -> LifecycleResult<Self> {
        let config = Self {
            max_maintenance_queue_depth,
            max_recovery_faults,
            close_timeout_policy,
            lossy_recovery,
            storage_budget: StorageRuntimeBudget::default(),
            wal_growth_policy: LifecycleWalGrowthPolicy::default(),
            maintenance_scheduling_policy: LifecycleMaintenanceSchedulingPolicy::default(),
        };
        config.validate()?;
        Ok(config)
    }

    pub(crate) fn with_storage_budget(
        mut self,
        storage_budget: StorageRuntimeBudget,
    ) -> LifecycleResult<Self> {
        storage_budget.validate()?;
        self.storage_budget = storage_budget;
        self.validate()?;
        Ok(self)
    }

    pub(crate) fn with_wal_growth_policy(
        mut self,
        wal_growth_policy: LifecycleWalGrowthPolicy,
    ) -> LifecycleResult<Self> {
        wal_growth_policy.validate()?;
        self.wal_growth_policy = wal_growth_policy;
        self.validate()?;
        Ok(self)
    }

    pub(crate) fn with_maintenance_scheduling_policy(
        mut self,
        maintenance_scheduling_policy: LifecycleMaintenanceSchedulingPolicy,
    ) -> LifecycleResult<Self> {
        self.maintenance_scheduling_policy = maintenance_scheduling_policy;
        self.validate()?;
        Ok(self)
    }

    pub(crate) const fn max_maintenance_queue_depth(self) -> usize {
        self.max_maintenance_queue_depth
    }

    pub(crate) const fn max_recovery_faults(self) -> usize {
        self.max_recovery_faults
    }

    pub(crate) const fn close_timeout_policy(self) -> LifecycleCloseTimeoutPolicy {
        self.close_timeout_policy
    }

    pub(crate) const fn lossy_recovery(self) -> LifecycleLossyRecoveryPolicy {
        self.lossy_recovery
    }

    pub(crate) const fn storage_budget(self) -> StorageRuntimeBudget {
        self.storage_budget
    }

    pub(crate) const fn wal_growth_policy(self) -> LifecycleWalGrowthPolicy {
        self.wal_growth_policy
    }

    pub(crate) const fn maintenance_scheduling_policy(
        self,
    ) -> LifecycleMaintenanceSchedulingPolicy {
        self.maintenance_scheduling_policy
    }

    pub(crate) fn validate(self) -> LifecycleResult<()> {
        if self.max_maintenance_queue_depth == 0 {
            return Err(LifecycleError::InvalidConfig {
                field: "max_maintenance_queue_depth",
                reason: "must be nonzero",
            });
        }
        if self.max_recovery_faults == 0 {
            return Err(LifecycleError::InvalidConfig {
                field: "max_recovery_faults",
                reason: "must be nonzero",
            });
        }
        self.storage_budget.validate()?;
        self.wal_growth_policy.validate()?;
        Ok(())
    }
}

impl LifecycleWalGrowthPolicy {
    pub(crate) const fn new(
        max_retained_wal_bytes: u64,
        max_retained_wal_segments: usize,
        max_commits_since_checkpoint: u64,
    ) -> Self {
        Self {
            enabled: true,
            max_retained_wal_bytes,
            max_retained_wal_segments,
            max_commits_since_checkpoint,
        }
    }

    pub(crate) const fn disabled() -> Self {
        Self {
            enabled: false,
            max_retained_wal_bytes: DEFAULT_WAL_GROWTH_MAX_BYTES,
            max_retained_wal_segments: DEFAULT_WAL_GROWTH_MAX_SEGMENTS,
            max_commits_since_checkpoint: DEFAULT_WAL_GROWTH_MAX_COMMITS,
        }
    }

    pub(crate) const fn enabled(self) -> bool {
        self.enabled
    }

    pub(crate) const fn max_retained_wal_bytes(self) -> u64 {
        self.max_retained_wal_bytes
    }

    pub(crate) const fn max_retained_wal_segments(self) -> usize {
        self.max_retained_wal_segments
    }

    pub(crate) const fn max_commits_since_checkpoint(self) -> u64 {
        self.max_commits_since_checkpoint
    }

    pub(crate) fn validate(self) -> LifecycleResult<()> {
        if self.enabled && self.max_retained_wal_bytes == 0 {
            return Err(LifecycleError::InvalidConfig {
                field: "max_retained_wal_bytes",
                reason: "must be nonzero when WAL growth policy is enabled",
            });
        }
        if self.enabled && self.max_retained_wal_segments == 0 {
            return Err(LifecycleError::InvalidConfig {
                field: "max_retained_wal_segments",
                reason: "must be nonzero when WAL growth policy is enabled",
            });
        }
        if self.enabled && self.max_commits_since_checkpoint == 0 {
            return Err(LifecycleError::InvalidConfig {
                field: "max_commits_since_checkpoint",
                reason: "must be nonzero when WAL growth policy is enabled",
            });
        }
        Ok(())
    }
}

impl Default for LifecycleWalGrowthPolicy {
    fn default() -> Self {
        Self::new(
            DEFAULT_WAL_GROWTH_MAX_BYTES,
            DEFAULT_WAL_GROWTH_MAX_SEGMENTS,
            DEFAULT_WAL_GROWTH_MAX_COMMITS,
        )
    }
}

impl LifecycleMaintenanceSchedulingPolicy {
    pub(crate) const fn enabled(self) -> bool {
        matches!(self, Self::EvaluateAndEnqueue | Self::DeterministicInline)
    }
}

impl Default for LifecycleConfig {
    fn default() -> Self {
        Self::new(
            DEFAULT_MAX_MAINTENANCE_QUEUE_DEPTH,
            DEFAULT_MAX_RECOVERY_FAULTS,
            LifecycleCloseTimeoutPolicy::ReturnTypedTimeout,
            LifecycleLossyRecoveryPolicy::Disabled,
        )
        .expect("default lifecycle configuration is valid")
    }
}
