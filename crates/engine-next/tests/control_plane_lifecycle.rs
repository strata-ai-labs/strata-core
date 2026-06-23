//! Control-plane lifecycle tests: fail-closed behavior and interrupted-operation
//! recovery, driven through the persistence fault seam.
//!
//! These rely on the testkit fault-injection surface, so the whole binary
//! compiles away without the `testkit` feature.
#![cfg(feature = "testkit")]

mod common;

use common::{assert_status, branch, open_cache_database, open_durable_database, space};
use strata_engine_next::testkit::StorageFaultKind;
use strata_engine_next::{AdminHealthStatus, ControlHealthStatus, EngineErrorClass};

/// When a branch operation fails and its pending-marker cleanup also fails, the
/// control plane fails closed: inspection still works and reports the plane as
/// unavailable (admin health is Degraded), but writes are rejected.
#[test]
fn fail_closed_control_plane_degrades_health_and_rejects_work() {
    let mut db = open_cache_database().expect("cache database opens");

    // The storage branch action fails, and the following cleanup commit (the
    // second commit, after the pending marker is written) fails too — which is
    // what forces the control plane closed.
    db.inject_branch_fault_for_test(StorageFaultKind::Unavailable);
    db.inject_commit_fault_after_for_test(1, StorageFaultKind::Unavailable);
    assert!(
        db.branches()
            .expect("branch service opens")
            .create(branch("feature"))
            .is_err(),
        "branch creation should fail"
    );

    // Inspection still works and reports every control area as unavailable.
    let diagnostics = db
        .control_diagnostics(None)
        .expect("diagnostics remain available");
    assert_eq!(
        diagnostics.identity_status(),
        ControlHealthStatus::Unavailable
    );
    assert_eq!(
        diagnostics.registry_status(),
        ControlHealthStatus::Unavailable
    );
    assert_eq!(
        diagnostics.branch_catalog_status(),
        ControlHealthStatus::Unavailable
    );

    // Admin health rolls this up to Degraded — reads and inspection still work.
    assert_eq!(
        db.admin().expect("admin opens").health(None).status,
        AdminHealthStatus::Degraded
    );

    // Further work is rejected with a structured control-plane error.
    let Err(rejected) = db.kv(branch("default"), space("default")) else {
        panic!("the fail-closed control plane must reject further work");
    };
    assert_status(
        &rejected,
        EngineErrorClass::Unavailable,
        "unavailable.engine.control_plane",
        true,
    );
}

/// A branch creation interrupted after its durable pending marker is written
/// leaves that marker behind; reopening the database detects it as corruption
/// rather than silently resuming.
#[cfg(feature = "localfs")]
#[test]
fn interrupted_branch_creation_fails_closed_on_durable_reopen() {
    let dir = tempfile::tempdir().expect("temp dir");
    {
        let mut db = open_durable_database(dir.path()).expect("durable opens");
        db.inject_branch_fault_for_test(StorageFaultKind::Unavailable);
        db.inject_commit_fault_after_for_test(1, StorageFaultKind::Unavailable);
        // The pending marker commits durably; the branch action and cleanup fail.
        assert!(
            db.branches()
                .expect("branch service opens")
                .create(branch("feature"))
                .is_err(),
            "branch creation should fail, leaving a durable pending marker"
        );
        db.close().expect("close succeeds");
    }

    let Err(error) = open_durable_database(dir.path()) else {
        panic!("reopen should reject the interrupted operation");
    };
    assert_status(
        &error,
        EngineErrorClass::Corruption,
        "data_loss.engine.branch_create_pending",
        false,
    );
}
