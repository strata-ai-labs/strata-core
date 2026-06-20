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
fn cache_over_budget_load_is_refused() {
    // Cache now obeys its memory budget (the Default profile). A sustained load that would grow
    // past the finite pools is refused with a typed resource error instead of growing unbounded,
    // and the runtime stays open.
    let mut runtime = StorageRuntime::open(StorageOpenOptions::cache())
        .expect("cache open should succeed")
        .into_runtime();

    let value = vec![0x5A; 1024 * 1024];
    let mut refused = false;
    for index in 0..300 {
        let name = format!("over-budget-{index:04}");
        match runtime.commit(&background_put_batch(name.as_bytes(), value.clone())) {
            Ok(_) => {}
            Err(error) => {
                assert_eq!(error.code(), "resource_exhausted.storage_api.memory_budget");
                assert_eq!(
                    error.class(),
                    crate::api::StorageApiErrorClass::ResourceExhausted
                );
                refused = true;
                break;
            }
        }
    }

    assert!(
        refused,
        "a sustained cache load must eventually be refused by the budget"
    );
    assert!(
        runtime.is_open(),
        "the runtime stays open after a refused commit"
    );
}

#[test]
fn cache_unlimited_override_allows_large_load() {
    // The named test-only override still lets a cache run unbounded for workloads that need it.
    let mut runtime = StorageRuntime::open(
        StorageOpenOptions::cache()
            .with_storage_budget_for_test(crate::lifecycle::StorageRuntimeBudget::unlimited()),
    )
    .expect("cache open should succeed")
    .into_runtime();

    let value = vec![0x5A; 1024 * 1024];
    for index in 0..70 {
        let name = format!("over-budget-{index:04}");
        runtime
            .commit(&background_put_batch(name.as_bytes(), value.clone()))
            .unwrap_or_else(|error| panic!("unlimited cache commit {index} must succeed: {error}"));
    }
    assert_eq!(runtime.state(), StorageRuntimeState::Open);
}

fn put_on_branch(branch: BranchId, name: &[u8], value: Vec<u8>) -> CommitBatch {
    CommitBatch::new(
        branch,
        vec![CommitMutation::Put {
            storage_space: StorageSpaceId::new(vec![0x20]).expect("engine storage space"),
            key: StorageKey::new(name.to_vec()).expect("valid key"),
            value: StorageValue::new(value),
            ttl: None,
        }],
        CommitOptions::default(),
    )
    .expect("valid put batch")
}

#[test]
fn cache_multi_branch_over_global_budget_is_refused() {
    // The global budget is the whole point of this work: total_bytes sits just above active_mutable,
    // so no single branch can exceed the total through its own active pool — only two branches'
    // combined resident can. This is the case a per-allocation or per-branch limit cannot reach, and
    // it proves the global pre-commit admission path surfaces the same typed resource error at the
    // public surface as the per-pool path.
    let parts = crate::lifecycle::StorageRuntimeBudgetParts {
        block_cache_bytes: 0,
        table_reader_bytes: 1024,
        active_mutable_bytes: 64 * 1024,
        frozen_mutable_bytes: 1024,
        maintenance_queue_bytes: 1024,
        generated_artifact_bytes: 1024,
        manifest_catalog_bytes: 1024,
        total_bytes: 64 * 1024 + 5 * 1024,
        max_open_readers: 4,
        max_frozen_tables: 4,
        max_pending_maintenance_tasks: 4,
    };
    let budget = crate::lifecycle::StorageRuntimeBudget::from_parts(parts).expect("budget");
    let mut runtime =
        StorageRuntime::open(StorageOpenOptions::cache().with_storage_budget_for_test(budget))
            .expect("cache open should succeed")
            .into_runtime();

    let branch_a = StorageRuntime::default_branch_id_for_test();
    let branch_b = branch_id(0x73);
    runtime
        .branch(&BranchRequest::new(
            branch_b,
            BranchAction::Create,
            Some(BranchGeneration::new(1)),
        ))
        .expect("create branch b");

    // Each ~48 KiB commit stays under active_mutable (64 KiB), so neither branch trips its own
    // per-pool admission or rotates; but the two branches' combined resident exceeds the total.
    let value = vec![0x55; 48 * 1024];
    runtime
        .commit(&put_on_branch(branch_a, b"a", value.clone()))
        .expect("branch a commit fits the per-branch and global budget");

    let error = runtime
        .commit(&put_on_branch(branch_b, b"b", value))
        .expect_err("branch b commit exceeds the database memory budget");
    assert_eq!(error.code(), "resource_exhausted.storage_api.memory_budget");
    assert_eq!(
        error.class(),
        crate::api::StorageApiErrorClass::ResourceExhausted
    );
    assert!(
        runtime.is_open(),
        "the runtime stays open after a refused commit"
    );
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
