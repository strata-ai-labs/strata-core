//! Executor Arrow import/export behavior tests.

#![cfg(feature = "arrow")]

use std::fs;
use std::path::Path;
use std::sync::Arc;

use arrow::array::{FixedSizeListBuilder, Float32Builder, StringArray};
use arrow::datatypes::{DataType, Field, Schema};
use arrow::record_batch::RecordBatch;
use serde_json::{json, Value};
use strata_executor::{
    ArrowExportPrimitive, ArrowExportResult, ArrowFileFormat, ArrowImportResult, ArrowImportTarget,
    BatchEventEntry, Bytes, Command, Executor, ExecutorErrorClass, GraphBindingPrimitive,
    GraphBindingTarget, GraphEntityBinding, Output, VectorDistanceMetric, DEFAULT_BRANCH,
};
use tempfile::TempDir;

#[test]
fn arrow_commands_round_trip_through_json() {
    let import = Command::ArrowImport {
        branch: Some("feature".to_owned()),
        space: Some("space-a".to_owned()),
        file_path: "input.csv".to_owned(),
        format: Some(ArrowFileFormat::Csv),
        target: ArrowImportTarget::Json,
        key_column: Some("id".to_owned()),
        value_column: Some("document".to_owned()),
        collection: None,
    };
    let encoded = serde_json::to_value(&import).expect("command serializes");
    assert_eq!(encoded["type"], "arrow_import");
    assert_eq!(encoded["format"], "csv");
    assert_eq!(encoded["target"], "json");
    assert_eq!(
        serde_json::from_value::<Command>(encoded).expect("command deserializes"),
        import
    );

    let output = Output::ArrowExportResult(ArrowExportResult::new(
        ArrowExportPrimitive::Kv,
        ArrowFileFormat::Jsonl,
        vec!["out.jsonl".to_owned()],
        2,
        100,
    ));
    let encoded = serde_json::to_value(&output).expect("output serializes");
    assert_eq!(encoded["type"], "arrow_export_result");
    assert_eq!(encoded["data"]["primitive"], "kv");
    assert_eq!(encoded["data"]["format"], "jsonl");
}

#[test]
fn csv_kv_import_and_jsonl_export_round_trip_bytes() {
    let dir = TempDir::new().expect("temp dir");
    let input_path = dir.path().join("kv.csv");
    let output_path = dir.path().join("kv.jsonl");
    fs::write(
        &input_path,
        "key,key_encoding,value,value_encoding\nplain,utf8,value,utf8\n//4=,base64,AP8=,base64\n",
    )
    .expect("write csv");

    let mut executor = Executor::open_cache().expect("cache executor opens");
    let output = executor
        .execute(Command::ArrowImport {
            branch: None,
            space: None,
            file_path: input_path.to_string_lossy().into_owned(),
            format: Some(ArrowFileFormat::Csv),
            target: ArrowImportTarget::Kv,
            key_column: None,
            value_column: None,
            collection: None,
        })
        .expect("kv import succeeds");
    let Output::ArrowImportResult(result) = output else {
        panic!("unexpected import output");
    };
    assert_import_result(&result, ArrowImportTarget::Kv, 2, 0, 1);

    assert_eq!(
        kv_get(&mut executor, Bytes::from("plain")),
        Some(b"value".to_vec())
    );
    assert_eq!(
        kv_get(&mut executor, Bytes::from(vec![0xff, 0xfe])),
        Some(vec![0x00, 0xff])
    );

    let output = executor
        .execute(Command::ArrowExport {
            branch: None,
            space: None,
            primitive: ArrowExportPrimitive::Kv,
            format: ArrowFileFormat::Jsonl,
            path: output_path.to_string_lossy().into_owned(),
            prefix: None,
            limit: None,
            collection: None,
            graph: None,
            event_type: None,
        })
        .expect("kv export succeeds");
    let Output::ArrowExportResult(result) = output else {
        panic!("unexpected export output");
    };
    assert_eq!(result.primitive(), ArrowExportPrimitive::Kv);
    assert_eq!(result.row_count(), 2);
    assert_eq!(
        result.paths(),
        &[output_path.to_string_lossy().into_owned()]
    );
    assert!(result.size_bytes() > 0);

    let lines = fs::read_to_string(&output_path).expect("read jsonl");
    assert!(lines.contains("\"key_encoding\":\"base64\""));
    assert!(lines.contains("\"value_encoding\":\"base64\""));
}

#[test]
fn csv_json_import_and_jsonl_export_preserve_documents() {
    let dir = TempDir::new().expect("temp dir");
    let input_path = dir.path().join("docs.csv");
    let output_path = dir.path().join("docs.jsonl");
    fs::write(
        &input_path,
        "id,document\nuser-a,\"{\"\"name\"\":\"\"Ada\"\",\"\"rank\"\":1}\"\nuser-b,\"{\"\"name\"\":\"\"Bob\"\"}\"\n",
    )
    .expect("write csv");

    let mut executor = Executor::open_cache().expect("cache executor opens");
    let output = executor
        .execute(Command::ArrowImport {
            branch: None,
            space: None,
            file_path: input_path.to_string_lossy().into_owned(),
            format: None,
            target: ArrowImportTarget::Json,
            key_column: Some("id".to_owned()),
            value_column: Some("document".to_owned()),
            collection: None,
        })
        .expect("json import succeeds");
    let Output::ArrowImportResult(result) = output else {
        panic!("unexpected import output");
    };
    assert_import_result(&result, ArrowImportTarget::Json, 2, 0, 1);
    assert_eq!(
        json_get(&mut executor, "user-a"),
        Some(json!({"name": "Ada", "rank": 1}))
    );

    executor
        .execute(Command::ArrowExport {
            branch: None,
            space: None,
            primitive: ArrowExportPrimitive::Json,
            format: ArrowFileFormat::Jsonl,
            path: output_path.to_string_lossy().into_owned(),
            prefix: Some("user-".to_owned()),
            limit: Some(1),
            collection: None,
            graph: None,
            event_type: None,
        })
        .expect("json export succeeds");
    let exported = fs::read_to_string(&output_path).expect("read jsonl");
    assert!(exported.contains("\"key\":\"user-a\""));
    assert!(!exported.contains("\"key\":\"user-b\""));
    assert!(exported.contains("\\\"name\\\":\\\"Ada\\\""));
}

#[test]
fn parquet_vector_import_and_export_uses_batch_commands() {
    let dir = TempDir::new().expect("temp dir");
    let input_path = dir.path().join("vectors.parquet");
    let output_path = dir.path().join("vectors.parquet");
    write_vector_parquet(&input_path);

    let mut executor = Executor::open_cache().expect("cache executor opens");
    create_docs_collection(&mut executor);
    let output = executor
        .execute(Command::ArrowImport {
            branch: None,
            space: None,
            file_path: input_path.to_string_lossy().into_owned(),
            format: Some(ArrowFileFormat::Parquet),
            target: ArrowImportTarget::Vector,
            key_column: None,
            value_column: None,
            collection: Some("docs".to_owned()),
        })
        .expect("vector import succeeds");
    let Output::ArrowImportResult(result) = output else {
        panic!("unexpected import output");
    };
    assert_import_result(&result, ArrowImportTarget::Vector, 2, 0, 1);

    assert_eq!(vector_count(&mut executor, "docs"), 2);
    assert_eq!(
        vector_get_metadata(&mut executor, "docs", "doc-a"),
        json!({"kind": "a"})
    );

    let output = executor
        .execute(Command::ArrowExport {
            branch: None,
            space: None,
            primitive: ArrowExportPrimitive::Vector,
            format: ArrowFileFormat::Parquet,
            path: output_path.to_string_lossy().into_owned(),
            prefix: Some("doc-".to_owned()),
            limit: None,
            collection: Some("docs".to_owned()),
            graph: None,
            event_type: None,
        })
        .expect("vector export succeeds");
    let Output::ArrowExportResult(result) = output else {
        panic!("unexpected export output");
    };
    assert_eq!(result.row_count(), 2);
    assert!(output_path.exists());
}

#[test]
fn vector_import_into_missing_collection_fails_instead_of_defaulting_a_metric() {
    let dir = TempDir::new().expect("temp dir");
    let input_path = dir.path().join("vectors.parquet");
    write_vector_parquet(&input_path);

    // THIN-2: the executor must not invent a distance metric by auto-creating a
    // Cosine collection; import into a non-existent vector collection fails.
    let mut executor = Executor::open_cache().expect("cache executor opens");
    let error = executor
        .execute(Command::ArrowImport {
            branch: None,
            space: None,
            file_path: input_path.to_string_lossy().into_owned(),
            format: Some(ArrowFileFormat::Parquet),
            target: ArrowImportTarget::Vector,
            key_column: None,
            value_column: None,
            collection: Some("missing".to_owned()),
        })
        .expect_err("import into a missing vector collection is rejected");
    assert_eq!(error.code(), "not_found.executor.vector_collection");
}

#[test]
fn arrow_import_export_respects_branch_and_space_isolation() {
    let dir = TempDir::new().expect("temp dir");
    let input_path = dir.path().join("scoped.csv");
    let output_path = dir.path().join("scoped.jsonl");
    fs::write(&input_path, "key,value\nscoped,visible\n").expect("write csv");

    let mut executor = Executor::open_cache().expect("cache executor opens");
    executor
        .execute(Command::BranchForkCurrent {
            source: DEFAULT_BRANCH.to_owned(),
            branch: "feature".to_owned(),
        })
        .expect("branch fork succeeds");

    let output = executor
        .execute(Command::ArrowImport {
            branch: Some("feature".to_owned()),
            space: Some("tenant-a".to_owned()),
            file_path: input_path.to_string_lossy().into_owned(),
            format: Some(ArrowFileFormat::Csv),
            target: ArrowImportTarget::Kv,
            key_column: Some("key".to_owned()),
            value_column: Some("value".to_owned()),
            collection: None,
        })
        .expect("scoped import succeeds");
    let Output::ArrowImportResult(result) = output else {
        panic!("unexpected import output");
    };
    assert_import_result(&result, ArrowImportTarget::Kv, 1, 0, 1);

    assert_eq!(kv_get(&mut executor, Bytes::from("scoped")), None);
    assert_eq!(
        kv_get_in(
            &mut executor,
            Some("feature"),
            Some("tenant-a"),
            Bytes::from("scoped"),
        ),
        Some(b"visible".to_vec())
    );
    assert_eq!(
        kv_get_in(&mut executor, Some("feature"), None, Bytes::from("scoped"),),
        None
    );

    let output = executor
        .execute(Command::ArrowExport {
            branch: Some("feature".to_owned()),
            space: Some("tenant-a".to_owned()),
            primitive: ArrowExportPrimitive::Kv,
            format: ArrowFileFormat::Jsonl,
            path: output_path.to_string_lossy().into_owned(),
            prefix: Some("sc".to_owned()),
            limit: Some(10),
            collection: None,
            graph: None,
            event_type: None,
        })
        .expect("scoped export succeeds");
    let Output::ArrowExportResult(result) = output else {
        panic!("unexpected export output");
    };
    assert_eq!(result.row_count(), 1);
    let exported = fs::read_to_string(output_path).expect("read jsonl");
    assert!(exported.contains("\"key\":\"scoped\""));
    assert!(exported.contains("\"value\":\"visible\""));
}

#[test]
fn durable_arrow_import_survives_reopen_and_exports() {
    let dir = TempDir::new().expect("temp dir");
    let db_path = dir.path().join("db");
    let kv_path = dir.path().join("kv.csv");
    let json_path = dir.path().join("docs.csv");
    let vector_path = dir.path().join("vectors.parquet");
    let export_path = dir.path().join("exported.jsonl");
    fs::write(&kv_path, "key,value\npersisted,value\n").expect("write kv csv");
    fs::write(
        &json_path,
        "id,document\nuser-a,\"{\"\"name\"\":\"\"Ada\"\"}\"\n",
    )
    .expect("write json csv");
    write_vector_parquet(&vector_path);

    {
        let mut executor = Executor::open_durable_local(&db_path).expect("durable executor opens");
        executor
            .execute(Command::ArrowImport {
                branch: None,
                space: None,
                file_path: kv_path.to_string_lossy().into_owned(),
                format: Some(ArrowFileFormat::Csv),
                target: ArrowImportTarget::Kv,
                key_column: Some("key".to_owned()),
                value_column: Some("value".to_owned()),
                collection: None,
            })
            .expect("kv import succeeds");
        executor
            .execute(Command::ArrowImport {
                branch: None,
                space: None,
                file_path: json_path.to_string_lossy().into_owned(),
                format: Some(ArrowFileFormat::Csv),
                target: ArrowImportTarget::Json,
                key_column: Some("id".to_owned()),
                value_column: Some("document".to_owned()),
                collection: None,
            })
            .expect("json import succeeds");
        create_docs_collection(&mut executor);
        executor
            .execute(Command::ArrowImport {
                branch: None,
                space: None,
                file_path: vector_path.to_string_lossy().into_owned(),
                format: Some(ArrowFileFormat::Parquet),
                target: ArrowImportTarget::Vector,
                key_column: None,
                value_column: None,
                collection: Some("docs".to_owned()),
            })
            .expect("vector import succeeds");
        executor.close().expect("close succeeds");
    }

    let mut reopened = Executor::open_durable_local(&db_path).expect("durable executor reopens");
    assert_eq!(
        kv_get(&mut reopened, Bytes::from("persisted")),
        Some(b"value".to_vec())
    );
    assert_eq!(
        json_get(&mut reopened, "user-a"),
        Some(json!({"name": "Ada"}))
    );
    assert_eq!(vector_count(&mut reopened, "docs"), 2);
    assert_eq!(
        vector_get_metadata(&mut reopened, "docs", "doc-b"),
        json!({"kind": "b"})
    );

    let output = reopened
        .execute(Command::ArrowExport {
            branch: None,
            space: None,
            primitive: ArrowExportPrimitive::Kv,
            format: ArrowFileFormat::Jsonl,
            path: export_path.to_string_lossy().into_owned(),
            prefix: Some("persist".to_owned()),
            limit: None,
            collection: None,
            graph: None,
            event_type: None,
        })
        .expect("durable export succeeds");
    let Output::ArrowExportResult(result) = output else {
        panic!("unexpected export output");
    };
    assert_eq!(result.row_count(), 1);
    assert!(fs::read_to_string(export_path)
        .expect("read export")
        .contains("\"key\":\"persisted\""));
}

#[test]
fn event_export_writes_filtered_jsonl_without_mutating_log() {
    let dir = TempDir::new().expect("temp dir");
    let output_path = dir.path().join("events.jsonl");
    let mut executor = Executor::open_cache().expect("cache executor opens");
    executor
        .execute(Command::EventBatchAppend {
            branch: None,
            space: None,
            entries: vec![
                BatchEventEntry::new("audit.created", json!({"id": 1})),
                BatchEventEntry::new("audit.updated", json!({"id": 1})),
            ],
        })
        .expect("event batch append succeeds");

    let output = executor
        .execute(Command::ArrowExport {
            branch: None,
            space: None,
            primitive: ArrowExportPrimitive::Event,
            format: ArrowFileFormat::Jsonl,
            path: output_path.to_string_lossy().into_owned(),
            prefix: None,
            limit: None,
            collection: None,
            graph: None,
            event_type: Some("audit.created".to_owned()),
        })
        .expect("event export succeeds");
    let Output::ArrowExportResult(result) = output else {
        panic!("unexpected export output");
    };
    assert_eq!(result.row_count(), 1);
    let exported = fs::read_to_string(output_path).expect("read jsonl");
    assert!(exported.contains("\"event_type\":\"audit.created\""));
    assert!(!exported.contains("\"event_type\":\"audit.updated\""));
    assert_eq!(event_len(&mut executor), 2);
}

#[test]
fn graph_export_writes_node_and_edge_tables() {
    let dir = TempDir::new().expect("temp dir");
    let output_path = dir.path().join("graph.jsonl");
    let mut executor = Executor::open_cache().expect("cache executor opens");
    create_graph_fixture(&mut executor);

    let output = executor
        .execute(Command::ArrowExport {
            branch: None,
            space: None,
            primitive: ArrowExportPrimitive::Graph,
            format: ArrowFileFormat::Jsonl,
            path: output_path.to_string_lossy().into_owned(),
            prefix: None,
            limit: None,
            collection: None,
            graph: Some("deps".to_owned()),
            event_type: None,
        })
        .expect("graph export succeeds");
    let Output::ArrowExportResult(result) = output else {
        panic!("unexpected export output");
    };
    assert_eq!(result.primitive(), ArrowExportPrimitive::Graph);
    assert_eq!(result.row_count(), 3);
    let node_path = dir.path().join("graph_nodes.jsonl");
    let edge_path = dir.path().join("graph_edges.jsonl");
    assert_eq!(
        result.paths(),
        &[
            node_path.to_string_lossy().into_owned(),
            edge_path.to_string_lossy().into_owned(),
        ]
    );
    assert!(
        !output_path.exists(),
        "graph export path is a stem; use returned paths for written files"
    );
    assert!(node_path.exists());
    assert!(edge_path.exists());
    let nodes = fs::read_to_string(&node_path).expect("nodes");
    assert!(nodes.contains("\"node_id\":\"node-a\""));
    assert!(nodes.contains("\\\"key\\\":\\\"doc-a\\\""));
    let edges = fs::read_to_string(&edge_path).expect("edges");
    assert!(edges.contains("\"edge_type\":\"depends_on\""));
}

#[test]
fn arrow_export_rejects_missing_primitive_options() {
    let dir = TempDir::new().expect("temp dir");
    let mut executor = Executor::open_cache().expect("cache executor opens");

    let vector_error = executor
        .execute(Command::ArrowExport {
            branch: None,
            space: None,
            primitive: ArrowExportPrimitive::Vector,
            format: ArrowFileFormat::Jsonl,
            path: dir
                .path()
                .join("vectors.jsonl")
                .to_string_lossy()
                .into_owned(),
            prefix: None,
            limit: None,
            collection: None,
            graph: None,
            event_type: None,
        })
        .expect_err("missing vector collection fails");
    assert_eq!(vector_error.class(), ExecutorErrorClass::InvalidInput);
    assert_eq!(
        vector_error.code(),
        "invalid_argument.executor.arrow_collection"
    );

    let graph_error = executor
        .execute(Command::ArrowExport {
            branch: None,
            space: None,
            primitive: ArrowExportPrimitive::Graph,
            format: ArrowFileFormat::Jsonl,
            path: dir
                .path()
                .join("graph.jsonl")
                .to_string_lossy()
                .into_owned(),
            prefix: None,
            limit: None,
            collection: None,
            graph: None,
            event_type: None,
        })
        .expect_err("missing graph fails");
    assert_eq!(graph_error.class(), ExecutorErrorClass::InvalidInput);
    assert_eq!(graph_error.code(), "invalid_argument.executor.arrow_graph");
}

#[test]
fn arrow_import_rejects_unknown_format_before_storage_mutation() {
    let dir = TempDir::new().expect("temp dir");
    let input_path = dir.path().join("records.arrow");
    fs::write(&input_path, b"not read").expect("write unknown extension");
    let mut executor = Executor::open_cache().expect("cache executor opens");

    let error = executor
        .execute(Command::ArrowImport {
            branch: None,
            space: None,
            file_path: input_path.to_string_lossy().into_owned(),
            format: None,
            target: ArrowImportTarget::Kv,
            key_column: None,
            value_column: None,
            collection: None,
        })
        .expect_err("unknown format fails");
    assert_eq!(error.class(), ExecutorErrorClass::InvalidInput);
    assert_eq!(error.code(), "invalid_argument.executor.arrow_format");
    assert!(kv_keys(&mut executor).is_empty());
}

#[test]
fn missing_input_is_reported_before_arrow_feature_work() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    let error = executor
        .execute(Command::ArrowImport {
            branch: None,
            space: None,
            file_path: "definitely-missing.csv".to_owned(),
            format: Some(ArrowFileFormat::Csv),
            target: ArrowImportTarget::Kv,
            key_column: None,
            value_column: None,
            collection: None,
        })
        .expect_err("missing input fails");
    assert_eq!(error.class(), ExecutorErrorClass::InvalidInput);
    assert_eq!(
        error.code(),
        "invalid_argument.executor.arrow_input_missing"
    );
}

fn assert_import_result(
    result: &ArrowImportResult,
    target: ArrowImportTarget,
    rows_imported: u64,
    rows_skipped: u64,
    batches_processed: u64,
) {
    assert_eq!(result.target(), target);
    assert_eq!(result.rows_imported(), rows_imported);
    assert_eq!(result.rows_skipped(), rows_skipped);
    assert_eq!(result.batches_processed(), batches_processed);
}

fn kv_get(executor: &mut Executor, key: Bytes) -> Option<Vec<u8>> {
    kv_get_in(executor, None, None, key)
}

fn kv_keys(executor: &mut Executor) -> Vec<Bytes> {
    let output = executor
        .execute(Command::KvList {
            branch: None,
            space: None,
            prefix: None,
            cursor: None,
            limit: None,
            as_of: None,
        })
        .expect("kv list succeeds");
    match output {
        Output::Keys { items: keys, .. } | Output::KeysPage { items: keys, .. } => keys,
        output => panic!("unexpected kv list output: {output:?}"),
    }
}

fn kv_get_in(
    executor: &mut Executor,
    branch: Option<&str>,
    space: Option<&str>,
    key: Bytes,
) -> Option<Vec<u8>> {
    let output = executor
        .execute(Command::KvGet {
            branch: branch.map(str::to_owned),
            space: space.map(str::to_owned),
            key,
            as_of: None,
        })
        .expect("kv get succeeds");
    let Output::KvVersionedValue(value) = output else {
        panic!("unexpected kv get output");
    };
    value.map(|value| value.value().as_slice().to_vec())
}

fn json_get(executor: &mut Executor, key: &str) -> Option<Value> {
    json_get_in(executor, None, None, key)
}

fn json_get_in(
    executor: &mut Executor,
    branch: Option<&str>,
    space: Option<&str>,
    key: &str,
) -> Option<Value> {
    let output = executor
        .execute(Command::JsonGet {
            branch: branch.map(str::to_owned),
            space: space.map(str::to_owned),
            key: key.to_owned(),
            path: "$".to_owned(),
            as_of: None,
        })
        .expect("json get succeeds");
    let Output::JsonVersionedValue(value) = output else {
        panic!("unexpected json get output");
    };
    value.value().map(|value| value.value().clone())
}

fn vector_count(executor: &mut Executor, collection: &str) -> u64 {
    let output = executor
        .execute(Command::VectorCount {
            branch: None,
            space: None,
            collection: collection.to_owned(),
        })
        .expect("vector count succeeds");
    let Output::Uint(count) = output else {
        panic!("unexpected vector count output");
    };
    count
}

fn event_len(executor: &mut Executor) -> u64 {
    let output = executor
        .execute(Command::EventLen {
            branch: None,
            space: None,
            as_of: None,
        })
        .expect("event len succeeds");
    let Output::EventLength { count } = output else {
        panic!("unexpected event len output");
    };
    count
}

fn vector_get_metadata(executor: &mut Executor, collection: &str, key: &str) -> Value {
    let output = executor
        .execute(Command::VectorGet {
            branch: None,
            space: None,
            collection: collection.to_owned(),
            key: key.to_owned(),
            as_of: None,
        })
        .expect("vector get succeeds");
    let Output::VectorData(Some(value)) = output else {
        panic!("unexpected vector get output");
    };
    value.data().metadata().cloned().expect("metadata")
}

fn create_docs_collection(executor: &mut Executor) {
    executor
        .execute(Command::VectorCreateCollection {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
            dimension: 2,
            metric: VectorDistanceMetric::Cosine,
        })
        .expect("collection create succeeds");
}

fn write_vector_parquet(path: &Path) {
    let schema = Arc::new(Schema::new(vec![
        Field::new("key", DataType::Utf8, false),
        Field::new(
            "embedding",
            DataType::FixedSizeList(Arc::new(Field::new("item", DataType::Float32, true)), 2),
            false,
        ),
        Field::new("kind", DataType::Utf8, false),
    ]));
    let mut embedding_builder = FixedSizeListBuilder::new(Float32Builder::new(), 2);
    for value in [1.0, 0.0, 0.0, 1.0] {
        embedding_builder.values().append_value(value);
    }
    embedding_builder.append(true);
    embedding_builder.append(true);
    let batch = RecordBatch::try_new(
        schema.clone(),
        vec![
            Arc::new(StringArray::from(vec!["doc-a", "doc-b"])),
            Arc::new(embedding_builder.finish()),
            Arc::new(StringArray::from(vec!["a", "b"])),
        ],
    )
    .expect("record batch");
    let file = std::fs::File::create(path).expect("create parquet");
    let mut writer =
        parquet::arrow::ArrowWriter::try_new(file, schema, None).expect("create parquet writer");
    writer.write(&batch).expect("write parquet batch");
    writer.close().expect("close parquet writer");
}

fn create_graph_fixture(executor: &mut Executor) {
    executor
        .execute(Command::GraphCreate {
            branch: None,
            space: None,
            graph: "deps".to_owned(),
        })
        .expect("graph create succeeds");
    for (node_id, kind, binding) in [
        ("node-a", "root", Some(graph_binding("doc-a"))),
        ("node-b", "leaf", None),
    ] {
        executor
            .execute(Command::GraphAddNode {
                object_type: None,
                branch: None,
                space: None,
                graph: "deps".to_owned(),
                node_id: node_id.to_owned(),
                properties: Some(json!({"kind": kind})),
                binding,
            })
            .expect("node add succeeds");
    }
    executor
        .execute(Command::GraphAddEdge {
            branch: None,
            space: None,
            graph: "deps".to_owned(),
            src: "node-a".to_owned(),
            edge_type: "depends_on".to_owned(),
            dst: "node-b".to_owned(),
            weight: Some(2.5),
            properties: Some(json!({"why": "test"})),
        })
        .expect("edge add succeeds");
}

fn graph_binding(key: &str) -> GraphEntityBinding {
    GraphEntityBinding::new(GraphBindingTarget::new(
        GraphBindingPrimitive::Json,
        None,
        "docs",
        key,
    ))
}
