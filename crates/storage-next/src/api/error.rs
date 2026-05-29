//! API error vocabulary.

use std::error::Error;
use std::fmt;
use std::sync::Arc;

use strata_core_next::BranchId;

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageApiErrorClass {
    InvalidArgument,
    FailedPrecondition,
    NotFound,
    AlreadyExists,
    Conflict,
    Unsupported,
    HistoryUnavailable,
    AmbiguousCommit,
    Internal,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageApiLowerLayer {
    Backend,
    Layout,
    Format,
    Service,
    Table,
    Branch,
    Commit,
    Lifecycle,
}

#[non_exhaustive]
#[derive(Clone, Debug)]
pub enum StorageApiError {
    InvalidArgument {
        field: &'static str,
        reason: &'static str,
    },
    UnsupportedCapability {
        capability: &'static str,
        reason: &'static str,
    },
    InvalidRuntimeState {
        reason: &'static str,
    },
    BranchNotFound {
        branch_id: BranchId,
    },
    BranchAlreadyExists {
        branch_id: BranchId,
    },
    BranchGenerationMismatch {
        branch_id: BranchId,
        expected: u64,
        actual: u64,
    },
    Conflict {
        branch_id: BranchId,
        storage_space: Option<u8>,
        key_fingerprint: Option<u64>,
        user_key_len: Option<usize>,
        reason: &'static str,
    },
    RetainedHistoryUnavailable {
        branch_id: BranchId,
        reason: &'static str,
    },
    TimestampHistoryUnavailable {
        branch_id: BranchId,
        reason: &'static str,
    },
    DurableUncertain {
        reason: &'static str,
        source: Option<Arc<dyn Error + Send + Sync + 'static>>,
    },
    RecoveryDegraded {
        reason: &'static str,
    },
    MaintenanceRejected {
        reason: &'static str,
    },
    LowerLayer {
        layer: StorageApiLowerLayer,
        reason: &'static str,
        source: Option<Arc<dyn Error + Send + Sync + 'static>>,
    },
}

impl StorageApiError {
    pub const fn code(&self) -> &'static str {
        match self {
            Self::InvalidArgument { .. } => "invalid_argument.storage_api.argument",
            Self::UnsupportedCapability { .. } => "unsupported.storage_api.capability",
            Self::InvalidRuntimeState { .. } => "failed_precondition.storage_api.state",
            Self::BranchNotFound { .. } => "not_found.storage_api.branch",
            Self::BranchAlreadyExists { .. } => "already_exists.storage_api.branch",
            Self::BranchGenerationMismatch { .. } => {
                "failed_precondition.storage_api.branch_generation"
            }
            Self::Conflict { .. } => "conflict.storage_api.conflict",
            Self::RetainedHistoryUnavailable { .. } => "history_unavailable.storage_api.retained",
            Self::TimestampHistoryUnavailable { .. } => "history_unavailable.storage_api.timestamp",
            Self::DurableUncertain { .. } => "ambiguous_commit.storage_api.durable_uncertain",
            Self::RecoveryDegraded { .. } => "failed_precondition.storage_api.recovery_degraded",
            Self::MaintenanceRejected { .. } => "failed_precondition.storage_api.maintenance",
            Self::LowerLayer { .. } => "internal.storage_api.lower_layer",
        }
    }

    pub const fn class(&self) -> StorageApiErrorClass {
        match self {
            Self::InvalidArgument { .. } => StorageApiErrorClass::InvalidArgument,
            Self::UnsupportedCapability { .. } => StorageApiErrorClass::Unsupported,
            Self::InvalidRuntimeState { .. }
            | Self::BranchGenerationMismatch { .. }
            | Self::MaintenanceRejected { .. }
            | Self::RecoveryDegraded { .. } => StorageApiErrorClass::FailedPrecondition,
            Self::BranchNotFound { .. } => StorageApiErrorClass::NotFound,
            Self::BranchAlreadyExists { .. } => StorageApiErrorClass::AlreadyExists,
            Self::Conflict { .. } => StorageApiErrorClass::Conflict,
            Self::RetainedHistoryUnavailable { .. } | Self::TimestampHistoryUnavailable { .. } => {
                StorageApiErrorClass::HistoryUnavailable
            }
            Self::DurableUncertain { .. } => StorageApiErrorClass::AmbiguousCommit,
            Self::LowerLayer { .. } => StorageApiErrorClass::Internal,
        }
    }

    pub fn lower_layer_with(
        layer: StorageApiLowerLayer,
        reason: &'static str,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::LowerLayer {
            layer,
            reason,
            source: Some(Arc::new(source)),
        }
    }

    pub const fn durable_uncertain(reason: &'static str) -> Self {
        Self::DurableUncertain {
            reason,
            source: None,
        }
    }

    pub fn durable_uncertain_with(
        reason: &'static str,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::DurableUncertain {
            reason,
            source: Some(Arc::new(source)),
        }
    }
}

impl fmt::Display for StorageApiError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidArgument { field, reason } => {
                write!(formatter, "invalid storage API argument {field}: {reason}")
            }
            Self::UnsupportedCapability { capability, reason } => {
                write!(
                    formatter,
                    "unsupported storage capability {capability}: {reason}"
                )
            }
            Self::InvalidRuntimeState { reason } => {
                write!(formatter, "invalid storage runtime state: {reason}")
            }
            Self::BranchNotFound { branch_id } => write!(formatter, "branch {branch_id} not found"),
            Self::BranchAlreadyExists { branch_id } => {
                write!(formatter, "branch {branch_id} already exists")
            }
            Self::BranchGenerationMismatch {
                branch_id,
                expected,
                actual,
            } => write!(
                formatter,
                "branch {branch_id} generation mismatch: expected {expected}, actual {actual}"
            ),
            Self::Conflict {
                branch_id, reason, ..
            } => {
                write!(formatter, "branch {branch_id} commit conflict: {reason}")
            }
            Self::RetainedHistoryUnavailable { branch_id, reason } => {
                write!(
                    formatter,
                    "branch {branch_id} retained history unavailable: {reason}"
                )
            }
            Self::TimestampHistoryUnavailable { branch_id, reason } => {
                write!(
                    formatter,
                    "branch {branch_id} timestamp history unavailable: {reason}"
                )
            }
            Self::DurableUncertain { reason, .. } => {
                write!(formatter, "storage durability is uncertain: {reason}")
            }
            Self::RecoveryDegraded { reason } => {
                write!(formatter, "storage recovery is degraded: {reason}")
            }
            Self::MaintenanceRejected { reason } => {
                write!(formatter, "storage maintenance rejected: {reason}")
            }
            Self::LowerLayer { layer, reason, .. } => {
                write!(formatter, "storage lower layer {layer:?} failed: {reason}")
            }
        }
    }
}

impl Error for StorageApiError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::LowerLayer {
                source: Some(source),
                ..
            }
            | Self::DurableUncertain {
                source: Some(source),
                ..
            } => Some(source.as_ref()),
            _ => None,
        }
    }
}
