//! Deterministic lifecycle maintenance task executor.

#![allow(
    dead_code,
    reason = "executor hooks are consumed by concrete maintenance modules"
)]

use super::{
    LifecycleAdmissionEffect, LifecycleError, LifecycleOperationAdmission, LifecycleOperationKind,
    LifecycleResult, LifecycleStateMachine, LifecycleStats, MaintenanceOutcome,
    MaintenanceOutcomeStatus, MaintenanceTaskKind, RecoveryDegradationClass, RecoveryFault,
    RecoveryFaultKind, RecoveryHealth,
};
use strata_core_next::BranchId;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct MaintenanceTaskId(u64);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(crate) struct MaintenanceTaskSequence(u64);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MaintenanceTaskPriority {
    Critical,
    High,
    Normal,
    Low,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MaintenanceTaskScope {
    Global,
    Branch(BranchId),
    Wal,
    Checkpoint,
    Quarantine,
    Retention,
    TableLevel { branch_id: BranchId, level: u8 },
    InheritedLayer { branch_id: BranchId },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MaintenanceClosePolicy {
    Ordinary,
    DrainBeforeClose,
    CancelBeforeClose,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MaintenanceTaskPolicy {
    close_policy: MaintenanceClosePolicy,
    coalesce: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MaintenanceTaskRequest {
    kind: MaintenanceTaskKind,
    priority: MaintenanceTaskPriority,
    scope: MaintenanceTaskScope,
    policy: MaintenanceTaskPolicy,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MaintenanceTask {
    id: MaintenanceTaskId,
    sequence: MaintenanceTaskSequence,
    request: MaintenanceTaskRequest,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MaintenanceCoalesceKey {
    kind: MaintenanceTaskKind,
    scope: MaintenanceTaskScope,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MaintenanceEnqueueStatus {
    Enqueued,
    Coalesced,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MaintenanceEnqueueOutcome {
    task_id: MaintenanceTaskId,
    status: MaintenanceEnqueueStatus,
    pending_tasks: usize,
    stats: LifecycleMaintenanceStats,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MaintenanceDrainOutcome {
    drained_tasks: usize,
    outcomes: Vec<MaintenanceOutcome>,
    stats: LifecycleMaintenanceStats,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MaintenanceCancelOutcome {
    canceled_tasks: usize,
    stats: LifecycleMaintenanceStats,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MaintenanceExecutorStatus {
    pending_tasks: usize,
    active_task: Option<MaintenanceTaskId>,
    stats: LifecycleMaintenanceStats,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct LifecycleMaintenanceStats {
    enqueued: usize,
    coalesced: usize,
    started: usize,
    completed: usize,
    deferred: usize,
    failed: usize,
    canceled: usize,
    drained: usize,
    queue_full: usize,
}

#[derive(Debug)]
pub(crate) struct LifecycleMaintenanceExecutor {
    next_id: u64,
    max_queue_depth: usize,
    queue: Vec<MaintenanceTask>,
    active: Option<MaintenanceTask>,
    stats: LifecycleMaintenanceStats,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MaintenanceFaultPoint {
    BeforeEnqueue,
    AfterEnqueue,
    AtTaskStart,
    AfterTaskRun,
    DuringDrain,
}

pub(crate) trait MaintenanceTaskRunner {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome>;
}

pub(crate) trait MaintenanceFaultHook {
    fn check(
        &mut self,
        point: MaintenanceFaultPoint,
        task: Option<&MaintenanceTask>,
    ) -> LifecycleResult<()>;
}

#[derive(Default)]
pub(crate) struct NoopMaintenanceFaultHook;

impl MaintenanceTaskId {
    pub(crate) const fn new(value: u64) -> LifecycleResult<Self> {
        if value == 0 {
            return Err(LifecycleError::MaintenanceFailed {
                reason: "maintenance task id must be nonzero",
            });
        }
        Ok(Self(value))
    }

    pub(crate) const fn get(self) -> u64 {
        self.0
    }
}

impl MaintenanceTaskSequence {
    const fn new(value: u64) -> Self {
        Self(value)
    }

    pub(crate) const fn get(self) -> u64 {
        self.0
    }
}

impl MaintenanceTaskPriority {
    const fn rank(self) -> u8 {
        match self {
            Self::Critical => 0,
            Self::High => 1,
            Self::Normal => 2,
            Self::Low => 3,
        }
    }
}

impl MaintenanceTaskPolicy {
    pub(crate) const fn ordinary() -> Self {
        Self {
            close_policy: MaintenanceClosePolicy::Ordinary,
            coalesce: false,
        }
    }

    pub(crate) const fn coalescing() -> Self {
        Self {
            close_policy: MaintenanceClosePolicy::Ordinary,
            coalesce: true,
        }
    }

    pub(crate) const fn drain_before_close() -> Self {
        Self {
            close_policy: MaintenanceClosePolicy::DrainBeforeClose,
            coalesce: false,
        }
    }

    pub(crate) const fn cancel_before_close() -> Self {
        Self {
            close_policy: MaintenanceClosePolicy::CancelBeforeClose,
            coalesce: false,
        }
    }

    pub(crate) const fn coalescing_drain_before_close() -> Self {
        Self {
            close_policy: MaintenanceClosePolicy::DrainBeforeClose,
            coalesce: true,
        }
    }

    pub(crate) const fn close_policy(self) -> MaintenanceClosePolicy {
        self.close_policy
    }

    pub(crate) const fn coalesces(self) -> bool {
        self.coalesce
    }
}

impl MaintenanceTaskRequest {
    pub(crate) fn new(
        kind: MaintenanceTaskKind,
        priority: MaintenanceTaskPriority,
        scope: MaintenanceTaskScope,
        policy: MaintenanceTaskPolicy,
    ) -> LifecycleResult<Self> {
        let request = Self {
            kind,
            priority,
            scope,
            policy,
        };
        request.validate()?;
        Ok(request)
    }

    pub(crate) fn health_collection() -> Self {
        Self::new(
            MaintenanceTaskKind::HealthCollection,
            MaintenanceTaskPriority::Low,
            MaintenanceTaskScope::Global,
            MaintenanceTaskPolicy::coalescing(),
        )
        .expect("health collection task request is valid")
    }

    pub(crate) fn flush(branch_id: BranchId) -> Self {
        Self::new(
            MaintenanceTaskKind::Flush,
            MaintenanceTaskPriority::Normal,
            MaintenanceTaskScope::Branch(branch_id),
            MaintenanceTaskPolicy::coalescing(),
        )
        .expect("flush task request is valid")
    }

    pub(crate) fn checkpoint() -> Self {
        Self::new(
            MaintenanceTaskKind::Checkpoint,
            MaintenanceTaskPriority::High,
            MaintenanceTaskScope::Checkpoint,
            MaintenanceTaskPolicy::coalescing(),
        )
        .expect("checkpoint task request is valid")
    }

    pub(crate) fn wal_truncation() -> Self {
        Self::new(
            MaintenanceTaskKind::WalTruncation,
            MaintenanceTaskPriority::Low,
            MaintenanceTaskScope::Wal,
            MaintenanceTaskPolicy::coalescing(),
        )
        .expect("WAL truncation task request is valid")
    }

    pub(crate) const fn kind(self) -> MaintenanceTaskKind {
        self.kind
    }

    pub(crate) const fn priority(self) -> MaintenanceTaskPriority {
        self.priority
    }

    pub(crate) const fn scope(self) -> MaintenanceTaskScope {
        self.scope
    }

    pub(crate) const fn policy(self) -> MaintenanceTaskPolicy {
        self.policy
    }

    pub(crate) const fn coalesce_key(self) -> Option<MaintenanceCoalesceKey> {
        if self.policy.coalesces() {
            Some(MaintenanceCoalesceKey {
                kind: self.kind,
                scope: self.scope,
            })
        } else {
            None
        }
    }

    fn validate(self) -> LifecycleResult<()> {
        if scope_matches_kind(self.kind, self.scope) {
            Ok(())
        } else {
            Err(LifecycleError::MaintenanceFailed {
                reason: "maintenance task scope does not match task kind",
            })
        }
    }
}

impl MaintenanceTask {
    const fn new(
        id: MaintenanceTaskId,
        sequence: MaintenanceTaskSequence,
        request: MaintenanceTaskRequest,
    ) -> Self {
        Self {
            id,
            sequence,
            request,
        }
    }

    pub(crate) const fn id(self) -> MaintenanceTaskId {
        self.id
    }

    #[cfg(test)]
    pub(crate) fn new_for_test(id: u64, request: MaintenanceTaskRequest) -> LifecycleResult<Self> {
        let id = MaintenanceTaskId::new(id)?;
        Ok(Self::new(
            id,
            MaintenanceTaskSequence::new(id.get()),
            request,
        ))
    }

    pub(crate) const fn sequence(self) -> MaintenanceTaskSequence {
        self.sequence
    }

    pub(crate) const fn kind(self) -> MaintenanceTaskKind {
        self.request.kind()
    }

    pub(crate) const fn priority(self) -> MaintenanceTaskPriority {
        self.request.priority()
    }

    pub(crate) const fn scope(self) -> MaintenanceTaskScope {
        self.request.scope()
    }

    pub(crate) const fn policy(self) -> MaintenanceTaskPolicy {
        self.request.policy()
    }

    const fn coalesce_key(self) -> Option<MaintenanceCoalesceKey> {
        self.request.coalesce_key()
    }
}

impl MaintenanceEnqueueOutcome {
    const fn new(
        task_id: MaintenanceTaskId,
        enqueue_status: MaintenanceEnqueueStatus,
        pending_tasks: usize,
        stats: LifecycleMaintenanceStats,
    ) -> Self {
        Self {
            task_id,
            status: enqueue_status,
            pending_tasks,
            stats,
        }
    }

    pub(crate) const fn task_id(self) -> MaintenanceTaskId {
        self.task_id
    }

    pub(crate) const fn status(self) -> MaintenanceEnqueueStatus {
        self.status
    }

    pub(crate) const fn pending_tasks(self) -> usize {
        self.pending_tasks
    }

    pub(crate) const fn stats(self) -> LifecycleMaintenanceStats {
        self.stats
    }
}

impl MaintenanceDrainOutcome {
    const fn new(
        drained_tasks: usize,
        outcomes: Vec<MaintenanceOutcome>,
        stats: LifecycleMaintenanceStats,
    ) -> Self {
        Self {
            drained_tasks,
            outcomes,
            stats,
        }
    }

    pub(crate) const fn drained_tasks(&self) -> usize {
        self.drained_tasks
    }

    pub(crate) fn outcomes(&self) -> &[MaintenanceOutcome] {
        &self.outcomes
    }

    pub(crate) const fn stats(&self) -> LifecycleMaintenanceStats {
        self.stats
    }
}

impl MaintenanceCancelOutcome {
    const fn new(canceled_tasks: usize, stats: LifecycleMaintenanceStats) -> Self {
        Self {
            canceled_tasks,
            stats,
        }
    }

    pub(crate) const fn canceled_tasks(self) -> usize {
        self.canceled_tasks
    }

    pub(crate) const fn stats(self) -> LifecycleMaintenanceStats {
        self.stats
    }
}

impl MaintenanceExecutorStatus {
    const fn new(
        pending_tasks: usize,
        active_task: Option<MaintenanceTaskId>,
        stats: LifecycleMaintenanceStats,
    ) -> Self {
        Self {
            pending_tasks,
            active_task,
            stats,
        }
    }

    pub(crate) const fn pending_tasks(self) -> usize {
        self.pending_tasks
    }

    pub(crate) const fn active_task(self) -> Option<MaintenanceTaskId> {
        self.active_task
    }

    pub(crate) const fn stats(self) -> LifecycleMaintenanceStats {
        self.stats
    }
}

impl LifecycleMaintenanceStats {
    pub(crate) const fn enqueued(self) -> usize {
        self.enqueued
    }

    pub(crate) const fn coalesced(self) -> usize {
        self.coalesced
    }

    pub(crate) const fn started(self) -> usize {
        self.started
    }

    pub(crate) const fn completed(self) -> usize {
        self.completed
    }

    pub(crate) const fn deferred(self) -> usize {
        self.deferred
    }

    pub(crate) const fn failed(self) -> usize {
        self.failed
    }

    pub(crate) const fn canceled(self) -> usize {
        self.canceled
    }

    pub(crate) const fn drained(self) -> usize {
        self.drained
    }

    pub(crate) const fn queue_full(self) -> usize {
        self.queue_full
    }

    pub(crate) const fn lifecycle_stats(self) -> LifecycleStats {
        LifecycleStats::new(0, 0, self.completed + self.deferred + self.failed, 0, 0)
    }
}

impl LifecycleMaintenanceExecutor {
    pub(crate) fn new(max_queue_depth: usize) -> LifecycleResult<Self> {
        if max_queue_depth == 0 {
            return Err(LifecycleError::InvalidConfig {
                field: "max_maintenance_queue_depth",
                reason: "must be nonzero",
            });
        }
        Ok(Self {
            next_id: 1,
            max_queue_depth,
            queue: Vec::new(),
            active: None,
            stats: LifecycleMaintenanceStats::default(),
        })
    }

    pub(crate) fn enqueue(
        &mut self,
        state: LifecycleStateMachine,
        request: MaintenanceTaskRequest,
    ) -> LifecycleResult<MaintenanceEnqueueOutcome> {
        self.enqueue_with_fault(state, request, &mut NoopMaintenanceFaultHook)
    }

    pub(crate) fn enqueue_with_fault(
        &mut self,
        state: LifecycleStateMachine,
        request: MaintenanceTaskRequest,
        fault: &mut impl MaintenanceFaultHook,
    ) -> LifecycleResult<MaintenanceEnqueueOutcome> {
        require_admitted(state, LifecycleOperationKind::OrdinaryMaintenance)?;
        fault.check(MaintenanceFaultPoint::BeforeEnqueue, None)?;
        if let Some(key) = request.coalesce_key() {
            if let Some(existing) = self
                .queue
                .iter()
                .find(|task| task.coalesce_key() == Some(key))
            {
                self.stats.coalesced = self.stats.coalesced.saturating_add(1);
                return Ok(MaintenanceEnqueueOutcome::new(
                    existing.id(),
                    MaintenanceEnqueueStatus::Coalesced,
                    self.queue.len(),
                    self.stats,
                ));
            }
        }
        if self.queue.len() >= self.max_queue_depth {
            self.stats.queue_full = self.stats.queue_full.saturating_add(1);
            return Err(LifecycleError::MaintenanceFailed {
                reason: "maintenance queue is full",
            });
        }
        let id = MaintenanceTaskId::new(self.next_id)?;
        let sequence = MaintenanceTaskSequence::new(self.next_id);
        self.next_id = self.next_id.saturating_add(1);
        let task = MaintenanceTask::new(id, sequence, request);
        self.queue.push(task);
        self.stats.enqueued = self.stats.enqueued.saturating_add(1);
        fault.check(MaintenanceFaultPoint::AfterEnqueue, Some(&task))?;
        Ok(MaintenanceEnqueueOutcome::new(
            id,
            MaintenanceEnqueueStatus::Enqueued,
            self.queue.len(),
            self.stats,
        ))
    }

    pub(crate) fn run_next(
        &mut self,
        state: LifecycleStateMachine,
        runner: &mut impl MaintenanceTaskRunner,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        self.run_next_with_fault(state, runner, &mut NoopMaintenanceFaultHook)
    }

    pub(crate) fn run_next_with_fault(
        &mut self,
        state: LifecycleStateMachine,
        runner: &mut impl MaintenanceTaskRunner,
        fault: &mut impl MaintenanceFaultHook,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        require_admitted(state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let Some(index) = self.next_task_index(|_| true) else {
            return Ok(None);
        };
        self.run_index(index, runner, fault, false).map(Some)
    }

    pub(crate) fn cancel_pending_for_close(
        &mut self,
        state: LifecycleStateMachine,
    ) -> LifecycleResult<MaintenanceCancelOutcome> {
        require_admitted(state, LifecycleOperationKind::CloseRequiredDrain)?;
        let before = self.queue.len();
        self.queue.retain(|task| {
            task.policy().close_policy() != MaintenanceClosePolicy::CancelBeforeClose
        });
        let canceled = before - self.queue.len();
        self.stats.canceled = self.stats.canceled.saturating_add(canceled);
        Ok(MaintenanceCancelOutcome::new(canceled, self.stats))
    }

    pub(crate) fn drain_for_close(
        &mut self,
        state: LifecycleStateMachine,
        runner: &mut impl MaintenanceTaskRunner,
    ) -> LifecycleResult<MaintenanceDrainOutcome> {
        self.drain_for_close_with_fault(state, runner, &mut NoopMaintenanceFaultHook)
    }

    pub(crate) fn drain_for_close_with_fault(
        &mut self,
        state: LifecycleStateMachine,
        runner: &mut impl MaintenanceTaskRunner,
        fault: &mut impl MaintenanceFaultHook,
    ) -> LifecycleResult<MaintenanceDrainOutcome> {
        require_admitted(state, LifecycleOperationKind::CloseRequiredDrain)?;
        let mut outcomes = Vec::new();
        while let Some(index) = self.next_task_index(|task| {
            task.policy().close_policy() == MaintenanceClosePolicy::DrainBeforeClose
        }) {
            let task = self.queue[index];
            if let Err(error) = fault.check(MaintenanceFaultPoint::DuringDrain, Some(&task)) {
                self.stats.failed = self.stats.failed.saturating_add(1);
                return Err(error);
            }
            let outcome = self.run_index(index, runner, fault, true)?;
            outcomes.push(outcome);
        }
        Ok(MaintenanceDrainOutcome::new(
            outcomes.len(),
            outcomes,
            self.stats,
        ))
    }

    pub(crate) const fn status(&self) -> MaintenanceExecutorStatus {
        MaintenanceExecutorStatus::new(
            self.queue.len(),
            match self.active {
                Some(task) => Some(task.id()),
                None => None,
            },
            self.stats,
        )
    }

    pub(crate) fn pending_tasks(&self) -> &[MaintenanceTask] {
        &self.queue
    }

    pub(crate) fn has_close_required_drain(&self) -> bool {
        self.queue
            .iter()
            .any(|task| task.policy().close_policy() == MaintenanceClosePolicy::DrainBeforeClose)
    }

    pub(crate) const fn stats(&self) -> LifecycleMaintenanceStats {
        self.stats
    }

    fn next_task_index(&self, predicate: impl Fn(&MaintenanceTask) -> bool) -> Option<usize> {
        self.queue
            .iter()
            .enumerate()
            .filter(|(_, task)| predicate(task))
            .min_by_key(|(_, task)| (task.priority().rank(), task.sequence().get()))
            .map(|(index, _)| index)
    }

    fn run_index(
        &mut self,
        index: usize,
        runner: &mut impl MaintenanceTaskRunner,
        fault: &mut impl MaintenanceFaultHook,
        draining: bool,
    ) -> LifecycleResult<MaintenanceOutcome> {
        let task = self.queue.remove(index);
        self.active = Some(task);
        self.stats.started = self.stats.started.saturating_add(1);
        if let Err(error) = fault.check(MaintenanceFaultPoint::AtTaskStart, Some(&task)) {
            self.stats.failed = self.stats.failed.saturating_add(1);
            self.active = None;
            return Err(error);
        }
        let outcome = match runner.run_task(&task) {
            Ok(outcome) => attach_executor_facts(outcome, task.id())?,
            Err(error) => {
                self.stats.failed = self.stats.failed.saturating_add(1);
                self.active = None;
                return Err(error);
            }
        };
        if let Err(_error) = fault.check(MaintenanceFaultPoint::AfterTaskRun, Some(&task)) {
            let outcome = MaintenanceOutcome::new(task.kind(), MaintenanceOutcomeStatus::Failed)
                .with_task_id(task.id())
                .with_recovery_health(telemetry_health_debt("maintenance task failed after run")?)
                .with_effects(0, 0, true);
            self.record_outcome(outcome.status(), draining);
            self.active = None;
            return Ok(outcome);
        }
        self.record_outcome(outcome.status(), draining);
        self.active = None;
        Ok(outcome)
    }

    fn record_outcome(&mut self, status: MaintenanceOutcomeStatus, draining: bool) {
        match status {
            MaintenanceOutcomeStatus::Completed => {
                self.stats.completed = self.stats.completed.saturating_add(1);
            }
            MaintenanceOutcomeStatus::Deferred => {
                self.stats.deferred = self.stats.deferred.saturating_add(1);
            }
            MaintenanceOutcomeStatus::Canceled => {
                self.stats.canceled = self.stats.canceled.saturating_add(1);
            }
            MaintenanceOutcomeStatus::Failed => {
                self.stats.failed = self.stats.failed.saturating_add(1);
            }
        }
        if draining {
            self.stats.drained = self.stats.drained.saturating_add(1);
        }
    }
}

fn attach_executor_facts(
    outcome: MaintenanceOutcome,
    task_id: MaintenanceTaskId,
) -> LifecycleResult<MaintenanceOutcome> {
    let outcome = outcome.with_task_id(task_id);
    if outcome.status() == MaintenanceOutcomeStatus::Failed && outcome.recovery_health().is_none() {
        Ok(outcome.with_recovery_health(telemetry_health_debt("maintenance task failed")?))
    } else {
        Ok(outcome)
    }
}

impl MaintenanceFaultHook for NoopMaintenanceFaultHook {
    fn check(
        &mut self,
        _point: MaintenanceFaultPoint,
        _task: Option<&MaintenanceTask>,
    ) -> LifecycleResult<()> {
        Ok(())
    }
}

pub(crate) const fn maintenance_ready_for_recovery_health(health: &RecoveryHealth) -> bool {
    match health {
        RecoveryHealth::Healthy
        | RecoveryHealth::Degraded {
            class: RecoveryDegradationClass::Telemetry,
            ..
        } => true,
        RecoveryHealth::Degraded { .. } | RecoveryHealth::Failed { .. } => false,
    }
}

pub(crate) fn telemetry_health_debt(reason: &'static str) -> LifecycleResult<RecoveryHealth> {
    RecoveryHealth::degraded(
        RecoveryDegradationClass::Telemetry,
        vec![RecoveryFault::new(RecoveryFaultKind::IoFailure, reason)?],
    )
}

fn scope_matches_kind(kind: MaintenanceTaskKind, scope: MaintenanceTaskScope) -> bool {
    matches!(
        (kind, scope),
        (MaintenanceTaskKind::Flush, MaintenanceTaskScope::Branch(_))
            | (
                MaintenanceTaskKind::Checkpoint,
                MaintenanceTaskScope::Checkpoint | MaintenanceTaskScope::Global
            )
            | (
                MaintenanceTaskKind::WalTruncation,
                MaintenanceTaskScope::Wal
            )
            | (
                MaintenanceTaskKind::Compaction,
                MaintenanceTaskScope::TableLevel { .. }
            )
            | (
                MaintenanceTaskKind::Materialization,
                MaintenanceTaskScope::InheritedLayer { .. }
            )
            | (
                MaintenanceTaskKind::SnapshotPruning | MaintenanceTaskKind::Retention,
                MaintenanceTaskScope::Retention
            )
            | (
                MaintenanceTaskKind::Quarantine | MaintenanceTaskKind::Purge,
                MaintenanceTaskScope::Quarantine
            )
            | (
                MaintenanceTaskKind::Repair,
                MaintenanceTaskScope::Quarantine | MaintenanceTaskScope::Global
            )
            | (
                MaintenanceTaskKind::HealthCollection,
                MaintenanceTaskScope::Global
            )
    )
}

fn require_admitted(
    state: LifecycleStateMachine,
    operation: LifecycleOperationKind,
) -> LifecycleResult<LifecycleAdmissionEffect> {
    match state.admit(operation) {
        LifecycleOperationAdmission::Allowed { effect } => Ok(effect),
        LifecycleOperationAdmission::Rejected { reason } => {
            Err(LifecycleError::InvalidLifecycleState { reason })
        }
    }
}
