//! Commit-runtime error vocabulary.

use crate::row::StorageSpaceId;
use std::error::Error;
use std::fmt;
use std::sync::Arc;
use strata_core_next::BranchId;

#[derive(Clone, Debug)]
pub(crate) enum CommitRuntimeError {
    InvalidConfig {
        field: &'static str,
        reason: &'static str,
    },
    InvalidCommitState {
        reason: &'static str,
    },
    InvalidCommitPhase {
        reason: &'static str,
    },
    InvalidVisibilityFacts {
        reason: &'static str,
    },
    InvalidBatch {
        reason: &'static str,
    },
    InvalidMutation {
        reason: &'static str,
    },
    InvalidValidationFacts {
        reason: &'static str,
    },
    DuplicateMutationKey {
        space_id: StorageSpaceId,
    },
    BranchMismatch {
        expected: BranchId,
        actual: BranchId,
    },
    StorageOwnedMutationSpace {
        space_id: StorageSpaceId,
    },
    BranchUnavailable {
        reason: &'static str,
    },
    DurabilityUnavailable {
        reason: &'static str,
    },
    LowerLayer {
        layer: CommitLowerLayer,
        reason: &'static str,
        source: Option<Arc<dyn Error + Send + Sync + 'static>>,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CommitLowerLayer {
    BranchRuntime,
    WalFormat,
    WalService,
}

impl CommitRuntimeError {
    pub(crate) const fn lower_layer(layer: CommitLowerLayer, reason: &'static str) -> Self {
        Self::LowerLayer {
            layer,
            reason,
            source: None,
        }
    }

    pub(crate) fn lower_layer_with(
        layer: CommitLowerLayer,
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

impl PartialEq for CommitRuntimeError {
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
                Self::InvalidCommitState { reason: left },
                Self::InvalidCommitState { reason: right },
            )
            | (
                Self::InvalidCommitPhase { reason: left },
                Self::InvalidCommitPhase { reason: right },
            )
            | (
                Self::InvalidVisibilityFacts { reason: left },
                Self::InvalidVisibilityFacts { reason: right },
            )
            | (Self::InvalidBatch { reason: left }, Self::InvalidBatch { reason: right })
            | (Self::InvalidMutation { reason: left }, Self::InvalidMutation { reason: right })
            | (
                Self::InvalidValidationFacts { reason: left },
                Self::InvalidValidationFacts { reason: right },
            )
            | (
                Self::BranchUnavailable { reason: left },
                Self::BranchUnavailable { reason: right },
            )
            | (
                Self::DurabilityUnavailable { reason: left },
                Self::DurabilityUnavailable { reason: right },
            ) => left == right,
            (
                Self::DuplicateMutationKey {
                    space_id: left_space,
                },
                Self::DuplicateMutationKey {
                    space_id: right_space,
                },
            )
            | (
                Self::StorageOwnedMutationSpace {
                    space_id: left_space,
                },
                Self::StorageOwnedMutationSpace {
                    space_id: right_space,
                },
            ) => left_space == right_space,
            (
                Self::BranchMismatch {
                    expected: left_expected,
                    actual: left_actual,
                },
                Self::BranchMismatch {
                    expected: right_expected,
                    actual: right_actual,
                },
            ) => left_expected == right_expected && left_actual == right_actual,
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

impl Eq for CommitRuntimeError {}

impl fmt::Display for CommitRuntimeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig { field, reason } => {
                write!(
                    formatter,
                    "commit runtime configuration field {field} is invalid: {reason}"
                )
            }
            Self::InvalidCommitState { reason } => {
                write!(formatter, "commit state is invalid: {reason}")
            }
            Self::InvalidCommitPhase { reason } => {
                write!(formatter, "commit phase is invalid: {reason}")
            }
            Self::InvalidVisibilityFacts { reason } => {
                write!(formatter, "commit visibility facts are invalid: {reason}")
            }
            Self::InvalidBatch { reason } => {
                write!(formatter, "commit batch is invalid: {reason}")
            }
            Self::InvalidMutation { reason } => {
                write!(formatter, "commit mutation is invalid: {reason}")
            }
            Self::InvalidValidationFacts { reason } => {
                write!(formatter, "commit validation facts are invalid: {reason}")
            }
            Self::DuplicateMutationKey { space_id } => {
                write!(
                    formatter,
                    "commit mutation has a duplicate physical key in storage space 0x{:02x}",
                    space_id.raw()
                )
            }
            Self::BranchMismatch { expected, actual } => {
                write!(
                    formatter,
                    "commit branch mismatch: expected {expected}, actual {actual}"
                )
            }
            Self::StorageOwnedMutationSpace { space_id } => {
                write!(
                    formatter,
                    "commit caller key targets storage-owned space 0x{:02x}",
                    space_id.raw()
                )
            }
            Self::BranchUnavailable { reason } => {
                write!(formatter, "commit branch is unavailable: {reason}")
            }
            Self::DurabilityUnavailable { reason } => {
                write!(formatter, "commit durability is unavailable: {reason}")
            }
            Self::LowerLayer { layer, reason, .. } => {
                write!(formatter, "commit lower layer {layer} failed: {reason}")
            }
        }
    }
}

impl Error for CommitRuntimeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::LowerLayer {
                source: Some(source),
                ..
            } => Some(source.as_ref()),
            Self::InvalidConfig { .. }
            | Self::InvalidCommitState { .. }
            | Self::InvalidCommitPhase { .. }
            | Self::InvalidVisibilityFacts { .. }
            | Self::InvalidBatch { .. }
            | Self::InvalidMutation { .. }
            | Self::InvalidValidationFacts { .. }
            | Self::DuplicateMutationKey { .. }
            | Self::BranchMismatch { .. }
            | Self::StorageOwnedMutationSpace { .. }
            | Self::BranchUnavailable { .. }
            | Self::DurabilityUnavailable { .. }
            | Self::LowerLayer { source: None, .. } => None,
        }
    }
}

impl fmt::Display for CommitLowerLayer {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::BranchRuntime => formatter.write_str("branch runtime"),
            Self::WalFormat => formatter.write_str("wal format"),
            Self::WalService => formatter.write_str("wal service"),
        }
    }
}
