use super::data::{map_commit_admission_pressure_reason, map_commit_admission_pressure_severity};
#[cfg(any(test, feature = "testkit"))]
use super::maintenance::map_maintenance_summary;
use super::{
    CommitBranchGeneration, LifecycleError, RecoveryHealth, RecoveryHealthSummary, StorageApiError,
    StorageApiLowerLayer, StorageApiResult, DEFAULT_BRANCH_GENERATION,
};
#[cfg(any(test, feature = "testkit"))]
use super::{LifecycleMaintenanceOutcome, MaintenanceRequest, MaintenanceSummary};

/// Surface a storage memory-budget breach as a typed `resource_exhausted` API error.
///
/// Both budget admission paths converge here: the global pre-commit check returns
/// `LifecycleError::StorageBudgetExceeded` directly, while the per-pool commit check
/// wraps the same error under `CommitLowerLayer::StorageBudget`. Routing both through
/// one mapping keeps a budget refusal a caller-actionable resource error instead of an
/// internal lower-layer failure.
fn budget_exceeded_to_api(error: &LifecycleError) -> Option<StorageApiError> {
    match error {
        LifecycleError::StorageBudgetExceeded {
            pool,
            requested_bytes,
            used_bytes,
            limit_bytes,
            reason,
            ..
        } => Some(StorageApiError::ResourceExhausted {
            resource: pool.name(),
            requested_bytes: *requested_bytes,
            used_bytes: *used_bytes,
            limit_bytes: *limit_bytes,
            reason,
        }),
        _ => None,
    }
}

pub(super) fn branch_error(error: crate::branch::error::BranchRuntimeError) -> StorageApiError {
    match error {
        crate::branch::error::BranchRuntimeError::InsufficientTimestampHistory {
            branch_id,
            ..
        } => StorageApiError::TimestampHistoryUnavailable {
            branch_id,
            reason: "timestamp is outside retained branch history",
        },
        other => {
            // TCP3.2a: `BranchRuntimeError::code()` already existed and was
            // dead — every branch failure reached the engine as one
            // indistinguishable code. Carry it across the boundary instead.
            let code = other.code();
            StorageApiError::lower_layer_coded(
                StorageApiLowerLayer::Branch,
                code,
                "branch read failed",
                other,
            )
        }
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "storage API keeps commit error mapping in one exhaustive registry"
)]
pub(super) fn commit_error(error: crate::commit::CommitRuntimeError) -> StorageApiError {
    match error {
        crate::commit::CommitRuntimeError::InvalidBatch { reason }
        | crate::commit::CommitRuntimeError::InvalidMutation { reason }
        | crate::commit::CommitRuntimeError::InvalidValidationFacts { reason }
        | crate::commit::CommitRuntimeError::InvalidTimestampPolicy { reason } => {
            StorageApiError::InvalidArgument {
                field: "commit",
                reason,
            }
        }
        crate::commit::CommitRuntimeError::DuplicateMutationKey { .. } => {
            StorageApiError::InvalidArgument {
                field: "mutations",
                reason: "commit batch must not contain duplicate keys",
            }
        }
        crate::commit::CommitRuntimeError::StorageOwnedMutationSpace { .. } => {
            StorageApiError::InvalidArgument {
                field: "storage_space",
                reason: "storage-owned commit spaces are not accepted by the API",
            }
        }
        crate::commit::CommitRuntimeError::BranchNotFound { branch_id } => {
            StorageApiError::BranchNotFound { branch_id }
        }
        crate::commit::CommitRuntimeError::BranchAlreadyExists { branch_id } => {
            StorageApiError::BranchAlreadyExists { branch_id }
        }
        crate::commit::CommitRuntimeError::BranchGenerationMismatch {
            branch_id,
            expected,
            actual,
        } => StorageApiError::BranchGenerationMismatch {
            branch_id,
            expected,
            actual,
        },
        crate::commit::CommitRuntimeError::BranchNotWritable { reason, .. }
        | crate::commit::CommitRuntimeError::BranchGuardUnavailable { reason, .. }
        | crate::commit::CommitRuntimeError::CommitQuiesceUnavailable { reason }
        | crate::commit::CommitRuntimeError::BranchUnavailable { reason } => {
            StorageApiError::InvalidRuntimeState { reason }
        }
        crate::commit::CommitRuntimeError::InvalidCommitPhase {
            reason: "read-only diagnostics are disabled",
        } => StorageApiError::UnsupportedCapability {
            capability: "read_only_diagnostics",
            reason: "read-only diagnostics are disabled",
        },
        crate::commit::CommitRuntimeError::CommitConflict { conflict } => {
            StorageApiError::Conflict {
                branch_id: conflict.branch_id(),
                storage_space: Some(conflict.storage_space_id().raw()),
                key_fingerprint: Some(conflict.key_fingerprint()),
                user_key_len: Some(conflict.user_key_len()),
                reason: "commit condition was not satisfied",
            }
        }
        crate::commit::CommitRuntimeError::DurabilityUnavailable { reason } => {
            StorageApiError::UnsupportedCapability {
                capability: "commit_durability",
                reason,
            }
        }
        crate::commit::CommitRuntimeError::DurabilityUncertain { reason, source, .. }
        | crate::commit::CommitRuntimeError::DurableButNotVisible { reason, source, .. } => {
            StorageApiError::DurableUncertain { reason, source }
        }
        crate::commit::CommitRuntimeError::UnresolvedDurableCommit { reason, .. }
        | crate::commit::CommitRuntimeError::AppliedButNotVisible { reason, .. } => {
            StorageApiError::durable_uncertain(reason)
        }
        crate::commit::CommitRuntimeError::InvalidTimelineFact { .. }
        | crate::commit::CommitRuntimeError::TimelineConflict { .. } => {
            StorageApiError::lower_layer_with(
                StorageApiLowerLayer::Commit,
                "commit timeline facts are invalid",
                error,
            )
        }
        crate::commit::CommitRuntimeError::LowerLayer {
            layer: crate::commit::CommitLowerLayer::StorageBudget,
            reason,
            source,
        } => {
            let mapped = source
                .as_deref()
                .and_then(|inner| inner.downcast_ref::<LifecycleError>())
                .and_then(budget_exceeded_to_api);
            mapped.unwrap_or(StorageApiError::LowerLayer {
                layer: StorageApiLowerLayer::Commit,
                // TCP3.2b wires `CommitRuntimeError::code()` here.
                inner_code: None,
                reason,
                source,
            })
        }
        other => StorageApiError::lower_layer_with(
            StorageApiLowerLayer::Commit,
            "commit runtime failed",
            other,
        ),
    }
}

pub(super) fn map_recovery_health(health: &RecoveryHealth) -> RecoveryHealthSummary {
    match health {
        RecoveryHealth::Healthy => RecoveryHealthSummary::Healthy,
        RecoveryHealth::Degraded { .. } => RecoveryHealthSummary::Degraded,
        RecoveryHealth::Failed { .. } => RecoveryHealthSummary::Failed,
    }
}

#[expect(
    clippy::too_many_lines,
    reason = "storage API keeps lifecycle error mapping in one exhaustive registry"
)]
pub(super) fn map_lifecycle_error(error: LifecycleError) -> StorageApiError {
    match error {
        LifecycleError::InvalidConfig { field, reason } => {
            StorageApiError::InvalidArgument { field, reason }
        }
        LifecycleError::InvalidOpenPlan { reason } => StorageApiError::InvalidArgument {
            field: "open_options",
            reason,
        },
        LifecycleError::InvalidLifecycleState { reason }
        | LifecycleError::PinnedViewReleaseBlocked { reason, .. } => {
            StorageApiError::InvalidRuntimeState { reason }
        }
        LifecycleError::BranchNotFound { branch_id } => {
            StorageApiError::BranchNotFound { branch_id }
        }
        LifecycleError::BranchAlreadyExists { branch_id } => {
            StorageApiError::BranchAlreadyExists { branch_id }
        }
        LifecycleError::BranchGenerationMismatch {
            branch_id,
            expected,
            actual,
        } => StorageApiError::BranchGenerationMismatch {
            branch_id,
            expected,
            actual,
        },
        LifecycleError::BranchNotWritable { state, .. } => {
            StorageApiError::InvalidRuntimeState { reason: state }
        }
        LifecycleError::BranchGenerationExhausted { .. } => StorageApiError::InvalidRuntimeState {
            reason: "branch generation is exhausted",
        },
        LifecycleError::BranchHistoryUnavailable { branch_id, reason } => {
            StorageApiError::RetainedHistoryUnavailable { branch_id, reason }
        }
        LifecycleError::InsufficientTimestampHistory { branch_id, reason } => {
            StorageApiError::TimestampHistoryUnavailable { branch_id, reason }
        }
        LifecycleError::SourceHasUnflushedRows { .. } => StorageApiError::InvalidRuntimeState {
            reason: "source branch has unflushed rows",
        },
        LifecycleError::CapabilityMismatch { .. } => StorageApiError::UnsupportedCapability {
            capability: "backend",
            reason: "backend capabilities do not satisfy storage mode",
        },
        LifecycleError::MaintenanceFailed { reason }
        | LifecycleError::MaintenanceQueueFull { reason }
        | LifecycleError::MaintenanceTaskFailed { reason }
        | LifecycleError::RetentionBlocked { reason }
        | LifecycleError::QuarantineProofBlocked { reason }
        | LifecycleError::PurgeProofBlocked { reason }
        | LifecycleError::WalRetentionProofIncomplete { reason }
        | LifecycleError::FlushPublicationFailed { reason }
        | LifecycleError::CheckpointPublicationFailed { reason }
        | LifecycleError::CheckpointSnapshotOrphaned { reason, .. } => {
            StorageApiError::MaintenanceRejected { reason }
        }
        LifecycleError::StoragePressureRejected {
            branch_id,
            severity,
            pressure_reason,
            retryable,
            reason,
            ..
        } => StorageApiError::StoragePressure {
            branch_id,
            severity: map_commit_admission_pressure_severity(severity),
            pressure_reason: map_commit_admission_pressure_reason(pressure_reason),
            reason,
            retryable,
        },
        LifecycleError::FlushPublicationUncertain { reason, source }
        | LifecycleError::FlushPublicationOrphaned { reason, source, .. }
        | LifecycleError::RewritePublicationUncertain { reason, source, .. }
        | LifecycleError::RewritePublicationOrphaned { reason, source, .. }
        | LifecycleError::TableManifestPublicationUncertain { reason, source } => {
            StorageApiError::DurableUncertain { reason, source }
        }
        LifecycleError::RewritePublicationFailed { reason, source }
        | LifecycleError::TableManifestPublicationFailed { reason, source } => {
            StorageApiError::LowerLayer {
                layer: StorageApiLowerLayer::Lifecycle,
                inner_code: None,
                reason,
                source,
            }
        }
        LifecycleError::LowerLayer {
            layer: crate::lifecycle::LifecycleLowerLayer::CommitRuntime,
            source: Some(source),
            ..
        } => source
            .as_ref()
            .downcast_ref::<crate::commit::CommitRuntimeError>()
            .cloned()
            .map_or_else(
                || {
                    StorageApiError::lower_layer_with(
                        StorageApiLowerLayer::Lifecycle,
                        "lifecycle commit runtime failed",
                        LifecycleError::LowerLayer {
                            layer: crate::lifecycle::LifecycleLowerLayer::CommitRuntime,
                            reason: "commit runtime failed",
                            source: Some(source),
                        },
                    )
                },
                commit_error,
            ),
        LifecycleError::StorageBudgetExceeded {
            pool,
            requested_bytes,
            used_bytes,
            limit_bytes,
            reason,
            ..
        } => StorageApiError::ResourceExhausted {
            resource: pool.name(),
            requested_bytes,
            used_bytes,
            limit_bytes,
            reason,
        },
        other => StorageApiError::lower_layer_with(
            StorageApiLowerLayer::Lifecycle,
            "lifecycle runtime failed",
            other,
        ),
    }
}

pub(super) fn default_branch_generation() -> StorageApiResult<CommitBranchGeneration> {
    CommitBranchGeneration::new(DEFAULT_BRANCH_GENERATION).map_err(|error| {
        StorageApiError::lower_layer_with(
            StorageApiLowerLayer::Commit,
            "commit branch generation failed",
            error,
        )
    })
}

#[cfg(any(test, feature = "testkit"))]
pub(crate) fn map_commit_error_for_test(
    error: crate::commit::CommitRuntimeError,
) -> StorageApiError {
    commit_error(error)
}

#[cfg(any(test, feature = "testkit"))]
pub(crate) fn map_lifecycle_error_for_test(error: LifecycleError) -> StorageApiError {
    map_lifecycle_error(error)
}

#[cfg(any(test, feature = "testkit"))]
pub(crate) fn map_maintenance_outcome_for_test(
    request: MaintenanceRequest,
    outcome: &LifecycleMaintenanceOutcome,
) -> MaintenanceSummary {
    map_maintenance_summary(request, outcome)
}

#[cfg(test)]
mod tests {
    use super::branch_error;
    use crate::api::{StorageApiErrorClass, StorageApiLowerLayer};
    use crate::branch::error::BranchRuntimeError;

    /// The real mapping — not a hand-built error — must carry the branch
    /// error's own code across the boundary. Before TCP3.2a this arm threw
    /// the discriminant away and every branch failure arrived as one
    /// indistinguishable code, which is why `BranchRuntimeError::code()`
    /// sat behind a `dead_code` allow.
    #[test]
    fn unmapped_branch_errors_carry_their_code_across_the_boundary() {
        let mapped = branch_error(BranchRuntimeError::InvalidReadBound {
            reason: "read bound is not valid",
        });
        assert_eq!(
            mapped.inner_code(),
            Some("failed_precondition.branch.read_bound")
        );
        assert_eq!(mapped.code(), "internal.storage_api.branch");
        assert_eq!(mapped.class(), StorageApiErrorClass::Internal);
        assert!(matches!(
            mapped,
            crate::api::StorageApiError::LowerLayer {
                layer: StorageApiLowerLayer::Branch,
                ..
            }
        ));
    }

    /// Distinct branch failures stay distinct after mapping.
    #[test]
    fn distinct_branch_errors_map_to_distinct_inner_codes() {
        let bound = branch_error(BranchRuntimeError::InvalidReadBound {
            reason: "bad bound",
        });
        let state = branch_error(BranchRuntimeError::InvalidBranchState {
            reason: "bad state",
        });
        assert_ne!(bound.inner_code(), state.inner_code());
        assert_eq!(bound.code(), state.code(), "same layer, same API code");
    }

    /// The explicitly-mapped arm keeps its dedicated API variant: carrying
    /// inner codes must not swallow errors the API already models.
    #[test]
    fn explicitly_mapped_branch_errors_keep_their_api_variant() {
        let mapped = branch_error(BranchRuntimeError::InsufficientTimestampHistory {
            branch_id: strata_core::BranchId::from_bytes([1; 16]),
            requested_timestamp: strata_core::Timestamp::from_micros(1),
            earliest_available_timestamp: Some(strata_core::Timestamp::from_micros(2)),
            source: crate::branch::error::BranchTimestampHistorySource::OwnState,
        });
        assert_eq!(mapped.class(), StorageApiErrorClass::HistoryUnavailable);
        assert_eq!(mapped.code(), "history_unavailable.storage_api.timestamp");
        assert_eq!(mapped.inner_code(), None, "not a LowerLayer error");
    }
}
