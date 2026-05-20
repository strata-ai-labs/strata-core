//! Branch-runtime error vocabulary.

use crate::table::TableRuntimeError;
use std::error::Error;
use std::fmt;
use std::sync::Arc;
use strata_core_next::{BranchId, Timestamp};

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
    InsufficientTimestampHistory {
        branch_id: BranchId,
        requested_timestamp: Timestamp,
        earliest_available_timestamp: Option<Timestamp>,
        source: BranchTimestampHistorySource,
    },
    InvalidInheritedLayer {
        reason: &'static str,
    },
    InvalidReachability {
        reason: &'static str,
    },
    InvalidCompaction {
        reason: &'static str,
    },
    InvalidSnapshotInstall {
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

#[expect(
    dead_code,
    reason = "own and inherited timestamp history sources are reserved for retained-history proofs"
)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BranchTimestampHistorySource {
    OwnState,
    InheritedState,
    Combined,
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
            )
            | (
                Self::InvalidCompaction { reason: left },
                Self::InvalidCompaction { reason: right },
            )
            | (
                Self::InvalidSnapshotInstall { reason: left },
                Self::InvalidSnapshotInstall { reason: right },
            ) => left == right,
            (
                Self::InsufficientTimestampHistory {
                    branch_id: left_branch_id,
                    requested_timestamp: left_requested,
                    earliest_available_timestamp: left_earliest,
                    source: left_source,
                },
                Self::InsufficientTimestampHistory {
                    branch_id: right_branch_id,
                    requested_timestamp: right_requested,
                    earliest_available_timestamp: right_earliest,
                    source: right_source,
                },
            ) => {
                left_branch_id == right_branch_id
                    && left_requested == right_requested
                    && left_earliest == right_earliest
                    && left_source == right_source
            }
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
            Self::InsufficientTimestampHistory {
                branch_id,
                requested_timestamp,
                earliest_available_timestamp,
                source,
            } => {
                if let Some(earliest) = earliest_available_timestamp {
                    write!(
                        formatter,
                        "branch {branch_id} has insufficient timestamp history for requested timestamp {requested_timestamp:?}; earliest available timestamp is {earliest:?} from {source:?}",
                    )
                } else {
                    write!(
                        formatter,
                        "branch {branch_id} has insufficient timestamp history for requested timestamp {requested_timestamp:?} from {source:?}",
                    )
                }
            }
            Self::InvalidInheritedLayer { reason } => {
                write!(formatter, "inherited branch layer is invalid: {reason}")
            }
            Self::InvalidReachability { reason } => {
                write!(formatter, "branch reachability facts are invalid: {reason}")
            }
            Self::InvalidCompaction { reason } => {
                write!(formatter, "branch compaction request is invalid: {reason}")
            }
            Self::InvalidSnapshotInstall { reason } => {
                write!(
                    formatter,
                    "branch snapshot install request is invalid: {reason}"
                )
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
            | Self::InsufficientTimestampHistory { .. }
            | Self::InvalidInheritedLayer { .. }
            | Self::InvalidReachability { .. }
            | Self::InvalidCompaction { .. }
            | Self::InvalidSnapshotInstall { .. }
            | Self::Publish { source: None, .. } => None,
        }
    }
}
