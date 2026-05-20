//! Branch read-bound, selected-row, and own-branch read-view vocabulary.

use super::{
    rewrite_physical_key_branch, rewrite_row_branch, BranchLevel, BranchRuntimeError,
    BranchRuntimeResult, BranchStateFacts, BranchTableDescriptor, BranchTimestampHistorySource,
    InheritedLayerDescriptor, InheritedLayerStatus,
};
use crate::row::{PhysicalKey, StorageRow, StorageSpaceId};
use crate::table::{
    FrozenTable, ImmutableTableReader, MutableTable, TableInternalKeyBytes, TablePhysicalKeyBytes,
    TableRow, TableRuntimeFacts,
};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
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

    pub(crate) const fn matches_row(self, row: &StorageRow) -> BranchRowBoundMatch {
        let version_in_bound = match self.max_commit_version {
            Some(version) => row.commit_version().as_u64() <= version.as_u64(),
            None => true,
        };
        let timestamp_in_bound = match self.max_commit_timestamp {
            Some(timestamp) => row.commit_timestamp().as_micros() <= timestamp.as_micros(),
            None => true,
        };
        BranchRowBoundMatch::new(version_in_bound, timestamp_in_bound)
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchRowBoundMatch {
    version_in_bound: bool,
    timestamp_in_bound: bool,
}

impl BranchRowBoundMatch {
    pub(crate) const fn new(version_in_bound: bool, timestamp_in_bound: bool) -> Self {
        Self {
            version_in_bound,
            timestamp_in_bound,
        }
    }

    pub(crate) const fn version_in_bound(self) -> bool {
        self.version_in_bound
    }

    pub(crate) const fn timestamp_in_bound(self) -> bool {
        self.timestamp_in_bound
    }

    pub(crate) const fn matches_effective_bound(self) -> bool {
        self.version_in_bound && self.timestamp_in_bound
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchRowCandidateFacts {
    source: BranchRowSource,
    physical_key: PhysicalKey,
    commit_version: CommitVersion,
    commit_timestamp: Timestamp,
    expires_at: Timestamp,
    is_tombstone: bool,
    bound_match: BranchRowBoundMatch,
}

impl BranchRowCandidateFacts {
    pub(crate) fn from_row(
        row: &StorageRow,
        source: BranchRowSource,
        effective_bound: BranchEffectiveReadBound,
    ) -> Self {
        Self {
            source,
            physical_key: row.physical_key().clone(),
            commit_version: row.commit_version(),
            commit_timestamp: row.commit_timestamp(),
            expires_at: row.expires_at(),
            is_tombstone: row.is_tombstone(),
            bound_match: effective_bound.matches_row(row),
        }
    }

    pub(crate) const fn source(&self) -> BranchRowSource {
        self.source
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

    pub(crate) const fn is_tombstone(&self) -> bool {
        self.is_tombstone
    }

    pub(crate) const fn bound_match(&self) -> BranchRowBoundMatch {
        self.bound_match
    }
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
        match self.status() {
            InheritedLayerStatus::Active | InheritedLayerStatus::Materializing => {
                Ok(Some(Self::new(
                    InheritedLayerDescriptor::new(
                        self.source_branch_id(),
                        self.fork_version(),
                        InheritedLayerStatus::Active,
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
            self.point_candidates(key, bound)?,
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
            if before_version
                .is_some_and(|version| candidate.row.commit_version().as_u64() >= version.as_u64())
            {
                continue;
            }
            if !options.includes_tombstones() && candidate.row.is_tombstone() {
                continue;
            }
            history.push(candidate.into_history_row());
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

    fn point_candidates(
        &self,
        key: &PhysicalKey,
        bound: BranchReadBound,
    ) -> BranchRuntimeResult<Vec<CandidateRow>> {
        let mut rows = Vec::new();
        for row in self.active.iter().filter(|row| row.physical_key() == key) {
            rows.push(CandidateRow::new(
                row.row().clone(),
                BranchRowSource::Active,
            ));
        }
        for (index, table) in self.frozen.iter().enumerate() {
            for row in table.iter().filter(|row| row.physical_key() == key) {
                rows.push(CandidateRow::new(
                    row.row().clone(),
                    BranchRowSource::Frozen { index },
                ));
            }
        }
        for tables in &self.owned_levels {
            for (table_index, table) in tables.iter().enumerate() {
                for row in table.rows().iter().filter(|row| row.physical_key() == key) {
                    rows.push(CandidateRow::new(
                        row.row().clone(),
                        BranchRowSource::OwnedTable {
                            level: table.level(),
                            table_index,
                        },
                    ));
                }
            }
        }
        self.collect_inherited_point_candidates(key, bound, &mut rows)?;
        Ok(rows)
    }

    fn collect_matching_scan_candidates(
        &self,
        bounds: &BranchScanBounds,
        bound: BranchReadBound,
        grouped: &mut BTreeMap<TablePhysicalKeyBytes, Vec<CandidateRow>>,
    ) -> BranchRuntimeResult<()> {
        for row in self
            .active
            .iter()
            .filter(|row| bounds.contains(row.physical_key()))
        {
            grouped
                .entry(TablePhysicalKeyBytes::from_row(row.row()))
                .or_default()
                .push(CandidateRow::new(
                    row.row().clone(),
                    BranchRowSource::Active,
                ));
        }
        for (index, table) in self.frozen.iter().enumerate() {
            for row in table
                .iter()
                .filter(|row| bounds.contains(row.physical_key()))
            {
                grouped
                    .entry(TablePhysicalKeyBytes::from_row(row.row()))
                    .or_default()
                    .push(CandidateRow::new(
                        row.row().clone(),
                        BranchRowSource::Frozen { index },
                    ));
            }
        }
        for tables in &self.owned_levels {
            for (table_index, table) in tables.iter().enumerate() {
                for row in table
                    .rows()
                    .iter()
                    .filter(|row| bounds.contains(row.physical_key()))
                {
                    grouped
                        .entry(TablePhysicalKeyBytes::from_row(row.row()))
                        .or_default()
                        .push(CandidateRow::new(
                            row.row().clone(),
                            BranchRowSource::OwnedTable {
                                level: table.level(),
                                table_index,
                            },
                        ));
                }
            }
        }
        self.collect_inherited_scan_candidates(bounds, bound, grouped)?;
        Ok(())
    }

    fn collect_inherited_point_candidates(
        &self,
        key: &PhysicalKey,
        bound: BranchReadBound,
        rows: &mut Vec<CandidateRow>,
    ) -> BranchRuntimeResult<()> {
        let mut seen_table_identities = BTreeSet::<&str>::new();
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
                if !seen_table_identities.insert(table.descriptor().identity().as_str()) {
                    continue;
                }
                for row in table.rows().iter().filter(|row| {
                    row.physical_key() == &source_key
                        && inherited_bound
                            .matches_row(row.row())
                            .matches_effective_bound()
                }) {
                    rows.push(CandidateRow::new(
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
        Ok(())
    }

    fn collect_inherited_scan_candidates(
        &self,
        bounds: &BranchScanBounds,
        bound: BranchReadBound,
        grouped: &mut BTreeMap<TablePhysicalKeyBytes, Vec<CandidateRow>>,
    ) -> BranchRuntimeResult<()> {
        let mut seen_table_identities = BTreeSet::<&str>::new();
        for (layer_index, layer) in self.inherited_layers.iter().enumerate() {
            if !layer.is_readable() {
                continue;
            }
            let inherited_bound =
                BranchEffectiveReadBound::for_inherited_layer(bound, layer.fork_version());
            for table in layer.owned_levels().iter().flatten() {
                if !seen_table_identities.insert(table.descriptor().identity().as_str()) {
                    continue;
                }
                for row in table.rows().iter().filter(|row| {
                    inherited_bound
                        .matches_row(row.row())
                        .matches_effective_bound()
                }) {
                    let rewritten =
                        rewrite_row_branch(row.row(), layer.source_branch_id(), self.branch_id)
                            .map_err(|_| BranchRuntimeError::InvalidInheritedLayer {
                                reason: "inherited row branch rewrite failed",
                            })?;
                    if bounds.contains(rewritten.physical_key()) {
                        grouped
                            .entry(TablePhysicalKeyBytes::from_row(&rewritten))
                            .or_default()
                            .push(CandidateRow::new(
                                rewritten,
                                BranchRowSource::Inherited {
                                    source_branch_id: layer.source_branch_id(),
                                    layer_index,
                                },
                            ));
                    }
                }
            }
        }
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

#[derive(Clone, Debug, Eq, PartialEq)]
struct CandidateRow {
    row: StorageRow,
    source: BranchRowSource,
}

impl CandidateRow {
    fn new(row: StorageRow, source: BranchRowSource) -> Self {
        Self { row, source }
    }

    fn into_visible_row(self, read_timestamp: Option<Timestamp>) -> Option<BranchVisibleRow> {
        if self.row.is_tombstone() || row_is_expired_at(&self.row, read_timestamp) {
            None
        } else {
            Some(BranchVisibleRow::new(self.row, self.source))
        }
    }

    fn into_history_row(self) -> BranchHistoryRow {
        BranchHistoryRow::new(self.row, self.source)
    }
}

fn effective_own_read_bound(bound: BranchReadBound) -> BranchEffectiveReadBound {
    BranchEffectiveReadBound::for_own_branch(bound)
}

fn select_visible_row(
    mut candidates: Vec<CandidateRow>,
    effective_bound: BranchEffectiveReadBound,
) -> Option<BranchVisibleRow> {
    sort_candidates_newest_first(&mut candidates);
    candidates
        .into_iter()
        .find(|candidate| {
            effective_bound
                .matches_row(&candidate.row)
                .matches_effective_bound()
        })?
        .into_visible_row(effective_bound.max_commit_timestamp())
}

fn row_is_expired_at(row: &StorageRow, read_timestamp: Option<Timestamp>) -> bool {
    read_timestamp.is_some_and(|timestamp| {
        !row.is_tombstone() && row.expires_at() != Timestamp::EPOCH && row.expires_at() <= timestamp
    })
}

fn sort_candidates_newest_first(candidates: &mut [CandidateRow]) {
    candidates.sort_by(|left, right| {
        right
            .row
            .commit_version()
            .as_u64()
            .cmp(&left.row.commit_version().as_u64())
            .then_with(|| source_order_cmp(left.source, right.source))
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
                layer_index: left, ..
            },
            BranchRowSource::Inherited {
                layer_index: right, ..
            },
        ) => left.cmp(&right),
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

    let mut observed = ObservedReadViewFacts::default();
    for row in active.iter() {
        observed.record(branch_id, row.row())?;
    }
    for table in frozen {
        for row in table.iter() {
            observed.record(branch_id, row.row())?;
        }
    }
    for tables in owned_levels {
        for table in tables {
            for row in table.rows() {
                observed.record(branch_id, row.row())?;
            }
        }
    }
    for layer in inherited_layers {
        observed.record_inherited_layer(layer)?;
    }
    observed.matches(facts)
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

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ObservedReadViewFacts {
    max_commit_version: Option<CommitVersion>,
    timestamp_min: Option<Timestamp>,
    timestamp_max: Option<Timestamp>,
}

impl ObservedReadViewFacts {
    fn record(&mut self, branch_id: BranchId, row: &StorageRow) -> BranchRuntimeResult<()> {
        if row.physical_key().branch_id() != branch_id {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "read view source rows must match the view branch",
            });
        }
        self.record_commit_version(row.commit_version());
        self.record_timestamp(row.commit_timestamp());
        Ok(())
    }

    fn record_inherited_layer(&mut self, layer: &BranchInheritedLayer) -> BranchRuntimeResult<()> {
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
                    self.record_commit_version(row.commit_version());
                    self.record_timestamp(row.commit_timestamp());
                }
            }
        }
        Ok(())
    }

    fn record_commit_version(&mut self, commit_version: CommitVersion) {
        self.max_commit_version = Some(
            self.max_commit_version
                .map_or(commit_version, |current| current.max(commit_version)),
        );
    }

    fn record_timestamp(&mut self, commit_timestamp: Timestamp) {
        self.timestamp_min = Some(
            self.timestamp_min
                .map_or(commit_timestamp, |current| current.min(commit_timestamp)),
        );
        self.timestamp_max = Some(
            self.timestamp_max
                .map_or(commit_timestamp, |current| current.max(commit_timestamp)),
        );
    }

    fn matches(self, facts: BranchStateFacts) -> BranchRuntimeResult<()> {
        if self.max_commit_version != facts.max_commit_version()
            || self.timestamp_min != facts.timestamp_min()
            || self.timestamp_max != facts.timestamp_max()
        {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "read view source facts must match branch facts",
            });
        }
        Ok(())
    }
}
