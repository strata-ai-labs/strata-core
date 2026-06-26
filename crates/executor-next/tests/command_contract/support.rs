pub(super) use serde_json::{json, Value};
pub(super) use strata_engine_next::{CommitOutcomeStatus, ErrorClass, ErrorDetail, RetryPolicy};
pub(super) use strata_executor_next::{
    AdminCapabilities, AdminConfig, AdminControlStatus, AdminDatabaseInfo, AdminDescribe,
    AdminGraph, AdminHealth, AdminHealthStatus, AdminMetrics, AdminOpenTarget, AdminPrimitives,
    AdminVectorCollection, ArrowExportPrimitive, ArrowExportResult, ArrowFileFormat,
    ArrowImportResult, ArrowImportTarget, BatchEventEntry, BatchGetItemResult, BatchItem,
    BatchItemResult, BatchItemStatus, BatchJsonDeleteEntry, BatchJsonEntry, BatchJsonGetEntry,
    BatchKvEntry, BatchMode, BatchResult, BatchStatus, BatchVectorEntry, BranchCleanupItem,
    BranchItem, BranchParentItem, BranchStatus, Bytes, Command, CommitReceipt, ErrorStatus,
    EventBatchAppendItemResult, EventChainVerification, EventData, EventRangeDirection,
    EventVersionedData, GraphBatchItemResult, GraphBatchOperation, GraphBindingHit,
    GraphBindingPrimitive, GraphBindingTarget, GraphDirection, GraphEdgeData, GraphEdgeDataOutput,
    GraphEntityBinding, GraphInfoData, GraphNeighborHit, GraphNodeData, GraphNodeDataOutput,
    HistoryItem, JsonBatchGetItemResult, JsonBatchItemResult, JsonHistoryItem, JsonIndexDefinition,
    JsonIndexType, JsonSampleItem, JsonVersionedValue, MaybeJsonValue, MaybeJsonVersionedValue,
    MutationEffect, MutationEffectKind, Output, PageInfo, SampleItem, ScanItem,
    VectorBatchGetItemResult, VectorBatchItemResult, VectorCollectionInfo, VectorData,
    VectorDistanceMetric, VectorFilterCondition, VectorFilterOp, VectorHistoryItem,
    VectorIndexArtifactSource, VectorIndexDiagnostics, VectorIndexQueryResult, VectorMatch,
    VectorMetadataFilter, VectorScalar, VectorVersionedData, VersionedValue,
};

pub(super) fn assert_json_fixture<T: serde::Serialize>(actual: &T, expected: &str) {
    let actual = serde_json::to_value(actual).expect("actual value serializes");
    let expected: Value = serde_json::from_str(expected).expect("fixture is valid JSON");
    assert_eq!(actual, expected);
}

pub(super) fn assert_pretty_fixture(fixture: &str) {
    assert!(fixture.ends_with('\n'), "fixture must end with newline");
    assert!(fixture.contains("\n  "), "fixture must be indented");
    let _: Value = serde_json::from_str(fixture).expect("fixture is valid JSON");
}

pub(super) fn response_fixture_texts() -> Vec<&'static str> {
    let mut fixtures = vec![
        include_str!("../fixtures/responses/v1/admin/database_info_cache.json"),
        include_str!("../fixtures/responses/v1/arrow/export_graph.json"),
        include_str!("../fixtures/responses/v1/arrow/import_kv.json"),
        include_str!("../fixtures/responses/v1/branches/branch_get.json"),
        include_str!("../fixtures/responses/v1/event/append_applied.json"),
        include_str!("../fixtures/responses/v1/graph/node_write_applied.json"),
        include_str!("../fixtures/responses/v1/json/get_found.json"),
        include_str!("../fixtures/responses/v1/kv/delete_missing.json"),
        include_str!("../fixtures/responses/v1/kv/write_applied.json"),
        include_str!("../fixtures/responses/v1/optional_reads/event_get_missing.json"),
        include_str!("../fixtures/responses/v1/optional_reads/json_get_missing.json"),
        include_str!("../fixtures/responses/v1/optional_reads/json_get_null.json"),
        include_str!("../fixtures/responses/v1/optional_reads/kv_get_versioned_missing.json"),
        include_str!("../fixtures/responses/v1/optional_reads/vector_get_missing.json"),
        include_str!("../fixtures/responses/v1/pages/continued_page.json"),
        include_str!("../fixtures/responses/v1/pages/first_page.json"),
        include_str!("../fixtures/responses/v1/pages/terminal_page.json"),
        include_str!("../fixtures/responses/v1/shared/batch_result_itemwise_ok.json"),
        include_str!("../fixtures/responses/v1/shared/batch_result_ok.json"),
        include_str!("../fixtures/responses/v1/shared/batch_result_partial.json"),
        include_str!("../fixtures/responses/v1/shared/commit_receipt_cache.json"),
        include_str!("../fixtures/responses/v1/shared/commit_receipt_durable.json"),
        include_str!("../fixtures/responses/v1/shared/error_status_invalid_argument.json"),
        include_str!("../fixtures/responses/v1/shared/maybe_json_null.json"),
        include_str!("../fixtures/responses/v1/shared/maybe_missing.json"),
        include_str!("../fixtures/responses/v1/shared/mutation_effect_created.json"),
        include_str!("../fixtures/responses/v1/shared/mutation_effect_not_found.json"),
        include_str!("../fixtures/responses/v1/shared/page_info_continued.json"),
        include_str!("../fixtures/responses/v1/shared/page_info_terminal.json"),
        include_str!("../fixtures/responses/v1/spaces/space_create_applied.json"),
        include_str!("../fixtures/responses/v1/status/bool_true.json"),
        include_str!("../fixtures/responses/v1/status/uint_count.json"),
        include_str!("../fixtures/responses/v1/vector/search_with_index_diagnostics.json"),
        include_str!("../fixtures/responses/v1/vector/upsert_applied.json"),
    ];

    #[cfg(feature = "inference")]
    fixtures.push(include_str!("../fixtures/responses/v1/inference/text.json"));

    fixtures
}

pub(super) fn error_status_fixture() -> ErrorStatus {
    ErrorStatus::new_with_docs_url(
        ErrorClass::InvalidArgument,
        "invalid_argument.executor.batch_item",
        RetryPolicy::Never,
        CommitOutcomeStatus::NotStarted,
        "invalid key",
        "Check the batch item input and retry with a valid key.",
        "https://strata.dev/docs/errors/registry#invalid_argument.executor.batch_item",
        "err-test-000001",
        None,
        vec![ErrorDetail::new("field", "key")],
        vec!["Batch item keys must be non-empty.".to_owned()],
    )
}

pub(super) fn bytes(value: &str) -> Bytes {
    Bytes::from(value)
}

pub(super) fn kv_batch(items: Vec<BatchItemResult>) -> BatchResult<BatchItemResult> {
    fixture_batch(BatchMode::Itemwise, items, |item| {
        (
            item.applied(),
            item.effect().copied(),
            item.commit().copied(),
            item.error_status().cloned(),
        )
    })
}

pub(super) fn kv_batch_get(items: Vec<BatchGetItemResult>) -> BatchResult<BatchGetItemResult> {
    fixture_batch(BatchMode::Itemwise, items, |item| {
        (false, None, None, item.error_status().cloned())
    })
}

pub(super) fn json_batch(items: Vec<JsonBatchItemResult>) -> BatchResult<JsonBatchItemResult> {
    fixture_batch(BatchMode::Itemwise, items, |item| {
        (
            item.applied(),
            item.effect().copied(),
            item.commit().copied(),
            item.error_status().cloned(),
        )
    })
}

pub(super) fn json_batch_get(
    items: Vec<JsonBatchGetItemResult>,
) -> BatchResult<JsonBatchGetItemResult> {
    fixture_batch(BatchMode::Itemwise, items, |item| {
        (false, None, None, item.error_status().cloned())
    })
}

pub(super) fn vector_batch(
    items: Vec<VectorBatchItemResult>,
) -> BatchResult<VectorBatchItemResult> {
    fixture_batch(BatchMode::Itemwise, items, |item| {
        (
            item.applied(),
            item.effect().copied(),
            item.commit().copied(),
            item.error_status().cloned(),
        )
    })
}

pub(super) fn vector_batch_get(
    items: Vec<VectorBatchGetItemResult>,
) -> BatchResult<VectorBatchGetItemResult> {
    fixture_batch(BatchMode::Itemwise, items, |item| {
        (false, None, None, item.error_status().cloned())
    })
}

pub(super) fn event_batch(
    items: Vec<EventBatchAppendItemResult>,
) -> BatchResult<EventBatchAppendItemResult> {
    fixture_batch(BatchMode::Itemwise, items, |item| {
        let effect = item.effect().copied();
        (
            effect.is_some_and(|value| value.applied()),
            effect,
            item.commit().copied(),
            item.error_status().cloned(),
        )
    })
}

pub(super) fn graph_batch(items: Vec<GraphBatchItemResult>) -> BatchResult<GraphBatchItemResult> {
    fixture_batch(BatchMode::Atomic, items, |item| {
        let effect = item.effect().copied();
        (
            effect.is_some_and(|value| value.applied()),
            effect,
            item.commit().copied(),
            item.error_status().cloned(),
        )
    })
}

pub(super) fn fixture_batch<T>(
    mode: BatchMode,
    items: Vec<T>,
    mut facts: impl FnMut(
        &T,
    ) -> (
        bool,
        Option<MutationEffect>,
        Option<CommitReceipt>,
        Option<ErrorStatus>,
    ),
) -> BatchResult<T> {
    BatchResult::from_items(
        mode,
        items
            .into_iter()
            .enumerate()
            .map(|(index, item)| {
                let (applied, effect, commit, error) = facts(&item);
                BatchItem::new(index as u64, applied, effect, commit, Some(item), error)
            })
            .collect(),
    )
}

pub(super) fn commit_receipt(
    version: u64,
    timestamp: u64,
    put_count: u64,
    delete_count: u64,
) -> CommitReceipt {
    CommitReceipt::new(version, timestamp, true, put_count, delete_count)
}

pub(super) fn unchanged_effect() -> MutationEffect {
    MutationEffect::new(false, MutationEffectKind::Unchanged, true, 0)
}

pub(super) fn deleted_count_effect(count: u64) -> MutationEffect {
    MutationEffect::new(true, MutationEffectKind::Deleted, true, count)
}

pub(super) fn event_versioned_data(
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

pub(super) fn branch_item(name: &str) -> BranchItem {
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

pub(super) fn json_index_definition(name: &str, index_type: JsonIndexType) -> JsonIndexDefinition {
    JsonIndexDefinition::new(
        name.to_owned(),
        "default".to_owned(),
        "name".to_owned(),
        index_type,
        1,
        10,
    )
}

pub(super) fn graph_binding_target() -> GraphBindingTarget {
    GraphBindingTarget::new(
        GraphBindingPrimitive::Json,
        Some("feature".to_owned()),
        "docs",
        "doc-a",
    )
}

pub(super) fn graph_binding() -> GraphEntityBinding {
    GraphEntityBinding::new(graph_binding_target())
}

pub(super) fn graph_node_output(graph: &str, node_id: &str) -> GraphNodeDataOutput {
    GraphNodeDataOutput::new(
        graph.to_owned(),
        node_id.to_owned(),
        Some(json!({"kind": "node"})),
        Some(graph_binding()),
        2,
        20,
    )
}

pub(super) fn graph_edge_output(
    graph: &str,
    src: &str,
    edge_type: &str,
    dst: &str,
) -> GraphEdgeDataOutput {
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
