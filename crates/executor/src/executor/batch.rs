use super::{
    bytes_from_key, optional_limit, usize_to_u64, BTreeSet, BatchExistsItemResult,
    BatchGetItemResult, BatchItem, BatchItemResult, BatchMode, BatchResult, Bytes, CommitReceipt,
    ErrorStatus, EventBatchAppendItemResult, ExecutorError, ExecutorErrorClass, ExecutorResult,
    GraphBatchItemResult, JsonBatchGetItemResult, JsonBatchItemResult, KvKey, MutationEffect,
    Output, PageInfo, VectorBatchGetItemResult, VectorBatchItemResult,
};

pub(super) fn page_or_keys(
    keys: Vec<KvKey>,
    cursor: Option<&KvKey>,
    limit: Option<u64>,
) -> ExecutorResult<Output> {
    if cursor.is_none() && limit.is_none() {
        return Ok(Output::Keys {
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

pub(super) fn empty_batch_results(len: usize) -> Vec<Option<BatchItemResult>> {
    std::iter::repeat_with(|| None).take(len).collect()
}

pub(super) fn finish_batch_results(
    results: Vec<Option<BatchItemResult>>,
) -> BatchResult<BatchItemResult> {
    kv_batch_result(
        results
            .into_iter()
            .map(|result| result.expect("all batch result slots are filled"))
            .collect(),
    )
}

pub(super) fn kv_batch_result(results: Vec<BatchItemResult>) -> BatchResult<BatchItemResult> {
    wrap_batch_items(BatchMode::Itemwise, results, |item| {
        (
            item.applied(),
            item.effect().copied(),
            item.commit().copied(),
            item.error_status().cloned(),
        )
    })
}

pub(super) fn kv_batch_get_result(
    results: Vec<BatchGetItemResult>,
) -> BatchResult<BatchGetItemResult> {
    BatchResult::from_items(
        BatchMode::Itemwise,
        results
            .into_iter()
            .enumerate()
            .map(|(index, result)| {
                let index = usize_to_u64(index);
                if let Some(error) = result.error_status().cloned() {
                    BatchItem::failed(index, Some(result), error)
                } else if result.found() {
                    BatchItem::ok(index, false, None, None, result)
                } else {
                    BatchItem::miss(index, result)
                }
            })
            .collect(),
    )
}

pub(super) fn finish_batch_get_results(
    results: Vec<Option<BatchGetItemResult>>,
) -> BatchResult<BatchGetItemResult> {
    kv_batch_get_result(
        results
            .into_iter()
            .map(|result| result.expect("all batch get result slots are filled"))
            .collect(),
    )
}

pub(super) fn empty_batch_exists_results(len: usize) -> Vec<Option<BatchExistsItemResult>> {
    std::iter::repeat_with(|| None).take(len).collect()
}

pub(super) fn kv_batch_exists_result(
    results: Vec<BatchExistsItemResult>,
) -> BatchResult<BatchExistsItemResult> {
    // `exists` is a definitive answer, so valid items are always `ok`
    // (true or false); only a malformed key is a positional error.
    BatchResult::from_items(
        BatchMode::Itemwise,
        results
            .into_iter()
            .enumerate()
            .map(|(index, result)| {
                let index = usize_to_u64(index);
                if let Some(error) = result.error_status().cloned() {
                    BatchItem::failed(index, Some(result), error)
                } else {
                    BatchItem::ok(index, false, None, None, result)
                }
            })
            .collect(),
    )
}

pub(super) fn finish_batch_exists_results(
    results: Vec<Option<BatchExistsItemResult>>,
) -> BatchResult<BatchExistsItemResult> {
    kv_batch_exists_result(
        results
            .into_iter()
            .map(|result| result.expect("all batch exists result slots are filled"))
            .collect(),
    )
}

pub(super) fn json_batch_result(
    results: Vec<JsonBatchItemResult>,
) -> BatchResult<JsonBatchItemResult> {
    wrap_batch_items(BatchMode::Itemwise, results, |item| {
        (
            item.applied(),
            item.effect().copied(),
            item.commit().copied(),
            item.error_status().cloned(),
        )
    })
}

pub(super) fn finish_json_batch_results(
    results: Vec<Option<JsonBatchItemResult>>,
) -> BatchResult<JsonBatchItemResult> {
    json_batch_result(
        results
            .into_iter()
            .map(|result| result.expect("all JSON batch result slots are filled"))
            .collect(),
    )
}

pub(super) fn json_batch_get_batch_result(
    results: Vec<JsonBatchGetItemResult>,
) -> BatchResult<JsonBatchGetItemResult> {
    // Mirror kv_batch_get_result: a not-found document is a positional miss,
    // not an ok, so the outer batch reads as partial rather than ok.
    BatchResult::from_items(
        BatchMode::Itemwise,
        results
            .into_iter()
            .enumerate()
            .map(|(index, result)| {
                let index = usize_to_u64(index);
                if let Some(error) = result.error_status().cloned() {
                    BatchItem::failed(index, Some(result), error)
                } else if result.found() {
                    BatchItem::ok(index, false, None, None, result)
                } else {
                    BatchItem::miss(index, result)
                }
            })
            .collect(),
    )
}

pub(super) fn finish_json_batch_get_results(
    results: Vec<Option<JsonBatchGetItemResult>>,
) -> BatchResult<JsonBatchGetItemResult> {
    json_batch_get_batch_result(
        results
            .into_iter()
            .map(|result| result.expect("all JSON batch get result slots are filled"))
            .collect(),
    )
}

pub(super) fn vector_batch_result(
    results: Vec<VectorBatchItemResult>,
) -> BatchResult<VectorBatchItemResult> {
    wrap_batch_items(BatchMode::Itemwise, results, |item| {
        (
            item.applied(),
            item.effect().copied(),
            item.commit().copied(),
            item.error_status().cloned(),
        )
    })
}

pub(super) fn empty_vector_batch_results(len: usize) -> Vec<Option<VectorBatchItemResult>> {
    std::iter::repeat_with(|| None).take(len).collect()
}

pub(super) fn finish_vector_batch_results(
    results: Vec<Option<VectorBatchItemResult>>,
) -> Vec<VectorBatchItemResult> {
    results
        .into_iter()
        .map(|result| result.expect("all vector batch result slots are filled"))
        .collect()
}

pub(super) fn vector_batch_get_result(
    results: Vec<VectorBatchGetItemResult>,
) -> BatchResult<VectorBatchGetItemResult> {
    // Mirror kv_batch_get_result: a not-found key is a positional miss, not
    // an ok, so the outer batch reads as partial rather than ok.
    BatchResult::from_items(
        BatchMode::Itemwise,
        results
            .into_iter()
            .enumerate()
            .map(|(index, result)| {
                let index = usize_to_u64(index);
                if let Some(error) = result.error_status().cloned() {
                    BatchItem::failed(index, Some(result), error)
                } else if result.found() {
                    BatchItem::ok(index, false, None, None, result)
                } else {
                    BatchItem::miss(index, result)
                }
            })
            .collect(),
    )
}

pub(super) fn empty_vector_batch_get_results(len: usize) -> Vec<Option<VectorBatchGetItemResult>> {
    std::iter::repeat_with(|| None).take(len).collect()
}

pub(super) fn finish_vector_batch_get_results(
    results: Vec<Option<VectorBatchGetItemResult>>,
) -> Vec<VectorBatchGetItemResult> {
    results
        .into_iter()
        .map(|result| result.expect("all vector batch get result slots are filled"))
        .collect()
}

pub(super) fn event_batch_result(
    results: Vec<EventBatchAppendItemResult>,
) -> BatchResult<EventBatchAppendItemResult> {
    wrap_batch_items(BatchMode::Itemwise, results, |item| {
        let effect = item.effect().copied();
        (
            effect.is_some_and(|value| value.applied()),
            effect,
            item.commit().copied(),
            item.error_status().cloned(),
        )
    })
}

pub(super) fn graph_batch_result(
    results: Vec<GraphBatchItemResult>,
) -> BatchResult<GraphBatchItemResult> {
    wrap_batch_items(BatchMode::Atomic, results, |item| {
        let effect = item.effect().copied();
        (
            effect.is_some_and(|value| value.applied()),
            effect,
            item.commit().copied(),
            item.error_status().cloned(),
        )
    })
}

pub(super) fn wrap_batch_items<T>(
    mode: BatchMode,
    results: Vec<T>,
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
        results
            .into_iter()
            .enumerate()
            .map(|(index, result)| {
                let (applied, effect, commit, error) = facts(&result);
                BatchItem::new(
                    usize_to_u64(index),
                    applied,
                    effect,
                    commit,
                    Some(result),
                    error,
                )
            })
            .collect(),
    )
}

pub(super) fn empty_batch_get_results(len: usize) -> Vec<Option<BatchGetItemResult>> {
    std::iter::repeat_with(|| None).take(len).collect()
}

pub(super) fn empty_json_batch_results(len: usize) -> Vec<Option<JsonBatchItemResult>> {
    std::iter::repeat_with(|| None).take(len).collect()
}

pub(super) fn empty_json_batch_get_results(len: usize) -> Vec<Option<JsonBatchGetItemResult>> {
    std::iter::repeat_with(|| None).take(len).collect()
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
