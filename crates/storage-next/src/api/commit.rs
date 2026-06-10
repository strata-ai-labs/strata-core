//! API commit request shells.

use std::collections::BTreeSet;
use std::time::Duration;

use strata_core_next::{BranchId, CommitVersion};

use super::{
    BranchGeneration, StorageApiError, StorageApiResult, StorageKey, StorageSpaceId, StorageValue,
};

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

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum CommitDurability {
    #[default]
    RuntimeDefault,
    NotDurable,
    Standard,
    Always,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitDurabilitySummary {
    NotDurable,
    Standard,
    Always,
    Uncertain,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitExpectedVersion {
    /// The key must not have a currently visible row.
    Absent,
    /// The key must have the exact currently visible commit version.
    Present(CommitVersion),
}

/// A compare-and-set condition for one storage key.
///
/// Conditions map to L7 CAS validation. They are explicit per-key checks, not a
/// captured read set: reads and scans performed before building the batch are
/// not remembered unless the caller adds matching conditions.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitCondition {
    storage_space: StorageSpaceId,
    key: StorageKey,
    expected: CommitExpectedVersion,
}

impl CommitCondition {
    #[must_use]
    pub const fn new(
        storage_space: StorageSpaceId,
        key: StorageKey,
        expected: CommitExpectedVersion,
    ) -> Self {
        Self {
            storage_space,
            key,
            expected,
        }
    }

    #[must_use]
    pub const fn expected_absent(storage_space: StorageSpaceId, key: StorageKey) -> Self {
        Self::new(storage_space, key, CommitExpectedVersion::Absent)
    }

    #[must_use]
    pub const fn expected_present(
        storage_space: StorageSpaceId,
        key: StorageKey,
        version: CommitVersion,
    ) -> Self {
        Self::new(storage_space, key, CommitExpectedVersion::Present(version))
    }

    #[must_use]
    pub const fn storage_space(&self) -> &StorageSpaceId {
        &self.storage_space
    }

    #[must_use]
    pub const fn key(&self) -> &StorageKey {
        &self.key
    }

    #[must_use]
    pub const fn expected(&self) -> CommitExpectedVersion {
        self.expected
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CommitOptions {
    require_conflict_check: bool,
    durability: CommitDurability,
    expected_generation: Option<BranchGeneration>,
}

impl CommitOptions {
    #[must_use]
    pub const fn require_conflict_check(mut self, enabled: bool) -> Self {
        self.require_conflict_check = enabled;
        self
    }

    #[must_use]
    pub const fn with_durability(mut self, durability: CommitDurability) -> Self {
        self.durability = durability;
        self
    }

    #[must_use]
    pub const fn with_expected_generation(mut self, generation: BranchGeneration) -> Self {
        self.expected_generation = Some(generation);
        self
    }

    #[must_use]
    pub const fn conflict_check_required(self) -> bool {
        self.require_conflict_check
    }

    #[must_use]
    pub const fn durability(self) -> CommitDurability {
        self.durability
    }

    #[must_use]
    pub const fn expected_generation(self) -> Option<BranchGeneration> {
        self.expected_generation
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitBatch {
    branch_id: BranchId,
    mutations: Vec<CommitMutation>,
    conditions: Vec<CommitCondition>,
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
            if matches!(
                mutation,
                CommitMutation::Put {
                    ttl: Some(ttl), ..
                } if ttl.is_zero()
            ) {
                return Err(StorageApiError::InvalidArgument {
                    field: "ttl",
                    reason: "ttl duration must be greater than zero",
                });
            }
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
            conditions: Vec::new(),
            options,
        })
    }

    /// Replaces the condition set for this batch.
    pub fn with_conditions(mut self, conditions: Vec<CommitCondition>) -> StorageApiResult<Self> {
        validate_unique_conditions(&conditions)?;
        self.conditions = conditions;
        Ok(self)
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
    pub fn conditions(&self) -> &[CommitCondition] {
        &self.conditions
    }

    #[must_use]
    pub const fn options(&self) -> CommitOptions {
        self.options
    }
}

fn validate_unique_conditions(conditions: &[CommitCondition]) -> StorageApiResult<()> {
    let mut seen = BTreeSet::new();
    for condition in conditions {
        let identity = (condition.storage_space(), condition.key());
        if !seen.insert(identity) {
            return Err(StorageApiError::InvalidArgument {
                field: "conditions",
                reason: "commit conditions must not contain duplicate keys",
            });
        }
        if matches!(
            condition.expected(),
            CommitExpectedVersion::Present(version) if version == CommitVersion::ZERO
        ) {
            return Err(StorageApiError::InvalidArgument {
                field: "conditions",
                reason: "expected present version must be nonzero",
            });
        }
    }
    Ok(())
}
