use super::{Bytes, Deserialize, Serialize};

/// Stored value with commit metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
pub struct VersionedValue {
    value: Bytes,
    version: u64,
    timestamp: u64,
}

/// Branch status exposed through the command boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum BranchStatus {
    /// Branch accepts reads and writes.
    Active,
    /// Branch was deleted and is hidden from normal listing.
    Deleted,
}

/// Fork parent facts exposed through the command boundary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
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

/// Positional batch write result payload.
///
/// The shared [`BatchItem`](crate::BatchItem) wrapper owns the status, mutation
/// effect, commit receipt, and error; this payload carries only the KV-specific
/// echoed key.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
pub struct BatchItemResult {
    key: Bytes,
}

impl BatchItemResult {
    /// Creates a batch item result payload.
    pub const fn new(key: Bytes) -> Self {
        Self { key }
    }

    /// Returns the input key.
    pub const fn key(&self) -> &Bytes {
        &self.key
    }
}

/// Positional batch read result payload.
///
/// The shared [`BatchItem`](crate::BatchItem) wrapper owns the status and error;
/// this payload carries the echoed key and the read facts.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
pub struct BatchGetItemResult {
    key: Bytes,
    found: bool,
    value: Option<Bytes>,
    version: Option<u64>,
    timestamp: Option<u64>,
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
            found: value.is_some(),
            value,
            version,
            timestamp,
        }
    }

    /// Creates a batch read payload for an item that failed validation.
    ///
    /// The failure carries no read facts; the [`BatchItem`](crate::BatchItem)
    /// wrapper carries the error.
    pub const fn not_found(key: Bytes) -> Self {
        Self {
            key,
            found: false,
            value: None,
            version: None,
            timestamp: None,
        }
    }

    /// Returns the input key.
    pub const fn key(&self) -> &Bytes {
        &self.key
    }

    /// Returns true when the key exists.
    pub const fn found(&self) -> bool {
        self.found
    }

    /// Returns the stored value, when present.
    pub const fn value(&self) -> Option<&Bytes> {
        if self.found {
            self.value.as_ref()
        } else {
            None
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
}

/// Positional batch existence result payload.
///
/// The shared [`BatchItem`](crate::BatchItem) wrapper owns the status and error;
/// this payload carries the echoed key and the definitive existence answer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
pub struct BatchExistsItemResult {
    key: Bytes,
    exists: bool,
}

impl BatchExistsItemResult {
    /// Creates a batch existence result. `exists` is a definitive answer,
    /// so both true and false are `ok` items (never a miss).
    pub const fn new(key: Bytes, exists: bool) -> Self {
        Self { key, exists }
    }

    /// Returns the input key.
    pub const fn key(&self) -> &Bytes {
        &self.key
    }

    /// Returns whether the key exists.
    pub const fn exists(&self) -> bool {
        self.exists
    }
}

/// KV scan item.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
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

/// Version-history result for one key.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
pub struct HistoryResult {
    items: Vec<HistoryItem>,
}

impl HistoryResult {
    /// Creates a version-history result.
    pub const fn new(items: Vec<HistoryItem>) -> Self {
        Self { items }
    }

    /// Returns the number of history items.
    pub const fn count(&self) -> usize {
        self.items.len()
    }

    /// Returns version-history items from newest to oldest.
    pub const fn items(&self) -> &[HistoryItem] {
        self.items.as_slice()
    }

    /// Consumes the result and returns its items.
    pub fn into_items(self) -> Vec<HistoryItem> {
        self.items
    }
}

/// Sampled KV item.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
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

/// The data capability a branch comparison entry belongs to.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ComparedCapability {
    /// The key-value capability.
    KeyValue,
    /// The JSON document capability.
    Json,
}

/// One entity that differs between two branches, exposed through the command
/// boundary. `identity` is the capability's space-relative logical key.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct ComparedEntityItem {
    identity: Bytes,
    version: u64,
}

impl ComparedEntityItem {
    /// Creates a compared entity item.
    pub const fn new(identity: Bytes, version: u64) -> Self {
        Self { identity, version }
    }

    /// Returns the entity's space-relative logical key.
    pub const fn identity(&self) -> &Bytes {
        &self.identity
    }

    /// Returns the commit version observed on the reported side.
    pub const fn version(&self) -> u64 {
        self.version
    }
}

/// The differing entities for one capability within one space. `added` are
/// present on branch B but not A, `removed` on A but not B, `modified` on both
/// with differing values.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct SpaceComparisonItem {
    space: String,
    capability: ComparedCapability,
    added: Vec<ComparedEntityItem>,
    removed: Vec<ComparedEntityItem>,
    modified: Vec<ComparedEntityItem>,
}

impl SpaceComparisonItem {
    /// Creates a space comparison item.
    pub fn new(
        space: String,
        capability: ComparedCapability,
        added: Vec<ComparedEntityItem>,
        removed: Vec<ComparedEntityItem>,
        modified: Vec<ComparedEntityItem>,
    ) -> Self {
        Self {
            space,
            capability,
            added,
            removed,
            modified,
        }
    }

    /// Returns the space this comparison covers.
    pub fn space(&self) -> &str {
        &self.space
    }

    /// Returns the capability this comparison covers.
    pub const fn capability(&self) -> ComparedCapability {
        self.capability
    }

    /// Entities present on branch B but not branch A.
    pub fn added(&self) -> &[ComparedEntityItem] {
        &self.added
    }

    /// Entities present on branch A but not branch B.
    pub fn removed(&self) -> &[ComparedEntityItem] {
        &self.removed
    }

    /// Entities present on both branches with differing values.
    pub fn modified(&self) -> &[ComparedEntityItem] {
        &self.modified
    }
}

/// The result of comparing two branches, exposed through the command boundary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct BranchComparisonItem {
    branch_a: String,
    branch_b: String,
    spaces: Vec<SpaceComparisonItem>,
}

impl BranchComparisonItem {
    /// Creates a branch comparison item.
    pub fn new(branch_a: String, branch_b: String, spaces: Vec<SpaceComparisonItem>) -> Self {
        Self {
            branch_a,
            branch_b,
            spaces,
        }
    }

    /// Returns the first branch of the comparison (the `A` side).
    pub fn branch_a(&self) -> &str {
        &self.branch_a
    }

    /// Returns the second branch of the comparison (the `B` side).
    pub fn branch_b(&self) -> &str {
        &self.branch_b
    }

    /// Returns the per-capability, per-space comparisons.
    pub fn spaces(&self) -> &[SpaceComparisonItem] {
        &self.spaces
    }
}
