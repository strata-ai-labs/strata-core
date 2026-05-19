//! Branch-runtime error vocabulary.

use crate::table::TableRuntimeError;
use std::error::Error;
use std::fmt;
use std::sync::Arc;
use strata_core_next::BranchId;

pub(crate) type BranchRuntimeResult<T> = Result<T, BranchRuntimeError>;

#[derive(Clone, Debug)]
pub(crate) enum BranchRuntimeError {
    InvalidConfig {
        field: &'static str,
        reason: &'static str,
    },
    InvalidBranchState {
        reason: &'static str,
    },
    BranchNotFound {
        branch_id: BranchId,
    },
    BranchAlreadyExists {
        branch_id: BranchId,
    },
    InvalidBranchRow {
        reason: &'static str,
    },
    InvalidReadBound {
        reason: &'static str,
    },
    InvalidInheritedLayer {
        reason: &'static str,
    },
    InvalidReachability {
        reason: &'static str,
    },
    TableRuntime {
        source: TableRuntimeError,
    },
    Publish {
        reason: &'static str,
        source: Option<Arc<dyn Error + Send + Sync + 'static>>,
    },
}

impl BranchRuntimeError {
    pub(crate) fn publish(reason: &'static str) -> Self {
        Self::Publish {
            reason,
            source: None,
        }
    }

    pub(crate) fn publish_with(
        reason: &'static str,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::Publish {
            reason,
            source: Some(Arc::new(source)),
        }
    }
}

impl PartialEq for BranchRuntimeError {
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
                Self::InvalidBranchState { reason: left },
                Self::InvalidBranchState { reason: right },
            )
            | (Self::InvalidBranchRow { reason: left }, Self::InvalidBranchRow { reason: right })
            | (Self::InvalidReadBound { reason: left }, Self::InvalidReadBound { reason: right })
            | (
                Self::InvalidInheritedLayer { reason: left },
                Self::InvalidInheritedLayer { reason: right },
            )
            | (
                Self::InvalidReachability { reason: left },
                Self::InvalidReachability { reason: right },
            ) => left == right,
            (
                Self::BranchNotFound { branch_id: left },
                Self::BranchNotFound { branch_id: right },
            )
            | (
                Self::BranchAlreadyExists { branch_id: left },
                Self::BranchAlreadyExists { branch_id: right },
            ) => left == right,
            (Self::TableRuntime { source: left }, Self::TableRuntime { source: right }) => {
                left == right
            }
            (Self::Publish { reason: left, .. }, Self::Publish { reason: right, .. }) => {
                left == right
            }
            _ => false,
        }
    }
}

impl Eq for BranchRuntimeError {}

impl fmt::Display for BranchRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig { field, reason } => {
                write!(
                    formatter,
                    "branch runtime configuration field {field} is invalid: {reason}"
                )
            }
            Self::InvalidBranchState { reason } => {
                write!(formatter, "branch state is invalid: {reason}")
            }
            Self::BranchNotFound { branch_id } => {
                write!(formatter, "branch {branch_id} was not found")
            }
            Self::BranchAlreadyExists { branch_id } => {
                write!(formatter, "branch {branch_id} already exists")
            }
            Self::InvalidBranchRow { reason } => {
                write!(formatter, "branch row is invalid: {reason}")
            }
            Self::InvalidReadBound { reason } => {
                write!(formatter, "branch read bound is invalid: {reason}")
            }
            Self::InvalidInheritedLayer { reason } => {
                write!(formatter, "inherited branch layer is invalid: {reason}")
            }
            Self::InvalidReachability { reason } => {
                write!(formatter, "branch reachability facts are invalid: {reason}")
            }
            Self::TableRuntime { source } => {
                write!(formatter, "table runtime operation failed: {source}")
            }
            Self::Publish { reason, .. } => {
                write!(formatter, "branch table publication failed: {reason}")
            }
        }
    }
}

impl Error for BranchRuntimeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::TableRuntime { source } => Some(source),
            Self::Publish {
                source: Some(source),
                ..
            } => Some(source.as_ref()),
            Self::InvalidConfig { .. }
            | Self::InvalidBranchState { .. }
            | Self::BranchNotFound { .. }
            | Self::BranchAlreadyExists { .. }
            | Self::InvalidBranchRow { .. }
            | Self::InvalidReadBound { .. }
            | Self::InvalidInheritedLayer { .. }
            | Self::InvalidReachability { .. }
            | Self::Publish { source: None, .. } => None,
        }
    }
}
