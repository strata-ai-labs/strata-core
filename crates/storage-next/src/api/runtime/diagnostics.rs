use super::error::{commit_error, default_branch_generation};
use super::{
    collect_storage_pressure_with_budget, BranchCleanupSummary, BranchGeneration, BranchId,
    BranchParentSummary, BranchReleasePlan, BranchStatus, BranchSummary, CommitBranchGeneration,
    CommitBranchGenerationGuard, CommitVersion, DiagnosticsBranchCatalogReport,
    DiagnosticsBudgetPool, DiagnosticsBudgetPressure, DiagnosticsBudgetReport,
    DiagnosticsBudgetUsage, DiagnosticsCheckpointReport, DiagnosticsRecoveryClass,
    DiagnosticsRecoveryFault, DiagnosticsRecoveryFaultKind, DiagnosticsRecoveryReport,
    DiagnosticsScope, DiagnosticsSourceLayoutReport, DiagnosticsSourceLevelTableCount,
    DiagnosticsStoragePressureReason, DiagnosticsStoragePressureReport,
    DiagnosticsStoragePressureSeverity, DiagnosticsWalGrowthReport, LifecycleBranchCatalog,
    LifecycleBranchDescriptor, LifecycleBranchStatus, LifecycleDurableLocalRuntime,
    LifecycleStorageMode, LifecycleStoragePressureReason, LifecycleStoragePressureSeverity,
    LifecycleWalGrowthPolicy, MaintenanceExecutorStatus, MaintenanceWalGrowthSummary,
    ModeLifecyclePolicy, RecoveryDegradationClass, RecoveryFaultKind, RecoveryHealth,
    RecoveryHealthSummary, StorageApiError, StorageApiResult, StorageBudgetPool,
    StorageBudgetPressureSeverity, StorageBudgetSnapshot, StorageDurabilityPolicy, StorageMode,
    StorageOpenPlan, StorageOpenSummary, StorageRuntime, StorageRuntimeBudget, StorageRuntimeInner,
    WalGrowthFacts, DEFAULT_BRANCH_ID,
};

pub(super) fn map_generation_guard(
    generation: Option<BranchGeneration>,
) -> StorageApiResult<CommitBranchGenerationGuard> {
    match generation {
        Some(generation) if generation == BranchGeneration::ZERO => {
            Err(StorageApiError::InvalidArgument {
                field: "branch_generation",
                reason: "expected branch generation must be nonzero",
            })
        }
        Some(generation) => CommitBranchGeneration::new(generation.as_u64())
            .map(CommitBranchGenerationGuard::exact)
            .map_err(commit_error),
        None => Ok(CommitBranchGenerationGuard::not_supplied()),
    }
}

pub(super) fn branch_generation_or_default(
    generation: Option<BranchGeneration>,
) -> StorageApiResult<CommitBranchGeneration> {
    match generation {
        Some(generation) if generation == BranchGeneration::ZERO => {
            Err(StorageApiError::InvalidArgument {
                field: "branch_generation",
                reason: "branch generation must be nonzero",
            })
        }
        Some(generation) => CommitBranchGeneration::new(generation.as_u64()).map_err(commit_error),
        None => default_branch_generation(),
    }
}

pub(super) fn require_valid_branch_identifier(
    branch_id: BranchId,
    field: &'static str,
) -> StorageApiResult<()> {
    if branch_id.as_bytes().iter().all(|byte| *byte == 0) {
        Err(StorageApiError::InvalidArgument {
            field,
            reason: "branch id must not be all zero",
        })
    } else {
        Ok(())
    }
}

#[allow(
    clippy::match_same_arms,
    reason = "borrowed and owned durable runtime variants share behavior but carry different backend lifetimes"
)]
pub(super) fn current_visible(runtime: &StorageRuntime<'_>) -> Option<CommitVersion> {
    let version = match &runtime.inner {
        StorageRuntimeInner::Cache(slot) => {
            let runtime = slot.lock();
            runtime.visible_version()
        }
        StorageRuntimeInner::Durable(slot) => {
            let runtime = slot.lock();
            runtime.visible_version()
        }
        StorageRuntimeInner::DurableOwned(slot) => {
            let runtime = slot.lock();
            runtime.visible_version()
        }
        StorageRuntimeInner::Closed => CommitVersion::ZERO,
    };
    (version != CommitVersion::ZERO).then_some(version)
}

pub(super) fn branch_for_diagnostics_scope(scope: DiagnosticsScope) -> BranchId {
    match scope {
        DiagnosticsScope::Global => DEFAULT_BRANCH_ID,
        DiagnosticsScope::Branch(branch_id) => branch_id,
    }
}

pub(super) fn diagnostics_mode_from_plan(
    open_summary: Option<StorageOpenSummary>,
    plan: &StorageOpenPlan,
) -> StorageMode {
    open_summary.map_or_else(
        || map_lifecycle_storage_mode(plan.storage_mode()),
        StorageOpenSummary::mode,
    )
}

pub(super) const fn map_lifecycle_storage_mode(mode: LifecycleStorageMode) -> StorageMode {
    match mode {
        LifecycleStorageMode::Cache => StorageMode::Cache,
        LifecycleStorageMode::DurableLocalStandard => StorageMode::DurableLocal {
            policy: StorageDurabilityPolicy::Standard,
        },
        LifecycleStorageMode::DurableLocalAlways => StorageMode::DurableLocal {
            policy: StorageDurabilityPolicy::Always,
        },
        LifecycleStorageMode::ObjectDurableCandidate => StorageMode::ObjectDurableCandidate,
    }
}

pub(super) fn durable_checkpoint_report<S>(
    runtime: &LifecycleDurableLocalRuntime<'_, S>,
) -> DiagnosticsCheckpointReport {
    let Some(manifest) = runtime.services().manifest().load_current().ok().flatten() else {
        return DiagnosticsCheckpointReport::unknown();
    };
    DiagnosticsCheckpointReport::known(
        manifest.snapshot_id(),
        manifest.snapshot_watermark().map(CommitVersion::new),
        manifest.flushed_through_commit_id(),
    )
}

pub(super) fn map_diagnostics_recovery(health: &RecoveryHealth) -> DiagnosticsRecoveryReport {
    match health {
        RecoveryHealth::Healthy => DiagnosticsRecoveryReport::healthy(),
        RecoveryHealth::Degraded { class, faults } => DiagnosticsRecoveryReport::new(
            RecoveryHealthSummary::Degraded,
            Some(map_diagnostics_recovery_class(*class)),
            faults.iter().map(map_diagnostics_recovery_fault).collect(),
        ),
        RecoveryHealth::Failed { fault } => DiagnosticsRecoveryReport::new(
            RecoveryHealthSummary::Failed,
            Some(map_failed_diagnostics_recovery_class(fault.kind())),
            vec![map_diagnostics_recovery_fault(fault)],
        ),
    }
}

pub(super) const fn map_diagnostics_recovery_class(
    class: RecoveryDegradationClass,
) -> DiagnosticsRecoveryClass {
    match class {
        RecoveryDegradationClass::DataLoss => DiagnosticsRecoveryClass::Corruption,
        RecoveryDegradationClass::PolicyDowngrade => DiagnosticsRecoveryClass::Policy,
        RecoveryDegradationClass::Telemetry => DiagnosticsRecoveryClass::Telemetry,
    }
}

pub(super) const fn map_failed_diagnostics_recovery_class(
    kind: RecoveryFaultKind,
) -> DiagnosticsRecoveryClass {
    match kind {
        RecoveryFaultKind::IoFailure | RecoveryFaultKind::WalTailRepairFailed => {
            DiagnosticsRecoveryClass::Io
        }
        RecoveryFaultKind::NoManifestFallback => DiagnosticsRecoveryClass::Policy,
        RecoveryFaultKind::CorruptManifest
        | RecoveryFaultKind::CorruptSnapshot
        | RecoveryFaultKind::CorruptWal
        | RecoveryFaultKind::MissingManifestObject
        | RecoveryFaultKind::MissingSnapshotObject
        | RecoveryFaultKind::MissingTableObject
        | RecoveryFaultKind::InheritedLayerLoss
        | RecoveryFaultKind::QuarantineInventoryMismatch
        | RecoveryFaultKind::TimelineMismatch => DiagnosticsRecoveryClass::Corruption,
    }
}

pub(super) fn map_diagnostics_recovery_fault(
    fault: &crate::lifecycle::RecoveryFault,
) -> DiagnosticsRecoveryFault {
    DiagnosticsRecoveryFault::new(
        map_diagnostics_recovery_fault_kind(fault.kind()),
        fault.reason(),
        fault.affected_branch(),
    )
}

pub(super) const fn map_diagnostics_recovery_fault_kind(
    kind: RecoveryFaultKind,
) -> DiagnosticsRecoveryFaultKind {
    match kind {
        RecoveryFaultKind::CorruptManifest => DiagnosticsRecoveryFaultKind::CorruptManifest,
        RecoveryFaultKind::CorruptSnapshot => DiagnosticsRecoveryFaultKind::CorruptSnapshot,
        RecoveryFaultKind::CorruptWal => DiagnosticsRecoveryFaultKind::CorruptWal,
        RecoveryFaultKind::MissingManifestObject => {
            DiagnosticsRecoveryFaultKind::MissingManifestObject
        }
        RecoveryFaultKind::MissingSnapshotObject => {
            DiagnosticsRecoveryFaultKind::MissingSnapshotObject
        }
        RecoveryFaultKind::MissingTableObject => DiagnosticsRecoveryFaultKind::MissingTableObject,
        RecoveryFaultKind::InheritedLayerLoss => DiagnosticsRecoveryFaultKind::InheritedLayerLoss,
        RecoveryFaultKind::NoManifestFallback => DiagnosticsRecoveryFaultKind::NoManifestFallback,
        RecoveryFaultKind::IoFailure => DiagnosticsRecoveryFaultKind::IoFailure,
        RecoveryFaultKind::QuarantineInventoryMismatch => {
            DiagnosticsRecoveryFaultKind::QuarantineInventoryMismatch
        }
        RecoveryFaultKind::TimelineMismatch => DiagnosticsRecoveryFaultKind::TimelineMismatch,
        RecoveryFaultKind::WalTailRepairFailed => DiagnosticsRecoveryFaultKind::WalTailRepairFailed,
    }
}

pub(super) fn map_budget_report(snapshot: &StorageBudgetSnapshot) -> DiagnosticsBudgetReport {
    let usages = snapshot
        .usages()
        .iter()
        .map(|usage| {
            DiagnosticsBudgetUsage::new(
                map_budget_pool(usage.pool()),
                usage.used_bytes(),
                usage.limit_bytes(),
                usage.used_count(),
                usage.limit_count(),
                map_budget_pressure(snapshot.pressure(usage.pool())),
            )
        })
        .collect();
    DiagnosticsBudgetReport::known(snapshot.budget().total_bytes(), usages)
}

pub(super) const fn map_budget_pool(pool: StorageBudgetPool) -> DiagnosticsBudgetPool {
    match pool {
        StorageBudgetPool::BlockCache => DiagnosticsBudgetPool::BlockCache,
        StorageBudgetPool::TableReader => DiagnosticsBudgetPool::TableReader,
        StorageBudgetPool::ActiveMutable => DiagnosticsBudgetPool::ActiveMutable,
        StorageBudgetPool::FrozenMutable => DiagnosticsBudgetPool::FrozenMutable,
        StorageBudgetPool::MaintenanceQueue => DiagnosticsBudgetPool::MaintenanceQueue,
        StorageBudgetPool::GeneratedArtifact => DiagnosticsBudgetPool::GeneratedArtifact,
        StorageBudgetPool::ManifestCatalog => DiagnosticsBudgetPool::ManifestCatalog,
    }
}

pub(super) const fn map_budget_pressure(
    severity: StorageBudgetPressureSeverity,
) -> DiagnosticsBudgetPressure {
    match severity {
        StorageBudgetPressureSeverity::Normal => DiagnosticsBudgetPressure::Normal,
        StorageBudgetPressureSeverity::Evicting => DiagnosticsBudgetPressure::Evicting,
        StorageBudgetPressureSeverity::DeferOptionalMaintenance => {
            DiagnosticsBudgetPressure::DeferOptionalMaintenance
        }
        StorageBudgetPressureSeverity::RejectOptionalWork => {
            DiagnosticsBudgetPressure::RejectOptionalWork
        }
        StorageBudgetPressureSeverity::RejectMutatingAdmission => {
            DiagnosticsBudgetPressure::RejectMutatingAdmission
        }
    }
}

pub(super) fn diagnostics_pressure_report(
    catalog: &LifecycleBranchCatalog,
    branch_id: BranchId,
    maintenance: MaintenanceExecutorStatus,
    budget: StorageRuntimeBudget,
    policy: ModeLifecyclePolicy,
) -> DiagnosticsStoragePressureReport {
    let Ok(branch) = catalog.branch_state(branch_id) else {
        return DiagnosticsStoragePressureReport::unknown();
    };
    let pressure = collect_storage_pressure_with_budget(branch, maintenance, Some(budget));
    // Mirror the admission chokepoint: volatile modes (cache) apply no
    // source-shape pressure, so diagnostics must report the neutralized view
    // rather than raw backlog the engine never acts on.
    let pressure = if policy.may_apply_source_shape_admission_pressure() {
        pressure
    } else {
        pressure.with_source_shape_neutralized()
    };
    DiagnosticsStoragePressureReport::known(
        branch_id,
        map_storage_pressure_severity(pressure.severity()),
        map_storage_pressure_reason(pressure.reason()),
        pressure.active_rows(),
        pressure.active_bytes(),
        pressure.frozen_tables(),
        pressure.frozen_bytes(),
        pressure.level_zero_tables(),
        pressure.owned_tables(),
        pressure.inherited_layers(),
        pressure.pending_maintenance(),
    )
}

pub(super) fn diagnostics_source_layout_report(
    catalog: &LifecycleBranchCatalog,
    branch_id: BranchId,
) -> DiagnosticsSourceLayoutReport {
    let Ok(branch) = catalog.branch_state(branch_id) else {
        return DiagnosticsSourceLayoutReport::unknown();
    };
    let layout = branch.source_layout();
    DiagnosticsSourceLayoutReport::known(
        layout.active_rows(),
        layout.frozen_table_count(),
        layout.frozen_rows(),
        layout.owned_l0_tables(),
        map_source_level_table_counts(layout.owned_nonzero_level_table_counts()),
        layout.owned_total_tables(),
        layout.inherited_layers(),
        layout.inherited_l0_tables(),
        map_source_level_table_counts(layout.inherited_nonzero_level_table_counts()),
        layout.inherited_total_tables(),
    )
}

pub(super) fn map_source_level_table_counts(
    counts: &[crate::branch::facts::BranchLevelTableCount],
) -> Vec<DiagnosticsSourceLevelTableCount> {
    counts
        .iter()
        .map(|count| {
            DiagnosticsSourceLevelTableCount::new(count.level().raw(), count.table_count())
        })
        .collect()
}

pub(super) const fn map_storage_pressure_severity(
    severity: LifecycleStoragePressureSeverity,
) -> DiagnosticsStoragePressureSeverity {
    match severity {
        LifecycleStoragePressureSeverity::None => DiagnosticsStoragePressureSeverity::None,
        LifecycleStoragePressureSeverity::Background => {
            DiagnosticsStoragePressureSeverity::Background
        }
        LifecycleStoragePressureSeverity::Urgent => DiagnosticsStoragePressureSeverity::Urgent,
        LifecycleStoragePressureSeverity::BlockMutatingAdmission => {
            DiagnosticsStoragePressureSeverity::BlockMutatingAdmission
        }
    }
}

pub(super) const fn map_storage_pressure_reason(
    reason: LifecycleStoragePressureReason,
) -> DiagnosticsStoragePressureReason {
    match reason {
        LifecycleStoragePressureReason::None => DiagnosticsStoragePressureReason::None,
        LifecycleStoragePressureReason::ActiveMutableBytes => {
            DiagnosticsStoragePressureReason::ActiveMutableBytes
        }
        LifecycleStoragePressureReason::FrozenBacklog => {
            DiagnosticsStoragePressureReason::FrozenBacklog
        }
        LifecycleStoragePressureReason::LevelZeroTableBacklog => {
            DiagnosticsStoragePressureReason::LevelZeroTableBacklog
        }
        LifecycleStoragePressureReason::NonZeroLevelTableBacklog => {
            DiagnosticsStoragePressureReason::NonZeroLevelTableBacklog
        }
        LifecycleStoragePressureReason::InheritedLayerBacklog => {
            DiagnosticsStoragePressureReason::InheritedLayerBacklog
        }
        LifecycleStoragePressureReason::MaintenanceQueueBacklog => {
            DiagnosticsStoragePressureReason::MaintenanceQueueBacklog
        }
    }
}

pub(super) fn map_wal_growth_report(
    policy: LifecycleWalGrowthPolicy,
    current_facts: Option<WalGrowthFacts>,
    last_status: Option<MaintenanceWalGrowthSummary>,
) -> DiagnosticsWalGrowthReport {
    DiagnosticsWalGrowthReport::known_with_current_retention(
        policy.enabled(),
        Some(policy.max_retained_wal_bytes()),
        Some(policy.max_retained_wal_segments()),
        current_facts.map(WalGrowthFacts::retained_bytes),
        current_facts.map(WalGrowthFacts::retained_segments),
        Some(policy.max_commits_since_checkpoint()),
        last_status,
    )
}

pub(super) fn map_branch_catalog_report(
    branches: &[BranchSummary],
) -> DiagnosticsBranchCatalogReport {
    let mut active_branches = 0;
    let mut deleted_branches = 0;
    let mut min_generation = None;
    let mut max_generation = None;
    for branch in branches {
        match branch.status() {
            BranchStatus::Active => {
                active_branches += 1;
                min_generation = Some(
                    min_generation.map_or(branch.generation(), |generation: BranchGeneration| {
                        generation.min(branch.generation())
                    }),
                );
                max_generation = Some(
                    max_generation.map_or(branch.generation(), |generation: BranchGeneration| {
                        generation.max(branch.generation())
                    }),
                );
            }
            BranchStatus::Deleted => deleted_branches += 1,
        }
    }
    DiagnosticsBranchCatalogReport::known(
        active_branches,
        deleted_branches,
        min_generation,
        max_generation,
    )
}

pub(super) fn map_branch_descriptor(descriptor: LifecycleBranchDescriptor) -> BranchSummary {
    let status = match descriptor.status() {
        LifecycleBranchStatus::Active => BranchStatus::Active,
        LifecycleBranchStatus::Deleted => BranchStatus::Deleted,
    };
    let parent = descriptor
        .parent()
        .map(|parent| BranchParentSummary::new(parent.source_branch_id(), parent.fork_version()));
    BranchSummary::new(
        descriptor.branch_id(),
        BranchGeneration::new(descriptor.generation().get()),
        status,
        parent,
        descriptor.created_at(),
        descriptor.deleted_at(),
        descriptor.state_revision(),
    )
}

pub(super) fn map_branch_cleanup(release_plan: &BranchReleasePlan) -> BranchCleanupSummary {
    BranchCleanupSummary::new(
        release_plan.removed_refs().len(),
        release_plan.releasable_tables().len(),
        release_plan.protected_tables().len(),
    )
}
