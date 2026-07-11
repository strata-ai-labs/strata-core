use super::{
    delete_effect, empty_json_batch_get_results, empty_json_batch_results, engine_json_index_type,
    finish_json_batch_get_results, finish_json_batch_results, json_batch_get_batch_result,
    json_batch_get_failed, json_batch_get_result, json_batch_item_failed, json_batch_item_result,
    json_batch_result, json_delete_output, json_document_id, json_get_entry, json_history_items,
    json_index_definition, json_index_name, json_list_output, json_path, json_sample_output,
    json_value, json_value_output, json_versioned_value, json_write_output,
    optional_json_document_id, optional_json_prefix, optional_limit, upsert_effect, usize_to_u64,
    BatchJsonDeleteEntry, BatchJsonEntry, BatchJsonGetEntry, Executor, ExecutorResult,
    JsonIndexType, JsonSetEntry, MaybeJsonValue, MaybeJsonVersionedValue, Output, PageInfo,
    Timestamp, DEFAULT_JSON_LIST_LIMIT,
};

impl Executor {
    pub(super) fn execute_json_set(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        key: &str,
        path: &str,
        value: serde_json::Value,
    ) -> ExecutorResult<Output> {
        let id = json_document_id(key)?;
        let path = json_path(path)?;
        let value = json_value(value)?;
        let mut service = self.json_service(branch, space)?;
        let outcome = service.set_or_create(id, &path, value)?;
        Ok(json_write_output(
            key,
            upsert_effect(!outcome.created()),
            outcome.commit(),
        ))
    }

    pub(super) fn execute_json_get(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        key: &str,
        path: &str,
        as_of: Option<u64>,
    ) -> ExecutorResult<Output> {
        let id = json_document_id(key)?;
        let path = json_path(path)?;
        let mut service = self.json_service(branch, space)?;
        if let Some(as_of) = as_of {
            let value = service.get_at(&id, &path, Timestamp::from_micros(as_of))?;
            return Ok(Output::JsonValue(MaybeJsonValue::from_option(
                value.map(json_value_output),
            )));
        }
        Ok(Output::JsonVersionedValue(
            MaybeJsonVersionedValue::from_option(
                service
                    .get_versioned(&id, &path)?
                    .as_ref()
                    .map(json_versioned_value),
            ),
        ))
    }

    pub(super) fn execute_json_delete(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        key: &str,
        path: &str,
    ) -> ExecutorResult<Output> {
        let id = json_document_id(key)?;
        let path = json_path(path)?;
        let mut service = self.json_service(branch, space)?;
        let outcome = service.delete(id, &path)?;
        Ok(json_delete_output(key, outcome.deleted(), outcome.commit()))
    }

    pub(super) fn execute_json_getv(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        key: String,
    ) -> ExecutorResult<Output> {
        let id = json_document_id(key)?;
        let mut service = self.json_service(branch, space)?;
        Ok(Output::JsonVersionHistory(
            service.get_versions(&id)?.as_ref().map(json_history_items),
        ))
    }

    pub(super) fn execute_json_exists(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        key: String,
    ) -> ExecutorResult<Output> {
        let id = json_document_id(key)?;
        let mut service = self.json_service(branch, space)?;
        Ok(Output::Bool(service.exists(&id)?))
    }

    pub(super) fn execute_json_batch_set(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        entries: Vec<BatchJsonEntry>,
    ) -> ExecutorResult<Output> {
        let mut service = self.json_service(branch, space)?;
        if entries.is_empty() {
            return Ok(Output::JsonBatchResults(json_batch_result(Vec::new())));
        }
        let mut results = empty_json_batch_results(entries.len());
        let mut valid_entries = Vec::with_capacity(entries.len());
        for (index, entry) in entries.into_iter().enumerate() {
            let (key, path, value) = entry.into_parts();
            let validation: ExecutorResult<JsonSetEntry> = (|| {
                let id = json_document_id(key)?;
                let path = json_path(&path)?;
                let value = json_value(value)?;
                Ok(JsonSetEntry::new(id, path, value))
            })();
            match validation {
                Ok(entry) => valid_entries.push((index, entry)),
                Err(error) => {
                    results[index] = Some(json_batch_item_failed(usize_to_u64(index), error));
                }
            }
        }
        if valid_entries.is_empty() {
            return Ok(Output::JsonBatchResults(finish_json_batch_results(results)));
        }
        let engine_entries = valid_entries
            .iter()
            .map(|(_, entry)| entry.clone())
            .collect::<Vec<_>>();
        // The engine owns create-vs-update per item, including intra-batch
        // repeats (a document id written earlier in this batch is an update for
        // its later entries), so the executor relays `created` with no pre-read.
        let outcome = service.batch_set_or_create(engine_entries)?;
        for ((index, _), item) in valid_entries.into_iter().zip(outcome.results()) {
            results[index] = Some(json_batch_item_result(
                usize_to_u64(index),
                upsert_effect(!item.created()),
                outcome.commit(),
                Some(item.document_version()),
            ));
        }
        Ok(Output::JsonBatchResults(finish_json_batch_results(results)))
    }

    pub(super) fn execute_json_batch_get(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        entries: Vec<BatchJsonGetEntry>,
    ) -> ExecutorResult<Output> {
        let mut service = self.json_service(branch, space)?;
        if entries.is_empty() {
            return Ok(Output::JsonBatchGetResults(json_batch_get_batch_result(
                Vec::new(),
            )));
        }
        let mut results = empty_json_batch_get_results(entries.len());
        let mut valid_entries = Vec::with_capacity(entries.len());
        for (index, entry) in entries.into_iter().enumerate() {
            let (key, path) = entry.into_parts();
            match json_get_entry(key, &path) {
                Ok(entry) => valid_entries.push((index, entry)),
                Err(error) => {
                    results[index] = Some(json_batch_get_failed(usize_to_u64(index), error));
                }
            }
        }
        if valid_entries.is_empty() {
            return Ok(Output::JsonBatchGetResults(finish_json_batch_get_results(
                results,
            )));
        }
        let engine_entries = valid_entries
            .iter()
            .map(|(_, entry)| entry.clone())
            .collect::<Vec<_>>();
        let values = service.batch_get(&engine_entries)?;
        for ((index, _), value) in valid_entries.into_iter().zip(values) {
            results[index] = Some(json_batch_get_result(usize_to_u64(index), value));
        }
        Ok(Output::JsonBatchGetResults(finish_json_batch_get_results(
            results,
        )))
    }

    pub(super) fn execute_json_batch_delete(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        entries: Vec<BatchJsonDeleteEntry>,
    ) -> ExecutorResult<Output> {
        let mut service = self.json_service(branch, space)?;
        if entries.is_empty() {
            return Ok(Output::JsonBatchResults(json_batch_result(Vec::new())));
        }
        let mut results = empty_json_batch_results(entries.len());
        let mut valid_entries = Vec::with_capacity(entries.len());
        for (index, entry) in entries.into_iter().enumerate() {
            let (key, path) = entry.into_parts();
            match json_get_entry(key, &path) {
                Ok(entry) => valid_entries.push((index, entry)),
                Err(error) => {
                    results[index] = Some(json_batch_item_failed(usize_to_u64(index), error));
                }
            }
        }
        if valid_entries.is_empty() {
            return Ok(Output::JsonBatchResults(finish_json_batch_results(results)));
        }
        let engine_entries = valid_entries
            .iter()
            .map(|(_, entry)| entry.clone())
            .collect::<Vec<_>>();
        let outcome = service.batch_delete_entries(engine_entries)?;
        for ((index, _), deleted) in valid_entries
            .into_iter()
            .zip(outcome.deleted().iter().copied())
        {
            results[index] = Some(json_batch_item_result(
                usize_to_u64(index),
                delete_effect(deleted),
                deleted.then(|| outcome.commit()).flatten(),
                None,
            ));
        }
        Ok(Output::JsonBatchResults(finish_json_batch_results(results)))
    }

    pub(super) fn execute_json_list(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        prefix: Option<String>,
        cursor: Option<String>,
        limit: Option<u64>,
        as_of: Option<u64>,
    ) -> ExecutorResult<Output> {
        let prefix = optional_json_prefix(prefix)?;
        let cursor = optional_json_document_id(cursor)?;
        let limit = optional_limit(limit)?.unwrap_or(DEFAULT_JSON_LIST_LIMIT);
        let mut service = self.json_service(branch, space)?;
        let page = if let Some(as_of) = as_of {
            service.list_at(
                prefix.as_ref(),
                cursor.as_ref(),
                limit,
                Timestamp::from_micros(as_of),
            )?
        } else {
            service.list(prefix.as_ref(), cursor.as_ref(), limit)?
        };
        Ok(json_list_output(&page))
    }

    pub(super) fn execute_json_count(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        prefix: Option<String>,
    ) -> ExecutorResult<Output> {
        let prefix = optional_json_prefix(prefix)?;
        let mut service = self.json_service(branch, space)?;
        Ok(Output::Uint(service.count(prefix.as_ref())?))
    }

    pub(super) fn execute_json_sample(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        prefix: Option<String>,
        count: Option<u64>,
    ) -> ExecutorResult<Output> {
        let prefix = optional_json_prefix(prefix)?;
        let count = optional_limit(count)?.unwrap_or(10);
        let mut service = self.json_service(branch, space)?;
        Ok(json_sample_output(&service.sample(prefix.as_ref(), count)?))
    }

    pub(super) fn execute_json_create_index(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        name: String,
        field_path: &str,
        index_type: JsonIndexType,
    ) -> ExecutorResult<Output> {
        let name = json_index_name(name)?;
        let field_path = json_path(field_path)?;
        let index_type = engine_json_index_type(index_type);
        let mut service = self.json_service(branch, space)?;
        let definition = service.create_index(name, field_path, index_type)?;
        Ok(Output::JsonIndexDefinition(json_index_definition(
            &definition,
        )))
    }

    pub(super) fn execute_json_drop_index(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        name: String,
    ) -> ExecutorResult<Output> {
        let name = json_index_name(name)?;
        let mut service = self.json_service(branch, space)?;
        Ok(Output::Bool(service.drop_index(&name)?))
    }

    pub(super) fn execute_json_list_indexes(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
    ) -> ExecutorResult<Output> {
        let mut service = self.json_service(branch, space)?;
        Ok(Output::JsonIndexList {
            items: service
                .list_indexes()?
                .iter()
                .map(json_index_definition)
                .collect(),
            page: PageInfo::terminal(),
        })
    }
}
