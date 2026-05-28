//! API commit request shells.

use std::collections::BTreeSet;
use std::time::Duration;

use strata_core_next::BranchId;

use super::{StorageApiError, StorageApiResult, StorageKey, StorageSpaceId, StorageValue};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitMutation {
    Put {
        storage_space: StorageSpaceId,
        key: StorageKey,
        value: StorageValue,
        ttl: Option<Duration>,
    },
    Delete {
        storage_space: StorageSpaceId,
        key: StorageKey,
    },
}

impl CommitMutation {
    #[must_use]
    pub const fn storage_space(&self) -> &StorageSpaceId {
        match self {
            Self::Put { storage_space, .. } | Self::Delete { storage_space, .. } => storage_space,
        }
    }

    #[must_use]
    pub const fn key(&self) -> &StorageKey {
        match self {
            Self::Put { key, .. } | Self::Delete { key, .. } => key,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CommitOptions {
    require_conflict_check: bool,
}

impl CommitOptions {
    #[must_use]
    pub const fn require_conflict_check(mut self, enabled: bool) -> Self {
        self.require_conflict_check = enabled;
        self
    }

    #[must_use]
    pub const fn conflict_check_required(self) -> bool {
        self.require_conflict_check
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitBatch {
    branch_id: BranchId,
    mutations: Vec<CommitMutation>,
    options: CommitOptions,
}

impl CommitBatch {
    pub fn new(
        branch_id: BranchId,
        mutations: Vec<CommitMutation>,
        options: CommitOptions,
    ) -> StorageApiResult<Self> {
        if mutations.is_empty() {
            return Err(StorageApiError::InvalidArgument {
                field: "mutations",
                reason: "commit batch must contain at least one mutation",
            });
        }

        let mut seen = BTreeSet::new();
        for mutation in &mutations {
            let identity = (mutation.storage_space(), mutation.key());
            if !seen.insert(identity) {
                return Err(StorageApiError::InvalidArgument {
                    field: "mutations",
                    reason: "commit batch must not contain duplicate keys",
                });
            }
        }

        Ok(Self {
            branch_id,
            mutations,
            options,
        })
    }

    #[must_use]
    pub const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    #[must_use]
    pub fn mutations(&self) -> &[CommitMutation] {
        &self.mutations
    }

    #[must_use]
    pub const fn options(&self) -> CommitOptions {
        self.options
    }
}
