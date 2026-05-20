mod batch;
mod scaffold;

use super::{
    CommitBatch, CommitBatchKind, CommitBatchOptions, CommitCasFact, CommitConflictValidationMode,
    CommitDuplicateKeyPolicy, CommitDurabilityClass, CommitDurabilityMode, CommitExpiry,
    CommitLowerLayer, CommitMutation, CommitObservedVersion, CommitOrigin, CommitPhase,
    CommitReadFact, CommitReadOnlyDiagnostics, CommitRetentionHint, CommitRuntimeConfig,
    CommitRuntimeError, CommitRuntimeResult, CommitRuntimeStats, CommitStamp,
    CommitTimestampPolicy, CommitValidationFacts, CommitVisibilityFacts,
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
