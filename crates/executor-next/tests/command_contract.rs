//! Executor command and output serialization contract tests.

use serde_json::{json, Value};
use strata_executor_next::{
    ArrowExportPrimitive, ArrowExportResult, ArrowFileFormat, ArrowImportResult, ArrowImportTarget,
    BatchEventEntry, BatchGetItemResult, BatchItemResult, BatchJsonDeleteEntry, BatchJsonEntry,
    BatchJsonGetEntry, BatchKvEntry, BatchVectorEntry, BranchCleanupItem, BranchItem,
    BranchParentItem, BranchStatus, Bytes, Command, EventBatchAppendItemResult,
    EventChainVerification, EventData, EventRangeDirection, EventVersionedData,
    GraphBatchItemResult, GraphBatchOperation, GraphBindingHit, GraphBindingPrimitive,
    GraphBindingTarget, GraphDirection, GraphEdgeData, GraphEdgeDataOutput, GraphEntityBinding,
    GraphInfoData, GraphNeighborHit, GraphNodeData, GraphNodeDataOutput, HistoryItem,
    JsonBatchGetItemResult, JsonBatchItemResult, JsonHistoryItem, JsonIndexDefinition,
    JsonIndexType, JsonSampleItem, JsonVersionedValue, Output, SampleItem, ScanItem,
    VectorBatchGetItemResult, VectorBatchItemResult, VectorCollectionInfo, VectorData,
    VectorDistanceMetric, VectorFilterCondition, VectorFilterOp, VectorHistoryItem, VectorMatch,
    VectorMetadataFilter, VectorScalar, VectorVersionedData, VersionedValue,
};

#[test]
fn every_command_round_trips_through_json() {
    for command in command_round_trip_cases() {
        let encoded = serde_json::to_string(&command).expect("command serializes");
        let decoded: Command = serde_json::from_str(&encoded).expect("command deserializes");
        assert_eq!(decoded, command);
    }
}

#[test]
fn every_output_round_trips_through_json() {
    for output in all_outputs() {
        let encoded = serde_json::to_string(&output).expect("output serializes");
        let decoded: Output = serde_json::from_str(&encoded).expect("output deserializes");
        assert_eq!(decoded, output);
    }
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
        events: Vec::new(),
        has_more: false,
        cursor: None,
    };
    let encoded = serde_json::to_value(&output).expect("output serializes");
    assert_eq!(encoded["type"], "event_range_result");
    assert_eq!(encoded["data"]["events"], Value::Array(Vec::new()));
    assert_eq!(encoded["data"]["has_more"], false);
    assert_eq!(encoded["data"]["cursor"], Value::Null);
    assert_eq!(
        serde_json::from_value::<Output>(encoded).expect("output deserializes"),
        output
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
        neighbors: vec![GraphNeighborHit::new(
            graph_node_output("deps", "node-b"),
            graph_edge_output("deps", "node-a", "depends_on", "node-b"),
            GraphDirection::Outgoing,
        )],
        has_more: false,
        cursor: None,
    };
    let encoded = serde_json::to_value(&output).expect("output serializes");
    assert_eq!(encoded["type"], "graph_neighbor_page");
    assert_eq!(encoded["data"]["neighbors"][0]["direction"], "outgoing");
    assert_eq!(encoded["data"]["neighbors"][0]["node"]["node_id"], "node-b");
    assert_eq!(
        encoded["data"]["neighbors"][0]["edge"]["edge_type"],
        "depends_on"
    );
    assert_eq!(
        serde_json::from_value::<Output>(encoded).expect("output deserializes"),
        output
    );
}

#[test]
fn command_names_cover_every_variant() {
    let names = all_commands()
        .into_iter()
        .map(|command| command.name())
        .collect::<Vec<_>>();

    assert_eq!(
        names,
        vec![
            "branch_list",
            "branch_get",
            "branch_create",
            "branch_fork_current",
            "branch_fork_at_version",
            "branch_fork_at_timestamp",
            "branch_delete",
            "kv_put",
            "kv_get",
            "kv_delete",
            "kv_list",
            "kv_scan",
            "kv_batch_put",
            "kv_batch_get",
            "kv_batch_delete",
            "kv_batch_exists",
            "kv_exists",
            "kv_getv",
            "kv_count",
            "kv_sample",
            "json_set",
            "json_get",
            "json_delete",
            "json_getv",
            "json_exists",
            "json_batch_set",
            "json_batch_get",
            "json_batch_delete",
            "json_list",
            "json_count",
            "json_sample",
            "json_create_index",
            "json_drop_index",
            "json_list_indexes",
            "vector_create_collection",
            "vector_delete_collection",
            "vector_list_collections",
            "vector_collection_stats",
            "vector_count",
            "vector_upsert",
            "vector_get",
            "vector_getv",
            "vector_exists",
            "vector_list_keys",
            "vector_update_metadata",
            "vector_delete",
            "vector_delete_by_filter",
            "vector_delete_all",
            "vector_query",
            "vector_batch_upsert",
            "vector_batch_get",
            "vector_batch_delete",
            "event_batch_append",
            "event_append",
            "event_get",
            "event_exists",
            "event_get_by_type",
            "event_len",
            "event_range",
            "event_range_by_time",
            "event_list_types",
            "event_list",
            "event_verify_chain",
            "graph_create",
            "graph_delete",
            "graph_list",
            "graph_get_meta",
            "graph_add_node",
            "graph_get_node",
            "graph_remove_node",
            "graph_list_nodes",
            "graph_add_edge",
            "graph_get_edge",
            "graph_remove_edge",
            "graph_neighbors",
            "graph_bindings_for_entity",
            "graph_batch_write",
            "arrow_import",
            "arrow_export",
        ]
    );
}

fn all_commands() -> Vec<Command> {
    let mut commands = branch_commands();
    commands.extend(kv_commands());
    commands.extend(json_commands());
    commands.extend(vector_commands());
    commands.extend(event_commands());
    commands.extend(graph_commands());
    commands.extend(arrow_commands());
    commands
}

fn command_round_trip_cases() -> Vec<Command> {
    let mut commands = all_commands();
    commands.extend(json_round_trip_edge_commands());
    commands.extend(vector_round_trip_edge_commands());
    commands.extend(event_round_trip_edge_commands());
    commands.extend(graph_round_trip_edge_commands());
    commands.extend(graph_binding_target_round_trip_commands());
    commands
}

fn branch_commands() -> Vec<Command> {
    vec![
        Command::BranchList,
        Command::BranchGet {
            branch: "main".to_owned(),
        },
        Command::BranchCreate {
            branch: "scratch".to_owned(),
        },
        Command::BranchForkCurrent {
            source: "default".to_owned(),
            branch: "feature".to_owned(),
        },
        Command::BranchForkAtVersion {
            source: "default".to_owned(),
            branch: "by-version".to_owned(),
            version: 7,
        },
        Command::BranchForkAtTimestamp {
            source: "default".to_owned(),
            branch: "by-time".to_owned(),
            timestamp: 99,
        },
        Command::BranchDelete {
            branch: "scratch".to_owned(),
        },
    ]
}

fn kv_commands() -> Vec<Command> {
    vec![
        Command::KvPut {
            branch: None,
            space: None,
            key: bytes("alpha"),
            value: bytes("one"),
        },
        Command::KvGet {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            key: bytes("alpha"),
            as_of: Some(42),
        },
        Command::KvDelete {
            branch: Some("feature".to_owned()),
            space: None,
            key: bytes("alpha"),
        },
        Command::KvList {
            branch: None,
            space: Some("space-a".to_owned()),
            prefix: Some(bytes("a")),
            cursor: Some(bytes("alpha")),
            limit: Some(2),
            as_of: Some(99),
        },
        Command::KvScan {
            branch: None,
            space: None,
            start: Some(bytes("a")),
            limit: Some(10),
        },
        Command::KvBatchPut {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            entries: vec![BatchKvEntry::new(bytes("a"), bytes("one"))],
        },
        Command::KvBatchGet {
            branch: None,
            space: None,
            keys: vec![bytes("a"), bytes("missing")],
        },
        Command::KvBatchDelete {
            branch: None,
            space: None,
            keys: vec![bytes("a"), bytes("missing")],
        },
        Command::KvBatchExists {
            branch: None,
            space: None,
            keys: vec![bytes("a"), bytes("missing")],
        },
        Command::KvExists {
            branch: None,
            space: None,
            key: bytes("a"),
        },
        Command::KvGetv {
            branch: None,
            space: None,
            key: bytes("a"),
        },
        Command::KvCount {
            branch: None,
            space: None,
            prefix: Some(bytes("a")),
        },
        Command::KvSample {
            branch: None,
            space: None,
            prefix: Some(bytes("a")),
            count: Some(4),
        },
    ]
}

fn json_commands() -> Vec<Command> {
    vec![
        Command::JsonSet {
            branch: None,
            space: None,
            key: "doc-a".to_owned(),
            path: "$.name".to_owned(),
            value: json!("Ada"),
        },
        Command::JsonGet {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            key: "doc-a".to_owned(),
            path: "$.name".to_owned(),
            as_of: Some(42),
        },
        Command::JsonDelete {
            branch: None,
            space: None,
            key: "doc-a".to_owned(),
            path: "$.name".to_owned(),
        },
        Command::JsonGetv {
            branch: None,
            space: None,
            key: "doc-a".to_owned(),
        },
        Command::JsonExists {
            branch: None,
            space: None,
            key: "doc-a".to_owned(),
        },
        Command::JsonBatchSet {
            branch: None,
            space: None,
            entries: vec![BatchJsonEntry::new("doc-a", "$.name", json!("Ada"))],
        },
        Command::JsonBatchGet {
            branch: None,
            space: None,
            entries: vec![BatchJsonGetEntry::new("doc-a", "$.name")],
        },
        Command::JsonBatchDelete {
            branch: None,
            space: None,
            entries: vec![BatchJsonDeleteEntry::new("doc-a", "$.name")],
        },
        Command::JsonList {
            branch: None,
            space: None,
            prefix: Some("doc-".to_owned()),
            cursor: Some("doc-a".to_owned()),
            limit: Some(2),
            as_of: Some(99),
        },
        Command::JsonCount {
            branch: None,
            space: None,
            prefix: Some("doc-".to_owned()),
        },
        Command::JsonSample {
            branch: None,
            space: None,
            prefix: Some("doc-".to_owned()),
            count: Some(3),
        },
        Command::JsonCreateIndex {
            branch: None,
            space: None,
            name: "by-name".to_owned(),
            field_path: "$.name".to_owned(),
            index_type: JsonIndexType::Text,
        },
        Command::JsonDropIndex {
            branch: None,
            space: None,
            name: "by-name".to_owned(),
        },
        Command::JsonListIndexes {
            branch: None,
            space: None,
        },
    ]
}

fn vector_commands() -> Vec<Command> {
    let mut commands = vector_collection_commands();
    commands.extend(vector_row_commands());
    commands.extend(vector_bulk_commands());
    commands
}

fn vector_collection_commands() -> Vec<Command> {
    vec![
        Command::VectorCreateCollection {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
            dimension: 2,
            metric: VectorDistanceMetric::Cosine,
        },
        Command::VectorDeleteCollection {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
        },
        Command::VectorListCollections {
            branch: None,
            space: None,
        },
        Command::VectorCollectionStats {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
        },
        Command::VectorCount {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
        },
    ]
}

fn vector_row_commands() -> Vec<Command> {
    vec![
        Command::VectorUpsert {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            collection: "docs".to_owned(),
            key: "doc-a".to_owned(),
            vector: vec![1.0, 0.0],
            metadata: Some(json!({"kind": "doc"})),
        },
        Command::VectorGet {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
            key: "doc-a".to_owned(),
            as_of: Some(42),
        },
        Command::VectorGetv {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
            key: "doc-a".to_owned(),
        },
        Command::VectorExists {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
            key: "doc-a".to_owned(),
        },
        Command::VectorListKeys {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
            prefix: Some("doc-".to_owned()),
            cursor: Some("doc-a".to_owned()),
            limit: Some(2),
        },
        Command::VectorUpdateMetadata {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
            key: "doc-a".to_owned(),
            patch: json!({"rank": 2}),
        },
        Command::VectorDelete {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
            key: "doc-a".to_owned(),
        },
    ]
}

fn vector_bulk_commands() -> Vec<Command> {
    vec![
        Command::VectorDeleteByFilter {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
            filter: VectorMetadataFilter::new(vec![VectorFilterCondition::new(
                "kind",
                VectorFilterOp::Eq,
                VectorScalar::from("doc"),
            )]),
        },
        Command::VectorDeleteAll {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
        },
        Command::VectorQuery {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
            query: vec![1.0, 0.0],
            k: 10,
            filter: Some(VectorMetadataFilter::new(vec![VectorFilterCondition::eq(
                "kind", "doc",
            )])),
            as_of: Some(99),
        },
        Command::VectorBatchUpsert {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
            entries: vec![BatchVectorEntry::new(
                "doc-a",
                vec![1.0, 0.0],
                Some(json!({"kind": "doc"})),
            )],
        },
        Command::VectorBatchGet {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
            keys: vec!["doc-a".to_owned(), "missing".to_owned()],
        },
        Command::VectorBatchDelete {
            branch: None,
            space: None,
            collection: "docs".to_owned(),
            keys: vec!["doc-a".to_owned(), "missing".to_owned()],
        },
    ]
}

fn event_commands() -> Vec<Command> {
    vec![
        Command::EventBatchAppend {
            branch: None,
            space: None,
            entries: vec![BatchEventEntry::new("user.created", json!({"id": 1}))],
        },
        Command::EventAppend {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            event_type: "user.updated".to_owned(),
            payload: json!({"id": 1, "name": "Ada"}),
        },
        Command::EventGet {
            branch: None,
            space: None,
            sequence: 0,
            as_of: Some(99),
        },
        Command::EventExists {
            branch: None,
            space: None,
            sequence: 0,
        },
        Command::EventGetByType {
            branch: None,
            space: None,
            event_type: "user.created".to_owned(),
            limit: Some(2),
            after_sequence: Some(0),
            as_of: Some(99),
        },
        Command::EventLen {
            branch: None,
            space: None,
            as_of: Some(99),
        },
        Command::EventRange {
            branch: None,
            space: None,
            start_seq: 0,
            end_seq: Some(10),
            limit: Some(5),
            direction: EventRangeDirection::Forward,
            event_type: Some("user.created".to_owned()),
        },
        Command::EventRangeByTime {
            branch: None,
            space: None,
            start_ts: 1,
            end_ts: Some(99),
            limit: Some(5),
            direction: EventRangeDirection::Reverse,
            event_type: Some("user.created".to_owned()),
        },
        Command::EventListTypes {
            branch: None,
            space: None,
            as_of: Some(99),
        },
        Command::EventList {
            branch: None,
            space: None,
            event_type: Some("user.created".to_owned()),
            limit: Some(5),
            as_of: Some(99),
        },
        Command::EventVerifyChain {
            branch: None,
            space: None,
        },
    ]
}

#[allow(clippy::too_many_lines)]
fn graph_commands() -> Vec<Command> {
    vec![
        Command::GraphCreate {
            branch: None,
            space: None,
            graph: "deps".to_owned(),
        },
        Command::GraphDelete {
            branch: None,
            space: None,
            graph: "deps".to_owned(),
        },
        Command::GraphList {
            branch: None,
            space: None,
            cursor: Some("deps".to_owned()),
            limit: Some(5),
        },
        Command::GraphGetMeta {
            branch: None,
            space: None,
            graph: "deps".to_owned(),
        },
        Command::GraphAddNode {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            graph: "deps".to_owned(),
            node_id: "node-a".to_owned(),
            properties: Some(json!({"kind": "root"})),
            binding: Some(graph_binding()),
        },
        Command::GraphGetNode {
            branch: None,
            space: None,
            graph: "deps".to_owned(),
            node_id: "node-a".to_owned(),
        },
        Command::GraphRemoveNode {
            branch: None,
            space: None,
            graph: "deps".to_owned(),
            node_id: "node-a".to_owned(),
        },
        Command::GraphListNodes {
            branch: None,
            space: None,
            graph: "deps".to_owned(),
            prefix: Some("node-".to_owned()),
            cursor: Some("node-a".to_owned()),
            limit: Some(5),
        },
        Command::GraphAddEdge {
            branch: None,
            space: None,
            graph: "deps".to_owned(),
            src: "node-a".to_owned(),
            edge_type: "depends_on".to_owned(),
            dst: "node-b".to_owned(),
            weight: Some(2.5),
            properties: Some(json!({"source": "test"})),
        },
        Command::GraphGetEdge {
            branch: None,
            space: None,
            graph: "deps".to_owned(),
            src: "node-a".to_owned(),
            edge_type: "depends_on".to_owned(),
            dst: "node-b".to_owned(),
        },
        Command::GraphRemoveEdge {
            branch: None,
            space: None,
            graph: "deps".to_owned(),
            src: "node-a".to_owned(),
            edge_type: "depends_on".to_owned(),
            dst: "node-b".to_owned(),
        },
        Command::GraphNeighbors {
            branch: None,
            space: None,
            graph: "deps".to_owned(),
            node_id: "node-a".to_owned(),
            direction: GraphDirection::Outgoing,
            edge_type: Some("depends_on".to_owned()),
            cursor: None,
            limit: Some(5),
        },
        Command::GraphBindingsForEntity {
            branch: None,
            space: None,
            target: graph_binding_target(),
            cursor: None,
            limit: Some(5),
        },
        Command::GraphBatchWrite {
            branch: None,
            space: None,
            graph: "deps".to_owned(),
            operations: Vec::new(),
        },
    ]
}

fn arrow_commands() -> Vec<Command> {
    vec![
        Command::ArrowImport {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            file_path: "input.parquet".to_owned(),
            format: Some(ArrowFileFormat::Parquet),
            target: ArrowImportTarget::Kv,
            key_column: Some("id".to_owned()),
            value_column: Some("payload".to_owned()),
            collection: None,
        },
        Command::ArrowExport {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            primitive: ArrowExportPrimitive::Vector,
            format: ArrowFileFormat::Jsonl,
            path: "output.jsonl".to_owned(),
            prefix: Some("doc-".to_owned()),
            limit: Some(100),
            collection: Some("docs".to_owned()),
            graph: None,
            event_type: None,
        },
    ]
}

fn json_round_trip_edge_commands() -> Vec<Command> {
    vec![
        Command::JsonSet {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            key: "doc-root".to_owned(),
            path: "$".to_owned(),
            value: json!({"name": "Ada", "tags": ["math"], "active": true}),
        },
        Command::JsonSet {
            branch: None,
            space: None,
            key: "doc-array".to_owned(),
            path: "$.tags".to_owned(),
            value: json!(["a", "b"]),
        },
        Command::JsonBatchSet {
            branch: None,
            space: None,
            entries: Vec::new(),
        },
        Command::JsonBatchSet {
            branch: None,
            space: None,
            entries: vec![
                BatchJsonEntry::new("", "$", json!("bad")),
                BatchJsonEntry::new("doc-a", "$[", json!({"bad": true})),
                BatchJsonEntry::new("doc-b", "$.nested", json!({"ok": true})),
            ],
        },
        Command::JsonBatchGet {
            branch: None,
            space: None,
            entries: Vec::new(),
        },
        Command::JsonBatchGet {
            branch: None,
            space: None,
            entries: vec![
                BatchJsonGetEntry::new("", "$"),
                BatchJsonGetEntry::new("doc-a", "$["),
            ],
        },
        Command::JsonBatchDelete {
            branch: None,
            space: None,
            entries: Vec::new(),
        },
        Command::JsonBatchDelete {
            branch: None,
            space: None,
            entries: vec![
                BatchJsonDeleteEntry::new("", "$"),
                BatchJsonDeleteEntry::new("doc-a", "$["),
            ],
        },
        Command::JsonCreateIndex {
            branch: None,
            space: None,
            name: "by-age".to_owned(),
            field_path: "$.age".to_owned(),
            index_type: JsonIndexType::Numeric,
        },
        Command::JsonCreateIndex {
            branch: None,
            space: None,
            name: "by-tag".to_owned(),
            field_path: "$.tag".to_owned(),
            index_type: JsonIndexType::Tag,
        },
    ]
}

fn vector_round_trip_edge_commands() -> Vec<Command> {
    vec![
        Command::VectorCreateCollection {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            collection: "cosine".to_owned(),
            dimension: 3,
            metric: VectorDistanceMetric::Cosine,
        },
        Command::VectorCreateCollection {
            branch: None,
            space: None,
            collection: "euclidean".to_owned(),
            dimension: 3,
            metric: VectorDistanceMetric::Euclidean,
        },
        Command::VectorCreateCollection {
            branch: None,
            space: None,
            collection: "dot".to_owned(),
            dimension: 3,
            metric: VectorDistanceMetric::DotProduct,
        },
        Command::VectorUpsert {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            collection: "cosine".to_owned(),
            key: "mixed".to_owned(),
            vector: vec![0.0, 1.5, -2.0],
            metadata: Some(json!({})),
        },
        Command::VectorListKeys {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            collection: "cosine".to_owned(),
            prefix: Some("doc-".to_owned()),
            cursor: Some("doc-001".to_owned()),
            limit: Some(25),
        },
        Command::VectorUpdateMetadata {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            collection: "cosine".to_owned(),
            key: "mixed".to_owned(),
            patch: json!({"rank": 3, "active": true, "tag": null}),
        },
        Command::VectorDeleteByFilter {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            collection: "cosine".to_owned(),
            filter: VectorMetadataFilter::new(vec![
                VectorFilterCondition::eq("tag", "doc"),
                VectorFilterCondition::new("rank", VectorFilterOp::Eq, VectorScalar::from(3)),
                VectorFilterCondition::new("active", VectorFilterOp::Eq, VectorScalar::from(true)),
                VectorFilterCondition::new("empty", VectorFilterOp::Eq, VectorScalar::Null),
            ]),
        },
        Command::VectorQuery {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            collection: "cosine".to_owned(),
            query: vec![0.0, 1.5, -2.0],
            k: 3,
            filter: Some(VectorMetadataFilter::new(vec![VectorFilterCondition::eq(
                "tag", "doc",
            )])),
            as_of: Some(123),
        },
        Command::VectorBatchUpsert {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            collection: "cosine".to_owned(),
            entries: Vec::new(),
        },
        Command::VectorBatchGet {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            collection: "cosine".to_owned(),
            keys: Vec::new(),
        },
        Command::VectorBatchDelete {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            collection: "cosine".to_owned(),
            keys: Vec::new(),
        },
    ]
}

fn event_round_trip_edge_commands() -> Vec<Command> {
    vec![
        Command::EventAppend {
            branch: None,
            space: None,
            event_type: "empty.object".to_owned(),
            payload: json!({}),
        },
        Command::EventAppend {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            event_type: "nested.object".to_owned(),
            payload: json!({
                "scalars": [true, false, 7, "value", null],
                "object": {"nested": [{"id": 1}, {"id": 2}]},
            }),
        },
        Command::EventBatchAppend {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            entries: Vec::new(),
        },
        Command::EventBatchAppend {
            branch: None,
            space: None,
            entries: vec![
                BatchEventEntry::new("", json!({"bad": true})),
                BatchEventEntry::new("bad.payload", json!(["not", "an", "object"])),
                BatchEventEntry::new("audit.recorded", json!({"ok": true})),
            ],
        },
        Command::EventRange {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            start_seq: 0,
            end_seq: None,
            limit: Some(0),
            direction: EventRangeDirection::Reverse,
            event_type: None,
        },
        Command::EventRangeByTime {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            start_ts: 0,
            end_ts: None,
            limit: Some(0),
            direction: EventRangeDirection::Forward,
            event_type: None,
        },
        Command::EventList {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            event_type: None,
            limit: Some(0),
            as_of: None,
        },
    ]
}

fn graph_round_trip_edge_commands() -> Vec<Command> {
    vec![
        Command::GraphCreate {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            graph: "wide".to_owned(),
        },
        Command::GraphAddNode {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            graph: "wide".to_owned(),
            node_id: "node-empty".to_owned(),
            properties: Some(json!({})),
            binding: None,
        },
        Command::GraphAddEdge {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            graph: "wide".to_owned(),
            src: "node-a".to_owned(),
            edge_type: "relates_to".to_owned(),
            dst: "node-b".to_owned(),
            weight: None,
            properties: None,
        },
        Command::GraphNeighbors {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            graph: "wide".to_owned(),
            node_id: "node-a".to_owned(),
            direction: GraphDirection::Incoming,
            edge_type: None,
            cursor: Some("cursor".to_owned()),
            limit: Some(0),
        },
        Command::GraphNeighbors {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            graph: "wide".to_owned(),
            node_id: "node-a".to_owned(),
            direction: GraphDirection::Both,
            edge_type: Some("relates_to".to_owned()),
            cursor: None,
            limit: Some(1),
        },
        Command::GraphBatchWrite {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            graph: "wide".to_owned(),
            operations: vec![
                GraphBatchOperation::UpsertNode {
                    node_id: "node-a".to_owned(),
                    data: GraphNodeData::new(None, Some(graph_binding())),
                },
                GraphBatchOperation::DeleteNode {
                    node_id: "node-old".to_owned(),
                },
                GraphBatchOperation::UpsertEdge {
                    src: "node-a".to_owned(),
                    edge_type: "relates_to".to_owned(),
                    dst: "node-b".to_owned(),
                    data: GraphEdgeData::new(Some(1.25), Some(json!({"batch": true}))),
                },
                GraphBatchOperation::DeleteEdge {
                    src: "node-a".to_owned(),
                    edge_type: "relates_to".to_owned(),
                    dst: "node-b".to_owned(),
                },
            ],
        },
    ]
}

fn graph_binding_target_round_trip_commands() -> Vec<Command> {
    [
        GraphBindingPrimitive::Kv,
        GraphBindingPrimitive::Json,
        GraphBindingPrimitive::Vector,
        GraphBindingPrimitive::Event,
        GraphBindingPrimitive::Graph,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, primitive)| Command::GraphBindingsForEntity {
        branch: Some("feature".to_owned()),
        space: Some("space-a".to_owned()),
        target: GraphBindingTarget::new(
            primitive,
            (index % 2 == 0).then(|| "entity-branch".to_owned()),
            "entity-space",
            format!("entity-{index}"),
        ),
        cursor: Some(format!("cursor-{index}")),
        limit: Some(10),
    })
    .collect()
}

fn all_outputs() -> Vec<Output> {
    let mut outputs = branch_outputs();
    outputs.extend(kv_outputs());
    outputs.extend(json_outputs());
    outputs.extend(vector_outputs());
    outputs.extend(event_outputs());
    outputs.extend(graph_outputs());
    outputs.extend(arrow_outputs());
    outputs
}

fn branch_outputs() -> Vec<Output> {
    vec![
        Output::Branch(branch_item("main")),
        Output::Branches(vec![branch_item("default"), branch_item("main")]),
        Output::BranchDeleteResult {
            branch: branch_item("scratch"),
            generation_before: Some(1),
            generation_after: Some(1),
            cleanup: Some(BranchCleanupItem::new(0, 0, 0)),
        },
    ]
}

fn kv_outputs() -> Vec<Output> {
    vec![
        Output::KvValue(Some(bytes("one"))),
        Output::KvValue(None),
        Output::KvVersionedValue(Some(VersionedValue::new(bytes("one"), 1, 10))),
        Output::KvVersionedValue(None),
        Output::VersionHistory(Some(vec![HistoryItem::new(
            Some(bytes("one")),
            false,
            1,
            10,
        )])),
        Output::VersionHistory(None),
        Output::Keys(vec![bytes("a"), bytes("b")]),
        Output::KeysPage {
            keys: vec![bytes("a")],
            has_more: true,
            cursor: Some(bytes("a")),
        },
        Output::WriteResult {
            key: bytes("a"),
            version: 1,
            timestamp: 10,
        },
        Output::DeleteResult {
            key: bytes("a"),
            deleted: true,
            version: Some(2),
            timestamp: Some(20),
        },
        Output::KvScanResult(vec![ScanItem::new(bytes("a"), bytes("one"), 1, 10)]),
        Output::BatchResults(vec![BatchItemResult::new(
            bytes("a"),
            true,
            Some(1),
            Some(10),
        )]),
        Output::BatchResults(vec![BatchItemResult::failed(
            Bytes::new(Vec::new()),
            "invalid key",
        )]),
        Output::BatchGetResults(vec![BatchGetItemResult::new(
            bytes("a"),
            Some(bytes("one")),
            Some(1),
            Some(10),
        )]),
        Output::BatchGetResults(vec![BatchGetItemResult::failed(
            Bytes::new(Vec::new()),
            "invalid key",
        )]),
        Output::Bool(true),
        Output::BoolList(vec![true, false]),
        Output::Uint(2),
        Output::SampleResult {
            total_count: 3,
            items: vec![SampleItem::new(bytes("a"), bytes("one"), 1, 10)],
        },
    ]
}

fn json_outputs() -> Vec<Output> {
    vec![
        Output::JsonValue(Some(json!({"name": "Ada"}))),
        Output::JsonValue(None),
        Output::JsonVersionedValue(Some(JsonVersionedValue::new(
            json!({"name": "Ada"}),
            1,
            10,
            2,
        ))),
        Output::JsonVersionedValue(None),
        Output::JsonVersionHistory(Some(vec![JsonHistoryItem::new(
            Some(json!({"name": "Ada"})),
            1,
            10,
            Some(2),
            false,
        )])),
        Output::JsonVersionHistory(Some(vec![JsonHistoryItem::new(None, 2, 20, None, true)])),
        Output::JsonVersionHistory(None),
        Output::JsonListResult {
            keys: vec!["doc-a".to_owned()],
            has_more: true,
            cursor: Some("doc-a".to_owned()),
        },
        Output::JsonBatchResults(vec![JsonBatchItemResult::new(Some(1), Some(10), Some(2))]),
        Output::JsonBatchResults(vec![JsonBatchItemResult::failed("invalid document id")]),
        Output::JsonBatchGetResults(vec![JsonBatchGetItemResult::new(
            Some(json!("Ada")),
            Some(1),
            Some(10),
            Some(2),
        )]),
        Output::JsonBatchGetResults(vec![JsonBatchGetItemResult::failed("invalid document id")]),
        Output::JsonSampleResult {
            total_count: 3,
            items: vec![JsonSampleItem::new(
                "doc-a".to_owned(),
                json!({"name": "Ada"}),
                1,
                10,
                2,
            )],
        },
        Output::JsonIndexDefinition(json_index_definition("by-name", JsonIndexType::Text)),
        Output::JsonIndexList(vec![
            json_index_definition("by-age", JsonIndexType::Numeric),
            json_index_definition("by-name", JsonIndexType::Tag),
            json_index_definition("by-bio", JsonIndexType::Text),
        ]),
    ]
}

fn vector_outputs() -> Vec<Output> {
    vec![
        Output::VectorWriteResult {
            collection: "docs".to_owned(),
            key: "doc-a".to_owned(),
            version: 1,
            timestamp: 10,
            vector_revision: 1,
        },
        Output::VectorMetadataUpdateResult {
            collection: "docs".to_owned(),
            key: "doc-a".to_owned(),
            updated: true,
            version: Some(2),
            timestamp: Some(20),
            vector_revision: Some(2),
        },
        Output::VectorDeleteResult {
            collection: "docs".to_owned(),
            key: "doc-a".to_owned(),
            deleted: true,
            version: Some(3),
            timestamp: Some(30),
        },
        Output::VectorBulkDeleteResult {
            collection: "docs".to_owned(),
            deleted_count: 2,
            version: Some(4),
            timestamp: Some(40),
        },
        Output::VectorData(Some(VectorVersionedData::new(
            "doc-a".to_owned(),
            VectorData::new(vec![1.0, 0.0], Some(json!({"kind": "doc"}))),
            1,
            10,
            1,
        ))),
        Output::VectorData(None),
        Output::VectorVersionHistory(Some(vec![
            VectorHistoryItem::new(
                "doc-a".to_owned(),
                Some(VectorData::new(vec![1.0, 0.0], None)),
                1,
                10,
                Some(1),
                false,
            ),
            VectorHistoryItem::new("doc-a".to_owned(), None, 2, 20, None, true),
        ])),
        Output::VectorVersionHistory(None),
        Output::VectorMatches(vec![VectorMatch::new(
            "doc-a".to_owned(),
            1.0,
            Some(json!({"kind": "doc"})),
        )]),
        Output::VectorKeyPage {
            keys: vec!["doc-a".to_owned()],
            has_more: true,
            cursor: Some("doc-a".to_owned()),
        },
        Output::VectorCollectionList(vec![VectorCollectionInfo::new(
            "docs".to_owned(),
            2,
            VectorDistanceMetric::Cosine,
            1,
        )]),
        Output::VectorBatchUpsertResults(vec![VectorBatchItemResult::new(
            true,
            Some(1),
            Some(10),
            Some(1),
        )]),
        Output::VectorBatchUpsertResults(vec![VectorBatchItemResult::failed("invalid vector")]),
        Output::VectorBatchGetResults(vec![VectorBatchGetItemResult::new(Some(
            VectorVersionedData::new(
                "doc-a".to_owned(),
                VectorData::new(vec![1.0, 0.0], None),
                1,
                10,
                1,
            ),
        ))]),
        Output::VectorBatchGetResults(vec![VectorBatchGetItemResult::failed("invalid vector key")]),
        Output::VectorBatchDeleteResults(vec![VectorBatchItemResult::new(
            true,
            Some(2),
            Some(20),
            None,
        )]),
    ]
}

fn event_outputs() -> Vec<Output> {
    vec![
        Output::EventAppendResult {
            sequence: 0,
            event_type: "user.created".to_owned(),
            version: 1,
            timestamp: 10,
        },
        Output::EventRecord(Some(event_versioned_data(0, "user.created", 1, 10))),
        Output::EventRecord(None),
        Output::EventRecords(vec![
            event_versioned_data(0, "user.created", 1, 10),
            event_versioned_data(1, "user.updated", 2, 20),
        ]),
        Output::EventLength { count: 2 },
        Output::EventTypeList(vec!["user.created".to_owned(), "user.updated".to_owned()]),
        Output::EventRangeResult {
            events: vec![event_versioned_data(0, "user.created", 1, 10)],
            has_more: true,
            cursor: Some(0),
        },
        Output::EventRangeResult {
            events: Vec::new(),
            has_more: false,
            cursor: None,
        },
        Output::EventBatchAppendResults(vec![EventBatchAppendItemResult::new(
            Some(0),
            Some("user.created".to_owned()),
            Some(1),
            Some(10),
        )]),
        Output::EventBatchAppendResults(vec![EventBatchAppendItemResult::failed("invalid event")]),
        Output::EventChainVerification(EventChainVerification::new(true, 2, None, None)),
        Output::EventChainVerification(EventChainVerification::new(
            false,
            2,
            Some(1),
            Some("hash mismatch".to_owned()),
        )),
    ]
}

fn graph_outputs() -> Vec<Output> {
    let mut outputs = graph_read_outputs();
    outputs.extend(graph_write_outputs());
    outputs
}

fn graph_read_outputs() -> Vec<Output> {
    vec![
        Output::GraphInfo(GraphInfoData::new("deps".to_owned(), 2, 1, 1, 10, 4, 40)),
        Output::GraphInfoResult(Some(GraphInfoData::new(
            "deps".to_owned(),
            2,
            1,
            1,
            10,
            4,
            40,
        ))),
        Output::GraphInfoResult(None),
        Output::GraphNamePage {
            graphs: vec!["deps".to_owned()],
            has_more: true,
            cursor: Some("deps".to_owned()),
        },
        Output::GraphNamePage {
            graphs: Vec::new(),
            has_more: false,
            cursor: None,
        },
        Output::GraphNodeResult(Some(graph_node_output("deps", "node-a"))),
        Output::GraphNodeResult(None),
        Output::GraphNodePage {
            nodes: vec![graph_node_output("deps", "node-a")],
            has_more: true,
            cursor: Some("node-a".to_owned()),
        },
        Output::GraphNodePage {
            nodes: Vec::new(),
            has_more: false,
            cursor: None,
        },
        Output::GraphEdgeResult(Some(graph_edge_output(
            "deps",
            "node-a",
            "depends_on",
            "node-b",
        ))),
        Output::GraphEdgeResult(None),
        Output::GraphNeighborPage {
            neighbors: vec![GraphNeighborHit::new(
                graph_node_output("deps", "node-b"),
                graph_edge_output("deps", "node-a", "depends_on", "node-b"),
                GraphDirection::Outgoing,
            )],
            has_more: false,
            cursor: None,
        },
        Output::GraphNeighborPage {
            neighbors: vec![GraphNeighborHit::new(
                graph_node_output("deps", "node-a"),
                graph_edge_output("deps", "node-a", "depends_on", "node-b"),
                GraphDirection::Incoming,
            )],
            has_more: true,
            cursor: Some("incoming:node-a".to_owned()),
        },
        Output::GraphBindingPage {
            bindings: vec![GraphBindingHit::new(
                "deps".to_owned(),
                "node-a".to_owned(),
                graph_binding(),
                2,
                20,
            )],
            has_more: false,
            cursor: None,
        },
        Output::GraphBindingPage {
            bindings: Vec::new(),
            has_more: false,
            cursor: None,
        },
    ]
}

fn graph_write_outputs() -> Vec<Output> {
    vec![
        Output::GraphNodeWriteResult {
            graph: "deps".to_owned(),
            node_id: "node-a".to_owned(),
            created: true,
            version: 2,
            timestamp: 20,
        },
        Output::GraphNodeWriteResult {
            graph: "deps".to_owned(),
            node_id: "node-a".to_owned(),
            created: false,
            version: 3,
            timestamp: 30,
        },
        Output::GraphEdgeWriteResult {
            graph: "deps".to_owned(),
            src: "node-a".to_owned(),
            edge_type: "depends_on".to_owned(),
            dst: "node-b".to_owned(),
            created: true,
            version: 3,
            timestamp: 30,
        },
        Output::GraphEdgeWriteResult {
            graph: "deps".to_owned(),
            src: "node-a".to_owned(),
            edge_type: "depends_on".to_owned(),
            dst: "node-b".to_owned(),
            created: false,
            version: 4,
            timestamp: 40,
        },
        Output::GraphDeleteResult {
            graph: "deps".to_owned(),
            node_id: Some("node-a".to_owned()),
            src: None,
            edge_type: None,
            dst: None,
            deleted: true,
            version: Some(4),
            timestamp: Some(40),
        },
        Output::GraphDeleteResult {
            graph: "deps".to_owned(),
            node_id: None,
            src: Some("node-a".to_owned()),
            edge_type: Some("depends_on".to_owned()),
            dst: Some("node-b".to_owned()),
            deleted: false,
            version: None,
            timestamp: None,
        },
        Output::GraphBatchWriteResult {
            graph: "deps".to_owned(),
            results: vec![
                GraphBatchItemResult::new(0, "upsert_node", Some(true), None, Some(5), Some(50)),
                GraphBatchItemResult::new(1, "delete_edge", None, Some(false), None, None),
                GraphBatchItemResult::failed(2, "upsert_edge", "invalid graph edge"),
            ],
            version: Some(5),
            timestamp: Some(50),
        },
    ]
}

fn arrow_outputs() -> Vec<Output> {
    vec![
        Output::ArrowImportResult(ArrowImportResult::new(
            ArrowImportTarget::Kv,
            "input.parquet".to_owned(),
            10,
            1,
            2,
        )),
        Output::ArrowExportResult(ArrowExportResult::new(
            ArrowExportPrimitive::Graph,
            ArrowFileFormat::Parquet,
            vec![
                "graph_nodes.parquet".to_owned(),
                "graph_edges.parquet".to_owned(),
            ],
            11,
            1024,
        )),
    ]
}

fn bytes(value: &str) -> Bytes {
    Bytes::from(value)
}

fn event_versioned_data(
    sequence: u64,
    event_type: &str,
    version: u64,
    timestamp: u64,
) -> EventVersionedData {
    EventVersionedData::new(
        EventData::new(
            sequence,
            event_type.to_owned(),
            json!({"sequence": sequence, "nested": [{"ok": true}], "empty": {}}),
            timestamp,
            "00".repeat(32),
            "11".repeat(32),
        ),
        version,
        timestamp,
    )
}

fn branch_item(name: &str) -> BranchItem {
    BranchItem::new(
        name.to_owned(),
        "00000000-0000-0000-0000-000000000000".to_owned(),
        1,
        BranchStatus::Active,
        Some(BranchParentItem::new(
            "default".to_owned(),
            "00000000-0000-0000-0000-000000000000".to_owned(),
            1,
            7,
            Some(99),
        )),
        Some(7),
        None,
        1,
    )
}

fn json_index_definition(name: &str, index_type: JsonIndexType) -> JsonIndexDefinition {
    JsonIndexDefinition::new(
        name.to_owned(),
        "default".to_owned(),
        "name".to_owned(),
        index_type,
        1,
        10,
    )
}

fn graph_binding_target() -> GraphBindingTarget {
    GraphBindingTarget::new(
        GraphBindingPrimitive::Json,
        Some("feature".to_owned()),
        "docs",
        "doc-a",
    )
}

fn graph_binding() -> GraphEntityBinding {
    GraphEntityBinding::new(graph_binding_target())
}

fn graph_node_output(graph: &str, node_id: &str) -> GraphNodeDataOutput {
    GraphNodeDataOutput::new(
        graph.to_owned(),
        node_id.to_owned(),
        Some(json!({"kind": "node"})),
        Some(graph_binding()),
        2,
        20,
    )
}

fn graph_edge_output(graph: &str, src: &str, edge_type: &str, dst: &str) -> GraphEdgeDataOutput {
    GraphEdgeDataOutput::new(
        graph.to_owned(),
        src.to_owned(),
        edge_type.to_owned(),
        dst.to_owned(),
        2.5,
        Some(json!({"kind": "edge"})),
        3,
        30,
    )
}
