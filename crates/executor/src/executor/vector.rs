use super::{
    commit_receipt, delete_effect, empty_vector_batch_get_results, empty_vector_batch_results,
    engine_vector_metric, finish_vector_batch_get_results, finish_vector_batch_results,
    optional_limit, optional_vector_key, optional_vector_metadata, require_vector_collection_info,
    required_usize, update_effect, upsert_effect, usize_to_u64, vector_batch_get_failed,
    vector_batch_get_item, vector_batch_item_failed, vector_batch_item_result,
    vector_bulk_delete_output, vector_collection, vector_collection_info,
    vector_dimension_mismatch_error, vector_embedding, vector_filter, vector_history_result,
    vector_index_diagnostics, vector_key, vector_key_page_output, vector_match,
    vector_metadata_patch, vector_upsert_entry, vector_versioned_data, vector_write_output,
    BatchVectorEntry, EngineVectorConfig, Executor, ExecutorError, ExecutorResult, Output,
    PageInfo, Timestamp, VectorDistanceMetric, VectorIndexQueryResult, VectorMetadataFilter,
    DEFAULT_VECTOR_LIST_LIMIT,
};

impl Executor {
    pub(super) fn execute_vector_create_collection(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        collection: String,
        dimension: u64,
        metric: VectorDistanceMetric,
    ) -> ExecutorResult<Output> {
        let collection = vector_collection(collection)?;
        let config = EngineVectorConfig::new(
            required_usize(
                dimension,
                "invalid_argument.executor.vector_dimension",
                "vector dimension does not fit this platform",
            )?,
            engine_vector_metric(metric),
        )?;
        let mut service = self.vector_service(branch, space)?;
        Ok(Output::VectorCollectionList {
            items: vec![vector_collection_info(
                &service.create_collection(collection, config)?,
            )],
            page: PageInfo::terminal(),
        })
    }

    pub(super) fn execute_vector_delete_collection(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        collection: String,
    ) -> ExecutorResult<Output> {
        let collection = vector_collection(collection)?;
        let mut service = self.vector_service(branch, space)?;
        Ok(Output::Bool(service.delete_collection(&collection)?))
    }

    pub(super) fn execute_vector_list_collections(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
    ) -> ExecutorResult<Output> {
        let mut service = self.vector_service(branch, space)?;
        Ok(Output::VectorCollectionList {
            items: service
                .list_collections()?
                .iter()
                .map(vector_collection_info)
                .collect(),
            page: PageInfo::terminal(),
        })
    }

    pub(super) fn execute_vector_collection_stats(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        collection: String,
    ) -> ExecutorResult<Output> {
        let collection = vector_collection(collection)?;
        let mut service = self.vector_service(branch, space)?;
        let Some(info) = service.collection_info(&collection)? else {
            return Err(ExecutorError::not_found(
                "not_found.engine.vector_collection",
                "vector collection does not exist",
            ));
        };
        Ok(Output::VectorCollectionList {
            items: vec![vector_collection_info(&info)],
            page: PageInfo::terminal(),
        })
    }

    pub(super) fn execute_vector_count(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        collection: String,
    ) -> ExecutorResult<Output> {
        let collection = vector_collection(collection)?;
        let mut service = self.vector_service(branch, space)?;
        Ok(Output::Uint(service.count(&collection)?))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn execute_vector_upsert(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        collection: String,
        key: String,
        vector: Vec<f32>,
        metadata: Option<serde_json::Value>,
    ) -> ExecutorResult<Output> {
        let collection = vector_collection(collection)?;
        let key = vector_key(key)?;
        let embedding = vector_embedding(vector)?;
        let metadata = optional_vector_metadata(metadata)?;
        let mut service = self.vector_service(branch, space)?;
        let outcome = service.upsert(collection.clone(), key.clone(), embedding, metadata)?;
        Ok(vector_write_output(
            &collection,
            &key,
            outcome.commit(),
            outcome.vector_revision(),
            outcome.created(),
        ))
    }

    pub(super) fn execute_vector_get(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        collection: String,
        key: String,
        as_of: Option<u64>,
    ) -> ExecutorResult<Output> {
        let collection = vector_collection(collection)?;
        let key = vector_key(key)?;
        let mut service = self.vector_service(branch, space)?;
        let value = if let Some(as_of) = as_of {
            service.get_at(&collection, &key, Timestamp::from_micros(as_of))?
        } else {
            service.get_versioned(&collection, &key)?
        };
        Ok(Output::VectorData(
            value.as_ref().map(vector_versioned_data),
        ))
    }

    pub(super) fn execute_vector_getv(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        collection: String,
        key: String,
    ) -> ExecutorResult<Output> {
        let collection = vector_collection(collection)?;
        let key = vector_key(key)?;
        let mut service = self.vector_service(branch, space)?;
        Ok(Output::VectorVersionHistory(
            service
                .history(&collection, &key)?
                .as_ref()
                .map(|history| vector_history_result(&key, history)),
        ))
    }

    pub(super) fn execute_vector_exists(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        collection: String,
        key: String,
    ) -> ExecutorResult<Output> {
        let collection = vector_collection(collection)?;
        let key = vector_key(key)?;
        let mut service = self.vector_service(branch, space)?;
        Ok(Output::Bool(service.exists(&collection, &key)?))
    }

    pub(super) fn execute_vector_list_keys(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        collection: String,
        prefix: Option<String>,
        cursor: Option<String>,
        limit: Option<u64>,
    ) -> ExecutorResult<Output> {
        let collection = vector_collection(collection)?;
        let prefix = optional_vector_key(prefix)?;
        let cursor = optional_vector_key(cursor)?;
        let limit = optional_limit(limit)?.unwrap_or(DEFAULT_VECTOR_LIST_LIMIT);
        let mut service = self.vector_service(branch, space)?;
        Ok(vector_key_page_output(&service.list_keys(
            &collection,
            prefix.as_ref(),
            cursor.as_ref(),
            limit,
        )?))
    }

    pub(super) fn execute_vector_update_metadata(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        collection: String,
        key: String,
        patch: serde_json::Value,
    ) -> ExecutorResult<Output> {
        let collection = vector_collection(collection)?;
        let key = vector_key(key)?;
        let patch = vector_metadata_patch(patch)?;
        let mut service = self.vector_service(branch, space)?;
        let outcome = service.update_metadata(&collection, key.clone(), &patch)?;
        Ok(Output::VectorMetadataUpdateResult {
            collection: collection.as_str().to_owned(),
            key: outcome.key().as_str().to_owned(),
            effect: update_effect(outcome.updated()),
            commit: outcome.commit().map(commit_receipt),
            vector_revision: outcome.vector_revision(),
        })
    }

    pub(super) fn execute_vector_delete(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        collection: String,
        key: String,
    ) -> ExecutorResult<Output> {
        let collection = vector_collection(collection)?;
        let key = vector_key(key)?;
        let mut service = self.vector_service(branch, space)?;
        let outcome = service.delete(&collection, key)?;
        Ok(Output::VectorDeleteResult {
            collection: collection.as_str().to_owned(),
            key: outcome.key().as_str().to_owned(),
            effect: delete_effect(outcome.deleted()),
            commit: outcome.commit().map(commit_receipt),
        })
    }

    pub(super) fn execute_vector_delete_by_filter(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        collection: String,
        filter: VectorMetadataFilter,
    ) -> ExecutorResult<Output> {
        let collection = vector_collection(collection)?;
        let filter = vector_filter(filter)?;
        let mut service = self.vector_service(branch, space)?;
        let outcome = service.delete_by_filter(&collection, &filter)?;
        Ok(vector_bulk_delete_output(&collection, outcome))
    }

    pub(super) fn execute_vector_delete_all(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        collection: String,
    ) -> ExecutorResult<Output> {
        let collection = vector_collection(collection)?;
        let mut service = self.vector_service(branch, space)?;
        let outcome = service.delete_all(&collection)?;
        Ok(vector_bulk_delete_output(&collection, outcome))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn execute_vector_query(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        collection: String,
        query: Vec<f32>,
        k: u64,
        filter: Option<VectorMetadataFilter>,
        as_of: Option<u64>,
    ) -> ExecutorResult<Output> {
        let collection = vector_collection(collection)?;
        let query = vector_embedding(query)?;
        let k = required_usize(
            k,
            "invalid_argument.executor.vector_limit",
            "vector match limit does not fit this platform",
        )?;
        let filter = filter.map(vector_filter).transpose()?;
        let mut service = self.vector_service(branch, space)?;
        let result = if let Some(as_of) = as_of {
            service.query_at(
                &collection,
                &query,
                k,
                filter.as_ref(),
                Timestamp::from_micros(as_of),
            )?
        } else {
            service.query(&collection, &query, k, filter.as_ref())?
        };
        Ok(Output::VectorMatches(
            result.matches().iter().map(vector_match).collect(),
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn execute_vector_index_query(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        collection: String,
        query: Vec<f32>,
        k: u64,
        filter: Option<VectorMetadataFilter>,
        as_of: Option<u64>,
    ) -> ExecutorResult<Output> {
        let collection = vector_collection(collection)?;
        let query = vector_embedding(query)?;
        let k = required_usize(
            k,
            "invalid_argument.executor.vector_limit",
            "vector match limit does not fit this platform",
        )?;
        let filter = filter.map(vector_filter).transpose()?;
        let mut service = self.vector_service(branch, space)?;
        let (result, diagnostics) = if let Some(as_of) = as_of {
            service.query_at_with_index_diagnostics(
                &collection,
                &query,
                k,
                filter.as_ref(),
                Timestamp::from_micros(as_of),
            )?
        } else {
            service.query_with_index_diagnostics(&collection, &query, k, filter.as_ref())?
        };
        Ok(Output::VectorIndexQuery(VectorIndexQueryResult::new(
            result.matches().iter().map(vector_match).collect(),
            vector_index_diagnostics(&diagnostics),
        )))
    }

    pub(super) fn execute_vector_batch_upsert(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        collection: String,
        entries: Vec<BatchVectorEntry>,
    ) -> ExecutorResult<Output> {
        let collection = vector_collection(collection)?;
        let mut service = self.vector_service(branch, space)?;
        let info = require_vector_collection_info(&mut service, &collection)?;
        let expected_dimension = info.config().dimension();
        let mut results = empty_vector_batch_results(entries.len());
        let mut valid_entries = Vec::with_capacity(entries.len());
        for (index, entry) in entries.into_iter().enumerate() {
            match vector_upsert_entry(entry) {
                Ok(entry) if entry.embedding().dimension() == expected_dimension => {
                    valid_entries.push((index, entry));
                }
                Ok(entry) => {
                    results[index] = Some(vector_batch_item_failed(
                        usize_to_u64(index),
                        vector_dimension_mismatch_error(
                            expected_dimension,
                            entry.embedding().dimension(),
                        ),
                    ));
                }
                Err(error) => {
                    results[index] = Some(vector_batch_item_failed(usize_to_u64(index), error));
                }
            }
        }
        if valid_entries.is_empty() {
            return Ok(Output::VectorBatchUpsertResults(
                finish_vector_batch_results(results),
            ));
        }
        let entries = valid_entries
            .iter()
            .map(|(_, entry)| entry.clone())
            .collect::<Vec<_>>();
        let outcome = service.batch_upsert(&collection, &entries)?;
        for ((index, _), (revision, created)) in valid_entries.into_iter().zip(
            outcome
                .vector_revisions()
                .iter()
                .copied()
                .zip(outcome.created().iter().copied()),
        ) {
            results[index] = Some(vector_batch_item_result(
                usize_to_u64(index),
                true,
                upsert_effect(!created),
                outcome.commit(),
                Some(revision),
            ));
        }
        Ok(Output::VectorBatchUpsertResults(
            finish_vector_batch_results(results),
        ))
    }

    pub(super) fn execute_vector_batch_get(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        collection: String,
        keys: Vec<String>,
    ) -> ExecutorResult<Output> {
        let collection = vector_collection(collection)?;
        let mut service = self.vector_service(branch, space)?;
        let _ = require_vector_collection_info(&mut service, &collection)?;
        let mut results = empty_vector_batch_get_results(keys.len());
        let mut valid_keys = Vec::with_capacity(keys.len());
        for (index, key) in keys.into_iter().enumerate() {
            match vector_key(key) {
                Ok(key) => valid_keys.push((index, key)),
                Err(error) => {
                    results[index] = Some(vector_batch_get_failed(usize_to_u64(index), error));
                }
            }
        }
        if valid_keys.is_empty() {
            return Ok(Output::VectorBatchGetResults(
                finish_vector_batch_get_results(results),
            ));
        }
        let keys = valid_keys
            .iter()
            .map(|(_, key)| key.clone())
            .collect::<Vec<_>>();
        let outcome = service.batch_get(&collection, &keys)?;
        for ((index, _), entry) in valid_keys.into_iter().zip(outcome.entries()) {
            results[index] = Some(vector_batch_get_item(
                usize_to_u64(index),
                entry.as_ref().map(vector_versioned_data),
            ));
        }
        Ok(Output::VectorBatchGetResults(
            finish_vector_batch_get_results(results),
        ))
    }

    pub(super) fn execute_vector_batch_delete(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        collection: String,
        keys: Vec<String>,
    ) -> ExecutorResult<Output> {
        let collection = vector_collection(collection)?;
        let mut service = self.vector_service(branch, space)?;
        let _ = require_vector_collection_info(&mut service, &collection)?;
        let mut results = empty_vector_batch_results(keys.len());
        let mut valid_keys = Vec::with_capacity(keys.len());
        for (index, key) in keys.into_iter().enumerate() {
            match vector_key(key) {
                Ok(key) => valid_keys.push((index, key)),
                Err(error) => {
                    results[index] = Some(vector_batch_item_failed(usize_to_u64(index), error));
                }
            }
        }
        if valid_keys.is_empty() {
            return Ok(Output::VectorBatchDeleteResults(
                finish_vector_batch_results(results),
            ));
        }
        let keys = valid_keys
            .iter()
            .map(|(_, key)| key.clone())
            .collect::<Vec<_>>();
        let outcome = service.batch_delete(&collection, &keys)?;
        for ((index, _), deleted) in valid_keys
            .into_iter()
            .zip(outcome.deleted().iter().copied())
        {
            results[index] = Some(vector_batch_item_result(
                usize_to_u64(index),
                deleted,
                delete_effect(deleted),
                deleted.then(|| outcome.commit()).flatten(),
                None,
            ));
        }
        Ok(Output::VectorBatchDeleteResults(
            finish_vector_batch_results(results),
        ))
    }
}
