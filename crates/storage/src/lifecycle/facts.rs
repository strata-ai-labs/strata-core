//! Lifecycle fact vocabulary.

use super::{LifecycleConfig, LifecycleError, LifecycleLossyRecoveryPolicy, LifecycleResult};
use std::fmt;

const MAX_LIFECYCLE_CODEC_ID_BYTES: usize = 128;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
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
#[non_exhaustive]
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

/// Per-mode lifecycle policy: which source/table lifecycle work a storage mode
/// is permitted to perform.
///
/// This is the single canonical place a [`StorageMode`] maps to its lifecycle
/// capabilities. It is a pure function of the mode — never of benchmark scale,
/// workload, or configuration flags — so a source guard can assert that cache
/// denies source-table work while durable permits it.
///
/// Cache mode is volatile in-memory storage: it shares the commit, branch,
/// conflict, timestamp, and read mechanics but performs no source/table
/// lifecycle maintenance and applies no source-shape write-admission pressure.
/// The mode is the canonical gate; the maintenance scheduling policy only
/// selects the executor flavor for modes this policy permits to run background
/// maintenance.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[allow(
    clippy::struct_excessive_bools,
    reason = "per-mode capability matrix; one bool per lifecycle capability is the clearest representation and lets future modes diverge per capability"
)]
pub(crate) struct ModeLifecyclePolicy {
    schedule_post_commit_source_maintenance: bool,
    apply_source_shape_admission_pressure: bool,
    run_background_maintenance: bool,
    flush_to_table_source: bool,
    rewrite_or_compact_tables: bool,
    checkpoint_or_truncate_wal: bool,
}

impl ModeLifecyclePolicy {
    /// The canonical mode → policy mapping. This is the only constructor; every
    /// gate derives its policy from a [`StorageMode`] through here so there is a
    /// single source of truth.
    pub(crate) const fn for_storage_mode(mode: StorageMode) -> Self {
        match mode {
            StorageMode::Cache => Self::volatile(),
            // Candidate modes are rejected at validation and never reach a
            // runtime; keep them permissive so no current path changes.
            StorageMode::DurableLocalStandard
            | StorageMode::DurableLocalAlways
            | StorageMode::ObjectDurableCandidate => Self::durable(),
        }
    }

    const fn volatile() -> Self {
        Self {
            schedule_post_commit_source_maintenance: false,
            apply_source_shape_admission_pressure: false,
            run_background_maintenance: false,
            flush_to_table_source: false,
            rewrite_or_compact_tables: false,
            checkpoint_or_truncate_wal: false,
        }
    }

    const fn durable() -> Self {
        Self {
            schedule_post_commit_source_maintenance: true,
            apply_source_shape_admission_pressure: true,
            run_background_maintenance: true,
            flush_to_table_source: true,
            rewrite_or_compact_tables: true,
            checkpoint_or_truncate_wal: true,
        }
    }

    /// Whether ordinary writes may enqueue flush/compaction/materialization work
    /// to maintain source/table shape.
    pub(crate) const fn may_schedule_post_commit_source_maintenance(self) -> bool {
        self.schedule_post_commit_source_maintenance
    }

    /// Whether write admission may slow or block on source/table-shape pressure
    /// (L0 backlog, frozen backlog, table fanout, nonzero-level bytes).
    pub(crate) const fn may_apply_source_shape_admission_pressure(self) -> bool {
        self.apply_source_shape_admission_pressure
    }

    /// Whether this mode requires a background maintenance executor.
    pub(crate) const fn may_run_background_maintenance(self) -> bool {
        self.run_background_maintenance
    }

    /// Whether this mode flushes active/frozen state to table sources.
    ///
    /// Part of the canonical capability surface and asserted by source guards;
    /// flush execution is reached only via scheduling (gated by
    /// [`Self::may_schedule_post_commit_source_maintenance`]) or the explicit
    /// test-only maintenance door, so this is not a separate product gate yet.
    #[allow(
        dead_code,
        reason = "canonical capability surface; exercised by source-guard tests"
    )]
    pub(crate) const fn may_flush_to_table_source(self) -> bool {
        self.flush_to_table_source
    }

    /// Whether this mode rewrites or compacts tables. See
    /// [`Self::may_flush_to_table_source`] for why this is not a separate
    /// product gate yet.
    #[allow(
        dead_code,
        reason = "canonical capability surface; exercised by source-guard tests"
    )]
    pub(crate) const fn may_rewrite_or_compact_tables(self) -> bool {
        self.rewrite_or_compact_tables
    }

    /// Whether this mode checkpoints or truncates the WAL. Cache already
    /// excludes these via the maintenance-kind allowlist; this records the
    /// invariant in the policy surface.
    #[allow(
        dead_code,
        reason = "canonical capability surface; exercised by source-guard tests"
    )]
    pub(crate) const fn may_checkpoint_or_truncate_wal(self) -> bool {
        self.checkpoint_or_truncate_wal
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
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
#[non_exhaustive]
pub(crate) enum MaintenanceTaskKind {
    Flush,
    Checkpoint,
    FlushWatermark,
    WalTruncation,
    Compaction,
    Materialization,
    SnapshotPruning,
    Retention,
    Quarantine,
    Purge,
    Repair,
    HealthCollection,
    CachePreheat,
}

impl MaintenanceTaskKind {
    /// Stable identifier for diagnostic surfaces (failure records, summaries).
    pub(crate) const fn as_static_str(self) -> &'static str {
        match self {
            Self::Flush => "flush",
            Self::Checkpoint => "checkpoint",
            Self::FlushWatermark => "flush_watermark",
            Self::WalTruncation => "wal_truncation",
            Self::Compaction => "compaction",
            Self::Materialization => "materialization",
            Self::SnapshotPruning => "snapshot_pruning",
            Self::Retention => "retention",
            Self::Quarantine => "quarantine",
            Self::Purge => "purge",
            Self::Repair => "repair",
            Self::HealthCollection => "health_collection",
            Self::CachePreheat => "cache_preheat",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum RetentionDecision {
    Retain,
    PruneCandidate,
    QuarantineCandidate,
    PurgeCandidate,
    RepairCandidate,
    SkipUntilProof,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum QuarantineStage {
    Candidate,
    InventoryPublished,
    Quarantined,
    PurgeEligible,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
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

    /// The per-mode lifecycle policy for this plan. Single source of truth — all
    /// gates derive their policy from here.
    pub(crate) const fn lifecycle_policy(&self) -> ModeLifecyclePolicy {
        ModeLifecyclePolicy::for_storage_mode(self.storage_mode)
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
