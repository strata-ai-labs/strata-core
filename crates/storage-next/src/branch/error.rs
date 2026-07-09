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
        reason: BranchCompactionInvalidity,
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

/// Typed reasons a branch compaction request was rejected.
///
/// Codes follow the `failed_precondition.branch.<detail>` format documented
/// in `v1-error-and-diagnostics-contract.md`. Tests should assert on the
/// variant, not on the human-readable display message.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum BranchCompactionInvalidity {
    /// Catch-all for non-pruning compaction validation failures. The
    /// embedded string is for human consumption only; tests should not
    /// assert on it.
    Generic(&'static str),
    /// The candidate references a table that is no longer part of the branch
    /// layout — concurrent maintenance superseded it between scheduling and
    /// execution. A benign race: the maintenance layer defers rather than
    /// fails, and coverage re-derives fresh candidates.
    StaleCandidate,
    /// A retention policy other than `KeepAll` was requested but no
    /// pruning proof was attached.
    ProofMissing,
    /// The branch state changed between proof construction and use
    /// (fingerprint mismatch).
    ProofStale,
    /// The proof was built for a different branch than the one being
    /// compacted.
    ProofBranchMismatch,
    /// The proof's `recovery_health_epoch` is zero or otherwise marks
    /// recovery as unsafe.
    ProofUnsafeRecoveryHealth,
    /// The proof's `visible_version` is lower than the branch's actual
    /// max commit version.
    ProofVisibleVersionBelowState,
    /// `retained_version_floor` exceeds `visible_version`.
    RetainedFloorAboveVisible,
    /// `retained_timestamp_floor` was supplied but the branch's
    /// `BranchTimestampCoverage` does not cover that floor.
    TimestampFloorWithoutCoverage,
    /// A pinned read view sits below the retained version floor, so
    /// pruning would invalidate it.
    PinnedViewBelowFloor,
    /// The proof has no explicit no-readable-inherited-layers gate.
    InheritedLayerUnknown,
    /// The no-readable-inherited-layers gate was asserted while the branch
    /// still has inherited layers attached.
    InheritedLayerUnsafe,
    /// The caller has not confirmed that no other branch references the
    /// candidate tables.
    SharedTableSafetyUnknown,
    /// Tombstone elision requested without the parent proof's tombstone gate.
    TombstoneElisionMissing,
    /// Tombstone elision requested but the compaction is not at the
    /// bottommost level.
    TombstoneElisionNotBottommost,
    /// Tombstone elision would resurrect a value still present in the
    /// rewrite inputs.
    TombstoneResurrectionRisk,
    /// `DropExpired` requested without a TTL cutoff bound on the pruning proof.
    TtlElisionMissing,
    /// `DropExpired` requested but the compaction is not bottommost.
    TtlElisionNotBottommost,
    /// The supplied TTL cutoff exceeds the `retained_timestamp_floor`.
    TtlCutoffExceedsTimestampFloor,
    /// `proof_epoch` is zero.
    ProofEpochInvalid,
    /// `branch_state_fingerprint` is zero (must be derived from real state).
    ProofFingerprintInvalid,
    /// `table_manifest_coverage_floor` exceeds `retained_version_floor`,
    /// indicating cache mode claims durable coverage above the retained
    /// boundary.
    TableManifestCoverageBeyondFloor,
    /// `retained_timestamp_floor` is required but was not supplied.
    RetainedTimestampFloorMissing,
    /// The candidate references a missing storage level — typically a
    /// stale plan.
    CandidateMissingLevel,
    /// The candidate references a missing table — typically a stale plan.
    CandidateMissingTable,
}

impl BranchCompactionInvalidity {
    /// Stable code suitable for telemetry and test assertions.
    #[allow(
        dead_code,
        reason = "tests in this crate assert on the code() string; downstream crates will consume it"
    )]
    pub(crate) const fn code(self) -> &'static str {
        match self {
            Self::Generic(_) => "failed_precondition.branch.invalid_compaction",
            Self::StaleCandidate => "failed_precondition.branch.compaction_candidate_stale",
            Self::ProofMissing => "failed_precondition.branch.row_pruning_proof_missing",
            Self::ProofStale => "failed_precondition.branch.row_pruning_proof_stale",
            Self::ProofBranchMismatch => {
                "failed_precondition.branch.row_pruning_proof_branch_mismatch"
            }
            Self::ProofUnsafeRecoveryHealth => {
                "failed_precondition.branch.row_pruning_proof_unsafe_recovery_health"
            }
            Self::ProofVisibleVersionBelowState => {
                "failed_precondition.branch.row_pruning_proof_visible_version_below_state"
            }
            Self::RetainedFloorAboveVisible => {
                "failed_precondition.branch.row_pruning_retained_floor_above_visible"
            }
            Self::TimestampFloorWithoutCoverage => {
                "failed_precondition.branch.row_pruning_timestamp_floor_without_coverage"
            }
            Self::PinnedViewBelowFloor => {
                "failed_precondition.branch.row_pruning_pinned_view_below_floor"
            }
            Self::InheritedLayerUnknown => {
                "failed_precondition.branch.row_pruning_inherited_layer_unknown"
            }
            Self::InheritedLayerUnsafe => {
                "failed_precondition.branch.row_pruning_inherited_layer_unsafe"
            }
            Self::SharedTableSafetyUnknown => {
                "failed_precondition.branch.row_pruning_shared_table_safety_unknown"
            }
            Self::TombstoneElisionMissing => {
                "failed_precondition.branch.row_pruning_tombstone_elision_missing"
            }
            Self::TombstoneElisionNotBottommost => {
                "failed_precondition.branch.row_pruning_tombstone_elision_not_bottommost"
            }
            Self::TombstoneResurrectionRisk => {
                "failed_precondition.branch.row_pruning_tombstone_resurrection_risk"
            }
            Self::TtlElisionMissing => "failed_precondition.branch.row_pruning_ttl_elision_missing",
            Self::TtlElisionNotBottommost => {
                "failed_precondition.branch.row_pruning_ttl_elision_not_bottommost"
            }
            Self::TtlCutoffExceedsTimestampFloor => {
                "failed_precondition.branch.row_pruning_ttl_cutoff_exceeds_timestamp_floor"
            }
            Self::ProofEpochInvalid => "failed_precondition.branch.row_pruning_proof_epoch_invalid",
            Self::ProofFingerprintInvalid => {
                "failed_precondition.branch.row_pruning_proof_fingerprint_invalid"
            }
            Self::TableManifestCoverageBeyondFloor => {
                "failed_precondition.branch.row_pruning_table_manifest_coverage_beyond_floor"
            }
            Self::RetainedTimestampFloorMissing => {
                "failed_precondition.branch.row_pruning_retained_timestamp_floor_missing"
            }
            Self::CandidateMissingLevel => {
                "failed_precondition.branch.compaction_candidate_missing_level"
            }
            Self::CandidateMissingTable => {
                "failed_precondition.branch.compaction_candidate_missing_table"
            }
        }
    }

    /// Human-readable detail for display.
    const fn detail(self) -> &'static str {
        match self {
            Self::Generic(message) => message,
            Self::StaleCandidate => "compaction candidate superseded by concurrent maintenance",
            Self::ProofMissing => "branch compaction pruning requires an explicit retention proof",
            Self::ProofStale => "row pruning proof is stale",
            Self::ProofBranchMismatch => "row pruning proof branch must match branch state",
            Self::ProofUnsafeRecoveryHealth => "row pruning recovery health epoch must be nonzero",
            Self::ProofVisibleVersionBelowState => {
                "row pruning proof visible version must not be below branch state"
            }
            Self::RetainedFloorAboveVisible => {
                "row pruning retained version floor must not exceed visible version"
            }
            Self::TimestampFloorWithoutCoverage => {
                "row pruning timestamp floor requires retained timestamp coverage"
            }
            Self::PinnedViewBelowFloor => "row pruning proof is blocked by pinned read history",
            Self::InheritedLayerUnknown => "row pruning inherited-layer safety is unknown",
            Self::InheritedLayerUnsafe => "row pruning proof cannot ignore inherited layers",
            Self::SharedTableSafetyUnknown => {
                "row pruning cross-branch shared-table safety is unknown"
            }
            Self::TombstoneElisionMissing => {
                "row pruning tombstone elision requires bottommost proof"
            }
            Self::TombstoneElisionNotBottommost => {
                "row pruning tombstone elision requires bottommost compaction"
            }
            Self::TombstoneResurrectionRisk => {
                "row pruning tombstone elision would resurrect an older value"
            }
            Self::TtlElisionMissing => "row pruning expired rows require TTL proof",
            Self::TtlElisionNotBottommost => {
                "row pruning expired rows require bottommost compaction"
            }
            Self::TtlCutoffExceedsTimestampFloor => {
                "row pruning TTL cutoff must not exceed retained timestamp floor"
            }
            Self::ProofEpochInvalid => "row pruning proof epoch must be nonzero",
            Self::ProofFingerprintInvalid => "row pruning branch state fingerprint must be nonzero",
            Self::TableManifestCoverageBeyondFloor => {
                "row pruning table manifest coverage must include retained floor"
            }
            Self::RetainedTimestampFloorMissing => {
                "row pruning requires a retained timestamp floor"
            }
            Self::CandidateMissingLevel => "row pruning candidate references missing level",
            Self::CandidateMissingTable => "row pruning candidate references missing table",
        }
    }
}

impl fmt::Display for BranchCompactionInvalidity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.detail())
    }
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

    /// Stable error code suitable for telemetry and test assertions.
    ///
    /// For `InvalidCompaction`, the code is sourced from the typed
    /// `BranchCompactionInvalidity` reason; tests can therefore assert
    /// on `error.code() == BranchCompactionInvalidity::ProofStale.code()`
    /// without depending on the human-readable display text.
    #[allow(
        dead_code,
        reason = "tests in this crate assert on the code() string; downstream crates will consume it"
    )]
    pub(crate) const fn code(&self) -> &'static str {
        match self {
            Self::InvalidConfig { .. } => "failed_precondition.branch.config",
            Self::InvalidBranchState { .. } => "failed_precondition.branch.state",
            Self::BranchNotFound { .. } => "not_found.branch",
            Self::BranchAlreadyExists { .. } => "already_exists.branch",
            Self::InvalidBranchRow { .. } => "failed_precondition.branch.row",
            Self::InvalidReadBound { .. } => "failed_precondition.branch.read_bound",
            Self::InsufficientTimestampHistory { .. } => {
                "failed_precondition.branch.insufficient_timestamp_history"
            }
            Self::InvalidInheritedLayer { .. } => "failed_precondition.branch.inherited_layer",
            Self::InvalidReachability { .. } => "failed_precondition.branch.reachability",
            Self::InvalidCompaction { reason } => reason.code(),
            Self::InvalidSnapshotInstall { .. } => "failed_precondition.branch.snapshot_install",
            Self::TableRuntime { .. } => "failed_precondition.branch.table_runtime",
            Self::Publish { .. } => "failed_precondition.branch.publish",
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
                Self::InvalidSnapshotInstall { reason: left },
                Self::InvalidSnapshotInstall { reason: right },
            ) => left == right,
            (
                Self::InvalidCompaction { reason: left },
                Self::InvalidCompaction { reason: right },
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
