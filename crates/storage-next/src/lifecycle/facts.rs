//! Lifecycle fact vocabulary.

use super::{LifecycleConfig, LifecycleError, LifecycleLossyRecoveryPolicy, LifecycleResult};
use std::fmt;

const MAX_LIFECYCLE_CODEC_ID_BYTES: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LifecycleState {
    New,
    Opening,
    Recovering,
    Open,
    Closing,
    Closed,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum StorageMode {
    Cache,
    DurableLocalStandard,
    DurableLocalAlways,
    ObjectDurableCandidate,
}

impl StorageMode {
    const fn name(self) -> &'static str {
        match self {
            Self::Cache => "cache",
            Self::DurableLocalStandard => "durable-local-standard",
            Self::DurableLocalAlways => "durable-local-always",
            Self::ObjectDurableCandidate => "object-durable-candidate",
        }
    }
}

impl fmt::Display for StorageMode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecoveryStrictness {
    Strict,
    AllowExplicitLossyFallback,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleCodecId {
    value: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StorageOpenPlan {
    storage_mode: StorageMode,
    codec_id: LifecycleCodecId,
    recovery_policy: RecoveryStrictness,
    lifecycle_config: LifecycleConfig,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MaintenanceTaskKind {
    Flush,
    Checkpoint,
    WalTruncation,
    Compaction,
    Materialization,
    SnapshotPruning,
    Retention,
    Quarantine,
    Purge,
    Repair,
    HealthCollection,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RetentionDecision {
    Retain,
    PruneCandidate,
    QuarantineCandidate,
    PurgeCandidate,
    SkipUntilProof,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum QuarantineStage {
    Candidate,
    InventoryPublished,
    Quarantined,
    PurgeEligible,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ClosePhase {
    QuiesceCommits,
    StopMaintenance,
    DrainMaintenance,
    SyncDurableState,
    ReleaseGuards,
    Closed,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct LifecycleStats {
    open_attempts: usize,
    recovery_faults: usize,
    maintenance_tasks: usize,
    retention_blocks: usize,
    close_attempts: usize,
}

impl LifecycleCodecId {
    pub(crate) fn new(value: impl Into<String>) -> LifecycleResult<Self> {
        let codec_id = Self {
            value: value.into(),
        };
        codec_id.validate()?;
        Ok(codec_id)
    }

    pub(crate) fn identity() -> Self {
        Self::new("identity").expect("identity codec id is valid")
    }

    pub(crate) fn as_str(&self) -> &str {
        &self.value
    }

    fn validate(&self) -> LifecycleResult<()> {
        if self.value.is_empty() {
            return Err(LifecycleError::InvalidOpenPlan {
                reason: "codec id must not be empty",
            });
        }
        if self.value.len() > MAX_LIFECYCLE_CODEC_ID_BYTES {
            return Err(LifecycleError::InvalidOpenPlan {
                reason: "codec id is too long",
            });
        }
        if self.value.as_bytes().contains(&0) {
            return Err(LifecycleError::InvalidOpenPlan {
                reason: "codec id must not contain null bytes",
            });
        }
        Ok(())
    }
}

impl StorageOpenPlan {
    pub(crate) fn new(
        storage_mode: StorageMode,
        codec_id: LifecycleCodecId,
        recovery_policy: RecoveryStrictness,
        lifecycle_config: LifecycleConfig,
    ) -> LifecycleResult<Self> {
        let plan = Self {
            storage_mode,
            codec_id,
            recovery_policy,
            lifecycle_config,
        };
        plan.validate()?;
        Ok(plan)
    }

    pub(crate) const fn storage_mode(&self) -> StorageMode {
        self.storage_mode
    }

    pub(crate) fn codec_id(&self) -> &LifecycleCodecId {
        &self.codec_id
    }

    pub(crate) const fn recovery_policy(&self) -> RecoveryStrictness {
        self.recovery_policy
    }

    pub(crate) const fn lifecycle_config(&self) -> LifecycleConfig {
        self.lifecycle_config
    }

    pub(crate) fn validate(&self) -> LifecycleResult<()> {
        self.lifecycle_config.validate()?;
        self.codec_id.validate()?;
        if matches!(self.storage_mode, StorageMode::Cache)
            && matches!(
                self.recovery_policy,
                RecoveryStrictness::AllowExplicitLossyFallback
            )
        {
            return Err(LifecycleError::InvalidOpenPlan {
                reason: "cache mode cannot request durable recovery fallback",
            });
        }
        if matches!(
            self.recovery_policy,
            RecoveryStrictness::AllowExplicitLossyFallback
        ) && !matches!(
            self.lifecycle_config.lossy_recovery(),
            LifecycleLossyRecoveryPolicy::ExplicitlyAllowed
        ) {
            return Err(LifecycleError::InvalidOpenPlan {
                reason: "lossy recovery must be enabled explicitly",
            });
        }
        Ok(())
    }
}

impl LifecycleStats {
    pub(crate) const fn new(
        open_attempts: usize,
        recovery_faults: usize,
        maintenance_tasks: usize,
        retention_blocks: usize,
        close_attempts: usize,
    ) -> Self {
        Self {
            open_attempts,
            recovery_faults,
            maintenance_tasks,
            retention_blocks,
            close_attempts,
        }
    }

    pub(crate) const fn open_attempts(self) -> usize {
        self.open_attempts
    }

    pub(crate) const fn recovery_faults(self) -> usize {
        self.recovery_faults
    }

    pub(crate) const fn maintenance_tasks(self) -> usize {
        self.maintenance_tasks
    }

    pub(crate) const fn retention_blocks(self) -> usize {
        self.retention_blocks
    }

    pub(crate) const fn close_attempts(self) -> usize {
        self.close_attempts
    }
}
