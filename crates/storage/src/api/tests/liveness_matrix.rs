//! STH-6 liveness matrix: every maintenance kind, in every mode, proven to
//! keep up under sustained load with deterministic draining. Per cell the
//! invariants are: commits never fail permanently, the queue drains to zero
//! with no failed tasks, storage never ends blocked on pressure, and every
//! written row reads back. The background-scheduler legs of the liveness
//! story live in `background_scale.rs` (perf-trace gated, nightly); this
//! matrix runs deterministically in the default suite.

use super::*;

/// Every maintenance kind the public queue accepts, cycled per mode.
const ALL_MAINTENANCE_KINDS: [MaintenanceTask; 11] = [
    MaintenanceTask::Checkpoint,
    MaintenanceTask::Flush,
    MaintenanceTask::Compact,
    MaintenanceTask::Materialize,
    MaintenanceTask::Retain,
    MaintenanceTask::SnapshotPruning,
    MaintenanceTask::Reclaim,
    MaintenanceTask::Quarantine,
    MaintenanceTask::Purge,
    MaintenanceTask::Repair,
    MaintenanceTask::WalGrowth,
];

/// Commits per maintenance-kind phase — small keeps the full matrix in CI
/// seconds while still exercising each kind against live write traffic.
const ROWS_PER_PHASE: usize = 16;

fn run_liveness_matrix_cell(runtime: &mut StorageRuntime<'_>, label: &str) {
    let branch = BranchId::from_bytes([0x01; BranchId::BYTE_LEN]);
    let value = vec![0x5C; 128];
    let mut written = 0usize;

    for (phase, task) in ALL_MAINTENANCE_KINDS.iter().enumerate() {
        for index in 0..ROWS_PER_PHASE {
            let key = format!("liveness-{label}-{phase:02}-{index:04}");
            runtime
                .commit(&background_put_batch(key.as_bytes(), value.clone()))
                .unwrap_or_else(|error| {
                    panic!(
                        "liveness cell [{label}] phase {task:?}: commit {written} \
                         failed permanently: {error}"
                    )
                });
            written += 1;
        }
        // Enqueue results are discarded: unsupported task/scope pairs are
        // no-ops by contract; the drain below must succeed and the summary
        // must show no failed task.
        let _ = runtime.enqueue_maintenance(&MaintenanceRequest::new(
            *task,
            MaintenanceScope::Branch(branch),
        ));
        let _ =
            runtime.enqueue_maintenance(&MaintenanceRequest::new(*task, MaintenanceScope::Global));
        let summary = runtime.drain_maintenance().unwrap_or_else(|error| {
            panic!("liveness cell [{label}] phase {task:?}: drain failed: {error:?}")
        });
        for outcome in summary.outcomes() {
            assert_ne!(
                outcome.status(),
                MaintenanceSummaryStatus::Failed,
                "liveness cell [{label}] phase {task:?}: maintenance failed: {outcome:?}"
            );
        }
    }

    let status = runtime
        .maintenance_status()
        .expect("liveness matrix maintenance status");
    assert_eq!(
        status.pending_tasks(),
        0,
        "liveness cell [{label}] left queue debt: {status:?}"
    );
    assert_eq!(
        status.failed(),
        0,
        "liveness cell [{label}] recorded maintenance failures: {status:?}"
    );
    assert_eq!(
        status.queue_full(),
        0,
        "liveness cell [{label}] filled the queue: {status:?}"
    );

    let report = runtime
        .diagnostics(DiagnosticsRequest::new(DiagnosticsScope::Global))
        .expect("liveness matrix diagnostics");
    assert_ne!(
        report.pressure().severity(),
        DiagnosticsStoragePressureSeverity::BlockMutatingAdmission,
        "liveness cell [{label}] ended blocked on pressure: {:?}",
        report.pressure()
    );

    // Every written row must read back — progress, not just survival.
    for phase in 0..ALL_MAINTENANCE_KINDS.len() {
        for index in 0..ROWS_PER_PHASE {
            let key = format!("liveness-{label}-{phase:02}-{index:04}");
            let read = runtime
                .read_point(&PointReadRequest::new(
                    branch,
                    StorageSpaceId::new(vec![0x20]).expect("space"),
                    StorageKey::new(key.clone().into_bytes()).expect("key"),
                    ReadBound::Latest,
                ))
                .expect("liveness matrix read");
            assert!(
                read.row().is_some_and(|row| row.value().is_some()),
                "liveness cell [{label}]: row {key} vanished"
            );
        }
    }
}

#[test]
fn every_maintenance_kind_stays_live_in_cache_mode() {
    let mut runtime = StorageRuntime::open(
        StorageOpenOptions::cache().with_maintenance_scheduling_policy(
            StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue,
        ),
    )
    .expect("cache open")
    .into_runtime();
    run_liveness_matrix_cell(&mut runtime, "cache");
}

#[cfg(feature = "localfs")]
#[test]
fn every_maintenance_kind_stays_live_in_durable_standard_mode() {
    let backend = StorageBackend::local_fs(temp_dir_for_api_test("liveness-matrix-standard"));
    let mut runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Standard)
            .with_maintenance_scheduling_policy(
                StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue,
            ),
        &backend,
    )
    .expect("durable standard open")
    .into_runtime();
    run_liveness_matrix_cell(&mut runtime, "durable-standard");
}

#[cfg(feature = "localfs")]
#[test]
fn every_maintenance_kind_stays_live_in_durable_always_mode() {
    let backend = StorageBackend::local_fs(temp_dir_for_api_test("liveness-matrix-always"));
    let mut runtime = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Always)
            .with_maintenance_scheduling_policy(
                StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue,
            ),
        &backend,
    )
    .expect("durable always open")
    .into_runtime();
    run_liveness_matrix_cell(&mut runtime, "durable-always");
}
