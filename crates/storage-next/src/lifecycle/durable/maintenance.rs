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
    bind_materialization_task_for_enqueue, collect_storage_pressure, compact_durable_branch,
    compaction_request_from_maintenance_task, materialization_request_from_maintenance_task,
    materialize_durable_branch,
};
use crate::lifecycle::flush::{flush_durable_branch, flush_request_from_maintenance_task};
use crate::lifecycle::retention::{
    build_retention_proof, build_retention_proof_from_facts, prune_snapshots_with_proof,
    retention_outcome_for_delegated_families, retention_outcome_for_scope,
    retention_request_from_maintenance_task, LifecycleRetentionOutcome, LifecycleRetentionRequest,
    LifecycleRetentionScope, LifecycleRetentionStatus, LifecycleSnapshotPruningOutcome,
    LifecycleSnapshotPruningRequest, LifecycleSnapshotPruningStatus,
};
use crate::lifecycle::{
    purge_quarantine as purge_lifecycle_quarantine, purge_request_from_maintenance_task,
    quarantine_object as quarantine_lifecycle_object, quarantine_task_without_request,
    repair_quarantine as repair_lifecycle_quarantine, repair_request_from_maintenance_task,
    FlushFrozenOutcome, FlushFrozenRequest, LifecycleCodecId, LifecycleCompactionOutcome,
    LifecycleCompactionRequest, LifecycleError, LifecycleMaterializationOutcome,
    LifecycleMaterializationRequest, LifecycleOperationKind, LifecyclePurgeOutcome,
    LifecyclePurgeRequest, LifecycleQuarantineOutcome, LifecycleQuarantineRepairOutcome,
    LifecycleQuarantineRepairRequest, LifecycleQuarantineRequest, LifecycleResult, LifecycleStats,
    LifecycleStoragePressure, MaintenanceEnqueueOutcome, MaintenanceExecutorStatus,
    MaintenanceOutcome, MaintenanceOutcomeStatus, MaintenanceTask, MaintenanceTaskKind,
    MaintenanceTaskRequest, MaintenanceTaskRunner, RecoveryDegradationClass, RecoveryHealth,
};
use crate::service::{QuarantineService, TableObjectReaderService, TableObjectService};
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
        reason = "durable maintenance dispatch uses this concrete retention hook"
    )]
    pub(crate) fn prove_retention(
        &mut self,
        request: &LifecycleRetentionRequest,
    ) -> LifecycleResult<LifecycleRetentionOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let health = self.current_recovery_health.clone();
        if recovery_health_prevents_listing(request, &health) {
            let proof = retention_proof_from_assembly(request, &self.services, &health);
            return retention_outcome_for_scope(request, proof, &[]);
        }
        let manifest = self
            .services
            .manifest()
            .load_current()
            .map_err(manifest_error)?;
        let snapshots = self
            .services
            .snapshot()
            .list_snapshots()
            .map_err(snapshot_error)?;
        let snapshot_count = snapshots.len();
        let proof = build_retention_proof(request, manifest.as_ref(), &health, snapshot_count);
        retention_outcome_for_scope(request, proof, &snapshots)
    }

    #[allow(
        dead_code,
        reason = "durable maintenance dispatch uses this concrete snapshot pruning hook"
    )]
    pub(crate) fn prune_snapshots(
        &mut self,
        request: &LifecycleRetentionRequest,
    ) -> LifecycleResult<LifecycleSnapshotPruningOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let health = self.current_recovery_health.clone();
        if recovery_health_prevents_listing(request, &health) {
            let proof = retention_proof_from_assembly(request, &self.services, &health);
            let pruning =
                LifecycleSnapshotPruningRequest::new(proof, request.retain_newest_snapshots())?;
            return prune_snapshots_with_proof(self.services.snapshot(), &pruning);
        }
        let manifest = self
            .services
            .manifest()
            .load_current()
            .map_err(manifest_error)?;
        let snapshot_count = self
            .services
            .snapshot()
            .list_snapshots()
            .map_err(snapshot_error)?
            .len();
        let proof = build_retention_proof(request, manifest.as_ref(), &health, snapshot_count);
        let pruning =
            LifecycleSnapshotPruningRequest::new(proof, request.retain_newest_snapshots())?;
        prune_snapshots_with_proof(self.services.snapshot(), &pruning)
    }

    #[allow(
        dead_code,
        reason = "durable maintenance dispatch uses this concrete quarantine hook"
    )]
    pub(crate) fn quarantine_object(
        &mut self,
        request: &LifecycleQuarantineRequest,
    ) -> LifecycleResult<LifecycleQuarantineOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let outcome = quarantine_lifecycle_object(self.services.quarantine(), request);
        self.record_recovery_health(outcome.recovery_health());
        Ok(outcome)
    }

    #[allow(
        dead_code,
        reason = "durable maintenance dispatch uses this concrete purge hook"
    )]
    pub(crate) fn purge_quarantine(
        &mut self,
        request: &LifecyclePurgeRequest,
    ) -> LifecycleResult<LifecyclePurgeOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let outcome = purge_lifecycle_quarantine(self.services.quarantine(), request);
        self.record_recovery_health(outcome.recovery_health());
        Ok(outcome)
    }

    #[allow(
        dead_code,
        reason = "durable maintenance dispatch uses this concrete repair hook"
    )]
    pub(crate) fn repair_quarantine(
        &mut self,
        request: &LifecycleQuarantineRepairRequest,
    ) -> LifecycleResult<LifecycleQuarantineRepairOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let outcome = repair_lifecycle_quarantine(self.services.quarantine(), request)?;
        self.record_recovery_health(outcome.recovery_health());
        Ok(outcome)
    }

    #[allow(
        dead_code,
        reason = "runtime hook is consumed by concrete maintenance modules"
    )]
    pub(crate) const fn maintenance_status(&self) -> MaintenanceExecutorStatus {
        self.maintenance.status()
    }

    #[cfg(test)]
    pub(crate) fn set_active_maintenance_for_test(&mut self, task: MaintenanceTask) {
        self.maintenance.set_active_for_test(task);
    }

    #[allow(
        dead_code,
        reason = "runtime hook is consumed by concrete maintenance modules"
    )]
    pub(crate) fn enqueue_maintenance(
        &mut self,
        request: MaintenanceTaskRequest,
    ) -> LifecycleResult<MaintenanceEnqueueOutcome> {
        let branch = &mut self.branch;
        self.maintenance
            .enqueue_with_binding(self.state, request, |request| {
                bind_materialization_task_for_enqueue(branch, request)
            })
    }

    #[allow(
        dead_code,
        reason = "runtime hook is consumed by concrete maintenance modules"
    )]
    pub(crate) fn run_next_maintenance(
        &mut self,
        runner: &mut impl MaintenanceTaskRunner,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        let outcome = self.maintenance.run_next(self.state, runner);
        self.record_optional_maintenance_health(&outcome);
        outcome
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

    #[allow(
        dead_code,
        reason = "runtime maintenance entry point is consumed by dedicated tests"
    )]
    pub(crate) fn run_next_retention_maintenance(
        &mut self,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        let state = self.state;
        let maintenance = &mut self.maintenance;
        let services = &self.services;
        let health = self.current_recovery_health.clone();
        let mut runner = DurableRetentionMaintenanceRunner { services, health };
        let outcome = maintenance.run_next_matching(state, &mut runner, |task| {
            matches!(
                task.kind(),
                MaintenanceTaskKind::SnapshotPruning | MaintenanceTaskKind::Retention
            )
        });
        self.record_optional_maintenance_health(&outcome);
        outcome
    }

    #[allow(
        dead_code,
        reason = "runtime maintenance entry point is consumed by dedicated tests"
    )]
    pub(crate) fn run_next_purge_maintenance(
        &mut self,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        let state = self.state;
        let maintenance = &mut self.maintenance;
        let quarantine = self.services.quarantine();
        let database_id = *self.services.assembly_facts().database_id();
        let codec_id = LifecycleCodecId::new(self.services.assembly_facts().codec_id())?;
        let health = self.current_recovery_health.clone();
        let default_branch_id = self.branch.branch_id();
        let mut runner = DurablePurgeMaintenanceRunner {
            quarantine,
            database_id,
            codec_id,
            health,
            default_branch_id,
        };
        let outcome = maintenance.run_next_matching(state, &mut runner, |task| {
            task.kind() == MaintenanceTaskKind::Purge
        });
        self.record_optional_maintenance_health(&outcome);
        outcome
    }

    #[allow(
        dead_code,
        reason = "runtime maintenance entry point is consumed by dedicated tests"
    )]
    pub(crate) fn run_next_quarantine_maintenance(
        &mut self,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        let state = self.state;
        let maintenance = &mut self.maintenance;
        let mut runner = DurableQuarantineMaintenanceRunner;
        let outcome = maintenance.run_next_matching(state, &mut runner, |task| {
            task.kind() == MaintenanceTaskKind::Quarantine
        });
        self.record_optional_maintenance_health(&outcome);
        outcome
    }

    #[allow(
        dead_code,
        reason = "runtime maintenance entry point is consumed by dedicated tests"
    )]
    pub(crate) fn run_next_quarantine_repair_maintenance(
        &mut self,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        let state = self.state;
        let maintenance = &mut self.maintenance;
        let quarantine = self.services.quarantine();
        let database_id = *self.services.assembly_facts().database_id();
        let codec_id = LifecycleCodecId::new(self.services.assembly_facts().codec_id())?;
        let mut runner = DurableQuarantineRepairMaintenanceRunner {
            quarantine,
            database_id,
            codec_id,
        };
        let outcome = maintenance.run_next_matching(state, &mut runner, |task| {
            task.kind() == MaintenanceTaskKind::Repair
        });
        self.record_optional_maintenance_health(&outcome);
        outcome
    }

    fn record_optional_maintenance_health(
        &mut self,
        outcome: &LifecycleResult<Option<MaintenanceOutcome>>,
    ) {
        if let Ok(Some(outcome)) = outcome {
            self.record_recovery_health(outcome.recovery_health());
        }
    }

    pub(super) fn record_recovery_health(&mut self, health: Option<&RecoveryHealth>) {
        let Some(health) = health else {
            return;
        };
        if health_rank(health) > health_rank(&self.current_recovery_health) {
            self.current_recovery_health = health.clone();
        }
    }
}

const fn health_rank(health: &RecoveryHealth) -> u8 {
    match health {
        RecoveryHealth::Healthy => 0,
        RecoveryHealth::Degraded { class, .. } => match class {
            RecoveryDegradationClass::Telemetry => 1,
            RecoveryDegradationClass::PolicyDowngrade => 2,
            RecoveryDegradationClass::DataLoss => 3,
        },
        RecoveryHealth::Failed { .. } => 4,
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

pub(super) const fn checkpoint_created_at(
    last_commit_timestamp: Option<Timestamp>,
    recovered_checkpoint_timestamp: Option<Timestamp>,
) -> Timestamp {
    match last_commit_timestamp {
        Some(timestamp) => timestamp,
        None => match recovered_checkpoint_timestamp {
            Some(timestamp) => timestamp,
            None => Timestamp::from_micros(1),
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

struct DurableRetentionMaintenanceRunner<'a, 'b> {
    services: &'a crate::lifecycle::LifecycleDurableLocalServices<'b>,
    health: crate::lifecycle::RecoveryHealth,
}

impl MaintenanceTaskRunner for DurableRetentionMaintenanceRunner<'_, '_> {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        let request = retention_request_from_maintenance_task(task)?;
        if recovery_health_prevents_listing(&request, &self.health) {
            let proof = retention_proof_from_assembly(&request, self.services, &self.health);
            return match request.scope() {
                LifecycleRetentionScope::SnapshotObjects => {
                    let pruning = LifecycleSnapshotPruningRequest::new(
                        proof,
                        request.retain_newest_snapshots(),
                    )?;
                    Ok(
                        prune_snapshots_with_proof(self.services.snapshot(), &pruning)?
                            .maintenance_outcome(),
                    )
                }
                _ => Ok(retention_outcome_for_scope(&request, proof, &[])?.maintenance_outcome()),
            };
        }
        let manifest = self
            .services
            .manifest()
            .load_current()
            .map_err(manifest_error)?;
        let snapshot_count = self
            .services
            .snapshot()
            .list_snapshots()
            .map_err(snapshot_error)?
            .len();
        let proof =
            build_retention_proof(&request, manifest.as_ref(), &self.health, snapshot_count);
        match request.scope() {
            LifecycleRetentionScope::SnapshotObjects => {
                let pruning =
                    LifecycleSnapshotPruningRequest::new(proof, request.retain_newest_snapshots())?;
                Ok(
                    prune_snapshots_with_proof(self.services.snapshot(), &pruning)?
                        .maintenance_outcome(),
                )
            }
            LifecycleRetentionScope::Global => {
                let pruning = LifecycleSnapshotPruningRequest::new(
                    proof.clone(),
                    request.retain_newest_snapshots(),
                )?;
                let snapshot_outcome =
                    prune_snapshots_with_proof(self.services.snapshot(), &pruning)?;
                let retention_outcome = retention_outcome_for_delegated_families(proof)?;
                Ok(global_retention_maintenance_outcome(
                    &snapshot_outcome,
                    &retention_outcome,
                ))
            }
            // `retention_request_from_maintenance_task` only emits
            // `SnapshotObjects` or `Global` scopes — every other scope is
            // rejected at request construction with a typed
            // `InvalidRequest`. The remaining match arms are therefore
            // unreachable through the runner, and we keep them as such
            // rather than silently routing to WAL/Quarantine delegations
            // that would not match a `TableObjects`/`WalObjects`/
            // `QuarantineObjects` request.
            _ => unreachable!(
                "retention runner reached an unsupported scope: {:?}; \
                 retention_request_from_maintenance_task should reject this kind",
                request.scope(),
            ),
        }
    }
}

struct DurablePurgeMaintenanceRunner<'a, 'b> {
    quarantine: &'a QuarantineService<'b>,
    database_id: [u8; 16],
    codec_id: LifecycleCodecId,
    health: RecoveryHealth,
    default_branch_id: strata_core_next::BranchId,
}

impl MaintenanceTaskRunner for DurablePurgeMaintenanceRunner<'_, '_> {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        let branch_id = purge_branch_id_from_task(task, self.default_branch_id)?;
        let inventory = self
            .quarantine
            .load_inventory(branch_id, self.database_id, self.codec_id.as_str())
            .map_err(durable_quarantine_service_error)?;
        let request = purge_request_from_maintenance_task(
            task,
            self.database_id,
            self.codec_id.clone(),
            self.health.clone(),
            self.default_branch_id,
            inventory.token(),
        )?;
        Ok(purge_lifecycle_quarantine(self.quarantine, &request).maintenance_outcome())
    }
}

pub(super) fn purge_branch_id_from_task(
    task: &MaintenanceTask,
    default_branch_id: strata_core_next::BranchId,
) -> LifecycleResult<strata_core_next::BranchId> {
    if task.kind() != MaintenanceTaskKind::Purge {
        return Err(LifecycleError::MaintenanceTaskFailed {
            reason: "purge request requires purge task",
        });
    }
    match task.scope() {
        crate::lifecycle::MaintenanceTaskScope::Branch(branch_id) => Ok(branch_id),
        crate::lifecycle::MaintenanceTaskScope::Quarantine => Ok(default_branch_id),
        _ => Err(LifecycleError::MaintenanceTaskFailed {
            reason: "purge task scope is invalid",
        }),
    }
}

pub(super) fn durable_quarantine_service_error(
    error: crate::service::QuarantineServiceError,
) -> LifecycleError {
    LifecycleError::lower_layer_with(
        crate::lifecycle::LifecycleLowerLayer::Service,
        "quarantine service failed",
        error,
    )
}

struct DurableQuarantineMaintenanceRunner;

impl MaintenanceTaskRunner for DurableQuarantineMaintenanceRunner {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        if task.kind() != MaintenanceTaskKind::Quarantine {
            return Err(LifecycleError::MaintenanceTaskFailed {
                reason: "quarantine runner requires quarantine task",
            });
        }
        Ok(quarantine_task_without_request())
    }
}

struct DurableQuarantineRepairMaintenanceRunner<'a, 'b> {
    quarantine: &'a QuarantineService<'b>,
    database_id: [u8; 16],
    codec_id: LifecycleCodecId,
}

impl MaintenanceTaskRunner for DurableQuarantineRepairMaintenanceRunner<'_, '_> {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        let request =
            repair_request_from_maintenance_task(task, self.database_id, self.codec_id.clone())?;
        Ok(repair_lifecycle_quarantine(self.quarantine, &request)?.maintenance_outcome())
    }
}

fn retention_proof_from_assembly(
    request: &LifecycleRetentionRequest,
    services: &crate::lifecycle::LifecycleDurableLocalServices<'_>,
    health: &crate::lifecycle::RecoveryHealth,
) -> crate::lifecycle::LifecycleRetentionProof {
    build_retention_proof_from_facts(
        request,
        services.assembly_facts().manifest_snapshot_id(),
        services.assembly_facts().manifest_snapshot_watermark(),
        services.assembly_facts().manifest_flush_watermark(),
        health,
        0,
    )
}

fn recovery_health_prevents_listing(
    request: &LifecycleRetentionRequest,
    health: &crate::lifecycle::RecoveryHealth,
) -> bool {
    match health {
        crate::lifecycle::RecoveryHealth::Healthy => false,
        crate::lifecycle::RecoveryHealth::Degraded { class, .. } => match class {
            RecoveryDegradationClass::Telemetry => !request.allow_telemetry_degraded_recovery(),
            RecoveryDegradationClass::PolicyDowngrade => {
                !request.allow_telemetry_degraded_recovery()
                    || !retention_scope_is_telemetry_only(request.scope())
            }
            RecoveryDegradationClass::DataLoss => true,
        },
        crate::lifecycle::RecoveryHealth::Failed { .. } => true,
    }
}

const fn retention_scope_is_telemetry_only(scope: LifecycleRetentionScope) -> bool {
    matches!(
        scope,
        LifecycleRetentionScope::WalObjects
            | LifecycleRetentionScope::QuarantineObjects
            | LifecycleRetentionScope::TableObjects { .. }
    )
}

fn global_retention_maintenance_outcome(
    snapshot_outcome: &LifecycleSnapshotPruningOutcome,
    retention_outcome: &LifecycleRetentionOutcome,
) -> MaintenanceOutcome {
    let status = if matches!(
        snapshot_outcome.status(),
        LifecycleSnapshotPruningStatus::DeferredIncompleteProof
            | LifecycleSnapshotPruningStatus::BlockedByRecoveryHealth
    ) || matches!(
        retention_outcome.status(),
        LifecycleRetentionStatus::DeferredIncompleteProof
            | LifecycleRetentionStatus::DeferredUnsupportedScope
            | LifecycleRetentionStatus::BlockedByRecoveryHealth
    ) {
        MaintenanceOutcomeStatus::Deferred
    } else {
        MaintenanceOutcomeStatus::Completed
    };
    let mut names = snapshot_outcome
        .deleted()
        .iter()
        .map(|snapshot| snapshot.object().to_string())
        .collect::<Vec<_>>();
    names.extend(
        snapshot_outcome
            .protected()
            .iter()
            .map(|snapshot| snapshot.object().to_string()),
    );
    names.extend(
        snapshot_outcome
            .failed()
            .iter()
            .map(|failure| failure.snapshot().object().to_string()),
    );
    names.extend(
        retention_outcome
            .decisions()
            .iter()
            .filter_map(|decision| decision.object().map(ToString::to_string)),
    );
    let recovery_health = snapshot_outcome
        .recovery_health()
        .cloned()
        .or_else(|| retention_outcome.recovery_health().cloned());
    let mut outcome = MaintenanceOutcome::new(MaintenanceTaskKind::Retention, status)
        .with_affected_object_names(names)
        .with_state_changes(snapshot_outcome.deleted().len())
        .with_stats(LifecycleStats::new(
            0,
            recovery_health
                .as_ref()
                .map_or(0, RecoveryHealth::fault_count),
            1,
            usize::from(status != MaintenanceOutcomeStatus::Completed),
            0,
        ));
    if let Some(health) = recovery_health {
        outcome = outcome.with_recovery_health(health);
    }
    if status == MaintenanceOutcomeStatus::Deferred {
        outcome = outcome.with_reason("retention proof is incomplete");
    }
    outcome
}

fn manifest_error(error: crate::service::ManifestServiceError) -> LifecycleError {
    LifecycleError::lower_layer_with(
        crate::lifecycle::LifecycleLowerLayer::Service,
        "manifest service failed",
        error,
    )
}

fn snapshot_error(error: crate::service::SnapshotServiceError) -> LifecycleError {
    LifecycleError::lower_layer_with(
        crate::lifecycle::LifecycleLowerLayer::Service,
        "snapshot service failed",
        error,
    )
}

#[cfg(test)]
mod tests {
    use super::checkpoint_created_at;
    use strata_core_next::Timestamp;

    #[test]
    fn checkpoint_timestamp_fallback_is_non_epoch_without_commits_or_manifest_timestamp() {
        assert_eq!(checkpoint_created_at(None, None), Timestamp::from_micros(1));
    }

    #[test]
    fn checkpoint_timestamp_prefers_last_commit_then_recovered_checkpoint() {
        assert_eq!(
            checkpoint_created_at(
                Some(Timestamp::from_micros(9)),
                Some(Timestamp::from_micros(7))
            ),
            Timestamp::from_micros(9)
        );
        assert_eq!(
            checkpoint_created_at(None, Some(Timestamp::from_micros(7))),
            Timestamp::from_micros(7)
        );
    }
}
