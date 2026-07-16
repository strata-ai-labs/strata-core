//! Write-ordering watchdog (STH-3b): the `SQLite` "db write before journal
//! sync" bug class. A pure-observer backend decorator asserts no manifest,
//! snapshot, or table publish becomes durably visible while a WAL segment
//! holds unsynced appended bytes. Violation-detection non-vacuity and the
//! per-durability stream checks live in the in-crate unit tests; this entry
//! proves the public harness surface end to end.

#![deny(unsafe_code)]

#[cfg(all(
    feature = "fault-injection",
    feature = "localfs",
    not(target_arch = "wasm32")
))]
mod harness {
    use strata_storage::api::{
        CommitBatch, CommitMutation, CommitOptions, MaintenanceRequest, MaintenanceScope,
        MaintenanceTask, StorageBackend, StorageDurabilityPolicy, StorageKey,
        StorageMaintenanceSchedulingPolicy, StorageOpenOptions, StorageRuntime, StorageSpaceId,
        StorageValue,
    };

    pub(super) fn run_watched_session(durability: StorageDurabilityPolicy) {
        let dir = tempfile::tempdir().expect("temp write-ordering root");
        let backend = StorageBackend::write_ordering_local_fs(dir.path());
        let branch = strata_core::BranchId::from_bytes([0x01; strata_core::BranchId::BYTE_LEN]);
        {
            let mut runtime = StorageRuntime::open_with_backend(
                StorageOpenOptions::durable_local(durability).with_maintenance_scheduling_policy(
                    StorageMaintenanceSchedulingPolicy::EvaluateAndEnqueue,
                ),
                &backend,
            )
            .expect("open")
            .into_runtime();
            let space = StorageSpaceId::new(vec![0x20]).expect("space");
            for index in 0u8..32 {
                let batch = CommitBatch::new(
                    branch,
                    vec![CommitMutation::Put {
                        storage_space: space.clone(),
                        key: StorageKey::new(vec![0x77, index]).expect("key"),
                        value: StorageValue::new(vec![0x5A; 256]),
                        ttl: None,
                    }],
                    CommitOptions::default(),
                )
                .expect("batch");
                runtime.commit(&batch).expect("commit");
            }
            for task in [
                MaintenanceTask::Flush,
                MaintenanceTask::Checkpoint,
                MaintenanceTask::Compact,
                MaintenanceTask::SnapshotPruning,
            ] {
                // Enqueue results are discarded: unsupported scopes are
                // no-ops; the drain runs whatever was accepted.
                let _ = runtime.enqueue_maintenance(&MaintenanceRequest::new(
                    task,
                    MaintenanceScope::Branch(branch),
                ));
                let _ = runtime
                    .enqueue_maintenance(&MaintenanceRequest::new(task, MaintenanceScope::Global));
                runtime.drain_maintenance().expect("drain");
            }
        }
        let report = backend
            .write_ordering_report()
            .expect("watchdog backend reports");
        assert!(
            report.violations().is_empty(),
            "write-ordering violations under {durability:?}: {:?}",
            report.violations()
        );
        assert!(report.wal_segment_appends() > 0, "no WAL appends observed");
        assert!(report.publishes_checked() > 0, "no publishes checked");
        assert!(
            report.wal_durability_events() > 0,
            "no WAL durability events observed"
        );
    }
}

#[cfg(all(
    feature = "fault-injection",
    feature = "localfs",
    not(target_arch = "wasm32")
))]
#[test]
fn no_dependent_publish_precedes_its_wal_sync_under_always_durability() {
    harness::run_watched_session(strata_storage::api::StorageDurabilityPolicy::Always);
}

#[cfg(all(
    feature = "fault-injection",
    feature = "localfs",
    not(target_arch = "wasm32")
))]
#[test]
fn no_dependent_publish_precedes_its_wal_sync_under_standard_durability() {
    harness::run_watched_session(strata_storage::api::StorageDurabilityPolicy::Standard);
}
