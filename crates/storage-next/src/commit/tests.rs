mod allocator;
mod batch;
mod outcome;
mod scaffold;
mod visibility;

use super::{
    execute_read_only_diagnostic, CommitBatch, CommitBatchKind, CommitBatchOptions, CommitCasFact,
    CommitConflictValidationMode, CommitDuplicateKeyPolicy, CommitDurabilityClass,
    CommitDurabilityMode, CommitExpiry, CommitFactAllocation, CommitFactAllocator,
    CommitLowerLayer, CommitManualTimestampSource, CommitMutation, CommitMutationCounts,
    CommitObservedVersion, CommitOrigin, CommitOutcome, CommitOutcomeKind, CommitPhase,
    CommitReadFact, CommitReadOnlyDiagnostics, CommitReadSnapshot, CommitRetentionHint,
    CommitRuntimeConfig, CommitRuntimeError, CommitRuntimeResult, CommitRuntimeStats, CommitStamp,
    CommitTimestampAllocationSource, CommitTimestampGuard, CommitTimestampPolicy,
    CommitTimestampSource, CommitValidationFacts, CommitVersionAllocator, CommitVisibilityFacts,
    ValidatedCommitBatch, VisibleVersionPublish, VisibleVersionTracker,
};
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
