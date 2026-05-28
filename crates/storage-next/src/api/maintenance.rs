//! API maintenance request shells.

use strata_core_next::BranchId;

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaintenanceTask {
    Checkpoint,
    Flush,
    Compact,
    Materialize,
    Retain,
    Reclaim,
    Quarantine,
    Purge,
    Repair,
    WalGrowth,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MaintenanceScope {
    Global,
    Branch(BranchId),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MaintenanceRequest {
    task: MaintenanceTask,
    scope: MaintenanceScope,
}

impl MaintenanceRequest {
    #[must_use]
    pub const fn new(task: MaintenanceTask, scope: MaintenanceScope) -> Self {
        Self { task, scope }
    }

    #[must_use]
    pub const fn task(self) -> MaintenanceTask {
        self.task
    }

    #[must_use]
    pub const fn scope(self) -> MaintenanceScope {
        self.scope
    }
}
