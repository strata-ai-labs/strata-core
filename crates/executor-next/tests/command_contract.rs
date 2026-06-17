//! Executor command and output serialization contract tests.

use strata_executor_next::{
    BatchGetItemResult, BatchItemResult, BatchKvEntry, BranchCleanupItem, BranchItem,
    BranchParentItem, BranchStatus, Bytes, Command, HistoryItem, Output, SampleItem, ScanItem,
    VersionedValue,
};

#[test]
fn every_kv_command_round_trips_through_json() {
    for command in all_commands() {
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
fn command_names_cover_every_kv_variant() {
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
        ]
    );
}

fn all_commands() -> Vec<Command> {
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

fn all_outputs() -> Vec<Output> {
    vec![
        Output::Branch(branch_item("main")),
        Output::Branches(vec![branch_item("default"), branch_item("main")]),
        Output::BranchDeleteResult {
            branch: branch_item("scratch"),
            generation_before: Some(1),
            generation_after: Some(1),
            cleanup: Some(BranchCleanupItem::new(0, 0, 0)),
        },
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
