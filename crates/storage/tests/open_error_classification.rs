//! Open-time failure classification.
//!
//! Permanent, unrecoverable open conditions must not masquerade as a transient
//! outage with retry advice. A byte-corrupted WAL is corruption (a permanent
//! `FailedPrecondition`), and a structurally invalid database path is an
//! invalid argument — never a retryable lower-layer outage.
#![cfg(feature = "localfs")]

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use strata_core::BranchId;
use strata_storage::api::{
    CommitBatch, CommitMutation, CommitOptions, MaintenanceRequest, MaintenanceScope,
    MaintenanceSummaryStatus, MaintenanceTask, StorageApiErrorClass, StorageDurabilityPolicy,
    StorageKey, StorageRuntime, StorageSpaceId, StorageValue,
};

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

fn temp_root(name: &str) -> PathBuf {
    let mut path = std::env::temp_dir();
    path.push(format!(
        "strata-open-fault-{name}-{}-{}",
        std::process::id(),
        NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed)
    ));
    if path.exists() {
        std::fs::remove_dir_all(&path).expect("clear old temp dir");
    }
    path
}

fn default_branch() -> BranchId {
    BranchId::from_bytes([0x01; BranchId::BYTE_LEN])
}

/// The active WAL segment file, so a test can damage the durable log directly.
fn active_wal_segment(root: &Path) -> PathBuf {
    let wal_dir = root.join("wal");
    let mut best: Option<(String, PathBuf)> = None;
    for entry in std::fs::read_dir(&wal_dir).expect("wal dir").flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(id) = name.strip_suffix(".object@") else {
            continue;
        };
        if id.len() != 16 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            continue;
        }
        let id = id.to_owned();
        if best
            .as_ref()
            .is_none_or(|(best_id, _)| id.as_str() > best_id.as_str())
        {
            best = Some((id, path));
        }
    }
    best.map(|(_, path)| path)
        .expect("an active WAL segment exists")
}

#[test]
fn corrupt_wal_record_reports_permanent_corruption_not_transient_outage() {
    let root = temp_root("corrupt-wal");
    // Write enough acknowledged rows to fill the log with complete interior
    // records, so a mid-file byte flip lands inside a checksummed record body
    // (a hard corruption) rather than at the trailing record (a torn tail).
    {
        let runtime = StorageRuntime::open_durable_local(&root, StorageDurabilityPolicy::Always)
            .expect("durable open")
            .into_runtime();
        let branch = default_branch();
        let space = StorageSpaceId::new(vec![0x20]).expect("engine space");
        for index in 0u32..64 {
            let key = StorageKey::new(format!("corrupt-{index:04}").into_bytes()).expect("key");
            let batch = CommitBatch::new(
                branch,
                vec![CommitMutation::Put {
                    storage_space: space.clone(),
                    key,
                    value: StorageValue::new(vec![b'v'; 64]),
                    ttl: None,
                }],
                CommitOptions::default(),
            )
            .expect("commit batch");
            runtime.commit(&batch).expect("durable commit");
        }
        // Drop (not close) to leave the synced WAL for recovery without a
        // close-time checkpoint truncating it.
    }

    let segment = active_wal_segment(&root);
    let mut bytes = std::fs::read(&segment).expect("read wal segment");
    assert!(bytes.len() > 128, "wal segment too small to corrupt safely");
    // Damage a run inside the first quarter of the log — well past the segment
    // header, well before the trailing record.
    let start = bytes.len() / 4;
    for offset in start..(start + 32).min(bytes.len()) {
        bytes[offset] ^= 0xff;
    }
    std::fs::write(&segment, &bytes).expect("write corrupted wal segment");

    let error = StorageRuntime::open_durable_local(&root, StorageDurabilityPolicy::Always)
        .expect_err("a byte-corrupted WAL must fail to open, not silently recover");

    assert_eq!(
        error.class(),
        StorageApiErrorClass::FailedPrecondition,
        "corrupt WAL must be a permanent precondition failure, not a transient `Internal` outage; got code {}",
        error.code()
    );
    assert_eq!(
        error.code(),
        "failed_precondition.storage_api.recovery_degraded",
        "corrupt WAL must surface the recovery-degraded code so the engine maps it to non-retryable corruption"
    );
}

#[test]
fn opening_a_regular_file_as_a_database_reports_invalid_argument() {
    let root = temp_root("file-as-db");
    std::fs::create_dir_all(root.parent().expect("temp parent")).expect("temp parent dir");
    std::fs::write(&root, b"not a database directory").expect("write regular file");

    let error = StorageRuntime::open_durable_local(&root, StorageDurabilityPolicy::Standard)
        .expect_err("a regular file is not a valid database directory");

    assert_eq!(
        error.class(),
        StorageApiErrorClass::InvalidArgument,
        "a file-as-database path is an invalid argument, not a transient outage; got code {}",
        error.code()
    );
}

#[test]
fn opening_under_a_missing_parent_directory_reports_invalid_argument() {
    let root = temp_root("missing-parent")
        .join("does-not-exist")
        .join("db");

    let error = StorageRuntime::open_durable_local(&root, StorageDurabilityPolicy::Standard)
        .expect_err("a database path under a missing parent cannot be opened");

    assert_eq!(
        error.class(),
        StorageApiErrorClass::InvalidArgument,
        "a missing-parent path is an invalid argument, not a transient outage; got code {}",
        error.code()
    );
}

/// Deletes every WAL segment object under `wal/`, leaving sidecar metadata
/// and every other artifact untouched (the #2765 sabotage).
fn delete_all_wal_segments(root: &Path) {
    let wal_dir = root.join("wal");
    let mut deleted = 0u32;
    for entry in std::fs::read_dir(&wal_dir).expect("wal dir").flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let Some(id) = name.strip_suffix(".object@") else {
            continue;
        };
        if id.len() != 16 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            continue;
        }
        std::fs::remove_file(&path).expect("delete wal segment");
        deleted += 1;
    }
    assert!(deleted > 0, "the stage produced no wal segments to delete");
}

/// Seeds a durable store with acknowledged commits, publishes a checkpoint
/// (so the manifest attests durable history, as every engine-level database
/// does from its creation barrier), and closes cleanly.
fn stage_cleanly_closed_store(root: &Path) {
    let mut runtime = StorageRuntime::open_durable_local(root, StorageDurabilityPolicy::Standard)
        .expect("durable open")
        .into_runtime();
    let branch = default_branch();
    let space = StorageSpaceId::new(vec![0x20]).expect("engine space");
    for index in 0u32..8 {
        let key = StorageKey::new(format!("acked-{index:04}").into_bytes()).expect("key");
        let batch = CommitBatch::new(
            branch,
            vec![CommitMutation::Put {
                storage_space: space.clone(),
                key,
                value: StorageValue::new(vec![b'v'; 64]),
                ttl: None,
            }],
            CommitOptions::default(),
        )
        .expect("commit batch");
        runtime.commit(&batch).expect("durable commit");
    }
    let summary = runtime
        .maintenance(&MaintenanceRequest::new(
            MaintenanceTask::Checkpoint,
            MaintenanceScope::Global,
        ))
        .expect("checkpoint request");
    assert_eq!(
        summary.status(),
        MaintenanceSummaryStatus::Completed,
        "the staging checkpoint must publish so the manifest attests history"
    );
    runtime.close().expect("clean close");
}

/// #2765: an existing database whose manifest attests a published checkpoint
/// must refuse to open when every WAL segment is gone — recreating a fresh
/// empty log silently presents a gutted store as a healthy empty database.
#[test]
fn missing_wal_segments_after_clean_close_report_permanent_corruption() {
    let root = temp_root("missing-wal");
    stage_cleanly_closed_store(&root);
    delete_all_wal_segments(&root);

    let error = StorageRuntime::open_durable_local(&root, StorageDurabilityPolicy::Standard)
        .expect_err("a checkpoint-attested database with no WAL segments must refuse to open");

    assert_eq!(
        error.class(),
        StorageApiErrorClass::FailedPrecondition,
        "missing WAL on an attested database is permanent corruption, not a fresh store; got code {}",
        error.code()
    );
    assert_eq!(
        error.code(),
        "failed_precondition.storage_api.recovery_degraded",
        "missing WAL must surface the recovery-degraded code so the engine maps it to non-retryable corruption"
    );
}

/// #2765, whole-directory variant: removing `wal/` entirely is the same
/// absence and must refuse identically.
#[test]
fn missing_wal_directory_after_clean_close_reports_permanent_corruption() {
    let root = temp_root("missing-wal-dir");
    stage_cleanly_closed_store(&root);
    std::fs::remove_dir_all(root.join("wal")).expect("remove wal dir");

    let error = StorageRuntime::open_durable_local(&root, StorageDurabilityPolicy::Standard)
        .expect_err("a checkpoint-attested database with no wal/ directory must refuse to open");

    assert_eq!(
        error.class(),
        StorageApiErrorClass::FailedPrecondition,
        "missing wal/ on an attested database is permanent corruption; got code {}",
        error.code()
    );
    assert_eq!(
        error.code(),
        "failed_precondition.storage_api.recovery_degraded",
        "the directory variant must classify identically to the segment variant"
    );
}

/// The refusal must not overfire: before any checkpoint publishes (a torn
/// first creation — manifest written, crash before the log or the creation
/// checkpoint landed), `snapshot_watermark` is absent, nothing acknowledged
/// could exist, and reopen must still recreate the log and succeed.
#[test]
fn unattested_store_with_missing_wal_still_opens_fresh() {
    let root = temp_root("torn-creation");
    {
        // Create and drop without close: no checkpoint publishes, so the
        // manifest attests nothing.
        let _runtime = StorageRuntime::open_durable_local(&root, StorageDurabilityPolicy::Standard)
            .expect("durable open")
            .into_runtime();
    }
    delete_all_wal_segments(&root);

    let runtime = StorageRuntime::open_durable_local(&root, StorageDurabilityPolicy::Standard)
        .expect("an unattested store recreates its log")
        .into_runtime();
    drop(runtime);
}
