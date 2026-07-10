//! The facade exposes the engine's D4 surface: open, write, read, branch.

use stratadb::{BranchName, CacheOpenOptions, Database, KvKey, KvValue, ProductSpace};

#[test]
fn facade_round_trips_cache_and_durable() {
    let mut db = Database::open_cache(CacheOpenOptions::new())
        .expect("cache open")
        .into_database();
    round_trip(&mut db);

    let dir = tempfile::tempdir().expect("tempdir");
    let mut db = Database::open_local(dir.path(), stratadb::DurableLocalOpenOptions::new())
        .expect("durable open")
        .into_database();
    round_trip(&mut db);
}

fn round_trip(db: &mut Database) {
    let mut kv = db
        .kv(
            BranchName::new("default").expect("branch"),
            ProductSpace::new("default").expect("space"),
        )
        .expect("kv service");
    kv.put(
        KvKey::new("greeting").expect("key"),
        KvValue::new(b"hello".to_vec()),
    )
    .expect("put");
    let value = kv
        .get(&KvKey::new("greeting").expect("key"))
        .expect("get")
        .expect("present");
    assert_eq!(value.as_bytes(), b"hello");
}
