//! `RemoteGet` behavior: the `strata remote` query surface over the
//! engine's recorded clone origin (StrataHub Ask 5's user-visible read).

use strata_engine::artifact::{RemoteOrigin, RemoteOriginFrontierEntry};
use strata_engine::{Database, DurableLocalOpenOptions};
use strata_executor::{Command, Executor, Output};

#[test]
fn remote_get_reads_none_before_any_clone() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    let output = executor
        .execute(Command::RemoteGet {})
        .expect("remote get succeeds");
    let Output::RemoteOriginResult { origin } = output else {
        panic!("unexpected output: {output:?}");
    };
    assert!(origin.is_none());
}

#[test]
fn remote_get_surfaces_the_recorded_origin() {
    let dir = tempfile::tempdir().expect("tempdir");
    {
        let mut db = Database::open_local(dir.path(), DurableLocalOpenOptions::new())
            .expect("db opens")
            .into_database();
        db.set_remote_origin(&RemoteOrigin::new(
            "https://hub.example.com".to_owned(),
            "titanic".to_owned(),
            "default".to_owned(),
            "blake3:9102042536aa000000000000000000000000000000000000000000000000aaaa".to_owned(),
            1_780_000_000_000_000,
            vec![RemoteOriginFrontierEntry::new(
                "default".to_owned(),
                "blake3:1111111111111111111111111111111111111111111111111111111111111111"
                    .to_owned(),
                None,
            )],
        ))
        .expect("origin recorded");
    }

    let mut executor = Executor::open_durable_local(dir.path()).expect("durable executor opens");
    let output = executor
        .execute(Command::RemoteGet {})
        .expect("remote get succeeds");
    let Output::RemoteOriginResult { origin } = output else {
        panic!("unexpected output: {output:?}");
    };
    let origin = origin.expect("origin present");
    assert_eq!(origin.remote_url, "https://hub.example.com");
    assert_eq!(origin.dataset, "titanic");
    assert_eq!(origin.branch, "default");
    assert_eq!(origin.fetched_at_micros, 1_780_000_000_000_000);
    assert_eq!(origin.base_frontier.len(), 1);
    assert_eq!(origin.base_frontier[0].branch, "default");
    assert!(origin.base_frontier[0].local_version.is_none());
}
