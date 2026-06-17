//! Control-plane bootstrap and durable reopen tests.

mod common;

use strata_engine_next::{CacheOpenOptions, Database, DatabaseOpenTarget, DurableLocalOpenOptions};

use common::{assert_branch_value, assert_default_branch_exists, branch, key, space, value};

#[test]
fn cache_open_bootstraps_default_branch() {
    let outcome = Database::open_cache(CacheOpenOptions::new()).expect("cache open succeeds");
    assert_eq!(outcome.summary().target(), DatabaseOpenTarget::Cache);
    assert!(outcome.summary().created());
    assert!(!outcome.summary().durable());

    let mut database = outcome.into_database();
    assert_default_branch_exists(&mut database);
    let close = database.close().expect("cache close succeeds");
    assert!(!close.durable());
    assert!(!close.durable_synced());
    assert!(!close.idempotent());

    let close = database.close().expect("second cache close succeeds");
    assert!(!close.durable());
    assert!(!close.durable_synced());
    assert!(close.idempotent());
}

#[cfg(feature = "localfs")]
#[test]
fn durable_open_reopen_preserves_control_plane_and_kv() {
    let temp = tempfile::tempdir().expect("temp dir");
    let path = temp.path().join("db");

    {
        let outcome = Database::open_local(&path, DurableLocalOpenOptions::new())
            .expect("durable open succeeds");
        assert_eq!(outcome.summary().target(), DatabaseOpenTarget::DurableLocal);
        assert!(outcome.summary().created());
        assert!(outcome.summary().durable());

        let mut database = outcome.into_database();
        database
            .kv(branch("default"), space("default"))
            .expect("KV service opens")
            .put(key(b"key-a"), value(b"value-a"))
            .expect("put succeeds");
        database
            .kv(branch("default"), space("default"))
            .expect("KV service opens")
            .put_batch([
                (key(b"batch-a"), value(b"batch-value-a")),
                (key(b"batch-b"), value(b"batch-value-b")),
            ])
            .expect("batch put succeeds");
        database
            .branches()
            .expect("branch service opens")
            .create_from_head(&branch("default"), branch("feature"))
            .expect("branch create succeeds");
        database
            .kv(branch("feature"), space("default"))
            .expect("KV service opens")
            .put(key(b"key-b"), value(b"value-b"))
            .expect("feature put succeeds");
        let close = database.close().expect("close succeeds");
        assert!(close.durable());
        assert!(!close.idempotent());
    }

    {
        let outcome = Database::open_local(&path, DurableLocalOpenOptions::new())
            .expect("durable reopen succeeds");
        assert_eq!(outcome.summary().target(), DatabaseOpenTarget::DurableLocal);
        assert!(!outcome.summary().created());
        assert!(outcome.summary().durable());

        let mut database = outcome.into_database();
        let branches = database.branches().expect("branch service opens").list();
        assert!(branches
            .iter()
            .any(|branch| branch.name().as_str() == "default"));
        assert!(branches
            .iter()
            .any(|branch| branch.name().as_str() == "feature"));
        assert_branch_value(&mut database, "default", "default", b"key-a", b"value-a");
        assert_branch_value(
            &mut database,
            "default",
            "default",
            b"batch-a",
            b"batch-value-a",
        );
        assert_branch_value(
            &mut database,
            "default",
            "default",
            b"batch-b",
            b"batch-value-b",
        );
        assert_branch_value(&mut database, "feature", "default", b"key-b", b"value-b");
    }
}

#[test]
fn system_branch_is_not_listed() {
    let mut database = Database::open_cache(CacheOpenOptions::new())
        .expect("cache open succeeds")
        .into_database();
    let branches = database.branches().expect("branch service opens").list();
    assert!(branches
        .iter()
        .all(|summary| summary.name().as_str() != "_system_"));
}
