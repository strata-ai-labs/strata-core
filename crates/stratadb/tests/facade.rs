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

/// Every one of the six data services must be reachable and functional
/// through the facade's re-exported names — the re-export regression guard
/// for the crates.io surface: if the glob ever stops carrying a service or
/// its argument types, this stops compiling or passing.
#[test]
fn facade_exposes_all_six_data_services() {
    use stratadb::{
        BranchName, CacheOpenOptions, Database, EventPayload, EventType, GraphName, GraphNodeData,
        GraphNodeId, GraphProperties, JsonDocumentId, JsonPath, JsonValue, KvKey, KvValue,
        ProductSpace, VectorCollectionName, VectorConfig, VectorDistanceMetric, VectorEmbedding,
        VectorKey,
    };

    let mut db = Database::open_cache(CacheOpenOptions::new())
        .expect("cache open")
        .into_database();
    let branch = || BranchName::new("default").expect("branch");
    let space = || ProductSpace::new("default").expect("space");

    db.kv(branch(), space())
        .expect("kv")
        .put(KvKey::new("k").expect("key"), KvValue::new(b"v".to_vec()))
        .expect("kv put");

    db.json(branch(), space())
        .expect("json")
        .set_or_create(
            JsonDocumentId::new("doc-1").expect("doc id"),
            &JsonPath::root(),
            JsonValue::new(serde_json::json!({"n": 1})).expect("json value"),
        )
        .expect("json write");

    db.event(branch(), space())
        .expect("event")
        .append(
            EventType::new("orders").expect("event type"),
            EventPayload::new(serde_json::json!({"n": 1})).expect("payload"),
        )
        .expect("event append");

    let mut vectors = db.vector(branch(), space()).expect("vector");
    let collection = VectorCollectionName::new("emb").expect("collection");
    vectors
        .create_collection(
            collection.clone(),
            VectorConfig::new(4, VectorDistanceMetric::Cosine).expect("config"),
        )
        .expect("collection create");
    vectors
        .upsert(
            collection,
            VectorKey::new("k1").expect("vector key"),
            VectorEmbedding::new([1.0, 0.0, 0.0, 0.0]).expect("embedding"),
            None,
        )
        .expect("vector upsert");

    let mut graph = db.graph(branch(), space()).expect("graph");
    let deps = GraphName::new("deps").expect("graph name");
    graph.create_graph(deps.clone()).expect("graph create");
    graph
        .upsert_node(
            &deps,
            GraphNodeId::new("n1").expect("node id"),
            GraphNodeData::new(
                Some(GraphProperties::new(serde_json::json!({"kind": "svc"})).expect("props")),
                None,
            ),
        )
        .expect("node upsert");

    db.branches().expect("branches");
}

/// The facade's error surface is the engine's: stable codes and classes are
/// reachable under the `stratadb` names.
#[test]
fn facade_errors_carry_stable_codes() {
    use stratadb::{BranchName, CacheOpenOptions, Database, EngineErrorClass, ProductSpace};

    let mut db = Database::open_cache(CacheOpenOptions::new())
        .expect("cache open")
        .into_database();
    let error = db
        .kv(
            BranchName::new("ghost").expect("branch"),
            ProductSpace::new("default").expect("space"),
        )
        .err()
        .expect("missing branch refuses");
    assert_eq!(error.code(), "not_found.engine.branch");
    assert_eq!(error.class(), EngineErrorClass::NotFound);
}
