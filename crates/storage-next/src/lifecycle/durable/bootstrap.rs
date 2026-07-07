//! Commit-runtime bootstrap after durable recovery.

use super::{
    branch_error, commit_error, require_admitted, LifecycleDurableLocalServices,
    LifecycleDurableLocalShell,
};
use crate::branch::error::BranchRuntimeError;
use crate::branch::read::{BranchHistoryRow, BranchReadBound, BranchReadView, BranchScanBounds};
use crate::branch::snapshot::{BranchSnapshotPublisher, BranchSnapshotRegistry};
use crate::branch::state::BranchLocalState;
use crate::commit::{
    finalize_commit_group, CommitBatch, CommitBatchKind, CommitBranchGeneration,
    CommitBranchGenerationGuard, CommitBranchGuardSet, CommitDurabilityClass, CommitDurabilityMode,
    CommitDurableRuntime, CommitFactAllocator, CommitGroupState, CommitManualTimestampSource,
    CommitOutcome, CommitReplayAction, CommitReplayRequest, CommitReplayRuntime,
    CommitRuntimeError, CommitTimestampSource, CommitUnresolvedDurable,
    CommitUnresolvedDurableGate, VisibleVersionPublish, VisibleVersionTracker,
};
use crate::format::WalRecord;
use crate::lifecycle::admission_ramp::{
    admission_mode_from_env, LifecycleAdmissionMode, WriteRateBucket,
};
use crate::lifecycle::background::{MaintenanceClock, RealMaintenanceClock};
use crate::lifecycle::{
    branch_resident_bytes, compaction_lane_cap, estimate_commit_batch_active_bytes,
    maintenance_ready_for_recovery_health, projected_commit_rotation_would_exceed_frozen_budget,
    BudgetedCommitBranch, LifecycleBranchCatalog, LifecycleDurableTableCatalog, LifecycleError,
    LifecycleMaintenanceExecutor, LifecycleOperationKind, LifecycleRecoveryOutcome,
    LifecycleResult, LifecycleState, LifecycleStateMachine, LifecycleStats,
    LifecycleStoragePressureReason, LifecycleStoragePressureSeverity, LifecycleTransitionTrigger,
    LifecycleWalGrowthOutcome, LifecycleWalGrowthTrigger, LifecycleWriteAdmissionOutcome,
    RecoveryExclusivityToken, RecoveryHealth, RuntimeReadHandles, StorageBudgetLedger,
    StorageBudgetPressureSeverity, StorageBudgetSnapshot, StorageMode, StorageOpenOutcome,
    StorageOpenPlan,
};
use crate::observability::perf_trace;
use crate::row::PhysicalKey;
use crate::service::WalGrowthFacts;
use crate::table::TableRuntimeError;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use strata_core_next::{BranchId, CommitVersion, Timestamp};

/// Cached database-manifest retention watermark (`max(snapshot_watermark,
/// flushed_through_commit_id)`), the value the per-commit growth/backpressure
/// checks need to derive `commits_since_checkpoint`. `Unknown` means it must be
/// refreshed from the manifest on the next read; `Known` is memoized until a
/// checkpoint/flush completion invalidates it. Avoids a manifest read per commit.
#[derive(Clone, Copy, Debug)]
pub(super) enum CachedRetentionWatermark {
    Unknown,
    Known(Option<CommitVersion>),
}

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
    /// BS2.2: release-published mirror of `visible.visible_version()` so a future off-lock reader
    /// (BS2.4) observes the visibility bound without the runtime lock. Stored under the lock on
    /// commit success; initial value tracks the recovered visible version (`0` == `CommitVersion::ZERO`).
    pub(super) visible_commit_version: Arc<AtomicU64>,
    /// BS2.3: per-branch published `Arc<BranchReadView>` snapshots. Published under the lock at the
    /// mutation sites; in BS2.3 read only by the debug equivalence oracle (reads still lock).
    pub(super) snapshot_publisher: BranchSnapshotPublisher,
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
    // BS3.4b graded write-admission (dark behind STRATA_ADMISSION). The debt-adaptive write rate is
    // recomputed at structural-change events (`republish_all_branch_snapshots`, which both the inline
    // and background install paths converge on) and enforced per-commit by the token bucket;
    // `admission_clock` times both. `Cell` interior mutability keeps the commit/read paths `&self`,
    // exactly like `retention_watermark` — the runtime is `!Sync` behind the runtime mutex.
    pub(super) admission_mode: LifecycleAdmissionMode,
    pub(super) admission_clock: Arc<dyn MaintenanceClock>,
    pub(super) admission_current_rate: Cell<u64>,
    pub(super) admission_last_debt: Cell<u64>,
    pub(super) admission_bucket: Cell<WriteRateBucket>,
    // Cached manifest retention watermark for the per-commit growth/backpressure
    // checks. `Cell` keeps the read paths `&self` (no `&mut` cascade); invalidated
    // at checkpoint/flush completion and refreshed lazily. Interior mutability
    // makes the runtime `!Sync`, which is fine behind the runtime mutex (only
    // `Send` is required, as for the WAL service's `sealed_retention` cache).
    pub(super) retention_watermark: Cell<CachedRetentionWatermark>,
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
    #[allow(
        clippy::too_many_lines,
        reason = "recovery assembly: replay report, open outcome, and runtime construction in one flow"
    )]
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
        // Seed the retention-watermark cache from the open-time manifest facts so
        // the first commit is a cache hit rather than a cold manifest read.
        let retention_watermark_seed = crate::lifecycle::wal_retention_watermark(
            self.assembly_facts()
                .manifest_snapshot_watermark()
                .map(CommitVersion::new),
            self.assembly_facts().manifest_flush_watermark(),
        );
        // BS3.4b: seed the graded-admission rate at the un-throttled ceiling and the token bucket at
        // the current clock. Production uses the real clock; tests swap in a manual clock via
        // `with_admission_clock_for_test`. `admission_mode` defaults to `Legacy` unless STRATA_ADMISSION.
        let admission_clock: Arc<dyn MaintenanceClock> = Arc::new(RealMaintenanceClock::new());
        let admission_initial_rate = self
            .open_plan
            .lifecycle_config()
            .write_throttle_policy()
            .max_rate_bytes_per_sec();
        let admission_now = admission_clock.now();
        let mut runtime = LifecycleDurableLocalRuntime {
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
            visible_commit_version: visible_version_mirror(self.visible),
            snapshot_publisher: BranchSnapshotPublisher::new(),
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
            admission_mode: admission_mode_from_env(),
            admission_clock,
            admission_current_rate: Cell::new(admission_initial_rate),
            admission_last_debt: Cell::new(0),
            admission_bucket: Cell::new(WriteRateBucket::new(admission_now)),
            retention_watermark: Cell::new(CachedRetentionWatermark::Known(
                retention_watermark_seed,
            )),
            pending_releases,
            branch_catalog_sequence,
            pending_releases_sequence,
            branch_publish_locks: HashMap::new(),
            maintenance: Self::build_maintenance_executor(max_maintenance_queue_depth)?,
            maintenance_coverage_idle_rounds: 0,
            close_retry_state: None,
        };
        // BS2.3: seed a published snapshot for every recovered branch before any read observes it.
        runtime.republish_all_branch_snapshots();
        // Table-object GC reconcile on REOPEN only: one coalescing mark covers the whole prior
        // session's backlog (the mark lists the global inventory against every current manifest),
        // so stale objects from crashes or pre-GC sessions are reclaimed instead of persisting
        // forever. A freshly created database has no prior backlog to reconcile. Best-effort: a
        // rejected enqueue only defers reclaim to the next publish-driven cycle.
        if runtime.open_outcome.disposition()
            == crate::lifecycle::StorageOpenDisposition::OpenedExisting
        {
            let _ = runtime.enqueue_maintenance(
                crate::lifecycle::MaintenanceTaskRequest::table_object_retention(
                    runtime.initial_branch_id,
                ),
            );
        }
        Ok(runtime)
    }

    /// Build the maintenance executor with the durable runtime's Rewrite-lane concurrency cap
    /// (env-tunable during the perf sweep; see [`compaction_lane_cap`]).
    fn build_maintenance_executor(
        max_queue_depth: usize,
    ) -> LifecycleResult<LifecycleMaintenanceExecutor> {
        let mut maintenance = LifecycleMaintenanceExecutor::new(max_queue_depth)?;
        maintenance.set_rewrite_lane_cap(compaction_lane_cap());
        Ok(maintenance)
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

impl<S> RuntimeReadHandles for LifecycleDurableLocalRuntime<'_, S> {
    fn visible_handle(&self) -> Arc<AtomicU64> {
        Arc::clone(&self.visible_commit_version)
    }

    fn snapshot_registry(&self) -> Arc<BranchSnapshotRegistry> {
        self.snapshot_publisher.registry_handle()
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
        // Refresh the database-wide total so diagnostics report a current global figure alongside
        // the per-pool snapshot.
        self.refresh_runtime_memory_total();
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

    pub(crate) fn budget_total_used_bytes(&self) -> u64 {
        self.budget.total_used_bytes()
    }

    /// The isolated database-wide runtime memory total (branch resident bytes + block cache),
    /// without the in-flight ledger pool reservations that `budget_total_used_bytes` adds.
    /// Test-only: lets the budget drift test assert the published total against an independent
    /// full fold.
    #[cfg(test)]
    pub(crate) fn runtime_total_bytes(&self) -> u64 {
        self.budget.runtime_total_bytes()
    }

    pub(crate) fn budget_global_pressure(&self) -> StorageBudgetPressureSeverity {
        self.budget.global_pressure()
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

    /// Returns the cached manifest retention watermark, refreshing it from the
    /// manifest only when a checkpoint/flush has invalidated it. This is the sole
    /// manifest read left on the per-commit growth/backpressure path; steady-state
    /// commits hit the memoized value.
    pub(crate) fn cached_retention_watermark(&self) -> LifecycleResult<Option<CommitVersion>> {
        if let CachedRetentionWatermark::Known(watermark) = self.retention_watermark.get() {
            return Ok(watermark);
        }
        let manifest_start = perf_trace::start_timer();
        let manifest_result = self.services.manifest().load_required();
        perf_trace::record_commit_wal_growth_manifest_elapsed(manifest_start);
        let current_manifest = manifest_result.map_err(super::manifest_error)?;
        let watermark = crate::lifecycle::wal_retention_watermark(
            current_manifest
                .snapshot_watermark()
                .map(CommitVersion::new),
            current_manifest.flushed_through_commit_id(),
        );
        self.retention_watermark
            .set(CachedRetentionWatermark::Known(watermark));
        Ok(watermark)
    }

    /// Invalidates the cached retention watermark so the next growth/backpressure
    /// check re-reads it from the manifest. Called at every checkpoint/flush
    /// completion — the only operations that advance the watermark.
    pub(crate) fn invalidate_retention_watermark_cache(&self) {
        self.retention_watermark
            .set(CachedRetentionWatermark::Unknown);
    }

    pub(crate) fn current_wal_growth_backpressure_snapshot(
        &self,
    ) -> LifecycleResult<(WalGrowthFacts, u64, Option<LifecycleWalGrowthTrigger>)> {
        let policy = self.open_plan.lifecycle_config().wal_growth_policy();
        let facts = self.current_wal_growth_facts()?;
        let commits_since_checkpoint = crate::lifecycle::commits_since_checkpoint(
            self.visible.visible_version(),
            self.cached_retention_watermark()?,
        );
        let trigger = policy.backpressure_trigger_for(facts, commits_since_checkpoint);
        Ok((facts, commits_since_checkpoint, trigger))
    }

    /// Whether retained WAL has exceeded the hard disk-safety cap (`max_total_wal_bytes`).
    /// When true the foreground WAL pacing must not give up — the WAL must never keep growing
    /// past this ceiling — so the writer waits on background reclaim until it drops back below.
    pub(crate) fn current_wal_growth_exceeds_hard_cap(&self) -> LifecycleResult<bool> {
        let policy = self.open_plan.lifecycle_config().wal_growth_policy();
        Ok(policy.hard_cap_exceeded(self.current_wal_growth_facts()?))
    }

    pub(crate) const fn last_write_admission(&self) -> Option<LifecycleWriteAdmissionOutcome> {
        self.last_write_admission
    }

    /// BS3.4b: whether the durable runtime is on the graded (debt-adaptive rate) admission path.
    /// The commit path branches on this to pick the token-bucket delay vs. the legacy quadratic.
    pub(crate) fn is_graded_admission(&self) -> bool {
        self.admission_mode == LifecycleAdmissionMode::Graded
    }

    /// BS3.4b (graded): recompute the debt-adaptive write rate at an install event. The rate is
    /// updated *only* here (event cadence — the `RocksDB` `InstallSuperVersion` analog); the commit
    /// path just reads it. The single global rate paces the writer to the most-behind branch (max
    /// compaction debt). No-op on the legacy path.
    pub(crate) fn recompute_admission_rate(&self) {
        if self.admission_mode != LifecycleAdmissionMode::Graded {
            return;
        }
        let mut worst_debt = 0u64;
        let mut worst_l0 = 0usize;
        for descriptor in self.branch_catalog.list_branches(false) {
            let Ok(branch) = self.branch_catalog.branch_state(descriptor.branch_id()) else {
                continue;
            };
            let per_level = branch.per_level_bytes();
            let targets =
                crate::lifecycle::compaction::nonzero_level_targets_from_level_bytes(per_level);
            let debt = crate::lifecycle::admission_ramp::compaction_debt(per_level, &targets);
            let l0 = branch.owned_levels().first().map_or(0, Vec::len);
            if debt > worst_debt || (debt == worst_debt && l0 > worst_l0) {
                worst_debt = debt;
                worst_l0 = l0;
            }
        }
        // The rate ramps *only* inside the L0 delay band (>= the slowdown/urgent grade). Below it,
        // feed the ramp zero debt so it recovers (×1.4) back toward the un-throttled ceiling; the
        // commit path then applies no pacing (rate == max). Within the band the debt direction
        // (growing/shrinking as L0 fills/drains) drives ×0.8 vs ×1.25.
        let grade_active = worst_l0 >= crate::lifecycle::compaction::level_zero_urgent_threshold();
        let effective_debt = if grade_active { worst_debt } else { 0 };
        let policy = self.open_plan.lifecycle_config().write_throttle_policy();
        let new_rate = crate::lifecycle::admission_ramp::next_write_rate(
            self.admission_current_rate.get(),
            effective_debt,
            self.admission_last_debt.get(),
            worst_l0,
            crate::lifecycle::compaction::level_zero_blocking_threshold(),
            policy.max_rate_bytes_per_sec(),
            policy.min_rate_bytes_per_sec(),
        );
        self.admission_current_rate.set(new_rate);
        self.admission_last_debt.set(effective_debt);
    }

    /// BS3.4b (graded): the per-commit token-bucket delay in millis. `0` on the legacy path, and `0`
    /// while the rate sits at the un-throttled ceiling (outside the delay band); otherwise the token
    /// bucket paces this commit to the current rate.
    pub(crate) fn graded_write_throttle_delay_millis(&self, batch_bytes: u64) -> u64 {
        if self.admission_mode != LifecycleAdmissionMode::Graded {
            return 0;
        }
        let policy = self.open_plan.lifecycle_config().write_throttle_policy();
        let rate = self.admission_current_rate.get();
        if rate >= policy.max_rate_bytes_per_sec() {
            return 0;
        }
        let mut bucket = self.admission_bucket.get();
        let delay = bucket.charge(batch_bytes, rate, self.admission_clock.now());
        self.admission_bucket.set(bucket);
        // Cap a single commit's sleep: at the near-stop floor rate a large batch would otherwise pace
        // for seconds (batch_bytes / 16 KiB/s). Beyond the cap, admit the write — it grows L0 toward
        // the hard stop, which then rejects with a bounded retry-wait instead of a multi-second sleep.
        u64::try_from(delay.as_millis())
            .unwrap_or(u64::MAX)
            .min(policy.max_graded_delay_millis())
    }

    #[cfg(test)]
    pub(crate) fn with_admission_mode_for_test(&mut self, mode: LifecycleAdmissionMode) {
        self.admission_mode = mode;
    }

    #[cfg(test)]
    pub(crate) fn admission_current_rate_for_test(&self) -> u64 {
        self.admission_current_rate.get()
    }

    #[cfg(test)]
    pub(crate) fn with_admission_clock_for_test(&mut self, clock: Arc<dyn MaintenanceClock>) {
        let now = clock.now();
        self.admission_clock = clock;
        self.admission_bucket.set(WriteRateBucket::new(now));
        self.admission_current_rate.set(
            self.open_plan
                .lifecycle_config()
                .write_throttle_policy()
                .max_rate_bytes_per_sec(),
        );
        self.admission_last_debt.set(0);
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
        // BS4.5a: database-wide memory budget as an *observability gauge*, not an admission failure.
        // After the disk-resident flip (BS4.4j/4.4l), durable tables hold lazy readers whose residency
        // is metadata-only, so a durable dataset is no longer bounded by RAM — the old hard reject
        // predated lazy block reads and only made sense while whole-object readers materialized every
        // table. `refresh_runtime_memory_total` records the measured residency (readable via
        // diagnostics); when it exceeds the budget we count the over-budget admission so health
        // monitoring can WARN, then admit. Cache mode keeps its hard reject (constraint C2), and
        // memtable/frozen pressure is still enforced by the rotation check below.
        self.refresh_runtime_memory_total();
        if self.budget.would_exceed_total(incoming_active_bytes) {
            perf_trace::record_durable_commit_admitted_over_budget();
        }
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

    /// Recompute and record the database-wide runtime memory total: the resident bytes of every
    /// active branch (memtables plus owned-table readers) plus the block cache. The
    /// commit-admission path and diagnostics read this so a per-branch check can reason about the
    /// whole database against `budget.total_bytes()`.
    pub(crate) fn refresh_runtime_memory_total(&self) {
        let resident =
            self.branch_catalog
                .list_branches(false)
                .iter()
                .fold(0u64, |total, descriptor| {
                    self.branch_catalog
                        .branch_state(descriptor.branch_id())
                        .map_or(total, |branch| {
                            total.saturating_add(branch_resident_bytes(branch))
                        })
                });
        let cache = self.services.table_object().block_cache_resident_bytes();
        self.budget
            .set_runtime_total_bytes(resident.saturating_add(cache));
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
        // Keep the release-published mirror in lockstep with the tracker (BS2.2): bounded
        // Latest reads load the atomic, so a tracker-only advance would strand them.
        self.visible_commit_version
            .store(self.visible.visible_version().as_u64(), Ordering::Release);
    }

    pub(crate) const fn allocator(&self) -> &CommitFactAllocator<S> {
        &self.allocator
    }

    pub(crate) fn unresolved_durable(&self) -> LifecycleResult<Option<CommitUnresolvedDurable>> {
        self.durable_gate.unresolved().map_err(commit_error)
    }

    /// BS2.3: capture and publish a fresh snapshot for `branch_id` (under the runtime lock). The
    /// capture is O(#tables) refcount-bumps (BS2.1); on capture error — infallible on a well-formed
    /// post-mutation state — the prior snapshot is kept. Called at every branch mutation site.
    pub(super) fn publish_branch_snapshot(&mut self, branch_id: BranchId) {
        let view = match self.branch_catalog.branch_state(branch_id) {
            Ok(state) => match state.capture_snapshot() {
                Ok(view) => Arc::new(view),
                Err(_error) => {
                    debug_assert!(
                        false,
                        "snapshot capture failed post-mutation for {branch_id:?}"
                    );
                    return;
                }
            },
            // Branch is gone (deleted / not yet installed); nothing to publish.
            Err(_) => return,
        };
        self.snapshot_publisher.publish_view(branch_id, view);
    }

    /// BS2.3: (re)publish a snapshot for every active branch. Used at construction (so a read
    /// before any mutation finds a snapshot) and after multi-branch or hard-to-attribute mutations
    /// (maintenance rewrites, fork's new child) — idempotent, and branch count is small.
    pub(super) fn republish_all_branch_snapshots(&mut self) {
        for descriptor in self.branch_catalog.list_branches(false) {
            self.publish_branch_snapshot(descriptor.branch_id());
        }
        // BS3.4b: every structural change (rotation / flush install / compaction install) republishes
        // here — the RocksDB `InstallSuperVersion` analog and the event-cadence point where the
        // graded-admission write rate is recomputed from the settled branch shapes. A flush that grew
        // L0 raises debt (rate down); a compaction that drained it lowers debt (rate recovers). No-op
        // on the legacy path.
        self.recompute_admission_rate();
    }

    /// BS2.3: drop a branch's slot on delete. A racing reader completes on the snapshot it holds.
    fn remove_branch_snapshot(&mut self, branch_id: BranchId) {
        self.snapshot_publisher.remove(branch_id);
    }

    /// BS2.3 test seam: republish after a direct `branch_state_mut` manipulation that bypasses the
    /// commit/maintenance publish sites (used by tests that synthesize a branch state).
    #[cfg(test)]
    pub(crate) fn publish_branch_snapshot_for_test(&mut self, branch_id: BranchId) {
        self.publish_branch_snapshot(branch_id);
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

    /// BS2.2: the visibility bound for a Latest read — `visible` loaded from the release-published
    /// atomic — with a debug check that bounding by it drops nothing under the lock (a no-op except
    /// in the deliberate `applied_not_visible` state, where the durable gate is tripped).
    fn latest_visibility_bound(&self, branch: &BranchLocalState) -> BranchReadBound {
        let visible = CommitVersion::new(self.visible_commit_version.load(Ordering::Acquire));
        debug_assert!(
            !self.durable_gate.is_clean()
                || branch
                    .max_commit_version()
                    .is_none_or(|max| max.as_u64() <= visible.as_u64()),
            "BS2.2 visibility bound must be a no-op under the lock: branch max {:?} > visible {:?} with a clean gate",
            branch.max_commit_version(),
            visible,
        );
        BranchReadBound::at_version(visible)
    }

    pub(crate) fn read_latest_point_or_tombstone_for_branch(
        &self,
        branch_id: strata_core_next::BranchId,
        key: &PhysicalKey,
    ) -> LifecycleResult<Option<BranchHistoryRow>> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryRead)?;
        let branch = self.branch_catalog.branch_state(branch_id)?;
        let bound = self.latest_visibility_bound(branch);
        branch
            .read_point_or_tombstone_borrowed(key, bound)
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
        let bound = self.latest_visibility_bound(branch);
        branch
            .scan_including_tombstones_borrowed(bounds, bound, visible_limit, None)
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
        self.republish_all_branch_snapshots(); // BS2.3: publish the new/forked branch snapshot.
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
    /// Publish the forked child's table manifest at fork time, durably recording its COW
    /// inherited-layer references. This is what makes the fork's parent-table references visible
    /// to manifest-based recovery (O(tables) reopen instead of the O(parent dataset)
    /// `rebuild_fork_snapshot_rows` re-materialization) and to table-object reachability (the
    /// child's references pin shared parent objects against GC across reopen — the COW invariant:
    /// an object is deletable only when unreachable from *every* branch's durable manifest).
    ///
    /// Skipped for eager (layer-less) children: their fork rows live only in the child memtable,
    /// so an empty manifest would make recovery treat the child as durably covered and skip the
    /// rebuild fallback — data loss on crash. Publish failure does not fail the fork (the catalog
    /// is already durable); it records health debt, and the child recovers through the gated
    /// rebuild fallback instead.
    fn publish_fork_child_table_manifest(
        &mut self,
        outcome: &crate::lifecycle::LifecycleBranchForkOutcome,
    ) {
        if outcome.inherited_layer_count() == 0 {
            return;
        }
        let publish_result = self
            .branch_catalog
            .branch_state(outcome.descriptor().branch_id())
            .map_err(branch_error)
            .and_then(|child| {
                crate::lifecycle::table_manifest::publish_table_manifest_for_branch_with_budget(
                    child,
                    self.services.table_manifest(),
                    &mut self.table_catalog,
                    Some(&self.budget),
                )
                .map(|_| ())
            });
        if publish_result.is_err() {
            if let Ok(health) = crate::lifecycle::telemetry_health_debt(
                "fork child table manifest publish failed; recovery falls back to fork rebuild",
            ) {
                self.record_recovery_health(Some(&health));
            }
        }
    }

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
        self.publish_fork_child_table_manifest(&outcome);
        self.republish_all_branch_snapshots(); // BS2.3: publish the new/forked branch snapshot.
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
        self.publish_fork_child_table_manifest(&outcome);
        self.republish_all_branch_snapshots(); // BS2.3: publish the new/forked branch snapshot.
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
        self.publish_fork_child_table_manifest(&outcome);
        self.republish_all_branch_snapshots(); // BS2.3: publish the new/forked branch snapshot.
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
        // Table-object GC: the buffered release plan (and the refs the clear dropped) are
        // reclaimed by the retention → quarantine → purge cycle. Best-effort, coalescing.
        let _ = self.enqueue_maintenance(
            crate::lifecycle::MaintenanceTaskRequest::table_object_retention(branch_id),
        );
        // BS2.3: clear reset the branch to an empty state; republish the (now empty) snapshot.
        self.publish_branch_snapshot(branch_id);
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
        // Table-object GC: the deleted branch's now-unreachable objects are reclaimed by the
        // retention → quarantine → purge cycle. Best-effort, coalescing.
        let _ = self.enqueue_maintenance(
            crate::lifecycle::MaintenanceTaskRequest::table_object_retention(branch_id),
        );
        // BS2.3: the branch is tombstoned; drop its snapshot slot.
        self.remove_branch_snapshot(branch_id);
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

/// Per-member result of a write group (BS5.1): the commit outcome plus the member's write
/// admission snapshot, captured immediately after the member executed so the API layer can
/// apply the same per-caller post-commit handling as a solo commit.
#[derive(Debug)]
pub(crate) struct DurableGroupMemberResult {
    pub(crate) outcome: LifecycleResult<CommitOutcome>,
    pub(crate) admission: Option<LifecycleWriteAdmissionOutcome>,
}

/// Which durable-gate check commit admission runs (BS5.1): a solo commit requires the gate
/// admission span to be available; a group member runs inside the leader's active span.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DurableGateAdmission {
    Solo,
    Member,
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
        let branch_id = batch.branch_id();
        let generation =
            self.admit_durable_commit(&batch, generation_guard, DurableGateAdmission::Solo)?;
        let frozen_before = self
            .branch_catalog
            .branch_state(branch_id)?
            .frozen_table_count();
        let outcome = {
            let setup_timer = perf_trace::start_timer();
            let (branch, registry) = self.branch_catalog.branch_state_mut_with_registry(
                branch_id,
                CommitBranchGenerationGuard::exact(generation),
            )?;
            let mut budgeted_branch = BudgetedCommitBranch::new(branch, &self.budget);
            let mut runtime = CommitDurableRuntime::new(
                &self.commit_config,
                registry,
                &self.guard_set,
                &mut self.allocator,
                &mut budgeted_branch,
                &mut self.visible,
                &mut self.services.wal,
                &self.durable_gate,
            );
            perf_trace::record_commit_setup_elapsed(setup_timer);
            runtime
                .execute(batch, generation_guard)
                .map_err(commit_error)
        };
        // Commit-triggered auto-rotation is a STRUCTURAL change: per the Model-2 contract
        // below, it must republish the snapshot in the same lock hold — the published view's
        // live-active handle now points at the rotated-out (frozen) table, so later commits'
        // rows in the fresh active would otherwise be invisible until the next background
        // publish. Republish BEFORE advancing the atomic mirror so any reader observing the
        // new visible version finds a covering snapshot (V-before-S).
        if outcome.is_ok() {
            self.republish_branch_snapshot_after_rotation(branch_id, frozen_before)?;
            // BS2.2: mirror the just-advanced visible version to the atomic (release) so off-lock
            // readers (BS2.4) observe it without the runtime lock. On the `applied_not_visible`
            // error path `outcome` is `Err`, so the atomic correctly does not advance.
            self.finish_durable_commit_post_publish(branch_id);
        }
        // BS2.4 Model 2: commits do NOT republish the snapshot (except the rotation case above).
        // The published snapshot holds the live (unpinned) active handle, so it already sees this
        // commit's appends; each off-lock read pins the active at read time and bounds by the
        // visible version. Only structural changes (rotation/flush/compaction/materialization/
        // fork/lifecycle) republish.
        outcome
    }

    /// Execute a write group (BS5.1): the caller is the group leader holding the runtime lock.
    /// Members execute sequentially in member order — WAL order equals version order equals
    /// member order — under one durable-gate admission span, deferring fsync and visible
    /// publish to one finalize. Pre-WAL failures are clean per-member rejections; a post-WAL
    /// failure is group-fatal: every member that reached the WAL reports durability-uncertain
    /// and recovery reconciles the group's version range through the widened gate fact.
    pub(crate) fn execute_durable_commit_group(
        &mut self,
        members: Vec<(CommitBatch, CommitBranchGenerationGuard)>,
    ) -> Vec<DurableGroupMemberResult> {
        let mut results: Vec<DurableGroupMemberResult> = Vec::with_capacity(members.len());
        // The leader's whole-group admission span. Failure (an unresolved fact) rejects every
        // member with the same typed error a solo commit would get from the gate.
        if self.durable_gate.begin_group_admission().is_err() {
            for _ in &members {
                results.push(DurableGroupMemberResult {
                    outcome: Err(self.group_admission_unavailable_error()),
                    admission: None,
                });
            }
            return results;
        }
        let mut group = CommitGroupState::new(self.visible.visible_version());
        let mut fatal = false;
        let mut touched_branches: Vec<BranchId> = Vec::new();
        for (batch, generation_guard) in members {
            if fatal {
                results.push(DurableGroupMemberResult {
                    outcome: Err(commit_error(CommitRuntimeError::DurabilityUnavailable {
                        reason: "write group aborted before this member's WAL append",
                    })),
                    admission: None,
                });
                continue;
            }
            let branch_id = batch.branch_id();
            let outcome = self.execute_group_member_commit(batch, generation_guard, &mut group);
            if outcome.is_ok() && !touched_branches.contains(&branch_id) {
                touched_branches.push(branch_id);
            }
            if outcome.is_err() {
                // A recorded fact means the failure happened after a WAL append
                // (durable-not-applied) — group-fatal. Pre-WAL rejections leave
                // the gate clean and the group continues.
                fatal = matches!(self.durable_gate.unresolved(), Ok(Some(_)) | Err(_));
            }
            results.push(DurableGroupMemberResult {
                outcome,
                admission: self.last_write_admission,
            });
        }
        if !fatal {
            let finalize = finalize_commit_group(
                &group,
                &mut self.services.wal,
                &mut self.visible,
                &self.durable_gate,
            );
            fatal = finalize.is_err();
        }
        self.durable_gate.end_group_admission();
        if fatal {
            // Group-fatal: nothing was published. Members that reached the WAL are covered by
            // the widened gate fact (or a halted writer) and must not be acked — replay
            // reconciles them idempotently after reopen.
            for result in &mut results {
                if let Ok(outcome) = &result.outcome {
                    // A successful group member is always a mutating visible outcome
                    // with an allocated version; ZERO is unreachable and only
                    // satisfies the type.
                    let commit_version = outcome.commit_version().unwrap_or(CommitVersion::ZERO);
                    result.outcome = Err(commit_error(CommitRuntimeError::DurabilityUncertain {
                        branch_id: outcome.branch_id(),
                        commit_version,
                        reason: "write group failed after this member's WAL append; \
                                 replay must reconcile",
                        source: None,
                    }));
                }
            }
            return results;
        }
        if group.last_stamp().is_some() {
            self.mirror_visible_and_evaluate_wal_growth();
            for branch_id in touched_branches {
                self.schedule_post_commit_maintenance_best_effort(branch_id);
            }
        }
        results
    }

    /// Execute one group member: solo-equivalent bootstrap admission (with the gate check in
    /// member mode), the member commit protocol, and the same-lock-hold rotation republish.
    fn execute_group_member_commit(
        &mut self,
        batch: CommitBatch,
        generation_guard: CommitBranchGenerationGuard,
        group: &mut CommitGroupState,
    ) -> LifecycleResult<CommitOutcome> {
        let branch_id = batch.branch_id();
        let generation =
            self.admit_durable_commit(&batch, generation_guard, DurableGateAdmission::Member)?;
        let frozen_before = self
            .branch_catalog
            .branch_state(branch_id)?
            .frozen_table_count();
        let outcome = {
            let setup_timer = perf_trace::start_timer();
            let (branch, registry) = self.branch_catalog.branch_state_mut_with_registry(
                branch_id,
                CommitBranchGenerationGuard::exact(generation),
            )?;
            let mut budgeted_branch = BudgetedCommitBranch::new(branch, &self.budget);
            let mut runtime = CommitDurableRuntime::new(
                &self.commit_config,
                registry,
                &self.guard_set,
                &mut self.allocator,
                &mut budgeted_branch,
                &mut self.visible,
                &mut self.services.wal,
                &self.durable_gate,
            );
            perf_trace::record_commit_setup_elapsed(setup_timer);
            runtime
                .execute_group_member(batch, generation_guard, group)
                .map_err(commit_error)
        };
        if outcome.is_ok() {
            // Republishing the snapshot before the group's visible publish is the safe
            // direction of V-before-S: the snapshot covers more than the visible bound.
            self.republish_branch_snapshot_after_rotation(branch_id, frozen_before)?;
        }
        outcome
    }

    /// Solo-equivalent commit admission (the phase before the commit protocol runs). In member
    /// mode the gate check verifies only that no unresolved fact exists: the leader's admission
    /// span is active by construction, which the solo check would reject.
    fn admit_durable_commit(
        &mut self,
        batch: &CommitBatch,
        generation_guard: CommitBranchGenerationGuard,
        gate_admission: DurableGateAdmission,
    ) -> LifecycleResult<CommitBranchGeneration> {
        let admit_timer = perf_trace::start_timer();
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
        Self::require_durable_commit_mode(batch)?;
        if batch.kind() == CommitBatchKind::Mutating {
            match gate_admission {
                DurableGateAdmission::Solo => self.require_no_unresolved_durable_commit()?,
                DurableGateAdmission::Member => self
                    .durable_gate
                    .require_no_unresolved_fact()
                    .map_err(commit_error)?,
            }
            self.require_branch_commit_guard_available(branch_id)?;
            self.require_write_admission_recovery_health(branch_id)?;
            self.evaluate_mutating_write_admission_for_branch(branch_id)?;
            self.require_projected_mutating_commit_budget(branch_id, batch)?;
        }
        perf_trace::record_commit_admit_elapsed(admit_timer);
        Ok(generation)
    }

    /// Same-lock-hold snapshot republish when a commit's auto-rotation changed the branch's
    /// frozen-table count (see the Model-2 contract note in `execute_durable_commit`).
    fn republish_branch_snapshot_after_rotation(
        &mut self,
        branch_id: BranchId,
        frozen_before: usize,
    ) -> LifecycleResult<()> {
        let frozen_after = self
            .branch_catalog
            .branch_state(branch_id)?
            .frozen_table_count();
        if frozen_after != frozen_before {
            self.publish_branch_snapshot(branch_id);
        }
        Ok(())
    }

    /// Post-publish bookkeeping shared by solo commits and group finalize: mirror the visible
    /// version to the off-lock atomic, evaluate WAL growth, and schedule post-commit maintenance.
    fn finish_durable_commit_post_publish(&mut self, branch_id: BranchId) {
        self.mirror_visible_and_evaluate_wal_growth();
        self.schedule_post_commit_maintenance_best_effort(branch_id);
    }

    fn mirror_visible_and_evaluate_wal_growth(&mut self) {
        self.visible_commit_version
            .store(self.visible.visible_version().as_u64(), Ordering::Release);
        let wal_growth_start = perf_trace::start_timer();
        self.evaluate_and_record_wal_growth_policy();
        perf_trace::record_commit_post_wal_growth_elapsed(wal_growth_start);
    }

    fn schedule_post_commit_maintenance_best_effort(&mut self, branch_id: BranchId) {
        let maintenance_start = perf_trace::start_timer();
        // Rationale: maintenance scheduling is best-effort after a successful commit; a
        // scheduling refusal must not fail the already-durable commit (same as solo).
        let _ = self.schedule_post_commit_maintenance_for_branch(branch_id);
        perf_trace::record_commit_post_maintenance_elapsed(maintenance_start);
    }

    /// Reproduce the per-member gate-admission error after the leader's group admission failed.
    /// Deterministic under the runtime lock: the gate cannot change concurrently.
    fn group_admission_unavailable_error(&self) -> LifecycleError {
        match self.durable_gate.require_admission_available() {
            Err(error) => commit_error(error),
            Ok(()) => commit_error(CommitRuntimeError::InvalidCommitState {
                reason: "write group admission unavailable",
            }),
        }
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
            // A child whose recovered state carries inherited layers is durably COW-covered:
            // layers only enter recovered state through the child's own table manifest, which the
            // fork now publishes at fork time. Re-materializing the parent's rows for such a child
            // is pure redundancy — and O(parent dataset) per child (the reopen OOM). The rebuild
            // below remains only for layer-less children: eager/historical forks whose materialized
            // rows are not WAL'd, and the crash window where the fork's catalog publish landed but
            // its child-manifest publish did not.
            if !branch_catalog
                .branch_state(descriptor.branch_id())?
                .inherited_layers()
                .is_empty()
            {
                continue;
            }
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

/// Fresh release-published mirror cell seeded from the tracker's current visible version
/// (BS2.2; constructed after recovery so the seed covers the recovered frontier).
fn visible_version_mirror(visible: VisibleVersionTracker) -> Arc<AtomicU64> {
    Arc::new(AtomicU64::new(visible.visible_version().as_u64()))
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
    // Fold in the highest committed version present in the restored branch states. Flushed
    // tables can be ahead of both the checkpoint watermark and the surviving WAL (e.g. a
    // checkpoint publish fault after a successful flush pruned the covering WAL segments);
    // without this term those durable, acknowledged rows would sit above `visible` — the
    // branch would reject all mutating commits (`require_branch_not_ahead_of_visible` fails
    // closed) and the allocator would re-issue their commit versions.
    let restored_catalog_max = branch_catalog
        .list_branches(false)
        .into_iter()
        .map(|descriptor| {
            Ok(branch_catalog
                .branch_state(descriptor.branch_id())?
                .max_commit_version()
                .unwrap_or(CommitVersion::ZERO))
        })
        .try_fold(
            CommitVersion::ZERO,
            |max, version: LifecycleResult<CommitVersion>| version.map(|version| max.max(version)),
        )?;
    let recovered_visible_version = checkpoint_watermark
        .max(replayed_max)
        .max(restored_catalog_max);
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
