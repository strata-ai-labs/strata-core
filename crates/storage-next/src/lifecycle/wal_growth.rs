//! Deterministic WAL growth pressure policy.

use super::{
    telemetry_health_debt, LifecycleError, LifecycleOperationKind, LifecycleResult,
    LifecycleStateMachine, LifecycleWalGrowthPolicy, MaintenanceCheckpointOptions,
    MaintenanceEnqueueOutcome, MaintenanceEnqueueStatus, MaintenanceTaskRequest, RecoveryHealth,
};
use crate::service::WalGrowthFacts;
use strata_core_next::CommitVersion;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LifecycleWalGrowthOutcome {
    status: LifecycleWalGrowthStatus,
    facts: WalGrowthFacts,
    commits_since_checkpoint: u64,
    trigger: Option<LifecycleWalGrowthTrigger>,
    enqueue: Option<MaintenanceEnqueueOutcome>,
    recovery_health: Option<RecoveryHealth>,
    source_error: Option<LifecycleError>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum LifecycleWalGrowthStatus {
    Disabled,
    BelowThreshold,
    CheckpointEnqueued,
    CheckpointCoalesced,
    Deferred,
    NoDurableAction,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub(crate) enum LifecycleWalGrowthTrigger {
    RetainedBytes,
    RetainedSegments,
    CommitsSinceCheckpoint,
}

impl LifecycleWalGrowthOutcome {
    fn new(
        status: LifecycleWalGrowthStatus,
        facts: WalGrowthFacts,
        commits_since_checkpoint: u64,
        trigger: Option<LifecycleWalGrowthTrigger>,
    ) -> Self {
        Self {
            status,
            facts,
            commits_since_checkpoint,
            trigger,
            enqueue: None,
            recovery_health: None,
            source_error: None,
        }
    }

    pub(crate) fn cache_mode() -> Self {
        Self::new(
            LifecycleWalGrowthStatus::NoDurableAction,
            WalGrowthFacts::empty(),
            0,
            None,
        )
    }

    pub(crate) fn disabled(facts: WalGrowthFacts, commits_since_checkpoint: u64) -> Self {
        Self::new(
            LifecycleWalGrowthStatus::Disabled,
            facts,
            commits_since_checkpoint,
            None,
        )
    }

    pub(crate) fn below_threshold(facts: WalGrowthFacts, commits_since_checkpoint: u64) -> Self {
        Self::new(
            LifecycleWalGrowthStatus::BelowThreshold,
            facts,
            commits_since_checkpoint,
            None,
        )
    }

    pub(crate) fn enqueued(
        facts: WalGrowthFacts,
        commits_since_checkpoint: u64,
        trigger: LifecycleWalGrowthTrigger,
        enqueue: MaintenanceEnqueueOutcome,
    ) -> Self {
        let status = match enqueue.status() {
            MaintenanceEnqueueStatus::Enqueued => LifecycleWalGrowthStatus::CheckpointEnqueued,
            MaintenanceEnqueueStatus::Coalesced => LifecycleWalGrowthStatus::CheckpointCoalesced,
        };
        Self::new(status, facts, commits_since_checkpoint, Some(trigger)).with_enqueue(enqueue)
    }

    pub(crate) fn deferred(
        facts: WalGrowthFacts,
        commits_since_checkpoint: u64,
        trigger: Option<LifecycleWalGrowthTrigger>,
        error: LifecycleError,
    ) -> Self {
        Self::new(
            LifecycleWalGrowthStatus::Deferred,
            facts,
            commits_since_checkpoint,
            trigger,
        )
        .with_source_error(error)
    }

    pub(crate) fn deferred_with_health(
        facts: WalGrowthFacts,
        commits_since_checkpoint: u64,
        trigger: Option<LifecycleWalGrowthTrigger>,
        error: LifecycleError,
    ) -> LifecycleResult<Self> {
        Ok(Self::new(
            LifecycleWalGrowthStatus::Deferred,
            facts,
            commits_since_checkpoint,
            trigger,
        )
        .with_recovery_health(telemetry_health_debt("WAL growth checkpoint deferred")?)
        .with_source_error(error))
    }

    fn with_enqueue(mut self, enqueue: MaintenanceEnqueueOutcome) -> Self {
        self.enqueue = Some(enqueue);
        self
    }

    fn with_recovery_health(mut self, recovery_health: RecoveryHealth) -> Self {
        self.recovery_health = Some(recovery_health);
        self
    }

    fn with_source_error(mut self, source_error: LifecycleError) -> Self {
        self.source_error = Some(source_error);
        self
    }

    pub(crate) const fn status(&self) -> LifecycleWalGrowthStatus {
        self.status
    }

    pub(crate) const fn facts(&self) -> WalGrowthFacts {
        self.facts
    }

    pub(crate) const fn commits_since_checkpoint(&self) -> u64 {
        self.commits_since_checkpoint
    }

    pub(crate) const fn trigger(&self) -> Option<LifecycleWalGrowthTrigger> {
        self.trigger
    }

    pub(crate) const fn enqueue(&self) -> Option<&MaintenanceEnqueueOutcome> {
        self.enqueue.as_ref()
    }

    pub(crate) const fn recovery_health(&self) -> Option<&RecoveryHealth> {
        self.recovery_health.as_ref()
    }

    pub(crate) const fn source_error(&self) -> Option<&LifecycleError> {
        self.source_error.as_ref()
    }
}

impl LifecycleWalGrowthPolicy {
    pub(crate) fn trigger_for(
        self,
        facts: WalGrowthFacts,
        commits_since_checkpoint: u64,
    ) -> Option<LifecycleWalGrowthTrigger> {
        if !self.enabled() {
            return None;
        }
        if facts.retained_bytes() > self.max_retained_wal_bytes() {
            return Some(LifecycleWalGrowthTrigger::RetainedBytes);
        }
        if facts.retained_segments() > self.max_retained_wal_segments() {
            return Some(LifecycleWalGrowthTrigger::RetainedSegments);
        }
        if commits_since_checkpoint > self.max_commits_since_checkpoint() {
            return Some(LifecycleWalGrowthTrigger::CommitsSinceCheckpoint);
        }
        None
    }
}

pub(crate) fn commits_since_checkpoint(
    visible_version: CommitVersion,
    checkpoint_watermark: Option<CommitVersion>,
) -> u64 {
    visible_version
        .as_u64()
        .saturating_sub(checkpoint_watermark.unwrap_or(CommitVersion::ZERO).as_u64())
}

pub(crate) fn checkpoint_task_for_wal_growth() -> MaintenanceTaskRequest {
    MaintenanceTaskRequest::checkpoint_with_options(MaintenanceCheckpointOptions::new(None, false))
}

pub(crate) fn policy_admission_error(state: LifecycleStateMachine) -> Option<LifecycleError> {
    match state.admit(LifecycleOperationKind::OrdinaryMaintenance) {
        super::LifecycleOperationAdmission::Allowed { .. } => None,
        super::LifecycleOperationAdmission::Rejected { reason } => {
            Some(LifecycleError::InvalidLifecycleState { reason })
        }
    }
}
