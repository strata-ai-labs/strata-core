//! Commit-runtime bootstrap after durable recovery.

use super::{
    branch_error, commit_error, require_admitted, LifecycleDurableLocalServices,
    LifecycleDurableLocalShell,
};
use crate::branch::error::BranchRuntimeError;
use crate::branch::read::{BranchHistoryRow, BranchReadBound, BranchReadView, BranchScanBounds};
use crate::branch::state::BranchLocalState;
use crate::commit::{
    CommitBatch, CommitBatchKind, CommitBranchGeneration, CommitBranchGenerationGuard,
    CommitBranchGuardSet, CommitDurabilityClass, CommitDurabilityMode, CommitDurableRuntime,
    CommitFactAllocator, CommitManualTimestampSource, CommitOutcome, CommitReplayAction,
    CommitReplayRequest, CommitReplayRuntime, CommitRuntimeError, CommitTimestampSource,
    CommitUnresolvedDurable, CommitUnresolvedDurableGate, VisibleVersionPublish,
    VisibleVersionTracker,
};
use crate::format::WalRecord;
use crate::lifecycle::{
    estimate_commit_batch_active_bytes, maintenance_ready_for_recovery_health,
    projected_commit_rotation_would_exceed_frozen_budget, BudgetedCommitBranch,
    LifecycleBranchCatalog, LifecycleDurableTableCatalog, LifecycleError,
    LifecycleMaintenanceExecutor, LifecycleOperationKind, LifecycleRecoveryOutcome,
    LifecycleResult, LifecycleState, LifecycleStateMachine, LifecycleStats,
    LifecycleStoragePressureReason, LifecycleStoragePressureSeverity, LifecycleTransitionTrigger,
    LifecycleWalGrowthOutcome, LifecycleWalGrowthTrigger, LifecycleWriteAdmissionOutcome,
    RecoveryExclusivityToken, RecoveryHealth, StorageBudgetLedger, StorageBudgetSnapshot,
    StorageMode, StorageOpenOutcome, StorageOpenPlan,
};
use crate::observability::perf_trace;
use crate::row::PhysicalKey;
use crate::service::WalGrowthFacts;
use crate::table::TableRuntimeError;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
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
    pub(super) last_wal_growth_outcome: Option<LifecycleWalGrowthOutcome>,
    pub(super) pressure_rejected_commit_branches: HashSet<BranchId>,
    pub(super) last_write_admission: Option<LifecycleWriteAdmissionOutcome>,
    // Released table references from `clear_branch`/`delete_branch` queue
    // here until the next retention pass drains them. In-memory only —
    // restart loses the buffer; durable persistence of release tombstones
    // is tracked in the closeout doc as a separate workstream.
    pub(super) pending_releases: Vec<crate::branch::facts::BranchReleasePlan>,
    // Branch catalog publish sequence counter. Increments on each
    // BranchCatalogManifest publication. Loaded from the manifest on
    // recovery so monotonicity holds across restarts.
    pub(super) branch_catalog_sequence: u64,
    // Pending-releases manifest publish sequence counter. Increments on
    // each PendingReleasesManifest publication (push by clear/delete or
    // drain by retention). Loaded from the manifest on recovery so the
    // sequence remains monotonic across restarts. Zero before the first
    // publication.
    pub(super) pending_releases_sequence: u64,
    // Per-branch publish slots. The off-lock publish phase performs the manifest fsync with the
    // global runtime lock released; this map serializes same-branch publishes so only one is ever
    // between sequence-reserve and record (no durable manifest-sequence regression). In-memory
    // only, populated lazily per branch. Held here rather than in the branch catalog because the
    // catalog is cloned during recovery/staging and the slots must be unique to the live runtime.
    pub(super) branch_publish_locks: HashMap<BranchId, Arc<AtomicBool>>,
    #[allow(
        dead_code,
        reason = "runtime hook is consumed by concrete maintenance modules"
    )]
    pub(super) maintenance: LifecycleMaintenanceExecutor,
    pub(super) maintenance_coverage_idle_rounds: usize,
    // Opaque close-session retry state owned by `lifecycle/durable/close.rs`.
    // Bootstrap stores the snapshot but does not interpret it; subsequent
    // idempotent close calls inside `close.rs` deconstruct it through
    // its own helpers. The wrapper exists so this file does not need
    // to reference any concrete close types, preserving the
    // bootstrap-vs-close layering enforced by the lifecycle source guard.
    pub(super) close_retry_state: Option<super::close::DurableCloseRetryState>,
}

/// RAII guard for a per-branch publish slot (see
/// [`LifecycleDurableLocalRuntime::try_acquire_branch_publish_guard`]). Held across the off-lock
/// manifest fsync so at most one publish per branch sits between sequence-reserve and record;
/// clears the per-branch flag on drop, including on early return or panic.
pub(super) struct BranchPublishGuard {
    flag: Arc<AtomicBool>,
}

impl Drop for BranchPublishGuard {
    fn drop(&mut self) {
        self.flag.store(false, Ordering::Release);
    }
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
        let (
            report,
            branch_catalog,
            branch_catalog_sequence,
            pending_releases_sequence,
            pending_releases,
            initial_branch_id,
        ) = match self.prepare_catalog_and_replay(recovery) {
            Ok(values) => values,
            Err(error) => {
                self.mark_recovery_bootstrap_failed();
                return Err(error);
            }
        };
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
            last_wal_growth_outcome: None,
            pressure_rejected_commit_branches: HashSet::new(),
            last_write_admission: None,
            pending_releases,
            branch_catalog_sequence,
            pending_releases_sequence,
            branch_publish_locks: HashMap::new(),
            maintenance: LifecycleMaintenanceExecutor::new(max_maintenance_queue_depth)?,
            maintenance_coverage_idle_rounds: 0,
            close_retry_state: None,
        })
    }

    /// Build the runtime catalog, replay durable manifests, and dispatch
    /// WAL replay per branch. Runs inside `complete_recovery`; any error
    /// here triggers the bootstrap-failure transition in the caller.
    fn prepare_catalog_and_replay(
        &mut self,
        recovery: &LifecycleRecoveryOutcome,
    ) -> LifecycleResult<(
        LifecycleRecoveryBootstrapReport,
        LifecycleBranchCatalog,
        u64,
        u64,
        Vec<crate::branch::facts::BranchReleasePlan>,
        BranchId,
    )> {
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

        // Build the catalog from the seeded branch's post-checkpoint state.
        let initial_branch_id = self.branch.branch_id();
        let branch_generation = self
            .registry
            .lookup(initial_branch_id)
            .map_err(commit_error)?
            .generation();
        let mut branch_catalog = LifecycleBranchCatalog::with_existing_branch(
            &self.branch,
            branch_generation,
            self.branch.max_commit_version(),
        )?;
        // The shell's `self.branch` was cloned into the catalog above;
        // the catalog owns the canonical state from here on. The shell's
        // copy is dropped together with `self` when `complete_recovery`
        // returns.

        // Replay the durable BranchCatalogManifest if present so non-seeded
        // descriptors survive restart. A missing manifest means the database
        // never published a multi-branch catalog; the seeded branch is the
        // only catalog entry.
        let branch_catalog_sequence = match self
            .services
            .branch_catalog_manifest()
            .load_current()
            .map_err(branch_catalog_manifest_service_error)?
        {
            Some(manifest) => {
                replay_branch_catalog_manifest(&mut branch_catalog, initial_branch_id, &manifest)?;
                manifest.manifest_sequence()
            }
            None => 0,
        };

        // Reload the durable PendingReleasesManifest if present. Each
        // entry is converted back to a release plan carrying the
        // persisted releasable-table list; protected_tables and
        // removed_refs stay empty (the next retention pass recomputes
        // reachability from the manifest).
        let (pending_releases, pending_releases_sequence) = match self
            .services
            .pending_releases_manifest()
            .load_current()
            .map_err(pending_releases_manifest_service_error)?
        {
            Some(manifest) => {
                let mut plans = Vec::with_capacity(manifest.entries().len());
                for entry in manifest.entries() {
                    let identities = entry
                        .released_tables()
                        .iter()
                        .map(|identity| {
                            crate::table::TableIdentity::new(identity.clone()).map_err(|source| {
                                LifecycleError::lower_layer_with(
                                    crate::lifecycle::LifecycleLowerLayer::Format,
                                    "pending releases manifest table identity invalid",
                                    source,
                                )
                            })
                        })
                        .collect::<LifecycleResult<Vec<_>>>()?;
                    plans.push(
                        crate::branch::facts::BranchReleasePlan::from_releasable_tables(
                            entry.branch_id(),
                            identities,
                        ),
                    );
                }
                (plans, manifest.manifest_sequence())
            }
            None => (Vec::new(), 0),
        };

        // Install per-branch durable table manifests into non-seeded slots.
        // The seeded branch's manifest was already applied by the pre-catalog
        // recovery phase (apply_table_manifest_recovery on the shell).
        recover_per_branch_table_manifests(
            &self.services,
            &mut self.table_catalog,
            &mut branch_catalog,
            initial_branch_id,
            Some(&self.budget),
        )?;

        // Install non-seeded checkpoint rows into their catalog slots.
        // Seeded-branch rows were installed during `recover_checkpoint`;
        // anything else came from the multi-branch checkpoint encoder.
        install_non_seeded_checkpoint_rows(
            &mut branch_catalog,
            recovery.checkpoint().non_seeded_rows(),
            recovery.checkpoint().install_identity_seed(),
        )?;

        // Validate the WAL package against the catalog (multi-branch aware).
        validate_recovered_wal_package(&branch_catalog, recovery.wal().records())?;

        // Dispatch WAL replay by branch_id into per-branch catalog slots.
        let report = replay_wal_into_catalog(
            &mut branch_catalog,
            &self.commit_config,
            &mut self.allocator,
            &mut self.visible,
            &self.durable_gate,
            recovery,
            durability,
            checkpoint_watermark,
        )?;
        rebuild_fork_snapshot_rows(&mut branch_catalog)?;
        Ok((
            report,
            branch_catalog,
            branch_catalog_sequence,
            pending_releases_sequence,
            pending_releases,
            initial_branch_id,
        ))
    }

    /// Test-only entry point that mirrors the production validation +
    /// replay path against a temporary catalog seeded from the shell's
    /// branch. Used to exercise per-record replay failures (e.g.
    /// unresolved-gate mismatches) without consuming the shell.
    #[cfg(test)]
    pub(crate) fn try_bootstrap_commit_runtime_for_test(
        &mut self,
        recovery: &LifecycleRecoveryOutcome,
    ) -> LifecycleResult<LifecycleRecoveryBootstrapReport> {
        match self.try_bootstrap_commit_runtime_for_test_inner(recovery) {
            Ok(report) => Ok(report),
            Err(error) => {
                self.mark_recovery_bootstrap_failed();
                Err(error)
            }
        }
    }

    #[cfg(test)]
    fn try_bootstrap_commit_runtime_for_test_inner(
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
        let branch_generation = self
            .registry
            .lookup(self.branch.branch_id())
            .map_err(commit_error)?
            .generation();
        let mut branch_catalog = LifecycleBranchCatalog::with_existing_branch(
            &self.branch,
            branch_generation,
            self.branch.max_commit_version(),
        )?;
        validate_recovered_wal_package(&branch_catalog, recovery.wal().records())?;
        replay_wal_into_catalog(
            &mut branch_catalog,
            &self.commit_config,
            &mut self.allocator,
            &mut self.visible,
            &self.durable_gate,
            recovery,
            durability,
            checkpoint_watermark,
        )
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
}

impl<S> LifecycleDurableLocalRuntime<'_, S> {
    /// Try to claim the per-branch publish slot for the off-lock publish phase. Returns the guard
    /// when no other publish is in flight for this branch; `None` (the caller should defer the
    /// task) when one already holds it. Claimed under the global runtime lock and held across the
    /// lock release during the off-lock fsync — the global lock is never blocked on this slot, so
    /// the global→per-branch acquisition order cannot deadlock.
    pub(super) fn try_acquire_branch_publish_guard(
        &mut self,
        branch_id: BranchId,
    ) -> Option<BranchPublishGuard> {
        let flag = self
            .branch_publish_locks
            .entry(branch_id)
            .or_insert_with(|| Arc::new(AtomicBool::new(false)))
            .clone();
        match flag.compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed) {
            Ok(_) => Some(BranchPublishGuard { flag }),
            Err(_) => None,
        }
    }

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
    pub(crate) fn pending_releases(&self) -> &[crate::branch::facts::BranchReleasePlan] {
        &self.pending_releases
    }

    pub(crate) const fn current_recovery_health(&self) -> &RecoveryHealth {
        &self.current_recovery_health
    }

    pub(crate) const fn last_wal_growth_outcome(&self) -> Option<&LifecycleWalGrowthOutcome> {
        self.last_wal_growth_outcome.as_ref()
    }

    pub(crate) fn current_wal_growth_facts(&self) -> LifecycleResult<WalGrowthFacts> {
        self.services.wal().growth_facts().map_err(super::wal_error)
    }

    pub(crate) fn current_wal_growth_backpressure_snapshot(
        &self,
    ) -> LifecycleResult<(WalGrowthFacts, u64, Option<LifecycleWalGrowthTrigger>)> {
        let policy = self.open_plan.lifecycle_config().wal_growth_policy();
        let facts = self.current_wal_growth_facts()?;
        let current_manifest = self
            .services
            .manifest()
            .load_required()
            .map_err(super::manifest_error)?;
        let checkpoint_watermark = current_manifest
            .snapshot_watermark()
            .map(CommitVersion::new);
        let retention_watermark = crate::lifecycle::wal_retention_watermark(
            checkpoint_watermark,
            current_manifest.flushed_through_commit_id(),
        );
        let commits_since_checkpoint = crate::lifecycle::commits_since_checkpoint(
            self.visible.visible_version(),
            retention_watermark,
        );
        let trigger = policy.backpressure_trigger_for(facts, commits_since_checkpoint);
        Ok((facts, commits_since_checkpoint, trigger))
    }

    pub(crate) const fn last_write_admission(&self) -> Option<LifecycleWriteAdmissionOutcome> {
        self.last_write_admission
    }

    fn require_generation_guard(
        branch_id: BranchId,
        generation: CommitBranchGeneration,
        generation_guard: CommitBranchGenerationGuard,
    ) -> LifecycleResult<()> {
        match generation_guard {
            CommitBranchGenerationGuard::NotSupplied => Ok(()),
            CommitBranchGenerationGuard::Exact(supplied) if supplied == generation => Ok(()),
            CommitBranchGenerationGuard::Exact(supplied) => {
                Err(LifecycleError::BranchGenerationMismatch {
                    branch_id,
                    expected: generation.get(),
                    actual: supplied.get(),
                })
            }
        }
    }

    fn require_durable_commit_mode(batch: &CommitBatch) -> LifecycleResult<()> {
        if batch.kind() == CommitBatchKind::Mutating
            && batch.options().durability() == CommitDurabilityMode::Cache
        {
            return Err(commit_error(CommitRuntimeError::DurabilityUnavailable {
                reason: "durable commit executor requires durable mode",
            }));
        }
        Ok(())
    }

    fn require_no_unresolved_durable_commit(&self) -> LifecycleResult<()> {
        self.durable_gate
            .require_admission_available()
            .map_err(commit_error)
    }

    fn require_branch_commit_guard_available(&self, branch_id: BranchId) -> LifecycleResult<()> {
        self.guard_set
            .require_branch_guard_available(branch_id)
            .map_err(commit_error)
    }

    fn require_write_admission_recovery_health(&self, branch_id: BranchId) -> LifecycleResult<()> {
        if maintenance_ready_for_recovery_health(&self.current_recovery_health) {
            return Ok(());
        }
        Err(LifecycleError::StoragePressureRejected {
            branch_id,
            severity: LifecycleStoragePressureSeverity::BlockMutatingAdmission,
            pressure_reason: LifecycleStoragePressureReason::None,
            retryable: false,
            reason: "durable recovery health blocks mutating commit admission",
        })
    }

    fn require_projected_mutating_commit_budget(
        &self,
        branch_id: BranchId,
        batch: &CommitBatch,
    ) -> LifecycleResult<()> {
        let incoming_active_bytes = estimate_commit_batch_active_bytes(batch)?;
        let branch = self
            .branch_catalog
            .branch_state(branch_id)
            .expect("commit target branch is present in the catalog");
        if projected_commit_rotation_would_exceed_frozen_budget(
            &self.budget,
            branch,
            incoming_active_bytes,
        )? {
            return Err(LifecycleError::StoragePressureRejected {
                branch_id,
                severity: LifecycleStoragePressureSeverity::BlockMutatingAdmission,
                pressure_reason: LifecycleStoragePressureReason::FrozenBacklog,
                retryable: branch.frozen_table_count() > 0,
                reason: "incoming commit would exceed frozen mutable storage budget after rotation",
            });
        }
        Ok(())
    }

    #[cfg(any(test, feature = "testkit"))]
    pub(crate) fn force_close_requested_for_test(&mut self) -> LifecycleResult<()> {
        self.state
            .transition(LifecycleTransitionTrigger::CloseRequested)?;
        Ok(())
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

    #[cfg(test)]
    pub(crate) fn branch_catalog_mut_for_test(&mut self) -> &mut LifecycleBranchCatalog {
        &mut self.branch_catalog
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

    #[cfg(test)]
    pub(crate) fn catch_up_commit_frontier_for_test(
        &mut self,
        version: CommitVersion,
        timestamp: Timestamp,
    ) {
        self.allocator.catch_up_to_recovered_version(version);
        self.allocator.catch_up_to_recovered_timestamp(timestamp);
        self.visible
            .catch_up_visible_after_replay(version)
            .expect("test commit frontier must not regress");
    }

    pub(crate) const fn allocator(&self) -> &CommitFactAllocator<S> {
        &self.allocator
    }

    pub(crate) fn unresolved_durable(&self) -> LifecycleResult<Option<CommitUnresolvedDurable>> {
        self.durable_gate.unresolved().map_err(commit_error)
    }

    pub(crate) fn read_view(&self) -> LifecycleResult<BranchReadView> {
        self.read_view_for_branch(self.initial_branch_id)
    }

    pub(crate) fn read_view_for_branch(
        &self,
        branch_id: strata_core_next::BranchId,
    ) -> LifecycleResult<BranchReadView> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryRead)?;
        let branch = self.branch_catalog.branch_state(branch_id)?;
        branch.capture_read_view().map_err(branch_error)
    }

    pub(crate) fn read_latest_point_or_tombstone_for_branch(
        &self,
        branch_id: strata_core_next::BranchId,
        key: &PhysicalKey,
    ) -> LifecycleResult<Option<BranchHistoryRow>> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryRead)?;
        let branch = self.branch_catalog.branch_state(branch_id)?;
        branch
            .read_point_or_tombstone_borrowed(key, BranchReadBound::Latest)
            .map_err(branch_error)
    }

    pub(crate) fn scan_latest_including_tombstones_for_branch(
        &self,
        branch_id: strata_core_next::BranchId,
        bounds: &BranchScanBounds,
        visible_limit: Option<usize>,
    ) -> LifecycleResult<Vec<BranchHistoryRow>> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryRead)?;
        let branch = self.branch_catalog.branch_state(branch_id)?;
        branch
            .scan_including_tombstones_borrowed(
                bounds,
                BranchReadBound::Latest,
                visible_limit,
                None,
            )
            .map_err(branch_error)
    }

    /// Storage-internal: create a new branch in the catalog. The new
    /// branch is visible via `list_branches` and the catalog accessor and
    /// can accept commits routed through the catalog's per-branch slot.
    /// `publish_branch_catalog` writes the updated descriptor list to
    /// `manifest/branch-catalog` so the entry survives restart.
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
        reason = "fork-at-history surface exposed for durable callers"
    )]
    pub(crate) fn fork_current(
        &mut self,
        source: BranchId,
        destination: BranchId,
        destination_generation: CommitBranchGeneration,
    ) -> LifecycleResult<crate::lifecycle::LifecycleBranchForkOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let _quiesce = self.guard_set.try_begin_quiesce().map_err(commit_error)?;
        let outcome =
            self.branch_catalog
                .fork_current(source, destination, destination_generation)?;
        self.publish_branch_catalog()?;
        Ok(outcome)
    }

    #[allow(
        dead_code,
        reason = "fork-at-history surface exposed for durable callers"
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
        let _quiesce = self.guard_set.try_begin_quiesce().map_err(commit_error)?;
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
        let _quiesce = self.guard_set.try_begin_quiesce().map_err(commit_error)?;
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
        let _quiesce = self.guard_set.try_begin_quiesce().map_err(commit_error)?;
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
        self.publish_pending_releases()?;
        Ok(outcome)
    }

    pub(crate) fn delete_branch(
        &mut self,
        branch_id: BranchId,
        generation_guard: CommitBranchGenerationGuard,
        deleted_at: Option<CommitVersion>,
    ) -> LifecycleResult<crate::lifecycle::LifecycleBranchDeleteOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let _quiesce = self.guard_set.try_begin_quiesce().map_err(commit_error)?;
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
        self.publish_pending_releases()?;
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

    /// Publish a fresh `PendingReleasesManifest` reflecting the current
    /// in-memory buffer. Called after `clear_branch`, `delete_branch`,
    /// and after each retention drain that consumed entries. The
    /// sequence counter advances monotonically so recovery can resolve
    /// concurrent-writer scenarios across restarts.
    pub(super) fn publish_pending_releases(&mut self) -> LifecycleResult<()> {
        let entries = pending_releases_to_durable_entries(&self.pending_releases)
            .map_err(pending_releases_format_error)?;
        self.pending_releases_sequence = self.pending_releases_sequence.saturating_add(1);
        if self.pending_releases_sequence == 0 {
            return Err(LifecycleError::CheckpointPublicationFailed {
                reason: "pending releases sequence overflow",
            });
        }
        let manifest = crate::format::PendingReleasesManifest::new(
            *self.services.assembly_facts().database_id(),
            self.pending_releases_sequence,
            entries,
        )
        .map_err(pending_releases_format_error)?;
        self.services
            .pending_releases_manifest()
            .publish_replace(&manifest)
            .map_err(pending_releases_manifest_service_error)?;
        Ok(())
    }
}

fn pending_releases_to_durable_entries(
    plans: &[crate::branch::facts::BranchReleasePlan],
) -> Result<Vec<crate::format::PendingReleasesEntry>, crate::format::FormatError> {
    // Group by branch_id; multiple plans for the same branch (multiple
    // clear/delete operations between drains) merge their releasable
    // tables into a single entry. Entries are sorted by branch_id byte
    // order to match the manifest's canonical encoding.
    use std::collections::BTreeMap;
    let mut grouped: BTreeMap<[u8; 16], Vec<String>> = BTreeMap::new();
    for plan in plans {
        let key = *plan.released_branch_id().as_bytes();
        let bucket = grouped.entry(key).or_default();
        for identity in plan.releasable_tables() {
            bucket.push(identity.as_str().to_owned());
        }
    }
    let mut entries = Vec::with_capacity(grouped.len());
    for (key, mut tables) in grouped {
        tables.sort();
        tables.dedup();
        let branch_id = strata_core_next::BranchId::from_bytes(key);
        entries.push(crate::format::PendingReleasesEntry::new(branch_id, tables)?);
    }
    Ok(entries)
}

fn branch_catalog_format_error(error: crate::format::FormatError) -> LifecycleError {
    LifecycleError::lower_layer_with(
        crate::lifecycle::LifecycleLowerLayer::Format,
        "branch catalog manifest encode failed",
        error,
    )
}

fn pending_releases_format_error(error: crate::format::FormatError) -> LifecycleError {
    LifecycleError::lower_layer_with(
        crate::lifecycle::LifecycleLowerLayer::Format,
        "pending releases manifest encode failed",
        error,
    )
}

fn pending_releases_manifest_service_error(
    error: crate::service::ManifestServiceError,
) -> LifecycleError {
    LifecycleError::lower_layer_with(
        crate::lifecycle::LifecycleLowerLayer::Service,
        "pending releases manifest service failed",
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
    use crate::lifecycle::LifecycleBranchParent;
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
            if let Some(parent) = entry.parent() {
                catalog.set_parent_for_recovery(
                    initial_branch_id,
                    LifecycleBranchParent::new(
                        parent.source_branch_id(),
                        CommitVersion::new(parent.fork_version()),
                    ),
                    RecoveryExclusivityToken::new(),
                )?;
            }
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
        if let Some(parent) = entry.parent() {
            catalog.set_parent_for_recovery(
                branch_id,
                LifecycleBranchParent::new(
                    parent.source_branch_id(),
                    CommitVersion::new(parent.fork_version()),
                ),
                RecoveryExclusivityToken::new(),
            )?;
        }
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
        self.last_write_admission = None;
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
        Self::require_generation_guard(branch_id, generation, generation_guard)?;
        Self::require_durable_commit_mode(&batch)?;
        if batch.kind() == CommitBatchKind::Mutating {
            self.require_no_unresolved_durable_commit()?;
            self.require_branch_commit_guard_available(branch_id)?;
            self.require_write_admission_recovery_health(branch_id)?;
            self.evaluate_mutating_write_admission_for_branch(branch_id)?;
            self.require_projected_mutating_commit_budget(branch_id, &batch)?;
        }
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
        if outcome.is_ok() {
            self.evaluate_and_record_wal_growth_policy();
            let _ = self.schedule_post_commit_maintenance_for_branch(branch_id);
        }
        outcome
    }

    pub(crate) fn evaluate_and_record_wal_growth_policy(&mut self) {
        match self.evaluate_wal_growth_policy() {
            Ok(policy_outcome) => self.record_wal_growth_outcome(policy_outcome),
            Err(error) => self.record_wal_growth_policy_error(error),
        }
    }

    fn record_wal_growth_outcome(&mut self, outcome: LifecycleWalGrowthOutcome) {
        let facts = outcome.facts();
        let checkpoint_enqueued =
            outcome.status() == crate::lifecycle::LifecycleWalGrowthStatus::MaintenanceEnqueued;
        let checkpoint_coalesced =
            outcome.status() == crate::lifecycle::LifecycleWalGrowthStatus::MaintenanceCoalesced;
        perf_trace::record_lifecycle_wal_growth_sample(
            facts.retained_bytes(),
            facts.retained_segments(),
            checkpoint_enqueued,
            checkpoint_coalesced,
        );
        self.record_recovery_health(outcome.recovery_health());
        self.last_wal_growth_outcome = Some(outcome);
    }

    fn record_wal_growth_policy_error(&mut self, error: LifecycleError) {
        if let Ok(outcome) =
            LifecycleWalGrowthOutcome::deferred_with_health(WalGrowthFacts::empty(), 0, None, error)
        {
            self.record_wal_growth_outcome(outcome);
        }
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

/// Validate the recovered WAL package against the rebuilt catalog. Each
/// record must reference a branch present in the catalog and not in
/// `Deleted` status; unknown or deleted branches indicate corruption or
/// post-deletion resurrection attempts and must fail closed. Records
/// must remain strictly ordered by commit version across all branches —
/// the WAL is a single durable log.
fn validate_recovered_wal_package(
    catalog: &LifecycleBranchCatalog,
    records: &[WalRecord],
) -> LifecycleResult<()> {
    use crate::lifecycle::LifecycleBranchStatus;
    let mut previous = None;
    for record in records {
        let branch_id = record.branch_id();
        let descriptor = catalog
            .lookup(branch_id)
            .map_err(|_| LifecycleError::RecoveryFailed {
                reason: "recovered WAL package references an unknown branch",
            })?;
        if descriptor.status() == LifecycleBranchStatus::Deleted {
            return Err(LifecycleError::RecoveryFailed {
                reason: "recovered WAL package references a deleted branch",
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

/// Enumerate persisted per-branch table manifests and install each into
/// its catalog slot. The seeded branch's manifest was already applied by
/// the pre-catalog recovery phase; skip it. Deleted descriptors are
/// resurrection-guarded — the manifest is treated as stale.
fn recover_per_branch_table_manifests(
    services: &LifecycleDurableLocalServices<'_>,
    table_catalog: &mut crate::lifecycle::LifecycleDurableTableCatalog,
    branch_catalog: &mut LifecycleBranchCatalog,
    initial_branch_id: BranchId,
    budget: Option<&StorageBudgetLedger>,
) -> LifecycleResult<()> {
    use crate::lifecycle::LifecycleBranchStatus;
    let manifests = services
        .table_manifest()
        .load_all_current()
        .map_err(|error| {
            LifecycleError::lower_layer_with(
                crate::lifecycle::LifecycleLowerLayer::Service,
                "branch table manifest enumeration failed",
                error,
            )
        })?;
    for manifest in manifests {
        let branch_id = manifest.branch_id();
        if branch_id == initial_branch_id {
            continue;
        }
        let descriptor =
            branch_catalog
                .lookup(branch_id)
                .map_err(|_| LifecycleError::RecoveryFailed {
                    reason: "persisted table manifest references an unknown branch",
                })?;
        if descriptor.status() == LifecycleBranchStatus::Deleted {
            continue;
        }
        let generation = branch_catalog
            .registry()
            .lookup(branch_id)
            .map_err(commit_error)?
            .generation();
        let branch_state = branch_catalog
            .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))?;
        crate::lifecycle::apply_loaded_table_manifest_to_branch(
            branch_state,
            &manifest,
            services.table_reader(),
            table_catalog,
            budget,
        )?;
    }
    Ok(())
}

/// Install checkpoint rows that did not belong to the seeded branch.
/// `recover_checkpoint` partitions rows by `branch_id` at decode time
/// and returns non-seeded rows in the recovery outcome; this helper
/// drives them into the catalog's per-branch slots after the catalog
/// has been rebuilt from `BranchCatalogManifest` and per-branch table
/// manifests.
///
/// Validation: each row's `branch_id` must be present in the catalog
/// and not in `Deleted` status. Unknown or deleted `branch_ids` fail
/// closed with typed `RecoveryFailed` errors, mirroring the multi-branch
/// WAL validator. Empty input is a no-op (common when the seeded branch
/// is the only branch with checkpoint coverage).
fn install_non_seeded_checkpoint_rows(
    branch_catalog: &mut LifecycleBranchCatalog,
    rows: &[crate::row::StorageRow],
    identity_seed: Option<&crate::table::TableIdentity>,
) -> LifecycleResult<()> {
    use crate::lifecycle::LifecycleBranchStatus;
    if rows.is_empty() {
        return Ok(());
    }
    let Some(identity_seed) = identity_seed else {
        return Err(LifecycleError::RecoveryFailed {
            reason: "non-seeded checkpoint rows require an install identity seed",
        });
    };
    let mut affected: Vec<BranchId> = Vec::new();
    for row in rows {
        let id = row.physical_key().branch_id();
        if !affected.contains(&id) {
            affected.push(id);
        }
    }
    affected.sort_by_key(|id| *id.as_bytes());
    for id in &affected {
        let descriptor =
            branch_catalog
                .lookup(*id)
                .map_err(|_| LifecycleError::RecoveryFailed {
                    reason: "checkpoint references an unknown branch",
                })?;
        if descriptor.status() == LifecycleBranchStatus::Deleted {
            return Err(LifecycleError::RecoveryFailed {
                reason: "checkpoint references a deleted branch",
            });
        }
    }
    let mut staged: Vec<BranchLocalState> = affected
        .iter()
        .map(|id| branch_catalog.branch_state(*id).cloned())
        .collect::<LifecycleResult<Vec<_>>>()?;
    let request = crate::branch::state::snapshot::BranchSnapshotInstallRequest::from_rows(
        identity_seed.as_str(),
        rows.to_vec(),
    )
    .map_err(branch_error)?;
    crate::branch::state::snapshot::install_snapshot_rows_into_branches(&mut staged, &request)
        .map_err(branch_error)?;
    for branch in staged {
        let branch_id = branch.branch_id();
        let descriptor = branch_catalog.lookup(branch_id)?;
        branch_catalog.replace_active_branch_state_with_descriptor(descriptor, branch)?;
    }
    Ok(())
}

fn rebuild_fork_snapshot_rows(branch_catalog: &mut LifecycleBranchCatalog) -> LifecycleResult<()> {
    let descriptors = branch_catalog
        .list_branches(false)
        .into_iter()
        .filter(|descriptor| descriptor.parent().is_some())
        .collect::<Vec<_>>();
    if descriptors.is_empty() {
        return Ok(());
    }

    for _ in 0..descriptors.len() {
        let mut appended_in_pass = 0;
        for descriptor in &descriptors {
            let Some(parent) = descriptor.parent() else {
                continue;
            };
            let rows = branch_catalog
                .branch_state(parent.source_branch_id())?
                .fork_snapshot_rows(parent.fork_version(), descriptor.branch_id())
                .map_err(branch_error)?;
            if rows.is_empty() {
                continue;
            }

            let mut staged = branch_catalog.branch_state(descriptor.branch_id())?.clone();
            let mut appended_for_branch = 0;
            for row in rows {
                match staged.append_committed_row(row) {
                    Ok(_) => appended_for_branch += 1,
                    Err(BranchRuntimeError::TableRuntime {
                        source: TableRuntimeError::DuplicateInternalKey { .. },
                    }) => {}
                    Err(error) => return Err(branch_error(error)),
                }
            }
            if appended_for_branch > 0 {
                let current_descriptor = branch_catalog.lookup(descriptor.branch_id())?;
                branch_catalog
                    .replace_active_branch_state_with_descriptor(current_descriptor, staged)?;
                appended_in_pass += appended_for_branch;
            }
        }
        if appended_in_pass == 0 {
            return Ok(());
        }
    }
    Ok(())
}

/// Replay WAL records against the rebuilt catalog. Each record is routed
/// to its branch's slot in the catalog. After all records replay, advance
/// the allocator and the visibility tracker to the highest version
/// observed (across all branches, including the checkpoint watermark).
fn replay_wal_into_catalog<S>(
    branch_catalog: &mut LifecycleBranchCatalog,
    commit_config: &crate::commit::CommitRuntimeConfig,
    allocator: &mut CommitFactAllocator<S>,
    visible: &mut VisibleVersionTracker,
    durable_gate: &CommitUnresolvedDurableGate,
    recovery: &LifecycleRecoveryOutcome,
    durability: CommitDurabilityClass,
    checkpoint_watermark: CommitVersion,
) -> LifecycleResult<LifecycleRecoveryBootstrapReport> {
    let mut report = LifecycleRecoveryBootstrapReport::new(recovery.health().clone());
    let mut replayed_max = CommitVersion::ZERO;
    for record in recovery.wal().records() {
        replayed_max = replayed_max.max(record.commit_version());
        let branch_id = record.branch_id();
        let generation = branch_catalog
            .registry()
            .lookup(branch_id)
            .map_err(commit_error)?
            .generation();
        let target_branch = branch_catalog
            .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))?;
        let replay = CommitReplayRequest::new(record.clone(), durability);
        let replay_report = CommitReplayRuntime::new(
            commit_config,
            allocator,
            target_branch,
            visible,
            durable_gate,
        )
        .replay(&replay)
        .map_err(commit_error)?;
        report.record_replay(&replay_report);
    }
    let recovered_visible_version = checkpoint_watermark.max(replayed_max);
    allocator.catch_up_to_recovered_version(recovered_visible_version);
    if let Some(timestamp) = recovery.checkpoint().timestamp_max() {
        allocator.catch_up_to_recovered_timestamp(timestamp);
    }
    let checkpoint_visible_publish = if recovered_visible_version > visible.visible_version() {
        Some(
            visible
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
