//! Serializable request and response helper types.

use serde::{Deserialize, Serialize};

/// Default product branch used when a command omits its branch.
pub const DEFAULT_BRANCH: &str = "default";

/// Default product space used when a command omits its space.
pub const DEFAULT_SPACE: &str = "default";

/// Byte-preserving command payload.
#[derive(Clone, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct Bytes(#[serde(with = "serde_bytes")] Vec<u8>);

impl Bytes {
    /// Creates a byte payload.
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self(bytes.into())
    }

    /// Returns the raw bytes.
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    /// Consumes the wrapper and returns raw bytes.
    pub fn into_vec(self) -> Vec<u8> {
        self.0
    }

    /// Returns true when this payload has no bytes.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<Vec<u8>> for Bytes {
    fn from(value: Vec<u8>) -> Self {
        Self::new(value)
    }
}

impl From<&[u8]> for Bytes {
    fn from(value: &[u8]) -> Self {
        Self::new(value)
    }
}

impl<const N: usize> From<&[u8; N]> for Bytes {
    fn from(value: &[u8; N]) -> Self {
        Self::new(value.as_slice())
    }
}

impl From<&str> for Bytes {
    fn from(value: &str) -> Self {
        Self::new(value.as_bytes())
    }
}

/// Entry for a batch KV write.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BatchKvEntry {
    key: Bytes,
    value: Bytes,
}

impl BatchKvEntry {
    /// Creates a batch write entry.
    pub fn new(key: Bytes, value: Bytes) -> Self {
        Self { key, value }
    }

    /// Returns the entry key.
    pub const fn key(&self) -> &Bytes {
        &self.key
    }

    /// Returns the entry value.
    pub const fn value(&self) -> &Bytes {
        &self.value
    }

    /// Consumes the entry.
    pub fn into_parts(self) -> (Bytes, Bytes) {
        (self.key, self.value)
    }
}

/// Stored value with commit metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VersionedValue {
    value: Bytes,
    version: u64,
    timestamp: u64,
}

impl VersionedValue {
    /// Creates a versioned value.
    pub fn new(value: Bytes, version: u64, timestamp: u64) -> Self {
        Self {
            value,
            version,
            timestamp,
        }
    }

    /// Returns the stored value.
    pub const fn value(&self) -> &Bytes {
        &self.value
    }

    /// Returns the commit version.
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Returns the commit timestamp.
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }
}

/// Positional batch write result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BatchItemResult {
    key: Bytes,
    applied: bool,
    version: Option<u64>,
    timestamp: Option<u64>,
    error: Option<String>,
}

impl BatchItemResult {
    /// Creates a batch item result.
    pub fn new(key: Bytes, applied: bool, version: Option<u64>, timestamp: Option<u64>) -> Self {
        Self {
            key,
            applied,
            version,
            timestamp,
            error: None,
        }
    }

    /// Creates a failed batch item result.
    pub fn failed(key: Bytes, error: impl Into<String>) -> Self {
        Self {
            key,
            applied: false,
            version: None,
            timestamp: None,
            error: Some(error.into()),
        }
    }

    /// Returns the input key.
    pub const fn key(&self) -> &Bytes {
        &self.key
    }

    /// Returns true when this item was applied.
    pub const fn applied(&self) -> bool {
        self.applied
    }

    /// Returns the commit version, when an item was applied.
    pub const fn version(&self) -> Option<u64> {
        self.version
    }

    /// Returns the commit timestamp, when an item was applied.
    pub const fn timestamp(&self) -> Option<u64> {
        self.timestamp
    }

    /// Returns the item error, when this item failed validation.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

/// Positional batch read result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BatchGetItemResult {
    key: Bytes,
    value: Option<Bytes>,
    version: Option<u64>,
    timestamp: Option<u64>,
    error: Option<String>,
}

impl BatchGetItemResult {
    /// Creates a batch read result.
    pub fn new(
        key: Bytes,
        value: Option<Bytes>,
        version: Option<u64>,
        timestamp: Option<u64>,
    ) -> Self {
        Self {
            key,
            value,
            version,
            timestamp,
            error: None,
        }
    }

    /// Creates a failed batch read result.
    pub fn failed(key: Bytes, error: impl Into<String>) -> Self {
        Self {
            key,
            value: None,
            version: None,
            timestamp: None,
            error: Some(error.into()),
        }
    }

    /// Returns the input key.
    pub const fn key(&self) -> &Bytes {
        &self.key
    }

    /// Returns the stored value, when present.
    pub const fn value(&self) -> Option<&Bytes> {
        self.value.as_ref()
    }

    /// Returns the commit version, when present.
    pub const fn version(&self) -> Option<u64> {
        self.version
    }

    /// Returns the commit timestamp, when present.
    pub const fn timestamp(&self) -> Option<u64> {
        self.timestamp
    }

    /// Returns the item error, when this item failed validation.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

/// KV scan item.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ScanItem {
    key: Bytes,
    value: Bytes,
    version: u64,
    timestamp: u64,
}

impl ScanItem {
    /// Creates a scan item.
    pub fn new(key: Bytes, value: Bytes, version: u64, timestamp: u64) -> Self {
        Self {
            key,
            value,
            version,
            timestamp,
        }
    }

    /// Returns the item key.
    pub const fn key(&self) -> &Bytes {
        &self.key
    }

    /// Returns the item value.
    pub const fn value(&self) -> &Bytes {
        &self.value
    }

    /// Returns the commit version.
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Returns the commit timestamp.
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }
}

/// Version-history item.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HistoryItem {
    value: Option<Bytes>,
    tombstone: bool,
    version: u64,
    timestamp: u64,
}

impl HistoryItem {
    /// Creates a history item.
    pub fn new(value: Option<Bytes>, tombstone: bool, version: u64, timestamp: u64) -> Self {
        Self {
            value,
            tombstone,
            version,
            timestamp,
        }
    }

    /// Returns the item value, when this is not a tombstone.
    pub const fn value(&self) -> Option<&Bytes> {
        self.value.as_ref()
    }

    /// Returns true when this item represents a delete.
    pub const fn is_tombstone(&self) -> bool {
        self.tombstone
    }

    /// Returns the commit version.
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Returns the commit timestamp.
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }
}

/// Sampled KV item.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SampleItem {
    key: Bytes,
    value: Bytes,
    version: u64,
    timestamp: u64,
}

impl SampleItem {
    /// Creates a sample item.
    pub fn new(key: Bytes, value: Bytes, version: u64, timestamp: u64) -> Self {
        Self {
            key,
            value,
            version,
            timestamp,
        }
    }

    /// Returns the item key.
    pub const fn key(&self) -> &Bytes {
        &self.key
    }

    /// Returns the item value.
    pub const fn value(&self) -> &Bytes {
        &self.value
    }

    /// Returns the commit version.
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Returns the commit timestamp.
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }
}
