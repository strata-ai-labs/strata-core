//! Executor command and output serialization contract tests.

use serde_json::json;
use strata_executor_next::{
    BatchGetItemResult, BatchItemResult, BatchJsonDeleteEntry, BatchJsonEntry, BatchJsonGetEntry,
    BatchKvEntry, BranchCleanupItem, BranchItem, BranchParentItem, BranchStatus, Bytes, Command,
    HistoryItem, JsonBatchGetItemResult, JsonBatchItemResult, JsonHistoryItem, JsonIndexDefinition,
    JsonIndexType, JsonSampleItem, JsonVersionedValue, Output, SampleItem, ScanItem,
    VersionedValue,
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
        ]
    );
}

fn all_commands() -> Vec<Command> {
    let mut commands = branch_commands();
    commands.extend(kv_commands());
    commands.extend(json_commands());
    commands
}

fn command_round_trip_cases() -> Vec<Command> {
    let mut commands = all_commands();
    commands.extend(json_round_trip_edge_commands());
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

fn all_outputs() -> Vec<Output> {
    let mut outputs = branch_outputs();
    outputs.extend(kv_outputs());
    outputs.extend(json_outputs());
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

fn bytes(value: &str) -> Bytes {
    Bytes::from(value)
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
