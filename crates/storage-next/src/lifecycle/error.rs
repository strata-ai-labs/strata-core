//! Lifecycle error vocabulary.

use super::{
    ClosePhase, LifecycleStoragePressureReason, LifecycleStoragePressureSeverity,
    StorageBudgetPool, StorageMode,
};
use crate::backend::BackendCapability;
use std::error::Error;
use std::fmt;
use std::sync::Arc;
use strata_core_next::{BranchId, CommitVersion};

#[non_exhaustive]
#[derive(Clone, Debug)]
pub(crate) enum LifecycleError {
    InvalidConfig {
        field: &'static str,
        reason: &'static str,
    },
    InvalidLifecycleState {
        reason: &'static str,
    },
    InvalidOpenPlan {
        reason: &'static str,
    },
    BranchAlreadyExists {
        branch_id: BranchId,
    },
    BranchNotFound {
        branch_id: BranchId,
    },
    BranchNotWritable {
        branch_id: BranchId,
        state: &'static str,
    },
    BranchGenerationMismatch {
        branch_id: BranchId,
        expected: u64,
        actual: u64,
    },
    BranchGenerationExhausted {
        branch_id: BranchId,
        generation: u64,
    },
    BranchHistoryUnavailable {
        branch_id: BranchId,
        reason: &'static str,
    },
    InsufficientTimestampHistory {
        branch_id: BranchId,
        reason: &'static str,
    },
    #[allow(
        dead_code,
        reason = "emission site is the release-plan buffer; declared on the public surface"
    )]
    PinnedViewReleaseBlocked {
        branch_id: BranchId,
        reason: &'static str,
    },
    SourceHasUnflushedRows {
        branch_id: BranchId,
    },
    BranchStateMismatch {
        expected: BranchId,
        actual: BranchId,
    },
    CapabilityMismatch {
        storage_mode: StorageMode,
        required: Vec<BackendCapability>,
        missing: Vec<BackendCapability>,
    },
    RecoveryFailed {
        reason: &'static str,
    },
    MaintenanceFailed {
        reason: &'static str,
    },
    MaintenanceQueueFull {
        reason: &'static str,
    },
    MaintenanceTaskFailed {
        reason: &'static str,
    },
    StoragePressureRejected {
        branch_id: BranchId,
        severity: LifecycleStoragePressureSeverity,
        pressure_reason: LifecycleStoragePressureReason,
        retryable: bool,
        reason: &'static str,
    },
    StorageBudgetExceeded {
        pool: StorageBudgetPool,
        requested_bytes: u64,
        used_bytes: u64,
        limit_bytes: u64,
        requested_count: u64,
        used_count: u64,
        limit_count: Option<u64>,
        reason: &'static str,
    },
    FlushPublicationFailed {
        reason: &'static str,
    },
    FlushPublicationUncertain {
        reason: &'static str,
        source: Option<Arc<dyn Error + Send + Sync + 'static>>,
    },
    FlushPublicationOrphaned {
        object: Option<String>,
        reason: &'static str,
        source: Option<Arc<dyn Error + Send + Sync + 'static>>,
    },
    RewritePublicationFailed {
        reason: &'static str,
        source: Option<Arc<dyn Error + Send + Sync + 'static>>,
    },
    RewritePublicationUncertain {
        objects: Vec<String>,
        reason: &'static str,
        source: Option<Arc<dyn Error + Send + Sync + 'static>>,
    },
    RewritePublicationOrphaned {
        objects: Vec<String>,
        reason: &'static str,
        source: Option<Arc<dyn Error + Send + Sync + 'static>>,
    },
    TableManifestPublicationFailed {
        reason: &'static str,
        source: Option<Arc<dyn Error + Send + Sync + 'static>>,
    },
    TableManifestPublicationUncertain {
        reason: &'static str,
        source: Option<Arc<dyn Error + Send + Sync + 'static>>,
    },
    TableManifestRecoveryMismatch {
        reason: &'static str,
        source: Option<Arc<dyn Error + Send + Sync + 'static>>,
    },
    TableManifestBranchInstallFailed {
        reason: &'static str,
        source: Option<Arc<dyn Error + Send + Sync + 'static>>,
    },
    TableManifestCheckpointConflict {
        reason: &'static str,
    },
    CheckpointPublicationFailed {
        reason: &'static str,
    },
    CheckpointSnapshotOrphaned {
        object: Option<String>,
        reason: &'static str,
    },
    RetentionBlocked {
        reason: &'static str,
    },
    QuarantineProofBlocked {
        reason: &'static str,
    },
    QuarantineInventoryMismatch {
        reason: &'static str,
        source: Option<Arc<dyn Error + Send + Sync + 'static>>,
    },
    QuarantinePublicationFailed {
        reason: &'static str,
        source: Option<Arc<dyn Error + Send + Sync + 'static>>,
    },
    QuarantinePublicationUncertain {
        reason: &'static str,
        source: Option<Arc<dyn Error + Send + Sync + 'static>>,
    },
    PurgeProofBlocked {
        reason: &'static str,
    },
    QuarantineRepairInconclusive {
        reason: &'static str,
        source: Option<Arc<dyn Error + Send + Sync + 'static>>,
    },
    WalRetentionProofIncomplete {
        reason: &'static str,
    },
    CloseFailed {
        reason: &'static str,
    },
    CloseTimeout {
        phase: ClosePhase,
        reason: &'static str,
    },
    TimelineRecoveryMismatch {
        reason: &'static str,
    },
    WalTailRepairRejected {
        reason: &'static str,
    },
    RecoveryVisibilityFailed {
        recovered_visible_version: CommitVersion,
        reason: &'static str,
        source: Option<Arc<dyn Error + Send + Sync + 'static>>,
    },
    LowerLayer {
        layer: LifecycleLowerLayer,
        reason: &'static str,
        source: Option<Arc<dyn Error + Send + Sync + 'static>>,
    },
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LifecycleLowerLayer {
    Backend,
    Layout,
    Format,
    Service,
    TableRuntime,
    BranchRuntime,
    CommitRuntime,
}

impl LifecycleError {
    pub(crate) const fn lower_layer(layer: LifecycleLowerLayer, reason: &'static str) -> Self {
        Self::LowerLayer {
            layer,
            reason,
            source: None,
        }
    }

    pub(crate) fn lower_layer_with(
        layer: LifecycleLowerLayer,
        reason: &'static str,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::LowerLayer {
            layer,
            reason,
            source: Some(Arc::new(source)),
        }
    }

    pub(crate) fn flush_publication_uncertain_with(
        reason: &'static str,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::FlushPublicationUncertain {
            reason,
            source: Some(Arc::new(source)),
        }
    }

    pub(crate) fn flush_publication_orphaned_with(
        object: Option<String>,
        reason: &'static str,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::FlushPublicationOrphaned {
            object,
            reason,
            source: Some(Arc::new(source)),
        }
    }

    pub(crate) fn rewrite_publication_failed_with(
        reason: &'static str,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::RewritePublicationFailed {
            reason,
            source: Some(Arc::new(source)),
        }
    }

    pub(crate) fn rewrite_publication_uncertain_with_objects(
        objects: Vec<String>,
        reason: &'static str,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::RewritePublicationUncertain {
            objects,
            reason,
            source: Some(Arc::new(source)),
        }
    }

    pub(crate) fn rewrite_publication_orphaned_with(
        objects: Vec<String>,
        reason: &'static str,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::RewritePublicationOrphaned {
            objects,
            reason,
            source: Some(Arc::new(source)),
        }
    }

    pub(crate) fn table_manifest_publication_uncertain_with(
        reason: &'static str,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::TableManifestPublicationUncertain {
            reason,
            source: Some(Arc::new(source)),
        }
    }

    pub(crate) fn table_manifest_publication_failed_with(
        reason: &'static str,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::TableManifestPublicationFailed {
            reason,
            source: Some(Arc::new(source)),
        }
    }

    pub(crate) fn table_manifest_recovery_mismatch_with(
        reason: &'static str,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::TableManifestRecoveryMismatch {
            reason,
            source: Some(Arc::new(source)),
        }
    }

    pub(crate) fn table_manifest_branch_install_failed_with(
        reason: &'static str,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::TableManifestBranchInstallFailed {
            reason,
            source: Some(Arc::new(source)),
        }
    }

    pub(crate) const fn table_manifest_checkpoint_conflict(reason: &'static str) -> Self {
        Self::TableManifestCheckpointConflict { reason }
    }

    pub(crate) fn quarantine_inventory_mismatch_with(
        reason: &'static str,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::QuarantineInventoryMismatch {
            reason,
            source: Some(Arc::new(source)),
        }
    }

    pub(crate) fn quarantine_publication_failed_with(
        reason: &'static str,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::QuarantinePublicationFailed {
            reason,
            source: Some(Arc::new(source)),
        }
    }

    pub(crate) fn quarantine_publication_uncertain_with(
        reason: &'static str,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::QuarantinePublicationUncertain {
            reason,
            source: Some(Arc::new(source)),
        }
    }

    pub(crate) fn quarantine_repair_inconclusive_with(
        reason: &'static str,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::QuarantineRepairInconclusive {
            reason,
            source: Some(Arc::new(source)),
        }
    }

    #[allow(
        clippy::too_many_lines,
        reason = "central lifecycle error code registry is intentionally exhaustive"
    )]
    pub(crate) const fn code(&self) -> &'static str {
        match self {
            Self::InvalidConfig { .. } => "invalid_argument.lifecycle.config",
            Self::InvalidLifecycleState { .. } => "failed_precondition.lifecycle.state",
            Self::InvalidOpenPlan { .. } => "invalid_argument.lifecycle.open_plan",
            Self::BranchAlreadyExists { .. } => "already_exists.lifecycle.branch",
            Self::BranchNotFound { .. } => "not_found.lifecycle.branch",
            Self::BranchNotWritable { .. } => "failed_precondition.lifecycle.branch",
            Self::BranchGenerationMismatch { .. } => {
                "failed_precondition.lifecycle.branch_generation"
            }
            Self::BranchGenerationExhausted { .. } => {
                "resource_exhausted.lifecycle.branch_generation"
            }
            Self::BranchHistoryUnavailable { .. } => "failed_precondition.lifecycle.branch_history",
            Self::InsufficientTimestampHistory { .. } => {
                "failed_precondition.lifecycle.timestamp_history"
            }
            Self::PinnedViewReleaseBlocked { .. } => {
                "failed_precondition.lifecycle.pinned_view_release"
            }
            Self::SourceHasUnflushedRows { .. } => {
                "failed_precondition.lifecycle.fork_source_unflushed"
            }
            Self::BranchStateMismatch { .. } => "failed_precondition.lifecycle.branch_state",
            Self::CapabilityMismatch { .. } => "failed_precondition.lifecycle.capability",
            Self::RecoveryFailed { .. } => "corruption.lifecycle.recovery",
            Self::MaintenanceFailed { .. } => "failed_precondition.lifecycle.maintenance",
            Self::MaintenanceQueueFull { .. } => "resource_exhausted.lifecycle.maintenance_queue",
            Self::MaintenanceTaskFailed { .. } => "failed_precondition.lifecycle.maintenance_task",
            Self::StoragePressureRejected { .. } => {
                "failed_precondition.lifecycle.storage_pressure"
            }
            Self::StorageBudgetExceeded { .. } => "resource_exhausted.lifecycle.storage_budget",
            Self::FlushPublicationFailed { .. } => {
                "failed_precondition.lifecycle.flush_publication"
            }
            Self::FlushPublicationUncertain { .. } => "unknown.lifecycle.flush_publication",
            Self::FlushPublicationOrphaned { .. } => "unknown.lifecycle.flush_publication_orphan",
            Self::RewritePublicationFailed { .. } => {
                "failed_precondition.lifecycle.rewrite_publication"
            }
            Self::RewritePublicationUncertain { .. } => "unknown.lifecycle.rewrite_publication",
            Self::RewritePublicationOrphaned { .. } => {
                "unknown.lifecycle.rewrite_publication_orphan"
            }
            Self::TableManifestPublicationFailed { .. } => {
                "failed_precondition.lifecycle.table_manifest_publication"
            }
            Self::TableManifestPublicationUncertain { .. } => {
                "unknown.lifecycle.table_manifest_publication"
            }
            Self::TableManifestRecoveryMismatch { .. } => "corruption.lifecycle.table_manifest",
            Self::TableManifestBranchInstallFailed { .. } => {
                "failed_precondition.lifecycle.table_manifest_branch_install"
            }
            Self::TableManifestCheckpointConflict { .. } => {
                "failed_precondition.lifecycle.table_manifest_checkpoint_conflict"
            }
            Self::CheckpointPublicationFailed { .. } => {
                "failed_precondition.lifecycle.checkpoint_publication"
            }
            Self::CheckpointSnapshotOrphaned { .. } => "unknown.lifecycle.checkpoint_snapshot",
            Self::RetentionBlocked { .. } => "failed_precondition.lifecycle.retention",
            Self::QuarantineProofBlocked { .. } => "failed_precondition.lifecycle.quarantine",
            Self::QuarantineInventoryMismatch { .. } => "corruption.lifecycle.quarantine",
            Self::QuarantinePublicationFailed { .. } => {
                "failed_precondition.lifecycle.quarantine_publication"
            }
            Self::QuarantinePublicationUncertain { .. } => {
                "unknown.lifecycle.quarantine_publication"
            }
            Self::PurgeProofBlocked { .. } => "failed_precondition.lifecycle.purge",
            Self::QuarantineRepairInconclusive { .. } => {
                "failed_precondition.lifecycle.quarantine_repair"
            }
            Self::WalRetentionProofIncomplete { .. } => {
                "failed_precondition.lifecycle.wal_retention"
            }
            Self::CloseFailed { .. } => "failed_precondition.lifecycle.close",
            Self::CloseTimeout { .. } => "deadline_exceeded.lifecycle.close",
            Self::TimelineRecoveryMismatch { .. } => "corruption.lifecycle.timeline",
            Self::WalTailRepairRejected { .. } => "failed_precondition.lifecycle.wal_tail_repair",
            Self::RecoveryVisibilityFailed { .. } => {
                "failed_precondition.lifecycle.recovery_visibility"
            }
            Self::LowerLayer {
                layer: LifecycleLowerLayer::Backend,
                ..
            } => "io.lifecycle.backend",
            Self::LowerLayer {
                layer: LifecycleLowerLayer::Layout,
                ..
            } => "internal.lifecycle.layout",
            Self::LowerLayer {
                layer: LifecycleLowerLayer::Format,
                ..
            } => "serialization.lifecycle.format",
            Self::LowerLayer {
                layer: LifecycleLowerLayer::Service,
                ..
            } => "failed_precondition.lifecycle.service",
            Self::LowerLayer {
                layer: LifecycleLowerLayer::TableRuntime,
                ..
            } => "failed_precondition.lifecycle.table_runtime",
            Self::LowerLayer {
                layer: LifecycleLowerLayer::BranchRuntime,
                ..
            } => "failed_precondition.lifecycle.branch_runtime",
            Self::LowerLayer {
                layer: LifecycleLowerLayer::CommitRuntime,
                ..
            } => "failed_precondition.lifecycle.commit_runtime",
        }
    }

    fn same_static_reason_variant(&self, other: &Self) -> Option<bool> {
        match (self, other) {
            (
                Self::InvalidLifecycleState { reason: left },
                Self::InvalidLifecycleState { reason: right },
            )
            | (Self::InvalidOpenPlan { reason: left }, Self::InvalidOpenPlan { reason: right })
            | (Self::RecoveryFailed { reason: left }, Self::RecoveryFailed { reason: right })
            | (
                Self::MaintenanceFailed { reason: left },
                Self::MaintenanceFailed { reason: right },
            )
            | (
                Self::MaintenanceQueueFull { reason: left },
                Self::MaintenanceQueueFull { reason: right },
            )
            | (
                Self::MaintenanceTaskFailed { reason: left },
                Self::MaintenanceTaskFailed { reason: right },
            )
            | (
                Self::FlushPublicationFailed { reason: left },
                Self::FlushPublicationFailed { reason: right },
            )
            | (
                Self::FlushPublicationUncertain { reason: left, .. },
                Self::FlushPublicationUncertain { reason: right, .. },
            )
            | (
                Self::RewritePublicationFailed { reason: left, .. },
                Self::RewritePublicationFailed { reason: right, .. },
            )
            | (
                Self::TableManifestPublicationFailed { reason: left, .. },
                Self::TableManifestPublicationFailed { reason: right, .. },
            )
            | (
                Self::TableManifestPublicationUncertain { reason: left, .. },
                Self::TableManifestPublicationUncertain { reason: right, .. },
            )
            | (
                Self::TableManifestRecoveryMismatch { reason: left, .. },
                Self::TableManifestRecoveryMismatch { reason: right, .. },
            )
            | (
                Self::TableManifestBranchInstallFailed { reason: left, .. },
                Self::TableManifestBranchInstallFailed { reason: right, .. },
            )
            | (
                Self::TableManifestCheckpointConflict { reason: left },
                Self::TableManifestCheckpointConflict { reason: right },
            )
            | (
                Self::CheckpointPublicationFailed { reason: left },
                Self::CheckpointPublicationFailed { reason: right },
            )
            | (Self::RetentionBlocked { reason: left }, Self::RetentionBlocked { reason: right })
            | (
                Self::WalRetentionProofIncomplete { reason: left },
                Self::WalRetentionProofIncomplete { reason: right },
            )
            | (
                Self::QuarantineProofBlocked { reason: left },
                Self::QuarantineProofBlocked { reason: right },
            )
            | (
                Self::QuarantineInventoryMismatch { reason: left, .. },
                Self::QuarantineInventoryMismatch { reason: right, .. },
            )
            | (
                Self::QuarantinePublicationFailed { reason: left, .. },
                Self::QuarantinePublicationFailed { reason: right, .. },
            )
            | (
                Self::QuarantinePublicationUncertain { reason: left, .. },
                Self::QuarantinePublicationUncertain { reason: right, .. },
            )
            | (
                Self::PurgeProofBlocked { reason: left },
                Self::PurgeProofBlocked { reason: right },
            )
            | (
                Self::QuarantineRepairInconclusive { reason: left, .. },
                Self::QuarantineRepairInconclusive { reason: right, .. },
            )
            | (Self::CloseFailed { reason: left }, Self::CloseFailed { reason: right })
            | (
                Self::TimelineRecoveryMismatch { reason: left },
                Self::TimelineRecoveryMismatch { reason: right },
            )
            | (
                Self::WalTailRepairRejected { reason: left },
                Self::WalTailRepairRejected { reason: right },
            ) => Some(left == right),
            _ => None,
        }
    }

    fn same_object_list_reason_variant(&self, other: &Self) -> Option<bool> {
        match (self, other) {
            (
                Self::RewritePublicationOrphaned {
                    objects: left_objects,
                    reason: left_reason,
                    ..
                },
                Self::RewritePublicationOrphaned {
                    objects: right_objects,
                    reason: right_reason,
                    ..
                },
            )
            | (
                Self::RewritePublicationUncertain {
                    objects: left_objects,
                    reason: left_reason,
                    ..
                },
                Self::RewritePublicationUncertain {
                    objects: right_objects,
                    reason: right_reason,
                    ..
                },
            ) => Some(left_objects == right_objects && left_reason == right_reason),
            _ => None,
        }
    }
}

impl PartialEq for LifecycleError {
    #[allow(
        clippy::too_many_lines,
        reason = "central lifecycle error equality keeps variant-specific comparisons explicit"
    )]
    fn eq(&self, other: &Self) -> bool {
        if let Some(equal) = self.same_static_reason_variant(other) {
            return equal;
        }
        if let Some(equal) = self.same_object_list_reason_variant(other) {
            return equal;
        }
        match (self, other) {
            (
                Self::InvalidConfig {
                    field: left_field,
                    reason: left_reason,
                },
                Self::InvalidConfig {
                    field: right_field,
                    reason: right_reason,
                },
            ) => left_field == right_field && left_reason == right_reason,
            (
                Self::BranchAlreadyExists {
                    branch_id: left_branch,
                },
                Self::BranchAlreadyExists {
                    branch_id: right_branch,
                },
            )
            | (
                Self::BranchNotFound {
                    branch_id: left_branch,
                },
                Self::BranchNotFound {
                    branch_id: right_branch,
                },
            )
            | (
                Self::SourceHasUnflushedRows {
                    branch_id: left_branch,
                },
                Self::SourceHasUnflushedRows {
                    branch_id: right_branch,
                },
            ) => left_branch == right_branch,
            (
                Self::BranchNotWritable {
                    branch_id: left_branch,
                    state: left_state,
                },
                Self::BranchNotWritable {
                    branch_id: right_branch,
                    state: right_state,
                },
            ) => left_branch == right_branch && left_state == right_state,
            (
                Self::BranchGenerationMismatch {
                    branch_id: left_branch,
                    expected: left_expected,
                    actual: left_actual,
                },
                Self::BranchGenerationMismatch {
                    branch_id: right_branch,
                    expected: right_expected,
                    actual: right_actual,
                },
            ) => {
                left_branch == right_branch
                    && left_expected == right_expected
                    && left_actual == right_actual
            }
            (
                Self::BranchGenerationExhausted {
                    branch_id: left_branch,
                    generation: left_generation,
                },
                Self::BranchGenerationExhausted {
                    branch_id: right_branch,
                    generation: right_generation,
                },
            ) => left_branch == right_branch && left_generation == right_generation,
            (
                Self::BranchHistoryUnavailable {
                    branch_id: left_branch,
                    reason: left_reason,
                },
                Self::BranchHistoryUnavailable {
                    branch_id: right_branch,
                    reason: right_reason,
                },
            )
            | (
                Self::InsufficientTimestampHistory {
                    branch_id: left_branch,
                    reason: left_reason,
                },
                Self::InsufficientTimestampHistory {
                    branch_id: right_branch,
                    reason: right_reason,
                },
            )
            | (
                Self::PinnedViewReleaseBlocked {
                    branch_id: left_branch,
                    reason: left_reason,
                },
                Self::PinnedViewReleaseBlocked {
                    branch_id: right_branch,
                    reason: right_reason,
                },
            ) => left_branch == right_branch && left_reason == right_reason,
            (
                Self::BranchStateMismatch {
                    expected: left_expected,
                    actual: left_actual,
                },
                Self::BranchStateMismatch {
                    expected: right_expected,
                    actual: right_actual,
                },
            ) => left_expected == right_expected && left_actual == right_actual,
            (
                Self::FlushPublicationOrphaned {
                    object: left_object,
                    reason: left_reason,
                    ..
                },
                Self::FlushPublicationOrphaned {
                    object: right_object,
                    reason: right_reason,
                    ..
                },
            )
            | (
                Self::CheckpointSnapshotOrphaned {
                    object: left_object,
                    reason: left_reason,
                },
                Self::CheckpointSnapshotOrphaned {
                    object: right_object,
                    reason: right_reason,
                },
            ) => left_object == right_object && left_reason == right_reason,
            (
                Self::CapabilityMismatch {
                    storage_mode: left_mode,
                    required: left_required,
                    missing: left_missing,
                },
                Self::CapabilityMismatch {
                    storage_mode: right_mode,
                    required: right_required,
                    missing: right_missing,
                },
            ) => {
                left_mode == right_mode
                    && left_required == right_required
                    && left_missing == right_missing
            }
            (
                Self::StorageBudgetExceeded {
                    pool: left_pool,
                    requested_bytes: left_requested_bytes,
                    used_bytes: left_used_bytes,
                    limit_bytes: left_limit_bytes,
                    requested_count: left_requested_count,
                    used_count: left_used_count,
                    limit_count: left_limit_count,
                    reason: left_reason,
                },
                Self::StorageBudgetExceeded {
                    pool: right_pool,
                    requested_bytes: right_requested_bytes,
                    used_bytes: right_used_bytes,
                    limit_bytes: right_limit_bytes,
                    requested_count: right_requested_count,
                    used_count: right_used_count,
                    limit_count: right_limit_count,
                    reason: right_reason,
                },
            ) => {
                left_pool == right_pool
                    && left_requested_bytes == right_requested_bytes
                    && left_used_bytes == right_used_bytes
                    && left_limit_bytes == right_limit_bytes
                    && left_requested_count == right_requested_count
                    && left_used_count == right_used_count
                    && left_limit_count == right_limit_count
                    && left_reason == right_reason
            }
            (
                Self::StoragePressureRejected {
                    branch_id: left_branch,
                    severity: left_severity,
                    pressure_reason: left_pressure_reason,
                    retryable: left_retryable,
                    reason: left_reason,
                },
                Self::StoragePressureRejected {
                    branch_id: right_branch,
                    severity: right_severity,
                    pressure_reason: right_pressure_reason,
                    retryable: right_retryable,
                    reason: right_reason,
                },
            ) => {
                left_branch == right_branch
                    && left_severity == right_severity
                    && left_pressure_reason == right_pressure_reason
                    && left_retryable == right_retryable
                    && left_reason == right_reason
            }
            (
                Self::RecoveryVisibilityFailed {
                    recovered_visible_version: left_version,
                    reason: left_reason,
                    ..
                },
                Self::RecoveryVisibilityFailed {
                    recovered_visible_version: right_version,
                    reason: right_reason,
                    ..
                },
            ) => left_version == right_version && left_reason == right_reason,
            (
                Self::CloseTimeout {
                    phase: left_phase,
                    reason: left_reason,
                },
                Self::CloseTimeout {
                    phase: right_phase,
                    reason: right_reason,
                },
            ) => left_phase == right_phase && left_reason == right_reason,
            (
                Self::LowerLayer {
                    layer: left_layer,
                    reason: left_reason,
                    ..
                },
                Self::LowerLayer {
                    layer: right_layer,
                    reason: right_reason,
                    ..
                },
            ) => left_layer == right_layer && left_reason == right_reason,
            _ => false,
        }
    }
}

impl Eq for LifecycleError {}

impl fmt::Display for LifecycleError {
    #[allow(
        clippy::too_many_lines,
        reason = "central lifecycle error display keeps variant wording in one registry"
    )]
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig { field, reason } => {
                write!(formatter, "invalid lifecycle config {field}: {reason}")
            }
            Self::InvalidLifecycleState { reason } => {
                write!(formatter, "invalid lifecycle state: {reason}")
            }
            Self::InvalidOpenPlan { reason } => {
                write!(formatter, "invalid storage open plan: {reason}")
            }
            Self::BranchAlreadyExists { branch_id } => {
                write!(formatter, "branch already exists: {branch_id}")
            }
            Self::BranchNotFound { branch_id } => {
                write!(formatter, "branch not found: {branch_id}")
            }
            Self::BranchNotWritable { branch_id, state } => {
                write!(formatter, "branch {branch_id} is not writable: {state}")
            }
            Self::BranchGenerationMismatch {
                branch_id,
                expected,
                actual,
            } => {
                write!(
                    formatter,
                    "branch generation mismatch for {branch_id}: expected {expected}, actual {actual}"
                )
            }
            Self::BranchGenerationExhausted {
                branch_id,
                generation,
            } => {
                write!(
                    formatter,
                    "branch generation exhausted for {branch_id}: {generation}"
                )
            }
            Self::BranchHistoryUnavailable { branch_id, reason } => {
                write!(
                    formatter,
                    "branch retained history unavailable for {branch_id}: {reason}"
                )
            }
            Self::InsufficientTimestampHistory { branch_id, reason } => {
                write!(
                    formatter,
                    "branch timestamp history insufficient for {branch_id}: {reason}"
                )
            }
            Self::PinnedViewReleaseBlocked { branch_id, reason } => {
                write!(
                    formatter,
                    "pinned view release blocked for {branch_id}: {reason}"
                )
            }
            Self::SourceHasUnflushedRows { branch_id } => {
                write!(formatter, "fork source {branch_id} contains unflushed rows")
            }
            Self::BranchStateMismatch { expected, actual } => {
                write!(
                    formatter,
                    "branch state mismatch: expected {expected}, actual {actual}"
                )
            }
            Self::CapabilityMismatch {
                storage_mode,
                required,
                missing,
            } => {
                write!(
                    formatter,
                    "storage capability mismatch for {storage_mode}: required {}; missing {}",
                    DisplayCapabilities(required),
                    DisplayCapabilities(missing),
                )
            }
            Self::RecoveryFailed { reason } => write!(formatter, "recovery failed: {reason}"),
            Self::MaintenanceFailed { reason } => {
                write!(formatter, "maintenance failed: {reason}")
            }
            Self::MaintenanceQueueFull { reason } => {
                write!(formatter, "maintenance queue is full: {reason}")
            }
            Self::MaintenanceTaskFailed { reason } => {
                write!(formatter, "maintenance task failed: {reason}")
            }
            Self::StoragePressureRejected {
                branch_id,
                severity,
                pressure_reason,
                retryable,
                reason,
            } => {
                write!(
                    formatter,
                    "branch {branch_id} commit rejected by {} storage pressure from {}: {reason}",
                    storage_pressure_severity_name(*severity),
                    storage_pressure_reason_name(*pressure_reason),
                )?;
                if *retryable {
                    formatter.write_str(" (retryable after maintenance)")?;
                }
                Ok(())
            }
            Self::StorageBudgetExceeded {
                pool,
                requested_bytes,
                used_bytes,
                limit_bytes,
                requested_count,
                used_count,
                limit_count,
                reason,
            } => {
                write!(
                    formatter,
                    "storage budget exceeded for {}: requested {requested_bytes} bytes/{requested_count} count, used {used_bytes} bytes/{used_count} count, limit {limit_bytes} bytes",
                    pool.name(),
                )?;
                if let Some(limit_count) = limit_count {
                    write!(formatter, "/{limit_count} count")?;
                }
                write!(formatter, ": {reason}")
            }
            Self::FlushPublicationFailed { reason } => {
                write!(formatter, "flush publication failed: {reason}")
            }
            Self::FlushPublicationUncertain { reason, .. } => {
                write!(formatter, "flush publication uncertain: {reason}")
            }
            Self::FlushPublicationOrphaned { object, reason, .. } => {
                formatter.write_str("flush publication orphaned")?;
                if let Some(object) = object {
                    write!(formatter, " at {object}")?;
                }
                write!(formatter, ": {reason}")
            }
            Self::RewritePublicationFailed { reason, .. } => {
                write!(formatter, "table rewrite publication failed: {reason}")
            }
            Self::RewritePublicationUncertain {
                objects, reason, ..
            } => {
                formatter.write_str("table rewrite publication uncertain")?;
                if !objects.is_empty() {
                    write!(formatter, " at {}", objects.join(","))?;
                }
                write!(formatter, ": {reason}")
            }
            Self::RewritePublicationOrphaned {
                objects, reason, ..
            } => {
                formatter.write_str("table rewrite publication orphaned")?;
                if !objects.is_empty() {
                    write!(formatter, " at {}", objects.join(","))?;
                }
                write!(formatter, ": {reason}")
            }
            Self::TableManifestPublicationFailed { reason, .. } => {
                write!(formatter, "table manifest publication failed: {reason}")
            }
            Self::TableManifestPublicationUncertain { reason, .. } => {
                write!(formatter, "table manifest publication uncertain: {reason}")
            }
            Self::TableManifestRecoveryMismatch { reason, .. } => {
                write!(formatter, "table manifest recovery mismatch: {reason}")
            }
            Self::TableManifestBranchInstallFailed { reason, .. } => {
                write!(formatter, "table manifest branch install failed: {reason}")
            }
            Self::TableManifestCheckpointConflict { reason } => {
                write!(
                    formatter,
                    "table manifest conflicts with checkpoint: {reason}"
                )
            }
            Self::CheckpointPublicationFailed { reason } => {
                write!(formatter, "checkpoint publication failed: {reason}")
            }
            Self::CheckpointSnapshotOrphaned { object, reason } => {
                formatter.write_str("checkpoint snapshot orphaned")?;
                if let Some(object) = object {
                    write!(formatter, " at {object}")?;
                }
                write!(formatter, ": {reason}")
            }
            Self::RetentionBlocked { reason } => write!(formatter, "retention blocked: {reason}"),
            Self::QuarantineProofBlocked { reason } => {
                write!(formatter, "quarantine proof blocked: {reason}")
            }
            Self::QuarantineInventoryMismatch { reason, .. } => {
                write!(formatter, "quarantine inventory mismatch: {reason}")
            }
            Self::QuarantinePublicationFailed { reason, .. } => {
                write!(formatter, "quarantine publication failed: {reason}")
            }
            Self::QuarantinePublicationUncertain { reason, .. } => {
                write!(formatter, "quarantine publication uncertain: {reason}")
            }
            Self::PurgeProofBlocked { reason } => {
                write!(formatter, "purge proof blocked: {reason}")
            }
            Self::QuarantineRepairInconclusive { reason, .. } => {
                write!(formatter, "quarantine repair inconclusive: {reason}")
            }
            Self::WalRetentionProofIncomplete { reason } => {
                write!(formatter, "WAL retention proof incomplete: {reason}")
            }
            Self::CloseFailed { reason } => write!(formatter, "close failed: {reason}"),
            Self::CloseTimeout { phase, reason } => {
                write!(formatter, "close timed out during {phase:?}: {reason}")
            }
            Self::TimelineRecoveryMismatch { reason } => {
                write!(formatter, "timeline recovery mismatch: {reason}")
            }
            Self::WalTailRepairRejected { reason } => {
                write!(formatter, "WAL tail repair rejected: {reason}")
            }
            Self::RecoveryVisibilityFailed {
                recovered_visible_version,
                reason,
                ..
            } => {
                write!(
                    formatter,
                    "recovery visibility failed at {recovered_visible_version}: {reason}"
                )
            }
            Self::LowerLayer { layer, reason, .. } => {
                write!(formatter, "lifecycle lower layer {layer} failed: {reason}")
            }
        }
    }
}

struct DisplayCapabilities<'a>(&'a [BackendCapability]);

impl fmt::Display for DisplayCapabilities<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut capabilities = self.0.iter();
        if let Some(first) = capabilities.next() {
            write!(formatter, "{first}")?;
            for capability in capabilities {
                write!(formatter, ", {capability}")?;
            }
            Ok(())
        } else {
            formatter.write_str("none")
        }
    }
}

impl Error for LifecycleError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::RecoveryVisibilityFailed {
                source: Some(source),
                ..
            }
            | Self::FlushPublicationUncertain {
                source: Some(source),
                ..
            }
            | Self::FlushPublicationOrphaned {
                source: Some(source),
                ..
            }
            | Self::RewritePublicationFailed {
                source: Some(source),
                ..
            }
            | Self::RewritePublicationUncertain {
                source: Some(source),
                ..
            }
            | Self::RewritePublicationOrphaned {
                source: Some(source),
                ..
            }
            | Self::TableManifestPublicationFailed {
                source: Some(source),
                ..
            }
            | Self::TableManifestPublicationUncertain {
                source: Some(source),
                ..
            }
            | Self::TableManifestRecoveryMismatch {
                source: Some(source),
                ..
            }
            | Self::TableManifestBranchInstallFailed {
                source: Some(source),
                ..
            }
            | Self::QuarantineInventoryMismatch {
                source: Some(source),
                ..
            }
            | Self::QuarantinePublicationFailed {
                source: Some(source),
                ..
            }
            | Self::QuarantinePublicationUncertain {
                source: Some(source),
                ..
            }
            | Self::QuarantineRepairInconclusive {
                source: Some(source),
                ..
            }
            | Self::LowerLayer {
                source: Some(source),
                ..
            } => Some(source.as_ref()),
            _ => None,
        }
    }
}

const fn storage_pressure_severity_name(
    severity: LifecycleStoragePressureSeverity,
) -> &'static str {
    match severity {
        LifecycleStoragePressureSeverity::None => "healthy",
        LifecycleStoragePressureSeverity::Background => "background",
        LifecycleStoragePressureSeverity::Urgent => "urgent",
        LifecycleStoragePressureSeverity::BlockMutatingAdmission => "blocking",
    }
}

const fn storage_pressure_reason_name(reason: LifecycleStoragePressureReason) -> &'static str {
    match reason {
        LifecycleStoragePressureReason::None => "no backlog",
        LifecycleStoragePressureReason::ActiveMutableBytes => "active mutable byte pressure",
        LifecycleStoragePressureReason::FrozenBacklog => "frozen table backlog",
        LifecycleStoragePressureReason::LevelZeroTableBacklog => "level-zero table backlog",
        LifecycleStoragePressureReason::NonZeroLevelTableBacklog => "nonzero-level table backlog",
        LifecycleStoragePressureReason::InheritedLayerBacklog => "inherited-layer backlog",
        LifecycleStoragePressureReason::MaintenanceQueueBacklog => "maintenance queue backlog",
    }
}

impl fmt::Display for LifecycleLowerLayer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let name = match self {
            Self::Backend => "backend",
            Self::Layout => "layout",
            Self::Format => "format",
            Self::Service => "service",
            Self::TableRuntime => "table-runtime",
            Self::BranchRuntime => "branch-runtime",
            Self::CommitRuntime => "commit-runtime",
        };
        formatter.write_str(name)
    }
}
