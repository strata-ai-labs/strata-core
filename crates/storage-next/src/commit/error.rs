//! Commit-runtime error vocabulary.

use super::{CommitConflict, CommitObservedVersion};
use crate::row::StorageSpaceId;
use std::error::Error;
use std::fmt;
use std::sync::Arc;
use strata_core_next::{BranchId, CommitVersion};

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
    InvalidTimelineFact {
        reason: &'static str,
    },
    TimelineConflict {
        reason: &'static str,
    },
    DuplicateMutationKey {
        space_id: StorageSpaceId,
    },
    BranchMismatch {
        expected: BranchId,
        actual: BranchId,
    },
    BranchAlreadyExists {
        branch_id: BranchId,
    },
    BranchNotFound {
        branch_id: BranchId,
    },
    BranchNotWritable {
        branch_id: BranchId,
        reason: &'static str,
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
    BranchGuardUnavailable {
        branch_id: BranchId,
        reason: &'static str,
    },
    CommitQuiesceUnavailable {
        reason: &'static str,
    },
    CommitConflict {
        conflict: CommitConflict,
    },
    AppliedButNotVisible {
        branch_id: BranchId,
        commit_version: CommitVersion,
        reason: &'static str,
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
    VersionAllocatorOverflow {
        last_allocated: CommitVersion,
    },
    TimestampUnavailable {
        reason: &'static str,
        source: Option<Arc<dyn Error + Send + Sync + 'static>>,
    },
    InvalidTimestampPolicy {
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

    pub(crate) const fn timestamp_unavailable(reason: &'static str) -> Self {
        Self::TimestampUnavailable {
            reason,
            source: None,
        }
    }

    pub(crate) fn timestamp_unavailable_with(
        reason: &'static str,
        source: impl Error + Send + Sync + 'static,
    ) -> Self {
        Self::TimestampUnavailable {
            reason,
            source: Some(Arc::new(source)),
        }
    }
}

impl PartialEq for CommitRuntimeError {
    #[expect(
        clippy::too_many_lines,
        reason = "manual equality intentionally ignores stored source chains"
    )]
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
                Self::InvalidTimelineFact { reason: left },
                Self::InvalidTimelineFact { reason: right },
            )
            | (Self::TimelineConflict { reason: left }, Self::TimelineConflict { reason: right })
            | (
                Self::CommitQuiesceUnavailable { reason: left },
                Self::CommitQuiesceUnavailable { reason: right },
            )
            | (
                Self::BranchUnavailable { reason: left },
                Self::BranchUnavailable { reason: right },
            )
            | (
                Self::DurabilityUnavailable { reason: left },
                Self::DurabilityUnavailable { reason: right },
            )
            | (
                Self::TimestampUnavailable { reason: left, .. },
                Self::TimestampUnavailable { reason: right, .. },
            )
            | (
                Self::InvalidTimestampPolicy { reason: left },
                Self::InvalidTimestampPolicy { reason: right },
            ) => left == right,
            (
                Self::VersionAllocatorOverflow {
                    last_allocated: left,
                },
                Self::VersionAllocatorOverflow {
                    last_allocated: right,
                },
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
                Self::BranchAlreadyExists { branch_id: left },
                Self::BranchAlreadyExists { branch_id: right },
            )
            | (
                Self::BranchNotFound { branch_id: left },
                Self::BranchNotFound { branch_id: right },
            ) => left == right,
            (
                Self::BranchNotWritable {
                    branch_id: left_branch,
                    reason: left_reason,
                },
                Self::BranchNotWritable {
                    branch_id: right_branch,
                    reason: right_reason,
                },
            )
            | (
                Self::BranchGuardUnavailable {
                    branch_id: left_branch,
                    reason: left_reason,
                },
                Self::BranchGuardUnavailable {
                    branch_id: right_branch,
                    reason: right_reason,
                },
            ) => left_branch == right_branch && left_reason == right_reason,
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
                Self::CommitConflict {
                    conflict: left_conflict,
                },
                Self::CommitConflict {
                    conflict: right_conflict,
                },
            ) => left_conflict == right_conflict,
            (
                Self::AppliedButNotVisible {
                    branch_id: left_branch,
                    commit_version: left_version,
                    reason: left_reason,
                },
                Self::AppliedButNotVisible {
                    branch_id: right_branch,
                    commit_version: right_version,
                    reason: right_reason,
                },
            ) => {
                left_branch == right_branch
                    && left_version == right_version
                    && left_reason == right_reason
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
            Self::InvalidTimelineFact { reason } => {
                write!(formatter, "commit timeline fact is invalid: {reason}")
            }
            Self::TimelineConflict { reason } => {
                write!(formatter, "commit timeline facts conflict: {reason}")
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
            Self::BranchAlreadyExists { .. }
            | Self::BranchNotFound { .. }
            | Self::BranchNotWritable { .. }
            | Self::BranchGenerationMismatch { .. }
            | Self::BranchGenerationExhausted { .. }
            | Self::BranchGuardUnavailable { .. }
            | Self::CommitQuiesceUnavailable { .. } => format_branch_error(self, formatter),
            Self::CommitConflict { conflict } => format_conflict_error(conflict, formatter),
            Self::AppliedButNotVisible {
                branch_id,
                commit_version,
                reason,
            } => {
                write!(
                    formatter,
                    "commit version {commit_version} for branch {branch_id} was applied but not made visible: {reason}"
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
            Self::VersionAllocatorOverflow { last_allocated } => {
                write!(
                    formatter,
                    "commit version allocator overflowed after version {last_allocated}"
                )
            }
            Self::TimestampUnavailable { reason, .. } => {
                write!(formatter, "commit timestamp is unavailable: {reason}")
            }
            Self::InvalidTimestampPolicy { reason } => {
                write!(formatter, "commit timestamp policy is invalid: {reason}")
            }
            Self::LowerLayer { layer, reason, .. } => {
                write!(formatter, "commit lower layer {layer} failed: {reason}")
            }
        }
    }
}

fn format_conflict_error(
    conflict: &CommitConflict,
    formatter: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    write!(
        formatter,
        "commit {:?} conflict for branch {} storage space 0x{:02x} key fingerprint 0x{:016x} user key bytes {}: expected {}, actual {}",
        conflict.kind(),
        conflict.branch_id(),
        conflict.storage_space_id().raw(),
        conflict.key_fingerprint(),
        conflict.user_key_len(),
        ObservedVersionDisplay(conflict.expected()),
        ObservedVersionDisplay(conflict.actual()),
    )
}

fn format_branch_error(
    error: &CommitRuntimeError,
    formatter: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    match error {
        CommitRuntimeError::BranchAlreadyExists { branch_id } => {
            write!(formatter, "commit branch {branch_id} already exists")
        }
        CommitRuntimeError::BranchNotFound { branch_id } => {
            write!(formatter, "commit branch {branch_id} was not found")
        }
        CommitRuntimeError::BranchNotWritable { branch_id, reason } => {
            write!(
                formatter,
                "commit branch {branch_id} is not writable: {reason}"
            )
        }
        CommitRuntimeError::BranchGenerationMismatch {
            branch_id,
            expected,
            actual,
        } => {
            write!(
                formatter,
                "commit branch {branch_id} generation mismatch: expected {expected}, actual {actual}"
            )
        }
        CommitRuntimeError::BranchGenerationExhausted {
            branch_id,
            generation,
        } => {
            write!(
                formatter,
                "commit branch {branch_id} generation is exhausted at {generation}"
            )
        }
        CommitRuntimeError::BranchGuardUnavailable { branch_id, reason } => {
            write!(
                formatter,
                "commit branch {branch_id} guard is unavailable: {reason}"
            )
        }
        CommitRuntimeError::CommitQuiesceUnavailable { reason } => {
            write!(formatter, "commit quiesce is unavailable: {reason}")
        }
        _ => unreachable!("branch error formatter received non-branch error"),
    }
}

struct ObservedVersionDisplay(CommitObservedVersion);

impl fmt::Display for ObservedVersionDisplay {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            CommitObservedVersion::Missing => formatter.write_str("missing"),
            CommitObservedVersion::Present(version) => write!(formatter, "present({version})"),
        }
    }
}

impl Error for CommitRuntimeError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::LowerLayer {
                source: Some(source),
                ..
            }
            | Self::TimestampUnavailable {
                source: Some(source),
                ..
            } => Some(source.as_ref()),
            Self::TimestampUnavailable { source: None, .. }
            | Self::InvalidConfig { .. }
            | Self::InvalidCommitState { .. }
            | Self::InvalidCommitPhase { .. }
            | Self::InvalidVisibilityFacts { .. }
            | Self::InvalidBatch { .. }
            | Self::InvalidMutation { .. }
            | Self::InvalidValidationFacts { .. }
            | Self::InvalidTimelineFact { .. }
            | Self::TimelineConflict { .. }
            | Self::DuplicateMutationKey { .. }
            | Self::BranchMismatch { .. }
            | Self::BranchAlreadyExists { .. }
            | Self::BranchNotFound { .. }
            | Self::BranchNotWritable { .. }
            | Self::BranchGenerationMismatch { .. }
            | Self::BranchGenerationExhausted { .. }
            | Self::BranchGuardUnavailable { .. }
            | Self::CommitQuiesceUnavailable { .. }
            | Self::CommitConflict { .. }
            | Self::AppliedButNotVisible { .. }
            | Self::StorageOwnedMutationSpace { .. }
            | Self::BranchUnavailable { .. }
            | Self::DurabilityUnavailable { .. }
            | Self::VersionAllocatorOverflow { .. }
            | Self::InvalidTimestampPolicy { .. }
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
