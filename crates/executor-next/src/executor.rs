//! Executor handle and command dispatch.

use std::collections::BTreeSet;
use std::path::PathBuf;

use strata_core_next::{CommitVersion, Timestamp};
use strata_engine_next::{
    api::CommitOutcome, AdminCapabilitySummary as EngineAdminCapabilitySummary,
    AdminConfigSummary as EngineAdminConfigSummary, AdminDatabaseInfo as EngineAdminDatabaseInfo,
    AdminDescribeSummary as EngineAdminDescribeSummary,
    AdminGraphSummary as EngineAdminGraphSummary, AdminHealthStatus as EngineAdminHealthStatus,
    AdminHealthSummary as EngineAdminHealthSummary,
    AdminMetricsSummary as EngineAdminMetricsSummary,
    AdminPrimitiveSummary as EngineAdminPrimitiveSummary,
    AdminVectorCollectionSummary as EngineAdminVectorCollectionSummary, BranchCleanupSummary,
    BranchName, BranchStatus as EngineBranchStatus, BranchSummary, CacheOpenOptions,
    ControlHealthStatus as EngineControlHealthStatus, Database,
    DatabaseOpenTarget as EngineDatabaseOpenTarget, DurableLocalOpenOptions,
    EventAppendOutcome as EngineEventAppendOutcome,
    EventBatchAppendEntry as EngineEventBatchAppendEntry,
    EventBatchAppendItemOutcome as EngineEventBatchAppendItemOutcome,
    EventChainVerification as EngineEventChainVerification, EventPayload as EngineEventPayload,
    EventRangeDirection as EngineEventRangeDirection, EventRangePage as EngineEventRangePage,
    EventSequence as EngineEventSequence, EventService, EventType as EngineEventType,
    EventVersionedRecord as EngineEventVersionedRecord,
    GraphBatchOpOutcome as EngineGraphBatchOpOutcome,
    GraphBatchOperation as EngineGraphBatchOperation, GraphBatchWrite as EngineGraphBatchWrite,
    GraphBatchWriteOutcome as EngineGraphBatchWriteOutcome, GraphBinding as EngineGraphBinding,
    GraphBindingPage as EngineGraphBindingPage,
    GraphBindingPrimitive as EngineGraphBindingPrimitive,
    GraphBindingTarget as EngineGraphBindingTarget, GraphDeleteOutcome as EngineGraphDeleteOutcome,
    GraphDirection as EngineGraphDirection, GraphEdge as EngineGraphEdge,
    GraphEdgeData as EngineGraphEdgeData, GraphEdgeType as EngineGraphEdgeType,
    GraphEdgeWriteOutcome as EngineGraphEdgeWriteOutcome,
    GraphEntityBinding as EngineGraphEntityBinding, GraphInfo as EngineGraphInfo,
    GraphName as EngineGraphName, GraphNamePage as EngineGraphNamePage,
    GraphNeighbor as EngineGraphNeighbor, GraphNeighborPage as EngineGraphNeighborPage,
    GraphNode as EngineGraphNode, GraphNodeData as EngineGraphNodeData,
    GraphNodeId as EngineGraphNodeId, GraphNodePage as EngineGraphNodePage,
    GraphProperties as EngineGraphProperties, GraphService,
    GraphWriteOutcome as EngineGraphWriteOutcome, JsonDocumentId, JsonGetEntry, JsonHistory,
    JsonHistoryRow, JsonIndexDefinition as EngineJsonIndexDefinition, JsonIndexName,
    JsonIndexType as EngineJsonIndexType, JsonListPage, JsonPath, JsonSample as EngineJsonSample,
    JsonSampleRow, JsonService, JsonSetEntry, JsonValue as EngineJsonValue,
    JsonVersionedValue as EngineJsonVersionedValue, KvHistory, KvHistoryRow, KvKey, KvSample,
    KvScanRow, KvValue, KvVersionedValue, ProductSpace,
    SpaceCreateOutcome as EngineSpaceCreateOutcome, SpaceDeleteOutcome as EngineSpaceDeleteOutcome,
    VectorBulkDeleteOutcome as EngineVectorBulkDeleteOutcome,
    VectorCollectionInfo as EngineVectorCollectionInfo,
    VectorCollectionName as EngineVectorCollectionName, VectorConfig as EngineVectorConfig,
    VectorDistanceMetric as EngineVectorDistanceMetric, VectorEmbedding as EngineVectorEmbedding,
    VectorEntry as EngineVectorEntry, VectorFilter as EngineVectorFilter,
    VectorFilterCondition as EngineVectorFilterCondition, VectorFilterOp as EngineVectorFilterOp,
    VectorHistory as EngineVectorHistory, VectorHistoryRow as EngineVectorHistoryRow,
    VectorIndexDiagnostics as EngineVectorIndexDiagnostics, VectorKey as EngineVectorKey,
    VectorKeyPage as EngineVectorKeyPage, VectorMetadata as EngineVectorMetadata,
    VectorMetadataPatch as EngineVectorMetadataPatch, VectorScalar as EngineVectorScalar,
    VectorSearchMatch as EngineVectorSearchMatch, VectorService,
    VectorUpsertEntry as EngineVectorUpsertEntry,
    VectorVersionedEntry as EngineVectorVersionedEntry,
};

use crate::command::Command;
use crate::error::{ExecutorError, ExecutorErrorClass, ExecutorResult};
use crate::output::Output;
use crate::types::{
    AdminCapabilities as OutputAdminCapabilities, AdminConfig as OutputAdminConfig,
    AdminControlStatus as OutputAdminControlStatus, AdminDatabaseInfo as OutputAdminDatabaseInfo,
    AdminDescribe as OutputAdminDescribe, AdminGraph as OutputAdminGraph,
    AdminHealth as OutputAdminHealth, AdminHealthStatus as OutputAdminHealthStatus,
    AdminMetrics as OutputAdminMetrics, AdminOpenTarget as OutputAdminOpenTarget,
    AdminPrimitives as OutputAdminPrimitives, AdminVectorCollection as OutputAdminVectorCollection,
    ArrowExportPrimitive, ArrowFileFormat, ArrowImportTarget, BatchEventEntry, BatchGetItemResult,
    BatchItemResult, BatchJsonDeleteEntry, BatchJsonEntry, BatchJsonGetEntry, BatchKvEntry,
    BatchVectorEntry, BranchCleanupItem, BranchItem, BranchParentItem, BranchStatus, Bytes,
    CommitReceipt, EventBatchAppendItemResult,
    EventChainVerification as OutputEventChainVerification, EventData, EventRangeDirection,
    EventVersionedData, GraphBatchItemResult, GraphBatchOperation, GraphBindingHit,
    GraphBindingPrimitive, GraphBindingTarget, GraphDirection, GraphEdgeData, GraphEdgeDataOutput,
    GraphEntityBinding, GraphInfoData, GraphNeighborHit, GraphNodeData, GraphNodeDataOutput,
    HistoryItem, JsonBatchGetItemResult, JsonBatchItemResult, JsonHistoryItem, JsonIndexDefinition,
    JsonIndexType, JsonSampleItem, JsonVersionedValue as OutputJsonVersionedValue, MaybeJsonValue,
    MaybeJsonVersionedValue, MutationEffect, SampleItem, ScanItem, VectorBatchGetItemResult,
    VectorBatchItemResult, VectorCollectionInfo as OutputVectorCollectionInfo, VectorData,
    VectorDistanceMetric, VectorFilterOp, VectorHistoryItem,
    VectorIndexArtifactSource as OutputVectorIndexArtifactSource,
    VectorIndexDiagnostics as OutputVectorIndexDiagnostics, VectorIndexQueryResult, VectorMatch,
    VectorMetadataFilter, VectorScalar, VectorVersionedData, VersionedValue, DEFAULT_BRANCH,
    DEFAULT_SPACE,
};

const DEFAULT_JSON_LIST_LIMIT: usize = 100;
const DEFAULT_VECTOR_LIST_LIMIT: usize = 100;
const DEFAULT_GRAPH_LIST_LIMIT: usize = 100;

/// Serialized command executor backed by an engine database handle.
pub struct Executor {
    database: Database,
    default_branch: String,
    #[cfg(feature = "inference")]
    inference: strata_inference_next::InferenceRuntime,
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
            #[cfg(feature = "inference")]
            inference: strata_inference_next::InferenceRuntime::default(),
        }
    }

    /// Replaces the inference runtime handle used by inference commands.
    #[cfg(feature = "inference")]
    #[must_use]
    pub fn with_inference_runtime(
        mut self,
        inference: strata_inference_next::InferenceRuntime,
    ) -> Self {
        self.inference = inference;
        self
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
            Command::Ping => self.execute_ping(),
            Command::Info { branch } => self.execute_info(branch.as_deref()),
            Command::Health { branch } => self.execute_health(branch.as_deref()),
            Command::Metrics { branch } => self.execute_metrics(branch.as_deref()),
            Command::Describe { branch } => self.execute_describe(branch.as_deref()),
            Command::ConfigGet => self.execute_config_get(),
            Command::ConfigureGetKey { key } => self.execute_configure_get_key(&key),
            Command::SpaceList { branch } => self.execute_space_list(branch.as_deref()),
            Command::SpaceCreate { branch, space } => {
                self.execute_space_create(branch.as_deref(), &space)
            }
            Command::SpaceExists { branch, space } => {
                self.execute_space_exists(branch.as_deref(), &space)
            }
            Command::SpaceDelete {
                branch,
                space,
                force,
            } => self.execute_space_delete(branch.as_deref(), &space, force),
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
            Command::VectorIndexQuery {
                branch,
                space,
                collection,
                query,
                k,
                filter,
                as_of,
            } => self.execute_vector_index_query(
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
            Command::GraphCreate {
                branch,
                space,
                graph,
            } => self.execute_graph_create(branch.as_deref(), space.as_deref(), graph),
            Command::GraphDelete {
                branch,
                space,
                graph,
            } => self.execute_graph_delete(branch.as_deref(), space.as_deref(), graph),
            Command::GraphList {
                branch,
                space,
                cursor,
                limit,
            } => self.execute_graph_list(branch.as_deref(), space.as_deref(), cursor, limit),
            Command::GraphGetMeta {
                branch,
                space,
                graph,
            } => self.execute_graph_get_meta(branch.as_deref(), space.as_deref(), graph),
            Command::GraphAddNode {
                branch,
                space,
                graph,
                node_id,
                properties,
                binding,
            } => self.execute_graph_add_node(
                branch.as_deref(),
                space.as_deref(),
                graph,
                node_id,
                properties,
                binding,
            ),
            Command::GraphGetNode {
                branch,
                space,
                graph,
                node_id,
            } => self.execute_graph_get_node(branch.as_deref(), space.as_deref(), graph, node_id),
            Command::GraphRemoveNode {
                branch,
                space,
                graph,
                node_id,
            } => {
                self.execute_graph_remove_node(branch.as_deref(), space.as_deref(), graph, node_id)
            }
            Command::GraphListNodes {
                branch,
                space,
                graph,
                prefix,
                cursor,
                limit,
            } => self.execute_graph_list_nodes(
                branch.as_deref(),
                space.as_deref(),
                graph,
                prefix,
                cursor,
                limit,
            ),
            Command::GraphAddEdge {
                branch,
                space,
                graph,
                src,
                edge_type,
                dst,
                weight,
                properties,
            } => self.execute_graph_add_edge(
                branch.as_deref(),
                space.as_deref(),
                graph,
                src,
                edge_type,
                dst,
                weight,
                properties,
            ),
            Command::GraphGetEdge {
                branch,
                space,
                graph,
                src,
                edge_type,
                dst,
            } => self.execute_graph_get_edge(
                branch.as_deref(),
                space.as_deref(),
                graph,
                src,
                edge_type,
                dst,
            ),
            Command::GraphRemoveEdge {
                branch,
                space,
                graph,
                src,
                edge_type,
                dst,
            } => self.execute_graph_remove_edge(
                branch.as_deref(),
                space.as_deref(),
                graph,
                src,
                edge_type,
                dst,
            ),
            Command::GraphNeighbors {
                branch,
                space,
                graph,
                node_id,
                direction,
                edge_type,
                cursor,
                limit,
            } => self.execute_graph_neighbors(
                branch.as_deref(),
                space.as_deref(),
                graph,
                node_id,
                direction,
                edge_type,
                cursor.as_deref(),
                limit,
            ),
            Command::GraphBindingsForEntity {
                branch,
                space,
                target,
                cursor,
                limit,
            } => self.execute_graph_bindings_for_entity(
                branch.as_deref(),
                space.as_deref(),
                target,
                cursor.as_deref(),
                limit,
            ),
            Command::GraphBatchWrite {
                branch,
                space,
                graph,
                operations,
            } => self.execute_graph_batch_write(
                branch.as_deref(),
                space.as_deref(),
                graph,
                operations,
            ),
            Command::ArrowImport {
                branch,
                space,
                file_path,
                format,
                target,
                key_column,
                value_column,
                collection,
            } => self.execute_arrow_import(
                branch.as_deref(),
                space.as_deref(),
                file_path,
                format,
                target,
                key_column.as_deref(),
                value_column.as_deref(),
                collection.as_deref(),
            ),
            Command::ArrowExport {
                branch,
                space,
                primitive,
                format,
                path,
                prefix,
                limit,
                collection,
                graph,
                event_type,
            } => self.execute_arrow_export(
                branch.as_deref(),
                space.as_deref(),
                primitive,
                format,
                path,
                prefix.as_deref(),
                limit,
                collection,
                graph,
                event_type,
            ),
            #[cfg(feature = "inference")]
            Command::InferenceModelsList => {
                Ok(Output::InferenceModels(self.inference.list_models()))
            }
            #[cfg(feature = "inference")]
            Command::InferenceModelsLocal => {
                Ok(Output::InferenceModels(self.inference.list_local_models()))
            }
            #[cfg(feature = "inference")]
            Command::InferenceModelsPull { model } => self
                .inference
                .pull_model(&model)
                .map(Output::InferenceModelPulled)
                .map_err(ExecutorError::from),
            #[cfg(feature = "inference")]
            Command::InferenceModelCapability { model } => self
                .inference
                .capability(&model)
                .map(Output::InferenceCapability)
                .map_err(ExecutorError::from),
            #[cfg(feature = "inference")]
            Command::InferenceGenerate { model, request } => self
                .inference
                .generate(&model, &request)
                .map(Output::InferenceGeneration)
                .map_err(ExecutorError::from),
            #[cfg(feature = "inference")]
            Command::InferenceTokenize {
                model,
                text,
                add_special,
            } => self
                .inference
                .tokenize(&model, &text, add_special)
                .map(Output::InferenceTokenIds)
                .map_err(ExecutorError::from),
            #[cfg(feature = "inference")]
            Command::InferenceDetokenize { model, ids } => self
                .inference
                .detokenize(&model, &ids)
                .map(Output::InferenceText)
                .map_err(ExecutorError::from),
            #[cfg(feature = "inference")]
            Command::InferenceEmbed { model, request } => self
                .inference
                .embed(&model, &request)
                .map(Output::InferenceEmbedding)
                .map_err(ExecutorError::from),
            #[cfg(feature = "inference")]
            Command::InferenceEmbedBatch { model, texts } => self
                .inference
                .embed_batch(&model, &texts)
                .map(Output::InferenceEmbeddings)
                .map_err(ExecutorError::from),
            #[cfg(feature = "inference")]
            Command::InferenceRank { model, request } => self
                .inference
                .rank(&model, &request)
                .map(Output::InferenceRanking)
                .map_err(ExecutorError::from),
            #[cfg(feature = "inference")]
            Command::InferenceUnload { model } => self
                .inference
                .unload(model.as_deref())
                .map(|unloaded| Output::InferenceUnloadResult { unloaded })
                .map_err(ExecutorError::from),
            #[cfg(feature = "inference")]
            Command::InferenceCacheStatus => self
                .inference
                .cache_status()
                .map(Output::InferenceCacheStatus)
                .map_err(ExecutorError::from),
        }
    }

    fn execute_ping(&mut self) -> ExecutorResult<Output> {
        let summary = self.database.admin()?.ping();
        Ok(Output::Pong {
            version: summary.version,
        })
    }

    fn execute_info(&mut self, branch: Option<&str>) -> ExecutorResult<Output> {
        let branch = branch_name(branch, &self.default_branch)?;
        let mut admin = self.database.admin()?;
        let summary = admin.info(Some(&branch))?;
        Ok(Output::DatabaseInfo(output_admin_info(&summary)))
    }

    fn execute_health(&mut self, branch: Option<&str>) -> ExecutorResult<Output> {
        let branch = branch_name(branch, &self.default_branch)?;
        let mut admin = self.database.admin()?;
        let summary = admin.health(Some(&branch));
        Ok(Output::Health(output_admin_health(&summary)))
    }

    fn execute_metrics(&mut self, branch: Option<&str>) -> ExecutorResult<Output> {
        let branch = branch_name(branch, &self.default_branch)?;
        let mut admin = self.database.admin()?;
        let summary = admin.metrics(Some(&branch))?;
        Ok(Output::Metrics(output_admin_metrics(&summary)))
    }

    fn execute_describe(&mut self, branch: Option<&str>) -> ExecutorResult<Output> {
        let branch = branch_name(branch, &self.default_branch)?;
        let mut admin = self.database.admin()?;
        let summary = admin.describe(Some(&branch))?;
        Ok(Output::Described(output_admin_describe(&summary)))
    }

    fn execute_config_get(&mut self) -> ExecutorResult<Output> {
        let admin = self.database.admin()?;
        Ok(Output::Config(output_admin_config(&admin.config())))
    }

    fn execute_configure_get_key(&mut self, key: &str) -> ExecutorResult<Output> {
        let admin = self.database.admin()?;
        Ok(Output::ConfigValue(admin.config_value(key)?))
    }

    fn execute_space_list(&mut self, branch: Option<&str>) -> ExecutorResult<Output> {
        let branch = branch_name(branch, &self.default_branch)?;
        let mut spaces = self.database.spaces(branch)?;
        Ok(Output::SpaceList(
            spaces
                .list()?
                .iter()
                .map(|space| space.as_str().to_owned())
                .collect(),
        ))
    }

    fn execute_space_create(
        &mut self,
        branch: Option<&str>,
        space: &str,
    ) -> ExecutorResult<Output> {
        let branch = branch_name(branch, &self.default_branch)?;
        let space = product_space(Some(space))?;
        let mut spaces = self.database.spaces(branch)?;
        let outcome = spaces.create(space)?;
        Ok(output_space_create(&outcome))
    }

    fn execute_space_exists(
        &mut self,
        branch: Option<&str>,
        space: &str,
    ) -> ExecutorResult<Output> {
        let branch = branch_name(branch, &self.default_branch)?;
        let space = product_space(Some(space))?;
        let mut spaces = self.database.spaces(branch)?;
        Ok(Output::Bool(spaces.exists(&space)?))
    }

    fn execute_space_delete(
        &mut self,
        branch: Option<&str>,
        space: &str,
        force: bool,
    ) -> ExecutorResult<Output> {
        let branch = branch_name(branch, &self.default_branch)?;
        let space = product_space(Some(space))?;
        let mut spaces = self.database.spaces(branch)?;
        let outcome = spaces.delete(&space, force)?;
        Ok(output_space_delete(&outcome))
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
        let key = kv_key(key)?;
        let mut service = self.kv_service(branch, space)?;
        let effect = upsert_effect(service.exists(&key)?);
        let outcome = service.put(key, kv_value(value))?;
        Ok(write_output(output_key, effect, outcome))
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
        let effect = upsert_effect(service.exists(&id)?);
        let outcome = service.set_or_create(id, &path, value)?;
        Ok(json_write_output(key, effect, outcome.commit()))
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
        let mut written_docs = BTreeSet::new();
        for (index, entry) in entries.into_iter().enumerate() {
            let (key, path, value) = entry.into_parts();
            let validation: ExecutorResult<(String, JsonSetEntry)> = (|| {
                let id = json_document_id(key.clone())?;
                let path = json_path(&path)?;
                let value = json_value(value)?;
                Ok((key, JsonSetEntry::new(id, path, value)))
            })();
            match validation {
                Ok((key, entry)) => {
                    let existed = service.exists(&json_document_id(key.clone())?)?;
                    let already_written = !written_docs.insert(key);
                    let effect = upsert_effect(existed || already_written);
                    valid_entries.push((index, effect, entry));
                }
                Err(error) => results[index] = Some(JsonBatchItemResult::failed_error(error)),
            }
        }
        if valid_entries.is_empty() {
            return Ok(Output::JsonBatchResults(finish_json_batch_results(results)));
        }
        let engine_entries = valid_entries
            .iter()
            .map(|(_, _, entry)| entry.clone())
            .collect::<Vec<_>>();
        let outcome = service.batch_set_or_create(engine_entries)?;
        for ((index, effect, _), item) in valid_entries.into_iter().zip(outcome.results()) {
            results[index] = Some(json_batch_item_result(
                effect,
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
                Err(error) => results[index] = Some(JsonBatchGetItemResult::failed_error(error)),
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
                Err(error) => results[index] = Some(JsonBatchItemResult::failed_error(error)),
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
                delete_effect(deleted),
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

    #[allow(clippy::too_many_arguments)]
    fn execute_vector_index_query(
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

    fn execute_graph_create(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        graph: String,
    ) -> ExecutorResult<Output> {
        let graph = graph_name(graph)?;
        let mut service = self.graph_service(branch, space)?;
        Ok(Output::GraphInfo(graph_info_data(
            &service.create_graph(graph)?,
        )))
    }

    fn execute_graph_delete(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        graph: String,
    ) -> ExecutorResult<Output> {
        let graph = graph_name(graph)?;
        let mut service = self.graph_service(branch, space)?;
        Ok(graph_delete_output(
            &service.delete_graph(&graph)?,
            None,
            None,
            None,
            None,
        ))
    }

    fn execute_graph_list(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        cursor: Option<String>,
        limit: Option<u64>,
    ) -> ExecutorResult<Output> {
        let cursor = optional_graph_name(cursor)?;
        let limit = optional_limit(limit)?.unwrap_or(DEFAULT_GRAPH_LIST_LIMIT);
        let mut service = self.graph_service(branch, space)?;
        Ok(graph_name_page_output(
            &service.list_graphs(cursor.as_ref(), limit)?,
        ))
    }

    fn execute_graph_get_meta(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        graph: String,
    ) -> ExecutorResult<Output> {
        let graph = graph_name(graph)?;
        let mut service = self.graph_service(branch, space)?;
        Ok(Output::GraphInfoResult(
            service.graph_info(&graph)?.as_ref().map(graph_info_data),
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_graph_add_node(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        graph: String,
        node_id: String,
        properties: Option<serde_json::Value>,
        binding: Option<GraphEntityBinding>,
    ) -> ExecutorResult<Output> {
        let graph = graph_name(graph)?;
        let node_id = graph_node_id(node_id)?;
        let data = engine_graph_node_data(GraphNodeData::new(properties, binding))?;
        let mut service = self.graph_service(branch, space)?;
        Ok(graph_node_write_output(
            &service.upsert_node(&graph, node_id, data)?,
        ))
    }

    fn execute_graph_get_node(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        graph: String,
        node_id: String,
    ) -> ExecutorResult<Output> {
        let graph = graph_name(graph)?;
        let node_id = graph_node_id(node_id)?;
        let mut service = self.graph_service(branch, space)?;
        Ok(Output::GraphNodeResult(
            service
                .get_node(&graph, &node_id)?
                .as_ref()
                .map(graph_node_data_output),
        ))
    }

    fn execute_graph_remove_node(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        graph: String,
        node_id: String,
    ) -> ExecutorResult<Output> {
        let graph = graph_name(graph)?;
        let node_id = graph_node_id(node_id)?;
        let mut service = self.graph_service(branch, space)?;
        Ok(graph_delete_output(
            &service.delete_node(&graph, &node_id)?,
            Some(node_id.as_str().to_owned()),
            None,
            None,
            None,
        ))
    }

    fn execute_graph_list_nodes(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        graph: String,
        prefix: Option<String>,
        cursor: Option<String>,
        limit: Option<u64>,
    ) -> ExecutorResult<Output> {
        let graph = graph_name(graph)?;
        let prefix = optional_graph_node_id(prefix)?;
        let cursor = optional_graph_node_id(cursor)?;
        let limit = optional_limit(limit)?.unwrap_or(DEFAULT_GRAPH_LIST_LIMIT);
        let mut service = self.graph_service(branch, space)?;
        Ok(graph_node_page_output(&service.list_nodes(
            &graph,
            prefix.as_ref(),
            cursor.as_ref(),
            limit,
        )?))
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_graph_add_edge(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        graph: String,
        src: String,
        edge_type: String,
        dst: String,
        weight: Option<f64>,
        properties: Option<serde_json::Value>,
    ) -> ExecutorResult<Output> {
        let graph = graph_name(graph)?;
        let src = graph_node_id(src)?;
        let edge_type = graph_edge_type(edge_type)?;
        let dst = graph_node_id(dst)?;
        let data = engine_graph_edge_data(GraphEdgeData::new(weight, properties))?;
        let mut service = self.graph_service(branch, space)?;
        Ok(graph_edge_write_output(
            &service.upsert_edge(&graph, src, edge_type, dst, data)?,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_graph_get_edge(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        graph: String,
        src: String,
        edge_type: String,
        dst: String,
    ) -> ExecutorResult<Output> {
        let graph = graph_name(graph)?;
        let src = graph_node_id(src)?;
        let edge_type = graph_edge_type(edge_type)?;
        let dst = graph_node_id(dst)?;
        let mut service = self.graph_service(branch, space)?;
        Ok(Output::GraphEdgeResult(
            service
                .get_edge(&graph, &src, &edge_type, &dst)?
                .as_ref()
                .map(graph_edge_data_output),
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_graph_remove_edge(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        graph: String,
        src: String,
        edge_type: String,
        dst: String,
    ) -> ExecutorResult<Output> {
        let graph = graph_name(graph)?;
        let src = graph_node_id(src)?;
        let edge_type = graph_edge_type(edge_type)?;
        let dst = graph_node_id(dst)?;
        let mut service = self.graph_service(branch, space)?;
        Ok(graph_delete_output(
            &service.delete_edge(&graph, &src, &edge_type, &dst)?,
            None,
            Some(src.as_str().to_owned()),
            Some(edge_type.as_str().to_owned()),
            Some(dst.as_str().to_owned()),
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn execute_graph_neighbors(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        graph: String,
        node_id: String,
        direction: GraphDirection,
        edge_type: Option<String>,
        cursor: Option<&str>,
        limit: Option<u64>,
    ) -> ExecutorResult<Output> {
        let graph = graph_name(graph)?;
        let node_id = graph_node_id(node_id)?;
        let direction = engine_graph_direction(direction);
        let edge_type = optional_graph_edge_type(edge_type)?;
        let limit = optional_limit(limit)?.unwrap_or(DEFAULT_GRAPH_LIST_LIMIT);
        let mut service = self.graph_service(branch, space)?;
        Ok(graph_neighbor_page_output(&service.neighbors(
            &graph,
            &node_id,
            direction,
            edge_type.as_ref(),
            cursor,
            limit,
        )?))
    }

    fn execute_graph_bindings_for_entity(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        target: GraphBindingTarget,
        cursor: Option<&str>,
        limit: Option<u64>,
    ) -> ExecutorResult<Output> {
        let target = engine_graph_binding_target(target)?;
        let limit = optional_limit(limit)?.unwrap_or(DEFAULT_GRAPH_LIST_LIMIT);
        let mut service = self.graph_service(branch, space)?;
        Ok(graph_binding_page_output(
            &service.bindings_for_entity(&target, cursor, limit)?,
        ))
    }

    fn execute_graph_batch_write(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        graph: String,
        operations: Vec<GraphBatchOperation>,
    ) -> ExecutorResult<Output> {
        let graph = graph_name(graph)?;
        let operation_names = operations
            .iter()
            .map(graph_batch_operation_name)
            .collect::<Vec<_>>();
        let operations = operations
            .into_iter()
            .map(engine_graph_batch_operation)
            .collect::<ExecutorResult<Vec<_>>>()?;
        let batch = EngineGraphBatchWrite::new(operations);
        let mut service = self.graph_service(branch, space)?;
        Ok(graph_batch_write_output(
            &service.batch_write(&graph, &batch)?,
            &operation_names,
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

    fn graph_service(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
    ) -> ExecutorResult<GraphService<'_>> {
        let branch = branch_name(branch, &self.default_branch)?;
        let space = product_space(space)?;
        {
            let branches = self.database.branches()?;
            branches.get(&branch)?;
        }
        Ok(self.database.graph(branch, space)?)
    }

    #[allow(clippy::too_many_arguments)]
    #[cfg(feature = "arrow")]
    fn execute_arrow_import(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        file_path: String,
        format: Option<ArrowFileFormat>,
        target: ArrowImportTarget,
        key_column: Option<&str>,
        value_column: Option<&str>,
        collection: Option<&str>,
    ) -> ExecutorResult<Output> {
        crate::arrow::import::import_file(
            self,
            branch,
            space,
            file_path,
            format,
            target,
            key_column,
            value_column,
            collection,
        )
    }

    #[allow(clippy::too_many_arguments)]
    #[cfg(not(feature = "arrow"))]
    fn execute_arrow_import(
        &mut self,
        _branch: Option<&str>,
        _space: Option<&str>,
        file_path: String,
        _format: Option<ArrowFileFormat>,
        _target: ArrowImportTarget,
        _key_column: Option<&str>,
        _value_column: Option<&str>,
        _collection: Option<&str>,
    ) -> ExecutorResult<Output> {
        if !std::path::Path::new(&file_path).exists() {
            return Err(ExecutorError::invalid_input(
                "invalid_argument.executor.arrow_input_missing",
                format!("file not found: '{file_path}'"),
            ));
        }
        Err(arrow_feature_disabled())
    }

    #[allow(clippy::too_many_arguments)]
    #[cfg(feature = "arrow")]
    fn execute_arrow_export(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        primitive: ArrowExportPrimitive,
        format: ArrowFileFormat,
        path: String,
        prefix: Option<&str>,
        limit: Option<u64>,
        collection: Option<String>,
        graph: Option<String>,
        event_type: Option<String>,
    ) -> ExecutorResult<Output> {
        crate::arrow::export::export_file(
            self, branch, space, primitive, format, path, prefix, limit, collection, graph,
            event_type,
        )
    }

    #[allow(clippy::too_many_arguments)]
    #[cfg(not(feature = "arrow"))]
    fn execute_arrow_export(
        &mut self,
        _branch: Option<&str>,
        _space: Option<&str>,
        _primitive: ArrowExportPrimitive,
        _format: ArrowFileFormat,
        _path: String,
        _prefix: Option<&str>,
        _limit: Option<u64>,
        _collection: Option<String>,
        _graph: Option<String>,
        _event_type: Option<String>,
    ) -> ExecutorResult<Output> {
        Err(arrow_feature_disabled())
    }
}

#[cfg(not(feature = "arrow"))]
fn arrow_feature_disabled() -> ExecutorError {
    ExecutorError::invalid_input(
        "invalid_argument.executor.arrow_feature_disabled",
        "Arrow import/export requires the executor arrow feature",
    )
}

const fn output_admin_open_target(target: EngineDatabaseOpenTarget) -> OutputAdminOpenTarget {
    match target {
        EngineDatabaseOpenTarget::Cache => OutputAdminOpenTarget::Cache,
        EngineDatabaseOpenTarget::DurableLocal => OutputAdminOpenTarget::DurableLocal,
    }
}

const fn output_admin_health_status(status: EngineAdminHealthStatus) -> OutputAdminHealthStatus {
    match status {
        EngineAdminHealthStatus::Healthy => OutputAdminHealthStatus::Healthy,
        EngineAdminHealthStatus::Degraded => OutputAdminHealthStatus::Degraded,
        EngineAdminHealthStatus::Unhealthy => OutputAdminHealthStatus::Unhealthy,
    }
}

const fn output_admin_control_status(
    status: EngineControlHealthStatus,
) -> OutputAdminControlStatus {
    match status {
        EngineControlHealthStatus::Healthy => OutputAdminControlStatus::Healthy,
        EngineControlHealthStatus::Missing => OutputAdminControlStatus::Missing,
        EngineControlHealthStatus::Corrupt => OutputAdminControlStatus::Corrupt,
        EngineControlHealthStatus::Unavailable => OutputAdminControlStatus::Unavailable,
    }
}

fn output_admin_info(info: &EngineAdminDatabaseInfo) -> OutputAdminDatabaseInfo {
    OutputAdminDatabaseInfo {
        version: info.version.clone(),
        target: output_admin_open_target(info.target),
        created: info.created,
        durable: info.durable,
        default_branch: info.default_branch.as_str().to_owned(),
        branch_count: info.branch_count,
        space_count: info.space_count,
        open: info.open,
    }
}

fn output_admin_health(health: &EngineAdminHealthSummary) -> OutputAdminHealth {
    OutputAdminHealth {
        status: output_admin_health_status(health.status),
        identity: output_admin_control_status(health.identity),
        registry: output_admin_control_status(health.registry),
        branch_catalog: output_admin_control_status(health.branch_catalog),
        space_catalog: health.space_catalog.map(output_admin_control_status),
        default_branch: health.default_branch.as_str().to_owned(),
        branch_count: health.branch_count,
    }
}

fn output_admin_metrics(metrics: &EngineAdminMetricsSummary) -> OutputAdminMetrics {
    OutputAdminMetrics {
        target: output_admin_open_target(metrics.target),
        durable: metrics.durable,
        open: metrics.open,
        branch_count: metrics.branch_count,
        space_count: metrics.space_count,
        control_status: output_admin_health_status(metrics.control_status),
    }
}

fn output_admin_config(config: &EngineAdminConfigSummary) -> OutputAdminConfig {
    OutputAdminConfig {
        target: output_admin_open_target(config.target),
        created: config.created,
        durable: config.durable,
        default_branch: config.default_branch.as_str().to_owned(),
    }
}

fn output_admin_capabilities(
    capabilities: &EngineAdminCapabilitySummary,
) -> OutputAdminCapabilities {
    OutputAdminCapabilities {
        kv: capabilities.kv,
        json: capabilities.json,
        event: capabilities.event,
        vector: capabilities.vector,
        vector_index: capabilities.vector_index,
        graph_core: capabilities.graph_core,
        arrow: cfg!(feature = "arrow"),
        inference: cfg!(feature = "inference"),
    }
}

fn output_admin_vector_collection(
    collection: &EngineAdminVectorCollectionSummary,
) -> OutputAdminVectorCollection {
    OutputAdminVectorCollection {
        name: collection.name.clone(),
        dimension: collection.dimension,
        metric: output_vector_metric(collection.metric),
        count: collection.count,
    }
}

fn output_admin_graph(graph: &EngineAdminGraphSummary) -> OutputAdminGraph {
    OutputAdminGraph {
        name: graph.name.clone(),
        node_count: graph.node_count,
        edge_count: graph.edge_count,
    }
}

fn output_admin_primitives(primitives: &EngineAdminPrimitiveSummary) -> OutputAdminPrimitives {
    OutputAdminPrimitives {
        kv_count: primitives.kv_count,
        json_count: primitives.json_count,
        event_count: primitives.event_count,
        vector_collections: primitives
            .vector_collections
            .iter()
            .map(output_admin_vector_collection)
            .collect(),
        graphs: primitives.graphs.iter().map(output_admin_graph).collect(),
    }
}

fn output_admin_describe(describe: &EngineAdminDescribeSummary) -> OutputAdminDescribe {
    OutputAdminDescribe {
        version: describe.version.clone(),
        target: output_admin_open_target(describe.target),
        default_branch: describe.default_branch.as_str().to_owned(),
        branch: describe.branch.as_str().to_owned(),
        branches: describe
            .branches
            .iter()
            .map(|branch| branch.as_str().to_owned())
            .collect(),
        spaces: describe
            .spaces
            .iter()
            .map(|space| space.as_str().to_owned())
            .collect(),
        primitives: output_admin_primitives(&describe.primitives),
        config: output_admin_config(&describe.config),
        capabilities: output_admin_capabilities(&describe.capabilities),
    }
}

fn output_space_create(outcome: &EngineSpaceCreateOutcome) -> Output {
    Output::SpaceCreateResult {
        space: outcome.space().as_str().to_owned(),
        created: outcome.created(),
        version: outcome.version().map(CommitVersion::as_u64),
        timestamp: outcome.timestamp().map(Timestamp::as_micros),
    }
}

fn output_space_delete(outcome: &EngineSpaceDeleteOutcome) -> Output {
    Output::SpaceDeleteResult {
        space: outcome.space().as_str().to_owned(),
        deleted: outcome.deleted(),
        force: outcome.force(),
        deleted_rows: outcome.deleted_rows(),
        version: outcome.version().map(CommitVersion::as_u64),
        timestamp: outcome.timestamp().map(Timestamp::as_micros),
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

    /// Executes a default-branch vector index-query command.
    pub fn vector_index_query(
        &mut self,
        collection: impl Into<String>,
        query: Vec<f32>,
        k: u64,
        filter: Option<VectorMetadataFilter>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::VectorIndexQuery {
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

    /// Executes a default-branch graph-create command.
    pub fn graph_create(&mut self, graph: impl Into<String>) -> ExecutorResult<Output> {
        self.execute(Command::GraphCreate {
            branch: None,
            space: None,
            graph: graph.into(),
        })
    }

    /// Executes a default-branch graph-delete command.
    pub fn graph_delete(&mut self, graph: impl Into<String>) -> ExecutorResult<Output> {
        self.execute(Command::GraphDelete {
            branch: None,
            space: None,
            graph: graph.into(),
        })
    }

    /// Executes a default-branch graph-list command.
    pub fn graph_list(
        &mut self,
        cursor: Option<String>,
        limit: Option<u64>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::GraphList {
            branch: None,
            space: None,
            cursor,
            limit,
        })
    }

    /// Executes a default-branch graph-metadata command.
    pub fn graph_get_meta(&mut self, graph: impl Into<String>) -> ExecutorResult<Output> {
        self.execute(Command::GraphGetMeta {
            branch: None,
            space: None,
            graph: graph.into(),
        })
    }

    /// Executes a default-branch graph node upsert command.
    pub fn graph_add_node(
        &mut self,
        graph: impl Into<String>,
        node_id: impl Into<String>,
        properties: Option<serde_json::Value>,
        binding: Option<GraphEntityBinding>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::GraphAddNode {
            branch: None,
            space: None,
            graph: graph.into(),
            node_id: node_id.into(),
            properties,
            binding,
        })
    }

    /// Executes a default-branch graph node get command.
    pub fn graph_get_node(
        &mut self,
        graph: impl Into<String>,
        node_id: impl Into<String>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::GraphGetNode {
            branch: None,
            space: None,
            graph: graph.into(),
            node_id: node_id.into(),
        })
    }

    /// Executes a default-branch graph node delete command.
    pub fn graph_remove_node(
        &mut self,
        graph: impl Into<String>,
        node_id: impl Into<String>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::GraphRemoveNode {
            branch: None,
            space: None,
            graph: graph.into(),
            node_id: node_id.into(),
        })
    }

    /// Executes a default-branch graph node-list command.
    pub fn graph_list_nodes(
        &mut self,
        graph: impl Into<String>,
        prefix: Option<String>,
        cursor: Option<String>,
        limit: Option<u64>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::GraphListNodes {
            branch: None,
            space: None,
            graph: graph.into(),
            prefix,
            cursor,
            limit,
        })
    }

    /// Executes a default-branch graph edge upsert command.
    #[allow(clippy::too_many_arguments)]
    pub fn graph_add_edge(
        &mut self,
        graph: impl Into<String>,
        src: impl Into<String>,
        edge_type: impl Into<String>,
        dst: impl Into<String>,
        weight: Option<f64>,
        properties: Option<serde_json::Value>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::GraphAddEdge {
            branch: None,
            space: None,
            graph: graph.into(),
            src: src.into(),
            edge_type: edge_type.into(),
            dst: dst.into(),
            weight,
            properties,
        })
    }

    /// Executes a default-branch graph edge get command.
    pub fn graph_get_edge(
        &mut self,
        graph: impl Into<String>,
        src: impl Into<String>,
        edge_type: impl Into<String>,
        dst: impl Into<String>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::GraphGetEdge {
            branch: None,
            space: None,
            graph: graph.into(),
            src: src.into(),
            edge_type: edge_type.into(),
            dst: dst.into(),
        })
    }

    /// Executes a default-branch graph edge delete command.
    pub fn graph_remove_edge(
        &mut self,
        graph: impl Into<String>,
        src: impl Into<String>,
        edge_type: impl Into<String>,
        dst: impl Into<String>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::GraphRemoveEdge {
            branch: None,
            space: None,
            graph: graph.into(),
            src: src.into(),
            edge_type: edge_type.into(),
            dst: dst.into(),
        })
    }

    /// Executes a default-branch graph neighbors command.
    pub fn graph_neighbors(
        &mut self,
        graph: impl Into<String>,
        node_id: impl Into<String>,
        direction: GraphDirection,
        edge_type: Option<String>,
        cursor: Option<String>,
        limit: Option<u64>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::GraphNeighbors {
            branch: None,
            space: None,
            graph: graph.into(),
            node_id: node_id.into(),
            direction,
            edge_type,
            cursor,
            limit,
        })
    }

    /// Executes a default-branch graph binding lookup command.
    pub fn graph_bindings_for_entity(
        &mut self,
        target: GraphBindingTarget,
        cursor: Option<String>,
        limit: Option<u64>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::GraphBindingsForEntity {
            branch: None,
            space: None,
            target,
            cursor,
            limit,
        })
    }

    /// Executes a default-branch graph batch write command.
    pub fn graph_batch_write(
        &mut self,
        graph: impl Into<String>,
        operations: Vec<GraphBatchOperation>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::GraphBatchWrite {
            branch: None,
            space: None,
            graph: graph.into(),
            operations,
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

fn commit_receipt(outcome: CommitOutcome) -> CommitReceipt {
    CommitReceipt::new(
        outcome.version().as_u64(),
        outcome.timestamp().as_micros(),
        outcome.durable(),
        usize_to_u64(outcome.put_count()),
        usize_to_u64(outcome.delete_count()),
    )
}

fn upsert_effect(existed: bool) -> MutationEffect {
    if existed {
        MutationEffect::updated()
    } else {
        MutationEffect::created()
    }
}

fn delete_effect(deleted: bool) -> MutationEffect {
    if deleted {
        MutationEffect::deleted()
    } else {
        MutationEffect::not_found()
    }
}

fn write_output(key: Bytes, effect: MutationEffect, outcome: CommitOutcome) -> Output {
    Output::WriteResult {
        key,
        effect,
        commit: commit_receipt(outcome),
    }
}

fn delete_output(key: Bytes, deleted: bool, outcome: Option<CommitOutcome>) -> Output {
    Output::DeleteResult {
        key,
        effect: delete_effect(deleted),
        commit: outcome.map(commit_receipt),
    }
}

fn batch_item_result(
    key: Bytes,
    effect: MutationEffect,
    outcome: Option<CommitOutcome>,
) -> BatchItemResult {
    BatchItemResult::new(key, effect, outcome.map(commit_receipt))
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

fn json_write_output(key: &str, effect: MutationEffect, outcome: CommitOutcome) -> Output {
    write_output(Bytes::from(key), effect, outcome)
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
    effect: MutationEffect,
    outcome: Option<CommitOutcome>,
    document_version: Option<u64>,
) -> JsonBatchItemResult {
    JsonBatchItemResult::new(effect, outcome.map(commit_receipt), document_version)
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

fn vector_index_diagnostics(value: &EngineVectorIndexDiagnostics) -> OutputVectorIndexDiagnostics {
    OutputVectorIndexDiagnostics::new(
        value.collection().to_owned(),
        value.manifest_status().to_owned(),
        value.manifest_generation(),
        usize_to_u64(value.manifest_ref_count()),
        usize_to_u64(value.manifest_inherited_ref_count()),
        usize_to_u64(value.manifest_owned_ref_count()),
        usize_to_u64(value.manifest_active_delta_count()),
        value.policy_mode().to_owned(),
        usize_to_u64(value.collection_exact_threshold()),
        usize_to_u64(value.source_flat_threshold()),
        usize_to_u64(value.source_hnsw_threshold()),
        usize_to_u64(value.overfetch_factor()),
        value.filtered_underfill_fallback(),
        usize_to_u64(value.active_delta_seal_threshold()),
        usize_to_u64(value.hnsw_memory_budget_bytes()),
        usize_to_u64(value.source_candidate_limit()),
        value.resolved_index_kind_summary().to_owned(),
        value.exact_fallback_count(),
        usize_to_u64(value.hnsw_graph_builds()),
        usize_to_u64(value.indexed_source_count()),
        usize_to_u64(value.exact_source_count()),
        usize_to_u64(value.flat_source_count()),
        usize_to_u64(value.hnsw_source_count()),
        usize_to_u64(value.active_delta_source_count()),
        usize_to_u64(value.indexed_vector_count()),
        value.derived_bytes(),
        value.last_query_used_index(),
        value
            .last_query_fallback_reason()
            .map(std::borrow::ToOwned::to_owned),
        value
            .artifact_sources()
            .iter()
            .map(|source| {
                OutputVectorIndexArtifactSource::new(
                    source.artifact_id().to_owned(),
                    source.status().to_owned(),
                    source.searched(),
                )
            })
            .collect(),
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
    if let Some(error) = item.error_status() {
        return EventBatchAppendItemResult::failed_engine_status(error);
    }
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

fn graph_name(name: String) -> ExecutorResult<EngineGraphName> {
    EngineGraphName::new(name).map_err(ExecutorError::from)
}

fn optional_graph_name(name: Option<String>) -> ExecutorResult<Option<EngineGraphName>> {
    name.map(graph_name).transpose()
}

fn graph_node_id(node_id: String) -> ExecutorResult<EngineGraphNodeId> {
    EngineGraphNodeId::new(node_id).map_err(ExecutorError::from)
}

fn optional_graph_node_id(node_id: Option<String>) -> ExecutorResult<Option<EngineGraphNodeId>> {
    node_id.map(graph_node_id).transpose()
}

fn graph_edge_type(edge_type: String) -> ExecutorResult<EngineGraphEdgeType> {
    EngineGraphEdgeType::new(edge_type).map_err(ExecutorError::from)
}

fn optional_graph_edge_type(
    edge_type: Option<String>,
) -> ExecutorResult<Option<EngineGraphEdgeType>> {
    edge_type.map(graph_edge_type).transpose()
}

fn engine_graph_properties(
    properties: Option<serde_json::Value>,
) -> ExecutorResult<Option<EngineGraphProperties>> {
    properties
        .map(EngineGraphProperties::new)
        .transpose()
        .map_err(ExecutorError::from)
}

fn engine_graph_node_data(data: GraphNodeData) -> ExecutorResult<EngineGraphNodeData> {
    let (properties, binding) = data.into_parts();
    Ok(EngineGraphNodeData::new(
        engine_graph_properties(properties)?,
        binding.map(engine_graph_entity_binding).transpose()?,
    ))
}

fn engine_graph_edge_data(data: GraphEdgeData) -> ExecutorResult<EngineGraphEdgeData> {
    let (weight, properties) = data.into_parts();
    let properties = engine_graph_properties(properties)?;
    if let Some(weight) = weight {
        return EngineGraphEdgeData::new(weight, properties).map_err(ExecutorError::from);
    }
    Ok(EngineGraphEdgeData::default_weight(properties))
}

const fn engine_graph_direction(direction: GraphDirection) -> EngineGraphDirection {
    match direction {
        GraphDirection::Outgoing => EngineGraphDirection::Outgoing,
        GraphDirection::Incoming => EngineGraphDirection::Incoming,
        GraphDirection::Both => EngineGraphDirection::Both,
    }
}

const fn output_graph_direction(direction: EngineGraphDirection) -> GraphDirection {
    match direction {
        EngineGraphDirection::Outgoing => GraphDirection::Outgoing,
        EngineGraphDirection::Incoming => GraphDirection::Incoming,
        EngineGraphDirection::Both => GraphDirection::Both,
    }
}

const fn engine_graph_binding_primitive(
    primitive: GraphBindingPrimitive,
) -> EngineGraphBindingPrimitive {
    match primitive {
        GraphBindingPrimitive::Kv => EngineGraphBindingPrimitive::Kv,
        GraphBindingPrimitive::Json => EngineGraphBindingPrimitive::Json,
        GraphBindingPrimitive::Vector => EngineGraphBindingPrimitive::Vector,
        GraphBindingPrimitive::Event => EngineGraphBindingPrimitive::Event,
        GraphBindingPrimitive::Graph => EngineGraphBindingPrimitive::Graph,
    }
}

const fn output_graph_binding_primitive(
    primitive: EngineGraphBindingPrimitive,
) -> GraphBindingPrimitive {
    match primitive {
        EngineGraphBindingPrimitive::Kv => GraphBindingPrimitive::Kv,
        EngineGraphBindingPrimitive::Json => GraphBindingPrimitive::Json,
        EngineGraphBindingPrimitive::Vector => GraphBindingPrimitive::Vector,
        EngineGraphBindingPrimitive::Event => GraphBindingPrimitive::Event,
        EngineGraphBindingPrimitive::Graph => GraphBindingPrimitive::Graph,
    }
}

fn engine_graph_binding_target(
    target: GraphBindingTarget,
) -> ExecutorResult<EngineGraphBindingTarget> {
    let (primitive, branch, space, key) = target.into_parts();
    let branch = branch
        .as_deref()
        .map(|branch| branch_name(Some(branch), DEFAULT_BRANCH))
        .transpose()?;
    let space = product_space(Some(&space))?;
    EngineGraphBindingTarget::new(
        engine_graph_binding_primitive(primitive),
        branch,
        space,
        key,
    )
    .map_err(ExecutorError::from)
}

fn engine_graph_entity_binding(
    binding: GraphEntityBinding,
) -> ExecutorResult<EngineGraphEntityBinding> {
    Ok(EngineGraphEntityBinding::new(engine_graph_binding_target(
        binding.into_target(),
    )?))
}

fn engine_graph_batch_operation(
    operation: GraphBatchOperation,
) -> ExecutorResult<EngineGraphBatchOperation> {
    match operation {
        GraphBatchOperation::UpsertNode { node_id, data } => {
            Ok(EngineGraphBatchOperation::UpsertNode {
                node_id: graph_node_id(node_id)?,
                data: engine_graph_node_data(data)?,
            })
        }
        GraphBatchOperation::DeleteNode { node_id } => Ok(EngineGraphBatchOperation::DeleteNode {
            node_id: graph_node_id(node_id)?,
        }),
        GraphBatchOperation::UpsertEdge {
            src,
            edge_type,
            dst,
            data,
        } => Ok(EngineGraphBatchOperation::UpsertEdge {
            src: graph_node_id(src)?,
            edge_type: graph_edge_type(edge_type)?,
            dst: graph_node_id(dst)?,
            data: engine_graph_edge_data(data)?,
        }),
        GraphBatchOperation::DeleteEdge {
            src,
            edge_type,
            dst,
        } => Ok(EngineGraphBatchOperation::DeleteEdge {
            src: graph_node_id(src)?,
            edge_type: graph_edge_type(edge_type)?,
            dst: graph_node_id(dst)?,
        }),
    }
}

const fn graph_batch_operation_name(operation: &GraphBatchOperation) -> &'static str {
    match operation {
        GraphBatchOperation::UpsertNode { .. } => "upsert_node",
        GraphBatchOperation::DeleteNode { .. } => "delete_node",
        GraphBatchOperation::UpsertEdge { .. } => "upsert_edge",
        GraphBatchOperation::DeleteEdge { .. } => "delete_edge",
    }
}

fn output_graph_binding_target(target: &EngineGraphBindingTarget) -> GraphBindingTarget {
    GraphBindingTarget::new(
        output_graph_binding_primitive(target.primitive()),
        target.branch().map(|branch| branch.as_str().to_owned()),
        target.space().as_str().to_owned(),
        target.key().to_owned(),
    )
}

fn output_graph_entity_binding(binding: &EngineGraphEntityBinding) -> GraphEntityBinding {
    GraphEntityBinding::new(output_graph_binding_target(binding.target()))
}

fn graph_info_data(info: &EngineGraphInfo) -> GraphInfoData {
    GraphInfoData::new(
        info.name().as_str().to_owned(),
        info.node_count(),
        info.edge_count(),
        info.created_version().as_u64(),
        info.created_timestamp().as_micros(),
        info.updated_version().as_u64(),
        info.updated_timestamp().as_micros(),
    )
}

fn graph_node_data_output(node: &EngineGraphNode) -> GraphNodeDataOutput {
    GraphNodeDataOutput::new(
        node.graph().as_str().to_owned(),
        node.node_id().as_str().to_owned(),
        node.data()
            .properties()
            .map(|properties| properties.as_inner().clone()),
        node.data().binding().map(output_graph_entity_binding),
        node.version().as_u64(),
        node.timestamp().as_micros(),
    )
}

fn graph_edge_data_output(edge: &EngineGraphEdge) -> GraphEdgeDataOutput {
    GraphEdgeDataOutput::new(
        edge.graph().as_str().to_owned(),
        edge.src().as_str().to_owned(),
        edge.edge_type().as_str().to_owned(),
        edge.dst().as_str().to_owned(),
        edge.data().weight(),
        edge.data()
            .properties()
            .map(|properties| properties.as_inner().clone()),
        edge.version().as_u64(),
        edge.timestamp().as_micros(),
    )
}

fn graph_neighbor_hit(neighbor: &EngineGraphNeighbor) -> GraphNeighborHit {
    GraphNeighborHit::new(
        graph_node_data_output(neighbor.node()),
        graph_edge_data_output(neighbor.edge()),
        output_graph_direction(neighbor.direction()),
    )
}

fn graph_binding_hit(binding: &EngineGraphBinding) -> GraphBindingHit {
    GraphBindingHit::new(
        binding.graph().as_str().to_owned(),
        binding.node_id().as_str().to_owned(),
        output_graph_entity_binding(binding.binding()),
        binding.version().as_u64(),
        binding.timestamp().as_micros(),
    )
}

fn graph_name_page_output(page: &EngineGraphNamePage) -> Output {
    Output::GraphNamePage {
        graphs: page
            .graphs()
            .iter()
            .map(|graph| graph.as_str().to_owned())
            .collect(),
        has_more: page.has_more(),
        cursor: page.cursor().map(|cursor| cursor.as_str().to_owned()),
    }
}

fn graph_node_page_output(page: &EngineGraphNodePage) -> Output {
    Output::GraphNodePage {
        nodes: page.nodes().iter().map(graph_node_data_output).collect(),
        has_more: page.has_more(),
        cursor: page.cursor().map(|cursor| cursor.as_str().to_owned()),
    }
}

fn graph_neighbor_page_output(page: &EngineGraphNeighborPage) -> Output {
    Output::GraphNeighborPage {
        neighbors: page.neighbors().iter().map(graph_neighbor_hit).collect(),
        has_more: page.has_more(),
        cursor: page.cursor().map(str::to_owned),
    }
}

fn graph_binding_page_output(page: &EngineGraphBindingPage) -> Output {
    Output::GraphBindingPage {
        bindings: page.bindings().iter().map(graph_binding_hit).collect(),
        has_more: page.has_more(),
        cursor: page.cursor().map(str::to_owned),
    }
}

fn graph_node_write_output(outcome: &EngineGraphWriteOutcome) -> Output {
    let commit = outcome.commit();
    Output::GraphNodeWriteResult {
        graph: outcome.graph().as_str().to_owned(),
        node_id: outcome.node_id().as_str().to_owned(),
        created: outcome.created(),
        version: commit.version().as_u64(),
        timestamp: commit.timestamp().as_micros(),
    }
}

fn graph_edge_write_output(outcome: &EngineGraphEdgeWriteOutcome) -> Output {
    let commit = outcome.commit();
    Output::GraphEdgeWriteResult {
        graph: outcome.graph().as_str().to_owned(),
        src: outcome.src().as_str().to_owned(),
        edge_type: outcome.edge_type().as_str().to_owned(),
        dst: outcome.dst().as_str().to_owned(),
        created: outcome.created(),
        version: commit.version().as_u64(),
        timestamp: commit.timestamp().as_micros(),
    }
}

fn graph_delete_output(
    outcome: &EngineGraphDeleteOutcome,
    node_id: Option<String>,
    src: Option<String>,
    edge_type: Option<String>,
    dst: Option<String>,
) -> Output {
    Output::GraphDeleteResult {
        graph: outcome.graph().as_str().to_owned(),
        node_id,
        src,
        edge_type,
        dst,
        deleted: outcome.deleted(),
        version: outcome.commit().map(|commit| commit.version().as_u64()),
        timestamp: outcome
            .commit()
            .map(|commit| commit.timestamp().as_micros()),
    }
}

fn graph_batch_write_output(
    outcome: &EngineGraphBatchWriteOutcome,
    operation_names: &[&'static str],
) -> Output {
    let version = outcome.commit().map(|commit| commit.version().as_u64());
    let timestamp = outcome
        .commit()
        .map(|commit| commit.timestamp().as_micros());
    Output::GraphBatchWriteResult {
        graph: outcome.graph().as_str().to_owned(),
        results: outcome
            .results()
            .iter()
            .map(|item| graph_batch_item_result(item, operation_names, version, timestamp))
            .collect(),
        version,
        timestamp,
    }
}

fn graph_batch_item_result(
    item: &EngineGraphBatchOpOutcome,
    operation_names: &[&'static str],
    version: Option<u64>,
    timestamp: Option<u64>,
) -> GraphBatchItemResult {
    let operation_index = usize_to_u64(item.operation_index());
    let operation = operation_names
        .get(item.operation_index())
        .copied()
        .unwrap_or("unknown");
    let applied = item.created_flag().is_some() || item.deleted_flag() == Some(true);
    GraphBatchItemResult::new(
        operation_index,
        operation,
        item.created_flag(),
        item.deleted_flag(),
        applied.then_some(version).flatten(),
        applied.then_some(timestamp).flatten(),
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
