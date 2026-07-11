use super::{
    bytes_from_key, optional_limit, BTreeSet, BatchExistsItemResult, BatchGetItemResult, BatchItem,
    BatchItemResult, BatchMode, BatchResult, Bytes, EventBatchAppendItemResult, ExecutorError,
    ExecutorErrorClass, ExecutorResult, GraphBatchItemResult, JsonBatchGetItemResult,
    JsonBatchItemResult, KvKey, Output, PageInfo, VectorBatchGetItemResult, VectorBatchItemResult,
};

pub(super) fn page_or_keys(
    keys: Vec<KvKey>,
    cursor: Option<&KvKey>,
    limit: Option<u64>,
) -> ExecutorResult<Output> {
    if cursor.is_none() && limit.is_none() {
        return Ok(Output::KeysPage {
            items: keys.iter().map(bytes_from_key).collect(),
            page: PageInfo::terminal(),
        });
    }
    let limit = optional_limit(limit)?.unwrap_or(usize::MAX);
    if limit == 0 {
        return Ok(Output::KeysPage {
            items: Vec::new(),
            page: PageInfo::terminal(),
        });
    }
    let filtered = keys
        .into_iter()
        .filter(|key| cursor.is_none_or(|cursor| key.as_bytes() > cursor.as_bytes()))
        .take(limit.saturating_add(1))
        .collect::<Vec<_>>();
    let has_more = filtered.len() > limit;
    let keys = filtered.into_iter().take(limit).collect::<Vec<_>>();
    let cursor = has_more.then(|| keys.last().expect("non-empty page").clone());
    Ok(Output::KeysPage {
        items: keys.iter().map(bytes_from_key).collect(),
        page: PageInfo::new(has_more, cursor.as_ref().map(bytes_from_key)),
    })
}

pub(super) fn empty_batch_results(len: usize) -> Vec<Option<BatchItem<BatchItemResult>>> {
    std::iter::repeat_with(|| None).take(len).collect()
}

pub(super) fn finish_batch_results(
    results: Vec<Option<BatchItem<BatchItemResult>>>,
) -> BatchResult<BatchItemResult> {
    kv_batch_result(unwrap_slots(results, "batch result"))
}

pub(super) fn kv_batch_result(
    items: Vec<BatchItem<BatchItemResult>>,
) -> BatchResult<BatchItemResult> {
    BatchResult::from_items(BatchMode::Itemwise, items)
}

pub(super) fn empty_batch_get_results(len: usize) -> Vec<Option<BatchItem<BatchGetItemResult>>> {
    std::iter::repeat_with(|| None).take(len).collect()
}

pub(super) fn kv_batch_get_result(
    items: Vec<BatchItem<BatchGetItemResult>>,
) -> BatchResult<BatchGetItemResult> {
    BatchResult::from_items(BatchMode::Itemwise, items)
}

pub(super) fn finish_batch_get_results(
    results: Vec<Option<BatchItem<BatchGetItemResult>>>,
) -> BatchResult<BatchGetItemResult> {
    kv_batch_get_result(unwrap_slots(results, "batch get result"))
}

pub(super) fn empty_batch_exists_results(
    len: usize,
) -> Vec<Option<BatchItem<BatchExistsItemResult>>> {
    std::iter::repeat_with(|| None).take(len).collect()
}

pub(super) fn kv_batch_exists_result(
    items: Vec<BatchItem<BatchExistsItemResult>>,
) -> BatchResult<BatchExistsItemResult> {
    BatchResult::from_items(BatchMode::Itemwise, items)
}

pub(super) fn finish_batch_exists_results(
    results: Vec<Option<BatchItem<BatchExistsItemResult>>>,
) -> BatchResult<BatchExistsItemResult> {
    kv_batch_exists_result(unwrap_slots(results, "batch exists result"))
}

pub(super) fn empty_json_batch_results(len: usize) -> Vec<Option<BatchItem<JsonBatchItemResult>>> {
    std::iter::repeat_with(|| None).take(len).collect()
}

pub(super) fn json_batch_result(
    items: Vec<BatchItem<JsonBatchItemResult>>,
) -> BatchResult<JsonBatchItemResult> {
    BatchResult::from_items(BatchMode::Itemwise, items)
}

pub(super) fn finish_json_batch_results(
    results: Vec<Option<BatchItem<JsonBatchItemResult>>>,
) -> BatchResult<JsonBatchItemResult> {
    json_batch_result(unwrap_slots(results, "JSON batch result"))
}

pub(super) fn empty_json_batch_get_results(
    len: usize,
) -> Vec<Option<BatchItem<JsonBatchGetItemResult>>> {
    std::iter::repeat_with(|| None).take(len).collect()
}

pub(super) fn json_batch_get_batch_result(
    items: Vec<BatchItem<JsonBatchGetItemResult>>,
) -> BatchResult<JsonBatchGetItemResult> {
    BatchResult::from_items(BatchMode::Itemwise, items)
}

pub(super) fn finish_json_batch_get_results(
    results: Vec<Option<BatchItem<JsonBatchGetItemResult>>>,
) -> BatchResult<JsonBatchGetItemResult> {
    json_batch_get_batch_result(unwrap_slots(results, "JSON batch get result"))
}

pub(super) fn empty_vector_batch_results(
    len: usize,
) -> Vec<Option<BatchItem<VectorBatchItemResult>>> {
    std::iter::repeat_with(|| None).take(len).collect()
}

pub(super) fn vector_batch_result(
    items: Vec<BatchItem<VectorBatchItemResult>>,
) -> BatchResult<VectorBatchItemResult> {
    BatchResult::from_items(BatchMode::Itemwise, items)
}

pub(super) fn finish_vector_batch_results(
    results: Vec<Option<BatchItem<VectorBatchItemResult>>>,
) -> BatchResult<VectorBatchItemResult> {
    vector_batch_result(unwrap_slots(results, "vector batch result"))
}

pub(super) fn empty_vector_batch_get_results(
    len: usize,
) -> Vec<Option<BatchItem<VectorBatchGetItemResult>>> {
    std::iter::repeat_with(|| None).take(len).collect()
}

pub(super) fn vector_batch_get_result(
    items: Vec<BatchItem<VectorBatchGetItemResult>>,
) -> BatchResult<VectorBatchGetItemResult> {
    BatchResult::from_items(BatchMode::Itemwise, items)
}

pub(super) fn finish_vector_batch_get_results(
    results: Vec<Option<BatchItem<VectorBatchGetItemResult>>>,
) -> BatchResult<VectorBatchGetItemResult> {
    vector_batch_get_result(unwrap_slots(results, "vector batch get result"))
}

pub(super) fn event_batch_result(
    items: Vec<BatchItem<EventBatchAppendItemResult>>,
) -> BatchResult<EventBatchAppendItemResult> {
    BatchResult::from_items(BatchMode::Itemwise, items)
}

pub(super) fn graph_batch_result(
    items: Vec<BatchItem<GraphBatchItemResult>>,
) -> BatchResult<GraphBatchItemResult> {
    BatchResult::from_items(BatchMode::Atomic, items)
}

fn unwrap_slots<T>(results: Vec<Option<BatchItem<T>>>, label: &str) -> Vec<BatchItem<T>> {
    results
        .into_iter()
        .map(|result| result.unwrap_or_else(|| panic!("all {label} slots are filled")))
        .collect()
}

pub(super) fn reject_duplicate_valid_keys<'a>(
    keys: impl IntoIterator<Item = &'a Bytes>,
) -> ExecutorResult<()> {
    reject_duplicate_bytes(keys)
}

pub(super) fn reject_duplicate_bytes<'a>(
    keys: impl IntoIterator<Item = &'a Bytes>,
) -> ExecutorResult<()> {
    let mut seen = BTreeSet::new();
    for key in keys {
        if !seen.insert(key.as_slice().to_vec()) {
            return Err(ExecutorError::new(
                ExecutorErrorClass::InvalidInput,
                "invalid_argument.executor.kv_batch_duplicate_key",
                false,
                "KV batch contains duplicate keys",
            ));
        }
    }
    Ok(())
}
