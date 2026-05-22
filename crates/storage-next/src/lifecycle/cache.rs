//! Cache-mode lifecycle runtime.

use super::{
    validate_backend_capabilities_for_open, CloseOutcome, CloseOutcomeStatus, ClosePhase,
    LifecycleCapabilityOutcome, LifecycleError, LifecycleOperationKind, LifecycleResult,
    LifecycleState, LifecycleStateMachine, LifecycleTransitionTrigger, RecoveryHealth, StorageMode,
    StorageOpenDisposition, StorageOpenOutcome, StorageOpenPlan,
};
use crate::backend::Backend;
use crate::branch::{BranchLocalState, BranchReadView, BranchRuntimeConfig};
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
            false,
        )?;

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

    pub(crate) fn close(&mut self) -> LifecycleResult<CloseOutcome> {
        match self.state.state() {
            LifecycleState::Closed => {
                self.state
                    .transition(LifecycleTransitionTrigger::CloseRetried)?;
                Ok(cache_close_outcome())
            }
            LifecycleState::Open => {
                require_admitted(self.state, LifecycleOperationKind::Close)?;
                self.state
                    .transition(LifecycleTransitionTrigger::CloseRequested)?;
                self.state
                    .transition(LifecycleTransitionTrigger::CloseCompleted)?;
                Ok(cache_close_outcome())
            }
            LifecycleState::Closing => {
                require_admitted(self.state, LifecycleOperationKind::CloseRetry)?;
                self.state
                    .transition(LifecycleTransitionTrigger::CloseRetried)?;
                self.state
                    .transition(LifecycleTransitionTrigger::CloseCompleted)?;
                Ok(cache_close_outcome())
            }
            LifecycleState::New
            | LifecycleState::Opening
            | LifecycleState::Recovering
            | LifecycleState::Failed => Err(LifecycleError::InvalidLifecycleState {
                reason: "cache runtime is not open for close",
            }),
        }
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

const fn cache_close_outcome() -> CloseOutcome {
    CloseOutcome::new(ClosePhase::Closed, CloseOutcomeStatus::Complete)
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
