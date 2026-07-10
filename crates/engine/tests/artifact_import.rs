//! Branch artifact import behavior (slice `HB6b`).
//!
//! The load-bearing proof: export → import into a fresh database →
//! re-export is byte-identical, events and all — the property StrataHub's
//! round-trip conformance (Ask 4) is built on.

use serde_json::json;
use strata_engine::artifact::BranchArtifact;
use strata_engine::{
    BranchName, CacheOpenOptions, Database, EventPayload, EventType, GraphEdgeData, GraphEdgeType,
    GraphName, GraphNodeData, GraphNodeId, GraphObjectTypeDef, GraphPropertyDef, GraphTypeName,
    JsonDocumentId, JsonPath, JsonValue, KvKey, KvValue, ProductSpace, VectorCollectionName,
    VectorConfig, VectorDistanceMetric, VectorEmbedding, VectorKey, VectorMetadata,
};

fn branch() -> BranchName {
    BranchName::new("default").expect("branch")
}

fn space(name: &str) -> ProductSpace {
    ProductSpace::new(name).expect("space")
}

fn fresh_db() -> Database {
    Database::open_cache(CacheOpenOptions::new())
        .expect("cache open")
        .into_database()
}

/// Every data model, two spaces, ontology, and events — the full surface.
#[allow(clippy::too_many_lines)]
fn populate(db: &mut Database) {
    let mut kv = db.kv(branch(), space("default")).expect("kv");
    kv.put_batch([
        (
            KvKey::new("user:ada").expect("key"),
            KvValue::new(b"engineer".to_vec()),
        ),
        (
            KvKey::new("user:lin").expect("key"),
            KvValue::new(b"designer".to_vec()),
        ),
    ])
    .expect("kv batch");

    let mut json = db.json(branch(), space("default")).expect("json");
    json.set_or_create(
        JsonDocumentId::new("config").expect("id"),
        &JsonPath::root(),
        JsonValue::new(json!({"model": "claude", "k": 5})).expect("value"),
    )
    .expect("json");

    let mut events = db.event(branch(), space("default")).expect("events");
    for tool in ["search", "fetch"] {
        events
            .append(
                EventType::new("tool_call").expect("type"),
                EventPayload::new(json!({ "tool": tool })).expect("payload"),
            )
            .expect("append");
    }

    let mut vectors = db.vector(branch(), space("default")).expect("vectors");
    let collection = VectorCollectionName::new("embeddings").expect("name");
    vectors
        .create_collection(
            collection.clone(),
            VectorConfig::new(4, VectorDistanceMetric::Cosine).expect("config"),
        )
        .expect("collection");
    vectors
        .upsert(
            collection,
            VectorKey::new("doc1").expect("key"),
            VectorEmbedding::new(vec![0.1, 0.2, 0.3, 0.4]).expect("embedding"),
            Some(VectorMetadata::new(json!({"title": "intro"})).expect("metadata")),
        )
        .expect("upsert");

    // Second space with its own kv row.
    db.spaces(branch())
        .expect("spaces")
        .create(space("tenant_a"))
        .expect("create space");
    let mut kv_b = db.kv(branch(), space("tenant_a")).expect("kv b");
    kv_b.put(
        KvKey::new("plan").expect("key"),
        KvValue::new(b"step-1".to_vec()),
    )
    .expect("put");

    // Graph with a frozen ontology.
    let mut graph = db.graph(branch(), space("default")).expect("graph");
    let name = GraphName::new("social").expect("name");
    graph.create_graph(name.clone()).expect("create");
    graph
        .define_object_type(
            &name,
            GraphObjectTypeDef::new(
                GraphTypeName::new("person").expect("type name"),
                [(
                    "role".to_owned(),
                    GraphPropertyDef::new(Some("string".to_owned()), false).expect("prop"),
                )],
            )
            .expect("def"),
        )
        .expect("define");
    graph
        .define_link_type(
            &name,
            strata_engine::GraphLinkTypeDef::new(
                GraphTypeName::new("knows").expect("link name"),
                GraphTypeName::new("person").expect("source"),
                GraphTypeName::new("person").expect("target"),
                None,
                [],
            )
            .expect("link def"),
        )
        .expect("define link");
    graph.freeze_ontology(&name).expect("freeze");
    for node in ["ada", "lin"] {
        graph
            .upsert_node(
                &name,
                GraphNodeId::new(node).expect("id"),
                GraphNodeData::new(None, None),
            )
            .expect("node");
    }
    graph
        .upsert_edge(
            &name,
            GraphNodeId::new("ada").expect("src"),
            GraphEdgeType::new("knows").expect("type"),
            GraphNodeId::new("lin").expect("dst"),
            GraphEdgeData::new(2.5, None).expect("edge"),
        )
        .expect("edge");
}

fn export(db: &mut Database) -> BranchArtifact {
    db.export_branch_artifact(&branch()).expect("export")
}

#[test]
fn round_trip_reproduces_the_artifact_byte_for_byte() {
    let mut source = fresh_db();
    populate(&mut source);
    let original = export(&mut source);

    let mut target = fresh_db();
    let summary = target
        .import_branch_artifact(&original)
        .expect("import succeeds");
    assert_eq!(summary.sections(), original.sections().len());

    let reexported = export(&mut target);
    assert_eq!(
        original, reexported,
        "export → import → export must be byte-identical"
    );
}

#[test]
fn imported_content_is_queryable_through_normal_reads() {
    let mut source = fresh_db();
    populate(&mut source);
    let artifact = export(&mut source);

    let mut target = fresh_db();
    target.import_branch_artifact(&artifact).expect("import");

    let mut kv = target.kv(branch(), space("default")).expect("kv");
    let value = kv
        .get(&KvKey::new("user:ada").expect("key"))
        .expect("get")
        .expect("present");
    assert_eq!(value.as_bytes(), b"engineer");

    let mut events = target.event(branch(), space("default")).expect("events");
    let verification = events.verify_chain().expect("verify");
    assert!(verification.is_valid(), "hash chain re-derives cleanly");

    let mut graph = target.graph(branch(), space("default")).expect("graph");
    let ontology = graph
        .ontology(&GraphName::new("social").expect("name"))
        .expect("read")
        .expect("defined");
    assert_eq!(ontology.object_types().len(), 1);
}

#[test]
fn import_into_populated_branch_refuses() {
    let mut source = fresh_db();
    populate(&mut source);
    let artifact = export(&mut source);

    let mut target = fresh_db();
    let mut kv = target.kv(branch(), space("default")).expect("kv");
    kv.put(
        KvKey::new("existing").expect("key"),
        KvValue::new(b"content".to_vec()),
    )
    .expect("put");

    let error = match target.import_branch_artifact(&artifact) {
        Err(error) => error,
        Ok(_) => panic!("import into populated branch must refuse"),
    };
    assert_eq!(error.code(), "conflict.engine.artifact_import");
}

#[test]
fn import_is_deterministic_across_targets() {
    let mut source = fresh_db();
    populate(&mut source);
    let artifact = export(&mut source);

    let mut target_a = fresh_db();
    target_a
        .import_branch_artifact(&artifact)
        .expect("import a");
    let mut target_b = fresh_db();
    target_b
        .import_branch_artifact(&artifact)
        .expect("import b");

    assert_eq!(export(&mut target_a), export(&mut target_b));
}

#[test]
fn empty_artifact_imports_cleanly() {
    let mut source = fresh_db();
    let artifact = export(&mut source);

    let mut target = fresh_db();
    let summary = target.import_branch_artifact(&artifact).expect("import");
    assert_eq!(summary.records(), 0);
    assert_eq!(export(&mut target), artifact);
}
