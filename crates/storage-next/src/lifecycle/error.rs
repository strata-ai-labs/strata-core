//! Lifecycle error vocabulary.

use std::error::Error;
use std::fmt;
use std::sync::Arc;

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
        reason: &'static str,
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
    LowerLayer {
        layer: LifecycleLowerLayer,
        reason: &'static str,
        source: Option<Arc<dyn Error + Send + Sync + 'static>>,
    },
}

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
            | (
                Self::CapabilityMismatch { reason: left },
                Self::CapabilityMismatch { reason: right },
            )
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
            Self::CapabilityMismatch { reason } => {
                write!(formatter, "storage capability mismatch: {reason}")
            }
            Self::RecoveryFailed { reason } => write!(formatter, "recovery failed: {reason}"),
            Self::MaintenanceFailed { reason } => {
                write!(formatter, "maintenance failed: {reason}")
            }
            Self::RetentionBlocked { reason } => write!(formatter, "retention blocked: {reason}"),
            Self::CloseFailed { reason } => write!(formatter, "close failed: {reason}"),
            Self::LowerLayer { layer, reason, .. } => {
                write!(formatter, "lifecycle lower layer {layer} failed: {reason}")
            }
        }
    }
}

impl Error for LifecycleError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::LowerLayer {
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
