//! In-memory physical storage row model.

#![cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "row records are consumed across storage layers; feature matrices can make individual helpers temporarily unused"
    )
)]

use std::fmt;
use strata_core_next::{BranchId, CommitVersion, Timestamp};

const STORAGE_ENGINE_MIN_ID: u8 = 0x20;

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(transparent)]
pub(crate) struct StorageSpaceId(u8);

impl StorageSpaceId {
    pub(crate) const INVALID: Self = Self(0x00);
    pub(crate) const COMMIT_TIMELINE: Self = Self(0x01);

    pub(crate) const fn raw(self) -> u8 {
        self.0
    }

    pub(crate) fn from_raw(raw: u8) -> Result<Self, RowError> {
        if raw == Self::INVALID.raw() {
            return Err(RowError::InvalidStorageSpaceId { raw });
        }
        Ok(Self(raw))
    }

    pub(crate) fn engine(raw: u8) -> Result<Self, RowError> {
        let id = Self::from_raw(raw)?;
        // Engine-owned spaces start at 0x20; lower nonzero ids are reserved for
        // storage metadata such as commit timeline rows.
        if !id.is_engine_owned() {
            return Err(RowError::StorageReservedSpaceId { raw });
        }
        Ok(id)
    }

    pub(crate) const fn is_storage_owned(self) -> bool {
        self.0 < STORAGE_ENGINE_MIN_ID && self.0 != Self::INVALID.raw()
    }

    pub(crate) const fn is_engine_owned(self) -> bool {
        self.0 >= STORAGE_ENGINE_MIN_ID
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PhysicalKey {
    branch_id: BranchId,
    space: String,
    storage_space_id: StorageSpaceId,
    user_key: Vec<u8>,
}

impl PhysicalKey {
    pub(crate) fn new(
        branch_id: BranchId,
        space: impl Into<String>,
        storage_space_id: StorageSpaceId,
        user_key: impl Into<Vec<u8>>,
    ) -> Result<Self, RowError> {
        let space = space.into();
        validate_space(&space)?;
        Ok(Self {
            branch_id,
            space,
            storage_space_id,
            user_key: user_key.into(),
        })
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) fn space(&self) -> &str {
        &self.space
    }

    pub(crate) const fn storage_space_id(&self) -> StorageSpaceId {
        self.storage_space_id
    }

    pub(crate) fn user_key(&self) -> &[u8] {
        &self.user_key
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct InternalKey {
    physical_key: PhysicalKey,
    commit_version: CommitVersion,
}

impl InternalKey {
    pub(crate) const fn new(physical_key: PhysicalKey, commit_version: CommitVersion) -> Self {
        Self {
            physical_key,
            commit_version,
        }
    }

    pub(crate) const fn physical_key(&self) -> &PhysicalKey {
        &self.physical_key
    }

    pub(crate) const fn commit_version(&self) -> CommitVersion {
        self.commit_version
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StorageRow {
    physical_key: PhysicalKey,
    commit_version: CommitVersion,
    commit_timestamp: Timestamp,
    // V1 uses Timestamp::EPOCH as the no-expiry sentinel. An explicit
    // expire-at-epoch row is not representable in this row shape.
    expires_at: Timestamp,
    value: Vec<u8>,
    is_tombstone: bool,
}

impl StorageRow {
    pub(crate) fn put(
        physical_key: PhysicalKey,
        commit_version: CommitVersion,
        commit_timestamp: Timestamp,
        expires_at: Timestamp,
        value: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            physical_key,
            commit_version,
            commit_timestamp,
            expires_at,
            value: value.into(),
            is_tombstone: false,
        }
    }

    pub(crate) const fn tombstone(
        physical_key: PhysicalKey,
        commit_version: CommitVersion,
        commit_timestamp: Timestamp,
    ) -> Self {
        Self {
            physical_key,
            commit_version,
            commit_timestamp,
            expires_at: Timestamp::EPOCH,
            value: Vec::new(),
            is_tombstone: true,
        }
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

    pub(crate) const fn expires_at(&self) -> Timestamp {
        self.expires_at
    }

    /// B4: destructure the row for the api's move-based point-read exit —
    /// (storage space id, moved user-key bytes, moved value bytes, version,
    /// timestamp, `expires_at`, tombstone). The physical key's branch/space
    /// context is dropped: the point-read caller supplied it.
    pub(crate) fn into_read_parts(
        self,
    ) -> (
        StorageSpaceId,
        Vec<u8>,
        Vec<u8>,
        CommitVersion,
        Timestamp,
        Timestamp,
        bool,
    ) {
        (
            self.physical_key.storage_space_id,
            self.physical_key.user_key,
            self.value,
            self.commit_version,
            self.commit_timestamp,
            self.expires_at,
            self.is_tombstone,
        )
    }

    pub(crate) fn value(&self) -> &[u8] {
        &self.value
    }

    pub(crate) const fn is_tombstone(&self) -> bool {
        self.is_tombstone
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RowError {
    InvalidStorageSpaceId { raw: u8 },
    StorageReservedSpaceId { raw: u8 },
    EmptySpace,
    SpaceContainsNul,
}

impl fmt::Display for RowError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidStorageSpaceId { raw } => {
                write!(formatter, "storage space id 0x{raw:02x} is invalid")
            }
            Self::StorageReservedSpaceId { raw } => {
                write!(
                    formatter,
                    "storage space id 0x{raw:02x} is reserved for storage"
                )
            }
            Self::EmptySpace => formatter.write_str("physical key space is empty"),
            Self::SpaceContainsNul => formatter.write_str("physical key space contains NUL"),
        }
    }
}

impl std::error::Error for RowError {}

fn validate_space(space: &str) -> Result<(), RowError> {
    if space.is_empty() {
        return Err(RowError::EmptySpace);
    }
    if space.as_bytes().contains(&0x00) {
        return Err(RowError::SpaceContainsNul);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
