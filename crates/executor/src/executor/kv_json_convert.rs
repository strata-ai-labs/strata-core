use super::{
    output_json_index_type, BatchExistsItemResult, BatchGetItemResult, BatchItem, BatchItemResult,
    BranchCleanupItem, BranchCleanupSummary, BranchItem, BranchParentItem, BranchStatus,
    BranchSummary, Bytes, CommitDurability, CommitOutcome, CommitReceipt, CommitVersion,
    EngineBranchStatus, EngineJsonIndexDefinition, EngineJsonSample, EngineJsonValue,
    EngineJsonVersionedValue, ExecutorError, HistoryItem, HistoryResult, JsonBatchGetItemResult,
    JsonBatchItemResult, JsonHistory, JsonHistoryItem, JsonHistoryRow, JsonIndexDefinition,
    JsonListPage, JsonSampleItem, JsonSampleRow, KvHistory, KvHistoryRow, KvKey, KvSample,
    KvScanRow, KvVersionedValue, MutationEffect, MutationEffectKind, Output,
    OutputJsonVersionedValue, PageInfo, SampleItem, ScanItem, Timestamp, VersionedValue,
};

pub(super) fn bytes_from_key(key: &KvKey) -> Bytes {
    Bytes::from(key.as_bytes())
}

pub(super) fn branch_item(summary: &BranchSummary) -> BranchItem {
    BranchItem::new(
        summary.name().as_str().to_owned(),
        summary.branch_id().to_string(),
        summary.generation(),
        branch_status(summary.status()),
        summary.parent().map(|parent| {
            BranchParentItem::new(
                parent.name().as_str().to_owned(),
                parent.branch_id().to_string(),
                parent.generation(),
                parent.fork_version().as_u64(),
                parent.fork_timestamp().map(Timestamp::as_micros),
            )
        }),
        summary.created_at().map(CommitVersion::as_u64),
        summary.deleted_at().map(CommitVersion::as_u64),
        summary.state_revision(),
    )
}

pub(super) const fn branch_status(status: EngineBranchStatus) -> BranchStatus {
    match status {
        EngineBranchStatus::Active => BranchStatus::Active,
        EngineBranchStatus::Deleted => BranchStatus::Deleted,
    }
}

pub(super) fn branch_cleanup_item(cleanup: BranchCleanupSummary) -> BranchCleanupItem {
    BranchCleanupItem::new(
        usize_to_u64(cleanup.removed_refs()),
        usize_to_u64(cleanup.releasable_tables()),
        usize_to_u64(cleanup.protected_tables()),
    )
}

pub(super) fn usize_to_u64(value: usize) -> u64 {
    // `usize` only exceeds `u64` on a >64-bit target, which Strata does not
    // support; saturating to `u64::MAX` keeps counts monotonic on the
    // theoretical overflow instead of panicking.
    u64::try_from(value).unwrap_or(u64::MAX)
}

pub(super) fn commit_receipt(outcome: CommitOutcome) -> CommitReceipt {
    CommitReceipt::new(
        outcome.version().as_u64(),
        outcome.timestamp().as_micros(),
        commit_durability(outcome.durability()),
        usize_to_u64(outcome.put_count()),
        usize_to_u64(outcome.delete_count()),
    )
}

const fn commit_durability(durability: strata_engine::CommitDurability) -> CommitDurability {
    match durability {
        strata_engine::CommitDurability::NotDurable => CommitDurability::NotDurable,
        strata_engine::CommitDurability::Standard => CommitDurability::Standard,
        strata_engine::CommitDurability::Always => CommitDurability::Always,
        _ => CommitDurability::Uncertain,
    }
}

pub(super) fn upsert_effect(existed: bool) -> MutationEffect {
    if existed {
        MutationEffect::updated()
    } else {
        MutationEffect::created()
    }
}

pub(super) fn update_effect(updated: bool) -> MutationEffect {
    if updated {
        MutationEffect::updated()
    } else {
        MutationEffect::not_found()
    }
}

pub(super) fn create_effect(created: bool) -> MutationEffect {
    if created {
        MutationEffect::created()
    } else {
        MutationEffect::new(false, MutationEffectKind::Unchanged, true, 0)
    }
}

pub(super) fn delete_effect(deleted: bool) -> MutationEffect {
    if deleted {
        MutationEffect::deleted()
    } else {
        MutationEffect::not_found()
    }
}

pub(super) fn bulk_delete_effect(deleted_count: u64) -> MutationEffect {
    if deleted_count == 0 {
        MutationEffect::not_found()
    } else {
        MutationEffect::new(true, MutationEffectKind::Deleted, true, deleted_count)
    }
}

pub(super) fn write_output(key: Bytes, effect: MutationEffect, outcome: CommitOutcome) -> Output {
    Output::WriteResult {
        key,
        effect,
        commit: commit_receipt(outcome),
    }
}

pub(super) fn delete_output(key: Bytes, deleted: bool, outcome: Option<CommitOutcome>) -> Output {
    Output::DeleteResult {
        key,
        effect: delete_effect(deleted),
        commit: outcome.map(commit_receipt),
    }
}

pub(super) fn batch_item_result(
    index: u64,
    key: Bytes,
    effect: MutationEffect,
    outcome: Option<CommitOutcome>,
) -> BatchItem<BatchItemResult> {
    BatchItem::ok(
        index,
        effect.applied(),
        Some(effect),
        outcome.map(commit_receipt),
        BatchItemResult::new(key),
    )
}

pub(super) fn batch_item_failed(
    index: u64,
    key: Bytes,
    error: ExecutorError,
) -> BatchItem<BatchItemResult> {
    BatchItem::failed(index, Some(BatchItemResult::new(key)), error.into_status())
}

pub(super) fn batch_get_result(
    index: u64,
    key: Bytes,
    value: Option<KvVersionedValue>,
) -> BatchItem<BatchGetItemResult> {
    match value {
        Some(value) => BatchItem::ok(
            index,
            false,
            None,
            None,
            BatchGetItemResult::new(
                key,
                Some(Bytes::from(value.value().as_bytes())),
                Some(value.version().as_u64()),
                Some(value.timestamp().as_micros()),
            ),
        ),
        None => BatchItem::miss(index, BatchGetItemResult::not_found(key)),
    }
}

pub(super) fn batch_get_failed(
    index: u64,
    key: Bytes,
    error: ExecutorError,
) -> BatchItem<BatchGetItemResult> {
    BatchItem::failed(
        index,
        Some(BatchGetItemResult::not_found(key)),
        error.into_status(),
    )
}

pub(super) fn batch_exists_item(
    index: u64,
    key: Bytes,
    exists: bool,
) -> BatchItem<BatchExistsItemResult> {
    BatchItem::ok(
        index,
        false,
        None,
        None,
        BatchExistsItemResult::new(key, exists),
    )
}

pub(super) fn batch_exists_failed(
    index: u64,
    key: Bytes,
    error: ExecutorError,
) -> BatchItem<BatchExistsItemResult> {
    BatchItem::failed(
        index,
        Some(BatchExistsItemResult::new(key, false)),
        error.into_status(),
    )
}

pub(super) fn versioned_value(value: &KvVersionedValue) -> VersionedValue {
    VersionedValue::new(
        Bytes::from(value.value().as_bytes()),
        value.version().as_u64(),
        value.timestamp().as_micros(),
    )
}

pub(super) fn history_result(history: &KvHistory) -> HistoryResult {
    HistoryResult::new(history.rows().iter().map(history_item).collect())
}

pub(super) fn history_item(row: &KvHistoryRow) -> HistoryItem {
    HistoryItem::new(
        row.value().map(|value| Bytes::from(value.as_bytes())),
        row.is_tombstone(),
        row.version().as_u64(),
        row.timestamp().as_micros(),
    )
}

pub(super) fn scan_item(row: &KvScanRow) -> ScanItem {
    ScanItem::new(
        bytes_from_key(row.key()),
        Bytes::from(row.value().as_bytes()),
        row.version().as_u64(),
        row.timestamp().as_micros(),
    )
}

pub(super) fn sample_item(row: &KvScanRow) -> SampleItem {
    SampleItem::new(
        bytes_from_key(row.key()),
        Bytes::from(row.value().as_bytes()),
        row.version().as_u64(),
        row.timestamp().as_micros(),
    )
}

pub(super) fn sample_output(sample: &KvSample) -> Output {
    Output::SampleResult {
        total_count: sample.total_count(),
        items: sample.rows().iter().map(sample_item).collect(),
        page: PageInfo::terminal(),
    }
}

pub(super) fn json_write_output(
    key: &str,
    effect: MutationEffect,
    outcome: CommitOutcome,
) -> Output {
    Output::JsonWriteResult {
        key: key.to_owned(),
        effect,
        commit: commit_receipt(outcome),
    }
}

pub(super) fn json_delete_output(
    key: &str,
    deleted: bool,
    outcome: Option<CommitOutcome>,
) -> Output {
    Output::JsonDeleteResult {
        key: key.to_owned(),
        effect: delete_effect(deleted),
        commit: outcome.map(commit_receipt),
    }
}

pub(super) fn json_value_output(value: EngineJsonValue) -> serde_json::Value {
    value.into_inner()
}

pub(super) fn json_versioned_value(value: &EngineJsonVersionedValue) -> OutputJsonVersionedValue {
    OutputJsonVersionedValue::new(
        value.value().clone().into_inner(),
        value.version().as_u64(),
        value.timestamp().as_micros(),
        value.document_version(),
    )
}

pub(super) fn json_history_items(history: &JsonHistory) -> Vec<JsonHistoryItem> {
    history.rows().iter().map(json_history_item).collect()
}

pub(super) fn json_history_item(row: &JsonHistoryRow) -> JsonHistoryItem {
    JsonHistoryItem::new(
        row.value().map(|value| value.clone().into_inner()),
        row.version().as_u64(),
        row.timestamp().as_micros(),
        row.document_version(),
        row.is_tombstone(),
    )
}

pub(super) fn json_batch_item_result(
    index: u64,
    effect: MutationEffect,
    outcome: Option<CommitOutcome>,
    document_version: Option<u64>,
) -> BatchItem<JsonBatchItemResult> {
    BatchItem::ok(
        index,
        effect.applied(),
        Some(effect),
        outcome.map(commit_receipt),
        JsonBatchItemResult::new(document_version),
    )
}

pub(super) fn json_batch_item_failed(
    index: u64,
    error: ExecutorError,
) -> BatchItem<JsonBatchItemResult> {
    BatchItem::failed(
        index,
        Some(JsonBatchItemResult::new(None)),
        error.into_status(),
    )
}

pub(super) fn json_batch_get_result(
    index: u64,
    value: Option<EngineJsonVersionedValue>,
) -> BatchItem<JsonBatchGetItemResult> {
    match value {
        Some(value) => BatchItem::ok(
            index,
            false,
            None,
            None,
            JsonBatchGetItemResult::new(
                Some(value.value().clone().into_inner()),
                Some(value.version().as_u64()),
                Some(value.timestamp().as_micros()),
                Some(value.document_version()),
            ),
        ),
        None => BatchItem::miss(index, JsonBatchGetItemResult::not_found()),
    }
}

pub(super) fn json_batch_get_failed(
    index: u64,
    error: ExecutorError,
) -> BatchItem<JsonBatchGetItemResult> {
    BatchItem::failed(
        index,
        Some(JsonBatchGetItemResult::not_found()),
        error.into_status(),
    )
}

pub(super) fn json_list_output(page: &JsonListPage) -> Output {
    Output::JsonListResult {
        items: page
            .document_ids()
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect(),
        page: PageInfo::new(
            page.has_more(),
            page.cursor().map(|cursor| cursor.as_str().to_owned()),
        ),
    }
}

pub(super) fn json_sample_output(sample: &EngineJsonSample) -> Output {
    Output::JsonSampleResult {
        total_count: sample.total_count(),
        items: sample.rows().iter().map(json_sample_item).collect(),
        page: PageInfo::terminal(),
    }
}

pub(super) fn json_sample_item(row: &JsonSampleRow) -> JsonSampleItem {
    JsonSampleItem::new(
        row.document_id().as_str().to_owned(),
        row.value().clone().into_inner(),
        row.version().as_u64(),
        row.timestamp().as_micros(),
        row.document_version(),
    )
}

pub(super) fn json_index_definition(definition: &EngineJsonIndexDefinition) -> JsonIndexDefinition {
    JsonIndexDefinition::new(
        definition.name().as_str().to_owned(),
        definition.space().as_str().to_owned(),
        definition.field_path().to_string(),
        output_json_index_type(definition.index_type()),
        definition.created_version(),
        definition.created_timestamp(),
    )
}
