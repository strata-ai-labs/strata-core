//! Serializable request and response helper types.

use serde::{Deserialize, Serialize};
use serde_json::Value;

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

/// Entry for a batch JSON set.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BatchJsonEntry {
    key: String,
    path: String,
    value: Value,
}

impl BatchJsonEntry {
    /// Creates a batch JSON set entry.
    pub fn new(key: impl Into<String>, path: impl Into<String>, value: Value) -> Self {
        Self {
            key: key.into(),
            path: path.into(),
            value,
        }
    }

    /// Returns the document key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the JSON path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the JSON value.
    pub const fn value(&self) -> &Value {
        &self.value
    }

    /// Consumes the entry.
    pub fn into_parts(self) -> (String, String, Value) {
        (self.key, self.path, self.value)
    }
}

/// Entry for a batch JSON get.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BatchJsonGetEntry {
    key: String,
    path: String,
}

impl BatchJsonGetEntry {
    /// Creates a batch JSON get entry.
    pub fn new(key: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            path: path.into(),
        }
    }

    /// Returns the document key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the JSON path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Consumes the entry.
    pub fn into_parts(self) -> (String, String) {
        (self.key, self.path)
    }
}

/// Entry for a batch JSON delete.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BatchJsonDeleteEntry {
    key: String,
    path: String,
}

impl BatchJsonDeleteEntry {
    /// Creates a batch JSON delete entry.
    pub fn new(key: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            path: path.into(),
        }
    }

    /// Returns the document key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the JSON path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Consumes the entry.
    pub fn into_parts(self) -> (String, String) {
        (self.key, self.path)
    }
}

/// JSON secondary index kind exposed through the command boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JsonIndexType {
    /// Numeric field index.
    Numeric,
    /// Exact tag/string field index.
    Tag,
    /// Lowercase text field index.
    Text,
}

/// Stored JSON value with commit metadata.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonVersionedValue {
    value: Value,
    version: u64,
    timestamp: u64,
    document_version: u64,
}

impl JsonVersionedValue {
    /// Creates a JSON versioned value.
    pub fn new(value: Value, version: u64, timestamp: u64, document_version: u64) -> Self {
        Self {
            value,
            version,
            timestamp,
            document_version,
        }
    }

    /// Returns the selected JSON value.
    pub const fn value(&self) -> &Value {
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

    /// Returns the document version.
    pub const fn document_version(&self) -> u64 {
        self.document_version
    }
}

/// JSON version-history item.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonHistoryItem {
    value: Option<Value>,
    version: u64,
    timestamp: u64,
    document_version: Option<u64>,
    tombstone: bool,
}

impl JsonHistoryItem {
    /// Creates a JSON history item.
    pub fn new(
        value: Option<Value>,
        version: u64,
        timestamp: u64,
        document_version: Option<u64>,
        tombstone: bool,
    ) -> Self {
        Self {
            value,
            version,
            timestamp,
            document_version,
            tombstone,
        }
    }

    /// Returns the full document value when this row is not a tombstone.
    pub const fn value(&self) -> Option<&Value> {
        self.value.as_ref()
    }

    /// Returns the commit version.
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Returns the commit timestamp.
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }

    /// Returns the document version, when present.
    pub const fn document_version(&self) -> Option<u64> {
        self.document_version
    }

    /// Returns true when this item represents a delete.
    pub const fn is_tombstone(&self) -> bool {
        self.tombstone
    }
}

/// Positional JSON batch write/delete result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct JsonBatchItemResult {
    version: Option<u64>,
    timestamp: Option<u64>,
    document_version: Option<u64>,
    error: Option<String>,
}

impl JsonBatchItemResult {
    /// Creates a successful JSON batch result.
    pub const fn new(
        version: Option<u64>,
        timestamp: Option<u64>,
        document_version: Option<u64>,
    ) -> Self {
        Self {
            version,
            timestamp,
            document_version,
            error: None,
        }
    }

    /// Creates a failed JSON batch result.
    pub fn failed(error: impl Into<String>) -> Self {
        Self {
            version: None,
            timestamp: None,
            document_version: None,
            error: Some(error.into()),
        }
    }

    /// Returns the commit version, when present.
    pub const fn version(&self) -> Option<u64> {
        self.version
    }

    /// Returns the commit timestamp, when present.
    pub const fn timestamp(&self) -> Option<u64> {
        self.timestamp
    }

    /// Returns the document version, when present.
    pub const fn document_version(&self) -> Option<u64> {
        self.document_version
    }

    /// Returns the item error, when this item failed validation.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

/// Positional JSON batch read result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonBatchGetItemResult {
    value: Option<Value>,
    version: Option<u64>,
    timestamp: Option<u64>,
    document_version: Option<u64>,
    error: Option<String>,
}

impl JsonBatchGetItemResult {
    /// Creates a JSON batch read result.
    pub fn new(
        value: Option<Value>,
        version: Option<u64>,
        timestamp: Option<u64>,
        document_version: Option<u64>,
    ) -> Self {
        Self {
            value,
            version,
            timestamp,
            document_version,
            error: None,
        }
    }

    /// Creates a failed JSON batch read result.
    pub fn failed(error: impl Into<String>) -> Self {
        Self {
            value: None,
            version: None,
            timestamp: None,
            document_version: None,
            error: Some(error.into()),
        }
    }

    /// Returns the selected JSON value, when present.
    pub const fn value(&self) -> Option<&Value> {
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

    /// Returns the document version, when present.
    pub const fn document_version(&self) -> Option<u64> {
        self.document_version
    }

    /// Returns the item error, when this item failed validation.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

/// Sampled JSON document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonSampleItem {
    key: String,
    value: Value,
    version: u64,
    timestamp: u64,
    document_version: u64,
}

impl JsonSampleItem {
    /// Creates a sampled JSON document.
    pub fn new(
        key: String,
        value: Value,
        version: u64,
        timestamp: u64,
        document_version: u64,
    ) -> Self {
        Self {
            key,
            value,
            version,
            timestamp,
            document_version,
        }
    }

    /// Returns the document key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the full document value.
    pub const fn value(&self) -> &Value {
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

    /// Returns the document version.
    pub const fn document_version(&self) -> u64 {
        self.document_version
    }
}

/// JSON secondary index definition exposed through the command boundary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct JsonIndexDefinition {
    name: String,
    space: String,
    field_path: String,
    index_type: JsonIndexType,
    created_version: u64,
    created_timestamp: u64,
}

impl JsonIndexDefinition {
    /// Creates a JSON index definition.
    pub fn new(
        name: String,
        space: String,
        field_path: String,
        index_type: JsonIndexType,
        created_version: u64,
        created_timestamp: u64,
    ) -> Self {
        Self {
            name,
            space,
            field_path,
            index_type,
            created_version,
            created_timestamp,
        }
    }

    /// Returns the index name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the product space.
    pub fn space(&self) -> &str {
        &self.space
    }

    /// Returns the indexed field path.
    pub fn field_path(&self) -> &str {
        &self.field_path
    }

    /// Returns the index kind.
    pub const fn index_type(&self) -> JsonIndexType {
        self.index_type
    }

    /// Returns the creation commit version.
    pub const fn created_version(&self) -> u64 {
        self.created_version
    }

    /// Returns the creation commit timestamp.
    pub const fn created_timestamp(&self) -> u64 {
        self.created_timestamp
    }
}

/// Stored value with commit metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VersionedValue {
    value: Bytes,
    version: u64,
    timestamp: u64,
}

/// Branch status exposed through the command boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchStatus {
    /// Branch accepts reads and writes.
    Active,
    /// Branch was deleted and is hidden from normal listing.
    Deleted,
}

/// Fork parent facts exposed through the command boundary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BranchParentItem {
    name: String,
    branch_id: String,
    generation: u64,
    fork_version: u64,
    fork_timestamp: Option<u64>,
}

impl BranchParentItem {
    /// Creates branch parent facts.
    pub fn new(
        name: String,
        branch_id: String,
        generation: u64,
        fork_version: u64,
        fork_timestamp: Option<u64>,
    ) -> Self {
        Self {
            name,
            branch_id,
            generation,
            fork_version,
            fork_timestamp,
        }
    }

    /// Returns the parent branch name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the parent branch id.
    pub fn branch_id(&self) -> &str {
        &self.branch_id
    }

    /// Returns the parent branch generation at fork time.
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Returns the fork version.
    pub const fn fork_version(&self) -> u64 {
        self.fork_version
    }

    /// Returns the timestamp used to resolve the fork point.
    pub const fn fork_timestamp(&self) -> Option<u64> {
        self.fork_timestamp
    }
}

/// Branch summary exposed through the command boundary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BranchItem {
    name: String,
    branch_id: String,
    generation: u64,
    status: BranchStatus,
    parent: Option<BranchParentItem>,
    created_at: Option<u64>,
    deleted_at: Option<u64>,
    state_revision: u64,
}

impl BranchItem {
    /// Creates a branch item.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        branch_id: String,
        generation: u64,
        status: BranchStatus,
        parent: Option<BranchParentItem>,
        created_at: Option<u64>,
        deleted_at: Option<u64>,
        state_revision: u64,
    ) -> Self {
        Self {
            name,
            branch_id,
            generation,
            status,
            parent,
            created_at,
            deleted_at,
            state_revision,
        }
    }

    /// Returns the branch name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the branch id.
    pub fn branch_id(&self) -> &str {
        &self.branch_id
    }

    /// Returns the branch generation.
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Returns the branch status.
    pub const fn status(&self) -> BranchStatus {
        self.status
    }

    /// Returns fork parent facts, when any.
    pub const fn parent(&self) -> Option<&BranchParentItem> {
        self.parent.as_ref()
    }

    /// Returns the storage creation version, when known.
    pub const fn created_at(&self) -> Option<u64> {
        self.created_at
    }

    /// Returns the storage deletion version, when known.
    pub const fn deleted_at(&self) -> Option<u64> {
        self.deleted_at
    }

    /// Returns the storage state revision.
    pub const fn state_revision(&self) -> u64 {
        self.state_revision
    }
}

/// Cleanup facts for branch deletion.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct BranchCleanupItem {
    removed_refs: u64,
    releasable_tables: u64,
    protected_tables: u64,
}

impl BranchCleanupItem {
    /// Creates branch cleanup facts.
    pub const fn new(removed_refs: u64, releasable_tables: u64, protected_tables: u64) -> Self {
        Self {
            removed_refs,
            releasable_tables,
            protected_tables,
        }
    }

    /// Returns the number of removed references.
    pub const fn removed_refs(self) -> u64 {
        self.removed_refs
    }

    /// Returns the number of releasable tables.
    pub const fn releasable_tables(self) -> u64 {
        self.releasable_tables
    }

    /// Returns the number of protected tables.
    pub const fn protected_tables(self) -> u64 {
        self.protected_tables
    }
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
