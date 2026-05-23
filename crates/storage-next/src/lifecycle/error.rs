//! Lifecycle error vocabulary.

use super::StorageMode;
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
    RetentionBlocked {
        reason: &'static str,
    },
    CloseFailed {
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

    pub(crate) const fn code(&self) -> &'static str {
        match self {
            Self::InvalidConfig { .. } => "invalid_argument.lifecycle.config",
            Self::InvalidLifecycleState { .. } => "failed_precondition.lifecycle.state",
            Self::InvalidOpenPlan { .. } => "invalid_argument.lifecycle.open_plan",
            Self::CapabilityMismatch { .. } => "failed_precondition.lifecycle.capability",
            Self::RecoveryFailed { .. } => "corruption.lifecycle.recovery",
            Self::MaintenanceFailed { .. } => "failed_precondition.lifecycle.maintenance",
            Self::RetentionBlocked { .. } => "failed_precondition.lifecycle.retention",
            Self::CloseFailed { .. } => "failed_precondition.lifecycle.close",
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
}

impl PartialEq for LifecycleError {
    fn eq(&self, other: &Self) -> bool {
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
                Self::InvalidLifecycleState { reason: left },
                Self::InvalidLifecycleState { reason: right },
            )
            | (Self::InvalidOpenPlan { reason: left }, Self::InvalidOpenPlan { reason: right })
            | (Self::RecoveryFailed { reason: left }, Self::RecoveryFailed { reason: right })
            | (
                Self::MaintenanceFailed { reason: left },
                Self::MaintenanceFailed { reason: right },
            )
            | (Self::RetentionBlocked { reason: left }, Self::RetentionBlocked { reason: right })
            | (Self::CloseFailed { reason: left }, Self::CloseFailed { reason: right }) => {
                left == right
            }
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
                Self::TimelineRecoveryMismatch { reason: left },
                Self::TimelineRecoveryMismatch { reason: right },
            )
            | (
                Self::WalTailRepairRejected { reason: left },
                Self::WalTailRepairRejected { reason: right },
            ) => left == right,
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
            Self::RetentionBlocked { reason } => write!(formatter, "retention blocked: {reason}"),
            Self::CloseFailed { reason } => write!(formatter, "close failed: {reason}"),
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
