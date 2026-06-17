//! Engine-owned row addresses and mutations.

use strata_core_next::BranchId;
use strata_core_next::{CommitVersion, Timestamp};

use super::RowClass;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RowAddress {
    branch_id: BranchId,
    row_class: RowClass,
    key: Vec<u8>,
}

impl RowAddress {
    pub(crate) const fn new(branch_id: BranchId, row_class: RowClass, key: Vec<u8>) -> Self {
        Self {
            branch_id,
            row_class,
            key,
        }
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn row_class(&self) -> RowClass {
        self.row_class
    }

    pub(crate) fn key(&self) -> &[u8] {
        &self.key
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum RowMutation {
    Put { address: RowAddress, value: Vec<u8> },
    Delete { address: RowAddress },
}

impl RowMutation {
    pub(crate) fn put(address: RowAddress, value: Vec<u8>) -> Self {
        Self::Put { address, value }
    }

    pub(crate) const fn delete(address: RowAddress) -> Self {
        Self::Delete { address }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ReadSelector {
    Latest,
    AtVersion(CommitVersion),
    AtTimestamp(Timestamp),
}
