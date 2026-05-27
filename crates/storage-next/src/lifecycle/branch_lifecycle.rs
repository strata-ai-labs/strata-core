//! Storage-internal branch lifecycle catalog.

use super::{LifecycleError, LifecycleLowerLayer, LifecycleResult};
use crate::branch::{
    install_snapshot_rows_into_branches, BranchLocalState, BranchReachabilityAggregate,
    BranchReachabilitySnapshot, BranchReadView, BranchReleasePlan, BranchRuntimeConfig,
    BranchSnapshotInstallRequest, BranchTableRef, BranchTableReferenceKind,
    BranchTimestampCoverage,
};
use crate::commit::{
    CommitBranchDescriptor, CommitBranchGeneration, CommitBranchGenerationGuard,
    CommitBranchRegistry, CommitBranchState, CommitRuntimeError,
};
use std::collections::{BTreeMap, BTreeSet};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LifecycleBranchStatus {
    Active,
    Clearing,
    Deleting,
    Deleted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleBranchParent {
    source_branch_id: BranchId,
    fork_version: CommitVersion,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleBranchDescriptor {
    branch_id: BranchId,
    generation: CommitBranchGeneration,
    status: LifecycleBranchStatus,
    parent: Option<LifecycleBranchParent>,
    created_at: Option<CommitVersion>,
    deleted_at: Option<CommitVersion>,
    state_revision: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleBranchCreateOutcome {
    descriptor: LifecycleBranchDescriptor,
    branch_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleBranchForkOutcome {
    descriptor: LifecycleBranchDescriptor,
    source_branch_id: BranchId,
    fork_version: CommitVersion,
    inherited_layer_count: usize,
    inherited_table_count: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleBranchClearOutcome {
    descriptor: LifecycleBranchDescriptor,
    release_plan: BranchReleasePlan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleBranchDeleteOutcome {
    descriptor: LifecycleBranchDescriptor,
    release_plan: BranchReleasePlan,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecyclePinnedBranchReachability {
    pin_id: u64,
    descriptor: LifecycleBranchDescriptor,
    snapshot: BranchReachabilitySnapshot,
}

#[derive(Clone, Debug)]
struct LifecycleBranchEntry {
    descriptor: LifecycleBranchDescriptor,
    state: Option<BranchLocalState>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LifecyclePinnedBranchReachabilityRecord {
    pin_id: u64,
    descriptor: LifecycleBranchDescriptor,
    snapshot: BranchReachabilitySnapshot,
}

#[derive(Clone, Debug)]
pub(crate) struct LifecycleBranchCatalog {
    branch_config: BranchRuntimeConfig,
    entries: Vec<LifecycleBranchEntry>,
    registry: CommitBranchRegistry,
    pinned_snapshots: Vec<LifecyclePinnedBranchReachabilityRecord>,
    next_pin_id: u64,
}

impl LifecycleBranchParent {
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

impl LifecycleBranchDescriptor {
    pub(crate) fn active(
        branch_id: BranchId,
        generation: CommitBranchGeneration,
        created_at: Option<CommitVersion>,
    ) -> Self {
        Self {
            branch_id,
            generation,
            status: LifecycleBranchStatus::Active,
            parent: None,
            created_at,
            deleted_at: None,
            state_revision: 0,
        }
    }

    fn with_status(mut self, status: LifecycleBranchStatus) -> Self {
        self.status = status;
        self
    }

    fn with_parent(mut self, parent: LifecycleBranchParent) -> Self {
        self.parent = Some(parent);
        self
    }

    fn with_deleted_at(mut self, deleted_at: Option<CommitVersion>) -> Self {
        self.deleted_at = deleted_at;
        self
    }

    fn with_next_revision(mut self) -> Self {
        self.state_revision = self.state_revision.saturating_add(1);
        self
    }

    pub(crate) const fn branch_id(self) -> BranchId {
        self.branch_id
    }

    pub(crate) const fn generation(self) -> CommitBranchGeneration {
        self.generation
    }

    pub(crate) const fn status(self) -> LifecycleBranchStatus {
        self.status
    }

    pub(crate) const fn parent(self) -> Option<LifecycleBranchParent> {
        self.parent
    }

    pub(crate) const fn created_at(self) -> Option<CommitVersion> {
        self.created_at
    }

    pub(crate) const fn deleted_at(self) -> Option<CommitVersion> {
        self.deleted_at
    }

    pub(crate) const fn state_revision(self) -> u64 {
        self.state_revision
    }
}

impl LifecycleBranchCreateOutcome {
    pub(crate) const fn descriptor(&self) -> LifecycleBranchDescriptor {
        self.descriptor
    }

    pub(crate) const fn branch_count(&self) -> usize {
        self.branch_count
    }
}

impl LifecycleBranchForkOutcome {
    pub(crate) const fn descriptor(&self) -> LifecycleBranchDescriptor {
        self.descriptor
    }

    pub(crate) const fn source_branch_id(&self) -> BranchId {
        self.source_branch_id
    }

    pub(crate) const fn fork_version(&self) -> CommitVersion {
        self.fork_version
    }

    pub(crate) const fn inherited_layer_count(&self) -> usize {
        self.inherited_layer_count
    }

    pub(crate) const fn inherited_table_count(&self) -> usize {
        self.inherited_table_count
    }
}

impl LifecycleBranchClearOutcome {
    pub(crate) const fn descriptor(&self) -> LifecycleBranchDescriptor {
        self.descriptor
    }

    pub(crate) const fn release_plan(&self) -> &BranchReleasePlan {
        &self.release_plan
    }
}

impl LifecycleBranchDeleteOutcome {
    pub(crate) const fn descriptor(&self) -> LifecycleBranchDescriptor {
        self.descriptor
    }

    pub(crate) const fn release_plan(&self) -> &BranchReleasePlan {
        &self.release_plan
    }
}

impl LifecyclePinnedBranchReachability {
    pub(crate) const fn descriptor(&self) -> LifecycleBranchDescriptor {
        self.descriptor
    }

    pub(crate) fn table_refs(&self) -> &[BranchTableRef] {
        self.snapshot.table_refs()
    }
}

impl LifecycleBranchCatalog {
    pub(crate) fn new(branch_config: BranchRuntimeConfig) -> LifecycleResult<Self> {
        branch_config.validate().map_err(branch_error)?;
        Ok(Self {
            branch_config,
            entries: Vec::new(),
            registry: CommitBranchRegistry::new(),
            pinned_snapshots: Vec::new(),
            next_pin_id: 1,
        })
    }

    pub(crate) fn with_initial_branch(
        branch_config: BranchRuntimeConfig,
        branch_id: BranchId,
        generation: CommitBranchGeneration,
        created_at: Option<CommitVersion>,
    ) -> LifecycleResult<Self> {
        let mut catalog = Self::new(branch_config)?;
        catalog.create_branch(branch_id, generation, created_at)?;
        Ok(catalog)
    }

    pub(crate) fn with_existing_branch(
        state: &BranchLocalState,
        generation: CommitBranchGeneration,
        created_at: Option<CommitVersion>,
    ) -> LifecycleResult<Self> {
        let branch_config = state.config();
        let branch_id = state.branch_id();
        let mut catalog = Self::new(branch_config)?;
        catalog.create_branch(branch_id, generation, created_at)?;
        catalog.seed_active_branch_state(state)?;
        Ok(catalog)
    }

    pub(crate) fn create_branch(
        &mut self,
        branch_id: BranchId,
        generation: CommitBranchGeneration,
        created_at: Option<CommitVersion>,
    ) -> LifecycleResult<LifecycleBranchCreateOutcome> {
        match self.find_entry_index(branch_id) {
            Some(index)
                if self.entries[index].descriptor.status() == LifecycleBranchStatus::Deleted =>
            {
                self.recreate_deleted_branch(branch_id, generation, created_at)
            }
            Some(_) => Err(LifecycleError::BranchAlreadyExists { branch_id }),
            None => {
                let state =
                    BranchLocalState::new(branch_id, self.branch_config).map_err(branch_error)?;
                let descriptor =
                    LifecycleBranchDescriptor::active(branch_id, generation, created_at);
                self.registry
                    .register_active(branch_id, generation)
                    .map_err(commit_error)?;
                self.entries.push(LifecycleBranchEntry {
                    descriptor,
                    state: Some(state),
                });
                self.sort_entries();
                Ok(LifecycleBranchCreateOutcome {
                    descriptor,
                    branch_count: self.entries.len(),
                })
            }
        }
    }

    /// Set the parent descriptor for a branch during recovery. The
    /// `replay_branch_catalog_manifest` path uses `create_branch` to
    /// install non-seeded entries, which initializes them without a
    /// parent; this helper attaches the parent metadata recovered from
    /// the `BranchCatalogManifest`. Production fork paths set parent on
    /// initial install via `install_new_branch_state`, so this method is
    /// recovery-only.
    pub(crate) fn set_parent_for_recovery(
        &mut self,
        branch_id: BranchId,
        parent: LifecycleBranchParent,
    ) -> LifecycleResult<()> {
        let index = self.entry_index(branch_id)?;
        self.entries[index].descriptor = self.entries[index].descriptor.with_parent(parent);
        Ok(())
    }

    pub(crate) fn recreate_deleted_branch(
        &mut self,
        branch_id: BranchId,
        generation: CommitBranchGeneration,
        created_at: Option<CommitVersion>,
    ) -> LifecycleResult<LifecycleBranchCreateOutcome> {
        let index = self.entry_index(branch_id)?;
        let current = self.entries[index].descriptor;
        if current.status() != LifecycleBranchStatus::Deleted {
            return Err(LifecycleError::BranchAlreadyExists { branch_id });
        }
        if current.generation().get() == u64::MAX {
            return Err(LifecycleError::BranchGenerationExhausted {
                branch_id,
                generation: current.generation().get(),
            });
        }
        if generation <= current.generation() {
            return Err(LifecycleError::BranchGenerationMismatch {
                branch_id,
                expected: current.generation().get().saturating_add(1),
                actual: generation.get(),
            });
        }

        let state = BranchLocalState::new(branch_id, self.branch_config).map_err(branch_error)?;
        let descriptor = LifecycleBranchDescriptor::active(branch_id, generation, created_at)
            .with_next_revision();
        self.registry
            .recreate_active(branch_id, generation)
            .map_err(commit_error)?;
        self.entries[index] = LifecycleBranchEntry {
            descriptor,
            state: Some(state),
        };
        self.sort_entries();
        Ok(LifecycleBranchCreateOutcome {
            descriptor,
            branch_count: self.entries.len(),
        })
    }

    pub(crate) fn list_branches(&self, include_deleted: bool) -> Vec<LifecycleBranchDescriptor> {
        let mut descriptors = self
            .entries
            .iter()
            .filter(|entry| {
                include_deleted || entry.descriptor.status() != LifecycleBranchStatus::Deleted
            })
            .map(|entry| entry.descriptor)
            .collect::<Vec<_>>();
        descriptors.sort_by_key(|descriptor| *descriptor.branch_id().as_bytes());
        descriptors
    }

    /// Build a deterministic snapshot of catalog descriptors for durable
    /// publication. Entries are sorted by `branch_id` byte order (mirroring
    /// the catalog's internal ordering). Transient states (`Clearing` /
    /// `Deleting`) are not currently observable through the synchronous
    /// catalog API and therefore not represented.
    pub(crate) fn durable_entries(
        &self,
    ) -> Result<Vec<crate::format::BranchCatalogEntry>, crate::format::FormatError> {
        use crate::format::{BranchCatalogEntry, BranchCatalogParent, BranchCatalogStatus};
        let mut entries = Vec::with_capacity(self.entries.len());
        for entry in &self.entries {
            let descriptor = entry.descriptor;
            let status = match descriptor.status() {
                LifecycleBranchStatus::Active
                | LifecycleBranchStatus::Clearing
                | LifecycleBranchStatus::Deleting => BranchCatalogStatus::Active,
                LifecycleBranchStatus::Deleted => BranchCatalogStatus::Deleted,
            };
            let mut durable = BranchCatalogEntry::new(
                descriptor.branch_id(),
                descriptor.generation().get(),
                status,
            )?
            .with_state_revision(descriptor.state_revision());
            if let Some(parent) = descriptor.parent() {
                durable = durable.with_parent(BranchCatalogParent::new(
                    parent.source_branch_id(),
                    parent.fork_version().as_u64(),
                ));
            }
            if let Some(created) = descriptor.created_at() {
                durable = durable.with_created_at(created.as_u64())?;
            }
            if let Some(deleted) = descriptor.deleted_at() {
                durable = durable.with_deleted_at(deleted.as_u64())?;
            }
            entries.push(durable);
        }
        Ok(entries)
    }

    pub(crate) fn lookup(&self, branch_id: BranchId) -> LifecycleResult<LifecycleBranchDescriptor> {
        Ok(self.entry(branch_id)?.descriptor)
    }

    pub(crate) fn branch_state(&self, branch_id: BranchId) -> LifecycleResult<&BranchLocalState> {
        self.active_entry(branch_id)?
            .state
            .as_ref()
            .ok_or(LifecycleError::BranchNotWritable {
                branch_id,
                state: "deleted",
            })
    }

    pub(crate) fn branch_state_mut(
        &mut self,
        branch_id: BranchId,
        generation_guard: CommitBranchGenerationGuard,
    ) -> LifecycleResult<&mut BranchLocalState> {
        let index = self.active_entry_index(branch_id)?;
        let descriptor = self.entries[index].descriptor;
        require_generation(descriptor, generation_guard)?;
        self.advance_state_revision(index);
        self.entries[index]
            .state
            .as_mut()
            .ok_or(LifecycleError::BranchNotWritable {
                branch_id,
                state: "deleted",
            })
    }

    /// Split-borrow variant: returns `&mut BranchLocalState` and a
    /// concurrent immutable `&CommitBranchRegistry` reference, both
    /// borrowed from disjoint fields of the catalog. Used by the runtime
    /// commit path so the commit runtime can validate generation guards
    /// against the registry while holding a mutable branch state.
    pub(crate) fn branch_state_mut_with_registry(
        &mut self,
        branch_id: BranchId,
        generation_guard: CommitBranchGenerationGuard,
    ) -> LifecycleResult<(&mut BranchLocalState, &CommitBranchRegistry)> {
        let index = self.active_entry_index(branch_id)?;
        let descriptor = self.entries[index].descriptor;
        require_generation(descriptor, generation_guard)?;
        // Direct field access splits the borrows: `entries` is mutably
        // borrowed, `registry` is immutably borrowed, but the compiler
        // tracks them disjointly.
        let entries = &mut self.entries;
        let registry = &self.registry;
        let entry = &mut entries[index];
        entry.descriptor = entry.descriptor.with_next_revision();
        let branch = entry
            .state
            .as_mut()
            .ok_or(LifecycleError::BranchNotWritable {
                branch_id,
                state: "deleted",
            })?;
        Ok((branch, registry))
    }

    pub(crate) fn replace_active_branch_state(
        &mut self,
        branch_id: BranchId,
        generation_guard: CommitBranchGenerationGuard,
        state: BranchLocalState,
    ) -> LifecycleResult<BranchReachabilitySnapshot> {
        if state.branch_id() != branch_id {
            return Err(LifecycleError::BranchStateMismatch {
                expected: branch_id,
                actual: state.branch_id(),
            });
        }
        let index = self.active_entry_index(branch_id)?;
        require_generation(self.entries[index].descriptor, generation_guard)?;
        let snapshot = state.reachability_snapshot().map_err(branch_error)?;
        self.advance_state_revision(index);
        self.entries[index].state = Some(state);
        Ok(snapshot)
    }

    fn seed_active_branch_state(
        &mut self,
        state: &BranchLocalState,
    ) -> LifecycleResult<BranchReachabilitySnapshot> {
        let index = self.active_entry_index(state.branch_id())?;
        let snapshot = state.reachability_snapshot().map_err(branch_error)?;
        self.entries[index].state = Some(state.clone());
        Ok(snapshot)
    }

    pub(crate) fn replace_active_branch_state_with_descriptor(
        &mut self,
        expected_descriptor: LifecycleBranchDescriptor,
        state: BranchLocalState,
    ) -> LifecycleResult<BranchReachabilitySnapshot> {
        if state.branch_id() != expected_descriptor.branch_id() {
            return Err(LifecycleError::BranchStateMismatch {
                expected: expected_descriptor.branch_id(),
                actual: state.branch_id(),
            });
        }
        let index = self.active_entry_index(expected_descriptor.branch_id())?;
        if self.entries[index].descriptor != expected_descriptor {
            return Err(LifecycleError::BranchNotWritable {
                branch_id: expected_descriptor.branch_id(),
                state: "stale branch descriptor",
            });
        }
        let snapshot = state.reachability_snapshot().map_err(branch_error)?;
        self.advance_state_revision(index);
        self.entries[index].state = Some(state);
        Ok(snapshot)
    }

    pub(crate) fn capture_read_view(
        &mut self,
        branch_id: BranchId,
    ) -> LifecycleResult<BranchReadView> {
        let descriptor = self.active_entry(branch_id)?.descriptor;
        let snapshot = self
            .branch_state(branch_id)?
            .reachability_snapshot()
            .map_err(branch_error)?;
        let view = self
            .branch_state(branch_id)?
            .capture_read_view()
            .map_err(branch_error)?;
        self.pin_snapshot(descriptor, snapshot)?;
        Ok(view)
    }

    pub(crate) fn pin_reachability(
        &mut self,
        branch_id: BranchId,
    ) -> LifecycleResult<LifecyclePinnedBranchReachability> {
        let descriptor = self.active_entry(branch_id)?.descriptor;
        let snapshot = self
            .branch_state(branch_id)?
            .reachability_snapshot()
            .map_err(branch_error)?;
        self.pin_snapshot(descriptor, snapshot)
    }

    pub(crate) fn release_pinned_reachability(
        &mut self,
        pin: &LifecyclePinnedBranchReachability,
    ) -> bool {
        let before = self.pinned_snapshots.len();
        self.pinned_snapshots
            .retain(|record| record.pin_id != pin.pin_id);
        before != self.pinned_snapshots.len()
    }

    pub(crate) fn clear_branch(
        &mut self,
        branch_id: BranchId,
        generation_guard: CommitBranchGenerationGuard,
    ) -> LifecycleResult<LifecycleBranchClearOutcome> {
        let index = self.active_entry_index(branch_id)?;
        let descriptor = self.entries[index].descriptor;
        require_generation(descriptor, generation_guard)?;
        let old_snapshot = self.entries[index]
            .state
            .as_ref()
            .expect("active entry has state")
            .reachability_snapshot()
            .map_err(branch_error)?;
        let empty_state =
            BranchLocalState::new(branch_id, self.branch_config).map_err(branch_error)?;
        let release_plan = self.release_plan_after_removing(branch_id, &old_snapshot)?;

        let active = descriptor.with_next_revision();
        self.entries[index].descriptor = descriptor.with_status(LifecycleBranchStatus::Clearing);
        self.entries[index].state = Some(empty_state);
        self.entries[index].descriptor = active;
        Ok(LifecycleBranchClearOutcome {
            descriptor: active,
            release_plan,
        })
    }

    pub(crate) fn delete_branch(
        &mut self,
        branch_id: BranchId,
        generation_guard: CommitBranchGenerationGuard,
        deleted_at: Option<CommitVersion>,
    ) -> LifecycleResult<LifecycleBranchDeleteOutcome> {
        let index = self.active_entry_index(branch_id)?;
        let descriptor = self.entries[index].descriptor;
        require_generation(descriptor, generation_guard)?;
        let old_snapshot = self.entries[index]
            .state
            .as_ref()
            .expect("active entry has state")
            .reachability_snapshot()
            .map_err(branch_error)?;
        let release_plan = self.release_plan_after_removing(branch_id, &old_snapshot)?;

        self.registry
            .mark_deleting(branch_id)
            .map_err(commit_error)?;
        let deleted = descriptor
            .with_status(LifecycleBranchStatus::Deleted)
            .with_deleted_at(deleted_at)
            .with_next_revision();
        self.registry
            .mark_deleted(branch_id)
            .map_err(commit_error)?;
        self.entries[index].descriptor = descriptor.with_status(LifecycleBranchStatus::Deleting);
        self.entries[index].descriptor = deleted;
        self.entries[index].state = None;
        Ok(LifecycleBranchDeleteOutcome {
            descriptor: deleted,
            release_plan,
        })
    }

    pub(crate) fn fork_current(
        &mut self,
        source_branch_id: BranchId,
        destination_branch_id: BranchId,
        destination_generation: CommitBranchGeneration,
    ) -> LifecycleResult<LifecycleBranchForkOutcome> {
        self.require_destination_available(destination_branch_id, destination_generation)?;
        let source = self.branch_state(source_branch_id)?.clone();
        if source.active_row_count() > 0 || source.frozen_table_count() > 0 {
            return Err(LifecycleError::SourceHasUnflushedRows {
                branch_id: source_branch_id,
            });
        }
        let (child, fork_outcome) = source
            .fork_into_empty_child(destination_branch_id)
            .map_err(branch_error)?;
        let parent = LifecycleBranchParent::new(source_branch_id, fork_outcome.fork_version());
        let descriptor = LifecycleBranchDescriptor::active(
            destination_branch_id,
            destination_generation,
            Some(fork_outcome.fork_version()),
        )
        .with_parent(parent);
        self.install_new_branch_state(descriptor, child)?;
        Ok(LifecycleBranchForkOutcome {
            descriptor,
            source_branch_id,
            fork_version: fork_outcome.fork_version(),
            inherited_layer_count: fork_outcome.inherited_layer_count(),
            inherited_table_count: fork_outcome.inherited_table_count(),
        })
    }

    pub(crate) fn fork_at_retained_version(
        &mut self,
        source_branch_id: BranchId,
        destination_branch_id: BranchId,
        destination_generation: CommitBranchGeneration,
        fork_version: CommitVersion,
        retained_floor: CommitVersion,
    ) -> LifecycleResult<LifecycleBranchForkOutcome> {
        self.require_destination_available(destination_branch_id, destination_generation)?;
        if fork_version < retained_floor {
            return Err(LifecycleError::BranchHistoryUnavailable {
                branch_id: source_branch_id,
                reason: "requested fork version is below retained history",
            });
        }
        let source = self.branch_state(source_branch_id)?;
        let source_facts = source.facts().map_err(branch_error)?;
        let visible =
            source_facts
                .max_commit_version()
                .ok_or(LifecycleError::BranchHistoryUnavailable {
                    branch_id: source_branch_id,
                    reason: "source branch has no retained rows",
                })?;
        if fork_version > visible {
            return Err(LifecycleError::BranchHistoryUnavailable {
                branch_id: source_branch_id,
                reason: "requested fork version is newer than visible branch version",
            });
        }

        let rows = source
            .fork_snapshot_rows(fork_version, destination_branch_id)
            .map_err(branch_error)?;
        let mut states = vec![
            BranchLocalState::new(destination_branch_id, self.branch_config)
                .map_err(branch_error)?,
        ];
        if !rows.is_empty() {
            let request = BranchSnapshotInstallRequest::from_rows(
                format!(
                    "branch-lifecycle-fork-at-{}-{}-{}",
                    destination_branch_id,
                    destination_generation.get(),
                    fork_version.as_u64()
                ),
                rows,
            )
            .map_err(branch_error)?;
            install_snapshot_rows_into_branches(&mut states, &request).map_err(branch_error)?;
        }
        let child = states
            .into_iter()
            .next()
            .expect("destination state is always present");
        let parent = LifecycleBranchParent::new(source_branch_id, fork_version);
        let descriptor = LifecycleBranchDescriptor::active(
            destination_branch_id,
            destination_generation,
            Some(fork_version),
        )
        .with_parent(parent);
        self.install_new_branch_state(descriptor, child)?;
        Ok(LifecycleBranchForkOutcome {
            descriptor,
            source_branch_id,
            fork_version,
            inherited_layer_count: 0,
            inherited_table_count: 0,
        })
    }

    pub(crate) fn fork_at_retained_timestamp(
        &mut self,
        source_branch_id: BranchId,
        destination_branch_id: BranchId,
        destination_generation: CommitBranchGeneration,
        timestamp: Timestamp,
        retained_floor: CommitVersion,
    ) -> LifecycleResult<LifecycleBranchForkOutcome> {
        self.require_destination_available(destination_branch_id, destination_generation)?;
        let source = self.branch_state(source_branch_id)?;
        match source.timestamp_coverage() {
            BranchTimestampCoverage::Complete => {}
            BranchTimestampCoverage::CompleteSince { earliest_timestamp } => {
                if timestamp < earliest_timestamp {
                    return Err(LifecycleError::InsufficientTimestampHistory {
                        branch_id: source_branch_id,
                        reason: "requested timestamp is below retained timestamp coverage",
                    });
                }
            }
            BranchTimestampCoverage::Unknown => {
                return Err(LifecycleError::InsufficientTimestampHistory {
                    branch_id: source_branch_id,
                    reason: "branch has no retained timestamp coverage",
                });
            }
        }
        let resolved = source
            .resolve_timestamp_to_commit_version(timestamp)
            .ok_or(LifecycleError::InsufficientTimestampHistory {
                branch_id: source_branch_id,
                reason: "branch has no rows at or before requested timestamp",
            })?;
        self.fork_at_retained_version(
            source_branch_id,
            destination_branch_id,
            destination_generation,
            resolved,
            retained_floor,
        )
    }

    pub(crate) fn registry(&self) -> &CommitBranchRegistry {
        &self.registry
    }

    fn install_new_branch_state(
        &mut self,
        descriptor: LifecycleBranchDescriptor,
        state: BranchLocalState,
    ) -> LifecycleResult<()> {
        if state.branch_id() != descriptor.branch_id() {
            return Err(LifecycleError::BranchStateMismatch {
                expected: descriptor.branch_id(),
                actual: state.branch_id(),
            });
        }
        match self.find_entry_index(descriptor.branch_id()) {
            Some(index)
                if self.entries[index].descriptor.status() == LifecycleBranchStatus::Deleted =>
            {
                self.registry
                    .recreate_active(descriptor.branch_id(), descriptor.generation())
                    .map_err(commit_error)?;
                self.entries[index] = LifecycleBranchEntry {
                    descriptor,
                    state: Some(state),
                };
            }
            Some(_) => {
                return Err(LifecycleError::BranchAlreadyExists {
                    branch_id: descriptor.branch_id(),
                });
            }
            None => {
                self.registry
                    .register_active(descriptor.branch_id(), descriptor.generation())
                    .map_err(commit_error)?;
                self.entries.push(LifecycleBranchEntry {
                    descriptor,
                    state: Some(state),
                });
            }
        }
        self.sort_entries();
        Ok(())
    }

    fn require_destination_available(
        &self,
        destination_branch_id: BranchId,
        destination_generation: CommitBranchGeneration,
    ) -> LifecycleResult<()> {
        match self.entry(destination_branch_id) {
            Ok(entry) if entry.descriptor.status() == LifecycleBranchStatus::Deleted => {
                if entry.descriptor.generation().get() == u64::MAX {
                    return Err(LifecycleError::BranchGenerationExhausted {
                        branch_id: destination_branch_id,
                        generation: entry.descriptor.generation().get(),
                    });
                }
                if destination_generation <= entry.descriptor.generation() {
                    return Err(LifecycleError::BranchGenerationMismatch {
                        branch_id: destination_branch_id,
                        expected: entry.descriptor.generation().get().saturating_add(1),
                        actual: destination_generation.get(),
                    });
                }
                Ok(())
            }
            Ok(_) => Err(LifecycleError::BranchAlreadyExists {
                branch_id: destination_branch_id,
            }),
            Err(LifecycleError::BranchNotFound { .. }) => Ok(()),
            Err(error) => Err(error),
        }
    }

    fn release_plan_after_removing(
        &self,
        branch_id: BranchId,
        removed: &BranchReachabilitySnapshot,
    ) -> LifecycleResult<BranchReleasePlan> {
        let aggregate = self.aggregate_reachability_after_current_state(branch_id)?;
        BranchReleasePlan::from_removed_refs(
            branch_id,
            removed.table_refs().to_vec(),
            &aggregate,
            None,
        )
        .map_err(branch_error)
    }

    fn aggregate_reachability_after_current_state(
        &self,
        removed_branch_id: BranchId,
    ) -> LifecycleResult<BranchReachabilityAggregate> {
        let mut refs_by_branch =
            BTreeMap::<[u8; BranchId::BYTE_LEN], (BranchId, Vec<BranchTableRef>)>::new();
        for snapshot in &self.pinned_snapshots {
            append_pinned_snapshot_refs(&mut refs_by_branch, snapshot);
        }
        for entry in &self.entries {
            if entry.descriptor.status() == LifecycleBranchStatus::Active
                && entry.descriptor.branch_id() != removed_branch_id
            {
                if let Some(state) = &entry.state {
                    let snapshot = state.reachability_snapshot().map_err(branch_error)?;
                    append_snapshot_refs(&mut refs_by_branch, &snapshot);
                }
            }
        }
        let mut snapshots = Vec::with_capacity(refs_by_branch.len());
        for (branch_id, refs) in refs_by_branch.into_values() {
            snapshots.push(dedup_reachability_snapshot(branch_id, refs)?);
        }
        BranchReachabilityAggregate::from_snapshots(&snapshots).map_err(branch_error)
    }

    fn active_entry(&self, branch_id: BranchId) -> LifecycleResult<&LifecycleBranchEntry> {
        let entry = self.entry(branch_id)?;
        require_active_descriptor(entry.descriptor)?;
        Ok(entry)
    }

    fn active_entry_index(&self, branch_id: BranchId) -> LifecycleResult<usize> {
        let index = self.entry_index(branch_id)?;
        require_active_descriptor(self.entries[index].descriptor)?;
        Ok(index)
    }

    fn entry(&self, branch_id: BranchId) -> LifecycleResult<&LifecycleBranchEntry> {
        let index = self.entry_index(branch_id)?;
        Ok(&self.entries[index])
    }

    fn entry_index(&self, branch_id: BranchId) -> LifecycleResult<usize> {
        self.find_entry_index(branch_id)
            .ok_or(LifecycleError::BranchNotFound { branch_id })
    }

    fn find_entry_index(&self, branch_id: BranchId) -> Option<usize> {
        self.entries
            .iter()
            .position(|entry| entry.descriptor.branch_id() == branch_id)
    }

    fn sort_entries(&mut self) {
        self.entries
            .sort_by_key(|entry| *entry.descriptor.branch_id().as_bytes());
    }

    fn advance_state_revision(&mut self, index: usize) {
        self.entries[index].descriptor = self.entries[index].descriptor.with_next_revision();
    }

    fn pin_snapshot(
        &mut self,
        descriptor: LifecycleBranchDescriptor,
        snapshot: BranchReachabilitySnapshot,
    ) -> LifecycleResult<LifecyclePinnedBranchReachability> {
        let pin_id = self.next_pin_id;
        self.next_pin_id =
            self.next_pin_id
                .checked_add(1)
                .ok_or(LifecycleError::InvalidConfig {
                    field: "branch_reachability_pin",
                    reason: "pin id exhausted",
                })?;
        let record = LifecyclePinnedBranchReachabilityRecord {
            pin_id,
            descriptor,
            snapshot,
        };
        self.pinned_snapshots.push(record.clone());
        Ok(LifecyclePinnedBranchReachability {
            pin_id: record.pin_id,
            descriptor: record.descriptor,
            snapshot: record.snapshot,
        })
    }
}

fn append_snapshot_refs(
    refs_by_branch: &mut BTreeMap<[u8; BranchId::BYTE_LEN], (BranchId, Vec<BranchTableRef>)>,
    snapshot: &BranchReachabilitySnapshot,
) {
    refs_by_branch
        .entry(*snapshot.branch_id().as_bytes())
        .or_insert_with(|| (snapshot.branch_id(), Vec::new()))
        .1
        .extend(snapshot.table_refs().iter().cloned());
}

fn append_pinned_snapshot_refs(
    refs_by_branch: &mut BTreeMap<[u8; BranchId::BYTE_LEN], (BranchId, Vec<BranchTableRef>)>,
    record: &LifecyclePinnedBranchReachabilityRecord,
) {
    refs_by_branch
        .entry(*record.snapshot.branch_id().as_bytes())
        .or_insert_with(|| (record.snapshot.branch_id(), Vec::new()))
        .1
        .extend(record.snapshot.table_refs().iter().cloned());
}

fn dedup_reachability_snapshot(
    branch_id: BranchId,
    refs: Vec<BranchTableRef>,
) -> LifecycleResult<BranchReachabilitySnapshot> {
    let mut seen = BTreeSet::new();
    let mut unique_refs = Vec::with_capacity(refs.len());
    for table_ref in refs {
        if seen.insert(table_ref_dedup_key(&table_ref)) {
            unique_refs.push(table_ref);
        }
    }
    BranchReachabilitySnapshot::new(branch_id, unique_refs).map_err(branch_error)
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct LifecycleTableRefDedupKey {
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

fn table_ref_dedup_key(table_ref: &BranchTableRef) -> LifecycleTableRefDedupKey {
    let mut source_branch = [0; BranchId::BYTE_LEN];
    let mut fork_version = 0;
    let mut layer_index = 0;
    let reference_kind_rank = match table_ref.reference_kind() {
        BranchTableReferenceKind::Owned => 0,
        BranchTableReferenceKind::Replacement {
            source_branch_id,
            fork_version: version,
        } => {
            source_branch = *source_branch_id.as_bytes();
            fork_version = version.as_u64();
            1
        }
        BranchTableReferenceKind::Inherited {
            source_branch_id,
            fork_version: version,
            layer_index: index,
        } => {
            source_branch = *source_branch_id.as_bytes();
            fork_version = version.as_u64();
            layer_index = index;
            2
        }
        BranchTableReferenceKind::MaterializingSource {
            source_branch_id,
            fork_version: version,
            layer_index: index,
        } => {
            source_branch = *source_branch_id.as_bytes();
            fork_version = version.as_u64();
            layer_index = index;
            3
        }
    };
    LifecycleTableRefDedupKey {
        table_identity: table_ref.table_identity().as_str().to_owned(),
        reference_kind_rank,
        owner_branch: *table_ref.owner_branch_id().as_bytes(),
        table_branch: *table_ref.table_branch_id().as_bytes(),
        source_branch,
        fork_version,
        layer_index,
        level: table_ref.level().raw(),
        table_index: table_ref.table_index(),
    }
}

fn require_active_descriptor(descriptor: LifecycleBranchDescriptor) -> LifecycleResult<()> {
    if descriptor.status() == LifecycleBranchStatus::Active {
        return Ok(());
    }
    Err(LifecycleError::BranchNotWritable {
        branch_id: descriptor.branch_id(),
        state: descriptor.status().name(),
    })
}

fn require_generation(
    descriptor: LifecycleBranchDescriptor,
    guard: CommitBranchGenerationGuard,
) -> LifecycleResult<()> {
    match guard {
        // sync_active_branch_state is the only legitimate sync-without-guard path;
        // every other catalog mutation must thread a typed generation guard so
        // stale queued work cannot race with create/clear/delete/fork.
        CommitBranchGenerationGuard::NotSupplied => Err(LifecycleError::BranchGenerationMismatch {
            branch_id: descriptor.branch_id(),
            expected: descriptor.generation().get(),
            actual: 0,
        }),
        CommitBranchGenerationGuard::Exact(actual) if actual == descriptor.generation() => Ok(()),
        CommitBranchGenerationGuard::Exact(actual) => {
            Err(LifecycleError::BranchGenerationMismatch {
                branch_id: descriptor.branch_id(),
                expected: descriptor.generation().get(),
                actual: actual.get(),
            })
        }
    }
}

impl LifecycleBranchStatus {
    const fn name(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Clearing => "clearing",
            Self::Deleting => "deleting",
            Self::Deleted => "deleted",
        }
    }
}

fn branch_error(error: impl std::error::Error + Send + Sync + 'static) -> LifecycleError {
    LifecycleError::lower_layer_with(
        LifecycleLowerLayer::BranchRuntime,
        "branch lifecycle branch runtime failed",
        error,
    )
}

fn commit_error(error: CommitRuntimeError) -> LifecycleError {
    match error {
        CommitRuntimeError::BranchAlreadyExists { branch_id } => {
            LifecycleError::BranchAlreadyExists { branch_id }
        }
        CommitRuntimeError::BranchNotFound { branch_id } => {
            LifecycleError::BranchNotFound { branch_id }
        }
        CommitRuntimeError::BranchGenerationMismatch {
            branch_id,
            expected,
            actual,
        } => LifecycleError::BranchGenerationMismatch {
            branch_id,
            expected,
            actual,
        },
        CommitRuntimeError::BranchGenerationExhausted {
            branch_id,
            generation,
        } => LifecycleError::BranchGenerationExhausted {
            branch_id,
            generation,
        },
        CommitRuntimeError::BranchNotWritable { branch_id, reason } => {
            LifecycleError::BranchNotWritable {
                branch_id,
                state: reason,
            }
        }
        other => LifecycleError::lower_layer_with(
            LifecycleLowerLayer::CommitRuntime,
            "branch lifecycle commit registry failed",
            other,
        ),
    }
}

impl From<LifecycleBranchDescriptor> for CommitBranchDescriptor {
    fn from(descriptor: LifecycleBranchDescriptor) -> Self {
        let state = match descriptor.status() {
            LifecycleBranchStatus::Active => CommitBranchState::Active,
            LifecycleBranchStatus::Deleted => CommitBranchState::Deleted,
            LifecycleBranchStatus::Clearing | LifecycleBranchStatus::Deleting => {
                CommitBranchState::Deleting
            }
        };
        Self::new(descriptor.branch_id(), descriptor.generation(), state)
    }
}
