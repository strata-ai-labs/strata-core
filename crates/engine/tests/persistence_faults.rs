//! Storage-fault injection tests at the engine persistence boundary.
//!
//! These drive real engine operations with a fault armed on the underlying
//! storage call, proving the engine maps each storage failure to the right
//! status through the genuine `map_storage_error` path, and that a failed
//! commit leaves no visible row.
//!
//! The fault-injection surface is only built under the `testkit` feature, so the
//! whole binary compiles away without it.
#![cfg(feature = "testkit")]

mod common;

use common::{assert_status, branch, key, open_cache_database, space, value};
use strata_engine::testkit::StorageFaultKind;
use strata_engine::{CommitOutcomeStatus, EngineErrorClass, ErrorClass, RetryPolicy};

#[test]
fn commit_fault_resource_exhausted_surfaces_retryable_and_writes_nothing() {
    let mut db = open_cache_database().expect("cache database opens");
    db.inject_commit_fault_for_test(StorageFaultKind::ResourceExhausted);
    let mut kv = db
        .kv(branch("default"), space("default"))
        .expect("kv opens");
    let error = kv
        .put(key(b"k"), value(b"v"))
        .expect_err("armed commit fault fails the put");
    assert_status(
        &error,
        EngineErrorClass::Unavailable,
        "resource_exhausted.engine.persistence_budget",
        true,
    );
    assert_eq!(error.public_class(), ErrorClass::ResourceExhausted);
    assert_eq!(error.retry_policy(), RetryPolicy::AfterStateChange);
    assert_eq!(
        error.commit_outcome(),
        CommitOutcomeStatus::DefinitelyNotCommitted
    );
    // The fault fires before the storage mutation, so nothing is persisted; the
    // consumed schedule then lets the follow-up read run normally.
    assert!(
        kv.get(&key(b"k")).expect("read succeeds").is_none(),
        "a failed commit must leave no visible row"
    );
}

#[test]
fn commit_fault_ambiguous_reports_unknown_retry_and_maybe_committed() {
    let mut db = open_cache_database().expect("cache database opens");
    db.inject_commit_fault_for_test(StorageFaultKind::AmbiguousCommit);
    let mut kv = db
        .kv(branch("default"), space("default"))
        .expect("kv opens");
    let error = kv
        .put(key(b"k"), value(b"v"))
        .expect_err("armed ambiguous commit fails the put");
    assert_status(
        &error,
        EngineErrorClass::AmbiguousCommit,
        "ambiguous_commit.engine.persistence",
        false,
    );
    assert_eq!(error.public_class(), ErrorClass::AmbiguousCommit);
    assert_eq!(error.retry_policy(), RetryPolicy::Unknown);
    assert_eq!(error.commit_outcome(), CommitOutcomeStatus::MaybeCommitted);
}

#[test]
fn commit_fault_recovery_degraded_maps_to_corruption() {
    let mut db = open_cache_database().expect("cache database opens");
    db.inject_commit_fault_for_test(StorageFaultKind::RecoveryDegraded);
    let mut kv = db
        .kv(branch("default"), space("default"))
        .expect("kv opens");
    let error = kv
        .put(key(b"k"), value(b"v"))
        .expect_err("armed recovery degradation fails the put");
    assert_status(
        &error,
        EngineErrorClass::Corruption,
        "corruption.engine.persistence_recovery",
        false,
    );
    assert_eq!(error.public_class(), ErrorClass::Corruption);
    assert_eq!(error.retry_policy(), RetryPolicy::Never);
    assert_eq!(error.commit_outcome(), CommitOutcomeStatus::NotApplicable);
}

#[test]
fn read_fault_surfaces_mapped_engine_error() {
    let mut db = open_cache_database().expect("cache database opens");
    {
        let mut kv = db
            .kv(branch("default"), space("default"))
            .expect("kv opens");
        kv.put(key(b"k"), value(b"v")).expect("seed write succeeds");
    }
    db.inject_read_fault_for_test(StorageFaultKind::Unavailable);
    let mut kv = db
        .kv(branch("default"), space("default"))
        .expect("kv opens");
    let error = kv
        .get(&key(b"k"))
        .expect_err("armed read fault fails the get");
    assert_status(
        &error,
        EngineErrorClass::Unavailable,
        "unavailable.engine.persistence",
        true,
    );
    assert_eq!(error.public_class(), ErrorClass::Unavailable);
    assert_eq!(error.retry_policy(), RetryPolicy::SameRequest);
    assert_eq!(error.commit_outcome(), CommitOutcomeStatus::NotApplicable);
}

#[test]
fn scan_fault_surfaces_mapped_engine_error() {
    let mut db = open_cache_database().expect("cache database opens");
    {
        let mut kv = db
            .kv(branch("default"), space("default"))
            .expect("kv opens");
        kv.put(key(b"a"), value(b"1")).expect("seed write succeeds");
    }
    db.inject_scan_fault_for_test(StorageFaultKind::Unavailable);
    let mut kv = db
        .kv(branch("default"), space("default"))
        .expect("kv opens");
    let error = kv.list(None).expect_err("armed scan fault fails the list");
    assert_status(
        &error,
        EngineErrorClass::Unavailable,
        "unavailable.engine.persistence",
        true,
    );
    assert_eq!(error.public_class(), ErrorClass::Unavailable);
    assert_eq!(error.retry_policy(), RetryPolicy::SameRequest);
    assert_eq!(error.commit_outcome(), CommitOutcomeStatus::NotApplicable);
}

#[test]
fn injected_fault_fires_once_then_clears() {
    let mut db = open_cache_database().expect("cache database opens");
    db.inject_commit_fault_for_test(StorageFaultKind::Conflict);
    let mut kv = db
        .kv(branch("default"), space("default"))
        .expect("kv opens");
    let error = kv
        .put(key(b"k"), value(b"first"))
        .expect_err("first put hits the armed fault");
    assert_status(
        &error,
        EngineErrorClass::Conflict,
        "conflict.engine.persistence",
        true,
    );
    assert_eq!(error.public_class(), ErrorClass::Conflict);
    assert_eq!(error.retry_policy(), RetryPolicy::AfterStateChange);
    assert_eq!(
        error.commit_outcome(),
        CommitOutcomeStatus::DefinitelyNotCommitted
    );
    // The fault is consumed once, so the retried write succeeds and persists.
    kv.put(key(b"k"), value(b"second"))
        .expect("second put succeeds after the fault clears");
    assert_eq!(
        kv.get(&key(b"k"))
            .expect("read succeeds")
            .expect("value present")
            .as_bytes(),
        b"second"
    );
}
