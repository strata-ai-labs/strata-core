//! API open options.

use super::{StorageApiError, StorageApiResult};

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageDurabilityPolicy {
    Standard,
    Always,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageMode {
    /// Volatile in-memory storage for tests, previews, and explicit
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageOpenOptions {
    mode: StorageMode,
    strict_recovery: bool,
    budget_policy: StorageBudgetPolicy,
    wal_growth_policy: StorageWalGrowthPolicy,
}

impl StorageOpenOptions {
    /// Open volatile cache storage.
    ///
    /// Cache mode is intentionally non-durable. Use this for tests, previews,
    /// and explicitly ephemeral sessions rather than normal database opens.
    #[must_use]
    pub const fn cache() -> Self {
        Self {
            mode: StorageMode::Cache,
            strict_recovery: true,
            budget_policy: StorageBudgetPolicy::Default,
            wal_growth_policy: StorageWalGrowthPolicy::Default,
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
        }
    }

    #[must_use]
    pub const fn object_durable_candidate() -> Self {
        Self {
            mode: StorageMode::ObjectDurableCandidate,
            strict_recovery: true,
            budget_policy: StorageBudgetPolicy::Default,
            wal_growth_policy: StorageWalGrowthPolicy::Default,
        }
    }

    #[must_use]
    pub const fn distributed_candidate() -> Self {
        Self {
            mode: StorageMode::DistributedCandidate,
            strict_recovery: true,
            budget_policy: StorageBudgetPolicy::Default,
            wal_growth_policy: StorageWalGrowthPolicy::Default,
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

    pub fn validate(&self) -> StorageApiResult<()> {
        self.wal_growth_policy.validate()?;
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
}

impl Default for StorageOpenOptions {
    fn default() -> Self {
        Self::cache()
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
