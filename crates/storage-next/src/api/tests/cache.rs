use super::*;

#[test]
fn open_options_reject_cache_lossy_recovery() {
    let options = StorageOpenOptions::cache().with_strict_recovery(false);
    let validation = options
        .validate()
        .expect_err("cache lossy recovery rejected");
    let open = StorageRuntime::open(options).expect_err("cache lossy recovery rejected");

    assert_eq!(validation.code(), "invalid_argument.storage_api.argument");
    assert_eq!(open.code(), "invalid_argument.storage_api.argument");
}

#[test]
fn open_options_rejects_durable_without_local_backend() {
    let error = StorageRuntime::open(StorageOpenOptions::durable_local(
        StorageDurabilityPolicy::Standard,
    ))
    .expect_err("durable local open requires explicit backend");

    assert_eq!(error.code(), "invalid_argument.storage_api.argument");
}

#[test]
fn open_options_preserves_recovery_strictness() {
    let strict = StorageOpenOptions::cache();
    let lossy = StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard)
        .with_strict_recovery(false);

    assert!(strict.strict_recovery());
    assert!(!lossy.strict_recovery());
}

#[test]
fn open_cache_returns_open_runtime_and_cache_summary() {
    let outcome =
        StorageRuntime::open(StorageOpenOptions::cache()).expect("cache open should succeed");
    let summary = outcome.summary();
    let runtime = outcome.into_runtime();

    assert_eq!(runtime.state(), StorageRuntimeState::Open);
    assert!(runtime.is_open());
    assert_eq!(summary.mode(), StorageMode::Cache);
    assert_eq!(summary.disposition(), StorageOpenDisposition::Created);
    assert_eq!(summary.recovery_health(), RecoveryHealthSummary::Healthy);
    assert_eq!(summary.recovered_visible_version(), None);
    assert!(summary.maintenance_ready());
    assert_eq!(
        summary.maintenance_scheduling_policy(),
        StorageMaintenanceSchedulingPolicy::Background
    );
    assert!(!summary.has_durable_recovery_facts());
    assert!(summary.backend_capabilities_used());
    assert_eq!(
        runtime.maintenance_scheduling_policy_for_test(),
        crate::lifecycle::LifecycleMaintenanceSchedulingPolicy::Background
    );
    let queue = runtime.maintenance_status().expect("maintenance status");
    assert_eq!(queue.background_worker_count(), 0);
    assert_eq!(queue.background_queue_depth(), 0);
    assert_eq!(queue.background_active_tasks(), 0);
}

#[test]
fn open_ephemeral_returns_open_runtime_and_cache_summary() {
    let outcome = StorageRuntime::open_ephemeral().expect("ephemeral open");
    let summary = outcome.summary();
    let runtime = outcome.into_runtime();

    assert_eq!(runtime.state(), StorageRuntimeState::Open);
    assert_eq!(summary.mode(), StorageMode::Cache);
    assert_eq!(summary.disposition(), StorageOpenDisposition::Created);
    assert_eq!(summary.recovery_health(), RecoveryHealthSummary::Healthy);
    assert_eq!(
        summary.maintenance_scheduling_policy(),
        StorageMaintenanceSchedulingPolicy::Background
    );
    assert!(!summary.has_durable_recovery_facts());
    assert_eq!(
        runtime.maintenance_scheduling_policy_for_test(),
        crate::lifecycle::LifecycleMaintenanceSchedulingPolicy::Background
    );
    assert_eq!(
        runtime
            .maintenance_status()
            .expect("maintenance status")
            .background_worker_count(),
        0
    );
}

#[test]
fn open_cache_helper_returns_open_runtime_and_cache_summary() {
    let outcome = StorageRuntime::open_cache().expect("cache helper open");
    let summary = outcome.summary();
    let runtime = outcome.into_runtime();

    assert_eq!(runtime.state(), StorageRuntimeState::Open);
    assert_eq!(summary.mode(), StorageMode::Cache);
    assert_eq!(summary.disposition(), StorageOpenDisposition::Created);
    assert_eq!(
        summary.maintenance_scheduling_policy(),
        StorageMaintenanceSchedulingPolicy::Background
    );
    assert_eq!(
        runtime
            .maintenance_status()
            .expect("maintenance status")
            .background_worker_count(),
        0
    );
}

#[test]
fn open_cache_ignores_configured_background_worker_count() {
    let configured_workers = 2;
    let outcome = StorageRuntime::open(
        StorageOpenOptions::cache().with_background_worker_count(configured_workers),
    )
    .expect("cache open with configured background worker count");
    let mut runtime = outcome.into_runtime();

    let status = runtime.maintenance_status().expect("maintenance status");
    assert_eq!(status.background_worker_count(), 0);
    assert_eq!(status.background_queue_depth(), 0);
    assert_eq!(status.background_active_tasks(), 0);
    runtime.close().expect("close configured-worker runtime");
}

#[test]
fn cache_load_records_no_source_table_maintenance() {
    let mut runtime = StorageRuntime::open(StorageOpenOptions::cache())
        .expect("cache open should succeed")
        .into_runtime();

    for index in 0..8 {
        let name = format!("cache-load-{index}");
        runtime
            .commit(&background_put_batch(name.as_bytes(), vec![0x42; 64]))
            .expect("cache load commit");
    }

    // Cache never schedules post-commit source-table maintenance and has no
    // background maintenance executor.
    let status = runtime.maintenance_status().expect("maintenance status");
    assert_eq!(status.pending_tasks(), 0);
    assert_eq!(status.enqueued(), 0);
    assert_eq!(status.background_worker_count(), 0);
    assert_eq!(status.background_active_tasks(), 0);
    assert_eq!(status.background_tasks_completed(), 0);
}

#[cfg(feature = "perf-trace")]
#[test]
fn cache_load_records_zero_durable_and_maintenance_counters() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let mut runtime = StorageRuntime::open(StorageOpenOptions::cache())
        .expect("cache open should succeed")
        .into_runtime();

    for batch in 0..6 {
        for index in 0..8 {
            let name = format!("cache-absence-{batch}-{index}");
            runtime
                .commit(&background_put_batch(name.as_bytes(), vec![0x42; 128]))
                .expect("cache load commit");
        }
    }

    let perf = crate::observability::perf_trace::snapshot();

    // WAL is never built or appended.
    assert_eq!(perf.commit_wal_records_built(), 0);
    assert_eq!(perf.commit_wal_appends(), 0);
    assert_eq!(perf.commit_wal_append_bytes(), 0);

    // No checkpoint or WAL-retention/truncation work.
    assert_eq!(perf.lifecycle_checkpoint_executions(), 0);
    assert_eq!(perf.lifecycle_wal_retention_samples(), 0);
    assert_eq!(perf.lifecycle_wal_checkpoint_enqueue_events(), 0);
    assert_eq!(perf.lifecycle_wal_truncation_deleted_segments(), 0);

    // No post-commit source-table maintenance scheduling or background work.
    assert_eq!(perf.lifecycle_post_commit_maintenance_tasks_enqueued(), 0);
    assert_eq!(perf.lifecycle_background_tasks_completed(), 0);

    // No flush, table rewrite, or compaction work — including zero compaction
    // input rows and bytes.
    assert_eq!(perf.lifecycle_flush_drain_operations_completed(), 0);
    assert_eq!(perf.lifecycle_compaction_operations_completed(), 0);
    assert_eq!(perf.lifecycle_compaction_input_rows(), 0);
    assert_eq!(perf.lifecycle_compaction_input_bytes(), 0);
}

#[test]
fn cache_load_exceeds_old_default_budget_without_rejecting() {
    // Review-fix regression guard: cache uses an effectively-unlimited memory
    // budget. A load that exceeds the old default 64 MiB active / 128 MiB frozen
    // caps must complete with every commit succeeding and the runtime open.
    let mut runtime = StorageRuntime::open(StorageOpenOptions::cache())
        .expect("cache open should succeed")
        .into_runtime();

    // ~70 commits of 1 MiB each => ~70 MiB held in one growing active table,
    // past the old 64 MiB active cap that would previously have rejected writes.
    let value = vec![0x5A; 1024 * 1024];
    for index in 0..70 {
        let name = format!("over-budget-{index:04}");
        runtime
            .commit(&background_put_batch(name.as_bytes(), value.clone()))
            .unwrap_or_else(|error| panic!("cache commit {index} must succeed: {error}"));
        assert!(
            runtime.is_open(),
            "runtime must stay open at commit {index}"
        );
    }

    assert_eq!(runtime.state(), StorageRuntimeState::Open);
    let status = runtime.maintenance_status().expect("maintenance status");
    assert_eq!(status.pending_tasks(), 0);
    assert_eq!(status.background_worker_count(), 0);
}

#[cfg(feature = "perf-trace")]
#[test]
fn cache_close_performs_no_durable_finalization_work() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let mut runtime = StorageRuntime::open(StorageOpenOptions::cache())
        .expect("cache open should succeed")
        .into_runtime();

    for index in 0..8 {
        let name = format!("cache-close-load-{index}");
        runtime
            .commit(&background_put_batch(name.as_bytes(), vec![0x42; 64]))
            .expect("cache load commit");
    }
    crate::observability::perf_trace::reset();

    let close = runtime.close().expect("cache close");

    // Close reports no durable sync, and performs no checkpoint, WAL
    // truncation, manifest publication, or source-table drain.
    assert!(!close.durable_synced());
    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.lifecycle_checkpoint_executions(), 0);
    assert_eq!(perf.lifecycle_wal_truncation_deleted_segments(), 0);
    assert_eq!(perf.lifecycle_wal_checkpoint_enqueue_events(), 0);
    assert_eq!(perf.commit_wal_appends(), 0);
    assert_eq!(perf.lifecycle_flush_drain_operations_completed(), 0);
    assert_eq!(perf.lifecycle_compaction_operations_completed(), 0);
    assert_eq!(perf.lifecycle_background_tasks_completed(), 0);
}

#[test]
fn open_cache_can_select_non_background_maintenance_policies_for_tests() {
    for (api_policy, lifecycle_policy, worker_count) in [
        (
            StorageMaintenanceSchedulingPolicy::DeterministicInline,
            crate::lifecycle::LifecycleMaintenanceSchedulingPolicy::Background,
            0,
        ),
        (
            StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue,
            crate::lifecycle::LifecycleMaintenanceSchedulingPolicy::EvaluateAndEnqueue,
            0,
        ),
        (
            StorageMaintenanceSchedulingPolicy::Disabled,
            crate::lifecycle::LifecycleMaintenanceSchedulingPolicy::Disabled,
            0,
        ),
    ] {
        let outcome = StorageRuntime::open(
            StorageOpenOptions::cache().with_maintenance_scheduling_policy(api_policy),
        )
        .expect("cache open should preserve explicit maintenance policy");
        let summary = outcome.summary();
        let runtime = outcome.into_runtime();

        assert_eq!(summary.maintenance_scheduling_policy(), api_policy);
        assert_eq!(
            runtime.maintenance_scheduling_policy_for_test(),
            lifecycle_policy
        );
        assert_eq!(
            runtime
                .maintenance_status()
                .expect("maintenance status")
                .background_worker_count(),
            worker_count
        );
    }
}
