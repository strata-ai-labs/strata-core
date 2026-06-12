//! API outcome summaries.

use strata_core_next::{BranchId, CommitVersion, Timestamp};

use super::{StorageMaintenanceSchedulingPolicy, StorageMode};

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageOpenDisposition {
    Created,
    OpenedExisting,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryHealthSummary {
    Healthy,
    Degraded,
    Failed,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageRuntimeState {
    Open,
    Closed,
    Failed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageOpenSummary {
    mode: StorageMode,
    disposition: StorageOpenDisposition,
    recovery_health: RecoveryHealthSummary,
    recovered_visible_version: Option<CommitVersion>,
    maintenance_ready: bool,
    maintenance_scheduling_policy: StorageMaintenanceSchedulingPolicy,
    durable_recovery_facts: bool,
    backend_capabilities_used: bool,
}

impl StorageOpenSummary {
    #[cfg(test)]
    #[must_use]
    pub(crate) const fn new(
        disposition: StorageOpenDisposition,
        recovery_health: RecoveryHealthSummary,
        recovered_visible_version: Option<CommitVersion>,
    ) -> Self {
        Self {
            mode: StorageMode::Cache,
            disposition,
            recovery_health,
            recovered_visible_version,
            maintenance_ready: true,
            maintenance_scheduling_policy: StorageMaintenanceSchedulingPolicy::Background,
            durable_recovery_facts: false,
            backend_capabilities_used: false,
        }
    }

    #[must_use]
    pub(crate) const fn with_open_facts(
        mode: StorageMode,
        disposition: StorageOpenDisposition,
        recovery_health: RecoveryHealthSummary,
        recovered_visible_version: Option<CommitVersion>,
        maintenance_ready: bool,
        maintenance_scheduling_policy: StorageMaintenanceSchedulingPolicy,
        durable_recovery_facts: bool,
        backend_capabilities_used: bool,
    ) -> Self {
        Self {
            mode,
            disposition,
            recovery_health,
            recovered_visible_version,
            maintenance_ready,
            maintenance_scheduling_policy,
            durable_recovery_facts,
            backend_capabilities_used,
        }
    }

    #[must_use]
    pub const fn mode(self) -> StorageMode {
        self.mode
    }

    #[must_use]
    pub const fn disposition(self) -> StorageOpenDisposition {
        self.disposition
    }

    #[must_use]
    pub const fn recovery_health(self) -> RecoveryHealthSummary {
        self.recovery_health
    }

    #[must_use]
    pub const fn recovered_visible_version(self) -> Option<CommitVersion> {
        self.recovered_visible_version
    }

    #[must_use]
    pub const fn maintenance_ready(self) -> bool {
        self.maintenance_ready
    }

    #[must_use]
    pub const fn maintenance_scheduling_policy(self) -> StorageMaintenanceSchedulingPolicy {
        self.maintenance_scheduling_policy
    }

    #[must_use]
    pub const fn has_durable_recovery_facts(self) -> bool {
        self.durable_recovery_facts
    }

    #[must_use]
    pub const fn backend_capabilities_used(self) -> bool {
        self.backend_capabilities_used
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageCloseSummary {
    state: StorageRuntimeState,
    idempotent: bool,
    effects: StorageCloseEffects,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct StorageCloseEffects {
    bits: u8,
}

impl StorageCloseSummary {
    #[must_use]
    pub const fn new(state: StorageRuntimeState, idempotent: bool) -> Self {
        Self {
            state,
            idempotent,
            effects: StorageCloseEffects::empty(),
        }
    }

    #[must_use]
    pub(crate) const fn with_close_facts(
        state: StorageRuntimeState,
        idempotent: bool,
        effects: StorageCloseEffects,
    ) -> Self {
        Self {
            state,
            idempotent,
            effects,
        }
    }

    #[must_use]
    pub const fn state(self) -> StorageRuntimeState {
        self.state
    }

    #[must_use]
    pub const fn idempotent(self) -> bool {
        self.idempotent
    }

    #[must_use]
    pub const fn commits_quiesced(self) -> bool {
        self.effects.commits_quiesced()
    }

    #[must_use]
    pub const fn maintenance_drained(self) -> bool {
        self.effects.maintenance_drained()
    }

    #[must_use]
    pub const fn durable_synced(self) -> bool {
        self.effects.durable_synced()
    }

    #[must_use]
    pub const fn guards_released(self) -> bool {
        self.effects.guards_released()
    }

    pub(crate) const fn effects(self) -> StorageCloseEffects {
        self.effects
    }
}

impl StorageCloseEffects {
    const COMMITS_QUIESCED: u8 = 1 << 0;
    const MAINTENANCE_DRAINED: u8 = 1 << 1;
    const DURABLE_SYNCED: u8 = 1 << 2;
    const GUARDS_RELEASED: u8 = 1 << 3;

    pub(crate) const fn empty() -> Self {
        Self { bits: 0 }
    }

    pub(crate) const fn with_commits_quiesced(mut self) -> Self {
        self.bits |= Self::COMMITS_QUIESCED;
        self
    }

    pub(crate) const fn with_maintenance_drained(mut self) -> Self {
        self.bits |= Self::MAINTENANCE_DRAINED;
        self
    }

    pub(crate) const fn with_durable_synced(mut self) -> Self {
        self.bits |= Self::DURABLE_SYNCED;
        self
    }

    pub(crate) const fn with_guards_released(mut self) -> Self {
        self.bits |= Self::GUARDS_RELEASED;
        self
    }

    const fn commits_quiesced(self) -> bool {
        self.bits & Self::COMMITS_QUIESCED != 0
    }

    const fn maintenance_drained(self) -> bool {
        self.bits & Self::MAINTENANCE_DRAINED != 0
    }

    const fn durable_synced(self) -> bool {
        self.bits & Self::DURABLE_SYNCED != 0
    }

    const fn guards_released(self) -> bool {
        self.bits & Self::GUARDS_RELEASED != 0
    }
}

#[derive(Debug)]
pub struct StorageOpenOutcome<'a> {
    runtime: crate::api::runtime::StorageRuntime<'a>,
    summary: StorageOpenSummary,
}

impl<'a> StorageOpenOutcome<'a> {
    #[must_use]
    pub(crate) const fn new(
        runtime: crate::api::runtime::StorageRuntime<'a>,
        summary: StorageOpenSummary,
    ) -> Self {
        Self { runtime, summary }
    }

    #[must_use]
    pub const fn summary(&self) -> StorageOpenSummary {
        self.summary
    }

    #[must_use]
    pub fn into_runtime(self) -> crate::api::runtime::StorageRuntime<'a> {
        self.runtime
    }

    #[must_use]
    pub fn into_parts(self) -> (crate::api::runtime::StorageRuntime<'a>, StorageOpenSummary) {
        (self.runtime, self.summary)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommitSummary {
    branch_id: BranchId,
    commit_version: CommitVersion,
    commit_timestamp: Timestamp,
    durability: crate::api::CommitDurabilitySummary,
    admission: CommitAdmissionSummary,
    put_count: usize,
    delete_count: usize,
    timeline_row_count: usize,
    visible: bool,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitAdmissionStatus {
    AcceptedClean,
    AcceptedUnderPressure,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitAdmissionPressureSeverity {
    None,
    Background,
    Urgent,
    Blocking,
}

#[non_exhaustive]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CommitAdmissionPressureReason {
    None,
    ActiveMutableBytes,
    FrozenBacklog,
    LevelZeroTableBacklog,
    NonZeroLevelTableBacklog,
    InheritedLayerBacklog,
    MaintenanceQueueBacklog,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CommitAdmissionSummary {
    status: CommitAdmissionStatus,
    pressure_severity: CommitAdmissionPressureSeverity,
    pressure_reason: CommitAdmissionPressureReason,
    inline_maintenance_driven: bool,
    cleared_prior_pressure_rejection: bool,
}

impl Default for CommitAdmissionSummary {
    fn default() -> Self {
        Self::accepted_clean(
            CommitAdmissionPressureSeverity::None,
            CommitAdmissionPressureReason::None,
            false,
        )
    }
}

impl CommitAdmissionSummary {
    #[must_use]
    pub const fn accepted_clean(
        pressure_severity: CommitAdmissionPressureSeverity,
        pressure_reason: CommitAdmissionPressureReason,
        cleared_prior_pressure_rejection: bool,
    ) -> Self {
        Self {
            status: CommitAdmissionStatus::AcceptedClean,
            pressure_severity,
            pressure_reason,
            inline_maintenance_driven: false,
            cleared_prior_pressure_rejection,
        }
    }

    #[must_use]
    pub const fn accepted_under_pressure(
        pressure_reason: CommitAdmissionPressureReason,
        cleared_prior_pressure_rejection: bool,
    ) -> Self {
        Self {
            status: CommitAdmissionStatus::AcceptedUnderPressure,
            pressure_severity: CommitAdmissionPressureSeverity::Urgent,
            pressure_reason,
            inline_maintenance_driven: false,
            cleared_prior_pressure_rejection,
        }
    }

    #[must_use]
    pub const fn with_inline_maintenance_driven(mut self, driven: bool) -> Self {
        self.inline_maintenance_driven = driven;
        self
    }

    #[must_use]
    pub const fn status(self) -> CommitAdmissionStatus {
        self.status
    }

    #[must_use]
    pub const fn pressure_severity(self) -> CommitAdmissionPressureSeverity {
        self.pressure_severity
    }

    #[must_use]
    pub const fn pressure_reason(self) -> CommitAdmissionPressureReason {
        self.pressure_reason
    }

    #[must_use]
    pub const fn inline_maintenance_driven(self) -> bool {
        self.inline_maintenance_driven
    }

    #[must_use]
    pub const fn cleared_prior_pressure_rejection(self) -> bool {
        self.cleared_prior_pressure_rejection
    }
}

impl CommitSummary {
    #[cfg(test)]
    #[must_use]
    pub(crate) const fn new(
        branch_id: BranchId,
        commit_version: CommitVersion,
        commit_timestamp: Timestamp,
    ) -> Self {
        Self {
            branch_id,
            commit_version,
            commit_timestamp,
            durability: crate::api::CommitDurabilitySummary::NotDurable,
            admission: CommitAdmissionSummary::accepted_clean(
                CommitAdmissionPressureSeverity::None,
                CommitAdmissionPressureReason::None,
                false,
            ),
            put_count: 0,
            delete_count: 0,
            timeline_row_count: 0,
            visible: true,
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "summary constructor mirrors the flat public commit outcome vocabulary"
    )]
    #[must_use]
    pub(crate) const fn with_commit_facts(
        branch_id: BranchId,
        commit_version: CommitVersion,
        commit_timestamp: Timestamp,
        durability: crate::api::CommitDurabilitySummary,
        put_count: usize,
        delete_count: usize,
        timeline_row_count: usize,
        visible: bool,
    ) -> Self {
        Self {
            branch_id,
            commit_version,
            commit_timestamp,
            durability,
            admission: CommitAdmissionSummary::accepted_clean(
                CommitAdmissionPressureSeverity::None,
                CommitAdmissionPressureReason::None,
                false,
            ),
            put_count,
            delete_count,
            timeline_row_count,
            visible,
        }
    }

    #[must_use]
    pub(crate) const fn with_admission_summary(
        mut self,
        admission: CommitAdmissionSummary,
    ) -> Self {
        self.admission = admission;
        self
    }

    #[must_use]
    pub const fn branch_id(self) -> BranchId {
        self.branch_id
    }

    #[must_use]
    pub const fn commit_version(self) -> CommitVersion {
        self.commit_version
    }

    #[must_use]
    pub const fn commit_timestamp(self) -> Timestamp {
        self.commit_timestamp
    }

    #[must_use]
    pub const fn durability(self) -> crate::api::CommitDurabilitySummary {
        self.durability
    }

    #[must_use]
    pub const fn admission(self) -> CommitAdmissionSummary {
        self.admission
    }

    #[must_use]
    pub const fn put_count(self) -> usize {
        self.put_count
    }

    #[must_use]
    pub const fn delete_count(self) -> usize {
        self.delete_count
    }

    #[must_use]
    pub const fn timeline_row_count(self) -> usize {
        self.timeline_row_count
    }

    #[must_use]
    pub const fn mutation_count(self) -> usize {
        self.put_count.saturating_add(self.delete_count)
    }

    #[must_use]
    pub const fn visible(self) -> bool {
        self.visible
    }
}
