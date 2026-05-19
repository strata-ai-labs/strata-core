//! Branch-local row identity and branch-id rewrite helpers.

use super::{BranchRuntimeError, BranchRuntimeResult};
use crate::row::{PhysicalKey, StorageRow};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchRowIdentity {
    branch_id: BranchId,
    physical_key: PhysicalKey,
    commit_version: CommitVersion,
    commit_timestamp: Timestamp,
}

impl BranchRowIdentity {
    pub(crate) fn from_row(
        expected_branch_id: BranchId,
        row: &StorageRow,
    ) -> BranchRuntimeResult<Self> {
        require_physical_key_branch(expected_branch_id, row.physical_key())?;
        Ok(Self {
            branch_id: expected_branch_id,
            physical_key: row.physical_key().clone(),
            commit_version: row.commit_version(),
            commit_timestamp: row.commit_timestamp(),
        })
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn physical_key(&self) -> &PhysicalKey {
        &self.physical_key
    }

    pub(crate) const fn commit_version(&self) -> CommitVersion {
        self.commit_version
    }

    pub(crate) const fn commit_timestamp(&self) -> Timestamp {
        self.commit_timestamp
    }
}

pub(crate) fn row_matches_branch(expected_branch_id: BranchId, row: &StorageRow) -> bool {
    row.physical_key().branch_id() == expected_branch_id
}

pub(crate) fn require_row_branch(
    expected_branch_id: BranchId,
    row: &StorageRow,
) -> BranchRuntimeResult<BranchRowIdentity> {
    BranchRowIdentity::from_row(expected_branch_id, row)
}

pub(crate) fn require_physical_key_branch(
    expected_branch_id: BranchId,
    physical_key: &PhysicalKey,
) -> BranchRuntimeResult<()> {
    if physical_key.branch_id() == expected_branch_id {
        Ok(())
    } else {
        Err(BranchRuntimeError::InvalidBranchRow {
            reason: "physical key branch id does not match branch",
        })
    }
}

pub(crate) fn rewrite_physical_key_branch(
    physical_key: &PhysicalKey,
    target_branch_id: BranchId,
) -> BranchRuntimeResult<PhysicalKey> {
    PhysicalKey::new(
        target_branch_id,
        physical_key.space().to_owned(),
        physical_key.storage_space_id(),
        physical_key.user_key().to_vec(),
    )
    .map_err(|_| BranchRuntimeError::InvalidBranchRow {
        reason: "rewritten physical key is invalid",
    })
}

pub(crate) fn rewrite_row_branch(
    row: &StorageRow,
    source_branch_id: BranchId,
    target_branch_id: BranchId,
) -> BranchRuntimeResult<StorageRow> {
    require_physical_key_branch(source_branch_id, row.physical_key())?;
    let physical_key = rewrite_physical_key_branch(row.physical_key(), target_branch_id)?;
    if row.is_tombstone() {
        Ok(StorageRow::tombstone(
            physical_key,
            row.commit_version(),
            row.commit_timestamp(),
        ))
    } else {
        Ok(StorageRow::put(
            physical_key,
            row.commit_version(),
            row.commit_timestamp(),
            row.expires_at(),
            row.value().to_vec(),
        ))
    }
}
