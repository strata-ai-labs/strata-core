//! `RemoteTrackingRef` behavior (Ask 5): the clone flow writes the ref as
//! its last step; a fresh open reads it back; sync-shaped overwrites
//! replace it.

use std::path::Path;

use strata_engine::{
    BranchName as EngineBranchName, Database, DurableLocalOpenOptions, KvKey, KvValue, ProductSpace,
};
use strata_hub::{
    import_bundle, read_remote_tracking_ref, write_remote_tracking_ref, EngineExportOptions,
    RemoteTrackingRef, StrataCoreEngine,
};
use stratahub_protocol::{BranchName, DatasetName, Hash};
use time::OffsetDateTime;

fn build_fixture_db(path: &Path) {
    let mut db = Database::open_local(path, DurableLocalOpenOptions::new())
        .expect("fixture opens")
        .into_database();
    let mut kv = db
        .kv(
            EngineBranchName::new("default").expect("branch"),
            ProductSpace::new("default").expect("space"),
        )
        .expect("kv");
    kv.put(
        KvKey::new("user:ada").expect("key"),
        KvValue::new(b"engineer".to_vec()),
    )
    .expect("put");
}

/// The clone flow end-to-end: export → import → write ref → read back.
#[test]
fn clone_writes_the_ref_and_reopen_reads_it_back() {
    let source = tempfile::tempdir().expect("tempdir");
    build_fixture_db(source.path());
    let mut engine = StrataCoreEngine::open(source.path()).expect("open");
    let output = engine
        .export_bundle(&EngineExportOptions::default())
        .expect("export");
    let manifest_hash = stratahub_protocol::hash_bytes(&output.manifest_canonical_bytes);
    let objects: std::collections::HashMap<Hash, Vec<u8>> = output
        .objects
        .iter()
        .map(|object| (object.hash.clone(), object.bytes.clone()))
        .collect();

    let workdir = tempfile::tempdir().expect("workdir");
    let target = workdir.path().join("clone.strata");
    import_bundle(&target, &output.manifest, &objects).expect("import");

    let fetched_at = OffsetDateTime::from_unix_timestamp(1_780_000_000).expect("timestamp");
    let tracking_ref = RemoteTrackingRef::for_clone(
        "https://hub.example.com".to_owned(),
        DatasetName::parse("titanic").expect("dataset"),
        BranchName::parse("default").expect("branch"),
        &output.manifest,
        manifest_hash.clone(),
        fetched_at,
    );
    write_remote_tracking_ref(&target, &tracking_ref).expect("write ref");

    let read_back = read_remote_tracking_ref(&target)
        .expect("read ref")
        .expect("ref recorded");
    assert_eq!(read_back, tracking_ref);
    assert_eq!(read_back.manifest_hash, manifest_hash);

    // Frontier derives from the fetched manifest's branch entries, with
    // local versions unset until sync records them.
    assert_eq!(read_back.base_frontier.len(), 1);
    let (branch, base, local_version) = &read_back.base_frontier[0];
    assert_eq!(branch, "default");
    assert_eq!(base, &output.manifest.branches[0].head_commit);
    assert!(base.starts_with("blake3:"));
    assert!(local_version.is_none());

    // The clone remains fully usable after the ref write.
    let mut db = Database::open_local(&target, DurableLocalOpenOptions::new())
        .expect("clone opens")
        .into_database();
    let mut kv = db
        .kv(
            EngineBranchName::new("default").expect("branch"),
            ProductSpace::new("default").expect("space"),
        )
        .expect("kv");
    assert!(kv
        .get(&KvKey::new("user:ada").expect("key"))
        .expect("get")
        .is_some());
}

#[test]
fn sync_shaped_overwrite_replaces_the_ref() {
    let dir = tempfile::tempdir().expect("tempdir");
    build_fixture_db(dir.path());

    let first = sample_ref(
        1_780_000_000,
        "blake3:1111111111111111111111111111111111111111111111111111111111111111",
    );
    write_remote_tracking_ref(dir.path(), &first).expect("first write");

    let second = sample_ref(
        1_790_000_000,
        "blake3:2222222222222222222222222222222222222222222222222222222222222222",
    );
    write_remote_tracking_ref(dir.path(), &second).expect("overwrite");

    let read_back = read_remote_tracking_ref(dir.path())
        .expect("read")
        .expect("recorded");
    assert_eq!(read_back, second);
    assert_ne!(read_back.fetched_at, first.fetched_at);
}

#[test]
fn databases_without_a_ref_read_none() {
    let dir = tempfile::tempdir().expect("tempdir");
    build_fixture_db(dir.path());
    assert!(read_remote_tracking_ref(dir.path())
        .expect("read")
        .is_none());
}

fn sample_ref(fetched_at_secs: i64, hash: &str) -> RemoteTrackingRef {
    RemoteTrackingRef {
        hub_url: "https://hub.example.com".to_owned(),
        dataset: DatasetName::parse("titanic").expect("dataset"),
        branch: BranchName::parse("default").expect("branch"),
        manifest_hash: Hash::parse(hash).expect("hash"),
        fetched_at: OffsetDateTime::from_unix_timestamp(fetched_at_secs).expect("timestamp"),
        base_frontier: vec![("default".to_owned(), hash.to_owned(), None)],
    }
}
