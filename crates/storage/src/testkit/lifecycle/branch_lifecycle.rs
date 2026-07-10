//! Branch lifecycle conformance helpers.

use super::{ensure, script_byte};
use crate::branch::config::BranchRuntimeConfig;
use crate::branch::state::snapshot::{
    install_snapshot_rows_into_branches, BranchSnapshotInstallRequest,
};
use crate::branch::state::BranchLocalState;
use crate::commit::{CommitBranchGeneration, CommitBranchGenerationGuard};
use crate::lifecycle::{LifecycleBranchCatalog, LifecycleBranchStatus};
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use crate::testkit::TestkitError;
use strata_core::{BranchId, CommitVersion, Timestamp};

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LifecycleBranchLifecycleContractOutcome {
    catalog: usize,
    fork: usize,
    clear_delete: usize,
    pinned_view: usize,
    generation: usize,
    stale_work: usize,
}

impl LifecycleBranchLifecycleContractOutcome {
    pub const fn catalog_cases(&self) -> usize {
        self.catalog
    }

    pub const fn fork_cases(&self) -> usize {
        self.fork
    }

    pub const fn clear_delete_cases(&self) -> usize {
        self.clear_delete
    }

    pub const fn pinned_view_cases(&self) -> usize {
        self.pinned_view
    }

    pub const fn generation_cases(&self) -> usize {
        self.generation
    }

    pub const fn stale_work_cases(&self) -> usize {
        self.stale_work
    }
}

pub fn check_lifecycle_branch_lifecycle_contract(
    script: &[u8],
) -> Result<LifecycleBranchLifecycleContractOutcome, TestkitError> {
    let mut outcome = LifecycleBranchLifecycleContractOutcome::default();
    check_catalog(script, &mut outcome)?;
    check_fork(script, &mut outcome)?;
    check_clear_delete(script, &mut outcome)?;
    check_generation_and_stale_work(script, &mut outcome)?;
    Ok(outcome)
}

fn check_catalog(
    script: &[u8],
    outcome: &mut LifecycleBranchLifecycleContractOutcome,
) -> Result<(), TestkitError> {
    let first = branch_id(script_byte(script, 0).wrapping_add(1));
    let second = branch_id(script_byte(script, 1).wrapping_add(2));
    let mut catalog =
        LifecycleBranchCatalog::new(BranchRuntimeConfig::default()).map_err(testkit_error)?;
    catalog
        .create_branch(first, generation(1)?, Some(CommitVersion::new(1)))
        .map_err(testkit_error)?;
    catalog
        .create_branch(second, generation(1)?, Some(CommitVersion::new(1)))
        .map_err(testkit_error)?;
    ensure(
        catalog.list_branches(false).len() == 2,
        "branch catalog did not list active branches",
    )?;
    ensure(
        catalog.create_branch(first, generation(2)?, None).is_err(),
        "duplicate branch create did not reject",
    )?;
    outcome.catalog += 1;
    Ok(())
}

fn check_fork(
    script: &[u8],
    outcome: &mut LifecycleBranchLifecycleContractOutcome,
) -> Result<(), TestkitError> {
    let parent = branch_id(script_byte(script, 2).wrapping_add(3));
    let child = branch_id(script_byte(script, 3).wrapping_add(4));
    let grandchild = branch_id(script_byte(script, 4).wrapping_add(5));
    let mut catalog = LifecycleBranchCatalog::with_initial_branch(
        BranchRuntimeConfig::default(),
        parent,
        generation(1)?,
        Some(CommitVersion::new(1)),
    )
    .map_err(testkit_error)?;
    catalog
        .replace_active_branch_state(
            parent,
            CommitBranchGenerationGuard::exact(generation(1)?),
            owned_state(
                parent,
                &[
                    put_row(parent, 2, b"fork-model", b"old"),
                    put_row(parent, 5, b"fork-model", b"new"),
                ],
            )?,
        )
        .map_err(testkit_error)?;
    catalog
        .fork_current(parent, child, generation(1)?)
        .map_err(testkit_error)?;
    catalog
        .fork_at_retained_version(
            child,
            grandchild,
            generation(1)?,
            CommitVersion::new(2),
            CommitVersion::new(1),
        )
        .map_err(testkit_error)?;
    let key = physical_key(grandchild, b"fork-model")?;
    let view = catalog
        .capture_read_view(grandchild)
        .map_err(testkit_error)?;
    ensure(
        view.latest(&key)
            .map_err(testkit_error)?
            .is_some_and(|row| row.row().value() == b"old"),
        "fork-at-version did not preserve requested rows",
    )?;
    outcome.fork += 1;
    Ok(())
}

fn check_clear_delete(
    script: &[u8],
    outcome: &mut LifecycleBranchLifecycleContractOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 5).wrapping_add(6));
    let key = physical_key(branch, b"clear-delete")?;
    let mut catalog = LifecycleBranchCatalog::with_initial_branch(
        BranchRuntimeConfig::default(),
        branch,
        generation(1)?,
        Some(CommitVersion::new(1)),
    )
    .map_err(testkit_error)?;
    catalog
        .branch_state_mut(branch, CommitBranchGenerationGuard::exact(generation(1)?))
        .map_err(testkit_error)?
        .append_committed_row(put_row(branch, 2, b"clear-delete", b"old"))
        .map_err(testkit_error)?;
    let pinned = catalog.capture_read_view(branch).map_err(testkit_error)?;
    let clear = catalog
        .clear_branch(branch, CommitBranchGenerationGuard::exact(generation(1)?))
        .map_err(testkit_error)?;
    ensure(
        clear.descriptor().status() == LifecycleBranchStatus::Active,
        "clear did not return active descriptor",
    )?;
    ensure(
        pinned
            .latest(&key)
            .map_err(testkit_error)?
            .is_some_and(|row| row.row().value() == b"old"),
        "pinned clear view lost old row",
    )?;
    outcome.pinned_view += 1;
    catalog
        .delete_branch(
            branch,
            CommitBranchGenerationGuard::exact(generation(1)?),
            Some(CommitVersion::new(3)),
        )
        .map_err(testkit_error)?;
    ensure(
        catalog.capture_read_view(branch).is_err(),
        "deleted branch accepted new read view",
    )?;
    outcome.clear_delete += 1;
    Ok(())
}

fn check_generation_and_stale_work(
    script: &[u8],
    outcome: &mut LifecycleBranchLifecycleContractOutcome,
) -> Result<(), TestkitError> {
    let branch = branch_id(script_byte(script, 6).wrapping_add(7));
    let mut catalog = LifecycleBranchCatalog::with_initial_branch(
        BranchRuntimeConfig::default(),
        branch,
        generation(1)?,
        Some(CommitVersion::new(1)),
    )
    .map_err(testkit_error)?;
    let old_state = owned_state(branch, &[put_row(branch, 2, b"stale-work", b"old")])?;
    catalog
        .replace_active_branch_state(
            branch,
            CommitBranchGenerationGuard::exact(generation(1)?),
            old_state.clone(),
        )
        .map_err(testkit_error)?;
    let descriptor = catalog.lookup(branch).map_err(testkit_error)?;
    catalog
        .clear_branch(branch, CommitBranchGenerationGuard::exact(generation(1)?))
        .map_err(testkit_error)?;
    ensure(
        catalog
            .replace_active_branch_state_with_descriptor(descriptor, old_state)
            .is_err(),
        "stale branch rewrite resurrected cleared state",
    )?;
    outcome.stale_work += 1;
    catalog
        .delete_branch(
            branch,
            CommitBranchGenerationGuard::exact(generation(1)?),
            Some(CommitVersion::new(4)),
        )
        .map_err(testkit_error)?;
    ensure(
        catalog.create_branch(branch, generation(1)?, None).is_err(),
        "same generation recreated deleted branch",
    )?;
    catalog
        .create_branch(branch, generation(2)?, Some(CommitVersion::new(5)))
        .map_err(testkit_error)?;
    outcome.generation += 1;
    Ok(())
}

fn owned_state(branch_id: BranchId, rows: &[StorageRow]) -> Result<BranchLocalState, TestkitError> {
    let mut branches = vec![
        BranchLocalState::new(branch_id, BranchRuntimeConfig::default()).map_err(testkit_error)?,
    ];
    let request = BranchSnapshotInstallRequest::from_rows(
        format!("branch-contract-seed-{branch_id}"),
        rows.to_vec(),
    )
    .map_err(testkit_error)?;
    install_snapshot_rows_into_branches(&mut branches, &request).map_err(testkit_error)?;
    branches
        .pop()
        .ok_or_else(|| TestkitError::new("branch snapshot install lost branch"))
}

fn put_row(
    branch: BranchId,
    version: u64,
    user_key: &'static [u8],
    value: &'static [u8],
) -> StorageRow {
    StorageRow::put(
        physical_key(branch, user_key).expect("physical key"),
        CommitVersion::new(version),
        Timestamp::from_micros(version * 100),
        Timestamp::EPOCH,
        value.to_vec(),
    )
}

fn physical_key(branch: BranchId, user_key: &'static [u8]) -> Result<PhysicalKey, TestkitError> {
    PhysicalKey::new(
        branch,
        "branch-lifecycle-contract",
        StorageSpaceId::engine(0x79).expect("storage space"),
        user_key.to_vec(),
    )
    .map_err(testkit_error)
}

fn branch_id(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}

fn generation(value: u64) -> Result<CommitBranchGeneration, TestkitError> {
    CommitBranchGeneration::new(value).map_err(testkit_error)
}

fn testkit_error(error: impl std::fmt::Display) -> TestkitError {
    TestkitError::new(error.to_string())
}
