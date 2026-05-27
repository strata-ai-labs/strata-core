//! Branch facts and descriptor shells.

use super::{BranchRuntimeError, BranchRuntimeResult};
use crate::table::{TableIdentity, TableRuntimeFacts};
use std::collections::{BTreeMap, BTreeSet};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(transparent)]
pub(crate) struct BranchLevel(u8);

impl BranchLevel {
    pub(crate) const ZERO: Self = Self(0);

    pub(crate) const fn new(raw: u8) -> Self {
        Self(raw)
    }

    pub(crate) const fn raw(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchStateFacts {
    branch_id: BranchId,
    active_rows: u64,
    frozen_table_count: usize,
    owned_table_count: usize,
    inherited_layer_count: usize,
    max_commit_version: Option<CommitVersion>,
    timestamp_min: Option<Timestamp>,
    timestamp_max: Option<Timestamp>,
}

impl BranchStateFacts {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        branch_id: BranchId,
        active_rows: u64,
        frozen_table_count: usize,
        owned_table_count: usize,
        inherited_layer_count: usize,
        max_commit_version: Option<CommitVersion>,
        timestamp_min: Option<Timestamp>,
        timestamp_max: Option<Timestamp>,
    ) -> BranchRuntimeResult<Self> {
        match (timestamp_min, timestamp_max) {
            (Some(min), Some(max)) if min > max => {
                return Err(BranchRuntimeError::InvalidBranchState {
                    reason: "timestamp_min must not exceed timestamp_max",
                });
            }
            (Some(_), None) | (None, Some(_)) => {
                return Err(BranchRuntimeError::InvalidBranchState {
                    reason: "timestamp_min and timestamp_max must both be present or absent",
                });
            }
            (Some(_), Some(_)) | (None, None) => {}
        }
        if max_commit_version.is_some() != timestamp_min.is_some() {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "max commit version and timestamp range must both be present or absent",
            });
        }

        if active_rows == 0
            && frozen_table_count == 0
            && owned_table_count == 0
            && inherited_layer_count == 0
        {
            if max_commit_version.is_some() {
                return Err(BranchRuntimeError::InvalidBranchState {
                    reason: "empty branch facts cannot report a max commit version",
                });
            }
            if timestamp_min.is_some() || timestamp_max.is_some() {
                return Err(BranchRuntimeError::InvalidBranchState {
                    reason: "empty branch facts cannot report a timestamp range",
                });
            }
        }

        Ok(Self {
            branch_id,
            active_rows,
            frozen_table_count,
            owned_table_count,
            inherited_layer_count,
            max_commit_version,
            timestamp_min,
            timestamp_max,
        })
    }

    pub(crate) fn empty(branch_id: BranchId) -> Self {
        Self::new(branch_id, 0, 0, 0, 0, None, None, None).expect("empty branch facts are valid")
    }

    pub(crate) const fn branch_id(self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn active_rows(self) -> u64 {
        self.active_rows
    }

    pub(crate) const fn frozen_table_count(self) -> usize {
        self.frozen_table_count
    }

    pub(crate) const fn owned_table_count(self) -> usize {
        self.owned_table_count
    }

    pub(crate) const fn inherited_layer_count(self) -> usize {
        self.inherited_layer_count
    }

    pub(crate) const fn max_commit_version(self) -> Option<CommitVersion> {
        self.max_commit_version
    }

    pub(crate) const fn timestamp_min(self) -> Option<Timestamp> {
        self.timestamp_min
    }

    pub(crate) const fn timestamp_max(self) -> Option<Timestamp> {
        self.timestamp_max
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchTableDescriptor {
    identity: TableIdentity,
    facts: TableRuntimeFacts,
    level: BranchLevel,
}

impl BranchTableDescriptor {
    pub(crate) fn new(
        identity: TableIdentity,
        facts: TableRuntimeFacts,
        level: BranchLevel,
    ) -> BranchRuntimeResult<Self> {
        if facts.identity() != &identity {
            return Err(BranchRuntimeError::InvalidBranchState {
                reason: "table descriptor identity must match table facts identity",
            });
        }
        Ok(Self {
            identity,
            facts,
            level,
        })
    }

    pub(crate) const fn identity(&self) -> &TableIdentity {
        &self.identity
    }

    pub(crate) const fn facts(&self) -> &TableRuntimeFacts {
        &self.facts
    }

    pub(crate) const fn level(&self) -> BranchLevel {
        self.level
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum InheritedLayerStatus {
    Active,
    Materializing,
    Materialized,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct InheritedLayerDescriptor {
    source_branch_id: BranchId,
    fork_version: CommitVersion,
    status: InheritedLayerStatus,
    table_count: usize,
}

impl InheritedLayerDescriptor {
    pub(crate) const fn new(
        source_branch_id: BranchId,
        fork_version: CommitVersion,
        status: InheritedLayerStatus,
        table_count: usize,
    ) -> Self {
        Self {
            source_branch_id,
            fork_version,
            status,
            table_count,
        }
    }

    pub(crate) const fn source_branch_id(self) -> BranchId {
        self.source_branch_id
    }

    pub(crate) const fn fork_version(self) -> CommitVersion {
        self.fork_version
    }

    pub(crate) const fn status(self) -> InheritedLayerStatus {
        self.status
    }

    pub(crate) const fn table_count(self) -> usize {
        self.table_count
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BranchReachabilityFacts {
    branch_id: BranchId,
    owned_table_count: usize,
    inherited_table_count: usize,
}

impl BranchReachabilityFacts {
    pub(crate) const fn new(
        branch_id: BranchId,
        owned_table_count: usize,
        inherited_table_count: usize,
    ) -> Self {
        Self {
            branch_id,
            owned_table_count,
            inherited_table_count,
        }
    }

    pub(crate) const fn branch_id(self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn owned_table_count(self) -> usize {
        self.owned_table_count
    }

    pub(crate) const fn inherited_table_count(self) -> usize {
        self.inherited_table_count
    }

    pub(crate) const fn reachable_table_count(self) -> usize {
        self.owned_table_count + self.inherited_table_count
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BranchTableReferenceKind {
    Owned,
    Inherited {
        source_branch_id: BranchId,
        fork_version: CommitVersion,
        layer_index: usize,
    },
    MaterializingSource {
        source_branch_id: BranchId,
        fork_version: CommitVersion,
        layer_index: usize,
    },
    Replacement {
        source_branch_id: BranchId,
        fork_version: CommitVersion,
    },
}

impl BranchTableReferenceKind {
    const fn rank(self) -> u8 {
        match self {
            Self::Owned => 0,
            Self::Replacement { .. } => 1,
            Self::Inherited { .. } => 2,
            Self::MaterializingSource { .. } => 3,
        }
    }

    pub(crate) const fn source_branch_id(self) -> Option<BranchId> {
        match self {
            Self::Owned | Self::Replacement { .. } => None,
            Self::Inherited {
                source_branch_id, ..
            }
            | Self::MaterializingSource {
                source_branch_id, ..
            } => Some(source_branch_id),
        }
    }

    pub(crate) const fn is_owned_like(self) -> bool {
        matches!(self, Self::Owned | Self::Replacement { .. })
    }

    pub(crate) const fn is_inherited_like(self) -> bool {
        matches!(
            self,
            Self::Inherited { .. } | Self::MaterializingSource { .. }
        )
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchTableRef {
    table_identity: TableIdentity,
    owner_branch_id: BranchId,
    table_branch_id: BranchId,
    level: BranchLevel,
    table_index: usize,
    reference_kind: BranchTableReferenceKind,
}

impl BranchTableRef {
    pub(crate) fn owned(
        owner_branch_id: BranchId,
        level: BranchLevel,
        table_index: usize,
        table_identity: TableIdentity,
    ) -> BranchRuntimeResult<Self> {
        Self::new(
            table_identity,
            owner_branch_id,
            owner_branch_id,
            level,
            table_index,
            BranchTableReferenceKind::Owned,
        )
    }

    pub(crate) fn replacement(
        owner_branch_id: BranchId,
        source_branch_id: BranchId,
        fork_version: CommitVersion,
        level: BranchLevel,
        table_index: usize,
        table_identity: TableIdentity,
    ) -> BranchRuntimeResult<Self> {
        Self::new(
            table_identity,
            owner_branch_id,
            owner_branch_id,
            level,
            table_index,
            BranchTableReferenceKind::Replacement {
                source_branch_id,
                fork_version,
            },
        )
    }

    pub(crate) fn inherited(
        owner_branch_id: BranchId,
        source_branch_id: BranchId,
        fork_version: CommitVersion,
        layer_index: usize,
        level: BranchLevel,
        table_index: usize,
        table_identity: TableIdentity,
    ) -> BranchRuntimeResult<Self> {
        Self::new(
            table_identity,
            owner_branch_id,
            source_branch_id,
            level,
            table_index,
            BranchTableReferenceKind::Inherited {
                source_branch_id,
                fork_version,
                layer_index,
            },
        )
    }

    pub(crate) fn materializing_source(
        owner_branch_id: BranchId,
        source_branch_id: BranchId,
        fork_version: CommitVersion,
        layer_index: usize,
        level: BranchLevel,
        table_index: usize,
        table_identity: TableIdentity,
    ) -> BranchRuntimeResult<Self> {
        Self::new(
            table_identity,
            owner_branch_id,
            source_branch_id,
            level,
            table_index,
            BranchTableReferenceKind::MaterializingSource {
                source_branch_id,
                fork_version,
                layer_index,
            },
        )
    }

    fn new(
        table_identity: TableIdentity,
        owner_branch_id: BranchId,
        table_branch_id: BranchId,
        level: BranchLevel,
        table_index: usize,
        reference_kind: BranchTableReferenceKind,
    ) -> BranchRuntimeResult<Self> {
        if table_identity.as_str().is_empty() {
            return Err(BranchRuntimeError::InvalidReachability {
                reason: "table reference identity must not be empty",
            });
        }
        if reference_kind.is_owned_like() && table_branch_id != owner_branch_id {
            return Err(BranchRuntimeError::InvalidReachability {
                reason: "owned table reference branch ids must match",
            });
        }
        if let Some(source_branch_id) = reference_kind.source_branch_id() {
            if source_branch_id != table_branch_id {
                return Err(BranchRuntimeError::InvalidReachability {
                    reason: "inherited table reference source branch must match table branch",
                });
            }
            if source_branch_id == owner_branch_id {
                return Err(BranchRuntimeError::InvalidReachability {
                    reason: "inherited table reference source must differ from owner branch",
                });
            }
        }
        Ok(Self {
            table_identity,
            owner_branch_id,
            table_branch_id,
            level,
            table_index,
            reference_kind,
        })
    }

    pub(crate) const fn table_identity(&self) -> &TableIdentity {
        &self.table_identity
    }

    pub(crate) const fn owner_branch_id(&self) -> BranchId {
        self.owner_branch_id
    }

    pub(crate) const fn table_branch_id(&self) -> BranchId {
        self.table_branch_id
    }

    pub(crate) const fn level(&self) -> BranchLevel {
        self.level
    }

    pub(crate) const fn table_index(&self) -> usize {
        self.table_index
    }

    pub(crate) const fn reference_kind(&self) -> BranchTableReferenceKind {
        self.reference_kind
    }

    fn stable_sort_key(&self) -> BranchTableRefSortKey {
        let mut source_branch = [0; BranchId::BYTE_LEN];
        let mut fork_version = 0;
        let mut layer_index = 0;
        match self.reference_kind {
            BranchTableReferenceKind::Owned => {}
            BranchTableReferenceKind::Replacement {
                source_branch_id,
                fork_version: version,
            } => {
                source_branch = *source_branch_id.as_bytes();
                fork_version = version.as_u64();
            }
            BranchTableReferenceKind::Inherited {
                source_branch_id,
                fork_version: version,
                layer_index: index,
            }
            | BranchTableReferenceKind::MaterializingSource {
                source_branch_id,
                fork_version: version,
                layer_index: index,
            } => {
                source_branch = *source_branch_id.as_bytes();
                fork_version = version.as_u64();
                layer_index = index;
            }
        }
        BranchTableRefSortKey {
            table_identity: self.table_identity.as_str().to_owned(),
            reference_kind_rank: self.reference_kind.rank(),
            owner_branch: *self.owner_branch_id.as_bytes(),
            table_branch: *self.table_branch_id.as_bytes(),
            source_branch,
            fork_version,
            layer_index,
            level: self.level.raw(),
            table_index: self.table_index,
        }
    }
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct BranchTableRefSortKey {
    table_identity: String,
    reference_kind_rank: u8,
    owner_branch: [u8; BranchId::BYTE_LEN],
    table_branch: [u8; BranchId::BYTE_LEN],
    source_branch: [u8; BranchId::BYTE_LEN],
    fork_version: u64,
    layer_index: usize,
    level: u8,
    table_index: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchReachabilitySnapshot {
    branch_id: BranchId,
    table_refs: Vec<BranchTableRef>,
    facts: BranchReachabilityFacts,
    protected_table_count: usize,
}

impl BranchReachabilitySnapshot {
    pub(crate) fn new(
        branch_id: BranchId,
        mut table_refs: Vec<BranchTableRef>,
    ) -> BranchRuntimeResult<Self> {
        table_refs.sort_by_key(BranchTableRef::stable_sort_key);
        let mut seen_refs = BTreeSet::new();
        let mut protected_tables = BTreeSet::new();
        let mut owned_table_count = 0;
        let mut inherited_table_count = 0;
        for table_ref in &table_refs {
            if table_ref.owner_branch_id() != branch_id {
                return Err(BranchRuntimeError::InvalidReachability {
                    reason: "snapshot table reference owner branch must match snapshot branch",
                });
            }
            if !seen_refs.insert(table_ref.stable_sort_key()) {
                return Err(BranchRuntimeError::InvalidReachability {
                    reason: "snapshot table references must be unique",
                });
            }
            protected_tables.insert(table_ref.table_identity().as_str().to_owned());
            if table_ref.reference_kind().is_owned_like() {
                owned_table_count += 1;
            } else {
                inherited_table_count += 1;
            }
        }
        Ok(Self {
            branch_id,
            table_refs,
            facts: BranchReachabilityFacts::new(
                branch_id,
                owned_table_count,
                inherited_table_count,
            ),
            protected_table_count: protected_tables.len(),
        })
    }

    pub(crate) fn empty(branch_id: BranchId) -> Self {
        Self::new(branch_id, Vec::new()).expect("empty reachability snapshot is valid")
    }

    pub(crate) const fn branch_id(&self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn facts(&self) -> BranchReachabilityFacts {
        self.facts
    }

    pub(crate) const fn protected_table_count(&self) -> usize {
        self.protected_table_count
    }

    pub(crate) fn table_refs(&self) -> &[BranchTableRef] {
        &self.table_refs
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchTableProtection {
    table_identity: TableIdentity,
    table_refs: Vec<BranchTableRef>,
}

impl BranchTableProtection {
    fn new(table_identity: TableIdentity, mut table_refs: Vec<BranchTableRef>) -> Self {
        table_refs.sort_by_key(BranchTableRef::stable_sort_key);
        Self {
            table_identity,
            table_refs,
        }
    }

    pub(crate) const fn table_identity(&self) -> &TableIdentity {
        &self.table_identity
    }

    pub(crate) fn table_refs(&self) -> &[BranchTableRef] {
        &self.table_refs
    }

    pub(crate) fn reference_count(&self) -> usize {
        self.table_refs.len()
    }

    pub(crate) fn is_shared(&self) -> bool {
        self.reference_count() > 1
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchReachabilityAggregate {
    table_protections: Vec<BranchTableProtection>,
    branch_count: usize,
    reference_count: usize,
}

impl BranchReachabilityAggregate {
    pub(crate) fn from_snapshots(
        snapshots: &[BranchReachabilitySnapshot],
    ) -> BranchRuntimeResult<Self> {
        let mut branch_ids = BTreeSet::new();
        let mut refs_by_identity = BTreeMap::<String, (TableIdentity, Vec<BranchTableRef>)>::new();
        let mut reference_count = 0;
        for snapshot in snapshots {
            if !branch_ids.insert(*snapshot.branch_id().as_bytes()) {
                return Err(BranchRuntimeError::InvalidReachability {
                    reason: "aggregate snapshots must contain each branch at most once",
                });
            }
            for table_ref in snapshot.table_refs() {
                reference_count += 1;
                refs_by_identity
                    .entry(table_ref.table_identity().as_str().to_owned())
                    .or_insert_with(|| (table_ref.table_identity().clone(), Vec::new()))
                    .1
                    .push(table_ref.clone());
            }
        }
        let mut table_protections = refs_by_identity
            .into_values()
            .map(|(identity, refs)| BranchTableProtection::new(identity, refs))
            .collect::<Vec<_>>();
        table_protections.sort_by(|left, right| {
            left.table_identity()
                .as_str()
                .cmp(right.table_identity().as_str())
        });
        Ok(Self {
            table_protections,
            branch_count: snapshots.len(),
            reference_count,
        })
    }

    pub(crate) fn empty() -> Self {
        Self {
            table_protections: Vec::new(),
            branch_count: 0,
            reference_count: 0,
        }
    }

    pub(crate) fn table_protections(&self) -> &[BranchTableProtection] {
        &self.table_protections
    }

    pub(crate) const fn branch_count(&self) -> usize {
        self.branch_count
    }

    pub(crate) const fn reference_count(&self) -> usize {
        self.reference_count
    }

    pub(crate) fn table_count(&self) -> usize {
        self.table_protections.len()
    }

    pub(crate) fn reference_count_for(&self, table_identity: &TableIdentity) -> usize {
        self.table_protections
            .iter()
            .find(|protection| protection.table_identity() == table_identity)
            .map_or(0, BranchTableProtection::reference_count)
    }

    pub(crate) fn is_reachable(&self, table_identity: &TableIdentity) -> bool {
        self.reference_count_for(table_identity) != 0
    }

    pub(crate) fn is_shared(&self, table_identity: &TableIdentity) -> bool {
        self.table_protections
            .iter()
            .find(|protection| protection.table_identity() == table_identity)
            .is_some_and(BranchTableProtection::is_shared)
    }
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct SharedTableRegistry {
    refs_by_identity: BTreeMap<String, (TableIdentity, usize)>,
    registered_refs_by_branch:
        BTreeMap<[u8; BranchId::BYTE_LEN], BTreeMap<String, (TableIdentity, usize)>>,
}

impl SharedTableRegistry {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn rebuild_from_snapshots(
        snapshots: &[BranchReachabilitySnapshot],
    ) -> BranchRuntimeResult<Self> {
        let mut registry = Self::new();
        for snapshot in snapshots {
            registry.register_snapshot(snapshot)?;
        }
        Ok(registry)
    }

    pub(crate) fn register_snapshot(
        &mut self,
        snapshot: &BranchReachabilitySnapshot,
    ) -> BranchRuntimeResult<()> {
        let branch_key = *snapshot.branch_id().as_bytes();
        if self.registered_refs_by_branch.contains_key(&branch_key) {
            return Err(BranchRuntimeError::InvalidReachability {
                reason: "runtime table reference snapshot is already registered",
            });
        }
        let required = snapshot_ref_counts(snapshot);
        for (table_identity, count) in required.values() {
            for _ in 0..*count {
                self.increment(table_identity.clone());
            }
        }
        self.registered_refs_by_branch.insert(branch_key, required);
        Ok(())
    }

    pub(crate) fn unregister_snapshot(
        &mut self,
        snapshot: &BranchReachabilitySnapshot,
    ) -> BranchRuntimeResult<()> {
        let branch_key = *snapshot.branch_id().as_bytes();
        let required = snapshot_ref_counts(snapshot);
        let Some(registered) = self.registered_refs_by_branch.get(&branch_key) else {
            return Err(BranchRuntimeError::InvalidReachability {
                reason: "runtime table reference snapshot must be registered before release",
            });
        };
        if registered != &required {
            return Err(BranchRuntimeError::InvalidReachability {
                reason: "runtime table reference release must match the registered snapshot",
            });
        }
        for (table_identity, required_count) in required.values() {
            for _ in 0..*required_count {
                self.decrement(table_identity)?;
            }
        }
        self.registered_refs_by_branch.remove(&branch_key);
        Ok(())
    }

    pub(crate) fn replace_snapshot(
        &mut self,
        snapshot: &BranchReachabilitySnapshot,
    ) -> BranchRuntimeResult<()> {
        let branch_key = *snapshot.branch_id().as_bytes();
        let Some(previous) = self.registered_refs_by_branch.get(&branch_key).cloned() else {
            return Err(BranchRuntimeError::InvalidReachability {
                reason: "runtime table reference snapshot must be registered before replacement",
            });
        };
        for (table_identity, previous_count) in previous.values() {
            if self.reference_count(table_identity) < *previous_count {
                return Err(BranchRuntimeError::InvalidReachability {
                    reason: "runtime table reference replacement must match registered references",
                });
            }
        }

        let replacement = snapshot_ref_counts(snapshot);
        for (table_identity, previous_count) in previous.values() {
            for _ in 0..*previous_count {
                self.decrement(table_identity)?;
            }
        }
        for (table_identity, replacement_count) in replacement.values() {
            for _ in 0..*replacement_count {
                self.increment(table_identity.clone());
            }
        }
        self.registered_refs_by_branch
            .insert(branch_key, replacement);
        Ok(())
    }

    pub(crate) fn reference_count(&self, table_identity: &TableIdentity) -> usize {
        self.refs_by_identity
            .get(table_identity.as_str())
            .map_or(0, |(_, count)| *count)
    }

    pub(crate) fn is_runtime_referenced(&self, table_identity: &TableIdentity) -> bool {
        self.reference_count(table_identity) != 0
    }

    pub(crate) fn table_count(&self) -> usize {
        self.refs_by_identity.len()
    }

    pub(crate) fn clear(&mut self) {
        self.refs_by_identity.clear();
        self.registered_refs_by_branch.clear();
    }

    fn increment(&mut self, table_identity: TableIdentity) {
        self.refs_by_identity
            .entry(table_identity.as_str().to_owned())
            .and_modify(|(_, count)| *count = count.saturating_add(1))
            .or_insert((table_identity, 1));
    }

    fn decrement(&mut self, table_identity: &TableIdentity) -> BranchRuntimeResult<()> {
        let Some((_, count)) = self.refs_by_identity.get_mut(table_identity.as_str()) else {
            return Err(BranchRuntimeError::InvalidReachability {
                reason: "runtime table reference release must match a registered reference",
            });
        };
        if *count == 0 {
            return Err(BranchRuntimeError::InvalidReachability {
                reason: "runtime table reference count must not underflow",
            });
        }
        *count -= 1;
        if *count == 0 {
            self.refs_by_identity.remove(table_identity.as_str());
        }
        Ok(())
    }
}

fn snapshot_ref_counts(
    snapshot: &BranchReachabilitySnapshot,
) -> BTreeMap<String, (TableIdentity, usize)> {
    let mut counts = BTreeMap::<String, (TableIdentity, usize)>::new();
    for table_ref in snapshot.table_refs() {
        counts
            .entry(table_ref.table_identity().as_str().to_owned())
            .and_modify(|(_, count)| *count = count.saturating_add(1))
            .or_insert((table_ref.table_identity().clone(), 1));
    }
    counts
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum BranchProtectionReason {
    StillReachable,
    RuntimeReferenced,
    RegistryDisagreement,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchProtectedTable {
    table_identity: TableIdentity,
    reason: BranchProtectionReason,
}

impl BranchProtectedTable {
    fn new(table_identity: TableIdentity, reason: BranchProtectionReason) -> Self {
        Self {
            table_identity,
            reason,
        }
    }

    pub(crate) const fn table_identity(&self) -> &TableIdentity {
        &self.table_identity
    }

    pub(crate) const fn reason(&self) -> BranchProtectionReason {
        self.reason
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BranchReleasePlan {
    released_branch_id: BranchId,
    removed_refs: Vec<BranchTableRef>,
    releasable_tables: Vec<TableIdentity>,
    protected_tables: Vec<BranchProtectedTable>,
}

impl BranchReleasePlan {
    pub(crate) fn from_removed_refs(
        released_branch_id: BranchId,
        mut removed_refs: Vec<BranchTableRef>,
        aggregate_after_release: &BranchReachabilityAggregate,
        runtime_registry: Option<&SharedTableRegistry>,
    ) -> BranchRuntimeResult<Self> {
        removed_refs.sort_by_key(BranchTableRef::stable_sort_key);
        let mut removed_tables = BTreeMap::<String, TableIdentity>::new();
        for table_ref in &removed_refs {
            if table_ref.owner_branch_id() != released_branch_id {
                return Err(BranchRuntimeError::InvalidReachability {
                    reason: "release plan removed refs must match released branch",
                });
            }
            removed_tables
                .entry(table_ref.table_identity().as_str().to_owned())
                .or_insert_with(|| table_ref.table_identity().clone());
        }

        let mut releasable_tables = Vec::new();
        let mut protected_tables = Vec::new();
        for table_identity in removed_tables.into_values() {
            let aggregate_count = aggregate_after_release.reference_count_for(&table_identity);
            let registry_count =
                runtime_registry.map(|registry| registry.reference_count(&table_identity));
            let protection = match (aggregate_count, registry_count) {
                (0, Some(count)) if count > 0 => Some(BranchProtectionReason::RuntimeReferenced),
                (0, _) => None,
                (count, Some(registry_count)) if count != registry_count => {
                    Some(BranchProtectionReason::RegistryDisagreement)
                }
                (_, _) => Some(BranchProtectionReason::StillReachable),
            };
            if let Some(reason) = protection {
                protected_tables.push(BranchProtectedTable::new(table_identity, reason));
            } else {
                releasable_tables.push(table_identity);
            }
        }
        releasable_tables.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        protected_tables.sort_by(|left, right| {
            left.table_identity()
                .as_str()
                .cmp(right.table_identity().as_str())
                .then_with(|| left.reason().rank().cmp(&right.reason().rank()))
        });
        Ok(Self {
            released_branch_id,
            removed_refs,
            releasable_tables,
            protected_tables,
        })
    }

    /// Reconstruct a release plan from its persisted audit-trail form
    /// (branch_id + sorted releasable table identities). `removed_refs`
    /// and `protected_tables` are intentionally empty — the persisted
    /// form omits them because the drain path only consumes
    /// `releasable_tables`, and the manifest-driven retention pass
    /// recomputes reachability on the next run.
    pub(crate) fn from_releasable_tables(
        released_branch_id: BranchId,
        mut releasable_tables: Vec<TableIdentity>,
    ) -> Self {
        releasable_tables.sort_by(|left, right| left.as_str().cmp(right.as_str()));
        Self {
            released_branch_id,
            removed_refs: Vec::new(),
            releasable_tables,
            protected_tables: Vec::new(),
        }
    }

    pub(crate) const fn released_branch_id(&self) -> BranchId {
        self.released_branch_id
    }

    pub(crate) fn removed_refs(&self) -> &[BranchTableRef] {
        &self.removed_refs
    }

    pub(crate) fn releasable_tables(&self) -> &[TableIdentity] {
        &self.releasable_tables
    }

    pub(crate) fn protected_tables(&self) -> &[BranchProtectedTable] {
        &self.protected_tables
    }
}

impl BranchProtectionReason {
    const fn rank(self) -> u8 {
        match self {
            Self::StillReachable => 0,
            Self::RuntimeReferenced => 1,
            Self::RegistryDisagreement => 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct BranchRuntimeStats {
    latest_reads: u64,
    bounded_reads: u64,
    history_reads: u64,
    inherited_layers_examined: u64,
}

impl BranchRuntimeStats {
    pub(crate) const fn new(
        latest_reads: u64,
        bounded_reads: u64,
        history_reads: u64,
        inherited_layers_examined: u64,
    ) -> Self {
        Self {
            latest_reads,
            bounded_reads,
            history_reads,
            inherited_layers_examined,
        }
    }

    pub(crate) const fn latest_reads(self) -> u64 {
        self.latest_reads
    }

    pub(crate) const fn bounded_reads(self) -> u64 {
        self.bounded_reads
    }

    pub(crate) const fn history_reads(self) -> u64 {
        self.history_reads
    }

    pub(crate) const fn inherited_layers_examined(self) -> u64 {
        self.inherited_layers_examined
    }
}
