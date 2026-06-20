use super::*;

fn open_runtime() -> StorageRuntime<'static> {
    StorageRuntime::open(StorageOpenOptions::cache())
        .expect("open cache runtime")
        .into_runtime()
}

fn open_manual_runtime() -> StorageRuntime<'static> {
    StorageRuntime::open(
        StorageOpenOptions::cache().with_maintenance_scheduling_policy(
            StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue,
        ),
    )
    .expect("open manual cache runtime")
    .into_runtime()
}

#[cfg(feature = "localfs")]
fn open_durable_runtime(name: &str) -> StorageRuntime<'static> {
    let backend = StorageBackend::local_fs(temp_dir_for_api_test(name));
    StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        Box::leak(Box::new(backend)),
    )
    .expect("open durable runtime")
    .into_runtime()
}

fn branch() -> BranchId {
    StorageRuntime::default_branch_id_for_test()
}

fn engine_space() -> StorageSpaceId {
    StorageSpaceId::new(vec![0x20]).expect("engine storage space")
}

fn api_key(bytes: &[u8]) -> StorageKey {
    StorageKey::new(bytes.to_vec()).expect("valid key")
}

fn put_batch(key: &[u8], value: &[u8]) -> CommitBatch {
    CommitBatch::new(
        branch(),
        vec![CommitMutation::Put {
            storage_space: engine_space(),
            key: api_key(key),
            value: StorageValue::new(value.to_vec()),
            ttl: None,
        }],
        CommitOptions::default(),
    )
    .expect("valid put batch")
}

fn diagnostics(runtime: &StorageRuntime<'_>) -> DiagnosticsOutcome {
    runtime
        .diagnostics(DiagnosticsRequest::new(DiagnosticsScope::Global))
        .expect("diagnostics")
}

fn usage(report: &DiagnosticsBudgetReport, pool: DiagnosticsBudgetPool) -> DiagnosticsBudgetUsage {
    report
        .usages()
        .iter()
        .copied()
        .find(|usage| usage.pool() == pool)
        .expect("budget pool usage")
}

#[test]
fn diagnostics_reports_healthy_recovery() {
    let mut runtime = open_runtime();
    let commit = runtime
        .commit(&put_batch(b"health", b"ok"))
        .expect("commit");

    let report = diagnostics(&runtime);

    assert_eq!(report.recovery().state(), DiagnosticsFactState::Known);
    assert_eq!(
        report.recovery().health(),
        Some(RecoveryHealthSummary::Healthy)
    );
    assert_eq!(report.recovery().faults(), &[]);
    assert_eq!(report.visible_version(), Some(commit.commit_version()));
    assert_eq!(
        report.timeline().max_version(),
        Some(commit.commit_version())
    );
}

#[test]
fn diagnostics_reports_degraded_recovery() {
    let fault = crate::lifecycle::RecoveryFault::new(
        crate::lifecycle::RecoveryFaultKind::MissingTableObject,
        "missing table object",
    )
    .expect("fault");
    let health = crate::lifecycle::RecoveryHealth::degraded(
        crate::lifecycle::RecoveryDegradationClass::DataLoss,
        vec![fault],
    )
    .expect("degraded health");

    let report = StorageRuntime::diagnostics_recovery_report_for_test(&health);

    assert_eq!(report.state(), DiagnosticsFactState::Known);
    assert_eq!(report.health(), Some(RecoveryHealthSummary::Degraded));
    assert_eq!(report.class(), Some(DiagnosticsRecoveryClass::Corruption));
    assert_eq!(report.faults().len(), 1);
}

#[cfg(feature = "localfs")]
#[test]
fn diagnostics_reports_live_degraded_recovery_from_runtime() {
    let mut runtime = open_durable_runtime("diagnostics-live-degraded");
    let fault = crate::lifecycle::RecoveryFault::new(
        crate::lifecycle::RecoveryFaultKind::MissingTableObject,
        "missing table object",
    )
    .expect("fault");
    let health = crate::lifecycle::RecoveryHealth::degraded(
        crate::lifecycle::RecoveryDegradationClass::DataLoss,
        vec![fault],
    )
    .expect("degraded health");
    runtime
        .record_recovery_health_for_test(&health)
        .expect("record health");

    let report = diagnostics(&runtime);

    assert_eq!(
        report.recovery().health(),
        Some(RecoveryHealthSummary::Degraded)
    );
    assert_eq!(
        report.recovery().class(),
        Some(DiagnosticsRecoveryClass::Corruption)
    );
}

#[test]
fn diagnostics_reports_failed_recovery() {
    let fault = crate::lifecycle::RecoveryFault::new(
        crate::lifecycle::RecoveryFaultKind::CorruptManifest,
        "manifest decode failed",
    )
    .expect("fault");
    let health = crate::lifecycle::RecoveryHealth::failed(fault);

    let report = StorageRuntime::diagnostics_recovery_report_for_test(&health);

    assert_eq!(report.health(), Some(RecoveryHealthSummary::Failed));
    assert_eq!(
        report.faults()[0].kind(),
        DiagnosticsRecoveryFaultKind::CorruptManifest
    );
}

#[cfg(feature = "localfs")]
#[test]
fn diagnostics_after_close_preserves_recovery_summary() {
    let mut runtime = open_durable_runtime("diagnostics-close-health");
    let fault = crate::lifecycle::RecoveryFault::new(
        crate::lifecycle::RecoveryFaultKind::MissingSnapshotObject,
        "missing snapshot object",
    )
    .expect("fault");
    let health = crate::lifecycle::RecoveryHealth::failed(fault);
    runtime
        .record_recovery_health_for_test(&health)
        .expect("record health");
    runtime.close().expect("close");

    let report = diagnostics(&runtime);

    assert_eq!(report.runtime_state(), StorageRuntimeState::Closed);
    assert_eq!(report.recovery().state(), DiagnosticsFactState::Known);
    assert_eq!(
        report.recovery().health(),
        Some(RecoveryHealthSummary::Failed)
    );
}

#[test]
fn diagnostics_closed_runtime_without_open_reports_unknown_recovery() {
    let runtime = StorageRuntime::closed();

    let report = diagnostics(&runtime);

    assert_eq!(report.runtime_state(), StorageRuntimeState::Closed);
    assert_eq!(report.recovery().state(), DiagnosticsFactState::Unknown);
    assert_eq!(report.recovery().health(), None);
}

#[test]
fn diagnostics_failed_io_recovery_is_not_classified_as_corruption() {
    let fault = crate::lifecycle::RecoveryFault::new(
        crate::lifecycle::RecoveryFaultKind::IoFailure,
        "read failed",
    )
    .expect("fault");
    let health = crate::lifecycle::RecoveryHealth::failed(fault);

    let report = StorageRuntime::diagnostics_recovery_report_for_test(&health);

    assert_eq!(report.health(), Some(RecoveryHealthSummary::Failed));
    assert_eq!(report.class(), Some(DiagnosticsRecoveryClass::Io));
}

#[test]
fn diagnostics_preserves_recovery_fault_class() {
    let fault = crate::lifecycle::RecoveryFault::new(
        crate::lifecycle::RecoveryFaultKind::TimelineMismatch,
        "timeline mismatch",
    )
    .expect("fault")
    .with_affected_branch(branch());
    let health = crate::lifecycle::RecoveryHealth::degraded(
        crate::lifecycle::RecoveryDegradationClass::PolicyDowngrade,
        vec![fault],
    )
    .expect("degraded health");

    let report = StorageRuntime::diagnostics_recovery_report_for_test(&health);

    assert_eq!(report.class(), Some(DiagnosticsRecoveryClass::Policy));
    assert_eq!(
        report.faults()[0].kind(),
        DiagnosticsRecoveryFaultKind::TimelineMismatch
    );
    assert_eq!(report.faults()[0].affected_branch(), Some(branch()));
}

#[test]
fn diagnostics_distinguishes_unknown_from_unsupported() {
    let runtime = open_runtime();

    let report = diagnostics(&runtime);

    assert_eq!(
        report.read_activity().state(),
        DiagnosticsFactState::Unknown
    );
    assert_eq!(
        report.table_manifest().state(),
        DiagnosticsFactState::Unsupported
    );
    assert_eq!(
        report.checkpoint().state(),
        DiagnosticsFactState::Unsupported
    );
}

#[test]
fn diagnostics_after_close_reports_closed_state() {
    let mut runtime = open_runtime();
    runtime.close().expect("close");

    let report = diagnostics(&runtime);

    assert_eq!(report.runtime_state(), StorageRuntimeState::Closed);
    assert_eq!(report.mode(), Some(StorageMode::Cache));
    assert_eq!(report.maintenance_state(), DiagnosticsFactState::Unknown);
}

#[test]
fn diagnostics_reports_memory_budget_limits() {
    let runtime = open_runtime();

    let report = diagnostics(&runtime);

    assert_eq!(report.budget().state(), DiagnosticsFactState::Known);
    assert!(report
        .budget()
        .total_limit_bytes()
        .is_some_and(|limit| limit > 0));
    assert!(usage(report.budget(), DiagnosticsBudgetPool::TableReader).limit_bytes() > 0);
}

#[test]
fn storage_memory_budget_rejects_below_minimum() {
    let error = StorageMemoryBudget::new(1024).expect_err("sub-minimum budget rejected");
    assert_eq!(error.code(), "invalid_argument.storage_api.argument");
    assert!(
        StorageMemoryBudget::new(1024 * 1024).is_ok(),
        "a 1 MiB budget is accepted"
    );
}

#[test]
fn cache_open_with_explicit_memory_budget_is_bounded() {
    let budget = StorageMemoryBudget::new(64 * 1024 * 1024).expect("budget");
    let runtime = StorageRuntime::open(StorageOpenOptions::cache().with_memory_budget(budget))
        .expect("open cache with explicit budget")
        .into_runtime();

    let report = diagnostics(&runtime);
    assert_eq!(
        report.budget().total_limit_bytes(),
        Some(64 * 1024 * 1024),
        "an explicit budget bounds cache instead of leaving it unlimited"
    );
}

#[cfg(feature = "localfs")]
#[test]
fn durable_open_with_explicit_memory_budget_reflects_it() {
    let budget = StorageMemoryBudget::new(64 * 1024 * 1024).expect("budget");
    let backend = StorageBackend::local_fs(temp_dir_for_api_test("budget-explicit-durable"));
    let runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard)
            .with_memory_budget(budget),
        Box::leak(Box::new(backend)),
    )
    .expect("open durable with explicit budget")
    .into_runtime();

    let report = diagnostics(&runtime);
    assert_eq!(report.budget().total_limit_bytes(), Some(64 * 1024 * 1024));
}

#[ignore = "L8G: cache has no background/inline maintenance executor or source-shape admission; durable executor/admission coverage is owned by L8H"]
#[test]
fn diagnostics_reports_background_scheduler_facts() {
    let runtime = open_runtime();

    let report = diagnostics(&runtime);
    let queue = report.maintenance().expect("maintenance diagnostics");

    assert_eq!(report.maintenance_state(), DiagnosticsFactState::Known);
    assert_eq!(
        queue.background_worker_count(),
        default_background_worker_count()
    );
    assert_eq!(queue.background_queue_depth(), 0);
    assert_eq!(queue.background_active_tasks(), 0);
}

#[cfg(feature = "localfs")]
#[test]
fn diagnostics_reports_durable_background_scheduler_facts() {
    let runtime = open_durable_runtime("diagnostics-durable-background");

    let report = diagnostics(&runtime);
    let queue = report.maintenance().expect("maintenance diagnostics");

    assert_eq!(report.maintenance_state(), DiagnosticsFactState::Known);
    assert_eq!(
        queue.background_worker_count(),
        default_background_worker_count()
    );
    assert_eq!(queue.background_queue_depth(), 0);
    assert_eq!(queue.background_active_tasks(), 0);
}

#[test]
fn diagnostics_reports_memory_budget_usage() {
    let mut runtime = open_manual_runtime();
    runtime
        .enqueue_maintenance(&MaintenanceRequest::new(
            MaintenanceTask::Flush,
            MaintenanceScope::Branch(branch()),
        ))
        .expect("enqueue flush");

    let report = diagnostics(&runtime);
    let queue = usage(report.budget(), DiagnosticsBudgetPool::MaintenanceQueue);

    assert_eq!(queue.used_count(), 1);
    assert!(queue.used_bytes() > 0);
}

#[test]
fn diagnostics_reports_cache_budget_facts() {
    // Cache now obeys the same budget as durable (the Default profile by default) rather than
    // running unbounded, so its total and mutable pools are finite.
    let runtime = StorageRuntime::open(StorageOpenOptions::cache())
        .expect("open cache runtime")
        .into_runtime();

    let report = diagnostics(&runtime);
    let frozen = usage(report.budget(), DiagnosticsBudgetPool::FrozenMutable);
    let active = usage(report.budget(), DiagnosticsBudgetPool::ActiveMutable);

    // Finite Default-profile pools, not the old effectively-unlimited cache.
    assert_eq!(report.budget().total_limit_bytes(), Some(512 * 1024 * 1024));
    assert!(frozen.limit_bytes() > 0 && frozen.limit_bytes() < (1 << 50));
    assert!(active.limit_bytes() > 0 && active.limit_bytes() < (1 << 50));
    assert_eq!(frozen.pressure(), DiagnosticsBudgetPressure::Normal);
    assert_eq!(active.pressure(), DiagnosticsBudgetPressure::Normal);
}

#[test]
fn diagnostics_reports_budget_usage_accuracy() {
    // Each pool reports whether its usage is a tracked live figure or an admission-only estimate
    // (the diagnostics contract's "exact or approximate"). Runtime-summed and counted pools are
    // tracked; the pools admitted per allocation via check_available are admission-only.
    let runtime = open_runtime();
    let report = diagnostics(&runtime);
    let budget = report.budget();

    for pool in [
        DiagnosticsBudgetPool::BlockCache,
        DiagnosticsBudgetPool::ActiveMutable,
        DiagnosticsBudgetPool::FrozenMutable,
        DiagnosticsBudgetPool::MaintenanceQueue,
    ] {
        assert_eq!(
            usage(budget, pool).accuracy(),
            DiagnosticsBudgetAccuracy::Tracked,
            "{pool:?} usage should be a tracked live figure"
        );
    }

    for pool in [
        DiagnosticsBudgetPool::TableReader,
        DiagnosticsBudgetPool::GeneratedArtifact,
        DiagnosticsBudgetPool::ManifestCatalog,
    ] {
        assert_eq!(
            usage(budget, pool).accuracy(),
            DiagnosticsBudgetAccuracy::AdmissionOnly,
            "{pool:?} usage is admission-only and not retained after the call"
        );
    }

    // The database-wide total is the tracked-live resident sum (admission-only pools excluded).
    assert_eq!(
        budget.total_used_accuracy(),
        Some(DiagnosticsBudgetAccuracy::Tracked)
    );
}

#[test]
fn diagnostics_reports_lazy_read_counters() {
    let runtime = open_runtime();

    let report = diagnostics(&runtime);

    assert_eq!(
        report.read_activity().state(),
        DiagnosticsFactState::Unknown
    );
    assert_eq!(report.read_activity().block_hits(), None);
    assert_eq!(report.read_activity().block_misses(), None);
}

#[test]
fn diagnostics_reports_pressure_facts() {
    let runtime = open_runtime();

    let report = diagnostics(&runtime);

    assert_eq!(report.pressure().state(), DiagnosticsFactState::Known);
    assert_eq!(report.pressure().branch_id(), Some(branch()));
    assert_eq!(
        report.pressure().severity(),
        DiagnosticsStoragePressureSeverity::None
    );
}

#[test]
fn diagnostics_reports_cache_volatile_in_memory_shape() {
    // Cache reports its volatile in-memory source layout explicitly (Known with
    // active/frozen rows and zero owned/inherited tables) rather than treating
    // absent durable table shape as unknown. Durable-only facts are Unsupported.
    let mut runtime = open_runtime();
    runtime
        .commit(&put_batch(b"volatile-shape", b"value"))
        .expect("commit");

    let report = diagnostics(&runtime);

    let layout = report.source_layout();
    assert_eq!(layout.state(), DiagnosticsFactState::Known);
    assert!(layout.active_rows() > 0);
    assert_eq!(layout.owned_total_tables(), 0);
    assert_eq!(layout.owned_l0_tables(), 0);
    assert_eq!(layout.inherited_total_tables(), 0);

    // Durable table-reachability, checkpoint, and retention facts are
    // unsupported for volatile cache mode.
    assert_eq!(
        report.table_manifest().state(),
        DiagnosticsFactState::Unsupported
    );
    assert_eq!(
        report.checkpoint().state(),
        DiagnosticsFactState::Unsupported
    );
    assert_eq!(
        report.retention().state(),
        DiagnosticsFactState::Unsupported
    );
}

#[test]
fn diagnostics_reports_neutralized_cache_pressure_with_frozen_tables() {
    // Review-fix regression guard: even when frozen tables exist (rotation
    // without flush), cache diagnostics report neutralized source-shape pressure
    // (severity None) while the source layout still shows the frozen tables. This
    // proves diagnostics pressure flows through the neutralized cache path.
    let mut runtime = open_runtime();
    runtime
        .commit(&put_batch(b"frozen-pressure", b"value"))
        .expect("commit");
    runtime
        .rotate_default_branch_for_test()
        .expect("rotate active table into frozen source");

    let report = diagnostics(&runtime);

    assert!(report.source_layout().frozen_table_count() > 0);
    assert_eq!(report.pressure().state(), DiagnosticsFactState::Known);
    assert_eq!(
        report.pressure().severity(),
        DiagnosticsStoragePressureSeverity::None
    );
    assert_eq!(
        report.pressure().reason(),
        DiagnosticsStoragePressureReason::None
    );
}

#[test]
fn diagnostics_reports_source_layout_after_flush_and_compact() {
    let mut runtime = open_runtime();
    runtime
        .commit(&put_batch(b"layout-key", b"layout-value"))
        .expect("commit");
    let active = diagnostics(&runtime);

    runtime
        .maintenance(&MaintenanceRequest::new(
            MaintenanceTask::Flush,
            MaintenanceScope::Branch(branch()),
        ))
        .expect("flush");
    let flushed = diagnostics(&runtime);

    runtime
        .maintenance(&MaintenanceRequest::new(
            MaintenanceTask::Compact,
            MaintenanceScope::Branch(branch()),
        ))
        .expect("compact");
    let compacted = diagnostics(&runtime);

    assert_eq!(active.source_layout().state(), DiagnosticsFactState::Known);
    assert!(active.source_layout().active_rows() > 0);
    assert_eq!(active.source_layout().owned_l0_tables(), 0);
    assert_eq!(flushed.source_layout().active_rows(), 0);
    assert_eq!(flushed.source_layout().frozen_rows(), 0);
    assert_eq!(flushed.source_layout().owned_l0_tables(), 1);
    assert_eq!(compacted.source_layout().active_rows(), 0);
    assert_eq!(compacted.source_layout().owned_l0_tables(), 0);
    assert_eq!(compacted.source_layout().owned_total_tables(), 1);
    assert_eq!(
        compacted
            .source_layout()
            .owned_nonzero_level_table_counts()
            .last()
            .map(|count| (count.level(), count.table_count())),
        Some((7, 1))
    );
}

#[test]
fn diagnostics_branch_scope_reports_requested_branch_pressure() {
    let mut runtime = open_runtime();
    let child = branch_id(0x45);
    runtime
        .branch(&BranchRequest::new(
            child,
            BranchAction::Create,
            Some(BranchGeneration::new(2)),
        ))
        .expect("create branch");

    let report = runtime
        .diagnostics(DiagnosticsRequest::new(DiagnosticsScope::Branch(child)))
        .expect("diagnostics");

    assert_eq!(report.pressure().state(), DiagnosticsFactState::Known);
    assert_eq!(report.pressure().branch_id(), Some(child));
}

#[test]
fn diagnostics_unknown_branch_scope_marks_pressure_unknown() {
    let runtime = open_runtime();

    let report = runtime
        .diagnostics(DiagnosticsRequest::new(DiagnosticsScope::Branch(
            branch_id(0x46),
        )))
        .expect("diagnostics");

    assert_eq!(report.pressure().state(), DiagnosticsFactState::Unknown);
    assert_eq!(report.pressure().branch_id(), None);
}

#[test]
fn diagnostics_cache_mode_marks_durable_facts_unsupported() {
    let runtime = open_runtime();

    let report = diagnostics(&runtime);

    assert_eq!(
        report.table_manifest().state(),
        DiagnosticsFactState::Unsupported
    );
    assert_eq!(
        report.retention().state(),
        DiagnosticsFactState::Unsupported
    );
    assert_eq!(
        report.quarantine().state(),
        DiagnosticsFactState::Unsupported
    );
    assert_eq!(
        report.checkpoint().state(),
        DiagnosticsFactState::Unsupported
    );
}

#[cfg(feature = "localfs")]
#[test]
fn diagnostics_reports_table_manifest_reachability() {
    let runtime = open_durable_runtime("diagnostics-table-manifest");

    let report = diagnostics(&runtime);

    assert_eq!(report.table_manifest().state(), DiagnosticsFactState::Known);
    assert_eq!(report.table_manifest().table_count(), 0);
    assert_eq!(report.table_manifest().object_count(), 0);
    assert_eq!(report.table_manifest().next_manifest_sequence(), Some(1));
}

#[cfg(feature = "localfs")]
#[test]
fn diagnostics_reports_table_object_retention_summary() {
    let runtime = open_durable_runtime("diagnostics-retention");

    let report = diagnostics(&runtime);

    assert_eq!(report.retention().state(), DiagnosticsFactState::Known);
    assert_eq!(report.retention().protected_objects(), None);
    assert_eq!(report.retention().pending_releases(), Some(0));
    assert_eq!(report.retention().reclaimed_objects(), None);
}

#[cfg(feature = "localfs")]
#[test]
fn diagnostics_reports_quarantine_summary() {
    let runtime = open_durable_runtime("diagnostics-quarantine");

    let report = diagnostics(&runtime);

    assert_eq!(report.quarantine().state(), DiagnosticsFactState::Unknown);
    assert_eq!(report.quarantine().quarantined_objects(), None);
}

#[test]
fn diagnostics_reports_wal_growth_policy() {
    let runtime = StorageRuntime::open(
        StorageOpenOptions::cache().with_wal_growth_policy(StorageWalGrowthPolicy::Disabled),
    )
    .expect("open cache runtime")
    .into_runtime();

    let report = diagnostics(&runtime);

    assert_eq!(report.wal_growth().state(), DiagnosticsFactState::Known);
    assert!(!report.wal_growth().policy_enabled());
    assert!(report.wal_growth().last_status().is_some());
}

#[cfg(feature = "localfs")]
#[test]
fn diagnostics_reports_checkpoint_watermark() {
    let runtime = open_durable_runtime("diagnostics-checkpoint");

    let report = diagnostics(&runtime);

    assert_eq!(report.checkpoint().state(), DiagnosticsFactState::Known);
    assert_eq!(report.checkpoint().checkpoint_watermark(), None);
    assert_eq!(report.checkpoint().flush_watermark(), None);
}

#[cfg(feature = "localfs")]
#[test]
fn diagnostics_manifest_read_failure_marks_checkpoint_unknown() {
    let backend = StorageBackend::local_fs(temp_dir_for_api_test("diagnostics-corrupt-manifest"));
    let runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard),
        &backend,
    )
    .expect("open durable runtime")
    .into_runtime();
    let manifest = crate::layout::ObjectLayout::database_manifest().expect("manifest object");
    backend
        .as_backend()
        .write_object(&manifest, b"not a database manifest")
        .expect("corrupt manifest");

    let report = runtime
        .diagnostics(DiagnosticsRequest::new(DiagnosticsScope::Global))
        .expect("diagnostics remains partial");

    assert_eq!(report.checkpoint().state(), DiagnosticsFactState::Unknown);
    assert_eq!(report.table_manifest().state(), DiagnosticsFactState::Known);
}

#[test]
fn diagnostics_reports_branch_count_and_generation_summary() {
    let mut runtime = open_runtime();
    let child = branch_id(0x44);
    runtime
        .branch(&BranchRequest::new(
            child,
            BranchAction::Create,
            Some(BranchGeneration::new(2)),
        ))
        .expect("create branch");

    let report = diagnostics(&runtime);

    assert_eq!(report.branch_catalog().state(), DiagnosticsFactState::Known);
    assert_eq!(report.branch_catalog().active_branches(), 2);
    assert_eq!(
        report.branch_catalog().min_generation(),
        Some(BranchGeneration::new(1))
    );
    assert_eq!(
        report.branch_catalog().max_generation(),
        Some(BranchGeneration::new(2))
    );
}

#[test]
fn diagnostics_branch_generation_summary_ignores_deleted_branches() {
    let mut runtime = open_runtime();
    let active = branch_id(0x47);
    let deleted = branch_id(0x48);
    runtime
        .branch(&BranchRequest::new(
            active,
            BranchAction::Create,
            Some(BranchGeneration::new(5)),
        ))
        .expect("create active branch");
    runtime
        .branch(&BranchRequest::new(
            deleted,
            BranchAction::Create,
            Some(BranchGeneration::new(9)),
        ))
        .expect("create branch to delete");
    runtime
        .branch(&BranchRequest::new(
            deleted,
            BranchAction::Delete,
            Some(BranchGeneration::new(9)),
        ))
        .expect("delete branch");

    let report = diagnostics(&runtime);

    assert_eq!(report.branch_catalog().active_branches(), 2);
    assert_eq!(report.branch_catalog().deleted_branches(), 1);
    assert_eq!(
        report.branch_catalog().min_generation(),
        Some(BranchGeneration::new(1))
    );
    assert_eq!(
        report.branch_catalog().max_generation(),
        Some(BranchGeneration::new(5))
    );
}
