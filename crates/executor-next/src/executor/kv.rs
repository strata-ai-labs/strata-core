use super::{
    batch_get_result, batch_item_result, bytes_from_key, bytes_from_value, delete_effect,
    delete_output, empty_batch_get_results, empty_batch_results, finish_batch_get_results,
    finish_batch_results, history_items, kv_batch_get_result, kv_batch_result, kv_key, kv_value,
    optional_key, optional_limit, page_or_keys, reject_duplicate_valid_keys, sample_output,
    scan_item, upsert_effect, versioned_value, write_output, BatchGetItemResult, BatchItemResult,
    BatchKvEntry, Bytes, Executor, ExecutorResult, Output, PageInfo, Timestamp,
};

impl Executor {
    pub(super) fn execute_kv_put(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        key: Bytes,
        value: Bytes,
    ) -> ExecutorResult<Output> {
        let output_key = key.clone();
        let key = kv_key(key)?;
        let mut service = self.kv_service(branch, space)?;
        let effect = upsert_effect(service.exists(&key)?);
        let outcome = service.put(key, kv_value(value))?;
        Ok(write_output(output_key, effect, outcome))
    }

    pub(super) fn execute_kv_get(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        key: Bytes,
        as_of: Option<u64>,
    ) -> ExecutorResult<Output> {
        let key = kv_key(key)?;
        let mut service = self.kv_service(branch, space)?;
        if let Some(as_of) = as_of {
            let value = service.get_at(&key, Timestamp::from_micros(as_of))?;
            return Ok(Output::KvValue(value.map(bytes_from_value)));
        }
        Ok(Output::KvVersionedValue(
            service.get_versioned(&key)?.as_ref().map(versioned_value),
        ))
    }

    pub(super) fn execute_kv_delete(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        key: Bytes,
    ) -> ExecutorResult<Output> {
        let output_key = key.clone();
        let key = kv_key(key)?;
        let mut service = self.kv_service(branch, space)?;
        let outcome = service.delete(key)?;
        Ok(delete_output(
            output_key,
            outcome.deleted(),
            outcome.commit(),
        ))
    }

    pub(super) fn execute_kv_list(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        prefix: Option<Bytes>,
        cursor: Option<Bytes>,
        limit: Option<u64>,
        as_of: Option<u64>,
    ) -> ExecutorResult<Output> {
        let prefix = optional_key(prefix)?;
        let cursor = optional_key(cursor)?;
        let mut service = self.kv_service(branch, space)?;
        if let Some(as_of) = as_of {
            let keys = service.list_at(prefix.as_ref(), Timestamp::from_micros(as_of))?;
            return page_or_keys(keys, cursor.as_ref(), limit);
        }
        if limit.is_some() {
            let limit = optional_limit(limit)?.unwrap_or(usize::MAX);
            let page = service.list_page(prefix.as_ref(), cursor.as_ref(), limit)?;
            return Ok(Output::KeysPage {
                items: page.keys().iter().map(bytes_from_key).collect(),
                page: PageInfo::new(page.has_more(), page.cursor().map(bytes_from_key)),
            });
        }
        let keys = service.list(prefix.as_ref())?;
        if cursor.is_some() {
            return page_or_keys(keys, cursor.as_ref(), limit);
        }
        Ok(Output::Keys {
            items: keys.iter().map(bytes_from_key).collect(),
            page: PageInfo::terminal(),
        })
    }

    pub(super) fn execute_kv_scan(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        start: Option<Bytes>,
        limit: Option<u64>,
    ) -> ExecutorResult<Output> {
        let start = optional_key(start)?;
        let limit = optional_limit(limit)?;
        let mut service = self.kv_service(branch, space)?;
        let rows = service
            .scan(start.as_ref(), limit)?
            .iter()
            .map(scan_item)
            .collect();
        Ok(Output::KvScanResult {
            items: rows,
            page: PageInfo::terminal(),
        })
    }

    pub(super) fn execute_kv_batch_put(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        entries: Vec<BatchKvEntry>,
    ) -> ExecutorResult<Output> {
        let mut service = self.kv_service(branch, space)?;
        if entries.is_empty() {
            return Ok(Output::BatchResults(kv_batch_result(Vec::new())));
        }
        let mut results = empty_batch_results(entries.len());
        let mut valid_entries = Vec::with_capacity(entries.len());
        for (index, entry) in entries.into_iter().enumerate() {
            let output_key = entry.key().clone();
            let (key, value) = entry.into_parts();
            match kv_key(key) {
                Ok(key) => valid_entries.push((index, output_key, key, kv_value(value))),
                Err(error) => {
                    results[index] = Some(BatchItemResult::failed_error(output_key, error));
                }
            }
        }
        reject_duplicate_valid_keys(valid_entries.iter().map(|(_, key, ..)| key))?;
        if valid_entries.is_empty() {
            return Ok(Output::BatchResults(finish_batch_results(results)));
        }
        let engine_entries = valid_entries
            .iter()
            .map(|(_, _, key, value)| (key.clone(), value.clone()))
            .collect::<Vec<_>>();
        let existing_keys = valid_entries
            .iter()
            .map(|(_, _, key, _)| key.clone())
            .collect::<Vec<_>>();
        let existing = service.batch_exists(&existing_keys)?;
        let outcome = service.put_batch(engine_entries)?;
        for ((index, output_key, ..), existed) in valid_entries.into_iter().zip(existing) {
            results[index] = Some(batch_item_result(
                output_key,
                upsert_effect(existed),
                Some(outcome),
            ));
        }
        Ok(Output::BatchResults(finish_batch_results(results)))
    }

    pub(super) fn execute_kv_batch_get(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        keys: Vec<Bytes>,
    ) -> ExecutorResult<Output> {
        let mut service = self.kv_service(branch, space)?;
        if keys.is_empty() {
            return Ok(Output::BatchGetResults(kv_batch_get_result(Vec::new())));
        }
        let mut results = empty_batch_get_results(keys.len());
        let mut valid_keys = Vec::with_capacity(keys.len());
        for (index, key) in keys.into_iter().enumerate() {
            let output_key = key.clone();
            match kv_key(key) {
                Ok(key) => valid_keys.push((index, output_key, key)),
                Err(error) => {
                    results[index] = Some(BatchGetItemResult::failed_error(output_key, error));
                }
            }
        }
        if valid_keys.is_empty() {
            return Ok(Output::BatchGetResults(finish_batch_get_results(results)));
        }
        let keys = valid_keys
            .iter()
            .map(|(_, _, key)| key.clone())
            .collect::<Vec<_>>();
        let values = service.batch_get(&keys)?;
        for ((index, output_key, _), value) in valid_keys.into_iter().zip(values) {
            results[index] = Some(batch_get_result(output_key, value));
        }
        Ok(Output::BatchGetResults(finish_batch_get_results(results)))
    }

    pub(super) fn execute_kv_batch_delete(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        keys: Vec<Bytes>,
    ) -> ExecutorResult<Output> {
        let mut service = self.kv_service(branch, space)?;
        if keys.is_empty() {
            return Ok(Output::BatchResults(kv_batch_result(Vec::new())));
        }
        let mut results = empty_batch_results(keys.len());
        let mut valid_keys = Vec::with_capacity(keys.len());
        for (index, key) in keys.into_iter().enumerate() {
            let output_key = key.clone();
            match kv_key(key) {
                Ok(key) => valid_keys.push((index, output_key, key)),
                Err(error) => {
                    results[index] = Some(BatchItemResult::failed_error(output_key, error));
                }
            }
        }
        reject_duplicate_valid_keys(valid_keys.iter().map(|(_, key, _)| key))?;
        if valid_keys.is_empty() {
            return Ok(Output::BatchResults(finish_batch_results(results)));
        }
        let engine_keys = valid_keys
            .iter()
            .map(|(_, _, key)| key.clone())
            .collect::<Vec<_>>();
        let outcome = service.delete_batch(engine_keys)?;
        for ((index, output_key, _), deleted) in valid_keys
            .into_iter()
            .zip(outcome.deleted().iter().copied())
        {
            let commit = deleted.then(|| outcome.commit()).flatten();
            results[index] = Some(batch_item_result(
                output_key,
                delete_effect(deleted),
                commit,
            ));
        }
        Ok(Output::BatchResults(finish_batch_results(results)))
    }

    pub(super) fn execute_kv_batch_exists(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        keys: Vec<Bytes>,
    ) -> ExecutorResult<Output> {
        let mut service = self.kv_service(branch, space)?;
        let keys = keys
            .into_iter()
            .map(kv_key)
            .collect::<ExecutorResult<Vec<_>>>()?;
        Ok(Output::BoolList(service.batch_exists(&keys)?))
    }

    pub(super) fn execute_kv_exists(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        key: Bytes,
    ) -> ExecutorResult<Output> {
        let key = kv_key(key)?;
        let mut service = self.kv_service(branch, space)?;
        Ok(Output::Bool(service.exists(&key)?))
    }

    pub(super) fn execute_kv_getv(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        key: Bytes,
    ) -> ExecutorResult<Output> {
        let key = kv_key(key)?;
        let mut service = self.kv_service(branch, space)?;
        Ok(Output::VersionHistory(
            service.get_versions(&key)?.as_ref().map(history_items),
        ))
    }

    pub(super) fn execute_kv_count(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        prefix: Option<Bytes>,
    ) -> ExecutorResult<Output> {
        let prefix = optional_key(prefix)?;
        let mut service = self.kv_service(branch, space)?;
        Ok(Output::Uint(service.count(prefix.as_ref())?))
    }

    pub(super) fn execute_kv_sample(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        prefix: Option<Bytes>,
        count: Option<u64>,
    ) -> ExecutorResult<Output> {
        let prefix = optional_key(prefix)?;
        let count = optional_limit(count)?.unwrap_or(10);
        let mut service = self.kv_service(branch, space)?;
        Ok(sample_output(&service.sample(prefix.as_ref(), count)?))
    }
}
