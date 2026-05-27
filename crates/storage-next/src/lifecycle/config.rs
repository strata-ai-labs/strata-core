//! Lifecycle configuration facts.

use super::{LifecycleError, LifecycleResult, StorageRuntimeBudget};

const DEFAULT_MAX_MAINTENANCE_QUEUE_DEPTH: usize = 1024;
const DEFAULT_MAX_RECOVERY_FAULTS: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleConfig {
    max_maintenance_queue_depth: usize,
    max_recovery_faults: usize,
    close_timeout_policy: LifecycleCloseTimeoutPolicy,
    lossy_recovery: LifecycleLossyRecoveryPolicy,
    storage_budget: StorageRuntimeBudget,
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
        Ok(())
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
