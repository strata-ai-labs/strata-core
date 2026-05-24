//! Lifecycle error vocabulary.

use super::{ClosePhase, StorageMode};
use crate::backend::BackendCapability;
use std::error::Error;
use std::fmt;
use std::sync::Arc;
use strata_core_next::CommitVersion;

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
            Self::CapabilityMismatch { .. } => "failed_precondition.lifecycle.capability",
            Self::RecoveryFailed { .. } => "corruption.lifecycle.recovery",
            Self::MaintenanceFailed { .. } => "failed_precondition.lifecycle.maintenance",
            Self::MaintenanceQueueFull { .. } => "resource_exhausted.lifecycle.maintenance_queue",
            Self::MaintenanceTaskFailed { .. } => "failed_precondition.lifecycle.maintenance_task",
            Self::FlushPublicationFailed { .. } => {
                "failed_precondition.lifecycle.flush_publication"
            }
            Self::FlushPublicationUncertain { .. } => "unknown.lifecycle.flush_publication",
            Self::FlushPublicationOrphaned { .. } => "unknown.lifecycle.flush_publication_orphan",
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
}

impl PartialEq for LifecycleError {
    fn eq(&self, other: &Self) -> bool {
        if let Some(equal) = self.same_static_reason_variant(other) {
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
