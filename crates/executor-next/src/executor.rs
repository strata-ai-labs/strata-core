//! Executor handle and command dispatch.

use std::collections::BTreeSet;
use std::path::PathBuf;

use strata_core_next::{CommitVersion, Timestamp};
use strata_engine_next::{
    api::CommitOutcome, BranchCleanupSummary, BranchName, BranchStatus as EngineBranchStatus,
    BranchSummary, CacheOpenOptions, Database, DurableLocalOpenOptions,
    EventAppendOutcome as EngineEventAppendOutcome,
    EventBatchAppendEntry as EngineEventBatchAppendEntry,
    EventBatchAppendItemOutcome as EngineEventBatchAppendItemOutcome,
    EventChainVerification as EngineEventChainVerification, EventPayload as EngineEventPayload,
    EventRangeDirection as EngineEventRangeDirection, EventRangePage as EngineEventRangePage,
    EventSequence as EngineEventSequence, EventService, EventType as EngineEventType,
    EventVersionedRecord as EngineEventVersionedRecord, JsonDocumentId, JsonGetEntry, JsonHistory,
    JsonHistoryRow, JsonIndexDefinition as EngineJsonIndexDefinition, JsonIndexName,
    JsonIndexType as EngineJsonIndexType, JsonListPage, JsonPath, JsonSample as EngineJsonSample,
    JsonSampleRow, JsonService, JsonSetEntry, JsonValue as EngineJsonValue,
    JsonVersionedValue as EngineJsonVersionedValue, KvHistory, KvHistoryRow, KvKey, KvSample,
    KvScanRow, KvValue, KvVersionedValue, ProductSpace,
    VectorBulkDeleteOutcome as EngineVectorBulkDeleteOutcome,
    VectorCollectionInfo as EngineVectorCollectionInfo,
    VectorCollectionName as EngineVectorCollectionName, VectorConfig as EngineVectorConfig,
    VectorDistanceMetric as EngineVectorDistanceMetric, VectorEmbedding as EngineVectorEmbedding,
    VectorEntry as EngineVectorEntry, VectorFilter as EngineVectorFilter,
    VectorFilterCondition as EngineVectorFilterCondition, VectorFilterOp as EngineVectorFilterOp,
    VectorHistory as EngineVectorHistory, VectorHistoryRow as EngineVectorHistoryRow,
    VectorKey as EngineVectorKey, VectorKeyPage as EngineVectorKeyPage,
    VectorMetadata as EngineVectorMetadata, VectorMetadataPatch as EngineVectorMetadataPatch,
    VectorScalar as EngineVectorScalar, VectorSearchMatch as EngineVectorSearchMatch,
    VectorService, VectorUpsertEntry as EngineVectorUpsertEntry,
    VectorVersionedEntry as EngineVectorVersionedEntry,
};

use crate::command::Command;
use crate::error::{ExecutorError, ExecutorErrorClass, ExecutorResult};
use crate::output::Output;
use crate::types::{
    BatchEventEntry, BatchGetItemResult, BatchItemResult, BatchJsonDeleteEntry, BatchJsonEntry,
    BatchJsonGetEntry, BatchKvEntry, BatchVectorEntry, BranchCleanupItem, BranchItem,
    BranchParentItem, BranchStatus, Bytes, EventBatchAppendItemResult,
    EventChainVerification as OutputEventChainVerification, EventData, EventRangeDirection,
    EventVersionedData, HistoryItem, JsonBatchGetItemResult, JsonBatchItemResult, JsonHistoryItem,
    JsonIndexDefinition, JsonIndexType, JsonSampleItem,
    JsonVersionedValue as OutputJsonVersionedValue, SampleItem, ScanItem, VectorBatchGetItemResult,
    VectorBatchItemResult, VectorCollectionInfo as OutputVectorCollectionInfo, VectorData,
    VectorDistanceMetric, VectorFilterOp, VectorHistoryItem, VectorMatch, VectorMetadataFilter,
    VectorScalar, VectorVersionedData, VersionedValue, DEFAULT_BRANCH, DEFAULT_SPACE,
};

const DEFAULT_JSON_LIST_LIMIT: usize = 100;
const DEFAULT_VECTOR_LIST_LIMIT: usize = 100;

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
            Command::VectorCreateCollection {
                branch,
                space,
                collection,
                dimension,
                metric,
            } => self.execute_vector_create_collection(
                branch.as_deref(),
                space.as_deref(),
                collection,
                dimension,
                metric,
            ),
            Command::VectorDeleteCollection {
                branch,
                space,
                collection,
            } => self.execute_vector_delete_collection(
                branch.as_deref(),
                space.as_deref(),
                collection,
            ),
            Command::VectorListCollections { branch, space } => {
                self.execute_vector_list_collections(branch.as_deref(), space.as_deref())
            }
            Command::VectorCollectionStats {
                branch,
                space,
                collection,
            } => self.execute_vector_collection_stats(
                branch.as_deref(),
                space.as_deref(),
                collection,
            ),
            Command::VectorCount {
                branch,
                space,
                collection,
            } => self.execute_vector_count(branch.as_deref(), space.as_deref(), collection),
            Command::VectorUpsert {
                branch,
                space,
                collection,
                key,
                vector,
                metadata,
            } => self.execute_vector_upsert(
                branch.as_deref(),
                space.as_deref(),
                collection,
                key,
                vector,
                metadata,
            ),
            Command::VectorGet {
                branch,
                space,
                collection,
                key,
                as_of,
            } => {
                self.execute_vector_get(branch.as_deref(), space.as_deref(), collection, key, as_of)
            }
            Command::VectorGetv {
                branch,
                space,
                collection,
                key,
            } => self.execute_vector_getv(branch.as_deref(), space.as_deref(), collection, key),
            Command::VectorExists {
                branch,
                space,
                collection,
                key,
            } => self.execute_vector_exists(branch.as_deref(), space.as_deref(), collection, key),
            Command::VectorListKeys {
                branch,
                space,
                collection,
                prefix,
                cursor,
                limit,
            } => self.execute_vector_list_keys(
                branch.as_deref(),
                space.as_deref(),
                collection,
                prefix,
                cursor,
                limit,
            ),
            Command::VectorUpdateMetadata {
                branch,
                space,
                collection,
                key,
                patch,
            } => self.execute_vector_update_metadata(
                branch.as_deref(),
                space.as_deref(),
                collection,
                key,
                patch,
            ),
            Command::VectorDelete {
                branch,
                space,
                collection,
                key,
            } => self.execute_vector_delete(branch.as_deref(), space.as_deref(), collection, key),
            Command::VectorDeleteByFilter {
                branch,
                space,
                collection,
                filter,
            } => self.execute_vector_delete_by_filter(
                branch.as_deref(),
                space.as_deref(),
                collection,
                filter,
            ),
            Command::VectorDeleteAll {
                branch,
                space,
                collection,
            } => self.execute_vector_delete_all(branch.as_deref(), space.as_deref(), collection),
            Command::VectorQuery {
                branch,
                space,
                collection,
                query,
                k,
                filter,
                as_of,
            } => self.execute_vector_query(
                branch.as_deref(),
                space.as_deref(),
                collection,
                query,
                k,
                filter,
                as_of,
            ),
            Command::VectorBatchUpsert {
                branch,
                space,
                collection,
                entries,
            } => self.execute_vector_batch_upsert(
                branch.as_deref(),
                space.as_deref(),
                collection,
                entries,
            ),
            Command::VectorBatchGet {
                branch,
                space,
                collection,
                keys,
            } => {
                self.execute_vector_batch_get(branch.as_deref(), space.as_deref(), collection, keys)
            }
            Command::VectorBatchDelete {
                branch,
                space,
                collection,
                keys,
            } => self.execute_vector_batch_delete(
                branch.as_deref(),
                space.as_deref(),
                collection,
                keys,
            ),
            Command::EventBatchAppend {
                branch,
                space,
                entries,
            } => self.execute_event_batch_append(branch.as_deref(), space.as_deref(), entries),
            Command::EventAppend {
                branch,
                space,
                event_type,
                payload,
            } => {
                self.execute_event_append(branch.as_deref(), space.as_deref(), event_type, payload)
            }
            Command::EventGet {
                branch,
                space,
                sequence,
                as_of,
            } => self.execute_event_get(branch.as_deref(), space.as_deref(), sequence, as_of),
            Command::EventExists {
                branch,
                space,
                sequence,
            } => self.execute_event_exists(branch.as_deref(), space.as_deref(), sequence),
            Command::EventGetByType {
                branch,
                space,
                event_type,
                limit,
                after_sequence,
                as_of,
            } => self.execute_event_get_by_type(
                branch.as_deref(),
                space.as_deref(),
                event_type,
                limit,
                after_sequence,
                as_of,
            ),
            Command::EventLen {
                branch,
                space,
                as_of,
            } => self.execute_event_len(branch.as_deref(), space.as_deref(), as_of),
            Command::EventRange {
                branch,
                space,
                start_seq,
                end_seq,
                limit,
                direction,
                event_type,
            } => self.execute_event_range(
                branch.as_deref(),
                space.as_deref(),
                start_seq,
                end_seq,
                limit,
                direction,
                event_type,
            ),
            Command::EventRangeByTime {
                branch,
                space,
                start_ts,
                end_ts,
                limit,
                direction,
                event_type,
            } => self.execute_event_range_by_time(
                branch.as_deref(),
                space.as_deref(),
                start_ts,
                end_ts,
                limit,
                direction,
                event_type,
            ),
            Command::EventListTypes {
                branch,
                space,
                as_of,
            } => self.execute_event_list_types(branch.as_deref(), space.as_deref(), as_of),
            Command::EventList {
                branch,
                space,
                event_type,
                limit,
                as_of,
            } => self.execute_event_list(
                branch.as_deref(),
                space.as_deref(),
                event_type,
                limit,
                as_of,
            ),
            Command::EventVerifyChain { branch, space } => {
                self.execute_event_verify_chain(branch.as_deref(), space.as_deref())
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

    fn execute_vector_create_collection(
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
        Ok(Output::VectorCollectionList(vec![vector_collection_info(
            &service.create_collection(collection, config)?,
        )]))
    }

    fn execute_vector_delete_collection(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        collection: String,
    ) -> ExecutorResult<Output> {
        let collection = vector_collection(collection)?;
        let mut service = self.vector_service(branch, space)?;
        Ok(Output::Bool(service.delete_collection(&collection)?))
    }

    fn execute_vector_list_collections(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
    ) -> ExecutorResult<Output> {
        let mut service = self.vector_service(branch, space)?;
        Ok(Output::VectorCollectionList(
            service
                .list_collections()?
                .iter()
                .map(vector_collection_info)
                .collect(),
        ))
    }

    fn execute_vector_collection_stats(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        collection: String,
    ) -> ExecutorResult<Output> {
        let collection = vector_collection(collection)?;
        let mut service = self.vector_service(branch, space)?;
        let Some(info) = service.collection_info(&collection)? else {
            return Err(ExecutorError::not_found(
                "not_found.executor.vector_collection",
                "vector collection does not exist",
            ));
        };
        Ok(Output::VectorCollectionList(vec![vector_collection_info(
            &info,
        )]))
    }

    fn execute_vector_count(
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
    fn execute_vector_upsert(
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
        ))
    }

    fn execute_vector_get(
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

    fn execute_vector_getv(
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
                .map(|history| vector_history_items(&key, history)),
        ))
    }

    fn execute_vector_exists(
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

    fn execute_vector_list_keys(
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

    fn execute_vector_update_metadata(
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
            updated: outcome.updated(),
            version: outcome.commit().map(|outcome| outcome.version().as_u64()),
            timestamp: outcome
                .commit()
                .map(|outcome| outcome.timestamp().as_micros()),
            vector_revision: outcome.vector_revision(),
        })
    }

    fn execute_vector_delete(
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
            deleted: outcome.deleted(),
            version: outcome.commit().map(|outcome| outcome.version().as_u64()),
            timestamp: outcome
                .commit()
                .map(|outcome| outcome.timestamp().as_micros()),
        })
    }

    fn execute_vector_delete_by_filter(
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

    fn execute_vector_delete_all(
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
    fn execute_vector_query(
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

    fn execute_vector_batch_upsert(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        collection: String,
        entries: Vec<BatchVectorEntry>,
    ) -> ExecutorResult<Output> {
        let collection = vector_collection(collection)?;
        let entries = entries
            .into_iter()
            .map(vector_upsert_entry)
            .collect::<ExecutorResult<Vec<_>>>()?;
        let mut service = self.vector_service(branch, space)?;
        let outcome = service.batch_upsert(&collection, &entries)?;
        Ok(Output::VectorBatchUpsertResults(
            outcome
                .vector_revisions()
                .iter()
                .copied()
                .map(|revision| vector_batch_item_result(true, outcome.commit(), Some(revision)))
                .collect(),
        ))
    }

    fn execute_vector_batch_get(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        collection: String,
        keys: Vec<String>,
    ) -> ExecutorResult<Output> {
        let collection = vector_collection(collection)?;
        let keys = keys
            .into_iter()
            .map(vector_key)
            .collect::<ExecutorResult<Vec<_>>>()?;
        let mut service = self.vector_service(branch, space)?;
        let outcome = service.batch_get(&collection, &keys)?;
        Ok(Output::VectorBatchGetResults(
            outcome
                .entries()
                .iter()
                .map(|entry| {
                    VectorBatchGetItemResult::new(entry.as_ref().map(vector_versioned_data))
                })
                .collect(),
        ))
    }

    fn execute_vector_batch_delete(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        collection: String,
        keys: Vec<String>,
    ) -> ExecutorResult<Output> {
        let collection = vector_collection(collection)?;
        let keys = keys
            .into_iter()
            .map(vector_key)
            .collect::<ExecutorResult<Vec<_>>>()?;
        let mut service = self.vector_service(branch, space)?;
        let outcome = service.batch_delete(&collection, &keys)?;
        Ok(Output::VectorBatchDeleteResults(
            outcome
                .deleted()
                .iter()
                .copied()
                .map(|deleted| {
                    vector_batch_item_result(
                        deleted,
                        deleted.then(|| outcome.commit()).flatten(),
                        None,
                    )
                })
                .collect(),
        ))
    }

    fn execute_event_batch_append(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        entries: Vec<BatchEventEntry>,
    ) -> ExecutorResult<Output> {
        let entries = entries
            .into_iter()
            .map(event_batch_entry)
            .collect::<Vec<_>>();
        let mut service = self.event_service(branch, space)?;
        let outcome = service.batch_append(entries)?;
        Ok(Output::EventBatchAppendResults(
            outcome
                .items()
                .iter()
                .map(event_batch_append_item_result)
                .collect(),
        ))
    }

    fn execute_event_append(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        event_type: String,
        payload: serde_json::Value,
    ) -> ExecutorResult<Output> {
        let event_type = engine_event_type(event_type)?;
        let payload = event_payload(payload)?;
        let mut service = self.event_service(branch, space)?;
        Ok(event_append_output(&service.append(event_type, payload)?))
    }

    fn execute_event_get(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        sequence: u64,
        as_of: Option<u64>,
    ) -> ExecutorResult<Output> {
        let sequence = event_sequence(sequence);
        let mut service = self.event_service(branch, space)?;
        let record = if let Some(as_of) = as_of {
            service.get_at(sequence, Timestamp::from_micros(as_of))?
        } else {
            service.get(sequence)?
        };
        Ok(Output::EventRecord(
            record.as_ref().map(event_versioned_data),
        ))
    }

    fn execute_event_exists(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        sequence: u64,
    ) -> ExecutorResult<Output> {
        let sequence = event_sequence(sequence);
        let mut service = self.event_service(branch, space)?;
        Ok(Output::Bool(service.exists(sequence)?))
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_event_get_by_type(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        event_type: String,
        limit: Option<u64>,
        after_sequence: Option<u64>,
        as_of: Option<u64>,
    ) -> ExecutorResult<Output> {
        let event_type = engine_event_type(event_type)?;
        let limit = optional_limit(limit)?;
        let after_sequence = after_sequence.map(event_sequence);
        let mut service = self.event_service(branch, space)?;
        let records = if let Some(as_of) = as_of {
            service.get_by_type_at(
                &event_type,
                Timestamp::from_micros(as_of),
                after_sequence,
                limit,
            )?
        } else {
            service.get_by_type(&event_type, after_sequence, limit)?
        };
        Ok(Output::EventRecords(event_records(&records)))
    }

    fn execute_event_len(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        as_of: Option<u64>,
    ) -> ExecutorResult<Output> {
        let mut service = self.event_service(branch, space)?;
        let count = if let Some(as_of) = as_of {
            service.len_at(Timestamp::from_micros(as_of))?.count()
        } else {
            service.len()?.count()
        };
        Ok(Output::EventLength { count })
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_event_range(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        start_seq: u64,
        end_seq: Option<u64>,
        limit: Option<u64>,
        direction: EventRangeDirection,
        event_type: Option<String>,
    ) -> ExecutorResult<Output> {
        let end_seq = end_seq.map(event_sequence);
        let limit = optional_limit(limit)?;
        let direction = engine_event_direction(direction);
        let event_type = optional_engine_event_type(event_type)?;
        let mut service = self.event_service(branch, space)?;
        let page = service.range(
            event_sequence(start_seq),
            end_seq,
            limit,
            direction,
            event_type.as_ref(),
        )?;
        Ok(event_range_output(&page))
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_event_range_by_time(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        start_ts: u64,
        end_ts: Option<u64>,
        limit: Option<u64>,
        direction: EventRangeDirection,
        event_type: Option<String>,
    ) -> ExecutorResult<Output> {
        let end_ts = end_ts.map(Timestamp::from_micros);
        let limit = optional_limit(limit)?;
        let direction = engine_event_direction(direction);
        let event_type = optional_engine_event_type(event_type)?;
        let mut service = self.event_service(branch, space)?;
        let page = service.range_by_time(
            Timestamp::from_micros(start_ts),
            end_ts,
            limit,
            direction,
            event_type.as_ref(),
        )?;
        Ok(event_range_output(&page))
    }

    fn execute_event_list_types(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        as_of: Option<u64>,
    ) -> ExecutorResult<Output> {
        let mut service = self.event_service(branch, space)?;
        let types = if let Some(as_of) = as_of {
            service.list_types_at(Timestamp::from_micros(as_of))?
        } else {
            service.list_types()?
        };
        Ok(Output::EventTypeList(
            types
                .event_types()
                .iter()
                .map(|event_type| event_type.as_str().to_owned())
                .collect(),
        ))
    }

    fn execute_event_list(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        event_type: Option<String>,
        limit: Option<u64>,
        as_of: Option<u64>,
    ) -> ExecutorResult<Output> {
        let event_type = optional_engine_event_type(event_type)?;
        let limit = optional_limit(limit)?;
        let as_of = as_of.map(Timestamp::from_micros);
        let mut service = self.event_service(branch, space)?;
        let records = service.list(event_type.as_ref(), limit, as_of)?;
        Ok(Output::EventRecords(event_records(&records)))
    }

    fn execute_event_verify_chain(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
    ) -> ExecutorResult<Output> {
        let mut service = self.event_service(branch, space)?;
        Ok(Output::EventChainVerification(event_chain_verification(
            &service.verify_chain()?,
        )))
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

    fn vector_service(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
    ) -> ExecutorResult<VectorService<'_>> {
        let branch = branch_name(branch, &self.default_branch)?;
        let space = product_space(space)?;
        {
            let branches = self.database.branches()?;
            branches.get(&branch)?;
        }
        Ok(self.database.vector(branch, space)?)
    }

    fn event_service(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
    ) -> ExecutorResult<EventService<'_>> {
        let branch = branch_name(branch, &self.default_branch)?;
        let space = product_space(space)?;
        {
            let branches = self.database.branches()?;
            branches.get(&branch)?;
        }
        Ok(self.database.event(branch, space)?)
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

    /// Executes a default-branch vector collection-create command.
    pub fn vector_create_collection(
        &mut self,
        collection: impl Into<String>,
        dimension: u64,
        metric: VectorDistanceMetric,
    ) -> ExecutorResult<Output> {
        self.execute(Command::VectorCreateCollection {
            branch: None,
            space: None,
            collection: collection.into(),
            dimension,
            metric,
        })
    }

    /// Executes a default-branch vector collection-delete command.
    pub fn vector_delete_collection(
        &mut self,
        collection: impl Into<String>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::VectorDeleteCollection {
            branch: None,
            space: None,
            collection: collection.into(),
        })
    }

    /// Executes a default-branch vector collection-list command.
    pub fn vector_list_collections(&mut self) -> ExecutorResult<Output> {
        self.execute(Command::VectorListCollections {
            branch: None,
            space: None,
        })
    }

    /// Executes a default-branch vector collection-stats command.
    pub fn vector_collection_stats(
        &mut self,
        collection: impl Into<String>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::VectorCollectionStats {
            branch: None,
            space: None,
            collection: collection.into(),
        })
    }

    /// Executes a default-branch vector count command.
    pub fn vector_count(&mut self, collection: impl Into<String>) -> ExecutorResult<Output> {
        self.execute(Command::VectorCount {
            branch: None,
            space: None,
            collection: collection.into(),
        })
    }

    /// Executes a default-branch vector upsert command.
    pub fn vector_upsert(
        &mut self,
        collection: impl Into<String>,
        key: impl Into<String>,
        vector: Vec<f32>,
        metadata: Option<serde_json::Value>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::VectorUpsert {
            branch: None,
            space: None,
            collection: collection.into(),
            key: key.into(),
            vector,
            metadata,
        })
    }

    /// Executes a default-branch vector get command.
    pub fn vector_get(
        &mut self,
        collection: impl Into<String>,
        key: impl Into<String>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::VectorGet {
            branch: None,
            space: None,
            collection: collection.into(),
            key: key.into(),
            as_of: None,
        })
    }

    /// Executes a default-branch vector history command.
    pub fn vector_getv(
        &mut self,
        collection: impl Into<String>,
        key: impl Into<String>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::VectorGetv {
            branch: None,
            space: None,
            collection: collection.into(),
            key: key.into(),
        })
    }

    /// Executes a default-branch vector exists command.
    pub fn vector_exists(
        &mut self,
        collection: impl Into<String>,
        key: impl Into<String>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::VectorExists {
            branch: None,
            space: None,
            collection: collection.into(),
            key: key.into(),
        })
    }

    /// Executes a default-branch vector key-list command.
    pub fn vector_list_keys(
        &mut self,
        collection: impl Into<String>,
        prefix: Option<String>,
        cursor: Option<String>,
        limit: Option<u64>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::VectorListKeys {
            branch: None,
            space: None,
            collection: collection.into(),
            prefix,
            cursor,
            limit,
        })
    }

    /// Executes a default-branch vector metadata-update command.
    pub fn vector_update_metadata(
        &mut self,
        collection: impl Into<String>,
        key: impl Into<String>,
        patch: serde_json::Value,
    ) -> ExecutorResult<Output> {
        self.execute(Command::VectorUpdateMetadata {
            branch: None,
            space: None,
            collection: collection.into(),
            key: key.into(),
            patch,
        })
    }

    /// Executes a default-branch vector delete command.
    pub fn vector_delete(
        &mut self,
        collection: impl Into<String>,
        key: impl Into<String>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::VectorDelete {
            branch: None,
            space: None,
            collection: collection.into(),
            key: key.into(),
        })
    }

    /// Executes a default-branch vector filtered-delete command.
    pub fn vector_delete_by_filter(
        &mut self,
        collection: impl Into<String>,
        filter: VectorMetadataFilter,
    ) -> ExecutorResult<Output> {
        self.execute(Command::VectorDeleteByFilter {
            branch: None,
            space: None,
            collection: collection.into(),
            filter,
        })
    }

    /// Executes a default-branch vector delete-all command.
    pub fn vector_delete_all(&mut self, collection: impl Into<String>) -> ExecutorResult<Output> {
        self.execute(Command::VectorDeleteAll {
            branch: None,
            space: None,
            collection: collection.into(),
        })
    }

    /// Executes a default-branch vector query command.
    pub fn vector_query(
        &mut self,
        collection: impl Into<String>,
        query: Vec<f32>,
        k: u64,
        filter: Option<VectorMetadataFilter>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::VectorQuery {
            branch: None,
            space: None,
            collection: collection.into(),
            query,
            k,
            filter,
            as_of: None,
        })
    }

    /// Executes a default-branch vector batch-upsert command.
    pub fn vector_batch_upsert(
        &mut self,
        collection: impl Into<String>,
        entries: Vec<BatchVectorEntry>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::VectorBatchUpsert {
            branch: None,
            space: None,
            collection: collection.into(),
            entries,
        })
    }

    /// Executes a default-branch vector batch-get command.
    pub fn vector_batch_get(
        &mut self,
        collection: impl Into<String>,
        keys: Vec<String>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::VectorBatchGet {
            branch: None,
            space: None,
            collection: collection.into(),
            keys,
        })
    }

    /// Executes a default-branch vector batch-delete command.
    pub fn vector_batch_delete(
        &mut self,
        collection: impl Into<String>,
        keys: Vec<String>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::VectorBatchDelete {
            branch: None,
            space: None,
            collection: collection.into(),
            keys,
        })
    }

    /// Executes a default-branch event batch-append command.
    pub fn event_batch_append(&mut self, entries: Vec<BatchEventEntry>) -> ExecutorResult<Output> {
        self.execute(Command::EventBatchAppend {
            branch: None,
            space: None,
            entries,
        })
    }

    /// Executes a default-branch event append command.
    pub fn event_append(
        &mut self,
        event_type: impl Into<String>,
        payload: serde_json::Value,
    ) -> ExecutorResult<Output> {
        self.execute(Command::EventAppend {
            branch: None,
            space: None,
            event_type: event_type.into(),
            payload,
        })
    }

    /// Executes a default-branch event get command.
    pub fn event_get(&mut self, sequence: u64) -> ExecutorResult<Output> {
        self.execute(Command::EventGet {
            branch: None,
            space: None,
            sequence,
            as_of: None,
        })
    }

    /// Executes a default-branch event exists command.
    pub fn event_exists(&mut self, sequence: u64) -> ExecutorResult<Output> {
        self.execute(Command::EventExists {
            branch: None,
            space: None,
            sequence,
        })
    }

    /// Executes a default-branch event type-filter command.
    pub fn event_get_by_type(
        &mut self,
        event_type: impl Into<String>,
        limit: Option<u64>,
        after_sequence: Option<u64>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::EventGetByType {
            branch: None,
            space: None,
            event_type: event_type.into(),
            limit,
            after_sequence,
            as_of: None,
        })
    }

    /// Executes a default-branch event length command.
    pub fn event_len(&mut self) -> ExecutorResult<Output> {
        self.execute(Command::EventLen {
            branch: None,
            space: None,
            as_of: None,
        })
    }

    /// Executes a default-branch event sequence-range command.
    pub fn event_range(
        &mut self,
        start_seq: u64,
        end_seq: Option<u64>,
        limit: Option<u64>,
        direction: EventRangeDirection,
        event_type: Option<String>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::EventRange {
            branch: None,
            space: None,
            start_seq,
            end_seq,
            limit,
            direction,
            event_type,
        })
    }

    /// Executes a default-branch event timestamp-range command.
    pub fn event_range_by_time(
        &mut self,
        start_ts: u64,
        end_ts: Option<u64>,
        limit: Option<u64>,
        direction: EventRangeDirection,
        event_type: Option<String>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::EventRangeByTime {
            branch: None,
            space: None,
            start_ts,
            end_ts,
            limit,
            direction,
            event_type,
        })
    }

    /// Executes a default-branch event type-list command.
    pub fn event_list_types(&mut self) -> ExecutorResult<Output> {
        self.execute(Command::EventListTypes {
            branch: None,
            space: None,
            as_of: None,
        })
    }

    /// Executes a default-branch event list command.
    pub fn event_list(
        &mut self,
        event_type: Option<String>,
        limit: Option<u64>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::EventList {
            branch: None,
            space: None,
            event_type,
            limit,
            as_of: None,
        })
    }

    /// Executes a default-branch event chain-verify command.
    pub fn event_verify_chain(&mut self) -> ExecutorResult<Output> {
        self.execute(Command::EventVerifyChain {
            branch: None,
            space: None,
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

fn required_usize(value: u64, code: &'static str, message: &'static str) -> ExecutorResult<usize> {
    usize::try_from(value).map_err(|_| ExecutorError::invalid_input(code, message))
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

fn vector_collection(name: String) -> ExecutorResult<EngineVectorCollectionName> {
    EngineVectorCollectionName::new(name).map_err(ExecutorError::from)
}

fn vector_key(key: String) -> ExecutorResult<EngineVectorKey> {
    EngineVectorKey::new(key).map_err(ExecutorError::from)
}

fn optional_vector_key(key: Option<String>) -> ExecutorResult<Option<EngineVectorKey>> {
    key.map(vector_key).transpose()
}

fn vector_embedding(vector: Vec<f32>) -> ExecutorResult<EngineVectorEmbedding> {
    EngineVectorEmbedding::new(vector).map_err(ExecutorError::from)
}

fn optional_vector_metadata(
    metadata: Option<serde_json::Value>,
) -> ExecutorResult<Option<EngineVectorMetadata>> {
    metadata
        .map(EngineVectorMetadata::new)
        .transpose()
        .map_err(ExecutorError::from)
}

fn vector_metadata_patch(value: serde_json::Value) -> ExecutorResult<EngineVectorMetadataPatch> {
    EngineVectorMetadataPatch::new(value).map_err(ExecutorError::from)
}

const fn engine_vector_metric(metric: VectorDistanceMetric) -> EngineVectorDistanceMetric {
    match metric {
        VectorDistanceMetric::Cosine => EngineVectorDistanceMetric::Cosine,
        VectorDistanceMetric::Euclidean => EngineVectorDistanceMetric::Euclidean,
        VectorDistanceMetric::DotProduct => EngineVectorDistanceMetric::DotProduct,
    }
}

const fn output_vector_metric(metric: EngineVectorDistanceMetric) -> VectorDistanceMetric {
    match metric {
        EngineVectorDistanceMetric::Cosine => VectorDistanceMetric::Cosine,
        EngineVectorDistanceMetric::Euclidean => VectorDistanceMetric::Euclidean,
        EngineVectorDistanceMetric::DotProduct => VectorDistanceMetric::DotProduct,
    }
}

fn engine_vector_scalar(value: VectorScalar) -> EngineVectorScalar {
    match value {
        VectorScalar::Null => EngineVectorScalar::Null,
        VectorScalar::Bool(value) => EngineVectorScalar::Bool(value),
        VectorScalar::Number(value) => EngineVectorScalar::Number(value),
        VectorScalar::String(value) => EngineVectorScalar::String(value),
    }
}

const fn engine_vector_filter_op(op: VectorFilterOp) -> EngineVectorFilterOp {
    match op {
        VectorFilterOp::Eq => EngineVectorFilterOp::Eq,
    }
}

fn vector_filter(filter: VectorMetadataFilter) -> ExecutorResult<EngineVectorFilter> {
    let conditions = filter
        .into_conditions()
        .into_iter()
        .map(|condition| {
            let (field, op, value) = condition.into_parts();
            EngineVectorFilterCondition::new(
                field,
                engine_vector_filter_op(op),
                engine_vector_scalar(value),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(EngineVectorFilter::from_conditions(conditions))
}

fn vector_upsert_entry(entry: BatchVectorEntry) -> ExecutorResult<EngineVectorUpsertEntry> {
    let (key, vector, metadata) = entry.into_parts();
    Ok(EngineVectorUpsertEntry::new(
        vector_key(key)?,
        vector_embedding(vector)?,
        optional_vector_metadata(metadata)?,
    ))
}

fn vector_collection_info(info: &EngineVectorCollectionInfo) -> OutputVectorCollectionInfo {
    OutputVectorCollectionInfo::new(
        info.name().as_str().to_owned(),
        usize_to_u64(info.config().dimension()),
        output_vector_metric(info.config().metric()),
        info.count(),
    )
}

fn vector_data(entry: &EngineVectorEntry) -> VectorData {
    VectorData::new(
        entry.embedding().as_slice().to_vec(),
        entry.metadata().map(|metadata| metadata.as_inner().clone()),
    )
}

fn vector_versioned_data(entry: &EngineVectorVersionedEntry) -> VectorVersionedData {
    VectorVersionedData::new(
        entry.key().as_str().to_owned(),
        vector_data(entry.entry()),
        entry.version().as_u64(),
        entry.timestamp().as_micros(),
        entry.vector_revision(),
    )
}

fn vector_history_items(
    key: &EngineVectorKey,
    history: &EngineVectorHistory,
) -> Vec<VectorHistoryItem> {
    history
        .rows()
        .iter()
        .map(|row| vector_history_item(key, row))
        .collect()
}

fn vector_history_item(key: &EngineVectorKey, row: &EngineVectorHistoryRow) -> VectorHistoryItem {
    VectorHistoryItem::new(
        key.as_str().to_owned(),
        row.entry().map(vector_data),
        row.version().as_u64(),
        row.timestamp().as_micros(),
        row.vector_revision(),
        row.is_tombstone(),
    )
}

fn vector_match(value: &EngineVectorSearchMatch) -> VectorMatch {
    VectorMatch::new(
        value.entry().key().as_str().to_owned(),
        value.score(),
        value
            .entry()
            .metadata()
            .map(|metadata| metadata.as_inner().clone()),
    )
}

fn vector_key_page_output(page: &EngineVectorKeyPage) -> Output {
    Output::VectorKeyPage {
        keys: page
            .keys()
            .iter()
            .map(|key| key.as_str().to_owned())
            .collect(),
        has_more: page.has_more(),
        cursor: page.cursor().map(|cursor| cursor.as_str().to_owned()),
    }
}

fn vector_write_output(
    collection: &EngineVectorCollectionName,
    key: &EngineVectorKey,
    outcome: CommitOutcome,
    vector_revision: u64,
) -> Output {
    Output::VectorWriteResult {
        collection: collection.as_str().to_owned(),
        key: key.as_str().to_owned(),
        version: outcome.version().as_u64(),
        timestamp: outcome.timestamp().as_micros(),
        vector_revision,
    }
}

fn vector_bulk_delete_output(
    collection: &EngineVectorCollectionName,
    outcome: EngineVectorBulkDeleteOutcome,
) -> Output {
    Output::VectorBulkDeleteResult {
        collection: collection.as_str().to_owned(),
        deleted_count: outcome.deleted_count(),
        version: outcome.commit().map(|outcome| outcome.version().as_u64()),
        timestamp: outcome
            .commit()
            .map(|outcome| outcome.timestamp().as_micros()),
    }
}

fn vector_batch_item_result(
    applied: bool,
    outcome: Option<CommitOutcome>,
    vector_revision: Option<u64>,
) -> VectorBatchItemResult {
    VectorBatchItemResult::new(
        applied,
        outcome.map(|outcome| outcome.version().as_u64()),
        outcome.map(|outcome| outcome.timestamp().as_micros()),
        vector_revision,
    )
}

fn engine_event_type(event_type: String) -> ExecutorResult<EngineEventType> {
    EngineEventType::new(event_type).map_err(ExecutorError::from)
}

fn optional_engine_event_type(
    event_type: Option<String>,
) -> ExecutorResult<Option<EngineEventType>> {
    event_type.map(engine_event_type).transpose()
}

fn event_payload(payload: serde_json::Value) -> ExecutorResult<EngineEventPayload> {
    EngineEventPayload::new(payload).map_err(ExecutorError::from)
}

const fn event_sequence(sequence: u64) -> EngineEventSequence {
    EngineEventSequence::new(sequence)
}

fn event_batch_entry(entry: BatchEventEntry) -> EngineEventBatchAppendEntry {
    let (event_type, payload) = entry.into_parts();
    EngineEventBatchAppendEntry::from_raw(event_type, payload)
}

const fn engine_event_direction(direction: EventRangeDirection) -> EngineEventRangeDirection {
    match direction {
        EventRangeDirection::Forward => EngineEventRangeDirection::Forward,
        EventRangeDirection::Reverse => EngineEventRangeDirection::Reverse,
    }
}

fn event_append_output(outcome: &EngineEventAppendOutcome) -> Output {
    let commit = outcome.commit();
    Output::EventAppendResult {
        sequence: outcome.sequence().as_u64(),
        event_type: outcome.event_type().as_str().to_owned(),
        version: commit.version().as_u64(),
        timestamp: commit.timestamp().as_micros(),
    }
}

fn event_records(records: &[EngineEventVersionedRecord]) -> Vec<EventVersionedData> {
    records.iter().map(event_versioned_data).collect()
}

fn event_versioned_data(record: &EngineEventVersionedRecord) -> EventVersionedData {
    EventVersionedData::new(
        event_data(record),
        record.version().as_u64(),
        record.commit_timestamp().as_micros(),
    )
}

fn event_data(record: &EngineEventVersionedRecord) -> EventData {
    EventData::new(
        record.sequence().as_u64(),
        record.event_type().as_str().to_owned(),
        record.payload().as_inner().clone(),
        record.timestamp().as_micros(),
        hash_hex(record.previous_hash()),
        hash_hex(record.hash()),
    )
}

fn hash_hex(hash: [u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(hash.len().saturating_mul(2));
    for byte in hash {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

fn event_range_output(page: &EngineEventRangePage) -> Output {
    Output::EventRangeResult {
        events: event_records(page.events()),
        has_more: page.has_more(),
        cursor: page.cursor().map(EngineEventSequence::as_u64),
    }
}

fn event_batch_append_item_result(
    item: &EngineEventBatchAppendItemOutcome,
) -> EventBatchAppendItemResult {
    if let Some(error) = item.error_message() {
        return EventBatchAppendItemResult::failed(error);
    }
    EventBatchAppendItemResult::new(
        item.sequence().map(EngineEventSequence::as_u64),
        item.event_type()
            .map(|event_type| event_type.as_str().to_owned()),
        item.commit_version().map(CommitVersion::as_u64),
        item.commit_timestamp().map(Timestamp::as_micros),
    )
}

fn event_chain_verification(
    verification: &EngineEventChainVerification,
) -> OutputEventChainVerification {
    OutputEventChainVerification::new(
        verification.is_valid(),
        verification.length(),
        verification
            .first_invalid()
            .map(EngineEventSequence::as_u64),
        verification.error_message().map(str::to_owned),
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
