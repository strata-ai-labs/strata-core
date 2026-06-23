//! Executor error-boundary and source-guard tests.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use strata_executor_next::{
    with_error_render_config, Bytes, Command, CommitOutcomeStatus, ErrorClass,
    ErrorReferenceIdSource, ErrorRenderConfig, Executor, ExecutorError, ExecutorErrorClass,
    RetryPolicy,
};

#[test]
fn executor_errors_have_stable_public_shape() {
    let mut executor = Executor::open_cache().expect("cache executor opens");

    let invalid_key = executor
        .execute(Command::KvGet {
            branch: None,
            space: None,
            key: Bytes::new(Vec::new()),
            as_of: None,
        })
        .expect_err("empty key fails");
    assert_eq!(invalid_key.class(), ExecutorErrorClass::InvalidInput);
    assert_eq!(invalid_key.public_class(), ErrorClass::InvalidArgument);
    assert_eq!(invalid_key.retry_policy(), RetryPolicy::Never);
    assert_eq!(
        invalid_key.commit_outcome(),
        CommitOutcomeStatus::NotStarted
    );
    assert!(!invalid_key.suggested_fix().is_empty());
    assert!(invalid_key.docs_url().ends_with(invalid_key.code()));
    assert!(invalid_key.reference_id().starts_with("err_local_"));
    assert!(invalid_key.code().contains(".engine."));

    let invalid_space = executor
        .execute(Command::KvPut {
            branch: None,
            space: Some("_system_".to_owned()),
            key: Bytes::from("key"),
            value: Bytes::from("value"),
        })
        .expect_err("reserved space fails");
    assert_eq!(invalid_space.class(), ExecutorErrorClass::InvalidInput);

    let missing_branch = executor
        .execute(Command::KvPut {
            branch: Some("missing".to_owned()),
            space: None,
            key: Bytes::from("key"),
            value: Bytes::from("value"),
        })
        .expect_err("missing branch fails");
    assert_eq!(missing_branch.class(), ExecutorErrorClass::NotFound);

    executor.close().expect("close succeeds");
    let closed = executor
        .execute(Command::KvExists {
            branch: None,
            space: None,
            key: Bytes::from("key"),
        })
        .expect_err("closed executor fails");
    assert_eq!(closed.class(), ExecutorErrorClass::ClosedHandle);
    assert_eq!(closed.public_class(), ErrorClass::FailedPrecondition);
    assert_eq!(closed.retry_policy(), RetryPolicy::Never);
    assert_eq!(closed.commit_outcome(), CommitOutcomeStatus::NotStarted);
}

#[derive(Debug)]
struct FixedReferenceIdSource;

impl ErrorReferenceIdSource for FixedReferenceIdSource {
    fn next_reference_id(&self) -> String {
        "ref_test_000001".to_owned()
    }
}

#[test]
fn executor_error_rendering_uses_injected_boundary_config() {
    let config = ErrorRenderConfig::new(
        "https://docs.example.test/errors/",
        Arc::new(FixedReferenceIdSource),
    );

    let error = with_error_render_config(config, || {
        ExecutorError::invalid_input("invalid_argument.executor.test", "public message")
    });

    assert_eq!(error.reference_id(), "ref_test_000001");
    assert_eq!(
        error.docs_url(),
        "https://docs.example.test/errors/invalid_argument.executor.test"
    );
}

#[test]
fn executor_preserves_engine_error_codes_at_public_boundary() {
    let error: ExecutorError =
        strata_engine_next::EngineError::closed_runtime("runtime closed").into();

    assert_eq!(error.code(), "failed_precondition.engine.runtime_closed");
    assert_eq!(error.public_class(), ErrorClass::FailedPrecondition);
    assert_eq!(error.commit_outcome(), CommitOutcomeStatus::NotStarted);
    assert!(error.docs_url().ends_with(error.code()));
}

#[test]
fn serialized_errors_have_v1_status_shape() {
    let error = ExecutorError::invalid_input("invalid_argument.executor.test", "public message");
    let encoded = serde_json::to_string(&error).expect("error serializes");
    let status: serde_json::Value = serde_json::from_str(&encoded).expect("json parses");

    assert_eq!(status["class"], "invalid_argument");
    assert_eq!(status["code"], "invalid_argument.executor.test");
    assert_eq!(status["retry_policy"], "never");
    assert_eq!(status["commit_outcome"], "not_started");
    assert_eq!(status["message"], "public message");
    assert_eq!(
        status["suggested_fix"],
        "Correct the command input and retry."
    );
    assert_eq!(
        status["docs_url"],
        "https://strata.dev/docs/errors/invalid_argument.executor.test"
    );
    assert!(status["reference_id"]
        .as_str()
        .expect("reference id is a string")
        .starts_with("err_local_"));

    for forbidden in forbidden_lower_layer_terms() {
        assert!(
            !encoded.contains(forbidden),
            "serialized error leaked forbidden term `{forbidden}`: {encoded}"
        );
    }
}

#[test]
fn executor_crate_does_not_depend_on_storage_crates() {
    let manifest = fs::read_to_string(crate_root().join("Cargo.toml")).expect("manifest reads");
    assert!(!manifest.contains("strata-storage-next"));
    assert!(!manifest.contains("strata_storage_next"));
}

#[test]
fn executor_sources_do_not_name_lower_layer_types() {
    for file in source_files(&crate_root().join("src")) {
        let text = fs::read_to_string(&file).expect("source reads");
        for forbidden in forbidden_lower_layer_terms() {
            assert!(
                !text.contains(forbidden),
                "{} leaked forbidden term `{forbidden}`",
                file.display()
            );
        }
    }
}

#[test]
fn executor_event_sources_do_not_own_event_product_behavior() {
    for file in source_files(&crate_root().join("src")) {
        let text = fs::read_to_string(&file).expect("source reads");
        for forbidden in forbidden_event_lower_layer_terms() {
            assert!(
                !text.contains(forbidden),
                "{} leaked forbidden event lower-layer term `{forbidden}`",
                file.display()
            );
        }
    }
}

#[test]
fn executor_vector_sources_do_not_own_index_or_distance_behavior() {
    for file in source_files(&crate_root().join("src")) {
        let text = fs::read_to_string(&file).expect("source reads");
        for forbidden in forbidden_vector_index_lower_layer_terms() {
            assert!(
                !text.contains(forbidden),
                "{} leaked forbidden vector lower-layer term `{forbidden}`",
                file.display()
            );
        }
    }
}

#[test]
fn command_and_output_are_serde_serializable() {
    let command_source =
        fs::read_to_string(crate_root().join("src/command.rs")).expect("command reads");
    let output_source =
        fs::read_to_string(crate_root().join("src/output.rs")).expect("output reads");

    assert!(command_source.contains("Serialize"));
    assert!(command_source.contains("Deserialize"));
    assert!(output_source.contains("Serialize"));
    assert!(output_source.contains("Deserialize"));
}

#[test]
fn convenience_facade_stays_command_shaped() {
    let source = fs::read_to_string(crate_root().join("src/executor.rs")).expect("executor reads");
    let facade_start = source
        .find("pub fn branch_list")
        .expect("convenience facade is present");
    let facade_end = source[facade_start..]
        .find("\n}\n\nfn branch_name")
        .expect("convenience facade ends before helper functions");
    let facade = &source[facade_start..facade_start + facade_end];

    assert!(facade.contains("self.execute(Command::KvPut"));
    assert!(facade.contains("self.execute(Command::KvBatchPut"));
    assert!(facade.contains("self.execute(Command::JsonSet"));
    assert!(facade.contains("self.execute(Command::JsonGet"));
    assert!(facade.contains("self.execute(Command::JsonDelete"));
    assert!(facade.contains("self.execute(Command::JsonBatchSet"));
    assert!(facade.contains("self.execute(Command::JsonBatchGet"));
    assert!(facade.contains("self.execute(Command::JsonBatchDelete"));
    assert!(facade.contains("self.execute(Command::VectorCreateCollection"));
    assert!(facade.contains("self.execute(Command::VectorUpsert"));
    assert!(facade.contains("self.execute(Command::VectorGet"));
    assert!(facade.contains("self.execute(Command::VectorQuery"));
    assert!(facade.contains("self.execute(Command::VectorBatchUpsert"));
    assert!(facade.contains("self.execute(Command::VectorBatchGet"));
    assert!(facade.contains("self.execute(Command::VectorBatchDelete"));
    assert!(facade.contains("self.execute(Command::EventBatchAppend"));
    assert!(facade.contains("self.execute(Command::EventAppend"));
    assert!(facade.contains("self.execute(Command::EventGet"));
    assert!(facade.contains("self.execute(Command::EventExists"));
    assert!(facade.contains("self.execute(Command::EventGetByType"));
    assert!(facade.contains("self.execute(Command::EventLen"));
    assert!(facade.contains("self.execute(Command::EventRange"));
    assert!(facade.contains("self.execute(Command::EventRangeByTime"));
    assert!(facade.contains("self.execute(Command::EventListTypes"));
    assert!(facade.contains("self.execute(Command::EventList"));
    assert!(facade.contains("self.execute(Command::EventVerifyChain"));
    assert!(facade.contains("self.execute(Command::GraphCreate"));
    assert!(facade.contains("self.execute(Command::GraphDelete"));
    assert!(facade.contains("self.execute(Command::GraphList"));
    assert!(facade.contains("self.execute(Command::GraphGetMeta"));
    assert!(facade.contains("self.execute(Command::GraphAddNode"));
    assert!(facade.contains("self.execute(Command::GraphGetNode"));
    assert!(facade.contains("self.execute(Command::GraphRemoveNode"));
    assert!(facade.contains("self.execute(Command::GraphListNodes"));
    assert!(facade.contains("self.execute(Command::GraphAddEdge"));
    assert!(facade.contains("self.execute(Command::GraphGetEdge"));
    assert!(facade.contains("self.execute(Command::GraphRemoveEdge"));
    assert!(facade.contains("self.execute(Command::GraphNeighbors"));
    assert!(facade.contains("self.execute(Command::GraphBindingsForEntity"));
    assert!(facade.contains("self.execute(Command::GraphBatchWrite"));
    assert!(!facade.contains(".kv("));
    assert!(!facade.contains(".json("));
    assert!(!facade.contains(".vector("));
    assert!(!facade.contains(".event("));
    assert!(!facade.contains(".graph("));
    assert!(!facade.contains("json_service("));
    assert!(!facade.contains("vector_service("));
    assert!(!facade.contains("event_service("));
    assert!(!facade.contains("graph_service("));
    assert!(!facade.contains(".put("));
    assert!(!facade.contains(".put_batch("));
    assert!(!facade.contains(".set_or_create("));
    assert!(!facade.contains(".batch_set_or_create("));
    assert!(!facade.contains(".batch_delete_entries("));
    assert!(!facade.contains(".batch_upsert("));
    assert!(!facade.contains(".query("));
    assert!(!facade.contains(".append("));
    assert!(!facade.contains(".batch_append("));
    assert!(!facade.contains(".delete("));
    assert!(!facade.contains(".create_graph("));
    assert!(!facade.contains(".batch_write("));
}

#[test]
fn event_batch_append_handler_uses_engine_batch_api() {
    let source = fs::read_to_string(crate_root().join("src/executor.rs")).expect("executor reads");
    let handler = source
        .split("fn execute_event_batch_append")
        .nth(1)
        .expect("event batch handler is present")
        .split("fn execute_event_append")
        .next()
        .expect("event append handler follows batch handler");

    assert!(handler.contains(".batch_append("));
    assert!(!handler.contains("execute_event_append"));
    assert!(!handler.contains(".append("));
}

#[test]
fn source_contract_uses_kv_specific_value_outputs() {
    let output_source =
        fs::read_to_string(crate_root().join("src/output.rs")).expect("output reads");
    let tests_source =
        fs::read_to_string(crate_root().join("tests/command_contract.rs")).expect("tests read");
    assert!(output_source.contains("KvValue"));
    assert!(output_source.contains("KvVersionedValue"));
    assert!(!output_source.contains("KvValue(Maybe"));
    assert!(!output_source.contains("KvVersionedValue(Maybe"));
    assert!(output_source.contains("JsonValue(MaybeJsonValue)"));
    assert!(output_source.contains("JsonVersionedValue(MaybeJsonVersionedValue)"));
    assert!(tests_source.contains("MaybeJsonValue::missing"));
    assert!(tests_source.contains("MaybeJsonVersionedValue::missing"));
}

#[test]
fn executor_graph_sources_do_not_own_graph_storage_behavior() {
    for file in source_files(&crate_root().join("src")) {
        let text = fs::read_to_string(&file).expect("source reads");
        for forbidden in forbidden_graph_lower_layer_terms() {
            assert!(
                !text.contains(forbidden),
                "{} leaked forbidden graph lower-layer term `{forbidden}`",
                file.display()
            );
        }
    }
}

#[test]
fn executor_graph_surface_excludes_deferred_old_commands() {
    for file in source_files(&crate_root().join("src")) {
        let text = fs::read_to_string(&file).expect("source reads");
        for forbidden in excluded_graph_command_names() {
            assert!(
                !text.contains(forbidden),
                "{} exposed deferred graph command `{forbidden}`",
                file.display()
            );
        }
    }
}

#[test]
fn executor_admin_surface_excludes_deferred_old_commands() {
    for file in source_files(&crate_root().join("src")) {
        let text = fs::read_to_string(&file).expect("source reads");
        for forbidden in excluded_admin_command_names() {
            assert!(
                !text.contains(forbidden),
                "{} exposed deferred admin command `{forbidden}`",
                file.display()
            );
        }
    }
}

#[test]
fn executor_benchmarks_do_not_bypass_commands() {
    let benchmark_root = workspace_root().join("benchmarks/src/bin");
    if !benchmark_root.exists() {
        return;
    }

    for file in source_files(&benchmark_root) {
        let text = fs::read_to_string(&file).expect("benchmark source reads");
        if !is_executor_benchmark_source(&text) {
            continue;
        }

        assert!(
            text.contains("Command::KvBatchPut")
                || text.contains("Command::JsonBatchSet")
                || text.contains("Command::VectorBatchUpsert")
                || text.contains("Command::EventBatchAppend")
                || text.contains("Command::GraphBatchWrite")
                || text.contains("Command::ArrowImport")
                || text.contains("Command::ArrowExport"),
            "{} must use serialized executor batch commands",
            file.display()
        );
        for forbidden in [
            "strata_storage_next",
            "StorageRuntime",
            "CommitBatch",
            ".put_batch(",
            ".commit(",
        ] {
            assert!(
                !text.contains(forbidden),
                "{} bypassed executor commands with `{forbidden}`",
                file.display()
            );
        }
    }
}

#[test]
fn executor_arrow_sources_stay_on_serialized_command_boundary() {
    let arrow_root = crate_root().join("src/arrow");
    for file in source_files(&arrow_root) {
        let text = fs::read_to_string(&file).expect("Arrow source reads");
        for forbidden in forbidden_arrow_lower_layer_terms() {
            assert!(
                !text.contains(forbidden),
                "{} bypassed executor commands with `{forbidden}`",
                file.display()
            );
        }
    }

    let import_source =
        fs::read_to_string(arrow_root.join("import.rs")).expect("Arrow import reads");
    assert!(import_source.contains("Command::KvBatchPut"));
    assert!(import_source.contains("Command::JsonBatchSet"));
    assert!(import_source.contains("Command::VectorBatchUpsert"));
    assert!(import_source.contains("Command::VectorListCollections"));
    assert!(import_source.contains("Command::VectorCreateCollection"));

    let export_source =
        fs::read_to_string(arrow_root.join("export.rs")).expect("Arrow export reads");
    assert!(export_source.contains("Command::KvList"));
    assert!(export_source.contains("Command::KvBatchGet"));
    assert!(export_source.contains("Command::JsonList"));
    assert!(export_source.contains("Command::JsonGet"));
    assert!(export_source.contains("Command::EventRange"));
    assert!(export_source.contains("Command::VectorListKeys"));
    assert!(export_source.contains("Command::VectorBatchGet"));
    assert!(export_source.contains("Command::GraphListNodes"));
    assert!(export_source.contains("Command::GraphNeighbors"));
}

fn is_executor_benchmark_source(text: &str) -> bool {
    text.contains("strata_executor_next")
        || text.contains("Command::Kv")
        || text.contains("Command::Json")
        || text.contains("Command::Vector")
        || text.contains("Command::Event")
        || text.contains("Command::Graph")
        || text.contains("Command::Arrow")
}

fn forbidden_lower_layer_terms() -> &'static [&'static str] {
    &[
        "strata-storage-next",
        "strata_storage_next",
        "StorageRuntime",
        "CommitBatch",
        "CommitMutation",
        "StorageSpaceId",
        "StorageKey",
        "StorageValue",
        "BranchRequest",
        "Wal",
        "TableRuntime",
        "Lifecycle",
        "Compaction",
        "storage_api",
    ]
}

fn forbidden_event_lower_layer_terms() -> &'static [&'static str] {
    &[
        "compute_event_hash",
        "EventRecordEnvelope",
        "EventLogMetadata",
        "EventHash",
        "encode_event_record",
        "decode_event_record",
        "encode_event_metadata",
        "decode_event_metadata",
        "event_raw_rows",
        "event_rows(",
        "event_address",
        "type_index_address",
        "metadata_address",
        "StoragePersistence",
        "PersistenceReadRow",
        "RowMutation",
        "RowAddress",
        "ReadSelector",
        "CommitPlan",
        "sha2",
        "Sha256",
        "shadow_embedding",
        "embedding_runtime",
        "export_hook",
        "ExportService",
        "SearchIndex",
        "search_index",
    ]
}

fn forbidden_graph_lower_layer_terms() -> &'static [&'static str] {
    &[
        "strata_engine_next::data::graph",
        "strata_engine::graph",
        "GraphMetadataRecord",
        "GraphNodeRecord",
        "GraphEdgeRecord",
        "GraphBindingRecord",
        "encode_graph_",
        "decode_graph_",
        "graph_metadata_row",
        "node_rows(",
        "edge_rows(",
        "reverse_edge_rows(",
        "binding_rows_for_space",
        "binding_address",
        "node_address",
        "edge_address",
        "reverse_edge_address",
        "Ontology",
        "Pagerank",
        "Cdlp",
        "Sssp",
        "Wcc",
        "Lcc",
    ]
}

fn forbidden_vector_index_lower_layer_terms() -> &'static [&'static str] {
    &[
        "strata_engine_next::data::vector",
        "VectorArtifact",
        "FlatVectorArtifact",
        "HnswVectorArtifact",
        "HnswRuntimeIndex",
        "VectorIndexManifest",
        "VectorIndexPolicy",
        "encode_flat_vector_artifact",
        "decode_flat_vector_artifact",
        "encode_hnsw_vector_artifact",
        "decode_hnsw_vector_artifact",
        "vector_score",
        "fast_hnsw",
        "Hnsw",
        "HNSW",
    ]
}

fn forbidden_arrow_lower_layer_terms() -> &'static [&'static str] {
    &[
        "strata_storage_next",
        "strata-storage-next",
        "strata_engine_next::data",
        "StorageRuntime",
        "CommitBatch",
        "CommitMutation",
        "RowMutation",
        "StorageKey",
        "StorageValue",
        "Wal",
        "TableRuntime",
        "Lifecycle",
        "Compaction",
        "database.",
        "self.database",
        ".kv(",
        ".json(",
        ".vector(",
        "kv_service(",
        "json_service(",
        "vector_service(",
        "event_service(",
        "graph_service(",
    ]
}

fn excluded_graph_command_names() -> &'static [&'static str] {
    &[
        "GraphBulkInsert",
        "GraphBfs",
        "GraphDefineObjectType",
        "GraphGetObjectType",
        "GraphListObjectTypes",
        "GraphDeleteObjectType",
        "GraphDefineLinkType",
        "GraphGetLinkType",
        "GraphListLinkTypes",
        "GraphDeleteLinkType",
        "GraphFreezeOntology",
        "GraphOntologyStatus",
        "GraphOntologySummary",
        "GraphListOntologyTypes",
        "GraphNodesByType",
        "GraphWcc",
        "GraphCdlp",
        "GraphPagerank",
        "GraphLcc",
        "GraphSssp",
    ]
}

fn excluded_admin_command_names() -> &'static [&'static str] {
    &[
        "Command::Flush",
        "Flush {",
        "Command::Compact",
        "Compact {",
        "Command::TimeRange",
        "TimeRange {",
        "Command::DurabilityCounters",
        "DurabilityCounters",
        "ConfigSetAutoEmbed",
        "AutoEmbedStatus",
        "EmbedStatus",
        "ReindexEmbeddings",
        "ConfigureModel",
        "ConfigureSet",
        "RetentionApply",
        "RetentionPreview",
        "RetentionStats",
    ]
}

fn source_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_source_files(root, &mut files);
    files
}

fn collect_source_files(root: &Path, files: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(root).expect("directory reads") {
        let entry = entry.expect("directory entry reads");
        let path = entry.path();
        if path.is_dir() {
            collect_source_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn workspace_root() -> PathBuf {
    crate_root()
        .parent()
        .and_then(Path::parent)
        .expect("crate is under workspace crates directory")
        .to_path_buf()
}
