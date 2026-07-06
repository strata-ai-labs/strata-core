//! Serializable command outputs.

use crate::types::{
    AdminConfig, AdminDatabaseInfo, AdminDescribe, AdminHealth, AdminMetrics, ArrowExportResult,
    ArrowImportResult, BatchGetItemResult, BatchItemResult, BatchResult, BranchCleanupItem,
    BranchItem, Bytes, CommitReceipt, EventBatchAppendItemResult, EventChainVerification,
    EventVersionedData, GraphBatchItemResult, GraphBindingHit, GraphEdgeDataOutput, GraphInfoData,
    GraphNeighborHit, GraphNodeDataOutput, HistoryResult, JsonBatchGetItemResult,
    JsonBatchItemResult, JsonHistoryItem, JsonIndexDefinition, JsonSampleItem, MaybeJsonValue,
    MaybeJsonVersionedValue, MutationEffect, PageInfo, SampleItem, ScanItem,
    VectorBatchGetItemResult, VectorBatchItemResult, VectorCollectionInfo, VectorHistoryResult,
    VectorIndexQueryResult, VectorMatch, VectorVersionedData, VersionedValue,
};
use serde::{Deserialize, Serialize};

/// Successful executor output.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Output {
    /// Lightweight admin liveness result.
    Pong {
        /// Engine package version.
        version: String,
    },
    /// Database identity and catalog summary.
    DatabaseInfo(AdminDatabaseInfo),
    /// Control-plane health facts.
    Health(AdminHealth),
    /// Lightweight database metrics.
    Metrics(AdminMetrics),
    /// Compact database description.
    Described(AdminDescribe),
    /// Sanitized configuration facts.
    Config(AdminConfig),
    /// Optional sanitized configuration value.
    ConfigValue(Option<String>),
    /// Product space list.
    SpaceList {
        /// Spaces in this page.
        items: Vec<String>,
        /// Shared page continuation facts.
        #[serde(flatten)]
        page: PageInfo<String>,
    },
    /// Product space create result.
    SpaceCreateResult {
        /// Product space name.
        space: String,
        /// True when a catalog row was created.
        created: bool,
        /// Mutation effect facts.
        effect: MutationEffect,
        /// Commit receipt when a catalog mutation was applied.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        commit: Option<CommitReceipt>,
        /// Commit version when a catalog mutation was applied.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        version: Option<u64>,
        /// Commit timestamp when a catalog mutation was applied.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timestamp: Option<u64>,
    },
    /// Product space delete result.
    SpaceDeleteResult {
        /// Product space name.
        space: String,
        /// True when a catalog row was deleted.
        deleted: bool,
        /// True when visible space data was force-deleted.
        force: bool,
        /// Number of visible space rows tombstoned, including primitive index/control rows.
        deleted_rows: u64,
        /// Mutation effect facts.
        effect: MutationEffect,
        /// Commit receipt when a catalog mutation was applied.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        commit: Option<CommitReceipt>,
        /// Commit version when a catalog mutation was applied.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        version: Option<u64>,
        /// Commit timestamp when a catalog mutation was applied.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timestamp: Option<u64>,
    },
    /// One branch summary.
    Branch(BranchItem),
    /// Branch list.
    Branches {
        /// Branches in this page.
        items: Vec<BranchItem>,
        /// Shared page continuation facts.
        #[serde(flatten)]
        page: PageInfo<String>,
    },
    /// Branch deletion result.
    BranchDeleteResult {
        /// True when the branch was deleted.
        deleted: bool,
        /// Mutation effect facts.
        effect: MutationEffect,
        /// Deleted branch summary.
        branch: BranchItem,
        /// Generation before delete.
        generation_before: Option<u64>,
        /// Generation after delete.
        generation_after: Option<u64>,
        /// Cleanup facts.
        cleanup: Option<BranchCleanupItem>,
    },
    /// Optional raw KV value.
    KvValue(Option<Bytes>),
    /// Optional KV value with commit metadata.
    KvVersionedValue(Option<VersionedValue>),
    /// Optional JSON value.
    JsonValue(MaybeJsonValue),
    /// Optional JSON value with commit metadata.
    JsonVersionedValue(MaybeJsonVersionedValue),
    /// Full version history for one key.
    VersionHistory(Option<HistoryResult>),
    /// Full JSON document version history.
    JsonVersionHistory(Option<Vec<JsonHistoryItem>>),
    /// Key list.
    Keys {
        /// Keys in this page.
        items: Vec<Bytes>,
        /// Shared page continuation facts.
        #[serde(flatten)]
        page: PageInfo<Bytes>,
    },
    /// Paginated key list.
    KeysPage {
        /// Keys in this page.
        items: Vec<Bytes>,
        /// Shared page continuation facts.
        #[serde(flatten)]
        page: PageInfo<Bytes>,
    },
    /// Write acknowledgement.
    WriteResult {
        /// Written key.
        key: Bytes,
        /// Mutation effect facts.
        effect: MutationEffect,
        /// Commit receipt.
        commit: CommitReceipt,
    },
    /// Delete acknowledgement.
    DeleteResult {
        /// Target key.
        key: Bytes,
        /// Mutation effect facts.
        effect: MutationEffect,
        /// Commit receipt when a delete was applied.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        commit: Option<CommitReceipt>,
    },
    /// KV scan result.
    KvScanResult {
        /// Rows in this page.
        items: Vec<ScanItem>,
        /// Shared page continuation facts.
        #[serde(flatten)]
        page: PageInfo<Bytes>,
    },
    /// Positional batch write/delete results.
    BatchResults(BatchResult<BatchItemResult>),
    /// Positional batch read results.
    BatchGetResults(BatchResult<BatchGetItemResult>),
    /// Positional JSON batch write/delete results.
    JsonBatchResults(BatchResult<JsonBatchItemResult>),
    /// Positional JSON batch read results.
    JsonBatchGetResults(BatchResult<JsonBatchGetItemResult>),
    /// Boolean result.
    Bool(bool),
    /// Positional boolean results.
    BoolList(Vec<bool>),
    /// Unsigned integer result.
    Uint(u64),
    /// Sampled KV result.
    SampleResult {
        /// Total matching live rows.
        total_count: u64,
        /// Sampled rows.
        items: Vec<SampleItem>,
        /// Shared page continuation facts.
        #[serde(flatten)]
        page: PageInfo<Bytes>,
    },
    /// Paginated JSON document key list.
    JsonListResult {
        /// Keys in this page.
        items: Vec<String>,
        /// Shared page continuation facts.
        #[serde(flatten)]
        page: PageInfo<String>,
    },
    /// Sampled JSON documents.
    JsonSampleResult {
        /// Total matching live documents.
        total_count: u64,
        /// Sampled documents.
        items: Vec<JsonSampleItem>,
        /// Shared page continuation facts.
        #[serde(flatten)]
        page: PageInfo<String>,
    },
    /// JSON secondary index definition.
    JsonIndexDefinition(JsonIndexDefinition),
    /// JSON secondary index definitions.
    JsonIndexList {
        /// Indexes in this page.
        items: Vec<JsonIndexDefinition>,
        /// Shared page continuation facts.
        #[serde(flatten)]
        page: PageInfo<String>,
    },
    /// Vector write acknowledgement.
    VectorWriteResult {
        /// Collection name.
        collection: String,
        /// Vector key.
        key: String,
        /// Mutation effect facts.
        effect: MutationEffect,
        /// Commit receipt.
        commit: CommitReceipt,
        /// Commit version.
        version: u64,
        /// Commit timestamp.
        timestamp: u64,
        /// Product vector revision.
        vector_revision: u64,
    },
    /// Vector metadata update acknowledgement.
    VectorMetadataUpdateResult {
        /// Collection name.
        collection: String,
        /// Vector key.
        key: String,
        /// True when a visible vector was updated.
        updated: bool,
        /// Mutation effect facts.
        effect: MutationEffect,
        /// Commit receipt when an update was applied.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        commit: Option<CommitReceipt>,
        /// Commit version when an update was applied.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        version: Option<u64>,
        /// Commit timestamp when an update was applied.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timestamp: Option<u64>,
        /// Product vector revision when an update was applied.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        vector_revision: Option<u64>,
    },
    /// Vector delete acknowledgement.
    VectorDeleteResult {
        /// Collection name.
        collection: String,
        /// Vector key.
        key: String,
        /// True when a visible vector was deleted.
        deleted: bool,
        /// Mutation effect facts.
        effect: MutationEffect,
        /// Commit receipt when a delete was applied.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        commit: Option<CommitReceipt>,
        /// Commit version when a delete was applied.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        version: Option<u64>,
        /// Commit timestamp when a delete was applied.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timestamp: Option<u64>,
    },
    /// Vector bulk delete acknowledgement.
    VectorBulkDeleteResult {
        /// Collection name.
        collection: String,
        /// Number of visible vectors deleted.
        deleted_count: u64,
        /// Mutation effect facts.
        effect: MutationEffect,
        /// Commit receipt when deletes were applied.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        commit: Option<CommitReceipt>,
        /// Commit version when deletes were applied.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        version: Option<u64>,
        /// Commit timestamp when deletes were applied.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timestamp: Option<u64>,
    },
    /// Optional vector value.
    VectorData(Option<VectorVersionedData>),
    /// Full vector history.
    VectorVersionHistory(Option<VectorHistoryResult>),
    /// Vector search matches.
    VectorMatches(Vec<VectorMatch>),
    /// Vector search matches plus index planner diagnostics.
    VectorIndexQuery(VectorIndexQueryResult),
    /// Paginated vector key list.
    VectorKeyPage {
        /// Keys in this page.
        items: Vec<String>,
        /// Shared page continuation facts.
        #[serde(flatten)]
        page: PageInfo<String>,
    },
    /// Vector collection list.
    VectorCollectionList {
        /// Collections in this page.
        items: Vec<VectorCollectionInfo>,
        /// Shared page continuation facts.
        #[serde(flatten)]
        page: PageInfo<String>,
    },
    /// Positional vector batch write results.
    VectorBatchUpsertResults(BatchResult<VectorBatchItemResult>),
    /// Positional vector batch read results.
    VectorBatchGetResults(BatchResult<VectorBatchGetItemResult>),
    /// Positional vector batch delete results.
    VectorBatchDeleteResults(BatchResult<VectorBatchItemResult>),
    /// Event append acknowledgement.
    EventAppendResult {
        /// Assigned sequence.
        sequence: u64,
        /// Appended event type.
        event_type: String,
        /// Mutation effect facts.
        effect: MutationEffect,
        /// Commit receipt.
        commit: CommitReceipt,
        /// Commit version.
        version: u64,
        /// Commit timestamp.
        timestamp: u64,
    },
    /// Optional event record.
    EventRecord(Option<EventVersionedData>),
    /// Event records.
    EventRecords {
        /// Events in this page.
        items: Vec<EventVersionedData>,
        /// Shared page continuation facts.
        #[serde(flatten)]
        page: PageInfo<u64>,
    },
    /// Event log length.
    EventLength {
        /// Visible event count.
        count: u64,
    },
    /// Event type list.
    EventTypeList {
        /// Event types in this page.
        items: Vec<String>,
        /// Shared page continuation facts.
        #[serde(flatten)]
        page: PageInfo<String>,
    },
    /// Paginated event range.
    EventRangeResult {
        /// Events in this page.
        items: Vec<EventVersionedData>,
        /// Shared page continuation facts.
        #[serde(flatten)]
        page: PageInfo<u64>,
    },
    /// Positional event batch append results.
    EventBatchAppendResults(BatchResult<EventBatchAppendItemResult>),
    /// Event hash-chain verification result.
    EventChainVerification(EventChainVerification),
    /// Graph metadata after create.
    GraphInfo(GraphInfoData),
    /// Optional graph metadata.
    GraphInfoResult(Option<GraphInfoData>),
    /// Paginated graph name list.
    GraphNamePage {
        /// Graphs in this page.
        items: Vec<String>,
        /// Shared page continuation facts.
        #[serde(flatten)]
        page: PageInfo<String>,
    },
    /// Optional graph node.
    GraphNodeResult(Option<GraphNodeDataOutput>),
    /// Paginated graph node list.
    GraphNodePage {
        /// Nodes in this page.
        items: Vec<GraphNodeDataOutput>,
        /// Shared page continuation facts.
        #[serde(flatten)]
        page: PageInfo<String>,
    },
    /// Optional graph edge.
    GraphEdgeResult(Option<GraphEdgeDataOutput>),
    /// Paginated graph neighbor list.
    GraphNeighborPage {
        /// Neighbor hits in this page.
        items: Vec<GraphNeighborHit>,
        /// Shared page continuation facts.
        #[serde(flatten)]
        page: PageInfo<String>,
    },
    /// Paginated graph binding list.
    GraphBindingPage {
        /// Binding hits in this page.
        items: Vec<GraphBindingHit>,
        /// Shared page continuation facts.
        #[serde(flatten)]
        page: PageInfo<String>,
    },
    /// Graph node write acknowledgement.
    GraphNodeWriteResult {
        /// Graph name.
        graph: String,
        /// Node id.
        node_id: String,
        /// True when the node did not previously exist.
        created: bool,
        /// Mutation effect facts.
        effect: MutationEffect,
        /// Commit receipt.
        commit: CommitReceipt,
        /// Commit version.
        version: u64,
        /// Commit timestamp.
        timestamp: u64,
    },
    /// Graph edge write acknowledgement.
    GraphEdgeWriteResult {
        /// Graph name.
        graph: String,
        /// Source node id.
        src: String,
        /// Edge type.
        edge_type: String,
        /// Destination node id.
        dst: String,
        /// True when the edge did not previously exist.
        created: bool,
        /// Mutation effect facts.
        effect: MutationEffect,
        /// Commit receipt.
        commit: CommitReceipt,
        /// Commit version.
        version: u64,
        /// Commit timestamp.
        timestamp: u64,
    },
    /// Graph delete acknowledgement.
    GraphDeleteResult {
        /// Graph name.
        graph: String,
        /// Deleted node id for node deletes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        node_id: Option<String>,
        /// Deleted edge source for edge deletes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        src: Option<String>,
        /// Deleted edge type for edge deletes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        edge_type: Option<String>,
        /// Deleted edge destination for edge deletes.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        dst: Option<String>,
        /// True when a visible graph fact was deleted.
        deleted: bool,
        /// Mutation effect facts.
        effect: MutationEffect,
        /// Commit receipt when a delete was applied.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        commit: Option<CommitReceipt>,
        /// Commit version when a delete was applied.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        version: Option<u64>,
        /// Commit timestamp when a delete was applied.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timestamp: Option<u64>,
    },
    /// Graph batch write acknowledgement.
    GraphBatchWriteResult {
        /// Graph name.
        graph: String,
        /// Shared batch result facts.
        #[serde(flatten)]
        batch: BatchResult<GraphBatchItemResult>,
    },
    /// Arrow import summary.
    ArrowImportResult(ArrowImportResult),
    /// Arrow export summary.
    ArrowExportResult(ArrowExportResult),
    /// Inference model list.
    #[cfg(feature = "inference")]
    InferenceModels {
        /// Models in this page.
        items: Vec<strata_inference_next::ModelInfo>,
        /// Shared page continuation facts.
        #[serde(flatten)]
        page: PageInfo<String>,
    },
    /// Inference model pull output.
    #[cfg(feature = "inference")]
    InferenceModelPulled(strata_inference_next::PullModelOutput),
    /// Inference capability facts.
    #[cfg(feature = "inference")]
    InferenceCapability(strata_inference_next::InferenceCapability),
    /// Inference generation output.
    #[cfg(feature = "inference")]
    InferenceGeneration(strata_inference_next::GenerateResponse),
    /// Inference token ids.
    #[cfg(feature = "inference")]
    InferenceTokenIds(Vec<u32>),
    /// Inference detokenized text.
    #[cfg(feature = "inference")]
    InferenceText(String),
    /// Inference embedding vector.
    #[cfg(feature = "inference")]
    InferenceEmbedding(Vec<f32>),
    /// Inference batch embedding output.
    #[cfg(feature = "inference")]
    InferenceEmbeddings(strata_inference_next::EmbedResponse),
    /// Inference ranking output.
    #[cfg(feature = "inference")]
    InferenceRanking(strata_inference_next::RankResponse),
    /// Inference unload result.
    #[cfg(feature = "inference")]
    InferenceUnloadResult {
        /// True when a cached entry was removed.
        unloaded: bool,
    },
    /// Inference runtime cache diagnostics.
    #[cfg(feature = "inference")]
    InferenceCacheStatus(strata_inference_next::ModelCacheStatus),
}
