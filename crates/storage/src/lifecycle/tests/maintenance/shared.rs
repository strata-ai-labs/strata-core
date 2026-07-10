use super::*;
use std::error::Error;
use std::fmt;
use strata_core::BranchId;

pub(super) fn valid_kind_scopes() -> [(MaintenanceTaskKind, MaintenanceTaskScope); 16] {
    [
        (
            MaintenanceTaskKind::Flush,
            MaintenanceTaskScope::Branch(branch_id(1)),
        ),
        (MaintenanceTaskKind::Flush, MaintenanceTaskScope::Global),
        (
            MaintenanceTaskKind::Checkpoint,
            MaintenanceTaskScope::Checkpoint,
        ),
        (
            MaintenanceTaskKind::Checkpoint,
            MaintenanceTaskScope::Global,
        ),
        (
            MaintenanceTaskKind::WalTruncation,
            MaintenanceTaskScope::Wal,
        ),
        (
            MaintenanceTaskKind::Compaction,
            MaintenanceTaskScope::TableLevel {
                branch_id: branch_id(2),
                level: 1,
            },
        ),
        (
            MaintenanceTaskKind::Materialization,
            MaintenanceTaskScope::InheritedLayer {
                branch_id: branch_id(3),
                layer_index: 0,
            },
        ),
        (
            MaintenanceTaskKind::SnapshotPruning,
            MaintenanceTaskScope::Retention,
        ),
        (
            MaintenanceTaskKind::Retention,
            MaintenanceTaskScope::Retention,
        ),
        (
            MaintenanceTaskKind::Quarantine,
            MaintenanceTaskScope::Quarantine,
        ),
        (
            MaintenanceTaskKind::Purge,
            MaintenanceTaskScope::Branch(branch_id(4)),
        ),
        (MaintenanceTaskKind::Purge, MaintenanceTaskScope::Quarantine),
        (
            MaintenanceTaskKind::Repair,
            MaintenanceTaskScope::Branch(branch_id(5)),
        ),
        (
            MaintenanceTaskKind::Repair,
            MaintenanceTaskScope::Quarantine,
        ),
        (MaintenanceTaskKind::Repair, MaintenanceTaskScope::Global),
        (
            MaintenanceTaskKind::HealthCollection,
            MaintenanceTaskScope::Global,
        ),
    ]
}

pub(super) fn run_all_task_ids(
    state: LifecycleStateMachine,
    executor: &mut LifecycleMaintenanceExecutor,
    runner: &mut impl MaintenanceTaskRunner,
) -> Vec<u64> {
    let mut ids = Vec::new();
    while let Some(outcome) = executor.run_next(state, runner).expect("run next") {
        ids.push(outcome.task_id().expect("task id").get());
    }
    ids
}

pub(super) fn new_state() -> LifecycleStateMachine {
    LifecycleStateMachine::new()
}

pub(super) fn opening_state() -> LifecycleStateMachine {
    let mut state = LifecycleStateMachine::new();
    state
        .transition(LifecycleTransitionTrigger::OpenRequested)
        .expect("open requested");
    state
}

pub(super) fn recovering_state() -> LifecycleStateMachine {
    let mut state = opening_state();
    state
        .transition(LifecycleTransitionTrigger::DurableRecoveryRequired)
        .expect("recovery required");
    state
}

pub(super) fn open_state() -> LifecycleStateMachine {
    let mut state = LifecycleStateMachine::new();
    state
        .transition(LifecycleTransitionTrigger::OpenRequested)
        .expect("open requested");
    state
        .transition(LifecycleTransitionTrigger::CacheOpenReady)
        .expect("cache open ready");
    state
}

pub(super) fn closing_state() -> LifecycleStateMachine {
    let mut state = open_state();
    state
        .transition(LifecycleTransitionTrigger::CloseRequested)
        .expect("close requested");
    state
}

pub(super) fn closed_state() -> LifecycleStateMachine {
    let mut state = closing_state();
    state
        .transition(LifecycleTransitionTrigger::CloseCompleted)
        .expect("close completed");
    state
}

pub(super) fn failed_state() -> LifecycleStateMachine {
    let mut state = opening_state();
    state
        .transition(LifecycleTransitionTrigger::PhaseFailed {
            reason: "test failure",
        })
        .expect("failed");
    state
}

pub(super) fn health_request(policy: MaintenanceTaskPolicy) -> MaintenanceTaskRequest {
    MaintenanceTaskRequest::new(
        MaintenanceTaskKind::HealthCollection,
        MaintenanceTaskPriority::Normal,
        MaintenanceTaskScope::Global,
        policy,
    )
    .expect("health request")
}

pub(super) fn repair_request(
    priority: MaintenanceTaskPriority,
    policy: MaintenanceTaskPolicy,
) -> MaintenanceTaskRequest {
    MaintenanceTaskRequest::new(
        MaintenanceTaskKind::Repair,
        priority,
        MaintenanceTaskScope::Global,
        policy,
    )
    .expect("repair request")
}

pub(super) fn branch_id(byte: u8) -> BranchId {
    BranchId::from_bytes([byte; BranchId::BYTE_LEN])
}

pub(super) struct RecordingRunner {
    status: MaintenanceOutcomeStatus,
}

pub(super) struct EffectsRunner {
    pub(super) status: MaintenanceOutcomeStatus,
    pub(super) affected_objects: usize,
    pub(super) bytes_reclaimed: u64,
    pub(super) retryable: bool,
}

impl MaintenanceTaskRunner for EffectsRunner {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        Ok(
            MaintenanceOutcome::new(task.kind(), self.status).with_effects(
                self.affected_objects,
                self.bytes_reclaimed,
                self.retryable,
            ),
        )
    }
}

impl RecordingRunner {
    pub(super) const fn completed() -> Self {
        Self {
            status: MaintenanceOutcomeStatus::Completed,
        }
    }

    pub(super) const fn failed() -> Self {
        Self {
            status: MaintenanceOutcomeStatus::Failed,
        }
    }

    pub(super) const fn canceled() -> Self {
        Self {
            status: MaintenanceOutcomeStatus::Canceled,
        }
    }
}

impl MaintenanceTaskRunner for RecordingRunner {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        Ok(MaintenanceOutcome::new(task.kind(), self.status))
    }
}

pub(super) struct ErrorRunner;

impl MaintenanceTaskRunner for ErrorRunner {
    fn run_task(&mut self, _task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        Err(LifecycleError::lower_layer_with(
            LifecycleLowerLayer::Backend,
            "maintenance runner failed",
            TestSourceError,
        ))
    }
}

pub(super) struct FailAt {
    point: MaintenanceFaultPoint,
}

#[derive(Default)]
pub(super) struct RecordingFaultHook {
    pub(super) points: Vec<MaintenanceFaultPoint>,
    pub(super) task_ids: Vec<u64>,
}

impl MaintenanceFaultHook for RecordingFaultHook {
    fn check(
        &mut self,
        point: MaintenanceFaultPoint,
        task: Option<&MaintenanceTask>,
    ) -> LifecycleResult<()> {
        self.points.push(point);
        if let Some(task) = task {
            self.task_ids.push(task.id().get());
        }
        Ok(())
    }
}

impl FailAt {
    pub(super) const fn new(point: MaintenanceFaultPoint) -> Self {
        Self { point }
    }
}

impl MaintenanceFaultHook for FailAt {
    fn check(
        &mut self,
        point: MaintenanceFaultPoint,
        _task: Option<&MaintenanceTask>,
    ) -> LifecycleResult<()> {
        if point == self.point {
            Err(LifecycleError::MaintenanceFailed {
                reason: "injected maintenance fault",
            })
        } else {
            Ok(())
        }
    }
}

#[derive(Debug)]
struct TestSourceError;

impl fmt::Display for TestSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("test source error")
    }
}

impl Error for TestSourceError {}
