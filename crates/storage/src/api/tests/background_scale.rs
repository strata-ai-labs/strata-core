use super::*;

#[cfg(feature = "perf-trace")]
#[ignore = "L8G: cache has no background/inline maintenance executor or source-shape admission; durable executor/admission coverage is owned by L8H"]
#[test]
#[allow(
    clippy::too_many_lines,
    reason = "closed-loop liveness test intentionally asserts all bounded-resource invariants"
)]
fn lifecycle_background_closed_loop_scaled_cache_converges_without_public_drain() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let value = vec![0x5A; SCALED_CLOSED_LOOP_CACHE_VALUE_BYTES];
    let runtime = StorageRuntime::open(
        StorageOpenOptions::cache()
            .with_storage_budget_for_test(
                crate::lifecycle::StorageRuntimeBudget::scaled_closed_loop_test_profile(),
            )
            .with_background_max_tasks_per_wake(32)
            .with_background_max_runtime_per_wake(std::time::Duration::from_millis(250)),
    )
    .expect("scaled low-memory background cache open")
    .into_runtime();

    let mut written = 0usize;
    while written < SCALED_CLOSED_LOOP_CACHE_ROWS {
        let end = written
            .saturating_add(SCALED_CLOSED_LOOP_CACHE_BATCH_SIZE)
            .min(SCALED_CLOSED_LOOP_CACHE_ROWS);
        runtime
            .commit(&background_put_batch_range(
                "scaled-liveness-",
                written,
                end,
                &value,
            ))
            .unwrap_or_else(|error| {
                panic!("scaled commit {written}..{end} failed permanently: {error}")
            });
        written = end;
    }

    runtime.wait_background_idle_for_test();

    let status = runtime
        .maintenance_status()
        .expect("scaled liveness maintenance status");
    assert_eq!(
        status.pending_tasks(),
        0,
        "background closed loop left lifecycle queue debt"
    );
    assert_eq!(
        status.failed(),
        0,
        "background closed loop recorded maintenance failures: {status:?}"
    );
    assert_eq!(
        status.queue_full(),
        0,
        "background closed loop filled the lifecycle queue: {status:?}"
    );

    let report = runtime
        .diagnostics(DiagnosticsRequest::new(DiagnosticsScope::Global))
        .expect("scaled liveness diagnostics");
    assert_ne!(
        report.pressure().severity(),
        DiagnosticsStoragePressureSeverity::BlockMutatingAdmission,
        "scaled closed loop left storage in blocking pressure"
    );
    assert!(
        report.source_layout().owned_l0_tables() <= 3,
        "background closed loop left uncompacted L0 pressure: {:?}",
        report.source_layout()
    );
    let max_clearable_nonzero_fanout = report
        .source_layout()
        .owned_nonzero_level_table_counts()
        .iter()
        .filter(|count| count.level() < default_terminal_nonzero_level())
        .map(|count| count.table_count())
        .max()
        .unwrap_or(0);
    assert!(
        max_clearable_nonzero_fanout <= 3,
        "background closed loop left uncompacted clearable nonzero fanout: {:?}",
        report.source_layout()
    );
    match report.pressure().severity() {
        DiagnosticsStoragePressureSeverity::None => {}
        DiagnosticsStoragePressureSeverity::Background | DiagnosticsStoragePressureSeverity::Urgent
            if report.pressure().reason() == DiagnosticsStoragePressureReason::ActiveMutableBytes
                && report.pressure().frozen_tables() == 0
                && report.pressure().pending_maintenance() == 0 => {}
        _ => panic!(
            "background closed loop left clearable storage pressure after maintenance reached a fixed point: {:?}",
            report.pressure()
        ),
    }

    assert_background_closed_loop_reads(
        &runtime,
        "scaled-liveness-",
        SCALED_CLOSED_LOOP_CACHE_ROWS,
        &value,
    );

    let perf = crate::observability::perf_trace::snapshot();
    assert!(perf.lifecycle_background_wake_submitted() > 0);
    assert!(perf.lifecycle_background_tasks_completed() > 0);
    assert_eq!(perf.lifecycle_wal_retention_samples(), 0);
    assert_eq!(perf.lifecycle_wal_checkpoint_enqueue_events(), 0);
    assert_eq!(perf.lifecycle_checkpoint_executions(), 0);
    assert_eq!(perf.lifecycle_wal_truncation_deleted_segments(), 0);
    assert_eq!(perf.lifecycle_write_admission_wait_timeouts(), 0);
    assert_scaled_compaction_amplification_below_gate(
        &perf,
        SCALED_CLOSED_LOOP_CACHE_ROWS as u64,
        (SCALED_CLOSED_LOOP_CACHE_ROWS * SCALED_CLOSED_LOOP_CACHE_VALUE_BYTES) as u64,
        &format!(
            "cache background closed-loop final_layout={:?} final_pressure={:?}",
            report.source_layout(),
            report.pressure()
        ),
    );
}

#[cfg(all(feature = "localfs", feature = "perf-trace"))]
#[test]
#[allow(
    clippy::too_many_lines,
    reason = "closed-loop liveness test intentionally asserts all bounded-resource invariants"
)]
fn lifecycle_background_closed_loop_scaled_durable_bounds_wal_without_public_drain() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let value = vec![0x6B; SCALED_CLOSED_LOOP_DURABLE_VALUE_BYTES];
    let root = temp_dir_for_api_test("scaled-durable-liveness");
    let backend = crate::testkit::leak_static(StorageBackend::local_fs(root.clone()));
    let runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard)
            .with_storage_budget_for_test(
                crate::lifecycle::StorageRuntimeBudget::scaled_closed_loop_test_profile(),
            )
            .with_wal_growth_policy(StorageWalGrowthPolicy::thresholds(8 * 1024, 2, 3))
            .with_wal_segment_size_for_test(1024)
            .with_background_max_tasks_per_wake(64)
            .with_background_max_runtime_per_wake(std::time::Duration::from_millis(250)),
        backend,
    )
    .expect("scaled low-memory durable background open")
    .into_runtime();

    let mut max_retained_bytes = 0_u64;
    let mut max_retained_segments = 0_u64;
    let mut max_segment_files = 0_usize;
    let mut saw_maintenance_enqueue = false;
    for index in 0..SCALED_CLOSED_LOOP_DURABLE_ROWS {
        let key = format!("scaled-durable-liveness-{index:08}");
        runtime
            .commit(&background_put_batch(key.as_bytes(), value.clone()))
            .unwrap_or_else(|error| {
                let report = runtime
                    .diagnostics(DiagnosticsRequest::new(DiagnosticsScope::Global))
                    .expect("scaled durable liveness failure diagnostics");
                let status = runtime
                    .maintenance_status()
                    .expect("scaled durable liveness failure maintenance status");
                let pending_kinds = runtime.pending_lifecycle_maintenance_kinds_for_test();
                let perf = crate::observability::perf_trace::snapshot();
                panic!(
                    "scaled durable commit {index} failed permanently: {error}; status={status:?}; pending_kinds={pending_kinds:?}; layout={:?}; pressure={:?}; checkpoint={:?}; wal={:?}; compactions={}; checkpoints={}; truncations={}; flushes={}; background_completed={}; background_wakes={}; wait_attempts={}; wait_timeouts={}; block_wait_ns={}",
                    report.source_layout(),
                    report.pressure(),
                    report.checkpoint(),
                    report.wal_growth(),
                    perf.lifecycle_compaction_operations_completed(),
                    perf.lifecycle_checkpoint_executions(),
                    perf.lifecycle_wal_truncation_deleted_segments(),
                    perf.lifecycle_flush_drain_operations_completed(),
                    perf.lifecycle_background_tasks_completed(),
                    perf.lifecycle_background_wake_submitted(),
                    perf.lifecycle_write_admission_wait_attempts(),
                    perf.lifecycle_write_admission_wait_timeouts(),
                    perf.lifecycle_write_admission_block_wait_ns()
                )
            });
        let report = runtime
            .diagnostics(DiagnosticsRequest::new(DiagnosticsScope::Global))
            .expect("scaled durable liveness diagnostics during load");
        if let Some(retained_bytes) = report.wal_growth().retained_wal_bytes() {
            max_retained_bytes = max_retained_bytes.max(retained_bytes);
        }
        if let Some(retained_segments) = report.wal_growth().retained_wal_segments() {
            max_retained_segments =
                max_retained_segments.max(u64::try_from(retained_segments).unwrap_or(u64::MAX));
        }
        if let Some(status) = report.wal_growth().last_status() {
            saw_maintenance_enqueue |= status.checkpoint_enqueued();
        }
        max_segment_files = max_segment_files.max(wal_segment_file_count(&root));
    }

    let background_stats = runtime
        .wait_background_idle_until_for_test(std::time::Duration::from_secs(5))
        .expect("durable background executor is present");

    let status = runtime
        .maintenance_status()
        .expect("scaled durable liveness maintenance status");
    let report = runtime
        .diagnostics(DiagnosticsRequest::new(DiagnosticsScope::Global))
        .expect("scaled durable liveness final diagnostics");
    let pending_kinds = runtime.pending_lifecycle_maintenance_kinds_for_test();
    let pending_watermark = runtime.pending_flush_watermark_candidate_for_test();
    assert_eq!(
        status.pending_tasks(),
        0,
        "durable background closed loop left lifecycle queue debt: pending_kinds={pending_kinds:?}, pending_watermark={pending_watermark:?}, background_stats={background_stats:?}, checkpoint={:?}, layout={:?}",
        report.checkpoint(),
        report.source_layout()
    );
    assert_eq!(
        status.failed(),
        0,
        "durable background closed loop recorded maintenance failures: {status:?}"
    );
    assert_eq!(
        status.queue_full(),
        0,
        "durable background closed loop filled the lifecycle queue: {status:?}"
    );
    assert_eq!(report.wal_growth().state(), DiagnosticsFactState::Known);
    assert!(
        saw_maintenance_enqueue,
        "WAL growth never enqueued maintenance"
    );
    assert!(
        report
            .checkpoint()
            .flush_watermark()
            .is_some_and(|watermark| watermark > CommitVersion::ZERO),
        "background flush watermark never advanced"
    );
    assert!(
        max_retained_segments <= 16,
        "retained WAL segments were not bounded during load: {max_retained_segments}"
    );
    assert!(
        max_retained_bytes <= 128 * 1024,
        "retained WAL bytes were not bounded during load: {max_retained_bytes}"
    );
    assert!(
        max_segment_files <= 16,
        "local WAL segment files were not bounded during load: {max_segment_files}"
    );
    let final_segment_files = wal_segment_file_count(&root);
    assert!(
        final_segment_files <= 4,
        "background WAL retention did not converge covered segments: final_segment_files={final_segment_files}"
    );
    assert_ne!(
        report.pressure().severity(),
        DiagnosticsStoragePressureSeverity::BlockMutatingAdmission,
        "durable closed loop left storage in blocking pressure"
    );
    assert!(
        report.source_layout().owned_l0_tables() <= 3,
        "durable background closed loop left uncompacted L0 pressure: {:?}; maintenance status: {:?}",
        report.source_layout(),
        status
    );
    let max_clearable_nonzero_fanout = report
        .source_layout()
        .owned_nonzero_level_table_counts()
        .iter()
        .filter(|count| count.level() < default_terminal_nonzero_level())
        .map(|count| count.table_count())
        .max()
        .unwrap_or(0);
    let perf = crate::observability::perf_trace::snapshot();
    assert!(
        max_clearable_nonzero_fanout <= 3,
        "durable background closed loop left uncompacted clearable nonzero fanout: {:?}; pressure={:?}; maintenance status={:?}; pending_kinds={pending_kinds:?}; background_stats={background_stats:?}; compactions={}, compaction_inputs={}, compaction_outputs={}, output_tables_built={}",
        report.source_layout(),
        report.pressure(),
        status,
        perf.lifecycle_compaction_operations_completed(),
        perf.lifecycle_compaction_input_tables(),
        perf.lifecycle_compaction_output_tables(),
        perf.table_compaction_output_tables_built()
    );
    assert_eq!(
        report.pressure().severity(),
        DiagnosticsStoragePressureSeverity::None,
        "durable background closed loop left storage pressure after maintenance reached a fixed point: {:?}",
        report.pressure()
    );
    assert_background_closed_loop_reads(
        &runtime,
        "scaled-durable-liveness-",
        SCALED_CLOSED_LOOP_DURABLE_ROWS,
        &value,
    );
    assert!(perf.lifecycle_background_wake_submitted() > 0);
    assert!(perf.lifecycle_background_tasks_completed() > 0);
    assert_eq!(perf.lifecycle_write_admission_wait_timeouts(), 0);
}

#[cfg(all(feature = "localfs", feature = "perf-trace"))]
#[test]
fn wal_retention_deletes_segments_without_public_drain() {
    let _capture = crate::observability::perf_trace::begin_test_capture();
    let root = temp_dir_for_api_test("wal-retention-deletes-segments");
    let backend = crate::testkit::leak_static(StorageBackend::local_fs(root.clone()));
    let runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard)
            .with_wal_growth_policy(StorageWalGrowthPolicy::thresholds(2 * 1024, 2, 3))
            .with_wal_segment_size_for_test(1024)
            .with_background_max_tasks_per_wake(64)
            .with_background_max_runtime_per_wake(std::time::Duration::from_millis(250)),
        backend,
    )
    .expect("durable WAL retention open")
    .into_runtime();

    let mut max_segment_files = wal_segment_file_count(&root);
    let mut last_segment_files = max_segment_files;
    let mut saw_segment_file_deletion = false;
    let mut saw_maintenance_enqueue = false;
    for index in 0..160 {
        let key = format!("wal-retention-{index:04}");
        runtime
            .commit(&background_put_batch(key.as_bytes(), vec![0x73; 256]))
            .unwrap_or_else(|error| panic!("WAL retention commit {index} failed: {error}"));
        let report = runtime
            .diagnostics(DiagnosticsRequest::new(DiagnosticsScope::Global))
            .expect("WAL retention diagnostics during load");
        if let Some(status) = report.wal_growth().last_status() {
            saw_maintenance_enqueue |= status.checkpoint_enqueued();
        }
        let segment_files = wal_segment_file_count(&root);
        saw_segment_file_deletion |= segment_files < last_segment_files;
        last_segment_files = segment_files;
        max_segment_files = max_segment_files.max(segment_files);
    }

    runtime.wait_background_idle_for_test();
    let final_segment_files = wal_segment_file_count(&root);
    saw_segment_file_deletion |= final_segment_files < last_segment_files;

    assert!(
        saw_maintenance_enqueue,
        "WAL retention never enqueued maintenance"
    );
    assert!(
        saw_segment_file_deletion,
        "background WAL retention did not delete covered segment files: max_segment_files={max_segment_files} final_segment_files={final_segment_files}"
    );
    assert!(
        final_segment_files <= 4,
        "background WAL retention did not converge covered segments: final_segment_files={final_segment_files}"
    );
    let status = runtime
        .maintenance_status()
        .expect("WAL retention maintenance status");
    assert_eq!(
        status.pending_tasks(),
        0,
        "pending kinds: {:?}",
        runtime.pending_lifecycle_maintenance_kinds_for_test()
    );
    assert_eq!(
        status.failed(),
        0,
        "WAL retention maintenance failed: {status:?}"
    );
    let report = runtime
        .diagnostics(DiagnosticsRequest::new(DiagnosticsScope::Global))
        .expect("WAL retention final diagnostics");
    assert!(
        report
            .checkpoint()
            .flush_watermark()
            .is_some_and(|watermark| watermark > CommitVersion::ZERO),
        "background flush watermark never advanced"
    );

    let perf = crate::observability::perf_trace::snapshot();
    assert!(perf.lifecycle_background_wake_submitted() > 0);
    assert!(perf.lifecycle_background_tasks_completed() > 0);
    assert!(perf.lifecycle_wal_retention_samples() > 0);
    assert!(perf.lifecycle_wal_checkpoint_enqueue_events() > 0);
    assert!(perf.lifecycle_checkpoint_executions() > 0);
    assert!(perf.lifecycle_wal_truncation_deleted_segments() > 0);
}

#[cfg(feature = "perf-trace")]
#[ignore = "L8G: cache has no background/inline maintenance executor or source-shape admission; durable executor/admission coverage is owned by L8H"]
#[test]
fn background_runtime_creation_perf_trace_records_only_background_opens() {
    let _capture = crate::observability::perf_trace::begin_test_capture();

    let background = StorageRuntime::open_cache().expect("background cache open");
    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.lifecycle_background_runtimes_created(), 1);
    assert_eq!(
        perf.lifecycle_background_runtime_workers_created(),
        default_background_worker_count() as u64
    );

    let enqueue = StorageRuntime::open(
        StorageOpenOptions::cache().with_maintenance_scheduling_policy(
            StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue,
        ),
    )
    .expect("explicit enqueue cache open");
    let perf = crate::observability::perf_trace::snapshot();
    assert_eq!(perf.lifecycle_background_runtimes_created(), 1);
    assert_eq!(
        perf.lifecycle_background_runtime_workers_created(),
        default_background_worker_count() as u64
    );

    drop(enqueue);
    drop(background);
}

#[ignore = "L8G: cache has no background/inline maintenance executor or source-shape admission; durable executor/admission coverage is owned by L8H"]
#[test]
fn background_close_drains_queued_work_before_lifecycle_close() {
    let runtime = StorageRuntime::open_cache()
        .expect("background cache open")
        .into_runtime();

    assert_background_close_drains_queued_work_before_lifecycle_close(runtime);
}

#[cfg(feature = "localfs")]
#[test]
fn durable_background_close_drains_queued_work_before_lifecycle_close() {
    let runtime = StorageRuntime::open_local(temp_dir_for_api_test("durable-background-close"))
        .expect("durable background open")
        .into_runtime();

    assert_background_close_drains_queued_work_before_lifecycle_close(runtime);
}

fn assert_background_close_drains_queued_work_before_lifecycle_close(
    mut runtime: StorageRuntime<'static>,
) {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Barrier};
    use std::time::{Duration, Instant};

    let shutdown_requested = runtime
        .background_shutdown_requested_flag_for_test()
        .expect("background runtime should expose shutdown flag");
    let ready = Arc::new(Barrier::new(2));
    let release = Arc::new(Barrier::new(2));
    let observed_open = Arc::new(AtomicBool::new(false));
    let close_returned = Arc::new(AtomicBool::new(false));

    assert!(
        runtime.submit_runtime_state_background_probe_for_test(
            Arc::clone(&ready),
            Arc::clone(&release),
            Arc::clone(&observed_open),
        ),
        "background runtime should accept the probe task"
    );
    ready.wait();

    std::thread::scope(|scope| {
        let close_returned_worker = Arc::clone(&close_returned);
        let runtime_ref = &mut runtime;
        let close_handle = scope.spawn(move || {
            let summary = runtime_ref.close().expect("close background runtime");
            close_returned_worker.store(true, Ordering::Release);
            summary
        });

        let deadline = Instant::now() + Duration::from_secs(5);
        while !shutdown_requested.load(Ordering::Acquire) {
            assert!(
                Instant::now() < deadline,
                "background close did not request scheduler shutdown"
            );
            std::thread::yield_now();
        }
        assert!(
            !close_returned.load(Ordering::Acquire),
            "close returned before the accepted background task was released"
        );

        release.wait();
        let summary = close_handle.join().expect("join close thread");
        assert_eq!(summary.state(), StorageRuntimeState::Closed);
    });

    assert!(
        observed_open.load(Ordering::Acquire),
        "queued background task must run before lifecycle close transitions the runtime"
    );
    assert_eq!(runtime.state(), StorageRuntimeState::Closed);
}
