//! Executor handle and command dispatch.

use std::collections::BTreeSet;
use std::path::PathBuf;

use strata_core_next::{CommitVersion, Timestamp};
use strata_engine_next::{
    api::CommitOutcome, BranchCleanupSummary, BranchName, BranchStatus as EngineBranchStatus,
    BranchSummary, CacheOpenOptions, Database, DurableLocalOpenOptions, JsonDocumentId,
    JsonGetEntry, JsonHistory, JsonHistoryRow, JsonIndexDefinition as EngineJsonIndexDefinition,
    JsonIndexName, JsonIndexType as EngineJsonIndexType, JsonListPage, JsonPath,
    JsonSample as EngineJsonSample, JsonSampleRow, JsonService, JsonSetEntry,
    JsonValue as EngineJsonValue, JsonVersionedValue as EngineJsonVersionedValue, KvHistory,
    KvHistoryRow, KvKey, KvSample, KvScanRow, KvValue, KvVersionedValue, ProductSpace,
};

use crate::command::Command;
use crate::error::{ExecutorError, ExecutorErrorClass, ExecutorResult};
use crate::output::Output;
use crate::types::{
    BatchGetItemResult, BatchItemResult, BatchJsonDeleteEntry, BatchJsonEntry, BatchJsonGetEntry,
    BatchKvEntry, BranchCleanupItem, BranchItem, BranchParentItem, BranchStatus, Bytes,
    HistoryItem, JsonBatchGetItemResult, JsonBatchItemResult, JsonHistoryItem, JsonIndexDefinition,
    JsonIndexType, JsonSampleItem, JsonVersionedValue as OutputJsonVersionedValue, SampleItem,
    ScanItem, VersionedValue, DEFAULT_BRANCH, DEFAULT_SPACE,
};

const DEFAULT_JSON_LIST_LIMIT: usize = 100;

/// Serialized command executor backed by an engine database handle.
pub struct Executor {
    database: Database,
    default_branch: String,
}

impl Executor {
    /// Opens a volatile cache-backed executor handle.
    pub fn open_cache() -> ExecutorResult<Self> {
        let outcome = Database::open_cache(CacheOpenOptions::new())?;
        Ok(Self::from_database(outcome.into_database()))
    }

    /// Opens a durable-local executor handle at the selected path.
    pub fn open_durable_local(path: impl Into<PathBuf>) -> ExecutorResult<Self> {
        let outcome = Database::open_local(path, DurableLocalOpenOptions::new())?;
        Ok(Self::from_database(outcome.into_database()))
    }

    /// Wraps an engine database handle.
    pub fn from_database(database: Database) -> Self {
        let default_branch = database.default_branch().as_str().to_owned();
        Self {
            database,
            default_branch,
        }
    }

    /// Sets the default branch used when commands omit their branch.
    pub fn with_default_branch(mut self, branch: impl Into<String>) -> ExecutorResult<Self> {
        let branch = branch.into();
        let _validated = branch_name(Some(branch.as_str()), DEFAULT_BRANCH)?;
        self.default_branch = branch;
        Ok(self)
    }

    /// Returns the default branch used for omitted command branches.
    #[must_use]
    pub fn default_branch(&self) -> &str {
        &self.default_branch
    }

    /// Closes the underlying database handle.
    pub fn close(&mut self) -> ExecutorResult<()> {
        self.database.close()?;
        Ok(())
    }

    /// Creates a branch from the current source branch head.
    pub fn create_branch_from_head(
        &mut self,
        source: impl AsRef<str>,
        branch: impl Into<String>,
    ) -> ExecutorResult<()> {
        let source = branch_name(Some(source.as_ref()), DEFAULT_BRANCH)?;
        let branch = branch.into();
        let branch = branch_name(Some(branch.as_str()), DEFAULT_BRANCH)?;
        let mut branches = self.database.branches()?;
        branches.fork_current(&source, branch)?;
        Ok(())
    }

    /// Executes one serialized command.
    pub fn execute(&mut self, command: Command) -> ExecutorResult<Output> {
        match command {
            Command::BranchList => self.execute_branch_list(),
            Command::BranchGet { branch } => self.execute_branch_get(&branch),
            Command::BranchCreate { branch } => self.execute_branch_create(&branch),
            Command::BranchForkCurrent { source, branch } => {
                self.execute_branch_fork_current(&source, &branch)
            }
            Command::BranchForkAtVersion {
                source,
                branch,
                version,
            } => self.execute_branch_fork_at_version(&source, &branch, version),
            Command::BranchForkAtTimestamp {
                source,
                branch,
                timestamp,
            } => self.execute_branch_fork_at_timestamp(&source, &branch, timestamp),
            Command::BranchDelete { branch } => self.execute_branch_delete(&branch),
            Command::KvPut {
                branch,
                space,
                key,
                value,
            } => self.execute_kv_put(branch.as_deref(), space.as_deref(), key, value),
            Command::KvGet {
                branch,
                space,
                key,
                as_of,
            } => self.execute_kv_get(branch.as_deref(), space.as_deref(), key, as_of),
            Command::KvDelete { branch, space, key } => {
                self.execute_kv_delete(branch.as_deref(), space.as_deref(), key)
            }
            Command::KvList {
                branch,
                space,
                prefix,
                cursor,
                limit,
                as_of,
            } => self.execute_kv_list(
                branch.as_deref(),
                space.as_deref(),
                prefix,
                cursor,
                limit,
                as_of,
            ),
            Command::KvScan {
                branch,
                space,
                start,
                limit,
            } => self.execute_kv_scan(branch.as_deref(), space.as_deref(), start, limit),
            Command::KvBatchPut {
                branch,
                space,
                entries,
            } => self.execute_kv_batch_put(branch.as_deref(), space.as_deref(), entries),
            Command::KvBatchGet {
                branch,
                space,
                keys,
            } => self.execute_kv_batch_get(branch.as_deref(), space.as_deref(), keys),
            Command::KvBatchDelete {
                branch,
                space,
                keys,
            } => self.execute_kv_batch_delete(branch.as_deref(), space.as_deref(), keys),
            Command::KvBatchExists {
                branch,
                space,
                keys,
            } => self.execute_kv_batch_exists(branch.as_deref(), space.as_deref(), keys),
            Command::KvExists { branch, space, key } => {
                self.execute_kv_exists(branch.as_deref(), space.as_deref(), key)
            }
            Command::KvGetv { branch, space, key } => {
                self.execute_kv_getv(branch.as_deref(), space.as_deref(), key)
            }
            Command::KvCount {
                branch,
                space,
                prefix,
            } => self.execute_kv_count(branch.as_deref(), space.as_deref(), prefix),
            Command::KvSample {
                branch,
                space,
                prefix,
                count,
            } => self.execute_kv_sample(branch.as_deref(), space.as_deref(), prefix, count),
            Command::JsonSet {
                branch,
                space,
                key,
                path,
                value,
            } => self.execute_json_set(branch.as_deref(), space.as_deref(), &key, &path, value),
            Command::JsonGet {
                branch,
                space,
                key,
                path,
                as_of,
            } => self.execute_json_get(branch.as_deref(), space.as_deref(), &key, &path, as_of),
            Command::JsonDelete {
                branch,
                space,
                key,
                path,
            } => self.execute_json_delete(branch.as_deref(), space.as_deref(), &key, &path),
            Command::JsonGetv { branch, space, key } => {
                self.execute_json_getv(branch.as_deref(), space.as_deref(), key)
            }
            Command::JsonExists { branch, space, key } => {
                self.execute_json_exists(branch.as_deref(), space.as_deref(), key)
            }
            Command::JsonBatchSet {
                branch,
                space,
                entries,
            } => self.execute_json_batch_set(branch.as_deref(), space.as_deref(), entries),
            Command::JsonBatchGet {
                branch,
                space,
                entries,
            } => self.execute_json_batch_get(branch.as_deref(), space.as_deref(), entries),
            Command::JsonBatchDelete {
                branch,
                space,
                entries,
            } => self.execute_json_batch_delete(branch.as_deref(), space.as_deref(), entries),
            Command::JsonList {
                branch,
                space,
                prefix,
                cursor,
                limit,
                as_of,
            } => self.execute_json_list(
                branch.as_deref(),
                space.as_deref(),
                prefix,
                cursor,
                limit,
                as_of,
            ),
            Command::JsonCount {
                branch,
                space,
                prefix,
            } => self.execute_json_count(branch.as_deref(), space.as_deref(), prefix),
            Command::JsonSample {
                branch,
                space,
                prefix,
                count,
            } => self.execute_json_sample(branch.as_deref(), space.as_deref(), prefix, count),
            Command::JsonCreateIndex {
                branch,
                space,
                name,
                field_path,
                index_type,
            } => self.execute_json_create_index(
                branch.as_deref(),
                space.as_deref(),
                name,
                &field_path,
                index_type,
            ),
            Command::JsonDropIndex {
                branch,
                space,
                name,
            } => self.execute_json_drop_index(branch.as_deref(), space.as_deref(), name),
            Command::JsonListIndexes { branch, space } => {
                self.execute_json_list_indexes(branch.as_deref(), space.as_deref())
            }
        }
    }

    fn execute_branch_list(&mut self) -> ExecutorResult<Output> {
        let branches = self
            .database
            .branches()?
            .list()?
            .iter()
            .map(branch_item)
            .collect();
        Ok(Output::Branches(branches))
    }

    fn execute_branch_get(&mut self, branch: &str) -> ExecutorResult<Output> {
        let branch = branch_name(Some(branch), DEFAULT_BRANCH)?;
        let summary = self.database.branches()?.get(&branch)?;
        Ok(Output::Branch(branch_item(&summary)))
    }

    fn execute_branch_create(&mut self, branch: &str) -> ExecutorResult<Output> {
        let branch = branch_name(Some(branch), DEFAULT_BRANCH)?;
        let outcome = self.database.branches()?.create(branch)?;
        Ok(Output::Branch(branch_item(outcome.branch())))
    }

    fn execute_branch_fork_current(
        &mut self,
        source: &str,
        branch: &str,
    ) -> ExecutorResult<Output> {
        let source = branch_name(Some(source), DEFAULT_BRANCH)?;
        let branch = branch_name(Some(branch), DEFAULT_BRANCH)?;
        let outcome = self.database.branches()?.fork_current(&source, branch)?;
        Ok(Output::Branch(branch_item(outcome.branch())))
    }

    fn execute_branch_fork_at_version(
        &mut self,
        source: &str,
        branch: &str,
        version: u64,
    ) -> ExecutorResult<Output> {
        let source = branch_name(Some(source), DEFAULT_BRANCH)?;
        let branch = branch_name(Some(branch), DEFAULT_BRANCH)?;
        let outcome = self.database.branches()?.fork_at_version(
            &source,
            branch,
            CommitVersion::new(version),
        )?;
        Ok(Output::Branch(branch_item(outcome.branch())))
    }

    fn execute_branch_fork_at_timestamp(
        &mut self,
        source: &str,
        branch: &str,
        timestamp: u64,
    ) -> ExecutorResult<Output> {
        let source = branch_name(Some(source), DEFAULT_BRANCH)?;
        let branch = branch_name(Some(branch), DEFAULT_BRANCH)?;
        let outcome = self.database.branches()?.fork_at_timestamp(
            &source,
            branch,
            Timestamp::from_micros(timestamp),
        )?;
        Ok(Output::Branch(branch_item(outcome.branch())))
    }

    fn execute_branch_delete(&mut self, branch: &str) -> ExecutorResult<Output> {
        let branch = branch_name(Some(branch), DEFAULT_BRANCH)?;
        let outcome = self.database.branches()?.delete(&branch)?;
        Ok(Output::BranchDeleteResult {
            branch: branch_item(outcome.branch()),
            generation_before: outcome.generation_before(),
            generation_after: outcome.generation_after(),
            cleanup: outcome.cleanup().map(branch_cleanup_item),
        })
    }

    fn execute_kv_put(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        key: Bytes,
        value: Bytes,
    ) -> ExecutorResult<Output> {
        let output_key = key.clone();
        let mut service = self.kv_service(branch, space)?;
        let outcome = service.put(kv_key(key)?, kv_value(value))?;
        Ok(write_output(output_key, outcome))
    }

    fn execute_kv_get(
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

    fn execute_kv_delete(
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

    fn execute_kv_list(
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
                keys: page.keys().iter().map(bytes_from_key).collect(),
                has_more: page.has_more(),
                cursor: page.cursor().map(bytes_from_key),
            });
        }
        let keys = service.list(prefix.as_ref())?;
        if cursor.is_some() {
            return page_or_keys(keys, cursor.as_ref(), limit);
        }
        Ok(Output::Keys(keys.iter().map(bytes_from_key).collect()))
    }

    fn execute_kv_scan(
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
        Ok(Output::KvScanResult(rows))
    }

    fn execute_kv_batch_put(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        entries: Vec<BatchKvEntry>,
    ) -> ExecutorResult<Output> {
        let mut service = self.kv_service(branch, space)?;
        if entries.is_empty() {
            return Ok(Output::BatchResults(Vec::new()));
        }
        let mut results = empty_batch_results(entries.len());
        let mut valid_entries = Vec::with_capacity(entries.len());
        for (index, entry) in entries.into_iter().enumerate() {
            let output_key = entry.key().clone();
            let (key, value) = entry.into_parts();
            match kv_key(key) {
                Ok(key) => valid_entries.push((index, output_key, key, kv_value(value))),
                Err(error) => {
                    results[index] = Some(BatchItemResult::failed(output_key, error.to_string()));
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
        let outcome = service.put_batch(engine_entries)?;
        for (index, output_key, ..) in valid_entries {
            results[index] = Some(batch_item_result(output_key, true, Some(outcome)));
        }
        Ok(Output::BatchResults(finish_batch_results(results)))
    }

    fn execute_kv_batch_get(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        keys: Vec<Bytes>,
    ) -> ExecutorResult<Output> {
        let mut service = self.kv_service(branch, space)?;
        if keys.is_empty() {
            return Ok(Output::BatchGetResults(Vec::new()));
        }
        let mut results = empty_batch_get_results(keys.len());
        let mut valid_keys = Vec::with_capacity(keys.len());
        for (index, key) in keys.into_iter().enumerate() {
            let output_key = key.clone();
            match kv_key(key) {
                Ok(key) => valid_keys.push((index, output_key, key)),
                Err(error) => {
                    results[index] =
                        Some(BatchGetItemResult::failed(output_key, error.to_string()));
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

    fn execute_kv_batch_delete(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        keys: Vec<Bytes>,
    ) -> ExecutorResult<Output> {
        let mut service = self.kv_service(branch, space)?;
        if keys.is_empty() {
            return Ok(Output::BatchResults(Vec::new()));
        }
        let mut results = empty_batch_results(keys.len());
        let mut valid_keys = Vec::with_capacity(keys.len());
        for (index, key) in keys.into_iter().enumerate() {
            let output_key = key.clone();
            match kv_key(key) {
                Ok(key) => valid_keys.push((index, output_key, key)),
                Err(error) => {
                    results[index] = Some(BatchItemResult::failed(output_key, error.to_string()));
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
            results[index] = Some(batch_item_result(output_key, deleted, commit));
        }
        Ok(Output::BatchResults(finish_batch_results(results)))
    }

    fn execute_kv_batch_exists(
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

    fn execute_kv_exists(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        key: Bytes,
    ) -> ExecutorResult<Output> {
        let key = kv_key(key)?;
        let mut service = self.kv_service(branch, space)?;
        Ok(Output::Bool(service.exists(&key)?))
    }

    fn execute_kv_getv(
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

    fn execute_kv_count(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        prefix: Option<Bytes>,
    ) -> ExecutorResult<Output> {
        let prefix = optional_key(prefix)?;
        let mut service = self.kv_service(branch, space)?;
        Ok(Output::Uint(service.count(prefix.as_ref())?))
    }

    fn execute_kv_sample(
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

    fn execute_json_set(
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
        Ok(json_write_output(key, outcome.commit()))
    }

    fn execute_json_get(
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
            return Ok(Output::JsonValue(value.map(json_value_output)));
        }
        Ok(Output::JsonVersionedValue(
            service
                .get_versioned(&id, &path)?
                .as_ref()
                .map(json_versioned_value),
        ))
    }

    fn execute_json_delete(
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

    fn execute_json_getv(
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

    fn execute_json_exists(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        key: String,
    ) -> ExecutorResult<Output> {
        let id = json_document_id(key)?;
        let mut service = self.json_service(branch, space)?;
        Ok(Output::Bool(service.exists(&id)?))
    }

    fn execute_json_batch_set(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        entries: Vec<BatchJsonEntry>,
    ) -> ExecutorResult<Output> {
        let mut service = self.json_service(branch, space)?;
        if entries.is_empty() {
            return Ok(Output::JsonBatchResults(Vec::new()));
        }
        let mut results = empty_json_batch_results(entries.len());
        let mut valid_entries = Vec::with_capacity(entries.len());
        for (index, entry) in entries.into_iter().enumerate() {
            let (key, path, value) = entry.into_parts();
            match json_set_entry(key, &path, value) {
                Ok(entry) => valid_entries.push((index, entry)),
                Err(error) => {
                    results[index] = Some(JsonBatchItemResult::failed(error.to_string()));
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
        let outcome = service.batch_set_or_create(engine_entries)?;
        for ((index, _), item) in valid_entries.into_iter().zip(outcome.results()) {
            results[index] = Some(json_batch_item_result(
                outcome.commit(),
                Some(item.document_version()),
            ));
        }
        Ok(Output::JsonBatchResults(finish_json_batch_results(results)))
    }

    fn execute_json_batch_get(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        entries: Vec<BatchJsonGetEntry>,
    ) -> ExecutorResult<Output> {
        let mut service = self.json_service(branch, space)?;
        if entries.is_empty() {
            return Ok(Output::JsonBatchGetResults(Vec::new()));
        }
        let mut results = empty_json_batch_get_results(entries.len());
        let mut valid_entries = Vec::with_capacity(entries.len());
        for (index, entry) in entries.into_iter().enumerate() {
            let (key, path) = entry.into_parts();
            match json_get_entry(key, &path) {
                Ok(entry) => valid_entries.push((index, entry)),
                Err(error) => {
                    results[index] = Some(JsonBatchGetItemResult::failed(error.to_string()));
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
            results[index] = Some(json_batch_get_result(value));
        }
        Ok(Output::JsonBatchGetResults(finish_json_batch_get_results(
            results,
        )))
    }

    fn execute_json_batch_delete(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        entries: Vec<BatchJsonDeleteEntry>,
    ) -> ExecutorResult<Output> {
        let mut service = self.json_service(branch, space)?;
        if entries.is_empty() {
            return Ok(Output::JsonBatchResults(Vec::new()));
        }
        let mut results = empty_json_batch_results(entries.len());
        let mut valid_entries = Vec::with_capacity(entries.len());
        for (index, entry) in entries.into_iter().enumerate() {
            let (key, path) = entry.into_parts();
            match json_get_entry(key, &path) {
                Ok(entry) => valid_entries.push((index, entry)),
                Err(error) => {
                    results[index] = Some(JsonBatchItemResult::failed(error.to_string()));
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
                deleted.then(|| outcome.commit()).flatten(),
                None,
            ));
        }
        Ok(Output::JsonBatchResults(finish_json_batch_results(results)))
    }

    fn execute_json_list(
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

    fn execute_json_count(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        prefix: Option<String>,
    ) -> ExecutorResult<Output> {
        let prefix = optional_json_prefix(prefix)?;
        let mut service = self.json_service(branch, space)?;
        Ok(Output::Uint(service.count(prefix.as_ref())?))
    }

    fn execute_json_sample(
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

    fn execute_json_create_index(
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

    fn execute_json_drop_index(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        name: String,
    ) -> ExecutorResult<Output> {
        let name = json_index_name(name)?;
        let mut service = self.json_service(branch, space)?;
        Ok(Output::Bool(service.drop_index(&name)?))
    }

    fn execute_json_list_indexes(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
    ) -> ExecutorResult<Output> {
        let mut service = self.json_service(branch, space)?;
        Ok(Output::JsonIndexList(
            service
                .list_indexes()?
                .iter()
                .map(json_index_definition)
                .collect(),
        ))
    }

    fn kv_service(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
    ) -> ExecutorResult<strata_engine_next::KvService<'_>> {
        let branch = branch_name(branch, &self.default_branch)?;
        let space = product_space(space)?;
        {
            let branches = self.database.branches()?;
            branches.get(&branch)?;
        }
        Ok(self.database.kv(branch, space)?)
    }

    fn json_service(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
    ) -> ExecutorResult<JsonService<'_>> {
        let branch = branch_name(branch, &self.default_branch)?;
        let space = product_space(space)?;
        {
            let branches = self.database.branches()?;
            branches.get(&branch)?;
        }
        Ok(self.database.json(branch, space)?)
    }
}

impl Executor {
    /// Executes a branch-list command.
    pub fn branch_list(&mut self) -> ExecutorResult<Output> {
        self.execute(Command::BranchList)
    }

    /// Executes a branch-get command.
    pub fn branch_get(&mut self, branch: impl Into<String>) -> ExecutorResult<Output> {
        self.execute(Command::BranchGet {
            branch: branch.into(),
        })
    }

    /// Executes a branch-create command.
    pub fn branch_create(&mut self, branch: impl Into<String>) -> ExecutorResult<Output> {
        self.execute(Command::BranchCreate {
            branch: branch.into(),
        })
    }

    /// Executes a branch-fork-current command.
    pub fn branch_fork_current(
        &mut self,
        source: impl Into<String>,
        branch: impl Into<String>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::BranchForkCurrent {
            source: source.into(),
            branch: branch.into(),
        })
    }

    /// Executes a branch-delete command.
    pub fn branch_delete(&mut self, branch: impl Into<String>) -> ExecutorResult<Output> {
        self.execute(Command::BranchDelete {
            branch: branch.into(),
        })
    }

    /// Executes a default-branch put command.
    pub fn kv_put(
        &mut self,
        key: impl Into<Bytes>,
        value: impl Into<Bytes>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::KvPut {
            branch: None,
            space: None,
            key: key.into(),
            value: value.into(),
        })
    }

    /// Executes a default-branch get command.
    pub fn kv_get(&mut self, key: impl Into<Bytes>) -> ExecutorResult<Output> {
        self.execute(Command::KvGet {
            branch: None,
            space: None,
            key: key.into(),
            as_of: None,
        })
    }

    /// Executes a default-branch delete command.
    pub fn kv_delete(&mut self, key: impl Into<Bytes>) -> ExecutorResult<Output> {
        self.execute(Command::KvDelete {
            branch: None,
            space: None,
            key: key.into(),
        })
    }

    /// Executes a default-branch list command.
    pub fn kv_list(&mut self, prefix: Option<Bytes>) -> ExecutorResult<Output> {
        self.execute(Command::KvList {
            branch: None,
            space: None,
            prefix,
            cursor: None,
            limit: None,
            as_of: None,
        })
    }

    /// Executes a default-branch scan command.
    pub fn kv_scan(&mut self, start: Option<Bytes>, limit: Option<u64>) -> ExecutorResult<Output> {
        self.execute(Command::KvScan {
            branch: None,
            space: None,
            start,
            limit,
        })
    }

    /// Executes a default-branch batch put command.
    pub fn kv_batch_put(&mut self, entries: Vec<BatchKvEntry>) -> ExecutorResult<Output> {
        self.execute(Command::KvBatchPut {
            branch: None,
            space: None,
            entries,
        })
    }

    /// Executes a default-branch batch get command.
    pub fn kv_batch_get(&mut self, keys: Vec<Bytes>) -> ExecutorResult<Output> {
        self.execute(Command::KvBatchGet {
            branch: None,
            space: None,
            keys,
        })
    }

    /// Executes a default-branch batch delete command.
    pub fn kv_batch_delete(&mut self, keys: Vec<Bytes>) -> ExecutorResult<Output> {
        self.execute(Command::KvBatchDelete {
            branch: None,
            space: None,
            keys,
        })
    }

    /// Executes a default-branch batch exists command.
    pub fn kv_batch_exists(&mut self, keys: Vec<Bytes>) -> ExecutorResult<Output> {
        self.execute(Command::KvBatchExists {
            branch: None,
            space: None,
            keys,
        })
    }

    /// Executes a default-branch exists command.
    pub fn kv_exists(&mut self, key: impl Into<Bytes>) -> ExecutorResult<Output> {
        self.execute(Command::KvExists {
            branch: None,
            space: None,
            key: key.into(),
        })
    }

    /// Executes a default-branch version-history command.
    pub fn kv_getv(&mut self, key: impl Into<Bytes>) -> ExecutorResult<Output> {
        self.execute(Command::KvGetv {
            branch: None,
            space: None,
            key: key.into(),
        })
    }

    /// Executes a default-branch count command.
    pub fn kv_count(&mut self, prefix: Option<Bytes>) -> ExecutorResult<Output> {
        self.execute(Command::KvCount {
            branch: None,
            space: None,
            prefix,
        })
    }

    /// Executes a default-branch sample command.
    pub fn kv_sample(
        &mut self,
        prefix: Option<Bytes>,
        count: Option<u64>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::KvSample {
            branch: None,
            space: None,
            prefix,
            count,
        })
    }

    /// Executes a default-branch JSON set command.
    pub fn json_set(
        &mut self,
        key: impl Into<String>,
        path: impl Into<String>,
        value: serde_json::Value,
    ) -> ExecutorResult<Output> {
        self.execute(Command::JsonSet {
            branch: None,
            space: None,
            key: key.into(),
            path: path.into(),
            value,
        })
    }

    /// Executes a default-branch JSON get command.
    pub fn json_get(
        &mut self,
        key: impl Into<String>,
        path: impl Into<String>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::JsonGet {
            branch: None,
            space: None,
            key: key.into(),
            path: path.into(),
            as_of: None,
        })
    }

    /// Executes a default-branch JSON delete command.
    pub fn json_delete(
        &mut self,
        key: impl Into<String>,
        path: impl Into<String>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::JsonDelete {
            branch: None,
            space: None,
            key: key.into(),
            path: path.into(),
        })
    }

    /// Executes a default-branch JSON batch set command.
    pub fn json_batch_set(&mut self, entries: Vec<BatchJsonEntry>) -> ExecutorResult<Output> {
        self.execute(Command::JsonBatchSet {
            branch: None,
            space: None,
            entries,
        })
    }

    /// Executes a default-branch JSON batch get command.
    pub fn json_batch_get(&mut self, entries: Vec<BatchJsonGetEntry>) -> ExecutorResult<Output> {
        self.execute(Command::JsonBatchGet {
            branch: None,
            space: None,
            entries,
        })
    }

    /// Executes a default-branch JSON batch delete command.
    pub fn json_batch_delete(
        &mut self,
        entries: Vec<BatchJsonDeleteEntry>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::JsonBatchDelete {
            branch: None,
            space: None,
            entries,
        })
    }
}

fn branch_name(branch: Option<&str>, default: &str) -> ExecutorResult<BranchName> {
    BranchName::new(branch.unwrap_or(default)).map_err(ExecutorError::from)
}

fn product_space(space: Option<&str>) -> ExecutorResult<ProductSpace> {
    ProductSpace::new(space.unwrap_or(DEFAULT_SPACE)).map_err(ExecutorError::from)
}

fn kv_key(key: Bytes) -> ExecutorResult<KvKey> {
    KvKey::new(key.into_vec()).map_err(ExecutorError::from)
}

fn optional_key(key: Option<Bytes>) -> ExecutorResult<Option<KvKey>> {
    key.map(kv_key).transpose()
}

fn kv_value(value: Bytes) -> KvValue {
    KvValue::new(value.into_vec())
}

fn json_document_id(key: impl Into<String>) -> ExecutorResult<JsonDocumentId> {
    JsonDocumentId::new(key).map_err(ExecutorError::from)
}

fn optional_json_document_id(key: Option<String>) -> ExecutorResult<Option<JsonDocumentId>> {
    key.map(json_document_id).transpose()
}

fn optional_json_prefix(key: Option<String>) -> ExecutorResult<Option<JsonDocumentId>> {
    match key {
        Some(key) if key.is_empty() => Ok(None),
        Some(key) => json_document_id(key).map(Some),
        None => Ok(None),
    }
}

fn json_path(path: &str) -> ExecutorResult<JsonPath> {
    path.parse().map_err(ExecutorError::from)
}

fn json_value(value: serde_json::Value) -> ExecutorResult<EngineJsonValue> {
    EngineJsonValue::new(value).map_err(ExecutorError::from)
}

fn json_index_name(name: String) -> ExecutorResult<JsonIndexName> {
    JsonIndexName::new(name).map_err(ExecutorError::from)
}

fn json_set_entry(
    key: String,
    path: &str,
    value: serde_json::Value,
) -> ExecutorResult<JsonSetEntry> {
    Ok(JsonSetEntry::new(
        json_document_id(key)?,
        json_path(path)?,
        json_value(value)?,
    ))
}

fn json_get_entry(key: String, path: &str) -> ExecutorResult<JsonGetEntry> {
    Ok(JsonGetEntry::new(json_document_id(key)?, json_path(path)?))
}

const fn engine_json_index_type(index_type: JsonIndexType) -> EngineJsonIndexType {
    match index_type {
        JsonIndexType::Numeric => EngineJsonIndexType::Numeric,
        JsonIndexType::Tag => EngineJsonIndexType::Tag,
        JsonIndexType::Text => EngineJsonIndexType::Text,
    }
}

const fn output_json_index_type(index_type: EngineJsonIndexType) -> JsonIndexType {
    match index_type {
        EngineJsonIndexType::Numeric => JsonIndexType::Numeric,
        EngineJsonIndexType::Tag => JsonIndexType::Tag,
        EngineJsonIndexType::Text => JsonIndexType::Text,
    }
}

fn optional_limit(limit: Option<u64>) -> ExecutorResult<Option<usize>> {
    limit
        .map(|limit| {
            usize::try_from(limit).map_err(|_| {
                ExecutorError::invalid_input(
                    "invalid_argument.executor.limit",
                    "limit does not fit this platform",
                )
            })
        })
        .transpose()
}

fn bytes_from_key(key: &KvKey) -> Bytes {
    Bytes::from(key.as_bytes())
}

fn bytes_from_value(value: KvValue) -> Bytes {
    Bytes::from(value.into_bytes())
}

fn branch_item(summary: &BranchSummary) -> BranchItem {
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

const fn branch_status(status: EngineBranchStatus) -> BranchStatus {
    match status {
        EngineBranchStatus::Active => BranchStatus::Active,
        EngineBranchStatus::Deleted => BranchStatus::Deleted,
    }
}

fn branch_cleanup_item(cleanup: BranchCleanupSummary) -> BranchCleanupItem {
    BranchCleanupItem::new(
        usize_to_u64(cleanup.removed_refs()),
        usize_to_u64(cleanup.releasable_tables()),
        usize_to_u64(cleanup.protected_tables()),
    )
}

fn usize_to_u64(value: usize) -> u64 {
    u64::try_from(value).unwrap_or(u64::MAX)
}

fn write_output(key: Bytes, outcome: CommitOutcome) -> Output {
    Output::WriteResult {
        key,
        version: outcome.version().as_u64(),
        timestamp: outcome.timestamp().as_micros(),
    }
}

fn delete_output(key: Bytes, deleted: bool, outcome: Option<CommitOutcome>) -> Output {
    Output::DeleteResult {
        key,
        deleted,
        version: outcome.map(|outcome| outcome.version().as_u64()),
        timestamp: outcome.map(|outcome| outcome.timestamp().as_micros()),
    }
}

fn batch_item_result(key: Bytes, applied: bool, outcome: Option<CommitOutcome>) -> BatchItemResult {
    BatchItemResult::new(
        key,
        applied,
        outcome.map(|outcome| outcome.version().as_u64()),
        outcome.map(|outcome| outcome.timestamp().as_micros()),
    )
}

fn batch_get_result(key: Bytes, value: Option<KvVersionedValue>) -> BatchGetItemResult {
    match value {
        Some(value) => BatchGetItemResult::new(
            key,
            Some(Bytes::from(value.value().as_bytes())),
            Some(value.version().as_u64()),
            Some(value.timestamp().as_micros()),
        ),
        None => BatchGetItemResult::new(key, None, None, None),
    }
}

fn versioned_value(value: &KvVersionedValue) -> VersionedValue {
    VersionedValue::new(
        Bytes::from(value.value().as_bytes()),
        value.version().as_u64(),
        value.timestamp().as_micros(),
    )
}

fn history_items(history: &KvHistory) -> Vec<HistoryItem> {
    history.rows().iter().map(history_item).collect()
}

fn history_item(row: &KvHistoryRow) -> HistoryItem {
    HistoryItem::new(
        row.value().map(|value| Bytes::from(value.as_bytes())),
        row.is_tombstone(),
        row.version().as_u64(),
        row.timestamp().as_micros(),
    )
}

fn scan_item(row: &KvScanRow) -> ScanItem {
    ScanItem::new(
        bytes_from_key(row.key()),
        Bytes::from(row.value().as_bytes()),
        row.version().as_u64(),
        row.timestamp().as_micros(),
    )
}

fn sample_item(row: &KvScanRow) -> SampleItem {
    SampleItem::new(
        bytes_from_key(row.key()),
        Bytes::from(row.value().as_bytes()),
        row.version().as_u64(),
        row.timestamp().as_micros(),
    )
}

fn sample_output(sample: &KvSample) -> Output {
    Output::SampleResult {
        total_count: sample.total_count(),
        items: sample.rows().iter().map(sample_item).collect(),
    }
}

fn json_write_output(key: &str, outcome: CommitOutcome) -> Output {
    write_output(Bytes::from(key), outcome)
}

fn json_delete_output(key: &str, deleted: bool, outcome: Option<CommitOutcome>) -> Output {
    delete_output(Bytes::from(key), deleted, outcome)
}

fn json_value_output(value: EngineJsonValue) -> serde_json::Value {
    value.into_inner()
}

fn json_versioned_value(value: &EngineJsonVersionedValue) -> OutputJsonVersionedValue {
    OutputJsonVersionedValue::new(
        value.value().clone().into_inner(),
        value.version().as_u64(),
        value.timestamp().as_micros(),
        value.document_version(),
    )
}

fn json_history_items(history: &JsonHistory) -> Vec<JsonHistoryItem> {
    history.rows().iter().map(json_history_item).collect()
}

fn json_history_item(row: &JsonHistoryRow) -> JsonHistoryItem {
    JsonHistoryItem::new(
        row.value().map(|value| value.clone().into_inner()),
        row.version().as_u64(),
        row.timestamp().as_micros(),
        row.document_version(),
        row.is_tombstone(),
    )
}

fn json_batch_item_result(
    outcome: Option<CommitOutcome>,
    document_version: Option<u64>,
) -> JsonBatchItemResult {
    JsonBatchItemResult::new(
        outcome.map(|outcome| outcome.version().as_u64()),
        outcome.map(|outcome| outcome.timestamp().as_micros()),
        document_version,
    )
}

fn json_batch_get_result(value: Option<EngineJsonVersionedValue>) -> JsonBatchGetItemResult {
    match value {
        Some(value) => JsonBatchGetItemResult::new(
            Some(value.value().clone().into_inner()),
            Some(value.version().as_u64()),
            Some(value.timestamp().as_micros()),
            Some(value.document_version()),
        ),
        None => JsonBatchGetItemResult::new(None, None, None, None),
    }
}

fn json_list_output(page: &JsonListPage) -> Output {
    Output::JsonListResult {
        keys: page
            .document_ids()
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect(),
        has_more: page.has_more(),
        cursor: page.cursor().map(|cursor| cursor.as_str().to_owned()),
    }
}

fn json_sample_output(sample: &EngineJsonSample) -> Output {
    Output::JsonSampleResult {
        total_count: sample.total_count(),
        items: sample.rows().iter().map(json_sample_item).collect(),
    }
}

fn json_sample_item(row: &JsonSampleRow) -> JsonSampleItem {
    JsonSampleItem::new(
        row.document_id().as_str().to_owned(),
        row.value().clone().into_inner(),
        row.version().as_u64(),
        row.timestamp().as_micros(),
        row.document_version(),
    )
}

fn json_index_definition(definition: &EngineJsonIndexDefinition) -> JsonIndexDefinition {
    JsonIndexDefinition::new(
        definition.name().as_str().to_owned(),
        definition.space().as_str().to_owned(),
        definition.field_path().to_string(),
        output_json_index_type(definition.index_type()),
        definition.created_version(),
        definition.created_timestamp(),
    )
}

fn page_or_keys(
    keys: Vec<KvKey>,
    cursor: Option<&KvKey>,
    limit: Option<u64>,
) -> ExecutorResult<Output> {
    if cursor.is_none() && limit.is_none() {
        return Ok(Output::Keys(keys.iter().map(bytes_from_key).collect()));
    }
    let limit = optional_limit(limit)?.unwrap_or(usize::MAX);
    if limit == 0 {
        return Ok(Output::KeysPage {
            keys: Vec::new(),
            has_more: false,
            cursor: None,
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
        keys: keys.iter().map(bytes_from_key).collect(),
        has_more,
        cursor: cursor.as_ref().map(bytes_from_key),
    })
}

fn empty_batch_results(len: usize) -> Vec<Option<BatchItemResult>> {
    std::iter::repeat_with(|| None).take(len).collect()
}

fn finish_batch_results(results: Vec<Option<BatchItemResult>>) -> Vec<BatchItemResult> {
    results
        .into_iter()
        .map(|result| result.expect("all batch result slots are filled"))
        .collect()
}

fn empty_batch_get_results(len: usize) -> Vec<Option<BatchGetItemResult>> {
    std::iter::repeat_with(|| None).take(len).collect()
}

fn finish_batch_get_results(results: Vec<Option<BatchGetItemResult>>) -> Vec<BatchGetItemResult> {
    results
        .into_iter()
        .map(|result| result.expect("all batch get result slots are filled"))
        .collect()
}

fn empty_json_batch_results(len: usize) -> Vec<Option<JsonBatchItemResult>> {
    std::iter::repeat_with(|| None).take(len).collect()
}

fn finish_json_batch_results(
    results: Vec<Option<JsonBatchItemResult>>,
) -> Vec<JsonBatchItemResult> {
    results
        .into_iter()
        .map(|result| result.expect("all JSON batch result slots are filled"))
        .collect()
}

fn empty_json_batch_get_results(len: usize) -> Vec<Option<JsonBatchGetItemResult>> {
    std::iter::repeat_with(|| None).take(len).collect()
}

fn finish_json_batch_get_results(
    results: Vec<Option<JsonBatchGetItemResult>>,
) -> Vec<JsonBatchGetItemResult> {
    results
        .into_iter()
        .map(|result| result.expect("all JSON batch get result slots are filled"))
        .collect()
}

fn reject_duplicate_valid_keys<'a>(
    keys: impl IntoIterator<Item = &'a Bytes>,
) -> ExecutorResult<()> {
    reject_duplicate_bytes(keys)
}

fn reject_duplicate_bytes<'a>(keys: impl IntoIterator<Item = &'a Bytes>) -> ExecutorResult<()> {
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
