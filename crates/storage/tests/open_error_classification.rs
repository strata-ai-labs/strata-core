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
    CommitBatch, CommitMutation, CommitOptions, StorageApiErrorClass, StorageDurabilityPolicy,
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
