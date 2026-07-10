//! API storage atoms.

use std::ops::RangeBounds;

use super::{StorageApiError, StorageApiResult};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(transparent)]
pub struct BranchGeneration(u64);

impl BranchGeneration {
    pub const ZERO: Self = Self(0);

    #[must_use]
    pub const fn new(raw: u64) -> Self {
        Self(raw)
    }

    #[must_use]
    pub const fn as_u64(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(transparent)]
pub struct StorageSpaceId(Vec<u8>);

impl StorageSpaceId {
    pub fn new(bytes: impl Into<Vec<u8>>) -> StorageApiResult<Self> {
        let bytes = bytes.into();
        validate_component("storage_space", &bytes)?;
        Ok(Self(bytes))
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
#[repr(transparent)]
pub struct StorageKey(Vec<u8>);

impl StorageKey {
    pub fn new(bytes: impl Into<Vec<u8>>) -> StorageApiResult<Self> {
        let bytes = bytes.into();
        validate_component("key", &bytes)?;
        Ok(Self(bytes))
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// B4: consume the key without copying (rule-32 note: additive public
    /// item; the engine adapter moves point-read keys across the boundary).
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct StorageValue(Vec<u8>);

impl StorageValue {
    #[must_use]
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self(bytes.into())
    }

    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// B4: consume the value without copying (rule-32 note: additive public
    /// item; the engine adapter moves values across the crate boundary).
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct ReadLimit(usize);

impl ReadLimit {
    pub fn new(limit: usize) -> StorageApiResult<Self> {
        if limit == 0 {
            return Err(StorageApiError::InvalidArgument {
                field: "limit",
                reason: "limit must be greater than zero",
            });
        }
        Ok(Self(limit))
    }

    #[must_use]
    pub const fn get(self) -> usize {
        self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScanRange {
    start: Option<StorageKey>,
    end: Option<StorageKey>,
}

impl ScanRange {
    pub fn new(start: Option<StorageKey>, end: Option<StorageKey>) -> StorageApiResult<Self> {
        if let (Some(start), Some(end)) = (&start, &end) {
            if start >= end {
                return Err(StorageApiError::InvalidArgument {
                    field: "range",
                    reason: "range start must be less than range end",
                });
            }
        }
        Ok(Self { start, end })
    }

    #[must_use]
    pub const fn start(&self) -> Option<&StorageKey> {
        self.start.as_ref()
    }

    #[must_use]
    pub const fn end(&self) -> Option<&StorageKey> {
        self.end.as_ref()
    }
}

impl RangeBounds<StorageKey> for ScanRange {
    fn start_bound(&self) -> std::ops::Bound<&StorageKey> {
        match &self.start {
            Some(key) => std::ops::Bound::Included(key),
            None => std::ops::Bound::Unbounded,
        }
    }

    fn end_bound(&self) -> std::ops::Bound<&StorageKey> {
        match &self.end {
            Some(key) => std::ops::Bound::Excluded(key),
            None => std::ops::Bound::Unbounded,
        }
    }
}

fn validate_component(field: &'static str, bytes: &[u8]) -> StorageApiResult<()> {
    if bytes.is_empty() {
        Err(StorageApiError::InvalidArgument {
            field,
            reason: "component must not be empty",
        })
    } else {
        Ok(())
    }
}
