//! Branch read-bound, selected-row, and own-branch read-view vocabulary.

use super::error::{BranchRuntimeError, BranchRuntimeResult, BranchTimestampHistorySource};
use super::facts::{
    BranchLevel, BranchStateFacts, BranchTableDescriptor, InheritedLayerDescriptor,
    InheritedLayerStatus,
};
use super::identity::{rewrite_physical_key_branch, rewrite_row_branch};
use super::state::BranchLocalState;
use crate::observability::perf_trace;
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use crate::table::{
    BoundedTableCursor, FrozenTable, ImmutableTableReader, MutableTable, TableCursor,
    TableInternalKeyBytes, TableKeyBounds, TablePhysicalKeyBound, TablePhysicalKeyBytes, TableRow,
    TableRuntimeFacts,
};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BranchReadBound {
    Latest,
    AtVersion(CommitVersion),
    AtTimestamp(Timestamp),
}

impl BranchReadBound {
    pub(crate) const fn latest() -> Self {
        Self::Latest
    }

    pub(crate) const fn at_version(version: CommitVersion) -> Self {
        Self::AtVersion(version)
    }

    pub(crate) const fn at_timestamp(timestamp: Timestamp) -> Self {
        Self::AtTimestamp(timestamp)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchEffectiveReadBound {
    max_commit_version: Option<CommitVersion>,
    max_commit_timestamp: Option<Timestamp>,
}

impl BranchEffectiveReadBound {
    pub(crate) const fn new(
        max_commit_version: Option<CommitVersion>,
        max_commit_timestamp: Option<Timestamp>,
    ) -> Self {
        Self {
            max_commit_version,
            max_commit_timestamp,
        }
    }

    pub(crate) const fn for_own_branch(bound: BranchReadBound) -> Self {
        match bound {
            BranchReadBound::Latest => Self::new(None, None),
            BranchReadBound::AtVersion(version) => Self::new(Some(version), None),
            BranchReadBound::AtTimestamp(timestamp) => Self::new(None, Some(timestamp)),
        }
    }

    /// Compute the effective read bound for a forked child reading rows
    /// from a parent's inherited layer. The fork inheritance contract is:
    ///
    /// - `Latest` and `AtVersion(version)` reads cap the version at
    ///   `fork_version`. The child sees parent rows up to and including
    ///   the fork point; parent post-fork commits are invisible.
    /// - `AtTimestamp(timestamp)` reads apply BOTH the `fork_version` cap
    ///   AND the timestamp filter. Per-row `commit_timestamp` drives
    ///   timestamp matching against parent's physical rows in the
    ///   inherited layer; no timeline transcription happens at fork
    ///   time and no parent-timeline lookup happens at read time.
    /// - For `T < fork_timestamp`, reads resolve through the parent's
    ///   inherited rows.
    /// - For `T >= fork_timestamp`, the version cap still bounds the
    ///   view at `fork_version`; the child's own (post-fork) commits
    ///   are read via `for_own_branch`, not this helper.
    ///
    /// The catalog does not transcribe parent timeline rows under the
    /// child's `branch_id` at fork, and the read path does not consult
    /// the parent's timeline metadata. Three pinning tests in
    /// `lifecycle/tests/branch_lifecycle/fork.rs` verify the contract:
    /// `forked_branch_at_timestamp_before_fork_returns_parent_row`,
    /// `forked_branch_at_timestamp_after_fork_returns_child_row`, and
    /// `forked_branch_isolated_from_parent_post_fork_commits`.
    pub(crate) const fn for_inherited_layer(
        bound: BranchReadBound,
        fork_version: CommitVersion,
    ) -> Self {
        match bound {
            BranchReadBound::Latest => Self::new(Some(fork_version), None),
            BranchReadBound::AtVersion(version) => {
                let effective_version = if version.as_u64() <= fork_version.as_u64() {
                    version
                } else {
                    fork_version
                };
                Self::new(Some(effective_version), None)
            }
            BranchReadBound::AtTimestamp(timestamp) => {
                Self::new(Some(fork_version), Some(timestamp))
            }
        }
    }

    pub(crate) const fn max_commit_version(self) -> Option<CommitVersion> {
        self.max_commit_version
    }

    pub(crate) const fn max_commit_timestamp(self) -> Option<Timestamp> {
        self.max_commit_timestamp
    }

    pub(crate) const fn row_version_in_bound(self, row: &StorageRow) -> bool {
        match self.max_commit_version {
            Some(version) => row.commit_version().as_u64() <= version.as_u64(),
            None => true,
        }
    }

    pub(crate) const fn row_timestamp_in_bound(self, row: &StorageRow) -> bool {
        match self.max_commit_timestamp {
            Some(timestamp) => row.commit_timestamp().as_micros() <= timestamp.as_micros(),
            None => true,
        }
    }

    pub(crate) const fn matches_row(self, row: &StorageRow) -> bool {
        self.row_version_in_bound(row) && self.row_timestamp_in_bound(row)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BranchTimestampCoverage {
    Unknown,
    Complete,
    CompleteSince { earliest_timestamp: Timestamp },
}

impl BranchTimestampCoverage {
    pub(crate) const fn unknown() -> Self {
        Self::Unknown
    }

    pub(crate) const fn complete() -> Self {
        Self::Complete
    }

    pub(crate) const fn complete_since(earliest_timestamp: Timestamp) -> Self {
        Self::CompleteSince { earliest_timestamp }
    }

    fn require_timestamp(
        self,
        branch_id: BranchId,
        requested_timestamp: Timestamp,
        source: BranchTimestampHistorySource,
    ) -> BranchRuntimeResult<()> {
        if let Self::CompleteSince { earliest_timestamp } = self {
            if requested_timestamp < earliest_timestamp {
                Err(BranchRuntimeError::InsufficientTimestampHistory {
                    branch_id,
                    requested_timestamp,
                    earliest_available_timestamp: Some(earliest_timestamp),
                    source,
                })
            } else {
                Ok(())
            }
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BranchRowSource {
    Active,
    Frozen {
        index: usize,
    },
    OwnedTable {
        level: BranchLevel,
        table_index: usize,
    },
    Inherited {
        source_branch_id: BranchId,
        layer_index: usize,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchVisibleRow {
    row: StorageRow,
    source: BranchRowSource,
}

impl BranchVisibleRow {
    pub(crate) fn new(row: StorageRow, source: BranchRowSource) -> Self {
        Self { row, source }
    }

    pub(crate) const fn row(&self) -> &StorageRow {
        &self.row
    }

    pub(crate) const fn source(&self) -> BranchRowSource {
        self.source
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchHistoryRow {
    row: StorageRow,
    source: BranchRowSource,
}

impl BranchHistoryRow {
    pub(crate) fn new(row: StorageRow, source: BranchRowSource) -> Self {
        Self { row, source }
    }

    pub(crate) const fn row(&self) -> &StorageRow {
        &self.row
    }

    pub(crate) const fn source(&self) -> BranchRowSource {
        self.source
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum BranchUserKeyBound {
    Unbounded,
    Included(Vec<u8>),
    Excluded(Vec<u8>),
}

impl BranchUserKeyBound {
    pub(crate) fn included(user_key: impl Into<Vec<u8>>) -> Self {
        Self::Included(user_key.into())
    }

    pub(crate) fn excluded(user_key: impl Into<Vec<u8>>) -> Self {
        Self::Excluded(user_key.into())
    }

    fn finite(&self) -> Option<&[u8]> {
        match self {
            Self::Unbounded => None,
            Self::Included(user_key) | Self::Excluded(user_key) => Some(user_key),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchScanBounds {
    branch_id: BranchId,
    space: String,
    storage_space_id: StorageSpaceId,
    user_key_prefix: Option<Vec<u8>>,
    lower_user_key: BranchUserKeyBound,
    upper_user_key: BranchUserKeyBound,
}

impl BranchScanBounds {
    pub(crate) fn prefix(prefix: &PhysicalKey) -> Self {
        Self {
            branch_id: prefix.branch_id(),
            space: prefix.space().to_owned(),
            storage_space_id: prefix.storage_space_id(),
            user_key_prefix: Some(prefix.user_key().to_vec()),
            lower_user_key: BranchUserKeyBound::Unbounded,
            upper_user_key: BranchUserKeyBound::Unbounded,
        }
    }

    pub(crate) fn unbounded(
        branch_id: BranchId,
        space: impl Into<String>,
        storage_space_id: StorageSpaceId,
    ) -> BranchRuntimeResult<Self> {
        Self::range(
            branch_id,
            space,
            storage_space_id,
            BranchUserKeyBound::Unbounded,
            BranchUserKeyBound::Unbounded,
        )
    }

    pub(crate) fn range(
        branch_id: BranchId,
        space: impl Into<String>,
        storage_space_id: StorageSpaceId,
        lower_user_key: BranchUserKeyBound,
        upper_user_key: BranchUserKeyBound,
    ) -> BranchRuntimeResult<Self> {
        let space = space.into();
        validate_scan_space(branch_id, &space, storage_space_id)?;
        validate_user_key_bound_order(&lower_user_key, &upper_user_key)?;
        Ok(Self {
            branch_id,
            space,
            storage_space_id,
            user_key_prefix: None,
            lower_user_key,
            upper_user_key,
        })
    }

    pub(crate) fn closed(lower: &PhysicalKey, upper: &PhysicalKey) -> BranchRuntimeResult<Self> {
        Self::from_physical_bounds(
            lower,
            upper,
            BranchUserKeyBound::included(lower.user_key()),
            BranchUserKeyBound::included(upper.user_key()),
        )
    }

    pub(crate) fn open(lower: &PhysicalKey, upper: &PhysicalKey) -> BranchRuntimeResult<Self> {
        Self::from_physical_bounds(
            lower,
            upper,
            BranchUserKeyBound::excluded(lower.user_key()),
            BranchUserKeyBound::excluded(upper.user_key()),
        )
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    fn table_key_bounds(&self) -> BranchRuntimeResult<TableKeyBounds> {
        self.table_key_bounds_for_branch(self.branch_id)
    }

    fn table_key_bounds_for_branch(
        &self,
        branch_id: BranchId,
    ) -> BranchRuntimeResult<TableKeyBounds> {
        if let Some(prefix) = &self.user_key_prefix {
            let prefix_key = scan_physical_key(
                branch_id,
                &self.space,
                self.storage_space_id,
                prefix.clone(),
            )?;
            return Ok(TableKeyBounds::prefix(
                TablePhysicalKeyBytes::from_physical_key_prefix(&prefix_key)
                    .as_slice()
                    .to_vec(),
            ));
        }

        let namespace_key =
            scan_physical_key(branch_id, &self.space, self.storage_space_id, Vec::new())?;
        let namespace_prefix = TablePhysicalKeyBytes::from_physical_key_prefix(&namespace_key);
        let lower = table_physical_bound_for_user_key(
            branch_id,
            &self.space,
            self.storage_space_id,
            &self.lower_user_key,
        )?;
        let upper = table_physical_bound_for_user_key(
            branch_id,
            &self.space,
            self.storage_space_id,
            &self.upper_user_key,
        )?;
        TableKeyBounds::physical_range(&namespace_prefix, lower, upper).map_err(|_| {
            BranchRuntimeError::InvalidReadBound {
                reason: "scan table key bounds are invalid",
            }
        })
    }

    fn contains(&self, key: &PhysicalKey) -> bool {
        key.branch_id() == self.branch_id
            && key.space() == self.space
            && key.storage_space_id() == self.storage_space_id
            && self
                .user_key_prefix
                .as_ref()
                .is_none_or(|prefix| key.user_key().starts_with(prefix))
            && lower_contains(&self.lower_user_key, key.user_key())
            && upper_contains(&self.upper_user_key, key.user_key())
    }

    fn from_physical_bounds(
        lower: &PhysicalKey,
        upper: &PhysicalKey,
        lower_user_key: BranchUserKeyBound,
        upper_user_key: BranchUserKeyBound,
    ) -> BranchRuntimeResult<Self> {
        if lower.branch_id() != upper.branch_id()
            || lower.space() != upper.space()
            || lower.storage_space_id() != upper.storage_space_id()
        {
            return Err(BranchRuntimeError::InvalidReadBound {
                reason: "range bounds must target the same branch, space, and storage space",
            });
        }
        Self::range(
            lower.branch_id(),
            lower.space().to_owned(),
            lower.storage_space_id(),
            lower_user_key,
            upper_user_key,
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchHistoryOptions {
    before_version: Option<CommitVersion>,
    limit: Option<usize>,
    include_tombstones: bool,
}

impl BranchHistoryOptions {
    pub(crate) const fn all() -> Self {
        Self {
            before_version: None,
            limit: None,
            include_tombstones: true,
        }
    }

    pub(crate) const fn before_version(mut self, before_version: CommitVersion) -> Self {
        self.before_version = Some(before_version);
        self
    }

    pub(crate) const fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    pub(crate) const fn include_tombstones(mut self, include_tombstones: bool) -> Self {
        self.include_tombstones = include_tombstones;
        self
    }

    pub(crate) const fn before_version_bound(self) -> Option<CommitVersion> {
        self.before_version
    }

    pub(crate) const fn limit_bound(self) -> Option<usize> {
        self.limit
    }

    pub(crate) const fn includes_tombstones(self) -> bool {
        self.include_tombstones
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchOwnedTable {
    branch_id: BranchId,
    descriptor: BranchTableDescriptor,
    reader: ImmutableTableReader,
    materialization_source: Option<BranchMaterializationSource>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchMaterializationSource {
    source_branch_id: BranchId,
    fork_version: CommitVersion,
}

impl BranchMaterializationSource {
    pub(crate) const fn new(source_branch_id: BranchId, fork_version: CommitVersion) -> Self {
        Self {
            source_branch_id,
            fork_version,
        }
    }

    pub(crate) const fn source_branch_id(self) -> BranchId {
        self.source_branch_id
    }

    pub(crate) const fn fork_version(self) -> CommitVersion {
        self.fork_version
    }
}

impl BranchOwnedTable {
    pub(crate) fn new(
        branch_id: BranchId,
        descriptor: BranchTableDescriptor,
        reader: ImmutableTableReader,
    ) -> BranchRuntimeResult<Self> {
        Self::new_with_materialization_layer(branch_id, descriptor, reader, None)
    }

    pub(crate) fn new_materialization_replacement(
        branch_id: BranchId,
        descriptor: BranchTableDescriptor,
        reader: ImmutableTableReader,
        materialization_source: BranchMaterializationSource,
    ) -> BranchRuntimeResult<Self> {
        Self::new_with_materialization_layer(
            branch_id,
            descriptor,
            reader,
            Some(materialization_source),
        )
    }

    fn new_with_materialization_layer(
        branch_id: BranchId,
        descriptor: BranchTableDescriptor,
        reader: ImmutableTableReader,
        materialization_source: Option<BranchMaterializationSource>,
    ) -> BranchRuntimeResult<Self> {
        if descriptor.facts() != reader.facts() {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "branch-owned table descriptor facts must match reader facts",
            });
        }
        if reader.rows().is_empty() {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "branch-owned table must not be empty",
            });
        }
        if reader
            .rows()
            .iter()
            .any(|row| row.physical_key().branch_id() != branch_id)
        {
            return Err(BranchRuntimeError::InvalidBranchRow {
                reason: "branch-owned table rows must match the target branch",
            });
        }
        Ok(Self {
            branch_id,
            descriptor,
            reader,
            materialization_source,
        })
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn descriptor(&self) -> &BranchTableDescriptor {
        &self.descriptor
    }

    pub(crate) const fn facts(&self) -> &TableRuntimeFacts {
        self.descriptor.facts()
    }

    pub(crate) const fn level(&self) -> BranchLevel {
        self.descriptor.level()
    }

    pub(crate) const fn materialization_source(&self) -> Option<BranchMaterializationSource> {
        self.materialization_source
    }

    pub(crate) fn rows(&self) -> &[TableRow] {
        self.reader.rows()
    }

    pub(crate) fn reader(&self) -> &ImmutableTableReader {
        &self.reader
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchInheritedLayer {
    descriptor: InheritedLayerDescriptor,
    owned_levels: Vec<Vec<BranchOwnedTable>>,
}

impl BranchInheritedLayer {
    pub(crate) fn new(
        descriptor: InheritedLayerDescriptor,
        owned_levels: Vec<Vec<BranchOwnedTable>>,
    ) -> BranchRuntimeResult<Self> {
        if descriptor.table_count() != owned_table_count(&owned_levels) {
            return Err(BranchRuntimeError::InvalidInheritedLayer {
                reason: "inherited layer table count must match descriptor",
            });
        }
        validate_owned_levels(&owned_levels)?;
        validate_inherited_layer_unique_table_identities(&owned_levels)?;
        validate_inherited_layer_unique_keys(&owned_levels)?;
        for table in owned_levels.iter().flatten() {
            if table.branch_id() != descriptor.source_branch_id() {
                return Err(BranchRuntimeError::InvalidInheritedLayer {
                    reason: "inherited table branch id must match source branch",
                });
            }
            if table
                .rows()
                .iter()
                .any(|row| row.physical_key().branch_id() != descriptor.source_branch_id())
            {
                return Err(BranchRuntimeError::InvalidInheritedLayer {
                    reason: "inherited table rows must match source branch",
                });
            }
            if table
                .rows()
                .iter()
                .any(|row| row.commit_version().as_u64() > descriptor.fork_version().as_u64())
            {
                return Err(BranchRuntimeError::InvalidInheritedLayer {
                    reason: "inherited table rows must not be newer than the fork version",
                });
            }
        }
        Ok(Self {
            descriptor,
            owned_levels,
        })
    }

    #[cfg(any(test, feature = "testkit"))]
    pub(crate) fn new_unchecked_for_test(
        descriptor: InheritedLayerDescriptor,
        owned_levels: Vec<Vec<BranchOwnedTable>>,
    ) -> Self {
        Self {
            descriptor,
            owned_levels,
        }
    }

    pub(crate) const fn descriptor(&self) -> InheritedLayerDescriptor {
        self.descriptor
    }

    pub(crate) const fn source_branch_id(&self) -> BranchId {
        self.descriptor.source_branch_id()
    }

    pub(crate) const fn fork_version(&self) -> CommitVersion {
        self.descriptor.fork_version()
    }

    pub(crate) const fn status(&self) -> InheritedLayerStatus {
        self.descriptor.status()
    }

    pub(crate) fn owned_levels(&self) -> &[Vec<BranchOwnedTable>] {
        &self.owned_levels
    }

    pub(crate) fn with_status(&self, status: InheritedLayerStatus) -> BranchRuntimeResult<Self> {
        Self::new(
            InheritedLayerDescriptor::new(
                self.source_branch_id(),
                self.fork_version(),
                status,
                self.table_count(),
            ),
            self.owned_levels.clone(),
        )
    }

    pub(crate) fn table_count(&self) -> usize {
        owned_table_count(&self.owned_levels)
    }

    pub(crate) fn clone_active_for_fork(&self) -> BranchRuntimeResult<Option<Self>> {
        let status = self.status();
        match status {
            InheritedLayerStatus::Active | InheritedLayerStatus::Materializing => {
                Ok(Some(Self::new(
                    InheritedLayerDescriptor::new(
                        self.source_branch_id(),
                        self.fork_version(),
                        status,
                        self.table_count(),
                    ),
                    self.owned_levels.clone(),
                )?))
            }
            InheritedLayerStatus::Materialized => Ok(None),
            InheritedLayerStatus::Unavailable => Err(BranchRuntimeError::InvalidInheritedLayer {
                reason: "unavailable inherited layers cannot be forked",
            }),
        }
    }

    fn is_readable(&self) -> bool {
        matches!(
            self.status(),
            InheritedLayerStatus::Active | InheritedLayerStatus::Materializing
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchReadView {
    branch_id: BranchId,
    active: MutableTable,
    frozen: Vec<FrozenTable>,
    owned_levels: Vec<Vec<BranchOwnedTable>>,
    inherited_layers: Vec<BranchInheritedLayer>,
    facts: BranchStateFacts,
    timestamp_coverage: BranchTimestampCoverage,
}

impl BranchReadView {
    pub(crate) fn new(
        branch_id: BranchId,
        active: MutableTable,
        frozen: Vec<FrozenTable>,
        owned_levels: Vec<Vec<BranchOwnedTable>>,
        facts: BranchStateFacts,
    ) -> BranchRuntimeResult<Self> {
        Self::new_with_inherited(branch_id, active, frozen, owned_levels, Vec::new(), facts)
    }

    pub(crate) fn new_with_inherited(
        branch_id: BranchId,
        active: MutableTable,
        frozen: Vec<FrozenTable>,
        owned_levels: Vec<Vec<BranchOwnedTable>>,
        inherited_layers: Vec<BranchInheritedLayer>,
        facts: BranchStateFacts,
    ) -> BranchRuntimeResult<Self> {
        validate_read_view_inputs(
            branch_id,
            &active,
            &frozen,
            &owned_levels,
            &inherited_layers,
            facts,
        )?;
        Ok(Self {
            branch_id,
            active,
            frozen,
            owned_levels,
            inherited_layers,
            facts,
            timestamp_coverage: BranchTimestampCoverage::unknown(),
        })
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn facts(&self) -> BranchStateFacts {
        self.facts
    }

    pub(crate) const fn timestamp_coverage(&self) -> BranchTimestampCoverage {
        self.timestamp_coverage
    }

    pub(crate) fn with_timestamp_coverage(
        mut self,
        timestamp_coverage: BranchTimestampCoverage,
    ) -> Self {
        self.timestamp_coverage = timestamp_coverage;
        self
    }

    pub(crate) fn active_row_count(&self) -> usize {
        self.active.len()
    }

    pub(crate) fn frozen_table_count(&self) -> usize {
        self.frozen.len()
    }

    pub(crate) fn owned_table_count(&self) -> usize {
        owned_table_count(&self.owned_levels)
    }

    pub(crate) fn owned_levels(&self) -> &[Vec<BranchOwnedTable>] {
        &self.owned_levels
    }

    pub(crate) fn inherited_layer_count(&self) -> usize {
        self.inherited_layers.len()
    }

    pub(crate) fn inherited_layers(&self) -> &[BranchInheritedLayer] {
        &self.inherited_layers
    }

    pub(crate) fn latest(
        &self,
        key: &PhysicalKey,
    ) -> BranchRuntimeResult<Option<BranchVisibleRow>> {
        self.read_point(key, BranchReadBound::latest())
    }

    pub(crate) fn at_version(
        &self,
        key: &PhysicalKey,
        version: CommitVersion,
    ) -> BranchRuntimeResult<Option<BranchVisibleRow>> {
        self.read_point(key, BranchReadBound::at_version(version))
    }

    pub(crate) fn read_point(
        &self,
        key: &PhysicalKey,
        bound: BranchReadBound,
    ) -> BranchRuntimeResult<Option<BranchVisibleRow>> {
        self.require_matching_branch(key.branch_id())?;
        let effective_bound = effective_own_read_bound(bound);
        self.require_timestamp_coverage(bound)?;
        Ok(select_visible_row(
            visible_point_candidates(
                self.branch_id,
                &self.active,
                &self.frozen,
                &self.owned_levels,
                &self.inherited_layers,
                key,
                bound,
                effective_bound,
            )?,
            effective_bound,
        ))
    }

    pub(crate) fn history(
        &self,
        key: &PhysicalKey,
        options: BranchHistoryOptions,
    ) -> BranchRuntimeResult<Vec<BranchHistoryRow>> {
        self.require_matching_branch(key.branch_id())?;
        if options.limit_bound() == Some(0) {
            return Ok(Vec::new());
        }

        let mut rows = self.point_candidates(key, BranchReadBound::latest())?;
        sort_candidates_newest_first(&mut rows);

        let before_version = options.before_version_bound();
        let mut history = Vec::new();
        for candidate in rows {
            let row = candidate_row_ref(&candidate);
            if before_version
                .is_some_and(|version| row.commit_version().as_u64() >= version.as_u64())
            {
                continue;
            }
            if !options.includes_tombstones() && row.is_tombstone() {
                continue;
            }
            history.push(candidate_into_history_row(candidate));
            if options
                .limit_bound()
                .is_some_and(|limit| history.len() >= limit)
            {
                break;
            }
        }
        Ok(history)
    }

    pub(crate) fn scan_prefix(
        &self,
        bounds: &BranchScanBounds,
        bound: BranchReadBound,
    ) -> BranchRuntimeResult<Vec<BranchVisibleRow>> {
        self.scan(bounds, bound)
    }

    pub(crate) fn scan_range(
        &self,
        bounds: &BranchScanBounds,
        bound: BranchReadBound,
    ) -> BranchRuntimeResult<Vec<BranchVisibleRow>> {
        self.scan(bounds, bound)
    }

    pub(crate) fn scan_prefix_including_tombstones(
        &self,
        bounds: &BranchScanBounds,
        bound: BranchReadBound,
    ) -> BranchRuntimeResult<Vec<BranchHistoryRow>> {
        self.scan_including_tombstones(bounds, bound)
    }

    pub(crate) fn scan_range_including_tombstones(
        &self,
        bounds: &BranchScanBounds,
        bound: BranchReadBound,
    ) -> BranchRuntimeResult<Vec<BranchHistoryRow>> {
        self.scan_including_tombstones(bounds, bound)
    }

    fn scan(
        &self,
        bounds: &BranchScanBounds,
        bound: BranchReadBound,
    ) -> BranchRuntimeResult<Vec<BranchVisibleRow>> {
        self.require_matching_branch(bounds.branch_id())?;
        let effective_bound = effective_own_read_bound(bound);
        self.require_timestamp_coverage(bound)?;
        let mut grouped: BTreeMap<TablePhysicalKeyBytes, Vec<CandidateRow>> = BTreeMap::new();
        self.collect_matching_scan_candidates(bounds, bound, &mut grouped)?;

        let mut visible = Vec::new();
        for candidates in grouped.into_values() {
            if let Some(row) = select_visible_row(candidates, effective_bound) {
                visible.push(row);
            }
        }
        Ok(visible)
    }

    fn scan_including_tombstones(
        &self,
        bounds: &BranchScanBounds,
        bound: BranchReadBound,
    ) -> BranchRuntimeResult<Vec<BranchHistoryRow>> {
        self.require_matching_branch(bounds.branch_id())?;
        let effective_bound = effective_own_read_bound(bound);
        self.require_timestamp_coverage(bound)?;
        let mut grouped: BTreeMap<TablePhysicalKeyBytes, Vec<CandidateRow>> = BTreeMap::new();
        self.collect_matching_scan_candidates(bounds, bound, &mut grouped)?;

        let mut visible = Vec::new();
        for candidates in grouped.into_values() {
            if let Some(row) = select_visible_row_or_tombstone(candidates, effective_bound) {
                visible.push(row);
            }
        }
        Ok(visible)
    }

    fn point_candidates(
        &self,
        key: &PhysicalKey,
        bound: BranchReadBound,
    ) -> BranchRuntimeResult<Vec<CandidateRow>> {
        let mut rows = Vec::new();
        let mut rows_visited = self.active.len();
        let initial_candidates = rows.len();
        for row in self.active.iter().filter(|row| row.physical_key() == key) {
            rows.push(candidate_row(row.row().clone(), BranchRowSource::Active));
        }
        for (index, table) in self.frozen.iter().enumerate() {
            rows_visited = rows_visited.saturating_add(table.len());
            for row in table.iter().filter(|row| row.physical_key() == key) {
                rows.push(candidate_row(
                    row.row().clone(),
                    BranchRowSource::Frozen { index },
                ));
            }
        }
        for tables in &self.owned_levels {
            for (table_index, table) in tables.iter().enumerate() {
                rows_visited = rows_visited.saturating_add(table.rows().len());
                for row in table.rows().iter().filter(|row| row.physical_key() == key) {
                    rows.push(candidate_row(
                        row.row().clone(),
                        BranchRowSource::OwnedTable {
                            level: table.level(),
                            table_index,
                        },
                    ));
                }
            }
        }
        perf_trace::record_point_candidate_collection(
            rows_visited,
            rows.len().saturating_sub(initial_candidates),
        );
        self.collect_inherited_point_candidates(key, bound, &mut rows)?;
        Ok(rows)
    }

    fn collect_matching_scan_candidates(
        &self,
        bounds: &BranchScanBounds,
        bound: BranchReadBound,
        grouped: &mut BTreeMap<TablePhysicalKeyBytes, Vec<CandidateRow>>,
    ) -> BranchRuntimeResult<()> {
        let mut rows_visited = self.active.len();
        let mut candidates_materialized = 0usize;
        for row in self
            .active
            .iter()
            .filter(|row| bounds.contains(row.physical_key()))
        {
            grouped
                .entry(TablePhysicalKeyBytes::from_row(row.row()))
                .or_default()
                .push(candidate_row(row.row().clone(), BranchRowSource::Active));
            candidates_materialized = candidates_materialized.saturating_add(1);
        }
        for (index, table) in self.frozen.iter().enumerate() {
            rows_visited = rows_visited.saturating_add(table.len());
            for row in table
                .iter()
                .filter(|row| bounds.contains(row.physical_key()))
            {
                grouped
                    .entry(TablePhysicalKeyBytes::from_row(row.row()))
                    .or_default()
                    .push(candidate_row(
                        row.row().clone(),
                        BranchRowSource::Frozen { index },
                    ));
                candidates_materialized = candidates_materialized.saturating_add(1);
            }
        }
        for tables in &self.owned_levels {
            for (table_index, table) in tables.iter().enumerate() {
                rows_visited = rows_visited.saturating_add(table.rows().len());
                for row in table
                    .rows()
                    .iter()
                    .filter(|row| bounds.contains(row.physical_key()))
                {
                    grouped
                        .entry(TablePhysicalKeyBytes::from_row(row.row()))
                        .or_default()
                        .push(candidate_row(
                            row.row().clone(),
                            BranchRowSource::OwnedTable {
                                level: table.level(),
                                table_index,
                            },
                        ));
                    candidates_materialized = candidates_materialized.saturating_add(1);
                }
            }
        }
        perf_trace::record_scan_candidate_collection(rows_visited, candidates_materialized);
        self.collect_inherited_scan_candidates(bounds, bound, grouped)?;
        Ok(())
    }

    fn collect_inherited_point_candidates(
        &self,
        key: &PhysicalKey,
        bound: BranchReadBound,
        rows: &mut Vec<CandidateRow>,
    ) -> BranchRuntimeResult<()> {
        let mut rows_visited = 0usize;
        let initial_candidates = rows.len();
        for (layer_index, layer) in self.inherited_layers.iter().enumerate() {
            if !layer.is_readable() {
                continue;
            }
            let source_key =
                rewrite_physical_key_branch(key, layer.source_branch_id()).map_err(|_| {
                    BranchRuntimeError::InvalidInheritedLayer {
                        reason: "inherited point key rewrite failed",
                    }
                })?;
            let inherited_bound =
                BranchEffectiveReadBound::for_inherited_layer(bound, layer.fork_version());
            for table in layer.owned_levels().iter().flatten() {
                rows_visited = rows_visited.saturating_add(table.rows().len());
                for row in table.rows().iter().filter(|row| {
                    row.physical_key() == &source_key && inherited_bound.matches_row(row.row())
                }) {
                    rows.push(candidate_row(
                        rewrite_row_branch(row.row(), layer.source_branch_id(), self.branch_id)
                            .map_err(|_| BranchRuntimeError::InvalidInheritedLayer {
                                reason: "inherited row branch rewrite failed",
                            })?,
                        BranchRowSource::Inherited {
                            source_branch_id: layer.source_branch_id(),
                            layer_index,
                        },
                    ));
                }
            }
        }
        perf_trace::record_point_candidate_collection(
            rows_visited,
            rows.len().saturating_sub(initial_candidates),
        );
        Ok(())
    }

    fn collect_inherited_scan_candidates(
        &self,
        bounds: &BranchScanBounds,
        bound: BranchReadBound,
        grouped: &mut BTreeMap<TablePhysicalKeyBytes, Vec<CandidateRow>>,
    ) -> BranchRuntimeResult<()> {
        let mut rows_visited = 0usize;
        let mut candidates_materialized = 0usize;
        for (layer_index, layer) in self.inherited_layers.iter().enumerate() {
            if !layer.is_readable() {
                continue;
            }
            let inherited_bound =
                BranchEffectiveReadBound::for_inherited_layer(bound, layer.fork_version());
            for table in layer.owned_levels().iter().flatten() {
                rows_visited = rows_visited.saturating_add(table.rows().len());
                for row in table
                    .rows()
                    .iter()
                    .filter(|row| inherited_bound.matches_row(row.row()))
                {
                    let rewritten =
                        rewrite_row_branch(row.row(), layer.source_branch_id(), self.branch_id)
                            .map_err(|_| BranchRuntimeError::InvalidInheritedLayer {
                                reason: "inherited row branch rewrite failed",
                            })?;
                    if bounds.contains(rewritten.physical_key()) {
                        grouped
                            .entry(TablePhysicalKeyBytes::from_row(&rewritten))
                            .or_default()
                            .push(candidate_row(
                                rewritten,
                                BranchRowSource::Inherited {
                                    source_branch_id: layer.source_branch_id(),
                                    layer_index,
                                },
                            ));
                        candidates_materialized = candidates_materialized.saturating_add(1);
                    }
                }
            }
        }
        perf_trace::record_scan_candidate_collection(rows_visited, candidates_materialized);
        Ok(())
    }

    fn require_matching_branch(&self, branch_id: BranchId) -> BranchRuntimeResult<()> {
        if branch_id == self.branch_id {
            Ok(())
        } else {
            Err(BranchRuntimeError::InvalidBranchRow {
                reason: "read target branch id does not match read view branch",
            })
        }
    }

    fn require_timestamp_coverage(&self, bound: BranchReadBound) -> BranchRuntimeResult<()> {
        match bound {
            BranchReadBound::Latest | BranchReadBound::AtVersion(_) => Ok(()),
            BranchReadBound::AtTimestamp(timestamp) => self.timestamp_coverage.require_timestamp(
                self.branch_id,
                timestamp,
                BranchTimestampHistorySource::Combined,
            ),
        }
    }
}

impl BranchLocalState {
    pub(crate) fn read_point_or_tombstone_borrowed(
        &self,
        key: &PhysicalKey,
        bound: BranchReadBound,
    ) -> BranchRuntimeResult<Option<BranchHistoryRow>> {
        require_state_matching_branch(self, key.branch_id())?;
        require_state_timestamp_coverage(self, bound)?;
        let effective_bound = effective_own_read_bound(bound);
        Ok(select_visible_row_or_tombstone(
            visible_point_candidates(
                self.branch_id(),
                self.active(),
                self.frozen(),
                self.owned_levels(),
                self.inherited_layers(),
                key,
                bound,
                effective_bound,
            )?,
            effective_bound,
        ))
    }

    pub(crate) fn scan_including_tombstones_borrowed(
        &self,
        bounds: &BranchScanBounds,
        bound: BranchReadBound,
        visible_limit: Option<usize>,
        visible_limit_timestamp: Option<Timestamp>,
    ) -> BranchRuntimeResult<Vec<BranchHistoryRow>> {
        require_state_matching_branch(self, bounds.branch_id())?;
        require_state_timestamp_coverage(self, bound)?;
        scan_including_tombstones_from_sources(
            self.branch_id(),
            self.active(),
            self.frozen(),
            self.owned_levels(),
            self.inherited_layers(),
            bounds,
            bound,
            visible_limit,
            visible_limit_timestamp,
        )
    }
}

type CandidateRow = (StorageRow, BranchRowSource);

fn candidate_row(row: StorageRow, source: BranchRowSource) -> CandidateRow {
    (row, source)
}

fn candidate_row_ref(candidate: &CandidateRow) -> &StorageRow {
    &candidate.0
}

fn candidate_source(candidate: &CandidateRow) -> BranchRowSource {
    candidate.1
}

fn candidate_into_visible_row(
    (row, source): CandidateRow,
    read_timestamp: Option<Timestamp>,
) -> Option<BranchVisibleRow> {
    if row.is_tombstone() || row_is_expired_at(&row, read_timestamp) {
        None
    } else {
        Some(BranchVisibleRow::new(row, source))
    }
}

fn candidate_into_history_row((row, source): CandidateRow) -> BranchHistoryRow {
    BranchHistoryRow::new(row, source)
}

fn effective_own_read_bound(bound: BranchReadBound) -> BranchEffectiveReadBound {
    BranchEffectiveReadBound::for_own_branch(bound)
}

fn visible_point_candidates(
    branch_id: BranchId,
    active: &MutableTable,
    frozen: &[FrozenTable],
    owned_levels: &[Vec<BranchOwnedTable>],
    inherited_layers: &[BranchInheritedLayer],
    key: &PhysicalKey,
    bound: BranchReadBound,
    effective_bound: BranchEffectiveReadBound,
) -> BranchRuntimeResult<Vec<CandidateRow>> {
    let mut rows = Vec::new();
    let mut rows_visited = 0usize;

    let (row, visited) = active.seek_physical_key(
        key,
        effective_bound.max_commit_version(),
        effective_bound.max_commit_timestamp(),
    );
    rows_visited = rows_visited.saturating_add(visited);
    if let Some(row) = row {
        rows.push(candidate_row(row.row().clone(), BranchRowSource::Active));
    }

    for (index, table) in frozen.iter().enumerate() {
        let (row, visited) = table.seek_physical_key(
            key,
            effective_bound.max_commit_version(),
            effective_bound.max_commit_timestamp(),
        );
        rows_visited = rows_visited.saturating_add(visited);
        if let Some(row) = row {
            rows.push(candidate_row(
                row.row().clone(),
                BranchRowSource::Frozen { index },
            ));
        }
    }

    for tables in owned_levels {
        for (table_index, table) in tables.iter().enumerate() {
            let (row, visited) = table.reader().seek_physical_key(
                key,
                effective_bound.max_commit_version(),
                effective_bound.max_commit_timestamp(),
            );
            rows_visited = rows_visited.saturating_add(visited);
            if let Some(row) = row {
                rows.push(candidate_row(
                    row.row().clone(),
                    BranchRowSource::OwnedTable {
                        level: table.level(),
                        table_index,
                    },
                ));
            }
        }
    }

    collect_visible_inherited_point_candidates(
        branch_id,
        inherited_layers,
        key,
        bound,
        &mut rows,
        &mut rows_visited,
    )?;
    perf_trace::record_point_candidate_collection(rows_visited, rows.len());
    Ok(rows)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BranchScanCursorSource {
    Active,
    Frozen {
        index: usize,
    },
    OwnedTable {
        level: BranchLevel,
        table_index: usize,
    },
    Inherited {
        source_branch_id: BranchId,
        layer_index: usize,
        child_branch_id: BranchId,
    },
}

struct BranchScanCursor<'a> {
    cursor: Box<dyn TableCursor + 'a>,
    source: BranchScanCursorSource,
    effective_bound: BranchEffectiveReadBound,
    current_logical_key: Option<TablePhysicalKeyBytes>,
}

impl<'a> BranchScanCursor<'a> {
    fn new(
        cursor: Box<dyn TableCursor + 'a>,
        source: BranchScanCursorSource,
        effective_bound: BranchEffectiveReadBound,
    ) -> Self {
        Self {
            cursor,
            source,
            effective_bound,
            current_logical_key: None,
        }
    }

    fn seek_to_first(&mut self) -> BranchRuntimeResult<()> {
        self.cursor
            .seek_to_first()
            .map_err(|_| BranchRuntimeError::InvalidBranchState {
                reason: "branch scan cursor seek failed",
            })?;
        self.refresh_current_logical_key()
    }

    fn advance(&mut self) -> BranchRuntimeResult<()> {
        self.cursor
            .advance()
            .map_err(|_| BranchRuntimeError::InvalidBranchState {
                reason: "branch scan cursor advance failed",
            })?;
        self.refresh_current_logical_key()
    }

    fn current_logical_physical_key(&self) -> Option<&TablePhysicalKeyBytes> {
        self.current_logical_key.as_ref()
    }

    fn refresh_current_logical_key(&mut self) -> BranchRuntimeResult<()> {
        self.current_logical_key = self.compute_current_logical_key()?;
        Ok(())
    }

    fn compute_current_logical_key(&self) -> BranchRuntimeResult<Option<TablePhysicalKeyBytes>> {
        let Some(row) = self.cursor.current() else {
            return Ok(None);
        };
        match self.source {
            BranchScanCursorSource::Inherited {
                source_branch_id,
                child_branch_id,
                ..
            } => {
                perf_trace::record_scan_logical_key_encode();
                let key = rewrite_physical_key_branch(row.physical_key(), child_branch_id)
                    .map_err(|_| BranchRuntimeError::InvalidInheritedLayer {
                        reason: "inherited scan key branch rewrite failed",
                    })?;
                if key.branch_id() == source_branch_id {
                    return Err(BranchRuntimeError::InvalidInheritedLayer {
                        reason: "inherited scan key rewrite did not change branch",
                    });
                }
                Ok(Some(TablePhysicalKeyBytes::from_physical_key(&key)))
            }
            BranchScanCursorSource::Active
            | BranchScanCursorSource::Frozen { .. }
            | BranchScanCursorSource::OwnedTable { .. } => {
                perf_trace::record_scan_logical_key_encode();
                Ok(Some(TablePhysicalKeyBytes::from_row(row.row())))
            }
        }
    }

    fn current_candidate(&self) -> BranchRuntimeResult<Option<CandidateRow>> {
        let Some(row) = self.cursor.current() else {
            return Ok(None);
        };
        if !self.effective_bound.matches_row(row.row()) {
            return Ok(None);
        }
        match self.source {
            BranchScanCursorSource::Active => {
                record_scan_candidate_clone(row.row());
                Ok(Some(candidate_row(
                    row.row().clone(),
                    BranchRowSource::Active,
                )))
            }
            BranchScanCursorSource::Frozen { index } => {
                record_scan_candidate_clone(row.row());
                Ok(Some(candidate_row(
                    row.row().clone(),
                    BranchRowSource::Frozen { index },
                )))
            }
            BranchScanCursorSource::OwnedTable { level, table_index } => {
                record_scan_candidate_clone(row.row());
                Ok(Some(candidate_row(
                    row.row().clone(),
                    BranchRowSource::OwnedTable { level, table_index },
                )))
            }
            BranchScanCursorSource::Inherited {
                source_branch_id,
                layer_index,
                child_branch_id,
            } => {
                record_scan_candidate_clone(row.row());
                Ok(Some(candidate_row(
                    rewrite_row_branch(row.row(), source_branch_id, child_branch_id).map_err(
                        |_| BranchRuntimeError::InvalidInheritedLayer {
                            reason: "inherited scan row branch rewrite failed",
                        },
                    )?,
                    BranchRowSource::Inherited {
                        source_branch_id,
                        layer_index,
                    },
                )))
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct BranchScanHeapItem {
    key: TablePhysicalKeyBytes,
    source_index: usize,
}

impl Ord for BranchScanHeapItem {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .key
            .cmp(&self.key)
            .then_with(|| other.source_index.cmp(&self.source_index))
    }
}

impl PartialOrd for BranchScanHeapItem {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "scan source assembly mirrors branch read-view source families"
)]
fn scan_including_tombstones_from_sources(
    branch_id: BranchId,
    active: &MutableTable,
    frozen: &[FrozenTable],
    owned_levels: &[Vec<BranchOwnedTable>],
    inherited_layers: &[BranchInheritedLayer],
    bounds: &BranchScanBounds,
    bound: BranchReadBound,
    visible_limit: Option<usize>,
    visible_limit_timestamp: Option<Timestamp>,
) -> BranchRuntimeResult<Vec<BranchHistoryRow>> {
    let effective_bound = effective_own_read_bound(bound);
    let source_setup_timer = perf_trace::start_timer();
    let mut cursors = scan_cursors_for_sources(
        branch_id,
        active,
        frozen,
        owned_levels,
        inherited_layers,
        bounds,
        bound,
    )?;
    for cursor in &mut cursors {
        cursor.seek_to_first()?;
    }
    perf_trace::record_branch_scan_source_setup_elapsed(source_setup_timer);

    let merge_timer = perf_trace::start_timer();
    if let [cursor] = cursors.as_mut_slice() {
        let rows = scan_single_source_including_tombstones(
            cursor,
            effective_bound,
            visible_limit,
            visible_limit_timestamp,
        )?;
        perf_trace::record_branch_scan_merge_elapsed(merge_timer);
        return Ok(rows);
    }

    let rows = scan_heap_sources_including_tombstones(
        &mut cursors,
        effective_bound,
        visible_limit,
        visible_limit_timestamp,
    )?;
    perf_trace::record_branch_scan_merge_elapsed(merge_timer);
    Ok(rows)
}

fn scan_heap_sources_including_tombstones(
    cursors: &mut [BranchScanCursor<'_>],
    effective_bound: BranchEffectiveReadBound,
    visible_limit: Option<usize>,
    visible_limit_timestamp: Option<Timestamp>,
) -> BranchRuntimeResult<Vec<BranchHistoryRow>> {
    let mut heap = BinaryHeap::new();
    for (source_index, cursor) in cursors.iter().enumerate() {
        if let Some(key) = cursor.current_logical_physical_key() {
            heap.push(BranchScanHeapItem {
                key: key.clone(),
                source_index,
            });
        }
    }

    let mut rows = Vec::new();
    let mut visible_rows = 0usize;
    let mut candidate_rows = 0usize;
    loop {
        let min_key_timer = perf_trace::start_timer();
        let selected = heap.pop();
        if selected.is_some() {
            perf_trace::record_branch_scan_min_key_elapsed(min_key_timer);
        }
        let Some(selected) = selected else {
            break;
        };
        let selected_key = selected.key;

        let mut candidates = Vec::new();
        candidate_rows = candidate_rows.saturating_add(collect_scan_group_from_cursor(
            &selected_key,
            selected.source_index,
            cursors,
            &mut heap,
            &mut candidates,
        )?);
        while heap.peek().is_some_and(|item| item.key == selected_key) {
            let matching = heap.pop().expect("matching heap item");
            candidate_rows = candidate_rows.saturating_add(collect_scan_group_from_cursor(
                &selected_key,
                matching.source_index,
                cursors,
                &mut heap,
                &mut candidates,
            )?);
        }

        let select_timer = perf_trace::start_timer();
        let selected = select_visible_row_or_tombstone(candidates, effective_bound);
        perf_trace::record_branch_scan_select_elapsed(select_timer);
        if let Some(row) = selected {
            let counts_for_limit =
                !row.row().is_tombstone() && !row_is_expired_at(row.row(), visible_limit_timestamp);
            rows.push(row);
            if counts_for_limit {
                visible_rows = visible_rows.saturating_add(1);
                if visible_limit.is_some_and(|limit| visible_rows >= limit) {
                    break;
                }
            }
        }
    }
    perf_trace::record_scan_candidate_collection(candidate_rows, candidate_rows);
    Ok(rows)
}

fn scan_single_source_including_tombstones(
    cursor: &mut BranchScanCursor<'_>,
    effective_bound: BranchEffectiveReadBound,
    visible_limit: Option<usize>,
    visible_limit_timestamp: Option<Timestamp>,
) -> BranchRuntimeResult<Vec<BranchHistoryRow>> {
    let mut rows = Vec::new();
    let mut visible_rows = 0usize;
    let mut candidate_rows = 0usize;
    loop {
        let Some(selected_key) = cursor.current_logical_physical_key().cloned() else {
            break;
        };

        let mut candidates = Vec::new();
        loop {
            let group_key_timer = perf_trace::start_timer();
            let cursor_key = cursor.current_logical_physical_key();
            perf_trace::record_branch_scan_group_key_elapsed(group_key_timer);
            if cursor_key != Some(&selected_key) {
                break;
            }

            let candidate_timer = perf_trace::start_timer();
            let candidate = cursor.current_candidate()?;
            perf_trace::record_branch_scan_candidate_elapsed(candidate_timer);
            if let Some(candidate) = candidate {
                candidate_rows = candidate_rows.saturating_add(1);
                candidates.push(candidate);
            }

            let advance_timer = perf_trace::start_timer();
            cursor.advance()?;
            perf_trace::record_branch_scan_advance_elapsed(advance_timer);
        }

        let select_timer = perf_trace::start_timer();
        let selected = select_visible_row_or_tombstone(candidates, effective_bound);
        perf_trace::record_branch_scan_select_elapsed(select_timer);
        if let Some(row) = selected {
            let counts_for_limit =
                !row.row().is_tombstone() && !row_is_expired_at(row.row(), visible_limit_timestamp);
            rows.push(row);
            if counts_for_limit {
                visible_rows = visible_rows.saturating_add(1);
                if visible_limit.is_some_and(|limit| visible_rows >= limit) {
                    break;
                }
            }
        }
    }
    perf_trace::record_scan_candidate_collection(candidate_rows, candidate_rows);
    Ok(rows)
}

fn collect_scan_group_from_cursor(
    selected_key: &TablePhysicalKeyBytes,
    source_index: usize,
    cursors: &mut [BranchScanCursor<'_>],
    heap: &mut BinaryHeap<BranchScanHeapItem>,
    candidates: &mut Vec<CandidateRow>,
) -> BranchRuntimeResult<usize> {
    let cursor = &mut cursors[source_index];
    let mut candidate_rows = 0usize;
    loop {
        let group_key_timer = perf_trace::start_timer();
        let cursor_key = cursor.current_logical_physical_key();
        perf_trace::record_branch_scan_group_key_elapsed(group_key_timer);
        if cursor_key != Some(selected_key) {
            break;
        }

        let candidate_timer = perf_trace::start_timer();
        let candidate = cursor.current_candidate()?;
        perf_trace::record_branch_scan_candidate_elapsed(candidate_timer);
        if let Some(candidate) = candidate {
            candidate_rows = candidate_rows.saturating_add(1);
            candidates.push(candidate);
        }

        let advance_timer = perf_trace::start_timer();
        cursor.advance()?;
        perf_trace::record_branch_scan_advance_elapsed(advance_timer);
    }
    if let Some(key) = cursor.current_logical_physical_key() {
        heap.push(BranchScanHeapItem {
            key: key.clone(),
            source_index,
        });
    }
    Ok(candidate_rows)
}

fn record_scan_candidate_clone(row: &StorageRow) {
    let bytes = row
        .physical_key()
        .space()
        .len()
        .saturating_add(row.physical_key().user_key().len())
        .saturating_add(row.value().len());
    perf_trace::record_scan_candidate_row_clone(bytes);
}

fn scan_cursors_for_sources<'a>(
    branch_id: BranchId,
    active: &'a MutableTable,
    frozen: &'a [FrozenTable],
    owned_levels: &'a [Vec<BranchOwnedTable>],
    inherited_layers: &'a [BranchInheritedLayer],
    bounds: &BranchScanBounds,
    bound: BranchReadBound,
) -> BranchRuntimeResult<Vec<BranchScanCursor<'a>>> {
    let own_bounds = bounds.table_key_bounds()?;
    let own_bound = BranchEffectiveReadBound::for_own_branch(bound);
    let mut cursors = Vec::new();
    cursors.push(BranchScanCursor::new(
        Box::new(BoundedTableCursor::new(
            Box::new(active.cursor()),
            own_bounds.clone(),
        )),
        BranchScanCursorSource::Active,
        own_bound,
    ));
    for (index, table) in frozen.iter().enumerate() {
        cursors.push(BranchScanCursor::new(
            Box::new(BoundedTableCursor::new(
                Box::new(table.cursor()),
                own_bounds.clone(),
            )),
            BranchScanCursorSource::Frozen { index },
            own_bound,
        ));
    }
    for tables in owned_levels {
        for (table_index, table) in tables.iter().enumerate() {
            cursors.push(BranchScanCursor::new(
                Box::new(BoundedTableCursor::new(
                    Box::new(table.reader().cursor()),
                    own_bounds.clone(),
                )),
                BranchScanCursorSource::OwnedTable {
                    level: table.level(),
                    table_index,
                },
                own_bound,
            ));
        }
    }
    for (layer_index, layer) in inherited_layers.iter().enumerate() {
        if !layer.is_readable() {
            continue;
        }
        let inherited_bound =
            BranchEffectiveReadBound::for_inherited_layer(bound, layer.fork_version());
        let source_bounds = bounds.table_key_bounds_for_branch(layer.source_branch_id())?;
        for table in layer.owned_levels().iter().flatten() {
            cursors.push(BranchScanCursor::new(
                Box::new(BoundedTableCursor::new(
                    Box::new(table.reader().cursor()),
                    source_bounds.clone(),
                )),
                BranchScanCursorSource::Inherited {
                    source_branch_id: layer.source_branch_id(),
                    layer_index,
                    child_branch_id: branch_id,
                },
                inherited_bound,
            ));
        }
    }
    Ok(cursors)
}

fn collect_visible_inherited_point_candidates(
    branch_id: BranchId,
    inherited_layers: &[BranchInheritedLayer],
    key: &PhysicalKey,
    bound: BranchReadBound,
    rows: &mut Vec<CandidateRow>,
    rows_visited: &mut usize,
) -> BranchRuntimeResult<()> {
    for (layer_index, layer) in inherited_layers.iter().enumerate() {
        if !layer.is_readable() {
            continue;
        }
        let source_key =
            rewrite_physical_key_branch(key, layer.source_branch_id()).map_err(|_| {
                BranchRuntimeError::InvalidInheritedLayer {
                    reason: "inherited point key rewrite failed",
                }
            })?;
        let inherited_bound =
            BranchEffectiveReadBound::for_inherited_layer(bound, layer.fork_version());
        for table in layer.owned_levels().iter().flatten() {
            let (row, visited) = table.reader().seek_physical_key(
                &source_key,
                inherited_bound.max_commit_version(),
                inherited_bound.max_commit_timestamp(),
            );
            *rows_visited = (*rows_visited).saturating_add(visited);
            if let Some(row) = row {
                rows.push(candidate_row(
                    rewrite_row_branch(row.row(), layer.source_branch_id(), branch_id).map_err(
                        |_| BranchRuntimeError::InvalidInheritedLayer {
                            reason: "inherited row branch rewrite failed",
                        },
                    )?,
                    BranchRowSource::Inherited {
                        source_branch_id: layer.source_branch_id(),
                        layer_index,
                    },
                ));
            }
        }
    }
    Ok(())
}

fn require_state_matching_branch(
    state: &BranchLocalState,
    branch_id: BranchId,
) -> BranchRuntimeResult<()> {
    if branch_id == state.branch_id() {
        Ok(())
    } else {
        Err(BranchRuntimeError::InvalidBranchRow {
            reason: "read target branch id does not match read view branch",
        })
    }
}

fn require_state_timestamp_coverage(
    state: &BranchLocalState,
    bound: BranchReadBound,
) -> BranchRuntimeResult<()> {
    match bound {
        BranchReadBound::Latest | BranchReadBound::AtVersion(_) => Ok(()),
        BranchReadBound::AtTimestamp(timestamp) => state.timestamp_coverage().require_timestamp(
            state.branch_id(),
            timestamp,
            BranchTimestampHistorySource::Combined,
        ),
    }
}

fn select_visible_row(
    mut candidates: Vec<CandidateRow>,
    effective_bound: BranchEffectiveReadBound,
) -> Option<BranchVisibleRow> {
    sort_candidates_newest_first(&mut candidates);
    let candidate = candidates
        .into_iter()
        .find(|candidate| effective_bound.matches_row(candidate_row_ref(candidate)))?;
    candidate_into_visible_row(candidate, effective_bound.max_commit_timestamp())
}

fn select_visible_row_or_tombstone(
    mut candidates: Vec<CandidateRow>,
    effective_bound: BranchEffectiveReadBound,
) -> Option<BranchHistoryRow> {
    sort_candidates_newest_first(&mut candidates);
    let candidate = candidates
        .into_iter()
        .find(|candidate| effective_bound.matches_row(candidate_row_ref(candidate)))?;
    if row_is_expired_at(
        candidate_row_ref(&candidate),
        effective_bound.max_commit_timestamp(),
    ) {
        None
    } else {
        Some(candidate_into_history_row(candidate))
    }
}

fn row_is_expired_at(row: &StorageRow, read_timestamp: Option<Timestamp>) -> bool {
    // This layer only applies TTL when the caller supplies an as-of timestamp.
    // Latest and version reads have no wall-clock input at this layer.
    read_timestamp.is_some_and(|timestamp| {
        !row.is_tombstone() && row.expires_at() != Timestamp::EPOCH && row.expires_at() <= timestamp
    })
}

fn sort_candidates_newest_first(candidates: &mut [CandidateRow]) {
    candidates.sort_by(|left, right| {
        right
            .0
            .commit_version()
            .as_u64()
            .cmp(&left.0.commit_version().as_u64())
            .then_with(|| source_order_cmp(candidate_source(left), candidate_source(right)))
    });
}

fn source_order_cmp(left: BranchRowSource, right: BranchRowSource) -> Ordering {
    match (left, right) {
        (BranchRowSource::Active, BranchRowSource::Active) => Ordering::Equal,
        (BranchRowSource::Active, _) => Ordering::Less,
        (_, BranchRowSource::Active) => Ordering::Greater,
        (BranchRowSource::Frozen { index: left }, BranchRowSource::Frozen { index: right }) => {
            left.cmp(&right)
        }
        (BranchRowSource::Frozen { .. }, _) => Ordering::Less,
        (_, BranchRowSource::Frozen { .. }) => Ordering::Greater,
        (
            BranchRowSource::OwnedTable {
                level: left_level,
                table_index: left_index,
            },
            BranchRowSource::OwnedTable {
                level: right_level,
                table_index: right_index,
            },
        ) => left_level
            .raw()
            .cmp(&right_level.raw())
            .then_with(|| left_index.cmp(&right_index)),
        (BranchRowSource::OwnedTable { .. }, _) => Ordering::Less,
        (_, BranchRowSource::OwnedTable { .. }) => Ordering::Greater,
        (
            BranchRowSource::Inherited {
                source_branch_id: left_source,
                layer_index: left_index,
            },
            BranchRowSource::Inherited {
                source_branch_id: right_source,
                layer_index: right_index,
            },
        ) => left_index
            .cmp(&right_index)
            .then_with(|| left_source.as_bytes().cmp(right_source.as_bytes())),
    }
}

fn validate_user_key_bound_order(
    lower: &BranchUserKeyBound,
    upper: &BranchUserKeyBound,
) -> BranchRuntimeResult<()> {
    if let (Some(lower), Some(upper)) = (lower.finite(), upper.finite()) {
        if lower > upper {
            return Err(BranchRuntimeError::InvalidReadBound {
                reason: "lower user-key bound must not sort after upper user-key bound",
            });
        }
    }
    Ok(())
}

fn validate_scan_space(
    branch_id: BranchId,
    space: &str,
    storage_space_id: StorageSpaceId,
) -> BranchRuntimeResult<()> {
    PhysicalKey::new(branch_id, space.to_owned(), storage_space_id, Vec::new())
        .map(|_| ())
        .map_err(|_| BranchRuntimeError::InvalidReadBound {
            reason: "scan bounds must use a valid physical key space",
        })
}

fn scan_physical_key(
    branch_id: BranchId,
    space: &str,
    storage_space_id: StorageSpaceId,
    user_key: Vec<u8>,
) -> BranchRuntimeResult<PhysicalKey> {
    PhysicalKey::new(branch_id, space.to_owned(), storage_space_id, user_key).map_err(|_| {
        BranchRuntimeError::InvalidReadBound {
            reason: "scan physical key bound is invalid",
        }
    })
}

fn table_physical_bound_for_user_key(
    branch_id: BranchId,
    space: &str,
    storage_space_id: StorageSpaceId,
    bound: &BranchUserKeyBound,
) -> BranchRuntimeResult<TablePhysicalKeyBound> {
    match bound {
        BranchUserKeyBound::Unbounded => Ok(TablePhysicalKeyBound::Unbounded),
        BranchUserKeyBound::Included(user_key) => {
            let key = scan_physical_key(branch_id, space, storage_space_id, user_key.clone())?;
            Ok(TablePhysicalKeyBound::included(
                TablePhysicalKeyBytes::from_physical_key(&key),
            ))
        }
        BranchUserKeyBound::Excluded(user_key) => {
            let key = scan_physical_key(branch_id, space, storage_space_id, user_key.clone())?;
            Ok(TablePhysicalKeyBound::excluded(
                TablePhysicalKeyBytes::from_physical_key(&key),
            ))
        }
    }
}

fn lower_contains(bound: &BranchUserKeyBound, user_key: &[u8]) -> bool {
    match bound {
        BranchUserKeyBound::Unbounded => true,
        BranchUserKeyBound::Included(lower) => user_key >= lower.as_slice(),
        BranchUserKeyBound::Excluded(lower) => user_key > lower.as_slice(),
    }
}

fn upper_contains(bound: &BranchUserKeyBound, user_key: &[u8]) -> bool {
    match bound {
        BranchUserKeyBound::Unbounded => true,
        BranchUserKeyBound::Included(upper) => user_key <= upper.as_slice(),
        BranchUserKeyBound::Excluded(upper) => user_key < upper.as_slice(),
    }
}

fn validate_read_view_inputs(
    branch_id: BranchId,
    active: &MutableTable,
    frozen: &[FrozenTable],
    owned_levels: &[Vec<BranchOwnedTable>],
    inherited_layers: &[BranchInheritedLayer],
    facts: BranchStateFacts,
) -> BranchRuntimeResult<()> {
    if branch_id != facts.branch_id() {
        return Err(BranchRuntimeError::InvalidBranchState {
            reason: "read view branch id must match branch facts",
        });
    }
    if facts.active_rows()
        != u64::try_from(active.len()).expect("active read-view row count fits in u64")
    {
        return Err(BranchRuntimeError::InvalidBranchState {
            reason: "read view active row count must match branch facts",
        });
    }
    if facts.frozen_table_count() != frozen.len() {
        return Err(BranchRuntimeError::InvalidBranchState {
            reason: "read view frozen table count must match branch facts",
        });
    }
    if facts.owned_table_count() != owned_table_count(owned_levels) {
        return Err(BranchRuntimeError::InvalidBranchState {
            reason: "read view owned table count must match branch facts",
        });
    }
    if facts.inherited_layer_count() != inherited_layers.len() {
        return Err(BranchRuntimeError::InvalidBranchState {
            reason: "read view inherited layer count must match branch facts",
        });
    }
    validate_owned_levels(owned_levels)?;
    validate_inherited_layers(branch_id, inherited_layers)?;

    perf_trace::record_read_view_validation_scan(read_view_validation_row_count(
        active,
        frozen,
        owned_levels,
        inherited_layers,
    ));

    let mut max_commit_version = None;
    let mut timestamp_min = None;
    let mut timestamp_max = None;
    for row in active.iter() {
        record_read_view_row_facts(
            branch_id,
            row.row(),
            &mut max_commit_version,
            &mut timestamp_min,
            &mut timestamp_max,
        )?;
    }
    for table in frozen {
        for row in table.iter() {
            record_read_view_row_facts(
                branch_id,
                row.row(),
                &mut max_commit_version,
                &mut timestamp_min,
                &mut timestamp_max,
            )?;
        }
    }
    for tables in owned_levels {
        for table in tables {
            for row in table.rows() {
                record_read_view_row_facts(
                    branch_id,
                    row.row(),
                    &mut max_commit_version,
                    &mut timestamp_min,
                    &mut timestamp_max,
                )?;
            }
        }
    }
    for layer in inherited_layers {
        record_inherited_layer_read_view_facts(
            layer,
            &mut max_commit_version,
            &mut timestamp_min,
            &mut timestamp_max,
        )?;
    }
    if max_commit_version != facts.max_commit_version()
        || timestamp_min != facts.timestamp_min()
        || timestamp_max != facts.timestamp_max()
    {
        return Err(BranchRuntimeError::InvalidBranchState {
            reason: "read view source facts must match branch facts",
        });
    }
    Ok(())
}

fn read_view_validation_row_count(
    active: &MutableTable,
    frozen: &[FrozenTable],
    owned_levels: &[Vec<BranchOwnedTable>],
    inherited_layers: &[BranchInheritedLayer],
) -> usize {
    active
        .len()
        .saturating_add(frozen.iter().map(FrozenTable::len).sum::<usize>())
        .saturating_add(
            owned_levels
                .iter()
                .flatten()
                .map(|table| table.rows().len())
                .sum::<usize>(),
        )
        .saturating_add(
            inherited_layers
                .iter()
                .filter(|layer| layer.status() != InheritedLayerStatus::Materialized)
                .flat_map(|layer| layer.owned_levels().iter().flatten())
                .map(|table| table.rows().len())
                .sum::<usize>(),
        )
}

fn owned_table_count(owned_levels: &[Vec<BranchOwnedTable>]) -> usize {
    owned_levels.iter().map(Vec::len).sum()
}

pub(super) fn inherited_table_count(layers: &[BranchInheritedLayer]) -> usize {
    layers.iter().map(BranchInheritedLayer::table_count).sum()
}

fn validate_owned_levels(owned_levels: &[Vec<BranchOwnedTable>]) -> BranchRuntimeResult<()> {
    for (level_index, tables) in owned_levels.iter().enumerate() {
        let level = BranchLevel::new(u8::try_from(level_index).map_err(|_| {
            BranchRuntimeError::InvalidBranchState {
                reason: "branch-owned table level index must fit in BranchLevel",
            }
        })?);
        for table in tables {
            if table.level() != level {
                return Err(BranchRuntimeError::InvalidBranchState {
                    reason: "branch-owned table descriptor level must match level index",
                });
            }
        }
        if level != BranchLevel::ZERO {
            validate_non_overlapping_tables(tables)?;
        }
    }
    Ok(())
}

fn validate_inherited_layer_unique_keys(
    owned_levels: &[Vec<BranchOwnedTable>],
) -> BranchRuntimeResult<()> {
    let mut keys = BTreeSet::<TableInternalKeyBytes>::new();
    for table in owned_levels.iter().flatten() {
        for row in table.rows() {
            if !keys.insert(row.key().clone()) {
                return Err(BranchRuntimeError::InvalidInheritedLayer {
                    reason: "inherited layer tables must not contain duplicate internal keys",
                });
            }
        }
    }
    Ok(())
}

fn validate_inherited_layer_unique_table_identities(
    owned_levels: &[Vec<BranchOwnedTable>],
) -> BranchRuntimeResult<()> {
    let mut identities = BTreeSet::<&str>::new();
    for table in owned_levels.iter().flatten() {
        if !identities.insert(table.descriptor().identity().as_str()) {
            return Err(BranchRuntimeError::InvalidInheritedLayer {
                reason: "inherited layer tables must not contain duplicate table identities",
            });
        }
    }
    Ok(())
}

fn validate_inherited_layers(
    branch_id: BranchId,
    layers: &[BranchInheritedLayer],
) -> BranchRuntimeResult<()> {
    for layer in layers {
        if layer.source_branch_id() == branch_id {
            return Err(BranchRuntimeError::InvalidInheritedLayer {
                reason: "inherited layer source branch must differ from child branch",
            });
        }
        if layer.status() == InheritedLayerStatus::Unavailable {
            return Err(BranchRuntimeError::InvalidInheritedLayer {
                reason: "unavailable inherited layers cannot be read",
            });
        }
        if layer.descriptor().table_count() != layer.table_count() {
            return Err(BranchRuntimeError::InvalidInheritedLayer {
                reason: "inherited layer table count must match descriptor",
            });
        }
        validate_inherited_layer_unique_table_identities(layer.owned_levels())?;
        for table in layer.owned_levels().iter().flatten() {
            if table.branch_id() != layer.source_branch_id() {
                return Err(BranchRuntimeError::InvalidInheritedLayer {
                    reason: "inherited table source branch must match layer",
                });
            }
            if table
                .rows()
                .iter()
                .any(|row| row.physical_key().branch_id() != layer.source_branch_id())
            {
                return Err(BranchRuntimeError::InvalidInheritedLayer {
                    reason: "inherited table rows must match layer source branch",
                });
            }
        }
    }
    Ok(())
}

fn validate_non_overlapping_tables(tables: &[BranchOwnedTable]) -> BranchRuntimeResult<()> {
    for pair in tables.windows(2) {
        let left_first = require_table_physical_first_key(&pair[0])?;
        let right_first = require_table_physical_first_key(&pair[1])?;
        if left_first > right_first || table_physical_ranges_overlap(&pair[0], &pair[1]) {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "branch-owned nonzero levels must be sorted and non-overlapping by physical key range",
            });
        }
    }
    Ok(())
}

pub(super) fn table_physical_ranges_overlap(
    left: &BranchOwnedTable,
    right: &BranchOwnedTable,
) -> bool {
    let Some((left_first, left_last)) = table_physical_key_bounds(left) else {
        return false;
    };
    let Some((right_first, right_last)) = table_physical_key_bounds(right) else {
        return false;
    };
    left_first <= right_last && right_first <= left_last
}

pub(super) fn table_physical_first_key(table: &BranchOwnedTable) -> Option<TablePhysicalKeyBytes> {
    table_physical_key_bounds(table).map(|(first, _)| first)
}

pub(super) fn require_table_physical_first_key(
    table: &BranchOwnedTable,
) -> BranchRuntimeResult<TablePhysicalKeyBytes> {
    table_physical_first_key(table).ok_or(BranchRuntimeError::InvalidBranchState {
        reason: "branch-owned table must contain at least one physical key",
    })
}

fn table_physical_key_bounds(
    table: &BranchOwnedTable,
) -> Option<(TablePhysicalKeyBytes, TablePhysicalKeyBytes)> {
    let mut keys = table
        .rows()
        .iter()
        .map(|row| TablePhysicalKeyBytes::from_row(row.row()));
    let first = keys.next()?;
    let (min, max) = keys.fold((first.clone(), first), |(min, max), key| {
        let next_min = if key < min { key.clone() } else { min };
        let next_max = if key > max { key } else { max };
        (next_min, next_max)
    });
    Some((min, max))
}

fn record_read_view_row_facts(
    branch_id: BranchId,
    row: &StorageRow,
    max_commit_version: &mut Option<CommitVersion>,
    timestamp_min: &mut Option<Timestamp>,
    timestamp_max: &mut Option<Timestamp>,
) -> BranchRuntimeResult<()> {
    if row.physical_key().branch_id() != branch_id {
        return Err(BranchRuntimeError::InvalidBranchState {
            reason: "read view source rows must match the view branch",
        });
    }
    record_commit_version(max_commit_version, row.commit_version());
    record_timestamp(timestamp_min, timestamp_max, row.commit_timestamp());
    Ok(())
}

fn record_inherited_layer_read_view_facts(
    layer: &BranchInheritedLayer,
    max_commit_version: &mut Option<CommitVersion>,
    timestamp_min: &mut Option<Timestamp>,
    timestamp_max: &mut Option<Timestamp>,
) -> BranchRuntimeResult<()> {
    if layer.status() == InheritedLayerStatus::Materialized {
        return Ok(());
    }
    for table in layer.owned_levels().iter().flatten() {
        for row in table.rows() {
            if row.physical_key().branch_id() != layer.source_branch_id() {
                return Err(BranchRuntimeError::InvalidInheritedLayer {
                    reason: "inherited read view source rows must match layer source branch",
                });
            }
            if row.commit_version().as_u64() <= layer.fork_version().as_u64() {
                record_commit_version(max_commit_version, row.commit_version());
                record_timestamp(timestamp_min, timestamp_max, row.commit_timestamp());
            }
        }
    }
    Ok(())
}

fn record_commit_version(
    max_commit_version: &mut Option<CommitVersion>,
    commit_version: CommitVersion,
) {
    *max_commit_version =
        Some((*max_commit_version).map_or(commit_version, |current| current.max(commit_version)));
}

fn record_timestamp(
    timestamp_min: &mut Option<Timestamp>,
    timestamp_max: &mut Option<Timestamp>,
    commit_timestamp: Timestamp,
) {
    *timestamp_min =
        Some((*timestamp_min).map_or(commit_timestamp, |current| current.min(commit_timestamp)));
    *timestamp_max =
        Some((*timestamp_max).map_or(commit_timestamp, |current| current.max(commit_timestamp)));
}
