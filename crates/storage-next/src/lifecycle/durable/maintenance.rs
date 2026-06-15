//! Durable-local maintenance dispatch.

use super::bootstrap::LifecycleDurableLocalRuntime;
use super::{branch_error, commit_error, require_admitted};
use crate::branch::state::{BranchLocalState, BranchRotationOutcome};
use crate::commit::{
    CommitBranchGenerationGuard, CommitBranchGuardSet, CommitBranchRegistry, VisibleVersionTracker,
};
use crate::format::TableManifest;
use crate::lifecycle::checkpoint::{
    checkpoint_durable_rows_with_budget, checkpoint_durable_runtime_with_budget,
    checkpoint_request_from_maintenance_task_with_snapshot_id, persist_flush_watermark,
    persist_flush_watermark_with_table_manifest_proof, truncate_wal,
    wal_truncation_request_from_maintenance_task, LifecycleCheckpointOutcome,
    LifecycleCheckpointRequest, LifecycleFlushWatermarkOutcome, LifecycleFlushWatermarkProof,
    LifecycleTableManifestFlushCoverageProof, LifecycleWalTruncationOutcome,
};
use crate::lifecycle::compaction::{
    bind_materialization_task_for_enqueue, collect_storage_pressure_with_budget,
    compact_branch_to_fixed_point_with_resource_policy, compaction_score_key_for_task,
    current_compaction_request_from_maintenance_task_with_budget,
    defer_compaction_for_resource_policy, materialization_request_from_maintenance_task,
    record_lifecycle_compaction_outcome,
    record_lifecycle_table_rewrite_post_operation_score_with_budget,
    stale_compaction_maintenance_outcome, table_rewrite_outcome_allows_chain_resubmit,
    table_rewrite_outcome_was_flush_preempted, table_rewrite_score_key_for_branch_with_budget,
    table_rewrite_score_key_for_task_with_budget,
    table_rewrite_task_request_for_branch_with_budget, LifecycleCompactionScoreKey,
    LifecycleTableRewriteScoreKey,
};
use crate::lifecycle::flush::{
    flush_branch_drain_with, flush_drain_maintenance_outcome_for_scope,
    flush_drain_request_for_branch_from_maintenance_task, flush_durable_branch_with_budget,
    install_prepared_durable_flush, install_prepared_durable_flush_drain_with,
    prepare_durable_flush_drain_with_budget, PreparedDurableFlushDrain,
};
use crate::lifecycle::maintenance::{
    schedule_post_commit_maintenance as schedule_suggested_post_commit_maintenance,
    MAINTENANCE_COVERAGE_IDLE_ROUND_LIMIT,
};
use crate::lifecycle::retention::{
    build_retention_proof, build_retention_proof_from_facts, prune_snapshots_with_proof,
    retention_outcome_for_delegated_families, retention_outcome_for_scope,
    retention_request_from_maintenance_task, LifecycleRetentionOutcome, LifecycleRetentionRequest,
    LifecycleRetentionScope, LifecycleRetentionStatus, LifecycleSnapshotPruningOutcome,
    LifecycleSnapshotPruningRequest,
};
use crate::lifecycle::table_manifest::{
    publish_table_manifest_for_branch_with_budget, table_manifest_debt_outcome,
};
use crate::lifecycle::table_reachability::{
    table_object_retention_outcome, LifecycleTableObjectInventoryEntry,
    LifecycleTableObjectProofEpochs, LifecycleTableObjectRetentionRequest,
};
use crate::lifecycle::{
    begin_durable_materialization_build, commits_since_checkpoint,
    compact_durable_branch_manifest_backed, evaluate_mutating_write_admission,
    install_prepared_durable_compaction, install_prepared_durable_materialization,
    materialize_durable_branch_manifest_backed, policy_admission_error,
    prepare_durable_compaction_publication, purge_proof_from_maintenance_task,
    purge_quarantine as purge_lifecycle_quarantine,
    quarantine_object as quarantine_lifecycle_object, quarantine_task_without_request,
    repair_branch_from_maintenance_task,
    repair_branch_quarantine as repair_branch_lifecycle_quarantine,
    repair_quarantine_family as repair_lifecycle_quarantine_family,
    require_maintenance_enqueue_budget, require_rotate_budget, telemetry_health_debt,
    wal_retention_watermark, DurableMaterializationBegin, DurableMaterializationBuild,
    FlushFrozenOutcome, FlushFrozenRequest, LifecycleCodecId, LifecycleCompactionDrainOutcome,
    LifecycleCompactionDrainRequest, LifecycleCompactionIoPolicy, LifecycleCompactionOutcome,
    LifecycleCompactionRequest, LifecycleError, LifecycleLowerLayer,
    LifecycleMaintenanceSchedulingPolicy, LifecycleMaterializationOutcome,
    LifecycleMaterializationRequest, LifecycleOperationKind, LifecyclePostCommitMaintenanceOutcome,
    LifecyclePurgeOutcome, LifecycleQuarantineOutcome, LifecycleQuarantineRepairOutcome,
    LifecycleQuarantineRequest, LifecycleResult, LifecycleStats, LifecycleStoragePressure,
    LifecycleStoragePressureSeverity, LifecycleWalGrowthOutcome, MaintenanceCheckpointOptions,
    MaintenanceEnqueueOutcome, MaintenanceExecutorStatus, MaintenanceOutcome,
    MaintenanceOutcomeStatus, MaintenanceTask, MaintenanceTaskId, MaintenanceTaskKind,
    MaintenanceTaskRequest, MaintenanceTaskRunner, MaintenanceTaskScope, PreparedDurableCompaction,
    PreparedDurableMaterialization, RecoveryDegradationClass, RecoveryHealth,
};
use crate::service::{
    QuarantineService, TableManifestService, TableObjectReaderService, TableObjectService,
    WalGrowthFacts, WalRetentionProof, WalService,
};
use strata_core_next::{BranchId, CommitVersion, Timestamp};

pub(crate) enum DurableBackgroundMaintenanceStep<'a> {
    Completed(Box<MaintenanceOutcome>),
    Build(Box<DurableBackgroundMaintenanceBuild<'a>>),
}

impl DurableBackgroundMaintenanceStep<'_> {
    pub(crate) fn completed(outcome: MaintenanceOutcome) -> Self {
        Self::Completed(Box::new(outcome))
    }
}

pub(crate) enum DurableBackgroundMaintenanceBuild<'a> {
    Flush {
        task: MaintenanceTask,
        branch_id: BranchId,
        request: crate::lifecycle::flush::FlushDrainRequest,
        branch_snapshot: BranchLocalState,
        table_object: TableObjectService<'a>,
        table_reader: TableObjectReaderService<'a>,
        budget: crate::lifecycle::StorageBudgetLedger,
        started_at: std::time::Instant,
    },
    Checkpoint {
        task: MaintenanceTask,
        request: LifecycleCheckpointRequest,
        visible_version: CommitVersion,
        branches: Vec<BranchLocalState>,
    },
    WalTruncation {
        task: MaintenanceTask,
        proof: WalRetentionProof,
        wal: WalService<'a>,
    },
    Compaction {
        task: MaintenanceTask,
        branch_id: BranchId,
        level: u8,
        request: LifecycleCompactionRequest,
        branch_snapshot: BranchLocalState,
        table_object: TableObjectService<'a>,
        table_reader: TableObjectReaderService<'a>,
        budget: crate::lifecycle::StorageBudgetLedger,
    },
    Materialization {
        task: MaintenanceTask,
        branch_id: BranchId,
        build: DurableMaterializationBuild,
        table_object: TableObjectService<'a>,
        table_reader: TableObjectReaderService<'a>,
        budget: crate::lifecycle::StorageBudgetLedger,
    },
}

pub(crate) enum DurableBackgroundMaintenanceBuilt {
    Flush {
        task: MaintenanceTask,
        branch_id: BranchId,
        prepared: PreparedDurableFlushDrain,
        elapsed: std::time::Duration,
    },
    Checkpoint {
        task: MaintenanceTask,
        request: LifecycleCheckpointRequest,
        visible_version: CommitVersion,
        rows: Vec<crate::row::StorageRow>,
    },
    WalTruncation {
        task: MaintenanceTask,
        outcome: LifecycleWalTruncationOutcome,
    },
    Compaction {
        task: MaintenanceTask,
        branch_id: BranchId,
        level: u8,
        prepared: Box<PreparedDurableCompaction>,
    },
    Materialization {
        task: MaintenanceTask,
        branch_id: BranchId,
        prepared: Box<PreparedDurableMaterialization>,
    },
}

impl DurableBackgroundMaintenanceBuild<'_> {
    pub(crate) const fn task(&self) -> MaintenanceTask {
        match self {
            Self::Flush { task, .. }
            | Self::Checkpoint { task, .. }
            | Self::WalTruncation { task, .. }
            | Self::Compaction { task, .. }
            | Self::Materialization { task, .. } => *task,
        }
    }

    pub(crate) fn build(self) -> LifecycleResult<DurableBackgroundMaintenanceBuilt> {
        match self {
            Self::Flush {
                task,
                branch_id,
                request,
                branch_snapshot,
                table_object,
                table_reader,
                budget,
                started_at,
            } => Ok(DurableBackgroundMaintenanceBuilt::Flush {
                task,
                branch_id,
                prepared: prepare_durable_flush_drain_with_budget(
                    &branch_snapshot,
                    &table_object,
                    &table_reader,
                    &request,
                    Some(&budget),
                )?,
                elapsed: started_at.elapsed(),
            }),
            Self::Checkpoint {
                task,
                request,
                visible_version,
                branches,
            } => {
                let mut rows = Vec::new();
                for branch in &branches {
                    let mut branch_rows = branch
                        .checkpoint_rows(visible_version)
                        .map_err(branch_error)?;
                    rows.append(&mut branch_rows);
                }
                Ok(DurableBackgroundMaintenanceBuilt::Checkpoint {
                    task,
                    request,
                    visible_version,
                    rows,
                })
            }
            Self::WalTruncation { task, proof, wal } => {
                let outcome = truncate_wal(&wal, proof)?;
                Ok(DurableBackgroundMaintenanceBuilt::WalTruncation { task, outcome })
            }
            Self::Compaction {
                task,
                branch_id,
                level,
                request,
                branch_snapshot,
                table_object,
                table_reader,
                budget,
            } => Ok(DurableBackgroundMaintenanceBuilt::Compaction {
                task,
                branch_id,
                level,
                prepared: Box::new(prepare_durable_compaction_publication(
                    &branch_snapshot,
                    &table_object,
                    &table_reader,
                    &request,
                    Some(&budget),
                )?),
            }),
            Self::Materialization {
                task,
                branch_id,
                build,
                table_object,
                table_reader,
                budget,
            } => Ok(DurableBackgroundMaintenanceBuilt::Materialization {
                task,
                branch_id,
                prepared: Box::new(build.build(&table_object, &table_reader, Some(&budget))?),
            }),
        }
    }
}

impl<'a, S> LifecycleDurableLocalRuntime<'a, S> {
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
        if let Ok(compaction) = &outcome {
            record_lifecycle_compaction_outcome(compaction);
        }
        outcome
    }

    #[allow(
        dead_code,
        reason = "durable maintenance dispatch uses this concrete table rewrite hook"
    )]
    pub(crate) fn compact_branch_tables_to_fixed_point(
        &mut self,
        request: &LifecycleCompactionDrainRequest,
    ) -> LifecycleResult<LifecycleCompactionDrainOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let branch_id = request.branch_id();
        let generation = self
            .branch_catalog
            .registry()
            .lookup(branch_id)
            .map_err(commit_error)?
            .generation();
        let services = &self.services;
        let table_catalog = &mut self.table_catalog;
        let budget = &self.budget;
        let outcome = {
            let branch = self
                .branch_catalog
                .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))?;
            compact_branch_to_fixed_point_with_resource_policy(
                branch,
                request,
                self.open_plan.lifecycle_config().compaction_io_policy(),
                |branch, compaction| {
                    compact_durable_branch_manifest_backed(
                        branch,
                        services.table_object(),
                        services.table_reader(),
                        services.table_manifest(),
                        table_catalog,
                        compaction,
                        Some(budget),
                    )
                },
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
        self.storage_pressure_for_branch(self.initial_branch_id)
    }

    pub(crate) fn storage_pressure_for_branch(
        &self,
        branch_id: BranchId,
    ) -> LifecycleStoragePressure {
        let branch = self
            .branch_catalog
            .branch_state(branch_id)
            .expect("pressure target branch is present in the catalog");
        collect_storage_pressure_with_budget(
            branch,
            self.maintenance.status(),
            Some(self.open_plan.lifecycle_config().storage_budget()),
        )
    }

    pub(super) fn evaluate_mutating_write_admission_for_branch(
        &mut self,
        branch_id: BranchId,
    ) -> LifecycleResult<()> {
        self.last_write_admission = None;
        let pressure = self.storage_pressure_for_branch(branch_id);
        let mut outcome = evaluate_mutating_write_admission(
            pressure,
            &mut self.pressure_rejected_commit_branches,
        )?;
        if self
            .open_plan
            .lifecycle_config()
            .maintenance_scheduling_policy()
            == LifecycleMaintenanceSchedulingPolicy::DeterministicInline
            && outcome.status()
                == crate::lifecycle::LifecycleWriteAdmissionStatus::AcceptedUnderPressure
            && outcome.pressure().severity()
                == crate::lifecycle::LifecycleStoragePressureSeverity::Urgent
            && self.run_inline_admission_maintenance(outcome.pressure())
        {
            outcome = outcome.with_inline_maintenance_driven();
        }
        self.last_write_admission = Some(outcome);
        Ok(())
    }

    #[allow(
        dead_code,
        reason = "post-commit scheduler is also exercised through commit execution"
    )]
    pub(crate) fn schedule_post_commit_maintenance(
        &mut self,
    ) -> LifecyclePostCommitMaintenanceOutcome {
        self.schedule_post_commit_maintenance_for_branch(self.initial_branch_id)
    }

    pub(crate) fn schedule_post_commit_maintenance_for_branch(
        &mut self,
        branch_id: BranchId,
    ) -> LifecyclePostCommitMaintenanceOutcome {
        let policy = self
            .open_plan
            .lifecycle_config()
            .maintenance_scheduling_policy();
        let pressure = self.storage_pressure_for_branch(branch_id);
        let outcome = schedule_suggested_post_commit_maintenance(policy, pressure, |request| {
            self.enqueue_maintenance(request)
        });
        let outcome = if policy == LifecycleMaintenanceSchedulingPolicy::DeterministicInline {
            self.run_inline_post_commit_maintenance(outcome)
        } else {
            outcome
        };
        self.schedule_maintenance_coverage_after_branch(branch_id, policy);
        outcome
    }

    #[cfg(test)]
    pub(crate) fn schedule_post_commit_maintenance_for_test(
        &mut self,
        branch_id: BranchId,
    ) -> LifecyclePostCommitMaintenanceOutcome {
        self.schedule_post_commit_maintenance_for_branch(branch_id)
    }

    pub(crate) fn schedule_background_maintenance_coverage(&mut self) -> bool {
        let outcome = self.schedule_post_commit_maintenance_for_branch(self.initial_branch_id);
        matches!(
            outcome.status(),
            crate::lifecycle::maintenance::LifecyclePostCommitMaintenanceStatus::Enqueued
        )
    }

    fn schedule_maintenance_coverage_after_branch(
        &mut self,
        source_branch_id: BranchId,
        policy: LifecycleMaintenanceSchedulingPolicy,
    ) {
        if policy == LifecycleMaintenanceSchedulingPolicy::Disabled {
            return;
        }
        if !self
            .state
            .admit(LifecycleOperationKind::OrdinaryMaintenance)
            .is_allowed()
        {
            crate::observability::perf_trace::record_lifecycle_maintenance_coverage_stop_failure();
            return;
        }
        let descriptors = self.branch_catalog.list_branches(false);
        crate::observability::perf_trace::record_lifecycle_maintenance_coverage_scan(
            descriptors.len(),
        );
        let maintenance_status = self.maintenance.status();
        let mut saw_eligible_work = false;
        for descriptor in descriptors {
            let branch_id = descriptor.branch_id();
            if branch_id == source_branch_id {
                continue;
            }
            let Ok(branch) = self.branch_catalog.branch_state(branch_id) else {
                crate::observability::perf_trace::
                    record_lifecycle_maintenance_coverage_stop_failure();
                return;
            };
            let pressure = collect_storage_pressure_with_budget(
                branch,
                maintenance_status,
                Some(self.open_plan.lifecycle_config().storage_budget()),
            );
            let Some(request) = pressure.suggested_task() else {
                continue;
            };
            if pressure.severity() != LifecycleStoragePressureSeverity::None {
                crate::observability::perf_trace::
                    record_lifecycle_maintenance_coverage_quiet_branch_pressure();
            }
            saw_eligible_work = true;
            match self.enqueue_maintenance(request) {
                Ok(enqueue) => {
                    crate::observability::perf_trace::record_lifecycle_maintenance_coverage_enqueue(
                        enqueue.was_enqueued(),
                        enqueue.was_coalesced(),
                    );
                }
                Err(LifecycleError::MaintenanceQueueFull { .. }) => {
                    crate::observability::perf_trace::
                        record_lifecycle_maintenance_coverage_stop_queue_full();
                    return;
                }
                Err(_) => {
                    crate::observability::perf_trace::
                        record_lifecycle_maintenance_coverage_stop_failure();
                    return;
                }
            }
        }
        if saw_eligible_work {
            self.maintenance_coverage_idle_rounds = 0;
        } else {
            self.record_maintenance_coverage_idle_stop();
        }
    }

    fn record_maintenance_coverage_idle_stop(&mut self) {
        let mut reached_idle_limit = false;
        if self.maintenance_coverage_idle_rounds < MAINTENANCE_COVERAGE_IDLE_ROUND_LIMIT {
            self.maintenance_coverage_idle_rounds =
                self.maintenance_coverage_idle_rounds.saturating_add(1);
            crate::observability::perf_trace::record_lifecycle_maintenance_coverage_idle_round();
            reached_idle_limit =
                self.maintenance_coverage_idle_rounds >= MAINTENANCE_COVERAGE_IDLE_ROUND_LIMIT;
        }
        if reached_idle_limit {
            crate::observability::perf_trace::record_lifecycle_maintenance_coverage_stop_idle_limit(
            );
        } else if self.maintenance_coverage_idle_rounds < MAINTENANCE_COVERAGE_IDLE_ROUND_LIMIT {
            crate::observability::perf_trace::
                record_lifecycle_maintenance_coverage_stop_no_pressure();
        }
    }

    fn run_inline_post_commit_maintenance(
        &mut self,
        outcome: LifecyclePostCommitMaintenanceOutcome,
    ) -> LifecyclePostCommitMaintenanceOutcome {
        // Simulation-boundary deletion condition: remove this lifecycle-local
        // deterministic-inline path once lower-level lifecycle tests migrate to
        // the API Background + InlineMaintenanceExecutor path.
        let (Some(request), Some(enqueue)) = (outcome.suggested_task(), outcome.enqueue()) else {
            return outcome;
        };
        let inline_start = std::time::Instant::now();
        let result = self.run_inline_maintenance_task(request, enqueue.task_id());
        crate::observability::perf_trace::record_lifecycle_inline_maintenance(
            inline_start.elapsed(),
        );
        match result {
            Ok(()) => outcome,
            Err(error) => {
                crate::observability::perf_trace::record_lifecycle_post_commit_maintenance_deferred(
                );
                outcome.with_inline_failure(error)
            }
        }
    }

    fn run_inline_maintenance_task(
        &mut self,
        request: MaintenanceTaskRequest,
        task_id: MaintenanceTaskId,
    ) -> LifecycleResult<()> {
        let outcome = match request.kind() {
            MaintenanceTaskKind::Flush => self.run_flush_maintenance_task(task_id)?,
            MaintenanceTaskKind::Compaction => self.run_compaction_maintenance_task(task_id)?,
            MaintenanceTaskKind::Materialization => {
                self.run_materialization_maintenance_task(task_id)?
            }
            _ => {
                return Err(LifecycleError::MaintenanceTaskFailed {
                    reason: "post-commit inline scheduling does not support task kind",
                });
            }
        };
        if outcome.is_none() {
            return Err(LifecycleError::MaintenanceTaskFailed {
                reason: "post-commit inline task was not pending",
            });
        }
        Ok(())
    }

    fn run_inline_admission_maintenance(&mut self, pressure: LifecycleStoragePressure) -> bool {
        // Simulation-boundary deletion condition: remove this lifecycle-local
        // deterministic-inline path once lower-level lifecycle tests migrate to
        // the API Background + InlineMaintenanceExecutor path.
        let Some(request) = pressure.suggested_task() else {
            return false;
        };
        crate::observability::perf_trace::record_lifecycle_write_admission_inline_attempt();
        crate::observability::perf_trace::record_lifecycle_write_admission_urgent_inline_attempt();
        let Ok(enqueue) = self.enqueue_maintenance(request) else {
            return false;
        };
        let inline_start = std::time::Instant::now();
        let result = self.run_inline_maintenance_task(request, enqueue.task_id());
        crate::observability::perf_trace::record_lifecycle_inline_maintenance(
            inline_start.elapsed(),
        );
        result.is_ok()
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
        reason = "explicit maintenance callers run checkpoints without queueing"
    )]
    pub(crate) fn checkpoint_for_explicit_maintenance(
        &mut self,
        branch_id: strata_core_next::BranchId,
        truncate_wal_after_checkpoint: bool,
    ) -> LifecycleResult<LifecycleCheckpointOutcome> {
        let created_at = checkpoint_created_at(
            self.allocator.timestamp_guard().last_allocated(),
            self.recovered_checkpoint_timestamp_max,
        );
        let request = LifecycleCheckpointRequest::new(
            branch_id,
            self.next_checkpoint_snapshot_id,
            created_at,
        )?
        .with_wal_truncation_after_checkpoint(truncate_wal_after_checkpoint);
        let outcome = self.checkpoint(&request)?;
        if let Some(snapshot_id) = outcome.snapshot_id() {
            self.next_checkpoint_snapshot_id =
                snapshot_id
                    .checked_add(1)
                    .ok_or(LifecycleError::CheckpointPublicationFailed {
                        reason: "checkpoint snapshot id overflow",
                    })?;
        }
        Ok(outcome)
    }

    #[allow(
        dead_code,
        reason = "durable maintenance dispatch uses this concrete watermark hook"
    )]
    pub(crate) fn persist_flush_watermark(
        &mut self,
        candidate: strata_core_next::CommitVersion,
        proof: &LifecycleFlushWatermarkProof,
    ) -> LifecycleResult<LifecycleFlushWatermarkOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        persist_flush_watermark(
            self.services.manifest(),
            self.visible.visible_version(),
            candidate,
            proof,
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
        persist_flush_watermark_with_table_manifest_proof(
            self.services.manifest(),
            self.visible.visible_version(),
            candidate,
            &LifecycleFlushWatermarkProof::TableManifestCovered(proof.clone()),
            proof.manifest_epoch(),
            proof.recovery_health_epoch(),
            &[(branch_id, manifest.manifest_sequence())],
        )
    }

    #[allow(
        dead_code,
        reason = "durable maintenance dispatch uses this concrete truncation hook"
    )]
    pub(crate) fn truncate_wal(
        &mut self,
        proof: crate::service::WalRetentionProof,
    ) -> LifecycleResult<LifecycleWalTruncationOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        truncate_wal(self.services.wal(), proof)
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
        branch_id: strata_core_next::BranchId,
        proof: &crate::lifecycle::LifecyclePurgeProof,
    ) -> LifecycleResult<LifecyclePurgeOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let codec_id = LifecycleCodecId::new(self.services.assembly_facts().codec_id())?;
        let outcome = purge_lifecycle_quarantine(
            self.services.quarantine(),
            branch_id,
            *self.services.assembly_facts().database_id(),
            &codec_id,
            proof,
        )?;
        self.record_recovery_health(outcome.recovery_health());
        Ok(outcome)
    }

    #[allow(
        dead_code,
        reason = "durable maintenance dispatch uses this concrete repair hook"
    )]
    pub(crate) fn repair_branch_quarantine(
        &mut self,
        branch_id: strata_core_next::BranchId,
    ) -> LifecycleResult<LifecycleQuarantineRepairOutcome> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let codec_id = LifecycleCodecId::new(self.services.assembly_facts().codec_id())?;
        let outcome = repair_branch_lifecycle_quarantine(
            self.services.quarantine(),
            branch_id,
            *self.services.assembly_facts().database_id(),
            &codec_id,
        )?;
        self.record_recovery_health(outcome.recovery_health());
        Ok(outcome)
    }

    #[allow(
        dead_code,
        reason = "runtime hook is consumed by concrete maintenance modules"
    )]
    pub(crate) fn maintenance_status(&self) -> MaintenanceExecutorStatus {
        self.maintenance.status()
    }

    #[cfg(test)]
    pub(crate) fn pending_flush_watermark_candidate_for_test(&self) -> Option<CommitVersion> {
        self.maintenance.pending_flush_watermark_candidate()
    }

    #[cfg(test)]
    pub(crate) fn pending_maintenance_kinds_for_test(&self) -> Vec<MaintenanceTaskKind> {
        self.maintenance.pending_kinds()
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
        {
            let maintenance = &mut self.maintenance;
            let branch_catalog = &mut self.branch_catalog;
            maintenance.enqueue_with_binding(state, request, |request| {
                require_maintenance_enqueue_budget(&budget, maintenance_status)?;
                bind_materialization_request_in_catalog(branch_catalog, request)
            })
        }
    }

    #[allow(
        dead_code,
        reason = "pre-public-boundary policy hook is consumed by lifecycle hardening tests"
    )]
    pub(crate) fn evaluate_wal_growth_policy(
        &mut self,
    ) -> LifecycleResult<LifecycleWalGrowthOutcome> {
        let policy = self.open_plan.lifecycle_config().wal_growth_policy();
        if !policy.enabled() {
            return Ok(LifecycleWalGrowthOutcome::disabled(
                crate::service::WalGrowthFacts::empty(),
                0,
            ));
        }
        let facts = match self.services.wal().growth_facts() {
            Ok(facts) => facts,
            Err(error) => {
                return LifecycleWalGrowthOutcome::deferred_with_health(
                    WalGrowthFacts::empty(),
                    0,
                    None,
                    wal_error(error),
                );
            }
        };
        let current_manifest = match self.services.manifest().load_required() {
            Ok(manifest) => manifest,
            Err(error) => {
                let trigger = policy.trigger_for(facts, 0);
                return LifecycleWalGrowthOutcome::deferred_with_health(
                    facts,
                    0,
                    trigger,
                    manifest_error(error),
                );
            }
        };
        let checkpoint_watermark = current_manifest
            .snapshot_watermark()
            .map(CommitVersion::new);
        let retention_watermark = wal_retention_watermark(
            checkpoint_watermark,
            current_manifest.flushed_through_commit_id(),
        );
        let commits_since_checkpoint =
            commits_since_checkpoint(self.visible.visible_version(), retention_watermark);
        let Some(trigger) = policy.trigger_for(facts, commits_since_checkpoint) else {
            return Ok(LifecycleWalGrowthOutcome::below_threshold(
                facts,
                commits_since_checkpoint,
            ));
        };
        if self.guard_set.is_quiescing().map_err(commit_error)? {
            return Ok(LifecycleWalGrowthOutcome::deferred(
                facts,
                commits_since_checkpoint,
                Some(trigger),
                LifecycleError::InvalidLifecycleState {
                    reason: "checkpoint policy deferred while commit quiesce is active",
                },
            ));
        }
        if self.guard_set.active_guard_count().map_err(commit_error)? > 0 {
            return Ok(LifecycleWalGrowthOutcome::deferred(
                facts,
                commits_since_checkpoint,
                Some(trigger),
                LifecycleError::InvalidLifecycleState {
                    reason: "checkpoint policy deferred while branch commit guard is active",
                },
            ));
        }
        if let Some(error) = policy_admission_error(self.state) {
            return Ok(LifecycleWalGrowthOutcome::deferred(
                facts,
                commits_since_checkpoint,
                Some(trigger),
                error,
            ));
        }
        let enqueue = (|| {
            self.enqueue_maintenance(MaintenanceTaskRequest::flush(self.initial_branch_id))?;
            self.enqueue_maintenance(MaintenanceTaskRequest::checkpoint_with_options(
                MaintenanceCheckpointOptions::new(None, true).retention_critical(),
            ))?;
            self.enqueue_maintenance(MaintenanceTaskRequest::table_manifest_flush_watermark(
                self.visible.visible_version(),
            ))?;
            self.enqueue_maintenance(MaintenanceTaskRequest::wal_truncation())
        })();
        match enqueue {
            Ok(enqueue) => Ok(LifecycleWalGrowthOutcome::maintenance_enqueued(
                facts,
                commits_since_checkpoint,
                trigger,
                enqueue,
            )),
            Err(error) => LifecycleWalGrowthOutcome::deferred_with_health(
                facts,
                commits_since_checkpoint,
                Some(trigger),
                error,
            ),
        }
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
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let Some(task) = self
            .maintenance
            .next_matching_task(|task| task.kind() == MaintenanceTaskKind::Flush)
        else {
            return Ok(None);
        };
        self.run_flush_maintenance_task(task.id())
    }

    fn run_flush_maintenance_task(
        &mut self,
        task_id: MaintenanceTaskId,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        let state = self.state;
        if self
            .maintenance
            .next_matching_task(|task| {
                task.id() == task_id && task.kind() == MaintenanceTaskKind::Flush
            })
            .is_none()
        {
            return Ok(None);
        }
        let outcome = {
            let maintenance = &mut self.maintenance;
            let table_object = self.services.table_object();
            let table_reader = self.services.table_reader();
            let table_manifest = self.services.table_manifest();
            let table_catalog = &mut self.table_catalog;
            let mut runner = DurableFlushMaintenanceRunner {
                branch_catalog: &mut self.branch_catalog,
                table_object,
                table_reader,
                table_manifest,
                table_catalog,
                budget: &self.budget,
            };
            maintenance.run_next_matching(state, &mut runner, |task| task.id() == task_id)
        };
        self.record_optional_maintenance_health(&outcome);
        outcome
    }

    pub(crate) fn start_next_background_flush_maintenance(
        &mut self,
    ) -> LifecycleResult<Option<DurableBackgroundMaintenanceStep<'a>>> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let Some(task) = self
            .maintenance
            .next_matching_task(|task| task.kind() == MaintenanceTaskKind::Flush)
        else {
            return Ok(None);
        };
        let MaintenanceTaskScope::Branch(branch_id) = task.scope() else {
            return Ok(None);
        };
        let state = self.state;
        let Some(task) = self
            .maintenance
            .start_next_matching(state, |queued| queued.id() == task.id())?
        else {
            return Ok(None);
        };
        let request = flush_drain_request_for_branch_from_maintenance_task(&task, branch_id)?;
        let generation = match self.branch_catalog.registry().lookup(branch_id) {
            Ok(descriptor) => descriptor.generation(),
            Err(error) => {
                let outcome =
                    MaintenanceOutcome::new(task.kind(), MaintenanceOutcomeStatus::Failed)
                        .with_source_error(commit_error(error));
                let outcome = self.maintenance.finish_started(task, outcome, false)?;
                self.record_optional_maintenance_health(&Ok(Some(outcome.clone())));
                return Ok(Some(DurableBackgroundMaintenanceStep::completed(outcome)));
            }
        };
        let branch = match self
            .branch_catalog
            .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))
        {
            Ok(branch) => branch,
            Err(error) => {
                let outcome =
                    MaintenanceOutcome::new(task.kind(), MaintenanceOutcomeStatus::Failed)
                        .with_source_error(error);
                let outcome = self.maintenance.finish_started(task, outcome, false)?;
                self.record_optional_maintenance_health(&Ok(Some(outcome.clone())));
                return Ok(Some(DurableBackgroundMaintenanceStep::completed(outcome)));
            }
        };
        if branch.active_row_count() > 0 {
            if let Err(error) = require_rotate_budget(&self.budget, branch) {
                let outcome =
                    MaintenanceOutcome::new(task.kind(), MaintenanceOutcomeStatus::Deferred)
                        .with_source_error(error);
                let outcome = self.maintenance.finish_started(task, outcome, false)?;
                self.record_optional_maintenance_health(&Ok(Some(outcome.clone())));
                return Ok(Some(DurableBackgroundMaintenanceStep::completed(outcome)));
            }
            branch.rotate_active();
        }
        Ok(Some(DurableBackgroundMaintenanceStep::Build(Box::new(
            DurableBackgroundMaintenanceBuild::Flush {
                task,
                branch_id,
                request,
                branch_snapshot: branch.clone(),
                table_object: self.services.table_object().clone(),
                table_reader: self.services.table_reader().clone(),
                budget: self.budget.clone(),
                started_at: std::time::Instant::now(),
            },
        ))))
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

    pub(crate) fn start_next_background_checkpoint_maintenance(
        &mut self,
    ) -> LifecycleResult<Option<DurableBackgroundMaintenanceStep<'a>>> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let Some(task) = self
            .maintenance
            .next_matching_task(|task| task.kind() == MaintenanceTaskKind::Checkpoint)
        else {
            return Ok(None);
        };
        let state = self.state;
        let Some(task) = self
            .maintenance
            .start_next_matching(state, |queued| queued.id() == task.id())?
        else {
            return Ok(None);
        };
        let created_at = checkpoint_created_at(
            self.allocator.timestamp_guard().last_allocated(),
            self.recovered_checkpoint_timestamp_max,
        );
        let request = match checkpoint_request_from_maintenance_task_with_snapshot_id(
            &task,
            self.initial_branch_id,
            self.services.manifest(),
            created_at,
            Some(self.next_checkpoint_snapshot_id),
        ) {
            Ok(request) => request,
            Err(error) => {
                let outcome = MaintenanceOutcome::new(
                    crate::lifecycle::MaintenanceTaskKind::Checkpoint,
                    MaintenanceOutcomeStatus::Failed,
                )
                .with_source_error(error);
                let outcome = self.maintenance.finish_started(task, outcome, false)?;
                self.record_optional_maintenance_health(&Ok(Some(outcome.clone())));
                return Ok(Some(DurableBackgroundMaintenanceStep::completed(outcome)));
            }
        };
        let visible_version = self.visible.visible_version();
        let mut branches = Vec::new();
        for descriptor in self.branch_catalog.list_branches(false) {
            branches.push(
                self.branch_catalog
                    .branch_state(descriptor.branch_id())?
                    .clone(),
            );
        }
        Ok(Some(DurableBackgroundMaintenanceStep::Build(Box::new(
            DurableBackgroundMaintenanceBuild::Checkpoint {
                task,
                request,
                visible_version,
                branches,
            },
        ))))
    }

    #[allow(
        dead_code,
        reason = "runtime maintenance entry point is consumed by dedicated tests"
    )]
    pub(crate) fn run_next_wal_truncation_maintenance(
        &mut self,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        if self
            .maintenance
            .has_pending_or_active_kind(MaintenanceTaskKind::Flush)
            || self
                .maintenance
                .has_pending_or_active_kind(MaintenanceTaskKind::FlushWatermark)
        {
            return Ok(None);
        }
        let state = self.state;
        let maintenance = &mut self.maintenance;
        let manifest = self.services.manifest();
        let wal = self.services.wal();
        let mut runner = DurableWalTruncationMaintenanceRunner { manifest, wal };
        maintenance.run_next_matching(state, &mut runner, |task| {
            task.kind() == MaintenanceTaskKind::WalTruncation
        })
    }

    pub(crate) fn start_next_background_wal_truncation_maintenance(
        &mut self,
    ) -> LifecycleResult<Option<DurableBackgroundMaintenanceStep<'a>>> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        if self
            .maintenance
            .has_pending_or_active_kind(MaintenanceTaskKind::Flush)
            || self
                .maintenance
                .has_pending_or_active_kind(MaintenanceTaskKind::FlushWatermark)
        {
            return Ok(None);
        }
        let Some(task) = self
            .maintenance
            .next_matching_task(|task| task.kind() == MaintenanceTaskKind::WalTruncation)
        else {
            return Ok(None);
        };
        let state = self.state;
        let Some(task) = self
            .maintenance
            .start_next_matching(state, |queued| queued.id() == task.id())?
        else {
            return Ok(None);
        };
        let proof =
            match wal_truncation_request_from_maintenance_task(&task, self.services.manifest()) {
                Ok(Some(proof)) => proof,
                Ok(None) => {
                    let outcome = MaintenanceOutcome::new(
                        crate::lifecycle::MaintenanceTaskKind::WalTruncation,
                        MaintenanceOutcomeStatus::Deferred,
                    )
                    .with_reason("WAL truncation has no retention proof");
                    let outcome = self.maintenance.finish_started(task, outcome, false)?;
                    return Ok(Some(DurableBackgroundMaintenanceStep::completed(outcome)));
                }
                Err(error) => {
                    let outcome = MaintenanceOutcome::new(
                        crate::lifecycle::MaintenanceTaskKind::WalTruncation,
                        MaintenanceOutcomeStatus::Failed,
                    )
                    .with_source_error(error);
                    let outcome = self.maintenance.finish_started(task, outcome, false)?;
                    self.record_optional_maintenance_health(&Ok(Some(outcome.clone())));
                    return Ok(Some(DurableBackgroundMaintenanceStep::completed(outcome)));
                }
            };
        Ok(Some(DurableBackgroundMaintenanceStep::Build(Box::new(
            DurableBackgroundMaintenanceBuild::WalTruncation {
                task,
                proof,
                wal: self.services.wal().clone_for_background_retention(),
            },
        ))))
    }

    pub(crate) fn finish_background_maintenance(
        &mut self,
        built: DurableBackgroundMaintenanceBuilt,
    ) -> LifecycleResult<MaintenanceOutcome> {
        let outcome = match built {
            DurableBackgroundMaintenanceBuilt::Flush {
                task,
                branch_id,
                prepared,
                elapsed,
            } => self.finish_background_flush_task(task, branch_id, prepared, elapsed),
            DurableBackgroundMaintenanceBuilt::Checkpoint {
                task,
                request,
                visible_version,
                rows,
            } => {
                let checkpoint = checkpoint_durable_rows_with_budget(
                    &self.services,
                    &request,
                    visible_version,
                    &rows,
                    Some(&self.budget),
                );
                match checkpoint {
                    Ok(outcome) => {
                        if let Some(snapshot_id) = outcome.snapshot_id() {
                            self.next_checkpoint_snapshot_id = snapshot_id.checked_add(1).ok_or(
                                LifecycleError::CheckpointPublicationFailed {
                                    reason: "checkpoint snapshot id overflow",
                                },
                            )?;
                        }
                        self.maintenance
                            .finish_started(task, outcome.maintenance_outcome(), false)
                    }
                    Err(error) => {
                        let outcome =
                            MaintenanceOutcome::new(task.kind(), MaintenanceOutcomeStatus::Failed)
                                .with_source_error(error);
                        self.maintenance.finish_started(task, outcome, false)
                    }
                }
            }
            DurableBackgroundMaintenanceBuilt::WalTruncation { task, outcome } => self
                .maintenance
                .finish_started(task, outcome.maintenance_outcome(), false),
            DurableBackgroundMaintenanceBuilt::Compaction {
                task,
                branch_id,
                level,
                prepared,
            } => self.finish_background_compaction_task(task, branch_id, level, *prepared),
            DurableBackgroundMaintenanceBuilt::Materialization {
                task,
                branch_id,
                prepared,
            } => self.finish_background_materialization_task(task, branch_id, *prepared),
        };
        self.record_optional_maintenance_health(&outcome.clone().map(Some));
        outcome
    }

    pub(crate) fn finish_background_build_error(
        &mut self,
        task: MaintenanceTask,
        error: LifecycleError,
    ) -> LifecycleResult<MaintenanceOutcome> {
        let outcome = MaintenanceOutcome::new(task.kind(), MaintenanceOutcomeStatus::Failed)
            .with_source_error(error);
        let outcome = self.maintenance.finish_started(task, outcome, false);
        self.record_optional_maintenance_health(&outcome.clone().map(Some));
        outcome
    }

    fn finish_background_flush_task(
        &mut self,
        task: MaintenanceTask,
        branch_id: BranchId,
        prepared: PreparedDurableFlushDrain,
        _elapsed: std::time::Duration,
    ) -> LifecycleResult<MaintenanceOutcome> {
        let result: LifecycleResult<MaintenanceOutcome> = (|| {
            let generation = self
                .branch_catalog
                .registry()
                .lookup(branch_id)
                .map_err(commit_error)?
                .generation();
            let branch = self
                .branch_catalog
                .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))?;
            install_prepared_durable_flush_drain_with(branch, prepared, |branch, prepared_flush| {
                let outcome = install_prepared_durable_flush(branch, prepared_flush);
                let mut maintenance_outcome = outcome.maintenance_outcome();
                if let Some(error) = publish_table_manifest_after_flush(
                    branch,
                    self.services.table_manifest(),
                    &mut self.table_catalog,
                    Some(&self.budget),
                    &outcome,
                ) {
                    maintenance_outcome = table_manifest_debt_outcome(maintenance_outcome, error);
                }
                Ok(maintenance_outcome)
            })
        })();
        let outcome = result.unwrap_or_else(|error| {
            MaintenanceOutcome::new(task.kind(), MaintenanceOutcomeStatus::Failed)
                .with_source_error(error)
        });
        self.maintenance.finish_started(task, outcome, false)
    }

    fn finish_background_compaction_task(
        &mut self,
        task: MaintenanceTask,
        branch_id: BranchId,
        level: u8,
        prepared: PreparedDurableCompaction,
    ) -> LifecycleResult<MaintenanceOutcome> {
        let result: LifecycleResult<LifecycleCompactionOutcome> = (|| {
            let generation = self
                .branch_catalog
                .registry()
                .lookup(branch_id)
                .map_err(commit_error)?
                .generation();
            let branch = self
                .branch_catalog
                .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))?;
            install_prepared_durable_compaction(
                branch,
                self.services.table_manifest(),
                &mut self.table_catalog,
                prepared,
                Some(&self.budget),
            )
        })();
        let maintenance = match result {
            Ok(compaction) => {
                record_lifecycle_compaction_outcome(&compaction);
                let maintenance = compaction.maintenance_outcome();
                if table_rewrite_outcome_was_flush_preempted(&maintenance) {
                    self.requeue_flush_preempted_compaction(branch_id, level);
                } else if table_rewrite_outcome_allows_chain_resubmit(&maintenance) {
                    self.resubmit_table_rewrite_if_any_branch_still_unhealthy(branch_id);
                }
                maintenance
            }
            Err(error) => MaintenanceOutcome::new(task.kind(), MaintenanceOutcomeStatus::Failed)
                .with_source_error(error),
        };
        self.maintenance.finish_started(task, maintenance, false)
    }

    fn finish_background_materialization_task(
        &mut self,
        task: MaintenanceTask,
        branch_id: BranchId,
        prepared: PreparedDurableMaterialization,
    ) -> LifecycleResult<MaintenanceOutcome> {
        let result: LifecycleResult<LifecycleMaterializationOutcome> = (|| {
            let generation = self
                .branch_catalog
                .registry()
                .lookup(branch_id)
                .map_err(commit_error)?
                .generation();
            let branch = self
                .branch_catalog
                .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))?;
            install_prepared_durable_materialization(
                branch,
                self.services.table_manifest(),
                &mut self.table_catalog,
                prepared,
                Some(&self.budget),
            )
        })();
        let maintenance = match result {
            Ok(materialization) => {
                let maintenance = materialization.maintenance_outcome();
                if table_rewrite_outcome_allows_chain_resubmit(&maintenance) {
                    self.resubmit_table_rewrite_if_any_branch_still_unhealthy(branch_id);
                }
                maintenance
            }
            Err(error) => MaintenanceOutcome::new(task.kind(), MaintenanceOutcomeStatus::Failed)
                .with_source_error(error),
        };
        self.maintenance.finish_started(task, maintenance, false)
    }

    #[allow(
        dead_code,
        reason = "runtime maintenance entry point is consumed by dedicated tests"
    )]
    pub(crate) fn run_next_flush_watermark_maintenance(
        &mut self,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        if self
            .maintenance
            .has_pending_or_active_kind(MaintenanceTaskKind::Flush)
        {
            return Ok(None);
        }
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

    pub(crate) fn run_next_background_flush_watermark_maintenance(
        &mut self,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        if self
            .maintenance
            .has_pending_or_active_kind(MaintenanceTaskKind::Flush)
        {
            return Ok(None);
        }
        let Some(task) = self
            .maintenance
            .next_matching_task(|task| task.kind() == MaintenanceTaskKind::FlushWatermark)
        else {
            return Ok(None);
        };
        if let Some(outcome) = self.run_background_flush_watermark_if_checkpoint_covered(task)? {
            return Ok(Some(outcome));
        }
        if !self.flush_watermark_task_has_table_coverage(task)? {
            let Some(candidate) = self.highest_coverable_flush_watermark_candidate()? else {
                return Ok(None);
            };
            if !self
                .maintenance
                .replace_pending_flush_watermark_candidate(task.id(), candidate)?
            {
                return Ok(None);
            }
        }
        self.run_next_flush_watermark_maintenance()
    }

    fn run_background_flush_watermark_if_checkpoint_covered(
        &mut self,
        task: MaintenanceTask,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        let Some(candidate) = task.flush_watermark_candidate() else {
            return Err(LifecycleError::MaintenanceTaskFailed {
                reason: "flush watermark task requires a candidate",
            });
        };
        let current_manifest = self
            .services
            .manifest()
            .load_required()
            .map_err(manifest_error)?;
        let Some(snapshot_watermark) = current_manifest
            .snapshot_watermark()
            .map(CommitVersion::new)
        else {
            return Ok(None);
        };
        if candidate > snapshot_watermark {
            return Ok(None);
        }
        let state = self.state;
        let Some(task) = self
            .maintenance
            .start_next_matching(state, |queued| queued.id() == task.id())?
        else {
            return Ok(None);
        };
        let outcome = self.persist_flush_watermark(
            candidate,
            &LifecycleFlushWatermarkProof::CheckpointCovered { snapshot_watermark },
        );
        let maintenance = match outcome {
            Ok(outcome) => outcome.maintenance_outcome(),
            Err(error) => MaintenanceOutcome::new(task.kind(), MaintenanceOutcomeStatus::Failed)
                .with_source_error(error),
        };
        let outcome = self.maintenance.finish_started(task, maintenance, false)?;
        self.record_optional_maintenance_health(&Ok(Some(outcome.clone())));
        Ok(Some(outcome))
    }

    fn flush_watermark_task_has_table_coverage(
        &self,
        task: MaintenanceTask,
    ) -> LifecycleResult<bool> {
        let Some(candidate) = task.flush_watermark_candidate() else {
            return Err(LifecycleError::MaintenanceTaskFailed {
                reason: "flush watermark task requires a candidate",
            });
        };
        let branch_id = self.initial_branch_id;
        let Some(manifest) = self
            .services
            .table_manifest()
            .load_current(branch_id)
            .map_err(manifest_error)?
        else {
            return Ok(false);
        };
        let branch = self
            .branch_catalog
            .branch_state(branch_id)
            .expect("seeded branch is always present in the catalog");
        match self.flush_watermark_candidate_has_table_coverage(candidate, branch, &manifest) {
            Ok(()) => Ok(self.branch_catalog.registry().active_branch_ids() == vec![branch_id]),
            Err(LifecycleError::WalRetentionProofIncomplete { .. }) => Ok(false),
            Err(error) => Err(error),
        }
    }

    fn highest_coverable_flush_watermark_candidate(
        &self,
    ) -> LifecycleResult<Option<CommitVersion>> {
        let branch_id = self.initial_branch_id;
        if self.branch_catalog.registry().active_branch_ids() != vec![branch_id] {
            return Ok(None);
        }
        let Some(manifest) = self
            .services
            .table_manifest()
            .load_current(branch_id)
            .map_err(manifest_error)?
        else {
            return Ok(None);
        };
        let current_manifest = self
            .services
            .manifest()
            .load_required()
            .map_err(manifest_error)?;
        let retention_watermark = wal_retention_watermark(
            current_manifest
                .snapshot_watermark()
                .map(CommitVersion::new),
            current_manifest.flushed_through_commit_id(),
        );
        let branch = self
            .branch_catalog
            .branch_state(branch_id)
            .expect("seeded branch is always present in the catalog");
        for candidate in flush_watermark_candidates_from_manifest(
            &manifest,
            self.visible.visible_version(),
            retention_watermark.unwrap_or(CommitVersion::ZERO),
        ) {
            match self.flush_watermark_candidate_has_table_coverage(candidate, branch, &manifest) {
                Ok(()) => return Ok(Some(candidate)),
                Err(LifecycleError::WalRetentionProofIncomplete { .. }) => {}
                Err(error) => return Err(error),
            }
        }
        Ok(None)
    }

    fn flush_watermark_candidate_has_table_coverage(
        &self,
        candidate: CommitVersion,
        branch: &BranchLocalState,
        manifest: &TableManifest,
    ) -> LifecycleResult<()> {
        let proof = LifecycleTableManifestFlushCoverageProof::from_branch_manifest(
            candidate,
            branch,
            manifest,
            &self.current_recovery_health,
        )?;
        let current_manifest = self
            .services
            .manifest()
            .load_required()
            .map_err(manifest_error)?;
        proof.validate_extends_checkpoint(
            wal_retention_watermark(
                current_manifest
                    .snapshot_watermark()
                    .map(CommitVersion::new),
                current_manifest.flushed_through_commit_id(),
            )
            .unwrap_or(CommitVersion::ZERO),
        )
    }

    #[allow(
        dead_code,
        reason = "runtime maintenance entry point is consumed by dedicated tests"
    )]
    pub(crate) fn run_next_compaction_maintenance(
        &mut self,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let Some(task) = self.next_scored_compaction_task() else {
            return Ok(None);
        };
        self.run_compaction_maintenance_task(task.id())
    }

    pub(crate) fn run_next_table_rewrite_maintenance(
        &mut self,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let Some(task) = self.next_scored_table_rewrite_task() else {
            return Ok(None);
        };
        match task.kind() {
            MaintenanceTaskKind::Compaction => self.run_compaction_maintenance_task(task.id()),
            MaintenanceTaskKind::Materialization => {
                self.run_materialization_maintenance_task(task.id())
            }
            _ => Ok(None),
        }
    }

    pub(crate) fn start_next_background_table_rewrite_maintenance(
        &mut self,
    ) -> LifecycleResult<Option<DurableBackgroundMaintenanceStep<'a>>> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let Some(task) = self.next_scored_table_rewrite_task() else {
            return Ok(None);
        };
        match task.kind() {
            MaintenanceTaskKind::Compaction => self.start_background_compaction_task(task),
            MaintenanceTaskKind::Materialization => {
                self.start_background_materialization_task(task)
            }
            _ => Ok(None),
        }
    }

    fn next_scored_table_rewrite_task(&self) -> Option<MaintenanceTask> {
        self.maintenance
            .pending_tasks()
            .iter()
            .copied()
            .filter(|task| {
                matches!(
                    task.kind(),
                    MaintenanceTaskKind::Compaction | MaintenanceTaskKind::Materialization
                )
            })
            .max_by_key(|task| {
                (
                    self.table_rewrite_task_score_key(*task),
                    std::cmp::Reverse(task.sequence()),
                )
            })
    }

    fn table_rewrite_task_score_key(
        &self,
        task: MaintenanceTask,
    ) -> Option<LifecycleTableRewriteScoreKey> {
        let branch_id = branch_id_from_table_rewrite_task(task).ok()?;
        let branch = self.branch_catalog.branch_state(branch_id).ok()?;
        table_rewrite_score_key_for_task_with_budget(
            branch,
            task,
            Some(self.open_plan.lifecycle_config().storage_budget()),
        )
    }

    fn next_scored_compaction_task(&self) -> Option<MaintenanceTask> {
        self.maintenance
            .pending_tasks()
            .iter()
            .copied()
            .filter(|task| task.kind() == MaintenanceTaskKind::Compaction)
            .max_by_key(|task| {
                (
                    self.compaction_task_score_key(*task),
                    std::cmp::Reverse(task.sequence()),
                )
            })
    }

    fn compaction_task_score_key(
        &self,
        task: MaintenanceTask,
    ) -> Option<LifecycleCompactionScoreKey> {
        let branch_id = branch_id_from_table_level_task(task).ok()?;
        let branch = self.branch_catalog.branch_state(branch_id).ok()?;
        compaction_score_key_for_task(branch, task)
    }

    pub(crate) fn run_compaction_maintenance_task(
        &mut self,
        task_id: MaintenanceTaskId,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        let state = self.state;
        let Some(task) = self.maintenance.next_matching_task(|task| {
            task.id() == task_id && task.kind() == MaintenanceTaskKind::Compaction
        }) else {
            return Ok(None);
        };
        let (branch_id, level) = table_level_scope_from_task(task)?;
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
                compaction_io_policy: self.open_plan.lifecycle_config().compaction_io_policy(),
            };
            maintenance.run_next_matching(state, &mut runner, |task| task.id() == task_id)
        }?;
        if outcome
            .as_ref()
            .is_some_and(table_rewrite_outcome_was_flush_preempted)
        {
            self.requeue_flush_preempted_compaction(branch_id, level);
        } else if outcome
            .as_ref()
            .is_some_and(table_rewrite_outcome_allows_chain_resubmit)
        {
            self.resubmit_table_rewrite_if_any_branch_still_unhealthy(branch_id);
        }
        Ok(outcome)
    }

    fn start_background_compaction_task(
        &mut self,
        task: MaintenanceTask,
    ) -> LifecycleResult<Option<DurableBackgroundMaintenanceStep<'a>>> {
        let state = self.state;
        let (branch_id, level) = table_level_scope_from_task(task)?;
        let Some(task) = self
            .maintenance
            .start_next_matching(state, |queued| queued.id() == task.id())?
        else {
            return Ok(None);
        };
        let generation = match self.branch_catalog.registry().lookup(branch_id) {
            Ok(descriptor) => descriptor.generation(),
            Err(error) => {
                let outcome =
                    MaintenanceOutcome::new(task.kind(), MaintenanceOutcomeStatus::Failed)
                        .with_source_error(commit_error(error));
                let outcome = self.maintenance.finish_started(task, outcome, false)?;
                self.record_optional_maintenance_health(&Ok(Some(outcome.clone())));
                return Ok(Some(DurableBackgroundMaintenanceStep::completed(outcome)));
            }
        };
        let branch = match self
            .branch_catalog
            .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))
        {
            Ok(branch) => branch,
            Err(error) => {
                let outcome =
                    MaintenanceOutcome::new(task.kind(), MaintenanceOutcomeStatus::Failed)
                        .with_source_error(error);
                let outcome = self.maintenance.finish_started(task, outcome, false)?;
                self.record_optional_maintenance_health(&Ok(Some(outcome.clone())));
                return Ok(Some(DurableBackgroundMaintenanceStep::completed(outcome)));
            }
        };
        let budget = self.open_plan.lifecycle_config().storage_budget();
        let request = match current_compaction_request_from_maintenance_task_with_budget(
            &task,
            branch,
            Some(budget),
        ) {
            Ok(Some(request)) => request,
            Ok(None) => {
                crate::observability::perf_trace::record_lifecycle_background_candidate_stale_deferred(
                );
                let outcome = stale_compaction_maintenance_outcome();
                let outcome = self.maintenance.finish_started(task, outcome, false)?;
                self.record_optional_maintenance_health(&Ok(Some(outcome.clone())));
                return Ok(Some(DurableBackgroundMaintenanceStep::completed(outcome)));
            }
            Err(error) => {
                let outcome =
                    MaintenanceOutcome::new(task.kind(), MaintenanceOutcomeStatus::Failed)
                        .with_source_error(error);
                let outcome = self.maintenance.finish_started(task, outcome, false)?;
                self.record_optional_maintenance_health(&Ok(Some(outcome.clone())));
                return Ok(Some(DurableBackgroundMaintenanceStep::completed(outcome)));
            }
        };
        let compaction_io_policy = self.open_plan.lifecycle_config().compaction_io_policy();
        if let Some(outcome) =
            defer_compaction_for_resource_policy(branch, &request, compaction_io_policy)?
        {
            let outcome = self.maintenance.finish_started(task, outcome, false)?;
            self.record_optional_maintenance_health(&Ok(Some(outcome.clone())));
            return Ok(Some(DurableBackgroundMaintenanceStep::completed(outcome)));
        }
        Ok(Some(DurableBackgroundMaintenanceStep::Build(Box::new(
            DurableBackgroundMaintenanceBuild::Compaction {
                task,
                branch_id,
                level,
                request,
                branch_snapshot: branch.clone(),
                table_object: self.services.table_object().clone(),
                table_reader: self.services.table_reader().clone(),
                budget: self.budget.clone(),
            },
        ))))
    }

    fn requeue_flush_preempted_compaction(
        &mut self,
        branch_id: strata_core_next::BranchId,
        level: u8,
    ) {
        let _ = self.enqueue_maintenance(MaintenanceTaskRequest::flush(branch_id));
        match self.enqueue_maintenance(MaintenanceTaskRequest::compaction(branch_id, level)) {
            Ok(enqueue) => {
                crate::observability::perf_trace::record_lifecycle_compaction_resubmit(
                    enqueue.was_coalesced(),
                );
            }
            Err(_error) => {
                crate::observability::perf_trace::record_lifecycle_compaction_resubmit_deferred();
            }
        }
    }

    fn resubmit_table_rewrite_if_any_branch_still_unhealthy(&mut self, branch_id: BranchId) {
        if let Ok(branch) = self.branch_catalog.branch_state(branch_id) {
            record_lifecycle_table_rewrite_post_operation_score_with_budget(
                branch,
                Some(self.budget.budget()),
            );
        }
        let Some(request) = self.highest_scored_table_rewrite_request() else {
            return;
        };
        let request_kind = request.kind();
        match self.enqueue_maintenance(request) {
            Ok(enqueue) => {
                if request_kind == MaintenanceTaskKind::Compaction {
                    crate::observability::perf_trace::record_lifecycle_compaction_resubmit(
                        enqueue.was_coalesced(),
                    );
                }
            }
            Err(_error) => {
                if request_kind == MaintenanceTaskKind::Compaction {
                    crate::observability::perf_trace::record_lifecycle_compaction_resubmit_deferred(
                    );
                }
            }
        }
    }

    fn highest_scored_table_rewrite_request(&self) -> Option<MaintenanceTaskRequest> {
        self.branch_catalog
            .list_branches(false)
            .into_iter()
            .filter_map(|descriptor| {
                let branch = self
                    .branch_catalog
                    .branch_state(descriptor.branch_id())
                    .ok()?;
                Some((
                    table_rewrite_score_key_for_branch_with_budget(
                        branch,
                        Some(self.open_plan.lifecycle_config().storage_budget()),
                    )?,
                    std::cmp::Reverse(*descriptor.branch_id().as_bytes()),
                    table_rewrite_task_request_for_branch_with_budget(
                        branch,
                        Some(self.open_plan.lifecycle_config().storage_budget()),
                    )?,
                ))
            })
            .max_by_key(|(score, branch_tiebreaker, _)| (*score, *branch_tiebreaker))
            .map(|(_, _, request)| request)
    }

    #[allow(
        dead_code,
        reason = "runtime maintenance entry point is consumed by dedicated tests"
    )]
    pub(crate) fn run_next_materialization_maintenance(
        &mut self,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        require_admitted(self.state, LifecycleOperationKind::OrdinaryMaintenance)?;
        let Some(task) = self
            .maintenance
            .next_matching_task(|task| task.kind() == MaintenanceTaskKind::Materialization)
        else {
            return Ok(None);
        };
        self.run_materialization_maintenance_task(task.id())
    }

    pub(crate) fn run_materialization_maintenance_task(
        &mut self,
        task_id: MaintenanceTaskId,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        let state = self.state;
        let Some(task) = self.maintenance.next_matching_task(|task| {
            task.id() == task_id && task.kind() == MaintenanceTaskKind::Materialization
        }) else {
            return Ok(None);
        };
        let branch_id = branch_id_from_inherited_layer_task(task)?;
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
            maintenance.run_next_matching(state, &mut runner, |task| task.id() == task_id)
        }?;
        if outcome
            .as_ref()
            .is_some_and(table_rewrite_outcome_allows_chain_resubmit)
        {
            self.resubmit_table_rewrite_if_any_branch_still_unhealthy(branch_id);
        }
        Ok(outcome)
    }

    fn start_background_materialization_task(
        &mut self,
        task: MaintenanceTask,
    ) -> LifecycleResult<Option<DurableBackgroundMaintenanceStep<'a>>> {
        let state = self.state;
        let branch_id = branch_id_from_inherited_layer_task(task)?;
        let Some(task) = self
            .maintenance
            .start_next_matching(state, |queued| queued.id() == task.id())?
        else {
            return Ok(None);
        };
        let request = materialization_request_from_maintenance_task(&task)?;
        let generation = match self.branch_catalog.registry().lookup(branch_id) {
            Ok(descriptor) => descriptor.generation(),
            Err(error) => {
                let outcome =
                    MaintenanceOutcome::new(task.kind(), MaintenanceOutcomeStatus::Failed)
                        .with_source_error(commit_error(error));
                let outcome = self.maintenance.finish_started(task, outcome, false)?;
                self.record_optional_maintenance_health(&Ok(Some(outcome.clone())));
                return Ok(Some(DurableBackgroundMaintenanceStep::completed(outcome)));
            }
        };
        let branch = match self
            .branch_catalog
            .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))
        {
            Ok(branch) => branch,
            Err(error) => {
                let outcome =
                    MaintenanceOutcome::new(task.kind(), MaintenanceOutcomeStatus::Failed)
                        .with_source_error(error);
                let outcome = self.maintenance.finish_started(task, outcome, false)?;
                self.record_optional_maintenance_health(&Ok(Some(outcome.clone())));
                return Ok(Some(DurableBackgroundMaintenanceStep::completed(outcome)));
            }
        };
        match begin_durable_materialization_build(branch, &request)? {
            DurableMaterializationBegin::Deferred(outcome) => {
                let outcome =
                    self.maintenance
                        .finish_started(task, outcome.maintenance_outcome(), false)?;
                self.record_optional_maintenance_health(&Ok(Some(outcome.clone())));
                Ok(Some(DurableBackgroundMaintenanceStep::completed(outcome)))
            }
            DurableMaterializationBegin::Build(build) => {
                Ok(Some(DurableBackgroundMaintenanceStep::Build(Box::new(
                    DurableBackgroundMaintenanceBuild::Materialization {
                        task,
                        branch_id,
                        build: *build,
                        table_object: self.services.table_object().clone(),
                        table_reader: self.services.table_reader().clone(),
                        budget: self.budget.clone(),
                    },
                ))))
            }
        }
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
        let pending_releases_sequence = &mut self.pending_releases_sequence;
        let mut runner = DurableRetentionMaintenanceRunner {
            services,
            branch_id,
            health,
            pending_releases,
            pending_releases_sequence,
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
        let Some(task) = self
            .maintenance
            .next_matching_task(|task| task.kind() == MaintenanceTaskKind::Purge)
        else {
            return Ok(None);
        };
        self.run_purge_maintenance_task(task.id())
    }

    pub(crate) fn run_purge_maintenance_task(
        &mut self,
        task_id: MaintenanceTaskId,
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
            task.id() == task_id && task.kind() == MaintenanceTaskKind::Purge
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
        let Some(task) = self
            .maintenance
            .next_matching_task(|task| task.kind() == MaintenanceTaskKind::Quarantine)
        else {
            return Ok(None);
        };
        self.run_quarantine_maintenance_task(task.id())
    }

    pub(crate) fn run_quarantine_maintenance_task(
        &mut self,
        task_id: MaintenanceTaskId,
    ) -> LifecycleResult<Option<MaintenanceOutcome>> {
        let state = self.state;
        let maintenance = &mut self.maintenance;
        let mut runner = DurableQuarantineMaintenanceRunner;
        let outcome = maintenance.run_next_matching(state, &mut runner, |task| {
            task.id() == task_id && task.kind() == MaintenanceTaskKind::Quarantine
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
        let Some(task) = self
            .maintenance
            .next_matching_task(|task| task.kind() == MaintenanceTaskKind::Repair)
        else {
            return Ok(None);
        };
        self.run_quarantine_repair_maintenance_task(task.id())
    }

    pub(crate) fn run_quarantine_repair_maintenance_task(
        &mut self,
        task_id: MaintenanceTaskId,
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
            task.id() == task_id && task.kind() == MaintenanceTaskKind::Repair
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
    branch_catalog: &'a mut crate::lifecycle::LifecycleBranchCatalog,
    table_object: &'a TableObjectService<'b>,
    table_reader: &'a TableObjectReaderService<'b>,
    table_manifest: &'a TableManifestService<'b>,
    table_catalog: &'a mut crate::lifecycle::LifecycleDurableTableCatalog,
    budget: &'a crate::lifecycle::StorageBudgetLedger,
}

impl MaintenanceTaskRunner for DurableFlushMaintenanceRunner<'_, '_> {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        let mut outcomes = Vec::new();
        for descriptor in flush_branch_descriptors(self.branch_catalog, task)? {
            let branch_id = descriptor.branch_id();
            let request = flush_drain_request_for_branch_from_maintenance_task(task, branch_id)?;
            let branch = self.branch_catalog.branch_state_mut(
                branch_id,
                CommitBranchGenerationGuard::exact(descriptor.generation()),
            )?;
            if branch.active_row_count() > 0 {
                require_rotate_budget(self.budget, branch)?;
                branch.rotate_active();
            }
            outcomes.push(flush_branch_drain_with(
                branch,
                &request,
                |branch, request| {
                    let outcome = flush_durable_branch_with_budget(
                        branch,
                        self.table_object,
                        self.table_reader,
                        request,
                        Some(self.budget),
                    )?;
                    let maintenance_outcome = outcome.maintenance_outcome();
                    if let Some(error) = publish_table_manifest_after_flush(
                        branch,
                        self.table_manifest,
                        self.table_catalog,
                        Some(self.budget),
                        &outcome,
                    ) {
                        return Ok(table_manifest_debt_outcome(maintenance_outcome, error));
                    }
                    Ok(maintenance_outcome)
                },
            )?);
        }
        Ok(flush_drain_maintenance_outcome_for_scope(&outcomes))
    }
}

pub(super) fn publish_table_manifest_after_flush(
    branch: &BranchLocalState,
    service: &TableManifestService<'_>,
    catalog: &mut crate::lifecycle::LifecycleDurableTableCatalog,
    budget: Option<&crate::lifecycle::StorageBudgetLedger>,
    outcome: &FlushFrozenOutcome,
) -> Option<LifecycleError> {
    if !outcome.completed() {
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

fn flush_branch_descriptors(
    branch_catalog: &crate::lifecycle::LifecycleBranchCatalog,
    task: &MaintenanceTask,
) -> LifecycleResult<Vec<crate::lifecycle::LifecycleBranchDescriptor>> {
    flush_branch_descriptors_for_scope(branch_catalog, task.scope())
}

fn flush_branch_descriptors_for_scope(
    branch_catalog: &crate::lifecycle::LifecycleBranchCatalog,
    scope: MaintenanceTaskScope,
) -> LifecycleResult<Vec<crate::lifecycle::LifecycleBranchDescriptor>> {
    match scope {
        MaintenanceTaskScope::Branch(branch_id) => Ok(vec![branch_catalog.lookup(branch_id)?]),
        MaintenanceTaskScope::Global => Ok(branch_catalog.list_branches(false)),
        _ => Err(LifecycleError::MaintenanceTaskFailed {
            reason: "flush task must target a branch or global scope",
        }),
    }
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
        Ok(persist_flush_watermark_with_table_manifest_proof(
            self.manifest,
            self.visible_version,
            candidate,
            &LifecycleFlushWatermarkProof::TableManifestCovered(proof.clone()),
            proof.manifest_epoch(),
            proof.recovery_health_epoch(),
            &[(branch_id, table_manifest.manifest_sequence())],
        )?
        .maintenance_outcome())
    }
}

struct DurableCompactionMaintenanceRunner<'a, 'b> {
    branch: &'a mut BranchLocalState,
    table_object: &'a TableObjectService<'b>,
    table_reader: &'a TableObjectReaderService<'b>,
    table_manifest: &'a TableManifestService<'b>,
    table_catalog: &'a mut crate::lifecycle::LifecycleDurableTableCatalog,
    budget: &'a crate::lifecycle::StorageBudgetLedger,
    compaction_io_policy: LifecycleCompactionIoPolicy,
}

impl MaintenanceTaskRunner for DurableCompactionMaintenanceRunner<'_, '_> {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        let Some(request) = current_compaction_request_from_maintenance_task_with_budget(
            task,
            self.branch,
            Some(self.budget.budget()),
        )?
        else {
            return Ok(stale_compaction_maintenance_outcome());
        };
        if let Some(outcome) =
            defer_compaction_for_resource_policy(self.branch, &request, self.compaction_io_policy)?
        {
            return Ok(outcome);
        }
        let compaction = compact_durable_branch_manifest_backed(
            self.branch,
            self.table_object,
            self.table_reader,
            self.table_manifest,
            self.table_catalog,
            &request,
            Some(self.budget),
        )?;
        record_lifecycle_compaction_outcome(&compaction);
        Ok(compaction.maintenance_outcome())
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
    pending_releases: &'a mut Vec<crate::branch::facts::BranchReleasePlan>,
    pending_releases_sequence: &'a mut u64,
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
    ) -> Vec<crate::branch::facts::BranchReleasePlan> {
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

    /// Publish a fresh `PendingReleasesManifest` reflecting the
    /// post-drain buffer. Called after each drain that consumed
    /// entries so the audit trail stays consistent across restarts.
    fn publish_pending_releases(&mut self) -> LifecycleResult<()> {
        let entries = pending_releases_to_durable_entries(self.pending_releases)
            .map_err(pending_releases_format_error)?;
        *self.pending_releases_sequence = self.pending_releases_sequence.saturating_add(1);
        if *self.pending_releases_sequence == 0 {
            return Err(LifecycleError::CheckpointPublicationFailed {
                reason: "pending releases sequence overflow",
            });
        }
        let manifest = crate::format::PendingReleasesManifest::new(
            *self.services.assembly_facts().database_id(),
            *self.pending_releases_sequence,
            entries,
        )
        .map_err(pending_releases_format_error)?;
        self.services
            .pending_releases_manifest()
            .publish_replace(&manifest)
            .map_err(|error| {
                LifecycleError::lower_layer_with(
                    crate::lifecycle::LifecycleLowerLayer::Service,
                    "pending releases manifest service failed",
                    error,
                )
            })?;
        Ok(())
    }
}

fn pending_releases_to_durable_entries(
    plans: &[crate::branch::facts::BranchReleasePlan],
) -> Result<Vec<crate::format::PendingReleasesEntry>, crate::format::FormatError> {
    use std::collections::BTreeMap;
    let mut grouped: BTreeMap<[u8; 16], Vec<String>> = BTreeMap::new();
    for plan in plans {
        let key = *plan.released_branch_id().as_bytes();
        let bucket = grouped.entry(key).or_default();
        for identity in plan.releasable_tables() {
            bucket.push(identity.as_str().to_owned());
        }
    }
    let mut entries = Vec::with_capacity(grouped.len());
    for (key, mut tables) in grouped {
        tables.sort();
        tables.dedup();
        let branch_id = strata_core_next::BranchId::from_bytes(key);
        entries.push(crate::format::PendingReleasesEntry::new(branch_id, tables)?);
    }
    Ok(entries)
}

fn pending_releases_format_error(error: crate::format::FormatError) -> LifecycleError {
    LifecycleError::lower_layer_with(
        crate::lifecycle::LifecycleLowerLayer::Format,
        "pending releases manifest encode failed",
        error,
    )
}

impl MaintenanceTaskRunner for DurableRetentionMaintenanceRunner<'_, '_> {
    fn run_task(&mut self, task: &MaintenanceTask) -> LifecycleResult<MaintenanceOutcome> {
        let request = retention_request_from_maintenance_task(task)?;
        let drained = self.drain_pending_releases(request.scope());
        // Persist the post-drain buffer when entries were actually
        // removed so the audit trail stays in sync with the in-memory
        // state across restarts.
        if !drained.is_empty() {
            self.publish_pending_releases()?;
        }
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
        let (branch_id, proof) = purge_proof_from_maintenance_task(
            task,
            self.health.clone(),
            self.default_branch_id,
            inventory.token(),
        )?;
        Ok(purge_lifecycle_quarantine(
            self.quarantine,
            branch_id,
            self.database_id,
            &self.codec_id,
            &proof,
        )?
        .maintenance_outcome())
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
        let branch_id = repair_branch_from_maintenance_task(task)?;
        let outcome = match branch_id {
            Some(branch_id) => repair_branch_lifecycle_quarantine(
                self.quarantine,
                branch_id,
                self.database_id,
                &self.codec_id,
            )?,
            None => repair_lifecycle_quarantine_family(
                self.quarantine,
                self.database_id,
                &self.codec_id,
            )?,
        };
        Ok(outcome.maintenance_outcome())
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
    let status = if !snapshot_outcome.completed()
        || matches!(
            retention_outcome.status(),
            LifecycleRetentionStatus::DeferredIncompleteProof
                | LifecycleRetentionStatus::DeferredUnsupportedScope
                | LifecycleRetentionStatus::BlockedByRecoveryHealth
        )
        || matches!(
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
        .map(|(object, byte_count)| LifecycleTableObjectInventoryEntry::new(object, byte_count))
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
    drained: &[crate::branch::facts::BranchReleasePlan],
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

fn flush_watermark_candidates_from_manifest(
    manifest: &TableManifest,
    visible_version: CommitVersion,
    retention_watermark: CommitVersion,
) -> Vec<CommitVersion> {
    let mut candidates = Vec::new();
    for level in manifest.levels() {
        for table in level.tables() {
            let candidate = table.facts().commit_max();
            if candidate <= visible_version && candidate > retention_watermark {
                candidates.push(candidate);
            }
        }
    }
    for layer in manifest.inherited_layers() {
        for level in layer.levels() {
            for table in level.tables() {
                let candidate = table.facts().commit_max();
                if candidate <= visible_version && candidate > retention_watermark {
                    candidates.push(candidate);
                }
            }
        }
    }
    candidates.sort();
    candidates.dedup();
    candidates.reverse();
    candidates
}

fn bind_materialization_request_in_catalog(
    branch_catalog: &mut crate::lifecycle::LifecycleBranchCatalog,
    request: MaintenanceTaskRequest,
) -> LifecycleResult<MaintenanceTaskRequest> {
    if request.kind() != MaintenanceTaskKind::Materialization
        || request.materialization_handle().is_some()
    {
        return Ok(request);
    }
    let branch_id = branch_id_from_inherited_layer_request(request)?;
    let generation = branch_catalog
        .registry()
        .lookup(branch_id)
        .map_err(commit_error)?
        .generation();
    let branch = branch_catalog
        .branch_state_mut(branch_id, CommitBranchGenerationGuard::exact(generation))?;
    bind_materialization_task_for_enqueue(branch, request)
}

const fn branch_id_from_table_level_task(
    task: MaintenanceTask,
) -> LifecycleResult<strata_core_next::BranchId> {
    match task.scope() {
        MaintenanceTaskScope::TableLevel { branch_id, .. } => Ok(branch_id),
        _ => Err(LifecycleError::MaintenanceTaskFailed {
            reason: "compaction task must target a table level",
        }),
    }
}

const fn table_level_scope_from_task(
    task: MaintenanceTask,
) -> LifecycleResult<(strata_core_next::BranchId, u8)> {
    match task.scope() {
        MaintenanceTaskScope::TableLevel { branch_id, level } => Ok((branch_id, level)),
        _ => Err(LifecycleError::MaintenanceTaskFailed {
            reason: "compaction task must target a table level",
        }),
    }
}

const fn branch_id_from_inherited_layer_task(
    task: MaintenanceTask,
) -> LifecycleResult<strata_core_next::BranchId> {
    match task.scope() {
        MaintenanceTaskScope::InheritedLayer { branch_id, .. } => Ok(branch_id),
        _ => Err(LifecycleError::MaintenanceTaskFailed {
            reason: "materialization task must target an inherited layer",
        }),
    }
}

const fn branch_id_from_table_rewrite_task(
    task: MaintenanceTask,
) -> LifecycleResult<strata_core_next::BranchId> {
    match task.scope() {
        MaintenanceTaskScope::TableLevel { branch_id, .. }
        | MaintenanceTaskScope::InheritedLayer { branch_id, .. } => Ok(branch_id),
        _ => Err(LifecycleError::MaintenanceTaskFailed {
            reason: "table rewrite task must target a branch table scope",
        }),
    }
}

const fn branch_id_from_inherited_layer_request(
    request: MaintenanceTaskRequest,
) -> LifecycleResult<strata_core_next::BranchId> {
    match request.scope() {
        MaintenanceTaskScope::InheritedLayer { branch_id, .. } => Ok(branch_id),
        _ => Err(LifecycleError::MaintenanceTaskFailed {
            reason: "materialization task must target an inherited layer",
        }),
    }
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

fn wal_error(error: crate::service::WalServiceError) -> LifecycleError {
    LifecycleError::lower_layer_with(LifecycleLowerLayer::Service, "WAL service failed", error)
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
