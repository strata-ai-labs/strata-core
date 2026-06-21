//! Runtime lifecycle tests for the database handle.

mod common;

use common::{assert_status, branch, open_cache_database, space};
use strata_engine_next::{EngineError, EngineErrorClass};

fn assert_closed(error: Option<EngineError>) {
    let error = error.expect("accessor must reject a closed handle");
    assert_status(
        &error,
        EngineErrorClass::ClosedRuntime,
        "failed_precondition.engine.runtime_closed",
        false,
    );
}

/// Every service accessor — data capabilities, branch, space, admin, and
/// diagnostics alike — rejects a closed handle before doing any work.
#[test]
fn every_accessor_rejects_after_close() {
    let mut db = open_cache_database().expect("cache database opens");
    db.close().expect("close succeeds");

    assert_closed(db.branches().err());
    assert_closed(db.kv(branch("default"), space("default")).err());
    assert_closed(db.json(branch("default"), space("default")).err());
    assert_closed(db.vector(branch("default"), space("default")).err());
    assert_closed(db.event(branch("default"), space("default")).err());
    assert_closed(db.graph(branch("default"), space("default")).err());
    assert_closed(db.spaces(branch("default")).err());
    assert_closed(db.admin().err());
    assert_closed(db.control_diagnostics(None).err());
}

/// Closing twice is idempotent and the second close reports it.
#[test]
fn second_close_is_idempotent() {
    let mut db = open_cache_database().expect("cache database opens");
    let first = db.close().expect("first close succeeds");
    assert!(!first.idempotent());
    let second = db.close().expect("second close succeeds");
    assert!(second.idempotent());
}
