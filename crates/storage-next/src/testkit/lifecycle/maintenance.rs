//! Generated lifecycle maintenance executor contract helpers.

use super::{ensure, script_byte, testkit_error};
use crate::lifecycle::{
    LifecycleMaintenanceExecutor, LifecycleOperationKind, LifecycleResult, LifecycleState,
    LifecycleStateMachine, LifecycleTransitionTrigger, MaintenanceEnqueueStatus,
    MaintenanceFaultHook, MaintenanceFaultPoint, MaintenanceOutcome, MaintenanceOutcomeStatus,
    MaintenanceTask, MaintenanceTaskKind, MaintenanceTaskPolicy, MaintenanceTaskPriority,
    MaintenanceTaskRequest, MaintenanceTaskRunner, MaintenanceTaskScope,
};
use crate::testkit::TestkitError;
use strata_core_next::BranchId;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LifecycleMaintenanceContractOutcome {
    enqueued: usize,
    coalesced: usize,
    ran: usize,
    canceled: usize,
    drained: usize,
    faulted: usize,
    queue_full: usize,
    admission_rejected: usize,
    model_steps: usize,
}

pub fn check_lifecycle_maintenance_contract(
    script: &[u8],
) -> Result<LifecycleMaintenanceContractOutcome, TestkitError> {
    let mut outcome = LifecycleMaintenanceContractOutcome::default();
    let open = state_for(LifecycleState::Open)?;
    let closing = state_for(LifecycleState::Closing)?;
    let mut executor =
        LifecycleMaintenanceExecutor::new(8).map_err(|error| testkit_error(&error))?;
    check_input_enqueue_and_coalesce(script, open, &mut executor, &mut outcome)?;
    check_input_run(script, open, &mut executor, &mut outcome)?;
    check_input_cancel_and_drain(script, open, closing, &mut executor, &mut outcome)?;
    check_input_fault(script, open, &mut outcome)?;
    check_input_queue_full_and_admission(script, open, closing, &mut outcome)?;
    check_input_model_script(script, open, closing, &mut outcome)?;
    Ok(outcome)
}

impl LifecycleMaintenanceContractOutcome {
    pub const fn input_enqueue_cases(&self) -> usize {
        self.enqueued
    }

    pub const fn input_coalesce_cases(&self) -> usize {
        self.coalesced
    }

    pub const fn input_run_cases(&self) -> usize {
        self.ran
    }

    pub const fn input_cancel_cases(&self) -> usize {
        self.canceled
    }

    pub const fn input_drain_cases(&self) -> usize {
        self.drained
    }

    pub const fn input_fault_cases(&self) -> usize {
        self.faulted
    }

    pub const fn input_queue_full_cases(&self) -> usize {
        self.queue_full
    }

    pub const fn input_admission_rejection_cases(&self) -> usize {
        self.admission_rejected
    }

    pub const fn input_model_step_cases(&self) -> usize {
        self.model_steps
    }
}

fn check_input_enqueue_and_coalesce(
    script: &[u8],
    open: LifecycleStateMachine,
    executor: &mut LifecycleMaintenanceExecutor,
    outcome: &mut LifecycleMaintenanceContractOutcome,
) -> Result<(), TestkitError> {
    let request = MaintenanceTaskRequest::new(
        MaintenanceTaskKind::Flush,
        priority_from_byte(script_byte(script, 0)),
        MaintenanceTaskScope::Branch(branch_id(script_byte(script, 1))),
        MaintenanceTaskPolicy::coalescing(),
    )
    .map_err(|error| testkit_error(&error))?;
    let first = executor
        .enqueue(open, request)
        .map_err(|error| testkit_error(&error))?;
    let second = executor
        .enqueue(open, request)
        .map_err(|error| testkit_error(&error))?;
    ensure(
        first.status() == MaintenanceEnqueueStatus::Enqueued,
        "maintenance enqueue did not enqueue first task",
    )?;
    ensure(
        second.status() == MaintenanceEnqueueStatus::Coalesced,
        "maintenance enqueue did not coalesce duplicate task",
    )?;
    ensure(
        pending_task_ids(executor) == vec![first.task_id().get()],
        "maintenance coalescing changed pending task order",
    )?;
    outcome.enqueued += 1;
    outcome.coalesced += 1;
    Ok(())
}

fn check_input_run(
    script: &[u8],
    open: LifecycleStateMachine,
    executor: &mut LifecycleMaintenanceExecutor,
    outcome: &mut LifecycleMaintenanceContractOutcome,
) -> Result<(), TestkitError> {
    let request = MaintenanceTaskRequest::new(
        MaintenanceTaskKind::Repair,
        priority_from_byte(script_byte(script, 2)),
        MaintenanceTaskScope::Global,
        MaintenanceTaskPolicy::ordinary(),
    )
    .map_err(|error| testkit_error(&error))?;
    executor
        .enqueue(open, request)
        .map_err(|error| testkit_error(&error))?;
    let mut runner = ScriptRunner::new(status_from_byte(script_byte(script, 3)));
    let outcome_value = executor
        .run_next(open, &mut runner)
        .map_err(|error| testkit_error(&error))?
        .ok_or_else(|| TestkitError::new("maintenance run unexpectedly found no task"))?;
    ensure(
        outcome_value.task_id().is_some(),
        "maintenance outcome lost task id",
    )?;
    ensure(
        outcome_value.status() == runner.status(),
        "maintenance runner status was not preserved",
    )?;
    ensure(
        executor.status().active_task().is_none(),
        "maintenance run left active task set",
    )?;
    outcome.ran += 1;
    Ok(())
}

fn check_input_cancel_and_drain(
    script: &[u8],
    open: LifecycleStateMachine,
    closing: LifecycleStateMachine,
    _executor: &mut LifecycleMaintenanceExecutor,
    outcome: &mut LifecycleMaintenanceContractOutcome,
) -> Result<(), TestkitError> {
    let mut executor =
        LifecycleMaintenanceExecutor::new(4).map_err(|error| testkit_error(&error))?;
    executor
        .enqueue(
            open,
            MaintenanceTaskRequest::new(
                MaintenanceTaskKind::HealthCollection,
                priority_from_byte(script_byte(script, 4)),
                MaintenanceTaskScope::Global,
                MaintenanceTaskPolicy::drain_before_close(),
            )
            .map_err(|error| testkit_error(&error))?,
        )
        .map_err(|error| testkit_error(&error))?;
    executor
        .enqueue(
            open,
            MaintenanceTaskRequest::new(
                MaintenanceTaskKind::Repair,
                priority_from_byte(script_byte(script, 5)),
                MaintenanceTaskScope::Global,
                MaintenanceTaskPolicy::cancel_before_close(),
            )
            .map_err(|error| testkit_error(&error))?,
        )
        .map_err(|error| testkit_error(&error))?;
    executor
        .enqueue(
            open,
            MaintenanceTaskRequest::new(
                MaintenanceTaskKind::Repair,
                MaintenanceTaskPriority::Low,
                MaintenanceTaskScope::Global,
                MaintenanceTaskPolicy::ordinary(),
            )
            .map_err(|error| testkit_error(&error))?,
        )
        .map_err(|error| testkit_error(&error))?;
    let mut runner = ScriptRunner::new(MaintenanceOutcomeStatus::Completed);
    let drain = executor
        .drain_for_close(closing, &mut runner)
        .map_err(|error| testkit_error(&error))?;
    let cancel = executor
        .cancel_pending_for_close(closing)
        .map_err(|error| testkit_error(&error))?;
    ensure(
        drain.drained_tasks() == 1,
        "maintenance drain did not run drain-required task",
    )?;
    ensure(
        cancel.canceled_tasks() == 1,
        "maintenance cancel did not remove cancelable task",
    )?;
    ensure(
        pending_task_ids(&executor) == vec![3],
        "maintenance drain/cancel left unexpected pending order",
    )?;
    outcome.drained += 1;
    outcome.canceled += 1;
    Ok(())
}

fn check_input_fault(
    script: &[u8],
    open: LifecycleStateMachine,
    outcome: &mut LifecycleMaintenanceContractOutcome,
) -> Result<(), TestkitError> {
    let mut executor =
        LifecycleMaintenanceExecutor::new(2).map_err(|error| testkit_error(&error))?;
    executor
        .enqueue(
            open,
            MaintenanceTaskRequest::new(
                MaintenanceTaskKind::Repair,
                priority_from_byte(script_byte(script, 6)),
                MaintenanceTaskScope::Global,
                MaintenanceTaskPolicy::ordinary(),
            )
            .map_err(|error| testkit_error(&error))?,
        )
        .map_err(|error| testkit_error(&error))?;
    let mut runner = ScriptRunner::new(MaintenanceOutcomeStatus::Completed);
    let mut fault = ScriptFault::new(point_from_byte(script_byte(script, 7)));
    let result = executor.run_next_with_fault(open, &mut runner, &mut fault);
    ensure(
        result.is_err() || executor.stats().completed() == 1 || executor.stats().failed() == 1,
        "maintenance fault contract lost both success and typed failure",
    )?;
    ensure(
        executor.status().active_task().is_none(),
        "maintenance fault contract left an active task behind",
    )?;
    outcome.faulted += 1;
    Ok(())
}

fn check_input_queue_full_and_admission(
    script: &[u8],
    open: LifecycleStateMachine,
    closing: LifecycleStateMachine,
    outcome: &mut LifecycleMaintenanceContractOutcome,
) -> Result<(), TestkitError> {
    let mut executor =
        LifecycleMaintenanceExecutor::new(1).map_err(|error| testkit_error(&error))?;
    executor
        .enqueue(
            open,
            MaintenanceTaskRequest::new(
                MaintenanceTaskKind::Flush,
                priority_from_byte(script_byte(script, 8)),
                MaintenanceTaskScope::Branch(branch_id(script_byte(script, 9))),
                MaintenanceTaskPolicy::ordinary(),
            )
            .map_err(|error| testkit_error(&error))?,
        )
        .map_err(|error| testkit_error(&error))?;
    let queue_full = executor
        .enqueue(
            open,
            MaintenanceTaskRequest::new(
                MaintenanceTaskKind::Repair,
                priority_from_byte(script_byte(script, 10)),
                MaintenanceTaskScope::Global,
                MaintenanceTaskPolicy::ordinary(),
            )
            .map_err(|error| testkit_error(&error))?,
        )
        .expect_err("queue-full contract should reject second task");
    ensure(
        queue_full.code() == "failed_precondition.lifecycle.maintenance",
        "queue-full contract returned wrong error code",
    )?;
    ensure(
        executor.status().pending_tasks() == 1 && executor.stats().queue_full() == 1,
        "queue-full contract mutated pending state incorrectly",
    )?;
    outcome.queue_full += 1;

    let admission = executor
        .enqueue(
            closing,
            MaintenanceTaskRequest::new(
                MaintenanceTaskKind::HealthCollection,
                priority_from_byte(script_byte(script, 11)),
                MaintenanceTaskScope::Global,
                MaintenanceTaskPolicy::ordinary(),
            )
            .map_err(|error| testkit_error(&error))?,
        )
        .expect_err("admission contract should reject ordinary work while closing");
    ensure(
        admission.code() == "failed_precondition.lifecycle.state",
        "admission contract returned wrong error code",
    )?;
    ensure(
        executor.status().pending_tasks() == 1,
        "admission rejection mutated pending state",
    )?;
    outcome.admission_rejected += 1;
    Ok(())
}

fn check_input_model_script(
    script: &[u8],
    open: LifecycleStateMachine,
    closing: LifecycleStateMachine,
    outcome: &mut LifecycleMaintenanceContractOutcome,
) -> Result<(), TestkitError> {
    let mut executor =
        LifecycleMaintenanceExecutor::new(3).map_err(|error| testkit_error(&error))?;
    let mut model = MaintenanceModel::new(3);
    let mut runner = ScriptRunner::new(MaintenanceOutcomeStatus::Completed);
    for step in 0..12 {
        apply_model_step(
            script,
            step,
            open,
            closing,
            &mut executor,
            &mut model,
            &mut runner,
            outcome,
        )?;
        assert_model_matches(&executor, &model)?;
        outcome.model_steps += 1;
    }
    ensure(
        model.admission_rejected > 0,
        "model script did not exercise admission rejection",
    )?;
    ensure(
        model.stats.queue_full() > 0 || model.stats.enqueued() > 0,
        "model script did not exercise enqueue accounting",
    )?;
    Ok(())
}

fn apply_model_step(
    script: &[u8],
    step: usize,
    open: LifecycleStateMachine,
    closing: LifecycleStateMachine,
    executor: &mut LifecycleMaintenanceExecutor,
    model: &mut MaintenanceModel,
    runner: &mut ScriptRunner,
    outcome: &mut LifecycleMaintenanceContractOutcome,
) -> Result<(), TestkitError> {
    match step {
        0 => model_enqueue_open(
            executor,
            model,
            open,
            model_flush_request(script, 0)?,
            "model enqueue expected only queue-full errors",
        )
        .map(|()| outcome.enqueued += 1),
        1 => model_enqueue_open(
            executor,
            model,
            open,
            model_flush_request(script, 0)?,
            "model duplicate enqueue expected only queue-full errors",
        )
        .map(|()| outcome.coalesced += 1),
        2 => model_enqueue_open(
            executor,
            model,
            open,
            model_repair_request(script, step)?,
            "model repair enqueue expected only queue-full errors",
        )
        .map(|()| outcome.enqueued += 1),
        3 => model_enqueue_open(
            executor,
            model,
            open,
            model_checkpoint_request(script, step)?,
            "model checkpoint enqueue expected only queue-full errors",
        )
        .map(|()| outcome.enqueued += 1),
        4 => model_queue_full(
            executor,
            model,
            open,
            model_repair_request(script, step)?,
            outcome,
        ),
        5 | 10 => model_run_next(script, step, open, executor, model, runner, outcome),
        6 => model_cancel_for_close(closing, executor, model, outcome),
        7 => model_drain_for_close(closing, executor, model, runner, outcome),
        8 => model_admission_rejection(script, step, closing, executor, model, outcome),
        9 => model_enqueue_open(
            executor,
            model,
            open,
            model_checkpoint_request(script, step)?,
            "model post-drain enqueue expected only queue-full errors",
        )
        .map(|()| outcome.enqueued += 1),
        _ => assert_model_matches(executor, model),
    }
}

fn model_enqueue_open(
    executor: &mut LifecycleMaintenanceExecutor,
    model: &mut MaintenanceModel,
    open: LifecycleStateMachine,
    request: MaintenanceTaskRequest,
    error_message: &'static str,
) -> Result<(), TestkitError> {
    let actual = executor.enqueue(open, request);
    model.enqueue_open(request);
    if let Err(error) = actual {
        ensure(
            error.code() == "failed_precondition.lifecycle.maintenance",
            error_message,
        )?;
    }
    Ok(())
}

fn model_queue_full(
    executor: &mut LifecycleMaintenanceExecutor,
    model: &mut MaintenanceModel,
    open: LifecycleStateMachine,
    request: MaintenanceTaskRequest,
    outcome: &mut LifecycleMaintenanceContractOutcome,
) -> Result<(), TestkitError> {
    let error = executor
        .enqueue(open, request)
        .expect_err("model queue-full rejection");
    ensure(
        error.code() == "failed_precondition.lifecycle.maintenance",
        "model queue-full rejection returned wrong code",
    )?;
    model.enqueue_open(request);
    outcome.queue_full += 1;
    Ok(())
}

fn model_run_next(
    script: &[u8],
    step: usize,
    open: LifecycleStateMachine,
    executor: &mut LifecycleMaintenanceExecutor,
    model: &mut MaintenanceModel,
    runner: &mut ScriptRunner,
    outcome: &mut LifecycleMaintenanceContractOutcome,
) -> Result<(), TestkitError> {
    runner.status = status_from_byte(script_byte(script, 40 + step));
    executor
        .run_next(open, runner)
        .map_err(|error| testkit_error(&error))?;
    model.run_next(runner.status());
    outcome.ran += 1;
    Ok(())
}

fn model_cancel_for_close(
    closing: LifecycleStateMachine,
    executor: &mut LifecycleMaintenanceExecutor,
    model: &mut MaintenanceModel,
    outcome: &mut LifecycleMaintenanceContractOutcome,
) -> Result<(), TestkitError> {
    executor
        .cancel_pending_for_close(closing)
        .map_err(|error| testkit_error(&error))?;
    model.cancel_pending_for_close();
    outcome.canceled += 1;
    Ok(())
}

fn model_drain_for_close(
    closing: LifecycleStateMachine,
    executor: &mut LifecycleMaintenanceExecutor,
    model: &mut MaintenanceModel,
    runner: &mut ScriptRunner,
    outcome: &mut LifecycleMaintenanceContractOutcome,
) -> Result<(), TestkitError> {
    runner.status = MaintenanceOutcomeStatus::Completed;
    executor
        .drain_for_close(closing, runner)
        .map_err(|error| testkit_error(&error))?;
    model.drain_for_close();
    outcome.drained += 1;
    Ok(())
}

fn model_admission_rejection(
    script: &[u8],
    step: usize,
    closing: LifecycleStateMachine,
    executor: &mut LifecycleMaintenanceExecutor,
    model: &mut MaintenanceModel,
    outcome: &mut LifecycleMaintenanceContractOutcome,
) -> Result<(), TestkitError> {
    let request = model_health_request(script, step)?;
    let error = executor
        .enqueue(closing, request)
        .expect_err("model admission rejection");
    ensure(
        error.code() == "failed_precondition.lifecycle.state",
        "model admission rejection returned wrong code",
    )?;
    model.admission_rejected += 1;
    outcome.admission_rejected += 1;
    Ok(())
}

fn pending_task_ids(executor: &LifecycleMaintenanceExecutor) -> Vec<u64> {
    executor
        .pending_tasks()
        .iter()
        .map(|task| task.id().get())
        .collect()
}

fn assert_model_matches(
    executor: &LifecycleMaintenanceExecutor,
    model: &MaintenanceModel,
) -> Result<(), TestkitError> {
    ensure(
        pending_task_ids(executor) == model.pending_task_ids(),
        "maintenance model pending order diverged",
    )?;
    ensure(
        executor.status().active_task().is_none(),
        "maintenance model found active task after operation boundary",
    )?;
    let stats = executor.stats();
    ensure(
        stats.enqueued() == model.stats.enqueued()
            && stats.coalesced() == model.stats.coalesced()
            && stats.started() == model.stats.started()
            && stats.completed() == model.stats.completed()
            && stats.deferred() == model.stats.deferred()
            && stats.failed() == model.stats.failed()
            && stats.canceled() == model.stats.canceled()
            && stats.drained() == model.stats.drained()
            && stats.queue_full() == model.stats.queue_full(),
        "maintenance model stats diverged",
    )?;
    Ok(())
}

fn model_flush_request(script: &[u8], step: usize) -> Result<MaintenanceTaskRequest, TestkitError> {
    MaintenanceTaskRequest::new(
        MaintenanceTaskKind::Flush,
        priority_from_byte(script_byte(script, 52 + step)),
        MaintenanceTaskScope::Branch(branch_id(script_byte(script, 64 + step))),
        MaintenanceTaskPolicy::coalescing(),
    )
    .map_err(|error| testkit_error(&error))
}

fn model_repair_request(
    script: &[u8],
    step: usize,
) -> Result<MaintenanceTaskRequest, TestkitError> {
    MaintenanceTaskRequest::new(
        MaintenanceTaskKind::Repair,
        priority_from_byte(script_byte(script, 76 + step)),
        MaintenanceTaskScope::Global,
        MaintenanceTaskPolicy::cancel_before_close(),
    )
    .map_err(|error| testkit_error(&error))
}

fn model_health_request(
    script: &[u8],
    step: usize,
) -> Result<MaintenanceTaskRequest, TestkitError> {
    MaintenanceTaskRequest::new(
        MaintenanceTaskKind::HealthCollection,
        priority_from_byte(script_byte(script, 88 + step)),
        MaintenanceTaskScope::Global,
        MaintenanceTaskPolicy::ordinary(),
    )
    .map_err(|error| testkit_error(&error))
}

fn model_checkpoint_request(
    script: &[u8],
    step: usize,
) -> Result<MaintenanceTaskRequest, TestkitError> {
    MaintenanceTaskRequest::new(
        MaintenanceTaskKind::Checkpoint,
        priority_from_byte(script_byte(script, 100 + step)),
        MaintenanceTaskScope::Checkpoint,
        MaintenanceTaskPolicy::drain_before_close(),
    )
    .map_err(|error| testkit_error(&error))
}

fn state_for(state: LifecycleState) -> Result<LifecycleStateMachine, TestkitError> {
    let mut machine = LifecycleStateMachine::new();
    match state {
        LifecycleState::Open => {
            machine
                .transition(LifecycleTransitionTrigger::OpenRequested)
                .map_err(|error| testkit_error(&error))?;
            machine
                .transition(LifecycleTransitionTrigger::CacheOpenReady)
                .map_err(|error| testkit_error(&error))?;
        }
        LifecycleState::Closing => {
            machine = state_for(LifecycleState::Open)?;
            machine
                .transition(LifecycleTransitionTrigger::CloseRequested)
                .map_err(|error| testkit_error(&error))?;
        }
        _ => {
            return Err(TestkitError::new(
                "unsupported lifecycle maintenance test state",
            ));
        }
    }
    ensure(
        machine
            .admit(match state {
                LifecycleState::Open => LifecycleOperationKind::OrdinaryMaintenance,
                LifecycleState::Closing => LifecycleOperationKind::CloseRequiredDrain,
                _ => LifecycleOperationKind::HealthQuery,
            })
            .is_allowed(),
        "maintenance test state did not admit expected operation",
    )?;
    Ok(machine)
}

fn priority_from_byte(byte: u8) -> MaintenanceTaskPriority {
    match byte % 4 {
        0 => MaintenanceTaskPriority::Critical,
        1 => MaintenanceTaskPriority::High,
        2 => MaintenanceTaskPriority::Normal,
        _ => MaintenanceTaskPriority::Low,
    }
}

fn status_from_byte(byte: u8) -> MaintenanceOutcomeStatus {
    match byte % 3 {
        0 => MaintenanceOutcomeStatus::Completed,
        1 => MaintenanceOutcomeStatus::Deferred,
        _ => MaintenanceOutcomeStatus::Failed,
    }
}

fn point_from_byte(byte: u8) -> MaintenanceFaultPoint {
    match byte % 2 {
        0 => MaintenanceFaultPoint::AtTaskStart,
        _ => MaintenanceFaultPoint::AfterTaskRun,
    }
}

fn branch_id(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}

struct ScriptRunner {
    status: MaintenanceOutcomeStatus,
}

impl ScriptRunner {
    const fn new(status: MaintenanceOutcomeStatus) -> Self {
        Self { status }
    }

    const fn status(&self) -> MaintenanceOutcomeStatus {
        self.status
    }
}

impl MaintenanceTaskRunner for ScriptRunner {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        Ok(MaintenanceOutcome::new(task.kind(), self.status))
    }
}

struct ScriptFault {
    point: MaintenanceFaultPoint,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ModelTask {
    id: u64,
    sequence: u64,
    priority: MaintenanceTaskPriority,
    policy: MaintenanceTaskPolicy,
    coalesce_key: Option<crate::lifecycle::MaintenanceCoalesceKey>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct MaintenanceModel {
    next_id: u64,
    max_queue_depth: usize,
    pending: Vec<ModelTask>,
    stats: ModelStats,
    admission_rejected: usize,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
struct ModelStats {
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

impl MaintenanceModel {
    fn new(max_queue_depth: usize) -> Self {
        Self {
            next_id: 1,
            max_queue_depth,
            pending: Vec::new(),
            stats: ModelStats::default(),
            admission_rejected: 0,
        }
    }

    fn enqueue_open(&mut self, request: MaintenanceTaskRequest) {
        if let Some(key) = request.coalesce_key() {
            if self
                .pending
                .iter()
                .any(|task| task.coalesce_key == Some(key))
            {
                self.stats = increment_coalesced(self.stats);
                return;
            }
        }
        if self.pending.len() >= self.max_queue_depth {
            self.stats = increment_queue_full(self.stats);
            return;
        }
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        self.pending.push(ModelTask {
            id,
            sequence: id,
            priority: request.priority(),
            policy: request.policy(),
            coalesce_key: request.coalesce_key(),
        });
        self.stats = increment_enqueued(self.stats);
    }

    fn run_next(&mut self, status: MaintenanceOutcomeStatus) {
        let Some(index) = self.next_index(|_| true) else {
            return;
        };
        self.pending.remove(index);
        self.stats = increment_started(self.stats);
        self.stats = increment_outcome(self.stats, status, false);
    }

    fn cancel_pending_for_close(&mut self) {
        let before = self.pending.len();
        self.pending.retain(|task| {
            task.policy.close_policy()
                != crate::lifecycle::MaintenanceClosePolicy::CancelBeforeClose
        });
        let canceled = before - self.pending.len();
        self.stats = add_canceled(self.stats, canceled);
    }

    fn drain_for_close(&mut self) {
        while let Some(index) = self.next_index(|task| {
            task.policy.close_policy() == crate::lifecycle::MaintenanceClosePolicy::DrainBeforeClose
        }) {
            self.pending.remove(index);
            self.stats = increment_started(self.stats);
            self.stats = increment_outcome(self.stats, MaintenanceOutcomeStatus::Completed, true);
        }
    }

    fn pending_task_ids(&self) -> Vec<u64> {
        self.pending.iter().map(|task| task.id).collect()
    }

    fn next_index(&self, predicate: impl Fn(&ModelTask) -> bool) -> Option<usize> {
        self.pending
            .iter()
            .enumerate()
            .filter(|(_, task)| predicate(task))
            .min_by_key(|(_, task)| (priority_rank(task.priority), task.sequence))
            .map(|(index, _)| index)
    }
}

fn priority_rank(priority: MaintenanceTaskPriority) -> u8 {
    match priority {
        MaintenanceTaskPriority::Critical => 0,
        MaintenanceTaskPriority::High => 1,
        MaintenanceTaskPriority::Normal => 2,
        MaintenanceTaskPriority::Low => 3,
    }
}

impl ModelStats {
    const fn enqueued(self) -> usize {
        self.enqueued
    }

    const fn coalesced(self) -> usize {
        self.coalesced
    }

    const fn started(self) -> usize {
        self.started
    }

    const fn completed(self) -> usize {
        self.completed
    }

    const fn deferred(self) -> usize {
        self.deferred
    }

    const fn failed(self) -> usize {
        self.failed
    }

    const fn canceled(self) -> usize {
        self.canceled
    }

    const fn drained(self) -> usize {
        self.drained
    }

    const fn queue_full(self) -> usize {
        self.queue_full
    }
}

fn increment_enqueued(mut stats: ModelStats) -> ModelStats {
    stats.enqueued = stats.enqueued.saturating_add(1);
    stats
}

fn increment_coalesced(mut stats: ModelStats) -> ModelStats {
    stats.coalesced = stats.coalesced.saturating_add(1);
    stats
}

fn increment_queue_full(mut stats: ModelStats) -> ModelStats {
    stats.queue_full = stats.queue_full.saturating_add(1);
    stats
}

fn increment_started(mut stats: ModelStats) -> ModelStats {
    stats.started = stats.started.saturating_add(1);
    stats
}

fn add_canceled(mut stats: ModelStats, count: usize) -> ModelStats {
    stats.canceled = stats.canceled.saturating_add(count);
    stats
}

fn increment_outcome(
    mut stats: ModelStats,
    outcome_status: MaintenanceOutcomeStatus,
    draining: bool,
) -> ModelStats {
    match outcome_status {
        MaintenanceOutcomeStatus::Completed => {
            stats.completed = stats.completed.saturating_add(1);
        }
        MaintenanceOutcomeStatus::Deferred => {
            stats.deferred = stats.deferred.saturating_add(1);
        }
        MaintenanceOutcomeStatus::Canceled => {
            stats.canceled = stats.canceled.saturating_add(1);
        }
        MaintenanceOutcomeStatus::Failed => {
            stats.failed = stats.failed.saturating_add(1);
        }
    }
    if draining {
        stats.drained = stats.drained.saturating_add(1);
    }
    stats
}

impl ScriptFault {
    const fn new(point: MaintenanceFaultPoint) -> Self {
        Self { point }
    }
}

impl MaintenanceFaultHook for ScriptFault {
    fn check(
        &mut self,
        point: MaintenanceFaultPoint,
        _task: Option<&MaintenanceTask>,
    ) -> LifecycleResult<()> {
        if point == self.point {
            Err(crate::lifecycle::LifecycleError::MaintenanceFailed {
                reason: "scripted maintenance fault",
            })
        } else {
            Ok(())
        }
    }
}
