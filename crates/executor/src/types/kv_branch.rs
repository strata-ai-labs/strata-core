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

/// The conflict-resolution strategy for a promotion, exposed through the command
/// boundary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum PromotionStrategy {
    /// Refuse the promotion when any conflict exists.
    #[default]
    Strict,
    /// Apply the source side's value or tombstone for each conflict.
    SourceWins,
}

/// How two branches diverged on one entity since their branch point.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ConflictKind {
    /// Both sides changed the entity to different present values.
    ValueDivergence,
    /// One side changed the value while the other deleted the entity.
    ModifyDeleteDivergence,
}

/// What the selected strategy did with a conflict.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ConflictStrategyResult {
    /// The conflict blocked the promotion (`strict`).
    Refused,
    /// The source value or tombstone overwrote the target (`source_wins`).
    SourceWins,
}

/// One entity a promotion applied to the target branch, exposed through the
/// command boundary. `value` is absent for a propagated deletion.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct PromotedEntityItem {
    capability: ComparedCapability,
    space: String,
    identity: Bytes,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    value: Option<Bytes>,
}

impl PromotedEntityItem {
    /// Creates a promoted entity item.
    pub const fn new(
        capability: ComparedCapability,
        space: String,
        identity: Bytes,
        value: Option<Bytes>,
    ) -> Self {
        Self {
            capability,
            space,
            identity,
            value,
        }
    }

    /// Returns the capability the promoted entity belongs to.
    pub const fn capability(&self) -> ComparedCapability {
        self.capability
    }

    /// Returns the space the promoted entity belongs to.
    pub fn space(&self) -> &str {
        &self.space
    }

    /// Returns the entity's space-relative logical key.
    pub const fn identity(&self) -> &Bytes {
        &self.identity
    }

    /// Returns the value written to the target, or `None` for a deletion.
    pub const fn value(&self) -> Option<&Bytes> {
        self.value.as_ref()
    }
}

/// One conflicting entity a promotion encountered, exposed through the command
/// boundary. `source_value`/`target_value` are absent for a deletion.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct PreviewConflictItem {
    capability: ComparedCapability,
    space: String,
    identity: Bytes,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    source_value: Option<Bytes>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    target_value: Option<Bytes>,
    kind: ConflictKind,
    strategy_result: ConflictStrategyResult,
}

impl PreviewConflictItem {
    /// Creates a preview conflict item.
    pub const fn new(
        capability: ComparedCapability,
        space: String,
        identity: Bytes,
        source_value: Option<Bytes>,
        target_value: Option<Bytes>,
        kind: ConflictKind,
        strategy_result: ConflictStrategyResult,
    ) -> Self {
        Self {
            capability,
            space,
            identity,
            source_value,
            target_value,
            kind,
            strategy_result,
        }
    }

    /// Returns the capability the conflicting entity belongs to.
    pub const fn capability(&self) -> ComparedCapability {
        self.capability
    }

    /// Returns the space the conflicting entity belongs to.
    pub fn space(&self) -> &str {
        &self.space
    }

    /// Returns the entity's space-relative logical key.
    pub const fn identity(&self) -> &Bytes {
        &self.identity
    }

    /// Returns the source side's value, or `None` if deleted.
    pub const fn source_value(&self) -> Option<&Bytes> {
        self.source_value.as_ref()
    }

    /// Returns the target side's value, or `None` if deleted.
    pub const fn target_value(&self) -> Option<&Bytes> {
        self.target_value.as_ref()
    }

    /// Returns how the two sides diverged.
    pub const fn kind(&self) -> ConflictKind {
        self.kind
    }

    /// Returns what the strategy did with this conflict.
    pub const fn strategy_result(&self) -> ConflictStrategyResult {
        self.strategy_result
    }
}

/// The result of promoting one branch into another, exposed through the command
/// boundary. `target_version` is absent when the promotion applied nothing.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
#[serde(deny_unknown_fields)]
pub struct PromotionOutcomeItem {
    source: String,
    target: String,
    branch_point: u64,
    strategy: PromotionStrategy,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    target_version: Option<u64>,
    applied: Vec<PromotedEntityItem>,
    deleted: Vec<PromotedEntityItem>,
    conflicts: Vec<PreviewConflictItem>,
}

impl PromotionOutcomeItem {
    /// Creates a promotion outcome item.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        source: String,
        target: String,
        branch_point: u64,
        strategy: PromotionStrategy,
        target_version: Option<u64>,
        applied: Vec<PromotedEntityItem>,
        deleted: Vec<PromotedEntityItem>,
        conflicts: Vec<PreviewConflictItem>,
    ) -> Self {
        Self {
            source,
            target,
            branch_point,
            strategy,
            target_version,
            applied,
            deleted,
            conflicts,
        }
    }

    /// Returns the branch whose changes were promoted.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns the branch that received the promotion.
    pub fn target(&self) -> &str {
        &self.target
    }

    /// Returns the derived branch point the promotion merged against.
    pub const fn branch_point(&self) -> u64 {
        self.branch_point
    }

    /// Returns the strategy the promotion was applied under.
    pub const fn strategy(&self) -> PromotionStrategy {
        self.strategy
    }

    /// Returns the target commit version written, or `None` for a no-op.
    pub const fn target_version(&self) -> Option<u64> {
        self.target_version
    }

    /// Returns the source entities written onto the target.
    pub fn applied(&self) -> &[PromotedEntityItem] {
        &self.applied
    }

    /// Returns the target entities deleted by propagated source deletions.
    pub fn deleted(&self) -> &[PromotedEntityItem] {
        &self.deleted
    }

    /// Returns the entities that diverged on both sides.
    pub fn conflicts(&self) -> &[PreviewConflictItem] {
        &self.conflicts
    }
}

#[cfg(test)]
mod branch_comparison_tests {
    use super::{
        BranchComparisonItem, Bytes, ComparedCapability, ComparedEntityItem, ConflictKind,
        ConflictStrategyResult, PreviewConflictItem, PromotedEntityItem, PromotionOutcomeItem,
        PromotionStrategy, SpaceComparisonItem,
    };

    #[test]
    fn branch_comparison_item_exposes_every_part() {
        let entity = ComparedEntityItem::new(Bytes::from(&b"alpha"[..]), 7);
        assert_eq!(entity.identity(), &Bytes::from(&b"alpha"[..]));
        assert_eq!(entity.version(), 7);

        let space = SpaceComparisonItem::new(
            "default".to_owned(),
            ComparedCapability::Json,
            vec![ComparedEntityItem::new(Bytes::from(&b"add"[..]), 1)],
            vec![ComparedEntityItem::new(Bytes::from(&b"rem"[..]), 2)],
            vec![ComparedEntityItem::new(Bytes::from(&b"mod"[..]), 3)],
        );
        assert_eq!(space.space(), "default");
        assert_eq!(space.capability(), ComparedCapability::Json);
        assert_eq!(space.added().len(), 1);
        assert_eq!(space.added()[0].identity(), &Bytes::from(&b"add"[..]));
        assert_eq!(space.removed()[0].identity(), &Bytes::from(&b"rem"[..]));
        assert_eq!(space.modified()[0].identity(), &Bytes::from(&b"mod"[..]));

        let comparison =
            BranchComparisonItem::new("default".to_owned(), "feature".to_owned(), vec![space]);
        assert_eq!(comparison.branch_a(), "default");
        assert_eq!(comparison.branch_b(), "feature");
        assert_eq!(comparison.spaces().len(), 1);
        assert_eq!(comparison.spaces()[0].space(), "default");
    }

    #[test]
    fn promotion_outcome_item_exposes_every_part() {
        let applied = PromotedEntityItem::new(
            ComparedCapability::KeyValue,
            "default".to_owned(),
            Bytes::from(&b"shared"[..]),
            Some(Bytes::from(&b"src"[..])),
        );
        assert_eq!(applied.capability(), ComparedCapability::KeyValue);
        assert_eq!(applied.space(), "default");
        assert_eq!(applied.identity(), &Bytes::from(&b"shared"[..]));
        assert_eq!(applied.value(), Some(&Bytes::from(&b"src"[..])));

        let deleted = PromotedEntityItem::new(
            ComparedCapability::Json,
            "docs".to_owned(),
            Bytes::from(&b"md"[..]),
            None,
        );
        assert_eq!(deleted.capability(), ComparedCapability::Json);
        assert_eq!(deleted.value(), None);

        let conflict = PreviewConflictItem::new(
            ComparedCapability::KeyValue,
            "default".to_owned(),
            Bytes::from(&b"shared"[..]),
            Some(Bytes::from(&b"src"[..])),
            Some(Bytes::from(&b"tgt"[..])),
            ConflictKind::ValueDivergence,
            ConflictStrategyResult::SourceWins,
        );
        assert_eq!(conflict.capability(), ComparedCapability::KeyValue);
        assert_eq!(conflict.space(), "default");
        assert_eq!(conflict.identity(), &Bytes::from(&b"shared"[..]));
        assert_eq!(conflict.source_value(), Some(&Bytes::from(&b"src"[..])));
        assert_eq!(conflict.target_value(), Some(&Bytes::from(&b"tgt"[..])));
        assert_eq!(conflict.kind(), ConflictKind::ValueDivergence);
        assert_eq!(
            conflict.strategy_result(),
            ConflictStrategyResult::SourceWins
        );

        let outcome = PromotionOutcomeItem::new(
            "feature".to_owned(),
            "default".to_owned(),
            3,
            PromotionStrategy::SourceWins,
            Some(9),
            vec![applied],
            vec![deleted],
            vec![conflict],
        );
        assert_eq!(outcome.source(), "feature");
        assert_eq!(outcome.target(), "default");
        assert_eq!(outcome.branch_point(), 3);
        assert_eq!(outcome.strategy(), PromotionStrategy::SourceWins);
        assert_eq!(outcome.target_version(), Some(9));
        assert_eq!(outcome.applied().len(), 1);
        assert_eq!(
            outcome.applied()[0].identity(),
            &Bytes::from(&b"shared"[..])
        );
        assert_eq!(outcome.deleted().len(), 1);
        assert_eq!(outcome.deleted()[0].identity(), &Bytes::from(&b"md"[..]));
        assert_eq!(outcome.conflicts().len(), 1);
        assert_eq!(outcome.conflicts()[0].kind(), ConflictKind::ValueDivergence);
    }
}
