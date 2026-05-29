//! Engine-facing API conformance harness entry point.

#![deny(unsafe_code)]

mod common;

#[cfg(feature = "localfs")]
use std::path::PathBuf;
#[cfg(feature = "localfs")]
use std::sync::atomic::{AtomicU64, Ordering};

use strata_storage_next::api::{
    BranchAction, BranchGeneration, BranchId, BranchOperation, BranchRequest, CommitBatch,
    CommitMutation, CommitOptions, MaintenanceRequest, MaintenanceScope, MaintenanceSummaryStatus,
    MaintenanceTask, PointReadRequest, ReadBound, StorageApiErrorClass, StorageKey, StorageMode,
    StorageOpenOptions, StorageRuntime, StorageRuntimeState, StorageSpaceId, StorageValue,
};

#[cfg(feature = "localfs")]
use strata_storage_next::api::{StorageBackend, StorageDurabilityPolicy, StorageOpenDisposition};

#[cfg(feature = "testkit")]
use strata_storage_next::testkit::TestBackendKind;

#[cfg(feature = "localfs")]
static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(1);

#[test]
#[cfg(feature = "testkit")]
fn api_conformance_harness_can_use_test_backend_selection() {
    let backend = TestBackendKind::parse("memory").expect("memory backend should be supported");

    assert_eq!(backend.name(), "memory");
    assert!(common::crate_root().join("src/api/mod.rs").is_file());
}

#[test]
fn api_conformance_cache_open_close_round_trip() {
    let open = StorageRuntime::open(StorageOpenOptions::cache()).expect("cache open");
    let summary = open.summary();
    let mut runtime = open.into_runtime();

    assert_eq!(summary.mode(), StorageMode::Cache);
    assert_eq!(runtime.state(), StorageRuntimeState::Open);

    let close = runtime.close().expect("cache close");
    assert_eq!(close.state(), StorageRuntimeState::Closed);
    assert!(!close.durable_synced());
}

#[test]
#[cfg(feature = "localfs")]
fn api_conformance_durable_open_close_round_trip() {
    let backend = StorageBackend::local_fs(temp_dir("api-conformance-durable"));
    let open = StorageRuntime::open_with_backend(
        StorageOpenOptions::durable_local(StorageDurabilityPolicy::Always),
        &backend,
    )
    .expect("durable open");
    let summary = open.summary();
    let mut runtime = open.into_runtime();

    assert_eq!(
        summary.mode(),
        StorageMode::DurableLocal {
            policy: StorageDurabilityPolicy::Always,
        }
    );
    assert_eq!(summary.disposition(), StorageOpenDisposition::Created);
    assert!(summary.has_durable_recovery_facts());

    let close = runtime.close().expect("durable close");
    assert_eq!(close.state(), StorageRuntimeState::Closed);
    assert!(close.durable_synced());
}

#[test]
fn api_conformance_unsupported_modes_fail_before_runtime_construction() {
    for options in [
        StorageOpenOptions::object_durable_candidate(),
        StorageOpenOptions::distributed_candidate(),
    ] {
        let error = options.validate().expect_err("unsupported mode");
        assert_eq!(error.class(), StorageApiErrorClass::Unsupported);
    }
}

#[test]
fn api_conformance_closed_runtime_rejects_operations() {
    let mut runtime = StorageRuntime::closed();
    let close = runtime.close().expect("closed runtime close is idempotent");
    assert!(close.idempotent());
    assert!(!close.commits_quiesced());
    assert!(!close.maintenance_drained());
    assert!(!close.durable_synced());
    assert!(!close.guards_released());

    let error = runtime
        .require_open("commit requires an open storage runtime")
        .expect_err("closed runtime rejects operation");
    assert_eq!(error.code(), "failed_precondition.storage_api.state");
}

#[test]
fn api_conformance_commit_then_read_round_trip() {
    let mut runtime = StorageRuntime::open(StorageOpenOptions::cache())
        .expect("cache open")
        .into_runtime();
    let branch = BranchId::from_bytes([0x01; BranchId::BYTE_LEN]);
    let space = StorageSpaceId::new(vec![0x20]).expect("engine space");
    let key = StorageKey::new(b"conformance".to_vec()).expect("key");
    let batch = CommitBatch::new(
        branch,
        vec![CommitMutation::Put {
            storage_space: space.clone(),
            key: key.clone(),
            value: StorageValue::new(b"value".to_vec()),
            ttl: None,
        }],
        strata_storage_next::api::CommitOptions::default(),
    )
    .expect("commit batch");

    let commit = runtime.commit(&batch).expect("commit");
    let read = runtime
        .read_point(&PointReadRequest::new(
            branch,
            space,
            key,
            ReadBound::Latest,
        ))
        .expect("read");

    assert_eq!(
        read.row().expect("row").commit_version(),
        commit.commit_version()
    );
}

#[test]
fn api_conformance_branch_lifecycle_round_trip() {
    let mut runtime = StorageRuntime::open(StorageOpenOptions::cache())
        .expect("cache open")
        .into_runtime();
    let parent = BranchId::from_bytes([0x01; BranchId::BYTE_LEN]);
    let branch = BranchId::from_bytes([0x31; BranchId::BYTE_LEN]);
    let sibling = BranchId::from_bytes([0x32; BranchId::BYTE_LEN]);
    let space = StorageSpaceId::new(vec![0x20]).expect("engine space");
    let key = StorageKey::new(b"branch-conformance".to_vec()).expect("key");
    let value = StorageValue::new(b"parent-value".to_vec());

    let created = runtime
        .branch(&BranchRequest::new(
            sibling,
            BranchAction::Create,
            Some(BranchGeneration::new(1)),
        ))
        .expect("create branch");
    assert_eq!(created.operation(), BranchOperation::Created);
    assert_eq!(
        created.branch().expect("created branch").generation(),
        BranchGeneration::new(1)
    );

    runtime
        .commit(
            &CommitBatch::new(
                parent,
                vec![CommitMutation::Put {
                    storage_space: space.clone(),
                    key: key.clone(),
                    value,
                    ttl: None,
                }],
                CommitOptions::default(),
            )
            .expect("commit batch"),
        )
        .expect("commit parent");

    let forked = runtime
        .branch(&BranchRequest::new(
            branch,
            BranchAction::ForkCurrent { source: parent },
            Some(BranchGeneration::new(1)),
        ))
        .expect("fork branch");
    assert_eq!(forked.operation(), BranchOperation::Forked);
    assert_eq!(
        read_conformance_value(&runtime, branch, space.clone(), key.clone()),
        Some(b"parent-value".to_vec())
    );

    let cleared = runtime
        .branch(&BranchRequest::new(
            branch,
            BranchAction::Clear,
            Some(BranchGeneration::new(1)),
        ))
        .expect("clear branch");
    assert_eq!(cleared.operation(), BranchOperation::Cleared);
    assert_eq!(
        read_conformance_value(&runtime, branch, space.clone(), key.clone()),
        None
    );

    let deleted = runtime
        .branch(&BranchRequest::new(
            branch,
            BranchAction::Delete,
            Some(BranchGeneration::new(1)),
        ))
        .expect("delete branch");
    assert_eq!(deleted.operation(), BranchOperation::Deleted);
    let listed = runtime
        .branch(&BranchRequest::new(parent, BranchAction::List, None))
        .expect("list branches");
    assert!(!listed
        .branches()
        .iter()
        .any(|summary| summary.branch_id() == branch));

    let recreated = runtime
        .branch(&BranchRequest::new(
            branch,
            BranchAction::Create,
            Some(BranchGeneration::new(2)),
        ))
        .expect("recreate branch");
    assert_eq!(recreated.operation(), BranchOperation::Created);
    assert_eq!(
        recreated.generation_before(),
        Some(BranchGeneration::new(1))
    );
    assert_eq!(recreated.generation_after(), Some(BranchGeneration::new(2)));
}

#[test]
fn api_conformance_maintenance_flush_and_wal_growth_round_trip() {
    let mut runtime = StorageRuntime::open(StorageOpenOptions::cache())
        .expect("cache open")
        .into_runtime();
    let branch = BranchId::from_bytes([0x01; BranchId::BYTE_LEN]);
    let space = StorageSpaceId::new(vec![0x20]).expect("engine space");
    let key = StorageKey::new(b"maintenance-conformance".to_vec()).expect("key");
    runtime
        .commit(
            &CommitBatch::new(
                branch,
                vec![CommitMutation::Put {
                    storage_space: space,
                    key,
                    value: StorageValue::new(b"value".to_vec()),
                    ttl: None,
                }],
                CommitOptions::default(),
            )
            .expect("commit batch"),
        )
        .expect("commit");

    let flush = runtime
        .maintenance(&MaintenanceRequest::new(
            MaintenanceTask::Flush,
            MaintenanceScope::Branch(branch),
        ))
        .expect("flush maintenance");
    assert_eq!(flush.status(), MaintenanceSummaryStatus::Completed);
    assert!(flush.rows_processed() > 0);
    assert!(!flush.wal_truncated());

    let growth = runtime
        .maintenance(&MaintenanceRequest::new(
            MaintenanceTask::WalGrowth,
            MaintenanceScope::Global,
        ))
        .expect("WAL growth maintenance");
    assert_eq!(growth.status(), MaintenanceSummaryStatus::Completed);
    assert!(growth.wal_growth().is_some());
}

fn read_conformance_value(
    runtime: &StorageRuntime<'_>,
    branch: BranchId,
    space: StorageSpaceId,
    key: StorageKey,
) -> Option<Vec<u8>> {
    runtime
        .read_point(&PointReadRequest::new(
            branch,
            space,
            key,
            ReadBound::Latest,
        ))
        .expect("read branch")
        .row()
        .and_then(|row| row.value().map(|value| value.as_bytes().to_vec()))
}

#[cfg(feature = "localfs")]
fn temp_dir(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "strata-storage-{name}-{}-{}",
        std::process::id(),
        NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
    ));
    if path.exists() {
        std::fs::remove_dir_all(&path).expect("clear old temp dir");
    }
    path
}
