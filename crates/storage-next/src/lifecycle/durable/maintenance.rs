//! Durable-local maintenance dispatch.

use super::bootstrap::LifecycleDurableLocalRuntime;
use super::{branch_error, commit_error, require_admitted};
use crate::branch::{BranchLocalState, BranchRotationOutcome};
use crate::commit::{
    CommitBranchGenerationGuard, CommitBranchGuardSet, CommitBranchRegistry, VisibleVersionTracker,
};
use crate::lifecycle::checkpoint::{
    checkpoint_durable_runtime_with_budget,
    checkpoint_request_from_maintenance_task_with_snapshot_id, persist_flush_watermark,
    truncate_wal, wal_truncation_request_from_maintenance_task, LifecycleCheckpointOutcome,
    LifecycleCheckpointRequest, LifecycleFlushWatermarkOutcome, LifecycleFlushWatermarkProof,
    LifecycleFlushWatermarkRequest, LifecycleFlushWatermarkValidationContext,
    LifecycleTableManifestFlushCoverageProof, LifecycleWalTruncationOutcome,
    LifecycleWalTruncationRequest,
};
use crate::lifecycle::compaction::{
    bind_materialization_task_for_enqueue, collect_storage_pressure,
    compaction_request_from_maintenance_task, materialization_request_from_maintenance_task,
};
use crate::lifecycle::flush::{
    flush_durable_branch_with_budget, flush_request_from_maintenance_task,
};
use crate::lifecycle::retention::{
    build_retention_proof, build_retention_proof_from_facts, prune_snapshots_with_proof,
    retention_outcome_for_delegated_families, retention_outcome_for_scope,
    retention_request_from_maintenance_task, LifecycleRetentionOutcome, LifecycleRetentionRequest,
    LifecycleRetentionScope, LifecycleRetentionStatus, LifecycleSnapshotPruningOutcome,
    LifecycleSnapshotPruningRequest, LifecycleSnapshotPruningStatus,
};
use crate::lifecycle::table_manifest::{
    publish_table_manifest_for_branch_with_budget, table_manifest_debt_outcome,
};
use crate::lifecycle::table_reachability::{
    table_object_retention_outcome, LifecycleTableObjectInventoryEntry,
    LifecycleTableObjectProofEpochs, LifecycleTableObjectRetentionRequest,
};
use crate::lifecycle::{
    compact_durable_branch_manifest_backed, materialize_durable_branch_manifest_backed,
    purge_quarantine as purge_lifecycle_quarantine, purge_request_from_maintenance_task,
    quarantine_object as quarantine_lifecycle_object, quarantine_task_without_request,
    repair_quarantine as repair_lifecycle_quarantine, repair_request_from_maintenance_task,
    require_maintenance_enqueue_budget, require_rotate_budget, telemetry_health_debt,
    FlushFrozenOutcome, FlushFrozenRequest, LifecycleCodecId, LifecycleCompactionOutcome,
    LifecycleCompactionRequest, LifecycleError, LifecycleLowerLayer,
    LifecycleMaterializationOutcome, LifecycleMaterializationRequest, LifecycleOperationKind,
    LifecyclePurgeOutcome, LifecyclePurgeRequest, LifecycleQuarantineOutcome,
    LifecycleQuarantineRepairOutcome, LifecycleQuarantineRepairRequest, LifecycleQuarantineRequest,
    LifecycleResult, LifecycleStats, LifecycleStoragePressure, MaintenanceEnqueueOutcome,
    MaintenanceExecutorStatus, MaintenanceOutcome, MaintenanceOutcomeStatus, MaintenanceTask,
    MaintenanceTaskKind, MaintenanceTaskRequest, MaintenanceTaskRunner, RecoveryDegradationClass,
    RecoveryHealth,
};
use crate::service::{
    QuarantineService, TableManifestService, TableObjectReaderService, TableObjectService,
};
use strata_core_next::Timestamp;

impl<S> LifecycleDurableLocalRuntime<'_, S> {
    #[allow(
        dead_code,
        reason = "durable maintenance tests and later dispatch use explicit active rotation"
    )]
    pub(crate) fn rotate_active_for_maintenance(
        &mut self,
    ) -> LifecycleResult<BranchRotationOutcome> {
        self.rotate_active_for_branch_for_maintenance(self.initial_branch_id)
    }

    /// Rotate any branch's active state into frozen. Used by maintenance
    /// flows that operate on non-seeded branches; the seeded entry point
    /// `rotate_active_for_maintenance` is preserved for compatibility
    /// with existing test callers.
    #[allow(
        dead_code,
        reason = "non-seeded rotation is exercised by multi-branch checkpoint tests"
    )]
    pub(crate) fn rotate_active_for_branch_for_maintenance(
        &mut self,
        branch_id: strata_core_next::BranchId,
    ) -> LifecycleResult<BranchRotationOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let generation = self
            .branch_catalog
            .registry()
            .lookup(branch_id)
            .map_err(commit_error)?
            .generation();
        require_rotate_budget(
            &self.budget,
            self.branch_catalog
                .branch_state(branch_id)
                .map_err(branch_error)?,
        )?;
        let outcome = {
            let branch = self
                .branch_catalog
                .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))?;
            branch.rotate_active()
        };
        Ok(outcome)
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
        let branch_id = request.branch_id();
        let generation = self
            .branch_catalog
            .registry()
            .lookup(branch_id)
            .map_err(commit_error)?
            .generation();
        let outcome = {
            let branch = self
                .branch_catalog
                .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))?;
            flush_durable_branch_with_budget(
                branch,
                self.services.table_object(),
                self.services.table_reader(),
                request,
                Some(&self.budget),
            )?
        };
        if publish_table_manifest_after_flush(
            self.branch_catalog
                .branch_state(branch_id)
                .map_err(branch_error)?,
            self.services.table_manifest(),
            &mut self.table_catalog,
            Some(&self.budget),
            &outcome,
        )
        .is_some()
        {
            if let Ok(health) = telemetry_health_debt("table manifest publication needs recovery") {
                self.record_recovery_health(Some(&health));
            }
        }
        Ok(outcome)
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
        let branch_id = request.branch_id();
        let generation = self
            .branch_catalog
            .registry()
            .lookup(branch_id)
            .map_err(commit_error)?
            .generation();
        let outcome = {
            let branch = self
                .branch_catalog
                .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))?;
            compact_durable_branch_manifest_backed(
                branch,
                self.services.table_object(),
                self.services.table_reader(),
                self.services.table_manifest(),
                &mut self.table_catalog,
                request,
                Some(&self.budget),
            )
        };
        outcome
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
        let branch_id = request.child_branch_id();
        let generation = self
            .branch_catalog
            .registry()
            .lookup(branch_id)
            .map_err(commit_error)?
            .generation();
        let outcome = {
            let branch = self
                .branch_catalog
                .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))?;
            materialize_durable_branch_manifest_backed(
                branch,
                self.services.table_object(),
                self.services.table_reader(),
                self.services.table_manifest(),
                &mut self.table_catalog,
                request,
                Some(&self.budget),
            )
        };
        outcome
    }

    #[allow(
        dead_code,
        reason = "durable maintenance dispatch uses this concrete pressure hook"
    )]
    pub(crate) fn storage_pressure(&self) -> LifecycleStoragePressure {
        let branch = self
            .branch_catalog
            .branch_state(self.initial_branch_id)
            .expect("seeded branch is always present in the catalog");
        collect_storage_pressure(branch, self.maintenance.status())
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
        checkpoint_durable_runtime_with_budget(
            &self.branch_catalog,
            &self.services,
            &self.guard_set,
            || self.visible.visible_version(),
            request,
            Some(&self.budget),
        )
    }

    #[allow(
        dead_code,
        reason = "durable maintenance dispatch uses this concrete watermark hook"
    )]
    pub(crate) fn persist_flush_watermark(
        &mut self,
        request: &LifecycleFlushWatermarkRequest,
    ) -> LifecycleResult<LifecycleFlushWatermarkOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        persist_flush_watermark(
            self.services.manifest(),
            self.visible.visible_version(),
            request,
            &LifecycleFlushWatermarkValidationContext::none(),
        )
    }

    #[allow(
        dead_code,
        reason = "durable maintenance dispatch uses this table-manifest-backed watermark hook"
    )]
    pub(crate) fn persist_table_manifest_flush_watermark(
        &mut self,
        candidate: strata_core_next::CommitVersion,
    ) -> LifecycleResult<LifecycleFlushWatermarkOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let branch_id = self.initial_branch_id;
        let manifest = self
            .services
            .table_manifest()
            .load_current(branch_id)
            .map_err(manifest_error)?
            .ok_or(LifecycleError::WalRetentionProofIncomplete {
                reason: "table manifest flush proof requires durable table manifest",
            })?;
        let branch = self
            .branch_catalog
            .branch_state(branch_id)
            .expect("seeded branch is always present in the catalog");
        let proof = LifecycleTableManifestFlushCoverageProof::from_branch_manifest(
            candidate,
            branch,
            &manifest,
            &self.current_recovery_health,
        )?;
        // Forward-compat guard. The current durable runtime opens exactly one
        // branch, so this check passes. If multi-branch runtimes land, the
        // proof construction above must be expanded to load every active
        // branch's manifest and per-branch state before this guard relaxes.
        let active_branches = self.branch_catalog.registry().active_branch_ids();
        if active_branches != vec![branch_id] {
            return Err(LifecycleError::WalRetentionProofIncomplete {
                reason: "table manifest flush proof requires all active branches to be loaded",
            });
        }
        let request = LifecycleFlushWatermarkRequest::new(
            candidate,
            LifecycleFlushWatermarkProof::TableManifestCovered(proof.clone()),
        )?;
        let context = LifecycleFlushWatermarkValidationContext::table_manifest(
            proof.manifest_epoch(),
            proof.recovery_health_epoch(),
        )?
        .with_required_branch_epochs([(branch_id, manifest.manifest_sequence())])?;
        persist_flush_watermark(
            self.services.manifest(),
            self.visible.visible_version(),
            &request,
            &context,
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
        if let LifecycleRetentionScope::TableObjects { branch_id } = request.scope() {
            let table_request = table_object_retention_request(&self.services, branch_id, &health)?;
            return Ok(table_object_retention_outcome(&table_request)?
                .retention()
                .clone());
        }
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
        let budget = self.budget.clone();
        let maintenance_status = self.maintenance.status();
        let state = self.state;
        let branch_id = self.initial_branch_id;
        let generation = self
            .branch_catalog
            .registry()
            .lookup(branch_id)
            .map_err(commit_error)?
            .generation();
        let outcome = {
            let maintenance = &mut self.maintenance;
            let branch = self
                .branch_catalog
                .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))?;
            maintenance.enqueue_with_binding(state, request, |request| {
                require_maintenance_enqueue_budget(&budget, maintenance_status)?;
                bind_materialization_task_for_enqueue(branch, request)
            })
        };
        outcome
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
        let branch_id = self.initial_branch_id;
        // Pre-sync shadow into catalog so the runner sees direct shadow
        // mutations (test-only) before fetching from the catalog.
        let generation = self
            .branch_catalog
            .registry()
            .lookup(branch_id)
            .map_err(commit_error)?
            .generation();
        let outcome = {
            let maintenance = &mut self.maintenance;
            let branch = self
                .branch_catalog
                .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))?;
            let table_object = self.services.table_object();
            let table_reader = self.services.table_reader();
            let table_manifest = self.services.table_manifest();
            let table_catalog = &mut self.table_catalog;
            let mut runner = DurableFlushMaintenanceRunner {
                branch,
                table_object,
                table_reader,
                table_manifest,
                table_catalog,
                budget: &self.budget,
            };
            maintenance.run_next_matching(state, &mut runner, |task| {
                task.kind() == MaintenanceTaskKind::Flush
            })
        };
        self.record_optional_maintenance_health(&outcome);
        outcome
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
        let branch_catalog = &self.branch_catalog;
        let initial_branch_id = self.initial_branch_id;
        let services = &self.services;
        let guard_set = &self.guard_set;
        let visible = &self.visible;
        let budget = &self.budget;
        let next_snapshot_id = &mut self.next_checkpoint_snapshot_id;
        let created_at = checkpoint_created_at(
            self.allocator.timestamp_guard().last_allocated(),
            self.recovered_checkpoint_timestamp_max,
        );
        let mut runner = DurableCheckpointMaintenanceRunner {
            branch_catalog,
            initial_branch_id,
            services,
            guard_set,
            visible,
            created_at,
            next_snapshot_id,
            budget,
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
    pub(crate) fn run_next_flush_watermark_maintenance(
        &mut self,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        let state = self.state;
        let maintenance = &mut self.maintenance;
        let branch = self
            .branch_catalog
            .branch_state(self.initial_branch_id)
            .expect("seeded branch is always present in the catalog");
        let registry = self.branch_catalog.registry();
        let manifest = self.services.manifest();
        let table_manifest = self.services.table_manifest();
        let visible_version = self.visible.visible_version();
        let health = &self.current_recovery_health;
        let mut runner = DurableFlushWatermarkMaintenanceRunner {
            branch,
            registry,
            manifest,
            table_manifest,
            visible_version,
            health,
        };
        maintenance.run_next_matching(state, &mut runner, |task| {
            task.kind() == MaintenanceTaskKind::FlushWatermark
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
        let branch_id = self.initial_branch_id;
        // Pre-sync shadow into catalog so the runner sees direct shadow
        // mutations (test-only) before fetching from the catalog.
        let generation = self
            .branch_catalog
            .registry()
            .lookup(branch_id)
            .map_err(commit_error)?
            .generation();
        let outcome = {
            let maintenance = &mut self.maintenance;
            let branch = self
                .branch_catalog
                .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))?;
            let table_object = self.services.table_object();
            let table_reader = self.services.table_reader();
            let table_manifest = self.services.table_manifest();
            let table_catalog = &mut self.table_catalog;
            let budget = &self.budget;
            let mut runner = DurableCompactionMaintenanceRunner {
                branch,
                table_object,
                table_reader,
                table_manifest,
                table_catalog,
                budget,
            };
            maintenance.run_next_matching(state, &mut runner, |task| {
                task.kind() == MaintenanceTaskKind::Compaction
            })
        };
        outcome
    }

    #[allow(
        dead_code,
        reason = "runtime maintenance entry point is consumed by dedicated tests"
    )]
    pub(crate) fn run_next_materialization_maintenance(
        &mut self,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        let state = self.state;
        let branch_id = self.initial_branch_id;
        // Pre-sync shadow into catalog so the runner sees direct shadow
        // mutations (test-only) before fetching from the catalog.
        let generation = self
            .branch_catalog
            .registry()
            .lookup(branch_id)
            .map_err(commit_error)?
            .generation();
        let outcome = {
            let maintenance = &mut self.maintenance;
            let branch = self
                .branch_catalog
                .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))?;
            let table_object = self.services.table_object();
            let table_reader = self.services.table_reader();
            let table_manifest = self.services.table_manifest();
            let table_catalog = &mut self.table_catalog;
            let budget = &self.budget;
            let mut runner = DurableMaterializationMaintenanceRunner {
                branch,
                table_object,
                table_reader,
                table_manifest,
                table_catalog,
                budget,
            };
            maintenance.run_next_matching(state, &mut runner, |task| {
                task.kind() == MaintenanceTaskKind::Materialization
            })
        };
        outcome
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
        let branch_id = self.initial_branch_id;
        let pending_releases = &mut self.pending_releases;
        let mut runner = DurableRetentionMaintenanceRunner {
            services,
            branch_id,
            health,
            pending_releases,
        };
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
        let default_branch_id = self.initial_branch_id;
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
    table_manifest: &'a TableManifestService<'b>,
    table_catalog: &'a mut crate::lifecycle::LifecycleDurableTableCatalog,
    budget: &'a crate::lifecycle::StorageBudgetLedger,
}

impl MaintenanceTaskRunner for DurableFlushMaintenanceRunner<'_, '_> {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        let request = flush_request_from_maintenance_task(task)?;
        let outcome = flush_durable_branch_with_budget(
            self.branch,
            self.table_object,
            self.table_reader,
            &request,
            Some(self.budget),
        )?;
        let maintenance_outcome = outcome.maintenance_outcome();
        if let Some(error) = publish_table_manifest_after_flush(
            self.branch,
            self.table_manifest,
            self.table_catalog,
            Some(self.budget),
            &outcome,
        ) {
            return Ok(table_manifest_debt_outcome(maintenance_outcome, error));
        }
        Ok(maintenance_outcome)
    }
}

fn publish_table_manifest_after_flush(
    branch: &BranchLocalState,
    service: &TableManifestService<'_>,
    catalog: &mut crate::lifecycle::LifecycleDurableTableCatalog,
    budget: Option<&crate::lifecycle::StorageBudgetLedger>,
    outcome: &FlushFrozenOutcome,
) -> Option<LifecycleError> {
    if !matches!(
        outcome.status(),
        crate::lifecycle::FlushFrozenStatus::Completed
    ) {
        return None;
    }
    let (Some(identity), Some(object_facts)) = (outcome.table_identity(), outcome.object_facts())
    else {
        return None;
    };
    if let Err(error) = catalog.record_table(identity.clone(), object_facts.clone()) {
        return Some(error);
    }
    publish_table_manifest_for_branch_with_budget(branch, service, catalog, budget)
        .map_or_else(Some, |_| None)
}

struct DurableCheckpointMaintenanceRunner<'a, 'b> {
    branch_catalog: &'a crate::lifecycle::LifecycleBranchCatalog,
    initial_branch_id: strata_core_next::BranchId,
    services: &'a crate::lifecycle::LifecycleDurableLocalServices<'b>,
    guard_set: &'a CommitBranchGuardSet,
    visible: &'a VisibleVersionTracker,
    created_at: Timestamp,
    next_snapshot_id: &'a mut u64,
    budget: &'a crate::lifecycle::StorageBudgetLedger,
}

impl MaintenanceTaskRunner for DurableCheckpointMaintenanceRunner<'_, '_> {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        // The maintenance task may not be branch-scoped (checkpoint is
        // a global operation in this catalog-aware path); fall back to
        // the seeded branch_id for request identification when the task
        // scope does not carry one.
        let request = checkpoint_request_from_maintenance_task_with_snapshot_id(
            task,
            self.initial_branch_id,
            self.services.manifest(),
            self.created_at,
            Some(*self.next_snapshot_id),
        )?;
        let outcome = checkpoint_durable_runtime_with_budget(
            self.branch_catalog,
            self.services,
            self.guard_set,
            || self.visible.visible_version(),
            &request,
            Some(self.budget),
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

struct DurableFlushWatermarkMaintenanceRunner<'a, 'b> {
    branch: &'a BranchLocalState,
    registry: &'a CommitBranchRegistry,
    manifest: &'a crate::service::DatabaseManifestService<'b>,
    table_manifest: &'a TableManifestService<'b>,
    visible_version: strata_core_next::CommitVersion,
    health: &'a RecoveryHealth,
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

impl MaintenanceTaskRunner for DurableFlushWatermarkMaintenanceRunner<'_, '_> {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        if task.kind() != MaintenanceTaskKind::FlushWatermark {
            return Err(LifecycleError::MaintenanceTaskFailed {
                reason: "maintenance task is not a flush watermark task",
            });
        }
        let Some(candidate) = task.flush_watermark_candidate() else {
            return Err(LifecycleError::MaintenanceTaskFailed {
                reason: "flush watermark task requires a candidate",
            });
        };
        let branch_id = self.branch.branch_id();
        let Some(table_manifest) = self
            .table_manifest
            .load_current(branch_id)
            .map_err(manifest_error)?
        else {
            return Ok(MaintenanceOutcome::new(
                MaintenanceTaskKind::FlushWatermark,
                MaintenanceOutcomeStatus::Deferred,
            )
            .with_reason("table manifest coverage is missing"));
        };
        let proof = match LifecycleTableManifestFlushCoverageProof::from_branch_manifest(
            candidate,
            self.branch,
            &table_manifest,
            self.health,
        ) {
            Ok(proof) => proof,
            Err(error @ LifecycleError::WalRetentionProofIncomplete { .. }) => {
                return Ok(MaintenanceOutcome::new(
                    MaintenanceTaskKind::FlushWatermark,
                    MaintenanceOutcomeStatus::Deferred,
                )
                .with_reason("table manifest coverage is incomplete")
                .with_source_error(error));
            }
            Err(error) => return Err(error),
        };
        if self.registry.active_branch_ids() != vec![branch_id] {
            return Err(LifecycleError::WalRetentionProofIncomplete {
                reason: "table manifest flush proof requires all active branches to be loaded",
            });
        }
        let request = LifecycleFlushWatermarkRequest::new(
            candidate,
            LifecycleFlushWatermarkProof::TableManifestCovered(proof.clone()),
        )?;
        let context = LifecycleFlushWatermarkValidationContext::table_manifest(
            proof.manifest_epoch(),
            proof.recovery_health_epoch(),
        )?
        .with_required_branch_epochs([(branch_id, table_manifest.manifest_sequence())])?;
        Ok(
            persist_flush_watermark(self.manifest, self.visible_version, &request, &context)?
                .maintenance_outcome(),
        )
    }
}

struct DurableCompactionMaintenanceRunner<'a, 'b> {
    branch: &'a mut BranchLocalState,
    table_object: &'a TableObjectService<'b>,
    table_reader: &'a TableObjectReaderService<'b>,
    table_manifest: &'a TableManifestService<'b>,
    table_catalog: &'a mut crate::lifecycle::LifecycleDurableTableCatalog,
    budget: &'a crate::lifecycle::StorageBudgetLedger,
}

impl MaintenanceTaskRunner for DurableCompactionMaintenanceRunner<'_, '_> {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        let request = compaction_request_from_maintenance_task(task)?;
        Ok(compact_durable_branch_manifest_backed(
            self.branch,
            self.table_object,
            self.table_reader,
            self.table_manifest,
            self.table_catalog,
            &request,
            Some(self.budget),
        )?
        .maintenance_outcome())
    }
}

struct DurableMaterializationMaintenanceRunner<'a, 'b> {
    branch: &'a mut BranchLocalState,
    table_object: &'a TableObjectService<'b>,
    table_reader: &'a TableObjectReaderService<'b>,
    table_manifest: &'a TableManifestService<'b>,
    table_catalog: &'a mut crate::lifecycle::LifecycleDurableTableCatalog,
    budget: &'a crate::lifecycle::StorageBudgetLedger,
}

impl MaintenanceTaskRunner for DurableMaterializationMaintenanceRunner<'_, '_> {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        let request = materialization_request_from_maintenance_task(task)?;
        Ok(materialize_durable_branch_manifest_backed(
            self.branch,
            self.table_object,
            self.table_reader,
            self.table_manifest,
            self.table_catalog,
            &request,
            Some(self.budget),
        )?
        .maintenance_outcome())
    }
}

struct DurableRetentionMaintenanceRunner<'a, 'b> {
    services: &'a crate::lifecycle::LifecycleDurableLocalServices<'b>,
    branch_id: strata_core_next::BranchId,
    health: crate::lifecycle::RecoveryHealth,
    pending_releases: &'a mut Vec<crate::branch::BranchReleasePlan>,
}

impl DurableRetentionMaintenanceRunner<'_, '_> {
    /// Drain pending release plans matching this retention pass's scope.
    /// `Global` scope drains all; `TableObjects { branch_id }` drains plans
    /// targeting that branch. Returns the drained release plans for
    /// downstream tagging; durable physical reclaim is manifest-driven and
    /// runs through the table-object retention handler.
    fn drain_pending_releases(
        &mut self,
        scope: LifecycleRetentionScope,
    ) -> Vec<crate::branch::BranchReleasePlan> {
        let drain_all = matches!(scope, LifecycleRetentionScope::Global);
        let scoped_branch = match scope {
            LifecycleRetentionScope::TableObjects { branch_id } => Some(branch_id),
            _ => None,
        };
        if !drain_all && scoped_branch.is_none() {
            return Vec::new();
        }
        let mut remaining = Vec::with_capacity(self.pending_releases.len());
        let mut drained = Vec::new();
        for plan in self.pending_releases.drain(..) {
            let matches_scope = drain_all
                || scoped_branch.is_some_and(|target| plan.released_branch_id() == target);
            if matches_scope {
                drained.push(plan);
            } else {
                remaining.push(plan);
            }
        }
        *self.pending_releases = remaining;
        drained
    }
}

impl MaintenanceTaskRunner for DurableRetentionMaintenanceRunner<'_, '_> {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        let request = retention_request_from_maintenance_task(task)?;
        let drained = self.drain_pending_releases(request.scope());
        if let LifecycleRetentionScope::TableObjects { branch_id } = request.scope() {
            let table_retention =
                table_object_retention_request(self.services, branch_id, &self.health)
                    .and_then(|request| table_object_retention_outcome(&request))?;
            return Ok(append_released_table_names(
                table_retention.retention().maintenance_outcome(),
                &drained,
            ));
        }
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
                _ => Ok(append_released_table_names(
                    retention_outcome_for_scope(&request, proof, &[])?.maintenance_outcome(),
                    &drained,
                )),
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
                let table_retention =
                    table_object_retention_request(self.services, self.branch_id, &self.health)
                        .and_then(|request| table_object_retention_outcome(&request))?;
                Ok(append_released_table_names(
                    global_retention_maintenance_outcome(
                        &snapshot_outcome,
                        &retention_outcome,
                        table_retention.retention(),
                    ),
                    &drained,
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
    table_retention_outcome: &LifecycleRetentionOutcome,
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
    ) || matches!(
        table_retention_outcome.status(),
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
    names.extend(
        table_retention_outcome
            .decisions()
            .iter()
            .filter_map(|decision| decision.object().map(ToString::to_string)),
    );
    let recovery_health = snapshot_outcome
        .recovery_health()
        .cloned()
        .or_else(|| retention_outcome.recovery_health().cloned())
        .or_else(|| table_retention_outcome.recovery_health().cloned());
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

fn table_object_retention_request(
    services: &crate::lifecycle::LifecycleDurableLocalServices<'_>,
    branch_id: strata_core_next::BranchId,
    health: &RecoveryHealth,
) -> LifecycleResult<LifecycleTableObjectRetentionRequest> {
    let manifests = services
        .table_manifest()
        .load_all_current()
        .map_err(manifest_error)?;
    let inventory = services
        .table_object()
        .list_inventory()
        .map_err(table_object_service_error)?
        .into_iter()
        .map(|entry| {
            LifecycleTableObjectInventoryEntry::new(entry.object().clone(), entry.byte_count())
        })
        .collect::<LifecycleResult<Vec<_>>>()?;
    // Inventory is global (`tables/` prefix); to avoid re-classifying another
    // branch's already-quarantined object as a fresh candidate, load the
    // quarantine inventory for every branch that owns a known table manifest,
    // plus the branch under retention so a branch with no manifest yet but a
    // populated quarantine is still consulted.
    //
    // BranchId is not `Ord`, so dedupe via a byte-keyed BTreeSet of the
    // 16-byte representations rather than the type itself.
    let mut seen_branches: std::collections::BTreeSet<[u8; 16]> = std::collections::BTreeSet::new();
    let mut quarantine_branches: Vec<strata_core_next::BranchId> = Vec::new();
    let record_branch = |branch: strata_core_next::BranchId,
                         seen: &mut std::collections::BTreeSet<[u8; 16]>,
                         ordered: &mut Vec<strata_core_next::BranchId>| {
        if seen.insert(*branch.as_bytes()) {
            ordered.push(branch);
        }
    };
    record_branch(branch_id, &mut seen_branches, &mut quarantine_branches);
    for manifest in &manifests {
        record_branch(
            manifest.branch_id(),
            &mut seen_branches,
            &mut quarantine_branches,
        );
    }
    let database_id = *services.assembly_facts().database_id();
    let codec_id = services.assembly_facts().codec_id();
    let mut quarantined_objects: Vec<crate::object::ObjectName> = Vec::new();
    let mut quarantine_inventory_bytes: u64 = 0;
    for branch in quarantine_branches {
        let load = services
            .quarantine()
            .load_inventory(branch, database_id, codec_id)
            .map_err(durable_quarantine_service_error)?;
        quarantine_inventory_bytes = quarantine_inventory_bytes.saturating_add(load.byte_count());
        quarantined_objects.extend(
            load.inventory()
                .entries()
                .iter()
                .map(|entry| entry.source_object().clone()),
        );
    }
    let epochs =
        table_object_proof_epochs(&manifests, &inventory, quarantine_inventory_bytes, health)?;
    LifecycleTableObjectRetentionRequest::new(
        branch_id,
        health.clone(),
        epochs,
        manifests,
        inventory,
        quarantined_objects,
    )
}

fn table_object_proof_epochs(
    manifests: &[crate::format::TableManifest],
    inventory: &[LifecycleTableObjectInventoryEntry],
    quarantine_inventory_bytes: u64,
    health: &RecoveryHealth,
) -> LifecycleResult<LifecycleTableObjectProofEpochs> {
    let manifest_epoch = manifests
        .iter()
        .map(crate::format::TableManifest::manifest_sequence)
        .max()
        .unwrap_or(1);
    // Backend listings do not expose a monotonic version, so the inventory
    // and quarantine "epochs" are SHA-256 derived content fingerprints
    // truncated to u64. Collisions are statistically negligible, so two
    // distinct inventories never alias the same epoch. The fingerprint on
    // the proof context is the authoritative freshness anchor; this field
    // gives the quarantine maintenance dispatch a cheap pre-check that
    // does not require hashing the full request.
    let table_inventory_epoch = inventory_content_epoch(inventory);
    let quarantine_inventory_epoch = quarantine_content_epoch(quarantine_inventory_bytes);
    let recovery_health_epoch = u64::try_from(health.fault_count())
        .unwrap_or(u64::MAX)
        .saturating_add(1);
    LifecycleTableObjectProofEpochs::new(
        manifest_epoch.max(1),
        table_inventory_epoch.max(1),
        quarantine_inventory_epoch.max(1),
        recovery_health_epoch.max(1),
    )
}

fn inventory_content_epoch(inventory: &[LifecycleTableObjectInventoryEntry]) -> u64 {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"inventory");
    hasher.update((inventory.len() as u64).to_be_bytes());
    let mut sorted: Vec<&LifecycleTableObjectInventoryEntry> = inventory.iter().collect();
    sorted.sort_by(|left, right| left.object().cmp(right.object()));
    for entry in sorted {
        hasher.update((entry.object().as_str().len() as u64).to_be_bytes());
        hasher.update(entry.object().as_str().as_bytes());
        hasher.update(entry.byte_count().to_be_bytes());
    }
    truncate_hash_to_u64(hasher.finalize().as_slice())
}

fn quarantine_content_epoch(byte_count: u64) -> u64 {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(b"quarantine");
    hasher.update(byte_count.to_be_bytes());
    truncate_hash_to_u64(hasher.finalize().as_slice())
}

fn truncate_hash_to_u64(digest: &[u8]) -> u64 {
    let mut buffer = [0_u8; 8];
    let take = digest.len().min(8);
    buffer[..take].copy_from_slice(&digest[..take]);
    u64::from_be_bytes(buffer)
}

/// Tag the maintenance outcome with the table identities of branch
/// releases that were drained from the pending-releases buffer during
/// this retention pass. The classification itself remains
/// manifest-driven; this is informational so callers can observe what
/// the buffer surfaced.
fn append_released_table_names(
    outcome: MaintenanceOutcome,
    drained: &[crate::branch::BranchReleasePlan],
) -> MaintenanceOutcome {
    if drained.is_empty() {
        return outcome;
    }
    let mut names: Vec<String> = outcome.affected_object_names().to_vec();
    for plan in drained {
        for table in plan.releasable_tables() {
            names.push(format!("branch-release:{}", table.as_str()));
        }
    }
    outcome.with_affected_object_names(names)
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

fn table_object_service_error(error: crate::service::TableObjectServiceError) -> LifecycleError {
    LifecycleError::lower_layer_with(
        LifecycleLowerLayer::Service,
        "table object service failed",
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
