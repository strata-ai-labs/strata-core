//! Bundle import behavior (slice `HB6c`): the round-trip conformance case
//! that closes Ask 4, plus the §3.5 invariants — completeness, hash
//! verification, all-or-nothing staging, idempotency.

use std::collections::HashMap;
use std::path::Path;

use serde_json::json;
use strata_engine::{
    BranchName, Database, DurableLocalOpenOptions, EventPayload, EventType, GraphEdgeData,
    GraphEdgeType, GraphName, GraphNodeData, GraphNodeId, JsonDocumentId, JsonPath, JsonValue,
    KvKey, KvValue, ProductSpace, VectorCollectionName, VectorConfig, VectorDistanceMetric,
    VectorEmbedding, VectorKey, VectorMetadata,
};
use strata_hub::{import_bundle, BundleImportError, EngineExportOptions, StrataCoreEngine};
use stratahub_protocol::Hash;

fn build_fixture_db(path: &Path) {
    let mut db = Database::open_local(path, DurableLocalOpenOptions::new())
        .expect("fixture db opens")
        .into_database();
    let branch = || BranchName::new("default").expect("branch");
    let space = || ProductSpace::new("default").expect("space");

    let mut kv = db.kv(branch(), space()).expect("kv");
    kv.put(
        KvKey::new("user:ada").expect("key"),
        KvValue::new(b"engineer".to_vec()),
    )
    .expect("put");

    let mut json = db.json(branch(), space()).expect("json");
    json.set_or_create(
        JsonDocumentId::new("config").expect("id"),
        &JsonPath::root(),
        JsonValue::new(json!({"model": "claude"})).expect("value"),
    )
    .expect("set");

    let mut events = db.event(branch(), space()).expect("events");
    events
        .append(
            EventType::new("tool_call").expect("type"),
            EventPayload::new(json!({"tool": "search"})).expect("payload"),
        )
        .expect("append");

    let mut vectors = db.vector(branch(), space()).expect("vectors");
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

    let mut graph = db.graph(branch(), space()).expect("graph");
    let name = GraphName::new("social").expect("name");
    graph.create_graph(name.clone()).expect("create");
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
            GraphEdgeData::new(1.0, None).expect("edge"),
        )
        .expect("edge");
}

struct Bundle {
    manifest: stratahub_protocol::Manifest,
    manifest_hash: Hash,
    objects: HashMap<Hash, Vec<u8>>,
}

fn export_fixture_bundle() -> (tempfile::TempDir, Bundle) {
    let source = tempfile::tempdir().expect("tempdir");
    build_fixture_db(source.path());
    let bundle = export_bundle_at(source.path());
    (source, bundle)
}

fn export_bundle_at(path: &Path) -> Bundle {
    let mut engine = StrataCoreEngine::open(path).expect("open");
    let output = engine
        .export_bundle(&EngineExportOptions::default())
        .expect("export");
    let manifest_hash = stratahub_protocol::hash_bytes(&output.manifest_canonical_bytes);
    let objects = output
        .objects
        .into_iter()
        .map(|object| (object.hash, object.bytes))
        .collect();
    Bundle {
        manifest: output.manifest,
        manifest_hash,
        objects,
    }
}

#[test]
fn round_trip_conformance_import_then_reexport_matches_the_manifest_hash() {
    let (_source, bundle) = export_fixture_bundle();

    let workdir = tempfile::tempdir().expect("workdir");
    let target = workdir.path().join("clone.strata");
    import_bundle(&target, &bundle.manifest, &bundle.objects).expect("import succeeds");

    // The reconstituted database opens, serves reads, and re-exports to
    // the identical manifest hash — Ask 4's acceptance property.
    let reexported = export_bundle_at(&target);
    assert_eq!(
        bundle.manifest_hash, reexported.manifest_hash,
        "import → re-export must reproduce the manifest hash"
    );

    let mut db = Database::open_local(&target, DurableLocalOpenOptions::new())
        .expect("clone opens")
        .into_database();
    let mut kv = db
        .kv(
            BranchName::new("default").expect("branch"),
            ProductSpace::new("default").expect("space"),
        )
        .expect("kv");
    let value = kv
        .get(&KvKey::new("user:ada").expect("key"))
        .expect("get")
        .expect("present");
    assert_eq!(value.as_bytes(), b"engineer");
}

#[test]
fn import_is_idempotent_across_targets() {
    let (_source, bundle) = export_fixture_bundle();
    let workdir = tempfile::tempdir().expect("workdir");

    let target_a = workdir.path().join("a.strata");
    let target_b = workdir.path().join("b.strata");
    import_bundle(&target_a, &bundle.manifest, &bundle.objects).expect("import a");
    import_bundle(&target_b, &bundle.manifest, &bundle.objects).expect("import b");

    assert_eq!(
        export_bundle_at(&target_a).manifest_hash,
        export_bundle_at(&target_b).manifest_hash
    );
}

#[test]
fn missing_objects_report_incomplete_bundle_and_leave_no_state() {
    let (_source, mut bundle) = export_fixture_bundle();
    let removed = bundle
        .manifest
        .objects
        .last()
        .expect("objects exist")
        .hash
        .clone();
    bundle.objects.remove(&removed);

    let workdir = tempfile::tempdir().expect("workdir");
    let target = workdir.path().join("clone.strata");
    let Err(error) = import_bundle(&target, &bundle.manifest, &bundle.objects) else {
        panic!("missing object must refuse");
    };
    let BundleImportError::IncompleteBundle { missing_hashes } = error else {
        panic!("unexpected error: {error}");
    };
    assert_eq!(missing_hashes, vec![removed]);
    assert!(!target.exists(), "no partial state on disk");
}

#[test]
fn corrupted_objects_report_hash_mismatch_and_leave_no_state() {
    let (_source, mut bundle) = export_fixture_bundle();
    let victim = bundle
        .manifest
        .objects
        .last()
        .expect("objects exist")
        .hash
        .clone();
    bundle.objects.get_mut(&victim).expect("present").push(0xFF);

    let workdir = tempfile::tempdir().expect("workdir");
    let target = workdir.path().join("clone.strata");
    let Err(error) = import_bundle(&target, &bundle.manifest, &bundle.objects) else {
        panic!("corrupted object must refuse");
    };
    assert!(matches!(
        error,
        BundleImportError::ObjectHashMismatch { expected } if expected == victim
    ));
    assert!(!target.exists(), "no partial state on disk");
}

#[test]
fn non_empty_target_refuses_and_empty_dir_is_claimed() {
    let (_source, bundle) = export_fixture_bundle();
    let workdir = tempfile::tempdir().expect("workdir");

    // Occupied target refuses.
    let occupied = workdir.path().join("occupied");
    std::fs::create_dir_all(&occupied).expect("mkdir");
    std::fs::write(occupied.join("keep.txt"), b"precious").expect("write");
    let Err(error) = import_bundle(&occupied, &bundle.manifest, &bundle.objects) else {
        panic!("occupied target must refuse");
    };
    assert!(matches!(error, BundleImportError::TargetNotEmpty(_)));
    assert!(occupied.join("keep.txt").exists(), "target untouched");

    // Pre-created empty directory is claimed.
    let empty = workdir.path().join("empty");
    std::fs::create_dir_all(&empty).expect("mkdir");
    import_bundle(&empty, &bundle.manifest, &bundle.objects).expect("empty dir claimed");
    assert_eq!(export_bundle_at(&empty).manifest_hash, bundle.manifest_hash);
}
