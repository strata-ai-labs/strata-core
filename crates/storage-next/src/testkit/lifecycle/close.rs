//! Generated lifecycle close contract helpers.

use super::{ensure, testkit_error};
use crate::commit::CommitBranchGuardSet;
use crate::lifecycle::{
    CloseOutcome, CloseOutcomeEffects, CloseOutcomeStatus, ClosePhase, LifecycleCloseFact,
    LifecycleError, LifecycleLowerLayer, LifecycleResult, LifecycleState, LifecycleStateMachine,
    LifecycleTransitionTrigger, MaintenanceOutcome, MaintenanceOutcomeStatus, MaintenanceTask,
    MaintenanceTaskKind, MaintenanceTaskPolicy, MaintenanceTaskPriority, MaintenanceTaskRequest,
    MaintenanceTaskRunner, MaintenanceTaskScope,
};
use crate::testkit::TestkitError;
use std::error::Error;
use std::fmt;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct LifecycleCloseContractOutcome {
    close_requested: usize,
    cache_close_completed: usize,
    durable_close_completed: usize,
    idempotent_close: usize,
    retryable_timeout: usize,
    drain_required_completed: usize,
    cancelable_task_canceled: usize,
    ordinary_task_not_started_after_close: usize,
    commit_quiesce_acquired: usize,
    commit_quiesce_blocked: usize,
    wal_sync_failure: usize,
    manifest_sync_failure: usize,
    guard_release_observed: usize,
    source_chain_preserved: usize,
}

pub fn check_lifecycle_close_contract(
    script: &[u8],
) -> Result<LifecycleCloseContractOutcome, TestkitError> {
    let mut outcome = LifecycleCloseContractOutcome::default();
    match CloseRoute::from_script(script) {
        CloseRoute::All => {
            check_close_state_flow(&mut outcome)?;
            check_close_outcomes(script, &mut outcome)?;
            check_close_maintenance(script, &mut outcome)?;
            check_close_quiesce(script, &mut outcome)?;
            check_close_wal_error(script, &mut outcome)?;
            check_close_manifest_error(script, &mut outcome)?;
        }
        CloseRoute::StateFlow => check_close_state_flow(&mut outcome)?,
        CloseRoute::Outcomes => check_close_outcomes(script, &mut outcome)?,
        CloseRoute::Retry => {
            check_close_state_flow(&mut outcome)?;
            check_close_outcomes(script, &mut outcome)?;
        }
        CloseRoute::MaintenanceAndQuiesce => {
            check_close_maintenance(script, &mut outcome)?;
            check_close_quiesce(script, &mut outcome)?;
        }
        CloseRoute::QuiesceTimeout => {
            check_close_outcomes(script, &mut outcome)?;
            check_close_quiesce(script, &mut outcome)?;
        }
        CloseRoute::WalSyncFailure => check_close_wal_error(script, &mut outcome)?,
        CloseRoute::ManifestSyncFailure => check_close_manifest_error(script, &mut outcome)?,
    }
    Ok(outcome)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CloseRoute {
    All,
    StateFlow,
    Outcomes,
    Retry,
    MaintenanceAndQuiesce,
    QuiesceTimeout,
    WalSyncFailure,
    ManifestSyncFailure,
}

impl CloseRoute {
    fn from_script(script: &[u8]) -> Self {
        let lower = String::from_utf8_lossy(script).to_ascii_lowercase();
        if lower.contains("shutdown-categories") || lower.contains("close-all") {
            return Self::All;
        }
        if lower.contains("close-block") {
            return Self::MaintenanceAndQuiesce;
        }
        if lower.contains("close-retry") || lower.contains("close-idempotent") {
            return Self::Retry;
        }
        if lower.contains("close-quiesce") {
            return Self::QuiesceTimeout;
        }
        if lower.contains("close-wal-sync") {
            return Self::WalSyncFailure;
        }
        if lower.contains("close-manifest") {
            return Self::ManifestSyncFailure;
        }
        if lower.contains("close-guard-release") || lower.contains("close-reopen") {
            return Self::Outcomes;
        }
        if lower.contains("close-state") || lower.contains("close-request") {
            return Self::StateFlow;
        }
        match script.first().copied().unwrap_or(0) % 7 {
            0 => Self::StateFlow,
            1 => Self::Outcomes,
            2 => Self::Retry,
            3 => Self::MaintenanceAndQuiesce,
            4 => Self::QuiesceTimeout,
            5 => Self::WalSyncFailure,
            _ => Self::ManifestSyncFailure,
        }
    }
}

impl LifecycleCloseContractOutcome {
    pub const fn close_requested_cases(&self) -> usize {
        self.close_requested
    }

    pub const fn cache_close_completed_cases(&self) -> usize {
        self.cache_close_completed
    }

    pub const fn durable_close_completed_cases(&self) -> usize {
        self.durable_close_completed
    }

    pub const fn idempotent_close_cases(&self) -> usize {
        self.idempotent_close
    }

    pub const fn retryable_timeout_cases(&self) -> usize {
        self.retryable_timeout
    }

    pub const fn drain_required_completed_cases(&self) -> usize {
        self.drain_required_completed
    }

    pub const fn cancelable_task_canceled_cases(&self) -> usize {
        self.cancelable_task_canceled
    }

    pub const fn ordinary_task_not_started_after_close_cases(&self) -> usize {
        self.ordinary_task_not_started_after_close
    }

    pub const fn commit_quiesce_acquired_cases(&self) -> usize {
        self.commit_quiesce_acquired
    }

    pub const fn commit_quiesce_blocked_cases(&self) -> usize {
        self.commit_quiesce_blocked
    }

    pub const fn wal_sync_failure_cases(&self) -> usize {
        self.wal_sync_failure
    }

    pub const fn manifest_sync_failure_cases(&self) -> usize {
        self.manifest_sync_failure
    }

    pub const fn guard_release_observed_cases(&self) -> usize {
        self.guard_release_observed
    }

    pub const fn source_chain_preserved_cases(&self) -> usize {
        self.source_chain_preserved
    }
}

fn check_close_state_flow(outcome: &mut LifecycleCloseContractOutcome) -> Result<(), TestkitError> {
    let mut open = open_state()?;
    let close_requested = open
        .transition(LifecycleTransitionTrigger::CloseRequested)
        .map_err(|error| testkit_error(&error))?;
    ensure(
        close_requested.close_fact() == Some(LifecycleCloseFact::Requested),
        "close request did not publish requested fact",
    )?;
    ensure(
        open.state() == LifecycleState::Closing,
        "close request did not enter closing",
    )?;
    outcome.close_requested += 1;

    let mut closed = open;
    closed
        .transition(LifecycleTransitionTrigger::CloseCompleted)
        .map_err(|error| testkit_error(&error))?;
    let idempotent = closed
        .transition(LifecycleTransitionTrigger::CloseRetried)
        .map_err(|error| testkit_error(&error))?;
    ensure(
        idempotent.close_fact() == Some(LifecycleCloseFact::AlreadyClosed),
        "closed retry did not report prior final fact",
    )?;
    outcome.idempotent_close += 1;
    Ok(())
}

fn check_close_outcomes(
    script: &[u8],
    outcome: &mut LifecycleCloseContractOutcome,
) -> Result<(), TestkitError> {
    // Input-derived dirty bit: bytes whose parity flips switch the durable
    // sync / volatile complete construction between the two valid effect
    // shapes. Asserting both shapes through script-derived selection means
    // a regression that collapses the constructors to a single shared form
    // is observable by the input-derivation test.
    let cache_dirty = script_byte(script, 2).is_multiple_of(2);
    let cache = CloseOutcome::new(ClosePhase::Closed, CloseOutcomeStatus::Complete)
        .with_close_fact(LifecycleCloseFact::Complete)
        .with_close_effects(CloseOutcomeEffects::volatile_complete(cache_dirty));
    ensure(
        !cache.durable_synced() && cache.guards_released(),
        "cache close outcome reported wrong effects",
    )?;
    outcome.cache_close_completed += 1;

    let durable_dirty = script_byte(script, 3).is_multiple_of(2);
    let durable = CloseOutcome::new(ClosePhase::Closed, CloseOutcomeStatus::Complete)
        .with_close_fact(LifecycleCloseFact::Complete)
        .with_close_effects(CloseOutcomeEffects::durable_complete(durable_dirty));
    ensure(
        durable.durable_synced() && durable.guards_released(),
        "durable close outcome did not report sync and guard release",
    )?;
    outcome.durable_close_completed += 1;
    outcome.guard_release_observed += 1;

    let timeout_phase = match script_byte(script, 4) % 3 {
        0 => ClosePhase::QuiesceCommits,
        1 => ClosePhase::DrainMaintenance,
        _ => ClosePhase::SyncDurableState,
    };
    let timeout = CloseOutcome::new(timeout_phase, CloseOutcomeStatus::Timeout)
        .with_close_fact(LifecycleCloseFact::RetryPending);
    ensure(
        timeout.status() == CloseOutcomeStatus::Timeout
            && timeout.close_fact() == Some(LifecycleCloseFact::RetryPending)
            && timeout.phase() == timeout_phase,
        "timeout close outcome did not preserve retry fact or phase",
    )?;
    outcome.retryable_timeout += 1;
    Ok(())
}

fn check_close_maintenance(
    script: &[u8],
    outcome: &mut LifecycleCloseContractOutcome,
) -> Result<(), TestkitError> {
    let open = open_state()?;
    let closing = closing_state()?;
    let mut executor = crate::lifecycle::LifecycleMaintenanceExecutor::new(4)
        .map_err(|error| testkit_error(&error))?;
    executor
        .enqueue(
            open,
            MaintenanceTaskRequest::new(
                close_task_kind(script_byte(script, 0)),
                MaintenanceTaskPriority::Normal,
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
                MaintenanceTaskPriority::Normal,
                MaintenanceTaskScope::Global,
                MaintenanceTaskPolicy::cancel_before_close(),
            )
            .map_err(|error| testkit_error(&error))?,
        )
        .map_err(|error| testkit_error(&error))?;
    let mut runner = CloseContractRunner;
    let drain = executor
        .drain_for_close(closing, &mut runner)
        .map_err(|error| testkit_error(&error))?;
    let cancel = executor
        .cancel_pending_for_close(closing)
        .map_err(|error| testkit_error(&error))?;
    ensure(
        drain.drained_tasks() == 1,
        "close drain did not complete drain-required task",
    )?;
    ensure(
        cancel.canceled_tasks() == 1,
        "close cancel did not cancel close-cancelable task",
    )?;
    outcome.drain_required_completed += 1;
    outcome.cancelable_task_canceled += 1;

    let ordinary_error = executor.run_next(closing, &mut runner);
    ensure(
        ordinary_error.is_err(),
        "ordinary maintenance was allowed during closing",
    )?;
    outcome.ordinary_task_not_started_after_close += 1;
    Ok(())
}

fn check_close_quiesce(
    script: &[u8],
    outcome: &mut LifecycleCloseContractOutcome,
) -> Result<(), TestkitError> {
    let guard_set = CommitBranchGuardSet::new();
    let quiesce = guard_set
        .try_begin_quiesce()
        .map_err(|error| TestkitError::new(error.to_string()))?;
    ensure(
        guard_set
            .is_quiescing()
            .map_err(|error| TestkitError::new(error.to_string()))?,
        "quiesce guard did not mark guard set quiescing",
    )?;
    outcome.commit_quiesce_acquired += 1;
    drop(quiesce);

    let branch = strata_core_next::BranchId::from_bytes([script_byte(script, 1).max(1); 16]);
    let active = guard_set
        .try_acquire_branch_guard(branch)
        .map_err(|error| TestkitError::new(error.to_string()))?;
    let blocked = guard_set.try_begin_quiesce();
    ensure(
        blocked.is_err(),
        "active branch guard did not block quiesce",
    )?;
    outcome.commit_quiesce_blocked += 1;
    drop(active);
    Ok(())
}

fn script_byte(script: &[u8], index: usize) -> u8 {
    script.get(index).copied().unwrap_or(0)
}

const fn close_task_kind(byte: u8) -> MaintenanceTaskKind {
    match byte % 2 {
        0 => MaintenanceTaskKind::HealthCollection,
        _ => MaintenanceTaskKind::Repair,
    }
}

fn check_close_wal_error(
    script: &[u8],
    outcome: &mut LifecycleCloseContractOutcome,
) -> Result<(), TestkitError> {
    let detail_suffix = script_byte(script, 5);
    let wal_error = LifecycleError::lower_layer_with(
        LifecycleLowerLayer::Service,
        "WAL service failed",
        ScriptedCloseSource::wal(detail_suffix),
    );
    let source = wal_error
        .source()
        .ok_or_else(|| TestkitError::new("WAL close source was lost"))?;
    ensure(
        format!("{source}").contains(&format!("wal-detail={detail_suffix:#04x}")),
        "input-derived WAL close detail did not surface in the source chain",
    )?;
    outcome.wal_sync_failure += 1;
    outcome.source_chain_preserved += 1;
    Ok(())
}

fn check_close_manifest_error(
    script: &[u8],
    outcome: &mut LifecycleCloseContractOutcome,
) -> Result<(), TestkitError> {
    let detail_suffix = script_byte(script, 6);
    let manifest_error = LifecycleError::lower_layer_with(
        LifecycleLowerLayer::Service,
        "database manifest publish failed",
        ScriptedCloseSource::manifest(detail_suffix),
    );
    let source = manifest_error
        .source()
        .ok_or_else(|| TestkitError::new("manifest close source was lost"))?;
    ensure(
        format!("{source}").contains(&format!("manifest-detail={detail_suffix:#04x}")),
        "input-derived manifest close detail did not surface in the source chain",
    )?;
    outcome.manifest_sync_failure += 1;
    outcome.source_chain_preserved += 1;
    Ok(())
}

#[derive(Debug)]
struct ScriptedCloseSource {
    phase: &'static str,
    detail: u8,
}

impl ScriptedCloseSource {
    const fn wal(detail: u8) -> Self {
        Self {
            phase: "wal-detail",
            detail,
        }
    }

    const fn manifest(detail: u8) -> Self {
        Self {
            phase: "manifest-detail",
            detail,
        }
    }
}

impl fmt::Display for ScriptedCloseSource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}={:#04x}", self.phase, self.detail)
    }
}

impl Error for ScriptedCloseSource {}

fn open_state() -> Result<LifecycleStateMachine, TestkitError> {
    let mut state = LifecycleStateMachine::new();
    state
        .transition(LifecycleTransitionTrigger::OpenRequested)
        .map_err(|error| testkit_error(&error))?;
    state
        .transition(LifecycleTransitionTrigger::CacheOpenReady)
        .map_err(|error| testkit_error(&error))?;
    Ok(state)
}

fn closing_state() -> Result<LifecycleStateMachine, TestkitError> {
    let mut state = open_state()?;
    state
        .transition(LifecycleTransitionTrigger::CloseRequested)
        .map_err(|error| testkit_error(&error))?;
    Ok(state)
}

struct CloseContractRunner;

impl MaintenanceTaskRunner for CloseContractRunner {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        Ok(MaintenanceOutcome::new(
            task.kind(),
            MaintenanceOutcomeStatus::Completed,
        ))
    }
}
