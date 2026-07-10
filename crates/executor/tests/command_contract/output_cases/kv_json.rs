use crate::support::*;

pub(super) fn kv_outputs() -> Vec<Output> {
    vec![
        Output::KvVersionedValue(Some(VersionedValue::new(bytes("one"), 1, 10))),
        Output::KvVersionedValue(None),
        Output::VersionHistory(Some(HistoryResult::new(vec![HistoryItem::new(
            Some(bytes("one")),
            false,
            1,
            10,
        )]))),
        Output::VersionHistory(None),
        Output::Keys {
            items: vec![bytes("a"), bytes("b")],
            page: PageInfo::terminal(),
        },
        Output::KeysPage {
            items: vec![bytes("a")],
            page: PageInfo::new(true, Some(bytes("a"))),
        },
        Output::WriteResult {
            key: bytes("a"),
            effect: MutationEffect::created(),
            commit: commit_receipt(1, 10, 1, 0),
        },
        Output::DeleteResult {
            key: bytes("a"),
            effect: MutationEffect::deleted(),
            commit: Some(commit_receipt(2, 20, 0, 1)),
        },
        Output::KvScanResult {
            items: vec![ScanItem::new(bytes("a"), bytes("one"), 1, 10)],
            page: PageInfo::terminal(),
        },
        Output::BatchResults(kv_batch(vec![BatchItemResult::new(
            bytes("a"),
            MutationEffect::created(),
            Some(commit_receipt(1, 10, 1, 0)),
        )])),
        Output::BatchResults(kv_batch(vec![BatchItemResult::failed(
            Bytes::new(Vec::new()),
            "invalid key",
        )])),
        Output::BatchGetResults(kv_batch_get(vec![BatchGetItemResult::new(
            bytes("a"),
            Some(bytes("one")),
            Some(1),
            Some(10),
        )])),
        Output::BatchGetResults(kv_batch_get(vec![BatchGetItemResult::failed(
            Bytes::new(Vec::new()),
            "invalid key",
        )])),
        Output::Bool(true),
        Output::BoolList(vec![true, false]),
        Output::Uint(2),
        Output::SampleResult {
            total_count: 3,
            items: vec![SampleItem::new(bytes("a"), bytes("one"), 1, 10)],
            page: PageInfo::terminal(),
        },
    ]
}

pub(super) fn json_outputs() -> Vec<Output> {
    vec![
        Output::JsonValue(MaybeJsonValue::found(json!({"name": "Ada"}))),
        Output::JsonValue(MaybeJsonValue::missing()),
        Output::JsonVersionedValue(MaybeJsonVersionedValue::found(JsonVersionedValue::new(
            json!({"name": "Ada"}),
            1,
            10,
            2,
        ))),
        Output::JsonVersionedValue(MaybeJsonVersionedValue::missing()),
        Output::JsonVersionHistory(Some(vec![JsonHistoryItem::new(
            Some(json!({"name": "Ada"})),
            1,
            10,
            Some(2),
            false,
        )])),
        Output::JsonVersionHistory(Some(vec![JsonHistoryItem::new(None, 2, 20, None, true)])),
        Output::JsonVersionHistory(None),
        Output::JsonWriteResult {
            key: "doc-a".to_owned(),
            effect: MutationEffect::created(),
            commit: commit_receipt(1, 10, 1, 0),
        },
        Output::JsonDeleteResult {
            key: "doc-a".to_owned(),
            effect: MutationEffect::deleted(),
            commit: Some(commit_receipt(2, 20, 0, 1)),
        },
        Output::JsonListResult {
            items: vec!["doc-a".to_owned()],
            page: PageInfo::new(true, Some("doc-a".to_owned())),
        },
        Output::JsonBatchResults(json_batch(vec![JsonBatchItemResult::new(
            MutationEffect::created(),
            Some(commit_receipt(1, 10, 1, 0)),
            Some(2),
        )])),
        Output::JsonBatchResults(json_batch(vec![JsonBatchItemResult::failed(
            "invalid document id",
        )])),
        Output::JsonBatchGetResults(json_batch_get(vec![JsonBatchGetItemResult::new(
            Some(json!("Ada")),
            Some(1),
            Some(10),
            Some(2),
        )])),
        Output::JsonBatchGetResults(json_batch_get(vec![JsonBatchGetItemResult::failed(
            "invalid document id",
        )])),
        Output::JsonSampleResult {
            total_count: 3,
            items: vec![JsonSampleItem::new(
                "doc-a".to_owned(),
                json!({"name": "Ada"}),
                1,
                10,
                2,
            )],
            page: PageInfo::terminal(),
        },
        Output::JsonIndexDefinition(json_index_definition("by-name", JsonIndexType::Text)),
        Output::JsonIndexList {
            items: vec![
                json_index_definition("by-age", JsonIndexType::Numeric),
                json_index_definition("by-name", JsonIndexType::Tag),
                json_index_definition("by-bio", JsonIndexType::Text),
            ],
            page: PageInfo::terminal(),
        },
    ]
}
