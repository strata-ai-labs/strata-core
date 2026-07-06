use super::output_cases::vector_index_diagnostics_fixture;
use super::support::*;

#[test]
fn kv_history_output_json_uses_counted_items_shape() {
    let output = Output::VersionHistory(Some(HistoryResult::new(vec![
        HistoryItem::new(Some(bytes("new")), false, 2, 20),
        HistoryItem::new(Some(bytes("old")), false, 1, 10),
    ])));
    let encoded = serde_json::to_value(&output).expect("history output serializes");
    assert_eq!(encoded["type"], "version_history");
    assert_eq!(encoded["data"]["count"], 2);
    assert_eq!(
        encoded["data"]["items"]
            .as_array()
            .expect("items is an array")
            .len(),
        2
    );
    assert_eq!(encoded["data"]["items"][0]["value"], json!([110, 101, 119]));
    assert_eq!(
        serde_json::from_value::<Output>(encoded).expect("output deserializes"),
        output
    );

    let missing = Output::VersionHistory(None);
    let missing_json = serde_json::to_value(&missing).expect("missing history serializes");
    assert_eq!(missing_json["type"], "version_history");
    assert_eq!(missing_json["data"], Value::Null);
    assert_eq!(
        serde_json::from_value::<Output>(missing_json).expect("output deserializes"),
        missing
    );
}

#[test]
fn vector_history_output_json_uses_counted_items_shape() {
    let output = Output::VectorVersionHistory(Some(VectorHistoryResult::new(vec![
        VectorHistoryItem::new(
            "doc-a".to_owned(),
            Some(VectorData::new(vec![1.0, 0.0], Some(json!({"rank": 2})))),
            2,
            20,
            Some(2),
            false,
        ),
        VectorHistoryItem::new("doc-a".to_owned(), None, 1, 10, None, true),
    ])));
    let encoded = serde_json::to_value(&output).expect("history output serializes");
    assert_eq!(encoded["type"], "vector_version_history");
    assert_eq!(encoded["data"]["count"], 2);
    assert_eq!(
        encoded["data"]["items"]
            .as_array()
            .expect("items is an array")
            .len(),
        2
    );
    assert_eq!(encoded["data"]["items"][0]["data"]["metadata"]["rank"], 2);
    assert_eq!(
        serde_json::from_value::<Output>(encoded).expect("output deserializes"),
        output
    );

    let missing = Output::VectorVersionHistory(None);
    let missing_json = serde_json::to_value(&missing).expect("missing history serializes");
    assert_eq!(missing_json["type"], "vector_version_history");
    assert_eq!(missing_json["data"], Value::Null);
    assert_eq!(
        serde_json::from_value::<Output>(missing_json).expect("output deserializes"),
        missing
    );
}

#[test]
fn admin_and_space_command_json_uses_stable_tags_and_field_shape() {
    let command = Command::SpaceDelete {
        branch: None,
        space: "tenant_a".to_owned(),
        force: false,
    };
    let encoded = serde_json::to_value(&command).expect("command serializes");
    assert_eq!(
        encoded,
        json!({
            "type": "space_delete",
            "space": "tenant_a",
        })
    );
    assert_eq!(
        serde_json::from_value::<Command>(encoded).expect("command deserializes"),
        command
    );

    let forced = Command::SpaceDelete {
        branch: Some("feature".to_owned()),
        space: "tenant_a".to_owned(),
        force: true,
    };
    let forced_json = serde_json::to_value(&forced).expect("command serializes");
    assert_eq!(forced_json["type"], "space_delete");
    assert_eq!(forced_json["branch"], "feature");
    assert_eq!(forced_json["space"], "tenant_a");
    assert_eq!(forced_json["force"], true);

    let info = Command::Info { branch: None };
    let info_json = serde_json::to_value(&info).expect("command serializes");
    assert_eq!(info_json, json!({ "type": "info" }));
}

#[test]
fn event_command_json_uses_stable_tags_and_field_shape() {
    let command = Command::EventAppend {
        branch: None,
        space: None,
        event_type: "audit.recorded".to_owned(),
        payload: json!({
            "id": 1,
            "nested": [{"ok": true}, {"count": 2}],
            "empty": {},
        }),
    };
    let encoded = serde_json::to_value(&command).expect("command serializes");
    assert_eq!(
        encoded,
        json!({
            "type": "event_append",
            "event_type": "audit.recorded",
            "payload": {
                "id": 1,
                "nested": [{"ok": true}, {"count": 2}],
                "empty": {},
            },
        })
    );
    assert_eq!(
        serde_json::from_value::<Command>(encoded).expect("command deserializes"),
        command
    );

    let explicit = Command::EventRangeByTime {
        branch: Some("feature".to_owned()),
        space: Some("space-a".to_owned()),
        start_ts: 10,
        end_ts: Some(99),
        limit: Some(5),
        direction: EventRangeDirection::Reverse,
        event_type: Some("audit.recorded".to_owned()),
    };
    let explicit_json = serde_json::to_value(&explicit).expect("command serializes");
    assert_eq!(explicit_json["type"], "event_range_by_time");
    assert_eq!(explicit_json["branch"], "feature");
    assert_eq!(explicit_json["space"], "space-a");
    assert_eq!(explicit_json["start_ts"], 10);
    assert_eq!(explicit_json["end_ts"], 99);
    assert_eq!(explicit_json["limit"], 5);
    assert_eq!(explicit_json["direction"], "reverse");
    assert_eq!(explicit_json["event_type"], "audit.recorded");

    let unknown_field = json!({
        "type": "event_append",
        "event_type": "audit.recorded",
        "payload": {},
        "extra": true,
    });
    assert!(serde_json::from_value::<Command>(unknown_field).is_err());
}

#[test]
fn event_output_json_uses_stable_tags_and_field_shape() {
    let output = Output::EventRangeResult {
        items: Vec::new(),
        page: PageInfo::terminal(),
    };
    let encoded = serde_json::to_value(&output).expect("output serializes");
    assert_eq!(encoded["type"], "event_range_result");
    assert_eq!(encoded["data"]["items"], Value::Array(Vec::new()));
    assert_eq!(encoded["data"]["has_more"], false);
    assert_eq!(encoded["data"]["cursor"], Value::Null);
    assert_eq!(
        serde_json::from_value::<Output>(encoded).expect("output deserializes"),
        output
    );

    let verification =
        Output::EventChainVerification(EventChainVerification::new(true, 2, None, None));
    let verification_json =
        serde_json::to_value(&verification).expect("verification output serializes");
    assert_eq!(verification_json["type"], "event_chain_verification");
    assert_eq!(verification_json["data"]["valid"], true);
    assert!(
        verification_json["data"].get("is_valid").is_none(),
        "event chain verification must serialize `valid`, not `is_valid`"
    );
    assert_eq!(
        serde_json::from_value::<Output>(json!({
            "type": "event_chain_verification",
            "data": {
                "is_valid": true,
                "length": 2,
                "first_invalid": null,
                "error": null
            }
        }))
        .expect("legacy verification output deserializes"),
        verification
    );
}

#[test]
fn graph_command_json_uses_stable_tags_and_field_shape() {
    let command = Command::GraphAddNode {
        branch: None,
        space: None,
        graph: "deps".to_owned(),
        node_id: "node-a".to_owned(),
        properties: Some(json!({"kind": "root"})),
        binding: Some(graph_binding()),
    };
    let encoded = serde_json::to_value(&command).expect("command serializes");
    assert_eq!(
        encoded,
        json!({
            "type": "graph_add_node",
            "graph": "deps",
            "node_id": "node-a",
            "properties": {"kind": "root"},
            "binding": {
                "target": {
                    "primitive": "json",
                    "branch": "feature",
                    "space": "docs",
                    "key": "doc-a",
                }
            }
        })
    );
    assert_eq!(
        serde_json::from_value::<Command>(encoded).expect("command deserializes"),
        command
    );

    let explicit = Command::GraphNeighbors {
        branch: Some("feature".to_owned()),
        space: Some("space-a".to_owned()),
        graph: "deps".to_owned(),
        node_id: "node-a".to_owned(),
        direction: GraphDirection::Both,
        edge_type: Some("depends_on".to_owned()),
        cursor: Some("cursor".to_owned()),
        limit: Some(5),
    };
    let explicit_json = serde_json::to_value(&explicit).expect("command serializes");
    assert_eq!(explicit_json["type"], "graph_neighbors");
    assert_eq!(explicit_json["branch"], "feature");
    assert_eq!(explicit_json["space"], "space-a");
    assert_eq!(explicit_json["direction"], "both");
    assert_eq!(explicit_json["edge_type"], "depends_on");

    let unknown_field = json!({
        "type": "graph_create",
        "graph": "deps",
        "extra": true,
    });
    assert!(serde_json::from_value::<Command>(unknown_field).is_err());
}

#[test]
fn graph_output_json_uses_stable_tags_and_field_shape() {
    let output = Output::GraphNeighborPage {
        items: vec![GraphNeighborHit::new(
            graph_node_output("deps", "node-b"),
            graph_edge_output("deps", "node-a", "depends_on", "node-b"),
            GraphDirection::Outgoing,
        )],
        page: PageInfo::terminal(),
    };
    let encoded = serde_json::to_value(&output).expect("output serializes");
    assert_eq!(encoded["type"], "graph_neighbor_page");
    assert_eq!(encoded["data"]["items"][0]["direction"], "outgoing");
    assert_eq!(encoded["data"]["items"][0]["node"]["node_id"], "node-b");
    assert_eq!(
        encoded["data"]["items"][0]["edge"]["edge_type"],
        "depends_on"
    );
    assert_eq!(
        serde_json::from_value::<Output>(encoded).expect("output deserializes"),
        output
    );
}

#[test]
fn arrow_command_json_uses_stable_tags_and_field_shape() {
    let import = Command::ArrowImport {
        branch: None,
        space: None,
        file_path: "input.jsonl".to_owned(),
        format: None,
        target: ArrowImportTarget::Vector,
        key_column: Some("_id".to_owned()),
        value_column: Some("embedding".to_owned()),
        collection: Some("docs".to_owned()),
    };
    let encoded = serde_json::to_value(&import).expect("command serializes");
    assert_eq!(encoded["type"], "arrow_import");
    assert_eq!(encoded["file_path"], "input.jsonl");
    assert_eq!(encoded["target"], "vector");
    assert_eq!(encoded["collection"], "docs");
    assert!(encoded.get("branch").is_none());
    assert!(encoded.get("space").is_none());
    assert!(encoded.get("format").is_none());
    assert_eq!(
        serde_json::from_value::<Command>(encoded).expect("command deserializes"),
        import
    );

    let export = Command::ArrowExport {
        branch: Some("feature".to_owned()),
        space: Some("space-a".to_owned()),
        primitive: ArrowExportPrimitive::Graph,
        format: ArrowFileFormat::Parquet,
        path: "graph.parquet".to_owned(),
        prefix: Some("node-".to_owned()),
        limit: Some(10),
        collection: None,
        graph: Some("deps".to_owned()),
        event_type: Some("audit.created".to_owned()),
    };
    let encoded = serde_json::to_value(&export).expect("command serializes");
    assert_eq!(encoded["type"], "arrow_export");
    assert_eq!(encoded["branch"], "feature");
    assert_eq!(encoded["space"], "space-a");
    assert_eq!(encoded["primitive"], "graph");
    assert_eq!(encoded["format"], "parquet");
    assert_eq!(encoded["path"], "graph.parquet");
    assert_eq!(encoded["prefix"], "node-");
    assert_eq!(encoded["limit"], 10);
    assert_eq!(encoded["graph"], "deps");
    assert_eq!(encoded["event_type"], "audit.created");
    assert_eq!(
        serde_json::from_value::<Command>(encoded).expect("command deserializes"),
        export
    );

    assert!(serde_json::from_value::<Command>(json!({
        "type": "arrow_import",
        "file_path": "input.csv",
        "target": "kv",
        "extra": true,
    }))
    .is_err());
    assert!(serde_json::from_value::<Command>(json!({
        "type": "arrow_export",
        "primitive": "kv",
        "format": "csv",
        "path": "out.csv",
        "extra": true,
    }))
    .is_err());
}

#[test]
fn arrow_output_json_uses_stable_tags_and_field_shape() {
    let import = Output::ArrowImportResult(ArrowImportResult::new(
        ArrowImportTarget::Json,
        "input.csv".to_owned(),
        0,
        2,
        1,
    ));
    let encoded = serde_json::to_value(&import).expect("output serializes");
    assert_eq!(encoded["type"], "arrow_import_result");
    assert_eq!(encoded["data"]["target"], "json");
    assert_eq!(encoded["data"]["file_path"], "input.csv");
    assert_eq!(encoded["data"]["rows_imported"], 0);
    assert_eq!(encoded["data"]["rows_skipped"], 2);
    assert_eq!(encoded["data"]["batches_processed"], 1);
    assert_eq!(
        serde_json::from_value::<Output>(encoded).expect("output deserializes"),
        import
    );

    let export = Output::ArrowExportResult(ArrowExportResult::new(
        ArrowExportPrimitive::Graph,
        ArrowFileFormat::Jsonl,
        vec![
            "graph_nodes.jsonl".to_owned(),
            "graph_edges.jsonl".to_owned(),
        ],
        0,
        0,
    ));
    let encoded = serde_json::to_value(&export).expect("output serializes");
    assert_eq!(encoded["type"], "arrow_export_result");
    assert_eq!(encoded["data"]["primitive"], "graph");
    assert_eq!(encoded["data"]["format"], "jsonl");
    assert_eq!(
        encoded["data"]["paths"],
        json!(["graph_nodes.jsonl", "graph_edges.jsonl"])
    );
    assert_eq!(encoded["data"]["row_count"], 0);
    assert_eq!(encoded["data"]["size_bytes"], 0);
    assert_eq!(
        serde_json::from_value::<Output>(encoded).expect("output deserializes"),
        export
    );
}

#[test]
fn vector_index_query_json_uses_stable_tags_and_field_shape() {
    let command = Command::VectorIndexQuery {
        branch: None,
        space: None,
        collection: "docs".to_owned(),
        query: vec![1.0, 0.0],
        k: 5,
        filter: Some(VectorMetadataFilter::new(vec![VectorFilterCondition::eq(
            "kind", "doc",
        )])),
        as_of: None,
    };
    let encoded = serde_json::to_value(&command).expect("command serializes");
    assert_eq!(encoded["type"], "vector_index_query");
    assert_eq!(encoded["collection"], "docs");
    assert_eq!(encoded["query"], json!([1.0, 0.0]));
    assert_eq!(encoded["k"], 5);
    assert!(encoded.get("branch").is_none());
    assert!(encoded.get("space").is_none());
    assert!(encoded.get("as_of").is_none());
    assert_eq!(
        serde_json::from_value::<Command>(encoded).expect("command deserializes"),
        command
    );

    let output = Output::VectorIndexQuery(VectorIndexQueryResult::new(
        vec![VectorMatch::new("doc-a".to_owned(), 1.0, None)],
        vector_index_diagnostics_fixture(),
    ));
    let encoded = serde_json::to_value(&output).expect("output serializes");
    assert_eq!(encoded["type"], "vector_index_query");
    assert_eq!(encoded["data"]["matches"][0]["key"], "doc-a");
    assert_eq!(encoded["data"]["diagnostics"]["collection"], "docs");
    assert_eq!(encoded["data"]["diagnostics"]["manifest_status"], "loaded");
    assert_eq!(
        encoded["data"]["diagnostics"]["artifact_sources"][0]["status"],
        "loaded"
    );
    assert_eq!(encoded["data"]["diagnostics"]["hnsw_graph_builds"], 0);
    assert_eq!(
        serde_json::from_value::<Output>(encoded).expect("output deserializes"),
        output
    );
}
