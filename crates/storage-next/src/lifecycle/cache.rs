//! Cache-mode lifecycle runtime.

use super::{
    compaction::{
        collect_storage_pressure, compact_cache_branch, compaction_request_from_maintenance_task,
        materialization_request_from_maintenance_task, materialize_cache_branch,
    },
    flush::{flush_cache_branch, flush_request_from_maintenance_task},
    validate_backend_capabilities_for_open, CloseOutcome, CloseOutcomeEffects, CloseOutcomeStatus,
    ClosePhase, FlushFrozenOutcome, FlushFrozenRequest, LifecycleCapabilityOutcome,
    LifecycleCloseFact, LifecycleCompactionOutcome, LifecycleCompactionRequest, LifecycleError,
    LifecycleMaintenanceExecutor, LifecycleMaterializationOutcome, LifecycleMaterializationRequest,
    LifecycleOperationKind, LifecycleResult, LifecycleState, LifecycleStateMachine, LifecycleStats,
    LifecycleStoragePressure, LifecycleTransitionTrigger, MaintenanceCancelOutcome,
    MaintenanceEnqueueOutcome, MaintenanceExecutorStatus, MaintenanceOutcome, MaintenanceTask,
    MaintenanceTaskKind, MaintenanceTaskRequest, MaintenanceTaskRunner, RecoveryHealth,
    StorageMode, StorageOpenDisposition, StorageOpenOutcome, StorageOpenPlan,
};
use crate::backend::Backend;
use crate::branch::{BranchLocalState, BranchReadView, BranchRotationOutcome, BranchRuntimeConfig};
use crate::commit::{
    CommitBatch, CommitBranchGeneration, CommitBranchGenerationGuard, CommitBranchGuardSet,
    CommitBranchRegistry, CommitCacheRuntime, CommitFactAllocator, CommitManualTimestampSource,
    CommitOutcome, CommitRuntimeConfig, CommitRuntimeError, CommitTimestampGuard,
    CommitTimestampSource, CommitUnresolvedDurable, CommitUnresolvedDurableGate,
    CommitVersionAllocator, VisibleVersionTracker,
};
use strata_core_next::{BranchId, CommitVersion};

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
    branch: BranchLocalState,
    registry: CommitBranchRegistry,
    guard_set: CommitBranchGuardSet,
    allocator: CommitFactAllocator<S>,
    visible: VisibleVersionTracker,
    durable_gate: CommitUnresolvedDurableGate,
    commit_config: CommitRuntimeConfig,
    maintenance: LifecycleMaintenanceExecutor,
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
        let mut registry = CommitBranchRegistry::new();
        registry
            .register_active(request.initial_branch_id(), request.branch_generation())
            .map_err(commit_error)?;
        commit_config.validate().map_err(commit_error)?;
        let open_outcome = StorageOpenOutcome::new(
            StorageMode::Cache,
            StorageOpenDisposition::Created,
            None,
            RecoveryHealth::Healthy,
            true,
        )?
        .with_backend_capabilities(capability_outcome.capabilities())
        .with_stats(LifecycleStats::new(1, 0, 0, 0, 0));
        let max_maintenance_queue_depth = request
            .plan
            .lifecycle_config()
            .max_maintenance_queue_depth();

        state.transition(LifecycleTransitionTrigger::CacheOpenReady)?;

        Ok(Self {
            state,
            open_plan: request.plan,
            open_outcome,
            capability_outcome,
            branch,
            registry,
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

    pub(crate) const fn capability_outcome(&self) -> &LifecycleCapabilityOutcome {
        &self.capability_outcome
    }

    pub(crate) const fn branch_state(&self) -> &BranchLocalState {
        &self.branch
    }

    pub(crate) const fn visible_version(&self) -> CommitVersion {
        self.visible.visible_version()
    }

    pub(crate) fn unresolved_durable(&self) -> LifecycleResult<Option<CommitUnresolvedDurable>> {
        self.durable_gate.unresolved().map_err(commit_error)
    }

    pub(crate) fn read_view(&self) -> LifecycleResult<BranchReadView> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryRead)?;
        self.branch.capture_read_view().map_err(branch_error)
    }

    pub(crate) fn rotate_active_for_maintenance(
        &mut self,
    ) -> LifecycleResult<BranchRotationOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        Ok(self.branch.rotate_active())
    }

    pub(crate) fn flush_frozen(
        &mut self,
        request: &FlushFrozenRequest,
    ) -> LifecycleResult<FlushFrozenOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        flush_cache_branch(&mut self.branch, request)
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
        compact_cache_branch(&mut self.branch, request)
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
        materialize_cache_branch(&mut self.branch, request)
    }

    #[allow(
        dead_code,
        reason = "runtime hook is consumed by concrete maintenance modules"
    )]
    pub(crate) fn storage_pressure(&self) -> LifecycleStoragePressure {
        collect_storage_pressure(&self.branch, self.maintenance.status())
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
        self.maintenance.enqueue(self.state, request)
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
        let maintenance = &mut self.maintenance;
        let branch = &mut self.branch;
        let mut runner = CacheFlushMaintenanceRunner { branch };
        maintenance.run_next_matching(state, &mut runner, |task| {
            task.kind() == MaintenanceTaskKind::Flush
        })
    }

    #[allow(
        dead_code,
        reason = "runtime maintenance entry point is consumed by dedicated tests"
    )]
    pub(crate) fn run_next_compaction_maintenance(
        &mut self,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        let state = self.state;
        let maintenance = &mut self.maintenance;
        let branch = &mut self.branch;
        let mut runner = CacheCompactionMaintenanceRunner { branch };
        maintenance.run_next_matching(state, &mut runner, |task| {
            task.kind() == MaintenanceTaskKind::Compaction
        })
    }

    #[allow(
        dead_code,
        reason = "runtime maintenance entry point is consumed by dedicated tests"
    )]
    pub(crate) fn run_next_materialization_maintenance(
        &mut self,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        let state = self.state;
        let maintenance = &mut self.maintenance;
        let branch = &mut self.branch;
        let mut runner = CacheMaterializationMaintenanceRunner { branch };
        maintenance.run_next_matching(state, &mut runner, |task| {
            task.kind() == MaintenanceTaskKind::Materialization
        })
    }

    pub(crate) fn close(&mut self) -> LifecycleResult<CloseOutcome> {
        match self.state.state() {
            LifecycleState::Closed => {
                self.state
                    .transition(LifecycleTransitionTrigger::CloseRetried)?;
                Ok(cache_idempotent_close_outcome())
            }
            LifecycleState::Open => {
                require_admitted(self.state, LifecycleOperationKind::Close)?;
                if self.maintenance.has_close_required_drain() {
                    return Err(LifecycleError::MaintenanceTaskFailed {
                        reason: "cache close cannot complete while drain-required maintenance is pending",
                    });
                }
                self.state
                    .transition(LifecycleTransitionTrigger::CloseRequested)?;
                let cancel = self.maintenance.cancel_pending_for_close(self.state)?;
                self.state
                    .transition(LifecycleTransitionTrigger::CloseCompleted)?;
                Ok(cache_close_outcome(cancel))
            }
            LifecycleState::New
            | LifecycleState::Opening
            | LifecycleState::Recovering
            | LifecycleState::Closing
            | LifecycleState::Failed => Err(LifecycleError::InvalidLifecycleState {
                reason: "cache runtime is not open for close",
            }),
        }
    }
}

struct CacheFlushMaintenanceRunner<'a> {
    branch: &'a mut BranchLocalState,
}

impl MaintenanceTaskRunner for CacheFlushMaintenanceRunner<'_> {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        let request = flush_request_from_maintenance_task(task)?;
        Ok(flush_cache_branch(self.branch, &request)?.maintenance_outcome())
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
        CommitCacheRuntime::new(
            &self.commit_config,
            &self.registry,
            &self.guard_set,
            &mut self.allocator,
            &mut self.branch,
            &mut self.visible,
            &self.durable_gate,
        )
        .execute(batch, generation_guard)
        .map_err(commit_error)
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

const fn cache_close_outcome(cancel: MaintenanceCancelOutcome) -> CloseOutcome {
    CloseOutcome::new(ClosePhase::Closed, CloseOutcomeStatus::Complete)
        .with_close_fact(LifecycleCloseFact::Complete)
        .with_close_effects(CloseOutcomeEffects::volatile_complete(false))
        .with_stats(LifecycleStats::new(1, 0, cancel.stats().canceled(), 0, 1))
}

const fn cache_idempotent_close_outcome() -> CloseOutcome {
    CloseOutcome::new(ClosePhase::Closed, CloseOutcomeStatus::Idempotent)
        .with_close_fact(LifecycleCloseFact::AlreadyClosed)
        .with_close_effects(CloseOutcomeEffects::volatile_complete(true))
        .with_stats(LifecycleStats::new(1, 0, 0, 0, 1))
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
