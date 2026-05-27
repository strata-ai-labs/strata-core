//! Commit-runtime bootstrap after durable recovery.

use super::{
    branch_error, commit_error, require_admitted, LifecycleDurableLocalServices,
    LifecycleDurableLocalShell,
};
use crate::branch::{BranchLocalState, BranchReadView};
use crate::commit::{
    CommitBatch, CommitBranchGeneration, CommitBranchGenerationGuard, CommitBranchGuardSet,
    CommitDurabilityClass, CommitDurableRuntime, CommitFactAllocator, CommitManualTimestampSource,
    CommitOutcome, CommitReplayAction, CommitReplayRequest, CommitReplayRuntime,
    CommitTimestampSource, CommitUnresolvedDurable, CommitUnresolvedDurableGate,
    VisibleVersionPublish, VisibleVersionTracker,
};
use crate::format::WalRecord;
use crate::lifecycle::{
    maintenance_ready_for_recovery_health, BudgetedCommitBranch, LifecycleBranchCatalog,
    LifecycleDurableTableCatalog, LifecycleError, LifecycleMaintenanceExecutor,
    LifecycleOperationKind, LifecycleRecoveryOutcome, LifecycleResult, LifecycleState,
    LifecycleStateMachine, LifecycleStats, LifecycleTransitionTrigger, RecoveryHealth,
    StorageBudgetLedger, StorageBudgetSnapshot, StorageMode, StorageOpenOutcome, StorageOpenPlan,
};
use std::sync::Arc;
use strata_core_next::{BranchId, CommitVersion, Timestamp};

#[derive(Debug)]
pub(crate) struct LifecycleDurableLocalRuntime<'a, S = CommitManualTimestampSource> {
    pub(super) state: LifecycleStateMachine,
    pub(super) open_plan: StorageOpenPlan,
    pub(super) open_outcome: StorageOpenOutcome,
    pub(super) bootstrap_report: LifecycleRecoveryBootstrapReport,
    pub(super) services: LifecycleDurableLocalServices<'a>,
    pub(super) branch_catalog: LifecycleBranchCatalog,
    pub(super) initial_branch_id: BranchId,
    pub(super) guard_set: CommitBranchGuardSet,
    pub(super) allocator: CommitFactAllocator<S>,
    pub(super) visible: VisibleVersionTracker,
    pub(super) durable_gate: CommitUnresolvedDurableGate,
    pub(super) commit_config: crate::commit::CommitRuntimeConfig,
    pub(super) table_catalog: LifecycleDurableTableCatalog,
    pub(super) budget: StorageBudgetLedger,
    pub(super) recovered_checkpoint_timestamp_max: Option<Timestamp>,
    pub(super) next_checkpoint_snapshot_id: u64,
    pub(super) current_recovery_health: RecoveryHealth,
    // Released table references from `clear_branch`/`delete_branch` queue
    // here until the next retention pass drains them. Slice A2 in-memory
    // only — restart loses the buffer (Follow-up B persists tombstones).
    pub(super) pending_releases: Vec<crate::branch::BranchReleasePlan>,
    // Branch catalog publish sequence counter. Increments on each
    // BranchCatalogManifest publication. Loaded from the manifest on
    // recovery so monotonicity holds across restarts.
    pub(super) branch_catalog_sequence: u64,
    #[allow(
        dead_code,
        reason = "runtime hook is consumed by concrete maintenance modules"
    )]
    pub(super) maintenance: LifecycleMaintenanceExecutor,
    // Opaque close-session retry state owned by `lifecycle/durable/close.rs`.
    // Bootstrap stores the snapshot but does not interpret it; subsequent
    // idempotent close calls inside `close.rs` deconstruct it through
    // its own helpers. The wrapper exists so this file does not need
    // to reference any concrete close types, preserving the
    // bootstrap-vs-close layering enforced by the lifecycle source guard.
    pub(super) close_retry_state: Option<super::close::DurableCloseRetryState>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleRecoveryBootstrapReport {
    records_seen: usize,
    records_applied: usize,
    records_already_applied: usize,
    rows_checked: usize,
    rows_applied: usize,
    gates_cleared: usize,
    checkpoint_visible_publish: Option<VisibleVersionPublish>,
    recovered_visible_version: CommitVersion,
    recovery_health: RecoveryHealth,
}

impl<'a, S> LifecycleDurableLocalShell<'a, S> {
    pub(crate) fn complete_recovery(
        mut self,
        recovery: &LifecycleRecoveryOutcome,
    ) -> LifecycleResult<LifecycleDurableLocalRuntime<'a, S>> {
        let report = self.bootstrap_commit_runtime_or_fail(recovery)?;
        let open_outcome = match StorageOpenOutcome::new(
            self.assembly_facts().mode(),
            self.assembly_facts().disposition(),
            Some(report.recovered_visible_version()),
            report.recovery_health().clone(),
            maintenance_ready_for_recovery_health(report.recovery_health()),
        ) {
            Ok(outcome) => outcome
                .with_backend_capabilities(self.services.capability_outcome().capabilities())
                .with_database_identity(
                    *self.assembly_facts().database_id(),
                    self.assembly_facts().codec_id().to_owned(),
                )
                .with_recovered_max_commit_version(Some(report.recovered_visible_version()))
                .with_durable_recovery_facts(recovery, &report)
                .with_budget_snapshot(self.budget.snapshot())
                .with_stats(LifecycleStats::new(
                    1,
                    recovery.health().fault_count(),
                    0,
                    0,
                    0,
                )),
            Err(error) => {
                self.mark_recovery_bootstrap_failed();
                return Err(error);
            }
        };
        if let Err(error) = self
            .state
            .transition(LifecycleTransitionTrigger::RecoveryAccepted)
        {
            self.mark_recovery_bootstrap_failed();
            return Err(error);
        }
        let max_maintenance_queue_depth = self
            .open_plan
            .lifecycle_config()
            .max_maintenance_queue_depth();
        let next_checkpoint_snapshot_id = self
            .assembly_facts()
            .manifest_snapshot_id()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or(LifecycleError::CheckpointPublicationFailed {
                reason: "checkpoint snapshot id overflow",
            })?;
        let branch_generation = self
            .registry
            .lookup(self.branch.branch_id())
            .map_err(commit_error)?
            .generation();
        let initial_branch_id = self.branch.branch_id();
        let mut branch_catalog = LifecycleBranchCatalog::with_existing_branch(
            &self.branch,
            branch_generation,
            self.branch.max_commit_version(),
        )?;
        // The shell's `self.branch` was used to seed the catalog above and
        // is no longer needed; drop it explicitly so the catalog is the
        // sole owner of branch state in the constructed runtime.
        drop(self.branch);
        // Replay the durable BranchCatalogManifest if present so create /
        // clear / delete / fork descriptors survive restart. Missing
        // manifest = pre-B database (single-branch mode); falls through
        // with the seeded branch the only catalog entry.
        let branch_catalog_sequence = match self
            .services
            .branch_catalog_manifest()
            .load_current()
            .map_err(|error| {
            LifecycleError::lower_layer_with(
                crate::lifecycle::LifecycleLowerLayer::Service,
                "branch catalog manifest load failed",
                error,
            )
        })? {
            Some(manifest) => {
                replay_branch_catalog_manifest(&mut branch_catalog, initial_branch_id, &manifest)?;
                manifest.manifest_sequence()
            }
            None => 0,
        };
        Ok(LifecycleDurableLocalRuntime {
            state: self.state,
            open_plan: self.open_plan,
            open_outcome,
            bootstrap_report: report,
            services: self.services,
            branch_catalog,
            initial_branch_id,
            guard_set: self.guard_set,
            allocator: self.allocator,
            visible: self.visible,
            durable_gate: self.durable_gate,
            commit_config: self.commit_config,
            table_catalog: self.table_catalog,
            budget: self.budget,
            recovered_checkpoint_timestamp_max: recovery.checkpoint().timestamp_max(),
            next_checkpoint_snapshot_id,
            current_recovery_health: recovery.health().clone(),
            pending_releases: Vec::new(),
            branch_catalog_sequence,
            maintenance: LifecycleMaintenanceExecutor::new(max_maintenance_queue_depth)?,
            close_retry_state: None,
        })
    }

    #[cfg(test)]
    pub(crate) fn try_bootstrap_commit_runtime_for_test(
        &mut self,
        recovery: &LifecycleRecoveryOutcome,
    ) -> LifecycleResult<LifecycleRecoveryBootstrapReport> {
        self.bootstrap_commit_runtime_or_fail(recovery)
    }

    fn bootstrap_commit_runtime_or_fail(
        &mut self,
        recovery: &LifecycleRecoveryOutcome,
    ) -> LifecycleResult<LifecycleRecoveryBootstrapReport> {
        match self.bootstrap_commit_runtime(recovery) {
            Ok(report) => Ok(report),
            Err(error) => {
                self.mark_recovery_bootstrap_failed();
                Err(error)
            }
        }
    }

    fn mark_recovery_bootstrap_failed(&mut self) {
        // The original bootstrap error is already being returned; this best-effort
        // transition only preserves the state-machine terminal fact.
        let _ = self
            .state
            .transition(LifecycleTransitionTrigger::PhaseFailed {
                reason: "recovery bootstrap failed",
            });
    }

    fn bootstrap_commit_runtime(
        &mut self,
        recovery: &LifecycleRecoveryOutcome,
    ) -> LifecycleResult<LifecycleRecoveryBootstrapReport> {
        require_admitted(self.state, LifecycleOperationKind::RecoveryStep)?;
        if matches!(recovery.health(), RecoveryHealth::Failed { .. }) {
            return Err(LifecycleError::RecoveryFailed {
                reason: "failed recovery package cannot be opened",
            });
        }
        let durability = commit_durability_class_for_mode(self.assembly_facts().mode())?;
        let checkpoint_watermark = recovery
            .checkpoint()
            .trusted_watermark()
            .unwrap_or(CommitVersion::ZERO);
        validate_recovered_wal_package(self.branch.branch_id(), recovery.wal().records())?;

        let mut report = LifecycleRecoveryBootstrapReport::new(recovery.health().clone());
        let mut replayed_max = CommitVersion::ZERO;
        for record in recovery.wal().records() {
            replayed_max = replayed_max.max(record.commit_version());
            let replay = CommitReplayRequest::new(record.clone(), durability);
            let replay_report = CommitReplayRuntime::new(
                &self.commit_config,
                &mut self.allocator,
                &mut self.branch,
                &mut self.visible,
                &self.durable_gate,
            )
            .replay(&replay)
            .map_err(commit_error)?;
            report.record_replay(&replay_report);
        }

        let recovered_visible_version = checkpoint_watermark.max(replayed_max);
        self.allocator
            .catch_up_to_recovered_version(recovered_visible_version);
        if let Some(timestamp) = recovery.checkpoint().timestamp_max() {
            self.allocator.catch_up_to_recovered_timestamp(timestamp);
        }
        let checkpoint_visible_publish =
            if recovered_visible_version > self.visible.visible_version() {
                Some(
                    self.visible
                        .catch_up_visible_after_replay(recovered_visible_version)
                        .map_err(|error| LifecycleError::RecoveryVisibilityFailed {
                            recovered_visible_version,
                            reason: "recovered rows were installed but visibility catch-up failed",
                            source: Some(Arc::new(error)),
                        })?,
                )
            } else {
                None
            };
        report.finish(checkpoint_visible_publish, recovered_visible_version);
        Ok(report)
    }
}

impl<S> LifecycleDurableLocalRuntime<'_, S> {
    pub(crate) const fn state(&self) -> LifecycleState {
        self.state.state()
    }

    pub(crate) const fn open_plan(&self) -> &StorageOpenPlan {
        &self.open_plan
    }

    pub(crate) const fn open_outcome(&self) -> &StorageOpenOutcome {
        &self.open_outcome
    }

    #[allow(
        dead_code,
        reason = "durable budget facts are consumed by integration and closeout slices"
    )]
    pub(crate) fn budget_snapshot(&self) -> StorageBudgetSnapshot {
        let branch = self
            .branch_catalog
            .branch_state(self.initial_branch_id)
            .expect("seeded branch is always present in the catalog");
        crate::lifecycle::snapshot_with_runtime_usage(
            &self.budget,
            branch,
            self.maintenance.status(),
        )
    }

    pub(crate) const fn bootstrap_report(&self) -> &LifecycleRecoveryBootstrapReport {
        &self.bootstrap_report
    }

    #[allow(
        dead_code,
        reason = "exposed for runtime tests; first non-test caller lands with the public storage api"
    )]
    pub(crate) fn pending_releases(&self) -> &[crate::branch::BranchReleasePlan] {
        &self.pending_releases
    }

    pub(crate) const fn current_recovery_health(&self) -> &RecoveryHealth {
        &self.current_recovery_health
    }

    pub(crate) const fn services(&self) -> &LifecycleDurableLocalServices<'_> {
        &self.services
    }

    /// Return the seeded branch's state. The seeded branch is registered
    /// at open time via `LifecycleBranchCatalog::with_existing_branch` and
    /// is the canonical anchor for the runtime's default-branch view; the
    /// `.expect(...)` reflects that invariant.
    pub(crate) fn branch_state(&self) -> &BranchLocalState {
        self.branch_catalog
            .branch_state(self.initial_branch_id)
            .expect("seeded branch is always present in the catalog")
    }

    #[allow(
        dead_code,
        reason = "branch lifecycle API uses this runtime catalog surface when public wrappers land"
    )]
    pub(crate) const fn branch_catalog(&self) -> &LifecycleBranchCatalog {
        &self.branch_catalog
    }

    /// Test-only mutable accessor; delegates to the catalog's
    /// `branch_state_mut` with the seeded branch's current generation.
    /// Each call advances the catalog's `state_revision` counter.
    #[cfg(any(test, feature = "testkit"))]
    pub(crate) fn branch_state_mut(&mut self) -> &mut BranchLocalState {
        let branch_id = self.initial_branch_id;
        let generation = self
            .branch_catalog
            .registry()
            .lookup(branch_id)
            .expect("seeded branch is always registered")
            .generation();
        self.branch_catalog
            .branch_state_mut(
                branch_id,
                crate::commit::CommitBranchGenerationGuard::exact(generation),
            )
            .expect("seeded branch is always present in the catalog")
    }

    #[allow(
        dead_code,
        reason = "durable table catalog is asserted by recovery tests"
    )]
    pub(crate) const fn table_catalog(&self) -> &LifecycleDurableTableCatalog {
        &self.table_catalog
    }

    #[cfg(test)]
    pub(crate) const fn guard_set(&self) -> &CommitBranchGuardSet {
        &self.guard_set
    }

    #[cfg(test)]
    pub(crate) const fn durable_gate(&self) -> &CommitUnresolvedDurableGate {
        &self.durable_gate
    }

    pub(crate) const fn visible_version(&self) -> CommitVersion {
        self.visible.visible_version()
    }

    pub(crate) const fn allocator(&self) -> &CommitFactAllocator<S> {
        &self.allocator
    }

    pub(crate) fn unresolved_durable(&self) -> LifecycleResult<Option<CommitUnresolvedDurable>> {
        self.durable_gate.unresolved().map_err(commit_error)
    }

    pub(crate) fn read_view(&self) -> LifecycleResult<BranchReadView> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryRead)?;
        let branch = self
            .branch_catalog
            .branch_state(self.initial_branch_id)
            .expect("seeded branch is always present in the catalog");
        branch.capture_read_view().map_err(branch_error)
    }

    /// Storage-internal: create a new branch in the catalog. Caveat for A1:
    /// commits route through the runtime's `self.branch` (the seeded branch).
    /// New branches created here are visible via `list_branches` and the
    /// catalog accessor but cannot accept commits until A2 makes the runtime
    /// catalog-authoritative for mutations. Durable mode in A1 is in-memory
    /// only — restart loses these descriptors (Follow-up B persists them).
    pub(crate) fn create_branch(
        &mut self,
        branch_id: BranchId,
        generation: CommitBranchGeneration,
        created_at: Option<CommitVersion>,
    ) -> LifecycleResult<crate::lifecycle::LifecycleBranchCreateOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let outcome = self
            .branch_catalog
            .create_branch(branch_id, generation, created_at)?;
        self.publish_branch_catalog()?;
        Ok(outcome)
    }

    pub(crate) fn list_branches(
        &self,
        include_deleted: bool,
    ) -> Vec<crate::lifecycle::LifecycleBranchDescriptor> {
        self.branch_catalog.list_branches(include_deleted)
    }

    #[allow(
        dead_code,
        reason = "exposed for A1 catalog surface; durable callers land in A2"
    )]
    pub(crate) fn fork_current(
        &mut self,
        source: BranchId,
        destination: BranchId,
        destination_generation: CommitBranchGeneration,
    ) -> LifecycleResult<crate::lifecycle::LifecycleBranchForkOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let outcome =
            self.branch_catalog
                .fork_current(source, destination, destination_generation)?;
        self.publish_branch_catalog()?;
        Ok(outcome)
    }

    #[allow(
        dead_code,
        reason = "exposed for A1 catalog surface; durable callers land in A2"
    )]
    pub(crate) fn fork_at_retained_version(
        &mut self,
        source: BranchId,
        destination: BranchId,
        destination_generation: CommitBranchGeneration,
        fork_version: CommitVersion,
        retained_floor: CommitVersion,
    ) -> LifecycleResult<crate::lifecycle::LifecycleBranchForkOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let outcome = self.branch_catalog.fork_at_retained_version(
            source,
            destination,
            destination_generation,
            fork_version,
            retained_floor,
        )?;
        self.publish_branch_catalog()?;
        Ok(outcome)
    }

    pub(crate) fn fork_at_retained_timestamp(
        &mut self,
        source: BranchId,
        destination: BranchId,
        destination_generation: CommitBranchGeneration,
        timestamp: Timestamp,
        retained_floor: CommitVersion,
    ) -> LifecycleResult<crate::lifecycle::LifecycleBranchForkOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let outcome = self.branch_catalog.fork_at_retained_timestamp(
            source,
            destination,
            destination_generation,
            timestamp,
            retained_floor,
        )?;
        self.publish_branch_catalog()?;
        Ok(outcome)
    }

    pub(crate) fn clear_branch(
        &mut self,
        branch_id: BranchId,
        generation_guard: CommitBranchGenerationGuard,
    ) -> LifecycleResult<crate::lifecycle::LifecycleBranchClearOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let outcome = self
            .branch_catalog
            .clear_branch(branch_id, generation_guard)?;
        let plan = outcome.release_plan().clone();
        if !plan.protected_tables().is_empty() {
            if let Ok(health) =
                crate::lifecycle::telemetry_health_debt("pinned view blocks clear/delete release")
            {
                self.record_recovery_health(Some(&health));
            }
        }
        self.pending_releases.push(plan);
        self.publish_branch_catalog()?;
        Ok(outcome)
    }

    pub(crate) fn delete_branch(
        &mut self,
        branch_id: BranchId,
        generation_guard: CommitBranchGenerationGuard,
        deleted_at: Option<CommitVersion>,
    ) -> LifecycleResult<crate::lifecycle::LifecycleBranchDeleteOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let outcome = self
            .branch_catalog
            .delete_branch(branch_id, generation_guard, deleted_at)?;
        let plan = outcome.release_plan().clone();
        if !plan.protected_tables().is_empty() {
            if let Ok(health) =
                crate::lifecycle::telemetry_health_debt("pinned view blocks clear/delete release")
            {
                self.record_recovery_health(Some(&health));
            }
        }
        self.pending_releases.push(plan);
        self.publish_branch_catalog()?;
        Ok(outcome)
    }

    /// Publish a fresh `BranchCatalogManifest` reflecting the current
    /// catalog state. Called after every durable catalog mutation. The
    /// runtime's `branch_catalog_sequence` is incremented monotonically
    /// so recovery can resolve concurrent-writer scenarios.
    fn publish_branch_catalog(&mut self) -> LifecycleResult<()> {
        let entries = self
            .branch_catalog
            .durable_entries()
            .map_err(branch_catalog_format_error)?;
        self.branch_catalog_sequence = self.branch_catalog_sequence.saturating_add(1);
        if self.branch_catalog_sequence == 0 {
            return Err(LifecycleError::CheckpointPublicationFailed {
                reason: "branch catalog sequence overflow",
            });
        }
        let manifest = crate::format::BranchCatalogManifest::new(
            *self.services.assembly_facts().database_id(),
            self.branch_catalog_sequence,
            entries,
        )
        .map_err(branch_catalog_format_error)?;
        self.services
            .branch_catalog_manifest()
            .publish_replace(&manifest)
            .map_err(branch_catalog_manifest_service_error)?;
        Ok(())
    }
}

fn branch_catalog_format_error(error: crate::format::FormatError) -> LifecycleError {
    LifecycleError::lower_layer_with(
        crate::lifecycle::LifecycleLowerLayer::Format,
        "branch catalog manifest encode failed",
        error,
    )
}

/// Reconstruct the in-memory catalog from a persisted
/// `BranchCatalogManifest`. The seeded branch is already in the catalog
/// (registered via `with_existing_branch`); reconcile its state against
/// the manifest entry. Other entries are created (Active) or registered
/// then deleted (Deleted) to produce the same descriptor as the original
/// runtime did before close.
fn replay_branch_catalog_manifest(
    catalog: &mut LifecycleBranchCatalog,
    initial_branch_id: BranchId,
    manifest: &crate::format::BranchCatalogManifest,
) -> LifecycleResult<()> {
    use crate::commit::{CommitBranchGeneration, CommitBranchGenerationGuard};
    use crate::format::BranchCatalogStatus;
    for entry in manifest.entries() {
        let branch_id = entry.branch_id();
        let generation_value = entry.generation();
        let generation = CommitBranchGeneration::new(generation_value).map_err(|_| {
            LifecycleError::RecoveryFailed {
                reason: "branch catalog manifest entry has invalid generation",
            }
        })?;
        let created_at = entry.created_at().map(CommitVersion::new);

        if branch_id == initial_branch_id {
            // The seeded branch is already registered via with_existing_branch.
            // For Active entries, no further work; for Deleted entries, mark
            // the seeded branch as deleted now. Generation mismatches against
            // the seeded branch's runtime generation surface as a recovery
            // conflict (the catalog says "this generation was seen at close
            // time"; if it disagrees with the runtime's seeded generation,
            // the seeded generation wins since it was just constructed).
            if matches!(entry.status(), BranchCatalogStatus::Deleted) {
                let deleted_at = entry.deleted_at().map(CommitVersion::new);
                // Use the current seeded generation rather than the manifest
                // generation: tombstone applies to whichever generation the
                // seeded branch carries today. Mismatches indicate corruption
                // or an in-flight restart; in either case the catalog wins
                // because the manifest survived past the runtime that wrote it.
                let current = catalog.lookup(initial_branch_id)?.generation();
                catalog.delete_branch(
                    initial_branch_id,
                    CommitBranchGenerationGuard::exact(current),
                    deleted_at,
                )?;
            }
            continue;
        }

        // Non-seeded branch: create_branch handles both fresh Active and
        // resurrection-after-deleted-by-newer-generation flows via its
        // existing generation arbitration.
        catalog.create_branch(branch_id, generation, created_at)?;
        if matches!(entry.status(), BranchCatalogStatus::Deleted) {
            let deleted_at = entry.deleted_at().map(CommitVersion::new);
            catalog.delete_branch(
                branch_id,
                CommitBranchGenerationGuard::exact(generation),
                deleted_at,
            )?;
        }
    }
    Ok(())
}

fn branch_catalog_manifest_service_error(
    error: crate::service::ManifestServiceError,
) -> LifecycleError {
    LifecycleError::lower_layer_with(
        crate::lifecycle::LifecycleLowerLayer::Service,
        "branch catalog manifest service failed",
        error,
    )
}

impl<S> LifecycleDurableLocalRuntime<'_, S>
where
    S: CommitTimestampSource,
{
    pub(crate) fn execute_durable_commit(
        &mut self,
        batch: CommitBatch,
        generation_guard: CommitBranchGenerationGuard,
    ) -> LifecycleResult<CommitOutcome> {
        require_admitted(self.state, LifecycleOperationKind::Commit)?;
        let branch_id = batch.branch_id();
        // Pre-sync shadow into catalog so the commit runtime sees any direct
        // shadow mutations (test-only) before fetching from the catalog.
        let generation = self
            .branch_catalog
            .registry()
            .lookup(branch_id)
            .map_err(commit_error)?
            .generation();
        let outcome = {
            let (branch, registry) = self.branch_catalog.branch_state_mut_with_registry(
                branch_id,
                CommitBranchGenerationGuard::exact(generation),
            )?;
            let mut budgeted_branch = BudgetedCommitBranch::new(branch, &self.budget);
            CommitDurableRuntime::new(
                &self.commit_config,
                registry,
                &self.guard_set,
                &mut self.allocator,
                &mut budgeted_branch,
                &mut self.visible,
                &mut self.services.wal,
                &self.durable_gate,
            )
            .execute(batch, generation_guard)
            .map_err(commit_error)
        };
        outcome
    }
}

impl LifecycleRecoveryBootstrapReport {
    const fn new(recovery_health: RecoveryHealth) -> Self {
        Self {
            records_seen: 0,
            records_applied: 0,
            records_already_applied: 0,
            rows_checked: 0,
            rows_applied: 0,
            gates_cleared: 0,
            checkpoint_visible_publish: None,
            recovered_visible_version: CommitVersion::ZERO,
            recovery_health,
        }
    }

    fn record_replay(&mut self, replay: &crate::commit::CommitReplayReport) {
        self.records_seen = self.records_seen.saturating_add(1);
        match replay.action() {
            CommitReplayAction::Applied => {
                self.records_applied = self.records_applied.saturating_add(1);
            }
            CommitReplayAction::AlreadyApplied => {
                self.records_already_applied = self.records_already_applied.saturating_add(1);
            }
        }
        self.rows_checked = self.rows_checked.saturating_add(replay.rows_checked());
        self.rows_applied = self.rows_applied.saturating_add(replay.rows_applied());
        if replay.gate_cleared() {
            self.gates_cleared = self.gates_cleared.saturating_add(1);
        }
    }

    fn finish(
        &mut self,
        checkpoint_visible_publish: Option<VisibleVersionPublish>,
        recovered_visible_version: CommitVersion,
    ) {
        self.checkpoint_visible_publish = checkpoint_visible_publish;
        self.recovered_visible_version = recovered_visible_version;
    }

    pub(crate) const fn records_seen(&self) -> usize {
        self.records_seen
    }

    pub(crate) const fn records_applied(&self) -> usize {
        self.records_applied
    }

    pub(crate) const fn records_already_applied(&self) -> usize {
        self.records_already_applied
    }

    pub(crate) const fn rows_checked(&self) -> usize {
        self.rows_checked
    }

    pub(crate) const fn rows_applied(&self) -> usize {
        self.rows_applied
    }

    pub(crate) const fn gates_cleared(&self) -> usize {
        self.gates_cleared
    }

    pub(crate) const fn checkpoint_visible_publish(&self) -> Option<VisibleVersionPublish> {
        self.checkpoint_visible_publish
    }

    pub(crate) const fn recovered_visible_version(&self) -> CommitVersion {
        self.recovered_visible_version
    }

    pub(crate) const fn recovery_health(&self) -> &RecoveryHealth {
        &self.recovery_health
    }
}

fn commit_durability_class_for_mode(mode: StorageMode) -> LifecycleResult<CommitDurabilityClass> {
    match mode {
        StorageMode::DurableLocalStandard => Ok(CommitDurabilityClass::Standard),
        StorageMode::DurableLocalAlways => Ok(CommitDurabilityClass::Always),
        StorageMode::Cache | StorageMode::ObjectDurableCandidate => {
            Err(LifecycleError::InvalidOpenPlan {
                reason: "commit recovery bootstrap requires durable local storage mode",
            })
        }
    }
}

fn validate_recovered_wal_package(
    branch_id: BranchId,
    records: &[WalRecord],
) -> LifecycleResult<()> {
    let mut previous = None;
    for record in records {
        if record.branch_id() != branch_id {
            return Err(LifecycleError::RecoveryFailed {
                reason: "recovered WAL package contains an unopened branch",
            });
        }
        if previous.is_some_and(|previous| record.commit_version() <= previous) {
            return Err(LifecycleError::RecoveryFailed {
                reason: "recovered WAL package must be strictly ordered",
            });
        }
        previous = Some(record.commit_version());
    }
    Ok(())
}
