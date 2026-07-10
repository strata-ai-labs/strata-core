//! Error redaction and retry-policy coverage.
//!
//! Engine errors — including those mapped from storage, which retain the raw
//! storage error as a source — must not leak storage internals or secrets
//! through their whole source chain, and must carry the right retry policy.
//!
//! Uses the testkit fault seam, so the binary compiles away without it.
#![cfg(feature = "testkit")]

mod common;

use common::{
    assert_no_secret_leak, assert_no_storage_leak, branch, key, open_cache_database, space, value,
};
use strata_engine::testkit::StorageFaultKind;
use strata_engine::{BranchName, CacheOpenOptions, Database, EngineError, EngineResult};

fn expect_error<T>(result: EngineResult<T>) -> EngineError {
    match result {
        Ok(_) => panic!("expected an error"),
        Err(error) => error,
    }
}

fn assert_redacted(error: &EngineError) {
    assert_no_storage_leak(error);
    assert_no_secret_leak(error);
}

/// Every storage fault, mapped through a real commit, redacts its internals and
/// carries the documented retry policy while retaining the storage cause.
#[test]
fn storage_mapped_commit_errors_redact_and_carry_retry_policy() {
    for (kind, retryable) in [
        (StorageFaultKind::ResourceExhausted, true),
        (StorageFaultKind::AmbiguousCommit, false),
        (StorageFaultKind::RecoveryDegraded, false),
        (StorageFaultKind::Unavailable, true),
        (StorageFaultKind::NotFound, false),
        (StorageFaultKind::Conflict, true),
    ] {
        let mut db = open_cache_database().expect("cache database opens");
        db.inject_commit_fault_for_test(kind);
        let mut kv = db
            .kv(branch("default"), space("default"))
            .expect("kv opens");
        let error = expect_error(kv.put(key(b"k"), value(b"v")));

        assert_redacted(&error);
        assert_eq!(error.retryable(), retryable, "retry policy for {kind:?}");
        assert!(
            std::error::Error::source(&error).is_some(),
            "the storage cause is retained for diagnosis ({kind:?})"
        );
    }
}

/// Read-path storage errors redact too.
#[test]
fn read_path_storage_error_redacts() {
    let mut db = open_cache_database().expect("cache database opens");
    db.inject_read_fault_for_test(StorageFaultKind::Unavailable);
    let mut kv = db
        .kv(branch("default"), space("default"))
        .expect("kv opens");
    assert_redacted(&expect_error(kv.get(&key(b"missing"))));
}

/// Engine-owned validation, not-found, and open-surface errors redact as well.
#[test]
fn engine_owned_errors_redact() {
    assert_redacted(&expect_error(BranchName::new("_system_")));

    let mut db = open_cache_database().expect("cache database opens");
    assert_redacted(&expect_error(
        db.branches()
            .expect("branch service opens")
            .get(&branch("ghost")),
    ));

    // The open path maps a storage validation error, which must redact too.
    assert_redacted(&expect_error(Database::open_cache(
        CacheOpenOptions::new().with_memory_budget(1),
    )));
}
