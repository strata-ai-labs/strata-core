mod allocator;
mod batch;
mod branch_registry;
mod cache;
mod conflict;
mod durable;
mod durable_gate;
mod guard;
mod outcome;
mod replay;
mod scaffold;
mod timeline;
mod visibility;

use super::cache::prepare_commit_rows;
use super::{
    commit_conflict_validation_needs_source, execute_read_only_diagnostic,
    validate_commit_conflicts, validate_commit_conflicts_without_source,
    CommitAdmissionPressureThresholds, CommitBatch, CommitBatchKind, CommitBatchOptions,
    CommitBranchApplyTarget, CommitBranchDescriptor, CommitBranchGeneration,
    CommitBranchGenerationGuard, CommitBranchGuardSet, CommitBranchReadViewConflictSource,
    CommitBranchRegistry, CommitBranchState, CommitCacheRuntime, CommitCasFact, CommitConflict,
    CommitConflictKind, CommitConflictReadSource, CommitConflictValidationMode,
    CommitDuplicateKeyPolicy, CommitDurabilityClass, CommitDurabilityMode, CommitDurableRuntime,
    CommitExpiry, CommitFactAllocation, CommitFactAllocator, CommitLowerLayer,
    CommitManualTimestampSource, CommitMutation, CommitMutationCounts, CommitObservedVersion,
    CommitOrigin, CommitOutcome, CommitOutcomeKind, CommitPhase, CommitReadFact,
    CommitReadOnlyDiagnostics, CommitReadSnapshot, CommitReplayAction, CommitReplayReport,
    CommitReplayRequest, CommitReplayRuntime, CommitRetentionHint, CommitRuntimeConfig,
    CommitRuntimeError, CommitRuntimeResult, CommitStamp, CommitTimelineEntry, CommitTimelineFact,
    CommitTimelineLookup, CommitTimelineMiss, CommitTimelineRowKind, CommitTimelineRows,
    CommitTimelineView, CommitTimestampAllocationSource, CommitTimestampGuard,
    CommitTimestampPolicy, CommitTimestampSource, CommitUnresolvedDurable,
    CommitUnresolvedDurableGate, CommitUnresolvedDurableKind, CommitValidationFacts,
    CommitVersionAllocator, CommitVisibilityFacts, CommitVisiblePublisher, CommitWalAppendError,
    CommitWalAppendFacts, CommitWalAppender, ValidatedCommitBatch, VisibleVersionPublish,
    VisibleVersionTracker, COMMIT_TIMELINE_SPACE,
};
use crate::config::mode::DurabilityPolicy;
use crate::format::{FormatError, WalCommitPayload, WalRecord};
use crate::row::{PhysicalKey, StorageSpaceId};
use std::error::Error;
use std::fmt;
use std::num::NonZeroUsize;
use strata_core_next::{BranchId, CommitVersion, Timestamp};

fn branch_id(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}

fn physical_key(branch_id: BranchId, storage_space_id: u8, user_key: Vec<u8>) -> PhysicalKey {
    PhysicalKey::new(
        branch_id,
        "default",
        StorageSpaceId::engine(storage_space_id).expect("engine-owned space"),
        user_key,
    )
    .expect("physical key")
}

fn storage_owned_key(branch_id: BranchId, user_key: Vec<u8>) -> PhysicalKey {
    PhysicalKey::new(
        branch_id,
        "timeline",
        StorageSpaceId::COMMIT_TIMELINE,
        user_key,
    )
    .expect("storage-owned physical key")
}

fn read_only_batch(branch: BranchId, options: CommitBatchOptions) -> ValidatedCommitBatch {
    CommitBatch::read_only_diagnostic(branch, CommitValidationFacts::empty(), options)
        .validate(&CommitRuntimeConfig::default())
        .expect("valid read-only diagnostic batch")
}

fn assert_bounded_storage_debug(debug: &str) {
    assert!(!debug.contains("VersionedValue"));
    assert!(!debug.contains("Transaction"));
    assert!(!debug.contains("rollback"));
}
