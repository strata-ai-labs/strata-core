//! Durable-local maintenance dispatch.

use super::bootstrap::LifecycleDurableLocalRuntime;
use super::require_admitted;
use crate::branch::{BranchLocalState, BranchRotationOutcome};
use crate::commit::{CommitBranchGuardSet, VisibleVersionTracker};
use crate::lifecycle::checkpoint::{
    checkpoint_durable_branch, checkpoint_request_from_maintenance_task_with_snapshot_id,
    persist_flush_watermark, truncate_wal, wal_truncation_request_from_maintenance_task,
    LifecycleCheckpointOutcome, LifecycleCheckpointRequest, LifecycleFlushWatermarkOutcome,
    LifecycleFlushWatermarkRequest, LifecycleWalTruncationOutcome, LifecycleWalTruncationRequest,
};
use crate::lifecycle::compaction::{
    collect_storage_pressure, compact_durable_branch, compaction_request_from_maintenance_task,
    materialization_request_from_maintenance_task, materialize_durable_branch,
};
use crate::lifecycle::flush::{flush_durable_branch, flush_request_from_maintenance_task};
use crate::lifecycle::{
    FlushFrozenOutcome, FlushFrozenRequest, LifecycleCompactionOutcome, LifecycleCompactionRequest,
    LifecycleError, LifecycleMaterializationOutcome, LifecycleMaterializationRequest,
    LifecycleOperationKind, LifecycleResult, LifecycleStoragePressure, MaintenanceEnqueueOutcome,
    MaintenanceExecutorStatus, MaintenanceOutcome, MaintenanceOutcomeStatus, MaintenanceTask,
    MaintenanceTaskKind, MaintenanceTaskRequest, MaintenanceTaskRunner,
};
use crate::service::{TableObjectReaderService, TableObjectService};
use strata_core_next::Timestamp;

impl<S> LifecycleDurableLocalRuntime<'_, S> {
    #[allow(
        dead_code,
        reason = "durable maintenance tests and later dispatch use explicit active rotation"
    )]
    pub(crate) fn rotate_active_for_maintenance(
        &mut self,
    ) -> LifecycleResult<BranchRotationOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        Ok(self.branch.rotate_active())
    }

    #[allow(
        dead_code,
        reason = "durable maintenance dispatch uses this concrete handler"
    )]
    pub(crate) fn flush_frozen(
        &mut self,
        request: &FlushFrozenRequest,
    ) -> LifecycleResult<FlushFrozenOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        flush_durable_branch(
            &mut self.branch,
            self.services.table_object(),
            self.services.table_reader(),
            request,
        )
    }

    #[allow(
        dead_code,
        reason = "durable maintenance dispatch uses this concrete table rewrite hook"
    )]
    pub(crate) fn compact_branch_tables(
        &mut self,
        request: &LifecycleCompactionRequest,
    ) -> LifecycleResult<LifecycleCompactionOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        compact_durable_branch(&mut self.branch, request)
    }

    #[allow(
        dead_code,
        reason = "durable maintenance dispatch uses this concrete table rewrite hook"
    )]
    pub(crate) fn materialize_inherited_layer(
        &mut self,
        request: &LifecycleMaterializationRequest,
    ) -> LifecycleResult<LifecycleMaterializationOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        materialize_durable_branch(&mut self.branch, request)
    }

    #[allow(
        dead_code,
        reason = "durable maintenance dispatch uses this concrete pressure hook"
    )]
    pub(crate) fn storage_pressure(&self) -> LifecycleStoragePressure {
        collect_storage_pressure(&self.branch, self.maintenance.status())
    }

    #[allow(
        dead_code,
        reason = "durable maintenance dispatch uses this concrete checkpoint hook"
    )]
    pub(crate) fn checkpoint(
        &mut self,
        request: &LifecycleCheckpointRequest,
    ) -> LifecycleResult<LifecycleCheckpointOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        checkpoint_durable_branch(
            &self.branch,
            &self.services,
            &self.guard_set,
            || self.visible.visible_version(),
            request,
        )
    }

    #[allow(
        dead_code,
        reason = "durable maintenance dispatch uses this concrete watermark hook"
    )]
    pub(crate) fn persist_flush_watermark(
        &mut self,
        request: LifecycleFlushWatermarkRequest,
    ) -> LifecycleResult<LifecycleFlushWatermarkOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        persist_flush_watermark(
            self.services.manifest(),
            self.visible.visible_version(),
            request,
        )
    }

    #[allow(
        dead_code,
        reason = "durable maintenance dispatch uses this concrete truncation hook"
    )]
    pub(crate) fn truncate_wal(
        &mut self,
        request: LifecycleWalTruncationRequest,
    ) -> LifecycleResult<LifecycleWalTruncationOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        truncate_wal(self.services.wal(), request)
    }

    #[allow(
        dead_code,
        reason = "runtime hook is consumed by concrete maintenance modules"
    )]
    pub(crate) const fn maintenance_status(&self) -> MaintenanceExecutorStatus {
        self.maintenance.status()
    }

    #[allow(
        dead_code,
        reason = "runtime hook is consumed by concrete maintenance modules"
    )]
    pub(crate) fn enqueue_maintenance(
        &mut self,
        request: MaintenanceTaskRequest,
    ) -> LifecycleResult<MaintenanceEnqueueOutcome> {
        self.maintenance.enqueue(self.state, request)
    }

    #[allow(
        dead_code,
        reason = "runtime hook is consumed by concrete maintenance modules"
    )]
    pub(crate) fn run_next_maintenance(
        &mut self,
        runner: &mut impl MaintenanceTaskRunner,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        self.maintenance.run_next(self.state, runner)
    }

    pub(crate) fn run_next_flush_maintenance(
        &mut self,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        let state = self.state;
        let maintenance = &mut self.maintenance;
        let branch = &mut self.branch;
        let table_object = self.services.table_object();
        let table_reader = self.services.table_reader();
        let mut runner = DurableFlushMaintenanceRunner {
            branch,
            table_object,
            table_reader,
        };
        maintenance.run_next_matching(state, &mut runner, |task| {
            task.kind() == MaintenanceTaskKind::Flush
        })
    }

    #[allow(
        dead_code,
        reason = "runtime maintenance entry point is consumed by dedicated tests"
    )]
    pub(crate) fn run_next_checkpoint_maintenance(
        &mut self,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        let state = self.state;
        let maintenance = &mut self.maintenance;
        let branch = &self.branch;
        let services = &self.services;
        let guard_set = &self.guard_set;
        let visible = &self.visible;
        let next_snapshot_id = &mut self.next_checkpoint_snapshot_id;
        let created_at = checkpoint_created_at(
            self.allocator.timestamp_guard().last_allocated(),
            self.recovered_checkpoint_timestamp_max,
        );
        let mut runner = DurableCheckpointMaintenanceRunner {
            branch,
            services,
            guard_set,
            visible,
            created_at,
            next_snapshot_id,
        };
        maintenance.run_next_matching(state, &mut runner, |task| {
            task.kind() == MaintenanceTaskKind::Checkpoint
        })
    }

    #[allow(
        dead_code,
        reason = "runtime maintenance entry point is consumed by dedicated tests"
    )]
    pub(crate) fn run_next_wal_truncation_maintenance(
        &mut self,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        let state = self.state;
        let maintenance = &mut self.maintenance;
        let manifest = self.services.manifest();
        let wal = self.services.wal();
        let mut runner = DurableWalTruncationMaintenanceRunner { manifest, wal };
        maintenance.run_next_matching(state, &mut runner, |task| {
            task.kind() == MaintenanceTaskKind::WalTruncation
        })
    }

    #[allow(
        dead_code,
        reason = "runtime maintenance entry point is consumed by dedicated tests"
    )]
    pub(crate) fn run_next_compaction_maintenance(
        &mut self,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        let state = self.state;
        let maintenance = &mut self.maintenance;
        let branch = &mut self.branch;
        let mut runner = DurableCompactionMaintenanceRunner { branch };
        maintenance.run_next_matching(state, &mut runner, |task| {
            task.kind() == MaintenanceTaskKind::Compaction
        })
    }

    #[allow(
        dead_code,
        reason = "runtime maintenance entry point is consumed by dedicated tests"
    )]
    pub(crate) fn run_next_materialization_maintenance(
        &mut self,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        let state = self.state;
        let maintenance = &mut self.maintenance;
        let branch = &mut self.branch;
        let mut runner = DurableMaterializationMaintenanceRunner { branch };
        maintenance.run_next_matching(state, &mut runner, |task| {
            task.kind() == MaintenanceTaskKind::Materialization
        })
    }
}

struct DurableFlushMaintenanceRunner<'a, 'b> {
    branch: &'a mut BranchLocalState,
    table_object: &'a TableObjectService<'b>,
    table_reader: &'a TableObjectReaderService<'b>,
}

impl MaintenanceTaskRunner for DurableFlushMaintenanceRunner<'_, '_> {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        let request = flush_request_from_maintenance_task(task)?;
        Ok(
            flush_durable_branch(self.branch, self.table_object, self.table_reader, &request)?
                .maintenance_outcome(),
        )
    }
}

struct DurableCheckpointMaintenanceRunner<'a, 'b> {
    branch: &'a BranchLocalState,
    services: &'a crate::lifecycle::LifecycleDurableLocalServices<'b>,
    guard_set: &'a CommitBranchGuardSet,
    visible: &'a VisibleVersionTracker,
    created_at: Timestamp,
    next_snapshot_id: &'a mut u64,
}

impl MaintenanceTaskRunner for DurableCheckpointMaintenanceRunner<'_, '_> {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        let request = checkpoint_request_from_maintenance_task_with_snapshot_id(
            task,
            self.branch.branch_id(),
            self.services.manifest(),
            self.created_at,
            Some(*self.next_snapshot_id),
        )?;
        let outcome = checkpoint_durable_branch(
            self.branch,
            self.services,
            self.guard_set,
            || self.visible.visible_version(),
            &request,
        )?;
        if let Some(snapshot_id) = outcome.snapshot_id() {
            *self.next_snapshot_id =
                snapshot_id
                    .checked_add(1)
                    .ok_or(LifecycleError::CheckpointPublicationFailed {
                        reason: "checkpoint snapshot id overflow",
                    })?;
        }
        Ok(outcome.maintenance_outcome())
    }
}

const fn checkpoint_created_at(
    last_commit_timestamp: Option<Timestamp>,
    recovered_checkpoint_timestamp: Option<Timestamp>,
) -> Timestamp {
    match last_commit_timestamp {
        Some(timestamp) => timestamp,
        None => match recovered_checkpoint_timestamp {
            Some(timestamp) => timestamp,
            None => Timestamp::EPOCH,
        },
    }
}

struct DurableWalTruncationMaintenanceRunner<'a, 'b> {
    manifest: &'a crate::service::DatabaseManifestService<'b>,
    wal: &'a crate::service::WalService<'b>,
}

impl MaintenanceTaskRunner for DurableWalTruncationMaintenanceRunner<'_, '_> {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        let Some(request) = wal_truncation_request_from_maintenance_task(task, self.manifest)?
        else {
            return Ok(MaintenanceOutcome::new(
                crate::lifecycle::MaintenanceTaskKind::WalTruncation,
                MaintenanceOutcomeStatus::Deferred,
            )
            .with_reason("WAL truncation has no retention proof"));
        };
        Ok(truncate_wal(self.wal, request)?.maintenance_outcome())
    }
}

struct DurableCompactionMaintenanceRunner<'a> {
    branch: &'a mut BranchLocalState,
}

impl MaintenanceTaskRunner for DurableCompactionMaintenanceRunner<'_> {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        let request = compaction_request_from_maintenance_task(task)?;
        Ok(compact_durable_branch(self.branch, &request)?.maintenance_outcome())
    }
}

struct DurableMaterializationMaintenanceRunner<'a> {
    branch: &'a mut BranchLocalState,
}

impl MaintenanceTaskRunner for DurableMaterializationMaintenanceRunner<'_> {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        let request = materialization_request_from_maintenance_task(task)?;
        Ok(materialize_durable_branch(self.branch, &request)?.maintenance_outcome())
    }
}
