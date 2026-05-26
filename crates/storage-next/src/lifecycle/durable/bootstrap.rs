//! Commit-runtime bootstrap after durable recovery.

use super::{
    branch_error, commit_error, require_admitted, LifecycleDurableLocalServices,
    LifecycleDurableLocalShell,
};
use crate::branch::{BranchLocalState, BranchReadView};
use crate::commit::{
    CommitBatch, CommitBranchGenerationGuard, CommitBranchGuardSet, CommitBranchRegistry,
    CommitDurabilityClass, CommitDurableRuntime, CommitFactAllocator, CommitManualTimestampSource,
    CommitOutcome, CommitReplayAction, CommitReplayRequest, CommitReplayRuntime,
    CommitTimestampSource, CommitUnresolvedDurable, CommitUnresolvedDurableGate,
    VisibleVersionPublish, VisibleVersionTracker,
};
use crate::format::WalRecord;
use crate::lifecycle::{
    maintenance_ready_for_recovery_health, LifecycleDurableTableCatalog, LifecycleError,
    LifecycleMaintenanceExecutor, LifecycleOperationKind, LifecycleRecoveryOutcome,
    LifecycleResult, LifecycleState, LifecycleStateMachine, LifecycleStats,
    LifecycleTransitionTrigger, RecoveryHealth, StorageMode, StorageOpenOutcome, StorageOpenPlan,
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
    pub(super) branch: BranchLocalState,
    pub(super) registry: CommitBranchRegistry,
    pub(super) guard_set: CommitBranchGuardSet,
    pub(super) allocator: CommitFactAllocator<S>,
    pub(super) visible: VisibleVersionTracker,
    pub(super) durable_gate: CommitUnresolvedDurableGate,
    pub(super) commit_config: crate::commit::CommitRuntimeConfig,
    pub(super) table_catalog: LifecycleDurableTableCatalog,
    pub(super) recovered_checkpoint_timestamp_max: Option<Timestamp>,
    pub(super) next_checkpoint_snapshot_id: u64,
    pub(super) current_recovery_health: RecoveryHealth,
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
            branch: self.branch,
            registry: self.registry,
            guard_set: self.guard_set,
            allocator: self.allocator,
            visible: self.visible,
            durable_gate: self.durable_gate,
            commit_config: self.commit_config,
            table_catalog: self.table_catalog,
            recovered_checkpoint_timestamp_max: recovery.checkpoint().timestamp_max(),
            next_checkpoint_snapshot_id,
            current_recovery_health: recovery.health().clone(),
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

    pub(crate) const fn bootstrap_report(&self) -> &LifecycleRecoveryBootstrapReport {
        &self.bootstrap_report
    }

    pub(crate) const fn current_recovery_health(&self) -> &RecoveryHealth {
        &self.current_recovery_health
    }

    pub(crate) const fn services(&self) -> &LifecycleDurableLocalServices<'_> {
        &self.services
    }

    pub(crate) const fn branch_state(&self) -> &BranchLocalState {
        &self.branch
    }

    #[cfg(any(test, feature = "testkit"))]
    pub(crate) fn branch_state_mut(&mut self) -> &mut BranchLocalState {
        &mut self.branch
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
        self.branch.capture_read_view().map_err(branch_error)
    }
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
        CommitDurableRuntime::new(
            &self.commit_config,
            &self.registry,
            &self.guard_set,
            &mut self.allocator,
            &mut self.branch,
            &mut self.visible,
            &mut self.services.wal,
            &self.durable_gate,
        )
        .execute(batch, generation_guard)
        .map_err(commit_error)
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
