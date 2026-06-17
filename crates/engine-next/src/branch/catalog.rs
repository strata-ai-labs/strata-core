//! Engine branch catalog records.

use strata_core_next::{BranchId, CommitVersion, Timestamp};
use uuid::Uuid;

use super::name::{BranchName, DEFAULT_BRANCH};

const BRANCH_NAMESPACE: Uuid = Uuid::from_bytes([
    0x6b, 0xa7, 0xb8, 0x10, 0x9d, 0xad, 0x11, 0xd1, 0x80, 0xb4, 0x00, 0xc0, 0x4f, 0xd4, 0x30, 0xc8,
]);

pub(crate) const DEFAULT_BRANCH_ID: BranchId = BranchId::from_bytes([0; BranchId::BYTE_LEN]);
pub(crate) const DEFAULT_STORAGE_BRANCH_ID: BranchId =
    BranchId::from_bytes([0x01; BranchId::BYTE_LEN]);
pub(crate) const SYSTEM_BRANCH_ID: BranchId = BranchId::from_bytes([0xf0; BranchId::BYTE_LEN]);
pub(crate) const DEFAULT_BRANCH_GENERATION: u64 = 1;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BranchStatus {
    Active,
    Deleted,
}

impl BranchStatus {
    pub(crate) const fn is_active(self) -> bool {
        matches!(self, Self::Active)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchParentRecord {
    name: BranchName,
    branch_id: BranchId,
    generation: u64,
    fork_version: CommitVersion,
    fork_timestamp: Option<Timestamp>,
}

impl BranchParentRecord {
    pub(crate) const fn new(
        name: BranchName,
        branch_id: BranchId,
        generation: u64,
        fork_version: CommitVersion,
        fork_timestamp: Option<Timestamp>,
    ) -> Self {
        Self {
            name,
            branch_id,
            generation,
            fork_version,
            fork_timestamp,
        }
    }

    pub(crate) fn name(&self) -> &BranchName {
        &self.name
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) const fn fork_version(&self) -> CommitVersion {
        self.fork_version
    }

    pub(crate) const fn fork_timestamp(&self) -> Option<Timestamp> {
        self.fork_timestamp
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchCatalogRecord {
    name: BranchName,
    branch_id: BranchId,
    storage_branch_id: BranchId,
    generation: u64,
    status: BranchStatus,
    parent: Option<BranchParentRecord>,
    created_at: Option<CommitVersion>,
    deleted_at: Option<CommitVersion>,
    state_revision: u64,
}

impl BranchCatalogRecord {
    pub(crate) fn default_record() -> Self {
        Self {
            name: BranchName::default_branch(),
            branch_id: DEFAULT_BRANCH_ID,
            storage_branch_id: DEFAULT_STORAGE_BRANCH_ID,
            generation: DEFAULT_BRANCH_GENERATION,
            status: BranchStatus::Active,
            parent: None,
            created_at: None,
            deleted_at: None,
            state_revision: 0,
        }
    }

    pub(crate) fn root(name: BranchName, generation: u64) -> Self {
        let branch_id = derive_branch_id(&name);
        let storage_branch_id = storage_branch_id_for_product_branch_id(branch_id);
        Self::new(
            name,
            branch_id,
            storage_branch_id,
            generation,
            BranchStatus::Active,
            None,
            None,
            None,
            0,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) const fn new(
        name: BranchName,
        branch_id: BranchId,
        storage_branch_id: BranchId,
        generation: u64,
        status: BranchStatus,
        parent: Option<BranchParentRecord>,
        created_at: Option<CommitVersion>,
        deleted_at: Option<CommitVersion>,
        state_revision: u64,
    ) -> Self {
        Self {
            name,
            branch_id,
            storage_branch_id,
            generation,
            status,
            parent,
            created_at,
            deleted_at,
            state_revision,
        }
    }

    pub(crate) fn forked(name: BranchName, generation: u64, parent: BranchParentRecord) -> Self {
        let branch_id = derive_branch_id(&name);
        let storage_branch_id = storage_branch_id_for_product_branch_id(branch_id);
        Self::new(
            name,
            branch_id,
            storage_branch_id,
            generation,
            BranchStatus::Active,
            Some(parent),
            None,
            None,
            0,
        )
    }

    pub(crate) fn with_storage_facts(
        mut self,
        generation: u64,
        status: BranchStatus,
        created_at: Option<CommitVersion>,
        deleted_at: Option<CommitVersion>,
        state_revision: u64,
    ) -> Self {
        self.generation = generation;
        self.status = status;
        self.created_at = created_at;
        self.deleted_at = deleted_at;
        self.state_revision = state_revision;
        self
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn storage_branch_id(&self) -> BranchId {
        self.storage_branch_id
    }

    pub(crate) const fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) const fn status(&self) -> BranchStatus {
        self.status
    }

    pub(crate) const fn is_active(&self) -> bool {
        self.status.is_active()
    }

    pub(crate) const fn parent(&self) -> Option<&BranchParentRecord> {
        self.parent.as_ref()
    }

    pub(crate) const fn created_at(&self) -> Option<CommitVersion> {
        self.created_at
    }

    pub(crate) const fn deleted_at(&self) -> Option<CommitVersion> {
        self.deleted_at
    }

    pub(crate) const fn state_revision(&self) -> u64 {
        self.state_revision
    }

    pub(crate) fn name(&self) -> &BranchName {
        &self.name
    }
}

pub(crate) fn derive_branch_id(name: &BranchName) -> BranchId {
    derive_branch_id_text(name.as_str())
}

const fn storage_branch_id_for_product_branch_id(branch_id: BranchId) -> BranchId {
    if same_branch_id(branch_id, DEFAULT_BRANCH_ID) {
        DEFAULT_STORAGE_BRANCH_ID
    } else {
        branch_id
    }
}

const fn same_branch_id(left: BranchId, right: BranchId) -> bool {
    let mut index = 0;
    while index < BranchId::BYTE_LEN {
        if left.as_bytes()[index] != right.as_bytes()[index] {
            return false;
        }
        index += 1;
    }
    true
}

pub(crate) fn aliases_reserved_branch_identity(name: &str) -> bool {
    if name == DEFAULT_BRANCH {
        return false;
    }
    let branch_id = derive_branch_id_text(name);
    same_branch_id(branch_id, DEFAULT_BRANCH_ID)
        || same_branch_id(branch_id, DEFAULT_STORAGE_BRANCH_ID)
        || same_branch_id(branch_id, SYSTEM_BRANCH_ID)
        || same_branch_id(
            storage_branch_id_for_product_branch_id(branch_id),
            SYSTEM_BRANCH_ID,
        )
}

fn derive_branch_id_text(name: &str) -> BranchId {
    if name == DEFAULT_BRANCH {
        return DEFAULT_BRANCH_ID;
    }
    if let Ok(uuid) = Uuid::parse_str(name) {
        return BranchId::from_bytes(*uuid.as_bytes());
    }
    let uuid = Uuid::new_v5(&BRANCH_NAMESPACE, name.as_bytes());
    BranchId::from_bytes(*uuid.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::{aliases_reserved_branch_identity, derive_branch_id_text, DEFAULT_BRANCH_ID};

    #[test]
    fn branch_id_derivation_matches_locked_old_engine_anchors() {
        assert_eq!(derive_branch_id_text("default"), DEFAULT_BRANCH_ID);
        assert_eq!(
            derive_branch_id_text("main").as_bytes(),
            &[
                0x1f, 0x64, 0xc0, 0x67, 0xac, 0xf2, 0x50, 0x34, 0xa5, 0xd0, 0x92, 0xd5, 0x76, 0x41,
                0x38, 0xec,
            ]
        );
        assert_eq!(
            derive_branch_id_text("_system_").as_bytes(),
            &[
                0x0d, 0x39, 0x06, 0xa0, 0xd8, 0x09, 0x58, 0x17, 0xb9, 0x0b, 0x09, 0xb6, 0x0e, 0x63,
                0xc9, 0x7d,
            ]
        );
        assert_eq!(
            derive_branch_id_text("f47ac10b-58cc-4372-a567-0e02b2c3d479").as_bytes(),
            &[
                0xf4, 0x7a, 0xc1, 0x0b, 0x58, 0xcc, 0x43, 0x72, 0xa5, 0x67, 0x0e, 0x02, 0xb2, 0xc3,
                0xd4, 0x79,
            ]
        );
    }

    #[test]
    fn branch_id_alias_detection_rejects_reserved_internal_ids() {
        assert!(aliases_reserved_branch_identity(
            "00000000-0000-0000-0000-000000000000"
        ));
        assert!(aliases_reserved_branch_identity(
            "01010101-0101-0101-0101-010101010101"
        ));
        assert!(aliases_reserved_branch_identity(
            "f0f0f0f0-f0f0-f0f0-f0f0-f0f0f0f0f0f0"
        ));
        assert!(!aliases_reserved_branch_identity("default"));
        assert!(!aliases_reserved_branch_identity("main"));
    }
}
