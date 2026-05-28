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
    Cache,
    DurableLocal { policy: StorageDurabilityPolicy },
    ObjectDurableCandidate,
    DistributedCandidate,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StorageOpenOptions {
    mode: StorageMode,
    strict_recovery: bool,
}

impl StorageOpenOptions {
    #[must_use]
    pub const fn cache() -> Self {
        Self {
            mode: StorageMode::Cache,
            strict_recovery: true,
        }
    }

    #[must_use]
    pub const fn durable_local(policy: StorageDurabilityPolicy) -> Self {
        Self {
            mode: StorageMode::DurableLocal { policy },
            strict_recovery: true,
        }
    }

    #[must_use]
    pub const fn object_durable_candidate() -> Self {
        Self {
            mode: StorageMode::ObjectDurableCandidate,
            strict_recovery: true,
        }
    }

    #[must_use]
    pub const fn distributed_candidate() -> Self {
        Self {
            mode: StorageMode::DistributedCandidate,
            strict_recovery: true,
        }
    }

    #[must_use]
    pub const fn with_strict_recovery(mut self, strict_recovery: bool) -> Self {
        self.strict_recovery = strict_recovery;
        self
    }

    pub fn validate(&self) -> StorageApiResult<()> {
        match self.mode {
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
    pub const fn mode(&self) -> StorageMode {
        self.mode
    }

    #[must_use]
    pub const fn strict_recovery(&self) -> bool {
        self.strict_recovery
    }
}
