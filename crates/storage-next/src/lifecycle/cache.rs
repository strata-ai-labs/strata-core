//! Cache-mode lifecycle runtime.

use super::{
    compaction::{
        bind_materialization_task_for_enqueue, collect_storage_pressure, compact_cache_branch,
        compaction_request_from_maintenance_task, materialization_request_from_maintenance_task,
        materialize_cache_branch,
    },
    flush::{flush_cache_branch_with_budget, flush_request_from_maintenance_task},
    require_maintenance_enqueue_budget, require_rotate_budget, snapshot_with_runtime_usage,
    validate_backend_capabilities_for_open, BudgetedCommitBranch, CloseOutcome,
    CloseOutcomeEffects, CloseOutcomeStatus, ClosePhase, FlushFrozenOutcome, FlushFrozenRequest,
    LifecycleBranchCatalog, LifecycleBranchClearOutcome, LifecycleBranchCreateOutcome,
    LifecycleBranchDeleteOutcome, LifecycleBranchDescriptor, LifecycleBranchForkOutcome,
    LifecycleCapabilityOutcome, LifecycleCloseFact, LifecycleCompactionOutcome,
    LifecycleCompactionRequest, LifecycleError, LifecycleMaintenanceExecutor,
    LifecycleMaterializationOutcome, LifecycleMaterializationRequest, LifecycleOperationKind,
    LifecycleResult, LifecycleState, LifecycleStateMachine, LifecycleStats,
    LifecycleStoragePressure, LifecycleTransitionTrigger, LifecycleWalGrowthOutcome,
    MaintenanceCancelOutcome, MaintenanceEnqueueOutcome, MaintenanceExecutorStatus,
    MaintenanceOutcome, MaintenanceOutcomeStatus, MaintenanceTask, MaintenanceTaskId,
    MaintenanceTaskKind, MaintenanceTaskRequest, MaintenanceTaskRunner, MaintenanceTaskScope,
    RecoveryHealth, StorageBudgetLedger, StorageBudgetSnapshot, StorageMode,
    StorageOpenDisposition, StorageOpenOutcome, StorageOpenPlan,
};
use crate::backend::Backend;
use crate::branch::{BranchLocalState, BranchReadView, BranchRotationOutcome, BranchRuntimeConfig};
use crate::commit::{
    CommitBatch, CommitBranchGeneration, CommitBranchGenerationGuard, CommitBranchGuardSet,
    CommitCacheRuntime, CommitFactAllocator, CommitManualTimestampSource, CommitOutcome,
    CommitRuntimeConfig, CommitRuntimeError, CommitTimestampGuard, CommitTimestampSource,
    CommitUnresolvedDurable, CommitUnresolvedDurableGate, CommitVersionAllocator,
    VisibleVersionTracker,
};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleCacheOpenRequest {
    plan: StorageOpenPlan,
    initial_branch_id: BranchId,
    branch_generation: CommitBranchGeneration,
}

#[derive(Debug)]
pub(crate) struct LifecycleCacheRuntime<S = CommitManualTimestampSource> {
    state: LifecycleStateMachine,
    open_plan: StorageOpenPlan,
    open_outcome: StorageOpenOutcome,
    capability_outcome: LifecycleCapabilityOutcome,
    branch_catalog: LifecycleBranchCatalog,
    initial_branch_id: BranchId,
    guard_set: CommitBranchGuardSet,
    allocator: CommitFactAllocator<S>,
    visible: VisibleVersionTracker,
    durable_gate: CommitUnresolvedDurableGate,
    commit_config: CommitRuntimeConfig,
    maintenance: LifecycleMaintenanceExecutor,
    budget: StorageBudgetLedger,
    // The CloseOutcome from the first successful close is preserved here so
    // subsequent idempotent close calls return the *prior final facts*
    // (cancel count, stats, close fact) instead of fabricating fresh
    // values. The lifecycle contract requires "Close after Closed is a
    // no-op success with the prior final facts" — without caching the
    // returned facts here, a second close would invent stats that diverge
    // from what callers already observed on the first call.
    last_close_outcome: Option<CloseOutcome>,
}

impl LifecycleCacheOpenRequest {
    pub(crate) fn new(
        plan: StorageOpenPlan,
        initial_branch_id: BranchId,
        branch_generation: CommitBranchGeneration,
    ) -> LifecycleResult<Self> {
        let request = Self {
            plan,
            initial_branch_id,
            branch_generation,
        };
        request.validate()?;
        Ok(request)
    }

    pub(crate) fn plan(&self) -> &StorageOpenPlan {
        &self.plan
    }

    pub(crate) const fn initial_branch_id(&self) -> BranchId {
        self.initial_branch_id
    }

    pub(crate) const fn branch_generation(&self) -> CommitBranchGeneration {
        self.branch_generation
    }

    fn validate(&self) -> LifecycleResult<()> {
        self.plan.validate()?;
        if self.plan.storage_mode() != StorageMode::Cache {
            return Err(LifecycleError::InvalidOpenPlan {
                reason: "cache lifecycle runtime requires cache storage mode",
            });
        }
        Ok(())
    }
}

impl<S> LifecycleCacheRuntime<S> {
    pub(crate) fn open(
        request: LifecycleCacheOpenRequest,
        backend: &dyn Backend,
        branch_config: BranchRuntimeConfig,
        commit_config: CommitRuntimeConfig,
        timestamp_source: S,
    ) -> LifecycleResult<Self> {
        request.validate()?;
        let mut state = LifecycleStateMachine::new();
        require_admitted(state, LifecycleOperationKind::Open)?;
        state.transition(LifecycleTransitionTrigger::OpenRequested)?;

        let capability_outcome = validate_backend_capabilities_for_open(request.plan(), backend)?;
        let branch = BranchLocalState::new(request.initial_branch_id(), branch_config)
            .map_err(branch_error)?;
        let branch_catalog = LifecycleBranchCatalog::with_existing_branch(
            &branch,
            request.branch_generation(),
            None,
        )?;
        commit_config.validate().map_err(commit_error)?;
        let budget = StorageBudgetLedger::new(request.plan.lifecycle_config().storage_budget())?;
        let open_outcome = StorageOpenOutcome::new(
            StorageMode::Cache,
            StorageOpenDisposition::Created,
            None,
            RecoveryHealth::Healthy,
            true,
        )?
        .with_backend_capabilities(capability_outcome.capabilities())
        .with_stats(LifecycleStats::new(1, 0, 0, 0, 0))
        .with_budget_snapshot(budget.snapshot());
        let max_maintenance_queue_depth = request
            .plan
            .lifecycle_config()
            .max_maintenance_queue_depth();

        state.transition(LifecycleTransitionTrigger::CacheOpenReady)?;

        let initial_branch_id = request.initial_branch_id();
        // The local `branch` value was used to seed the catalog via
        // `with_existing_branch` and is no longer needed; drop it to make
        // the catalog the sole owner.
        drop(branch);
        Ok(Self {
            state,
            open_plan: request.plan,
            open_outcome,
            capability_outcome,
            branch_catalog,
            initial_branch_id,
            guard_set: CommitBranchGuardSet::new(),
            allocator: CommitFactAllocator::new(
                CommitVersionAllocator::default(),
                CommitTimestampGuard::default(),
                timestamp_source,
            ),
            visible: VisibleVersionTracker::default(),
            durable_gate: CommitUnresolvedDurableGate::new(),
            commit_config,
            maintenance: LifecycleMaintenanceExecutor::new(max_maintenance_queue_depth)?,
            budget,
            last_close_outcome: None,
        })
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

    pub(crate) const fn allocator(&self) -> &CommitFactAllocator<S> {
        &self.allocator
    }

    pub(crate) fn budget_snapshot(&self) -> StorageBudgetSnapshot {
        let branch = self
            .branch_catalog
            .branch_state(self.initial_branch_id)
            .expect("seeded branch is always present in the catalog");
        snapshot_with_runtime_usage(&self.budget, branch, self.maintenance.status())
    }

    pub(crate) const fn capability_outcome(&self) -> &LifecycleCapabilityOutcome {
        &self.capability_outcome
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

    pub(crate) const fn branch_catalog(&self) -> &LifecycleBranchCatalog {
        &self.branch_catalog
    }

    #[cfg(test)]
    pub(crate) fn branch_catalog_mut_for_test(&mut self) -> &mut LifecycleBranchCatalog {
        &mut self.branch_catalog
    }

    pub(crate) const fn visible_version(&self) -> CommitVersion {
        self.visible.visible_version()
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

    pub(crate) fn rotate_active_for_maintenance(
        &mut self,
    ) -> LifecycleResult<BranchRotationOutcome> {
        self.rotate_active_for_branch_for_maintenance(self.initial_branch_id)
    }

    pub(crate) fn rotate_active_for_branch_for_maintenance(
        &mut self,
        branch_id: strata_core_next::BranchId,
    ) -> LifecycleResult<BranchRotationOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let generation = self
            .branch_catalog
            .registry()
            .lookup(branch_id)
            .map_err(commit_error)?
            .generation();
        require_rotate_budget(
            &self.budget,
            self.branch_catalog
                .branch_state(branch_id)
                .expect("seeded branch present"),
        )?;
        let branch = self
            .branch_catalog
            .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))?;
        let outcome = branch.rotate_active();
        Ok(outcome)
    }

    /// Storage-internal: create a new branch in the catalog. The new
    /// branch is visible via `list_branches` and
    /// `branch_catalog().branch_state(...)` and can accept commits routed
    /// through the catalog's per-branch slot.
    pub(crate) fn create_branch(
        &mut self,
        branch_id: BranchId,
        generation: CommitBranchGeneration,
        created_at: Option<CommitVersion>,
    ) -> LifecycleResult<LifecycleBranchCreateOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        self.branch_catalog
            .create_branch(branch_id, generation, created_at)
    }

    pub(crate) fn list_branches(&self, include_deleted: bool) -> Vec<LifecycleBranchDescriptor> {
        self.branch_catalog.list_branches(include_deleted)
    }

    pub(crate) fn fork_current(
        &mut self,
        source: BranchId,
        destination: BranchId,
        destination_generation: CommitBranchGeneration,
    ) -> LifecycleResult<LifecycleBranchForkOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let _quiesce = self.guard_set.try_begin_quiesce().map_err(commit_error)?;
        self.branch_catalog
            .fork_current(source, destination, destination_generation)
    }

    #[allow(
        dead_code,
        reason = "fork-at-history surface exposed for cache callers"
    )]
    pub(crate) fn fork_at_retained_version(
        &mut self,
        source: BranchId,
        destination: BranchId,
        destination_generation: CommitBranchGeneration,
        fork_version: CommitVersion,
        retained_floor: CommitVersion,
    ) -> LifecycleResult<LifecycleBranchForkOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let _quiesce = self.guard_set.try_begin_quiesce().map_err(commit_error)?;
        self.branch_catalog.fork_at_retained_version(
            source,
            destination,
            destination_generation,
            fork_version,
            retained_floor,
        )
    }

    #[allow(
        dead_code,
        reason = "fork-at-history surface exposed for cache callers"
    )]
    pub(crate) fn fork_at_retained_timestamp(
        &mut self,
        source: BranchId,
        destination: BranchId,
        destination_generation: CommitBranchGeneration,
        timestamp: Timestamp,
        retained_floor: CommitVersion,
    ) -> LifecycleResult<LifecycleBranchForkOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let _quiesce = self.guard_set.try_begin_quiesce().map_err(commit_error)?;
        self.branch_catalog.fork_at_retained_timestamp(
            source,
            destination,
            destination_generation,
            timestamp,
            retained_floor,
        )
    }

    pub(crate) fn clear_branch(
        &mut self,
        branch_id: BranchId,
        generation_guard: CommitBranchGenerationGuard,
    ) -> LifecycleResult<LifecycleBranchClearOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let _quiesce = self.guard_set.try_begin_quiesce().map_err(commit_error)?;
        let outcome = self
            .branch_catalog
            .clear_branch(branch_id, generation_guard)?;
        // Cache has no retention pass; release plan is discarded after
        // the outcome leaves this method.
        Ok(outcome)
    }

    pub(crate) fn delete_branch(
        &mut self,
        branch_id: BranchId,
        generation_guard: CommitBranchGenerationGuard,
        deleted_at: Option<CommitVersion>,
    ) -> LifecycleResult<LifecycleBranchDeleteOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let _quiesce = self.guard_set.try_begin_quiesce().map_err(commit_error)?;
        let outcome = self
            .branch_catalog
            .delete_branch(branch_id, generation_guard, deleted_at)?;
        // Cache mode discards the release plan — no retention to drain.
        Ok(outcome)
    }

    pub(crate) fn flush_frozen(
        &mut self,
        request: &FlushFrozenRequest,
    ) -> LifecycleResult<FlushFrozenOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let branch_id = request.branch_id();
        let generation = self
            .branch_catalog
            .registry()
            .lookup(branch_id)
            .map_err(commit_error)?
            .generation();
        let outcome = {
            let branch = self
                .branch_catalog
                .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))?;
            flush_cache_branch_with_budget(branch, request, Some(&self.budget))
        };
        outcome
    }

    #[allow(
        dead_code,
        reason = "runtime hook is consumed by concrete maintenance modules"
    )]
    pub(crate) fn compact_branch_tables(
        &mut self,
        request: &LifecycleCompactionRequest,
    ) -> LifecycleResult<LifecycleCompactionOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let branch_id = request.branch_id();
        let generation = self
            .branch_catalog
            .registry()
            .lookup(branch_id)
            .map_err(commit_error)?
            .generation();
        let outcome = {
            let branch = self
                .branch_catalog
                .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))?;
            compact_cache_branch(branch, request)
        };
        outcome
    }

    #[allow(
        dead_code,
        reason = "runtime hook is consumed by concrete maintenance modules"
    )]
    pub(crate) fn materialize_inherited_layer(
        &mut self,
        request: &LifecycleMaterializationRequest,
    ) -> LifecycleResult<LifecycleMaterializationOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let branch_id = request.child_branch_id();
        let generation = self
            .branch_catalog
            .registry()
            .lookup(branch_id)
            .map_err(commit_error)?
            .generation();
        let outcome = {
            let branch = self
                .branch_catalog
                .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))?;
            materialize_cache_branch(branch, request)
        };
        outcome
    }

    #[allow(
        dead_code,
        reason = "runtime hook is consumed by concrete maintenance modules"
    )]
    pub(crate) fn storage_pressure(&self) -> LifecycleStoragePressure {
        let branch = self
            .branch_catalog
            .branch_state(self.initial_branch_id)
            .expect("seeded branch is always present in the catalog");
        collect_storage_pressure(branch, self.maintenance.status())
    }

    #[allow(
        dead_code,
        reason = "runtime hook is consumed by concrete maintenance modules"
    )]
    pub(crate) const fn maintenance_status(&self) -> MaintenanceExecutorStatus {
        self.maintenance.status()
    }

    #[allow(
        dead_code,
        reason = "pre-public-boundary policy hook is consumed by lifecycle hardening tests"
    )]
    pub(crate) fn evaluate_wal_growth_policy(&self) -> LifecycleWalGrowthOutcome {
        debug_assert_eq!(self.state(), LifecycleState::Open);
        LifecycleWalGrowthOutcome::cache_mode()
    }

    #[cfg(test)]
    pub(crate) fn force_close_requested_for_test(&mut self) -> LifecycleResult<()> {
        self.state
            .transition(LifecycleTransitionTrigger::CloseRequested)?;
        Ok(())
    }

    #[allow(
        dead_code,
        reason = "runtime hook is consumed by concrete maintenance modules"
    )]
    pub(crate) fn enqueue_maintenance(
        &mut self,
        request: MaintenanceTaskRequest,
    ) -> LifecycleResult<MaintenanceEnqueueOutcome> {
        if !cache_supports_maintenance_kind(request.kind()) {
            return Err(LifecycleError::MaintenanceTaskFailed {
                reason: "volatile runtime does not support durable maintenance task",
            });
        }
        let budget = self.budget.clone();
        let maintenance_status = self.maintenance.status();
        let state = self.state;
        {
            let maintenance = &mut self.maintenance;
            let branch_catalog = &mut self.branch_catalog;
            maintenance.enqueue_with_binding(state, request, |request| {
                require_maintenance_enqueue_budget(&budget, maintenance_status)?;
                bind_materialization_request_in_catalog(branch_catalog, request)
            })
        }
    }

    #[allow(
        dead_code,
        reason = "runtime hook is consumed by concrete maintenance modules"
    )]
    pub(crate) fn run_next_maintenance(
        &mut self,
        runner: &mut impl MaintenanceTaskRunner,
    ) -> LifecycleResult<Option<super::MaintenanceOutcome>> {
        self.maintenance.run_next(self.state, runner)
    }

    pub(crate) fn run_next_flush_maintenance(
        &mut self,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        let state = self.state;
        let Some(task) = self
            .maintenance
            .next_matching_task(|task| task.kind() == MaintenanceTaskKind::Flush)
        else {
            return Ok(None);
        };
        let task_id = task.id();
        let branch_id = branch_id_from_branch_task(task)?;
        // Pre-sync the shadow into the catalog so any direct shadow
        // mutations (e.g. test-only branch_state_mut writes) are visible
        // when the runner fetches branch state via the catalog.
        let generation = self
            .branch_catalog
            .registry()
            .lookup(branch_id)
            .map_err(commit_error)?
            .generation();
        let outcome = {
            let maintenance = &mut self.maintenance;
            let branch = self
                .branch_catalog
                .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))?;
            let mut runner = CacheFlushMaintenanceRunner {
                branch,
                budget: &self.budget,
            };
            maintenance.run_next_matching(state, &mut runner, |task| task.id() == task_id)
        };
        outcome
    }

    #[allow(
        dead_code,
        reason = "runtime maintenance entry point is consumed by dedicated tests"
    )]
    pub(crate) fn run_next_compaction_maintenance(
        &mut self,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        let state = self.state;
        let Some(task) = self
            .maintenance
            .next_matching_task(|task| task.kind() == MaintenanceTaskKind::Compaction)
        else {
            return Ok(None);
        };
        let task_id = task.id();
        let branch_id = branch_id_from_table_level_task(task)?;
        // Pre-sync the shadow into the catalog so any direct shadow
        // mutations (e.g. test-only branch_state_mut writes) are visible
        // when the runner fetches branch state via the catalog.
        let generation = self
            .branch_catalog
            .registry()
            .lookup(branch_id)
            .map_err(commit_error)?
            .generation();
        let outcome = {
            let maintenance = &mut self.maintenance;
            let branch = self
                .branch_catalog
                .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))?;
            let mut runner = CacheCompactionMaintenanceRunner { branch };
            maintenance.run_next_matching(state, &mut runner, |task| task.id() == task_id)
        };
        outcome
    }

    #[allow(
        dead_code,
        reason = "runtime maintenance entry point is consumed by dedicated tests"
    )]
    pub(crate) fn run_next_materialization_maintenance(
        &mut self,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        let Some(task) = self
            .maintenance
            .next_matching_task(|task| task.kind() == MaintenanceTaskKind::Materialization)
        else {
            return Ok(None);
        };
        self.run_materialization_maintenance_task(task.id())
    }

    pub(crate) fn run_materialization_maintenance_task(
        &mut self,
        task_id: MaintenanceTaskId,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        let state = self.state;
        let Some(task) = self.maintenance.next_matching_task(|task| {
            task.id() == task_id && task.kind() == MaintenanceTaskKind::Materialization
        }) else {
            return Ok(None);
        };
        let branch_id = branch_id_from_inherited_layer_task(task)?;
        // Pre-sync the shadow into the catalog so any direct shadow
        // mutations (e.g. test-only branch_state_mut writes) are visible
        // when the runner fetches branch state via the catalog.
        let generation = self
            .branch_catalog
            .registry()
            .lookup(branch_id)
            .map_err(commit_error)?
            .generation();
        let outcome = {
            let maintenance = &mut self.maintenance;
            let branch = self
                .branch_catalog
                .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))?;
            let mut runner = CacheMaterializationMaintenanceRunner { branch };
            maintenance.run_next_matching(state, &mut runner, |task| task.id() == task_id)
        };
        outcome
    }

    pub(crate) fn close(&mut self) -> LifecycleResult<CloseOutcome> {
        match self.state.state() {
            LifecycleState::Closed => {
                self.state
                    .transition(LifecycleTransitionTrigger::CloseRetried)?;
                // Return the prior final facts from the first close so
                // callers observing the second outcome see the same stats
                // (canceled count, status, close fact) they saw on the
                // first call, just with the close fact remapped to
                // AlreadyClosed and the status to Idempotent. Falling
                // back to a fabricated outcome would silently drift from
                // the first-close stats.
                Ok(self
                    .last_close_outcome
                    .as_ref()
                    .map_or_else(cache_idempotent_close_outcome, idempotent_from_prior_close))
            }
            LifecycleState::Open => {
                require_admitted(self.state, LifecycleOperationKind::Close)?;
                if self.maintenance.status().active_task().is_some() {
                    return Err(LifecycleError::CloseTimeout {
                        phase: ClosePhase::DrainMaintenance,
                        reason: "maintenance task is active",
                    });
                }
                self.state
                    .transition(LifecycleTransitionTrigger::CloseRequested)?;
                // Cache supports its own drain-required path by dispatching
                // queued drain tasks through the per-kind cache runners.
                // The cache runtime is volatile-only, so drained tasks have
                // no durable side effects; the work still runs so frozen
                // mutable state is folded into branch read state before
                // close completes.
                let drained = self.drain_cache_required_tasks()?;
                self.finish_cache_close(drained)
            }
            LifecycleState::Closing => {
                require_admitted(self.state, LifecycleOperationKind::CloseRetry)?;
                self.state
                    .transition(LifecycleTransitionTrigger::CloseRetried)?;
                // A retry from Closing must also drain any drain-required
                // tasks that the first close didn't complete — otherwise
                // they leak past the close transition.
                let drained = self.drain_cache_required_tasks()?;
                self.finish_cache_close(drained)
            }
            LifecycleState::Failed => {
                // Symmetric with the durable runtime: Failed admits Close
                // only when the prior failure was a close-class failure.
                // The state machine's `admit` enforces this; rejection
                // surfaces here as `InvalidLifecycleState`. The drain must
                // still run on the retry so the queue is empty when the
                // runtime reaches Closed.
                require_admitted(self.state, LifecycleOperationKind::Close)?;
                self.state
                    .transition(LifecycleTransitionTrigger::CloseRequested)?;
                let drained = self.drain_cache_required_tasks()?;
                self.finish_cache_close(drained)
            }
            LifecycleState::New | LifecycleState::Opening | LifecycleState::Recovering => {
                Err(LifecycleError::InvalidLifecycleState {
                    reason: "cache runtime is not open for close",
                })
            }
        }
    }

    fn finish_cache_close(&mut self, drained: usize) -> LifecycleResult<CloseOutcome> {
        let cancel = self.maintenance.cancel_pending_for_close(self.state)?;
        self.state
            .transition(LifecycleTransitionTrigger::CloseCompleted)?;
        let outcome = cache_close_outcome(cancel, drained);
        // Snapshot the outcome so subsequent idempotent close calls return
        // the same prior facts instead of fabricating fresh values.
        self.last_close_outcome = Some(outcome);
        Ok(outcome)
    }

    fn drain_cache_required_tasks(&mut self) -> LifecycleResult<usize> {
        // Cache only supports Flush / Compaction / Materialization /
        // HealthCollection. Each kind has a per-kind runner that the
        // close-time drain dispatches through. The drain executor's
        // `drain_for_close` runs every queued drain-required task in
        // order; if any task errors the drain aborts and surfaces the
        // typed lifecycle error, which the caller routes to close-retry.
        // The returned count feeds the close outcome's
        // `maintenance_tasks` stat so callers see drained work, parity
        // with the durable runtime.
        let state = self.state;
        let branch_id = self.initial_branch_id;
        // Pre-sync shadow into catalog so the close-time drain runs over
        // up-to-date branch state.
        let generation = self
            .branch_catalog
            .registry()
            .lookup(branch_id)
            .map_err(commit_error)?
            .generation();
        let drained = {
            let maintenance = &mut self.maintenance;
            let branch = self
                .branch_catalog
                .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))?;
            let mut runner = CacheCloseRunner {
                branch,
                budget: &self.budget,
            };
            maintenance.drain_for_close(state, &mut runner)?
        };
        Ok(drained.drained_tasks())
    }
}

struct CacheCloseRunner<'a> {
    branch: &'a mut BranchLocalState,
    budget: &'a StorageBudgetLedger,
}

impl MaintenanceTaskRunner for CacheCloseRunner<'_> {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        match task.kind() {
            MaintenanceTaskKind::Flush => {
                let request = flush_request_from_maintenance_task(task)?;
                Ok(
                    flush_cache_branch_with_budget(self.branch, &request, Some(self.budget))?
                        .maintenance_outcome(),
                )
            }
            MaintenanceTaskKind::Compaction => {
                let request = compaction_request_from_maintenance_task(task)?;
                Ok(compact_cache_branch(self.branch, &request)?.maintenance_outcome())
            }
            MaintenanceTaskKind::Materialization => {
                let request = materialization_request_from_maintenance_task(task)?;
                Ok(materialize_cache_branch(self.branch, &request)?.maintenance_outcome())
            }
            MaintenanceTaskKind::HealthCollection => Ok(MaintenanceOutcome::new(
                MaintenanceTaskKind::HealthCollection,
                MaintenanceOutcomeStatus::Completed,
            )),
            other => Err(LifecycleError::MaintenanceTaskFailed {
                reason: cache_unsupported_drain_reason(other),
            }),
        }
    }
}

const fn cache_unsupported_drain_reason(_kind: MaintenanceTaskKind) -> &'static str {
    // Volatile cache runtimes only host the four kinds enumerated by
    // `cache_supports_maintenance_kind`. Reaching this arm means a
    // checkpoint/wal/retention/quarantine task was enqueued through a
    // path that bypassed the enqueue-time guard — surface a typed
    // failure rather than running it.
    "cache drain rejected a task kind that cache mode does not implement"
}

struct CacheFlushMaintenanceRunner<'a> {
    branch: &'a mut BranchLocalState,
    budget: &'a StorageBudgetLedger,
}

impl MaintenanceTaskRunner for CacheFlushMaintenanceRunner<'_> {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        let request = flush_request_from_maintenance_task(task)?;
        Ok(
            flush_cache_branch_with_budget(self.branch, &request, Some(self.budget))?
                .maintenance_outcome(),
        )
    }
}

struct CacheCompactionMaintenanceRunner<'a> {
    branch: &'a mut BranchLocalState,
}

impl MaintenanceTaskRunner for CacheCompactionMaintenanceRunner<'_> {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        let request = compaction_request_from_maintenance_task(task)?;
        Ok(compact_cache_branch(self.branch, &request)?.maintenance_outcome())
    }
}

struct CacheMaterializationMaintenanceRunner<'a> {
    branch: &'a mut BranchLocalState,
}

impl MaintenanceTaskRunner for CacheMaterializationMaintenanceRunner<'_> {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        let request = materialization_request_from_maintenance_task(task)?;
        Ok(materialize_cache_branch(self.branch, &request)?.maintenance_outcome())
    }
}

impl<S> LifecycleCacheRuntime<S> {
    #[allow(
        dead_code,
        reason = "close integration consumes this drain/cancel hook"
    )]
    pub(crate) fn cancel_pending_maintenance_for_close(
        &mut self,
    ) -> LifecycleResult<MaintenanceCancelOutcome> {
        self.maintenance.cancel_pending_for_close(self.state)
    }

    #[cfg(test)]
    pub(crate) const fn guard_set(&self) -> &CommitBranchGuardSet {
        &self.guard_set
    }
}

impl<S> LifecycleCacheRuntime<S>
where
    S: CommitTimestampSource,
{
    pub(crate) fn execute_cache_commit(
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
            CommitCacheRuntime::new(
                &self.commit_config,
                registry,
                &self.guard_set,
                &mut self.allocator,
                &mut budgeted_branch,
                &mut self.visible,
                &self.durable_gate,
            )
            .execute(batch, generation_guard)
            .map_err(commit_error)
        };
        outcome
    }
}

fn require_admitted(
    state: LifecycleStateMachine,
    operation: LifecycleOperationKind,
) -> LifecycleResult<()> {
    let admission = state.admit(operation);
    if admission.is_allowed() {
        Ok(())
    } else {
        Err(LifecycleError::InvalidLifecycleState {
            reason: admission
                .rejection_reason()
                .unwrap_or("operation is not admitted in current lifecycle state"),
        })
    }
}

fn cache_close_outcome(cancel: MaintenanceCancelOutcome, drained_tasks: usize) -> CloseOutcome {
    let maintenance_tasks = cancel.canceled_tasks().saturating_add(drained_tasks);
    CloseOutcome::new(ClosePhase::Closed, CloseOutcomeStatus::Complete)
        .with_close_fact(LifecycleCloseFact::Complete)
        .with_close_effects(CloseOutcomeEffects::volatile_complete(false))
        .with_stats(LifecycleStats::new(1, 0, maintenance_tasks, 0, 1))
}

const fn cache_idempotent_close_outcome() -> CloseOutcome {
    CloseOutcome::new(ClosePhase::Closed, CloseOutcomeStatus::Idempotent)
        .with_close_fact(LifecycleCloseFact::AlreadyClosed)
        .with_close_effects(CloseOutcomeEffects::volatile_complete(true))
        .with_stats(LifecycleStats::new(1, 0, 0, 0, 1))
}

/// Convert a prior first-close `CloseOutcome` into the idempotent retry
/// shape. The stats from the first close are preserved verbatim; the
/// effects are remapped to the volatile-complete shape with the
/// `prior_final` bit set so `CloseOutcome::validate` accepts the
/// `Idempotent` status. Status flips to `Idempotent` and the close fact
/// to `AlreadyClosed`. Callers observing the second outcome see the
/// same canceled/drained counts they saw on the first call.
fn idempotent_from_prior_close(prior: &CloseOutcome) -> CloseOutcome {
    CloseOutcome::new(ClosePhase::Closed, CloseOutcomeStatus::Idempotent)
        .with_close_fact(LifecycleCloseFact::AlreadyClosed)
        .with_close_effects(CloseOutcomeEffects::volatile_complete(true))
        .with_stats(prior.stats())
}

const fn cache_supports_maintenance_kind(kind: MaintenanceTaskKind) -> bool {
    matches!(
        kind,
        MaintenanceTaskKind::Flush
            | MaintenanceTaskKind::Compaction
            | MaintenanceTaskKind::Materialization
            | MaintenanceTaskKind::HealthCollection
    )
}

fn bind_materialization_request_in_catalog(
    branch_catalog: &mut LifecycleBranchCatalog,
    request: MaintenanceTaskRequest,
) -> LifecycleResult<MaintenanceTaskRequest> {
    if request.kind() != MaintenanceTaskKind::Materialization
        || request.materialization_handle().is_some()
    {
        return Ok(request);
    }
    let branch_id = branch_id_from_inherited_layer_request(request)?;
    let generation = branch_catalog
        .registry()
        .lookup(branch_id)
        .map_err(commit_error)?
        .generation();
    let branch = branch_catalog
        .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))?;
    bind_materialization_task_for_enqueue(branch, request)
}

const fn branch_id_from_branch_task(task: MaintenanceTask) -> LifecycleResult<BranchId> {
    match task.scope() {
        MaintenanceTaskScope::Branch(branch_id) => Ok(branch_id),
        _ => Err(LifecycleError::MaintenanceTaskFailed {
            reason: "maintenance task must target a branch",
        }),
    }
}

const fn branch_id_from_table_level_task(task: MaintenanceTask) -> LifecycleResult<BranchId> {
    match task.scope() {
        MaintenanceTaskScope::TableLevel { branch_id, .. } => Ok(branch_id),
        _ => Err(LifecycleError::MaintenanceTaskFailed {
            reason: "compaction task must target a table level",
        }),
    }
}

const fn branch_id_from_inherited_layer_task(task: MaintenanceTask) -> LifecycleResult<BranchId> {
    match task.scope() {
        MaintenanceTaskScope::InheritedLayer { branch_id, .. } => Ok(branch_id),
        _ => Err(LifecycleError::MaintenanceTaskFailed {
            reason: "materialization task must target an inherited layer",
        }),
    }
}

const fn branch_id_from_inherited_layer_request(
    request: MaintenanceTaskRequest,
) -> LifecycleResult<BranchId> {
    match request.scope() {
        MaintenanceTaskScope::InheritedLayer { branch_id, .. } => Ok(branch_id),
        _ => Err(LifecycleError::MaintenanceTaskFailed {
            reason: "materialization task must target an inherited layer",
        }),
    }
}

fn branch_error(error: impl std::error::Error + Send + Sync + 'static) -> LifecycleError {
    LifecycleError::lower_layer_with(
        super::LifecycleLowerLayer::BranchRuntime,
        "branch runtime failed",
        error,
    )
}

fn commit_error(error: CommitRuntimeError) -> LifecycleError {
    LifecycleError::lower_layer_with(
        super::LifecycleLowerLayer::CommitRuntime,
        "commit runtime failed",
        error,
    )
}
