//! Engine branch catalog records.

use sha2::{Digest, Sha256};
use strata_core_next::BranchId;

use super::name::{BranchName, DEFAULT_BRANCH};

pub(crate) const DEFAULT_BRANCH_ID: BranchId = BranchId::from_bytes([0x01; BranchId::BYTE_LEN]);
pub(crate) const SYSTEM_BRANCH_ID: BranchId = BranchId::from_bytes([0xf0; BranchId::BYTE_LEN]);
pub(crate) const DEFAULT_BRANCH_GENERATION: u64 = 1;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchCatalogRecord {
    name: BranchName,
    branch_id: BranchId,
    generation: u64,
    source: Option<BranchId>,
}

impl BranchCatalogRecord {
    pub(crate) fn default_record() -> Self {
        Self {
            name: BranchName::default_branch(),
            branch_id: DEFAULT_BRANCH_ID,
            generation: DEFAULT_BRANCH_GENERATION,
            source: None,
        }
    }

    pub(crate) const fn new(
        name: BranchName,
        branch_id: BranchId,
        generation: u64,
        source: Option<BranchId>,
    ) -> Self {
        Self {
            name,
            branch_id,
            generation,
            source,
        }
    }

    pub(crate) fn derived(name: BranchName, source: BranchId) -> Self {
        let branch_id = derive_branch_id(&name);
        Self::new(name, branch_id, DEFAULT_BRANCH_GENERATION, Some(source))
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) const fn source(&self) -> Option<BranchId> {
        self.source
    }

    pub(crate) fn name(&self) -> &BranchName {
        &self.name
    }
}

pub(crate) fn derive_branch_id(name: &BranchName) -> BranchId {
    if name.as_str() == DEFAULT_BRANCH {
        return DEFAULT_BRANCH_ID;
    }

    let mut hasher = Sha256::new();
    hasher.update(b"strata-engine.branch-id.v1\0");
    hasher.update(name.as_str().as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0; BranchId::BYTE_LEN];
    bytes.copy_from_slice(&digest[..BranchId::BYTE_LEN]);
    if matches!(bytes[0], 0x00 | 0x01 | 0xf0) {
        bytes[0] ^= 0x80;
    }
    BranchId::from_bytes(bytes)
}
