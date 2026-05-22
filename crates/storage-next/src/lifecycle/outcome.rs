//! Lifecycle outcome facts.

use super::{
    ClosePhase, LifecycleError, LifecycleResult, MaintenanceTaskKind, RecoveryHealth, StorageMode,
};
use strata_core_next::CommitVersion;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct StorageOpenOutcome {
    mode: StorageMode,
    opened_existing: bool,
    recovered_visible_version: Option<CommitVersion>,
    recovery_health: RecoveryHealth,
    maintenance_ready: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MaintenanceOutcome {
    task_kind: MaintenanceTaskKind,
    status: MaintenanceOutcomeStatus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum MaintenanceOutcomeStatus {
    Completed,
    Deferred,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct CloseOutcome {
    phase: ClosePhase,
    status: CloseOutcomeStatus,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CloseOutcomeStatus {
    Complete,
    Timeout,
    Failed,
}

impl StorageOpenOutcome {
    pub(crate) fn new(
        mode: StorageMode,
        opened_existing: bool,
        recovered_visible_version: Option<CommitVersion>,
        recovery_health: RecoveryHealth,
        maintenance_ready: bool,
    ) -> LifecycleResult<Self> {
        if matches!(mode, StorageMode::Cache) && recovered_visible_version.is_some() {
            return Err(LifecycleError::InvalidOpenPlan {
                reason: "cache mode cannot report recovered durable visibility",
            });
        }
        Ok(Self {
            mode,
            opened_existing,
            recovered_visible_version,
            recovery_health,
            maintenance_ready,
        })
    }

    pub(crate) const fn mode(&self) -> StorageMode {
        self.mode
    }

    pub(crate) const fn opened_existing(&self) -> bool {
        self.opened_existing
    }

    pub(crate) const fn recovered_visible_version(&self) -> Option<CommitVersion> {
        self.recovered_visible_version
    }

    pub(crate) const fn recovery_health(&self) -> &RecoveryHealth {
        &self.recovery_health
    }

    pub(crate) const fn maintenance_ready(&self) -> bool {
        self.maintenance_ready
    }
}

impl MaintenanceOutcome {
    pub(crate) const fn new(
        task_kind: MaintenanceTaskKind,
        status: MaintenanceOutcomeStatus,
    ) -> Self {
        Self { task_kind, status }
    }

    pub(crate) const fn task_kind(self) -> MaintenanceTaskKind {
        self.task_kind
    }

    pub(crate) const fn status(self) -> MaintenanceOutcomeStatus {
        self.status
    }
}

impl CloseOutcome {
    pub(crate) const fn new(phase: ClosePhase, status: CloseOutcomeStatus) -> Self {
        Self { phase, status }
    }

    pub(crate) const fn phase(self) -> ClosePhase {
        self.phase
    }

    pub(crate) const fn status(self) -> CloseOutcomeStatus {
        self.status
    }
}
