use super::*;
use crate::branch::{
    install_snapshot_rows_into_branches, BranchHistoryOptions, BranchLocalState,
    BranchMaterializationRequest, BranchReadBound, BranchRuntimeConfig, BranchScanBounds,
    BranchSnapshotInstallRequest, BranchTableReferenceKind, BranchTimestampCoverage,
};
use crate::commit::{CommitBranchGeneration, CommitBranchGenerationGuard};
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use strata_core_next::{BranchId, Timestamp};

mod catalog;
mod clear_delete;
mod fork;
mod isolation;

fn catalog_with_branch(
    branch_id: BranchId,
    generation: CommitBranchGeneration,
) -> LifecycleBranchCatalog {
    LifecycleBranchCatalog::with_initial_branch(
        BranchRuntimeConfig::default(),
        branch_id,
        generation,
        None,
    )
    .expect("catalog")
}

fn owned_state(branch_id: BranchId, rows: &[StorageRow]) -> BranchLocalState {
    let mut branches =
        vec![BranchLocalState::new(branch_id, BranchRuntimeConfig::default()).expect("branch")];
    let request = BranchSnapshotInstallRequest::from_rows(
        format!("branch-test-seed-{branch_id}"),
        rows.to_vec(),
    )
    .expect("snapshot request");
    install_snapshot_rows_into_branches(&mut branches, &request).expect("snapshot install");
    branches.pop().expect("branch state")
}

fn owned_state_with_timestamp_coverage(
    branch_id: BranchId,
    rows: &[StorageRow],
    coverage: BranchTimestampCoverage,
) -> BranchLocalState {
    let mut state = owned_state(branch_id, rows);
    state.set_timestamp_coverage(coverage);
    state
}

fn put_row(
    branch: BranchId,
    version: u64,
    user_key: &'static [u8],
    value: &'static [u8],
) -> StorageRow {
    StorageRow::put(
        physical_key(branch, user_key),
        CommitVersion::new(version),
        Timestamp::from_micros(version * 100),
        Timestamp::EPOCH,
        value.to_vec(),
    )
}

fn tombstone_row(branch: BranchId, version: u64, user_key: &'static [u8]) -> StorageRow {
    StorageRow::tombstone(
        physical_key(branch, user_key),
        CommitVersion::new(version),
        Timestamp::from_micros(version * 100),
    )
}

fn physical_key(branch: BranchId, user_key: &'static [u8]) -> PhysicalKey {
    PhysicalKey::new(
        branch,
        "branch-lifecycle",
        StorageSpaceId::engine(0x79).expect("storage space"),
        user_key.to_vec(),
    )
    .expect("physical key")
}

fn branch_id(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}

fn generation(value: u64) -> CommitBranchGeneration {
    CommitBranchGeneration::new(value).expect("generation")
}
