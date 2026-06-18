//! Executor-facing engine contract built over the storage persistence boundary.

#![deny(unsafe_code)]

pub mod api;

mod branch;
mod commit;
mod config;
mod control;
mod data;
mod diagnostics;
mod persistence;
mod runtime;

#[cfg(any(test, feature = "testkit"))]
pub mod test_support;

#[cfg(any(test, feature = "testkit"))]
pub mod testkit;

pub use api::{
    BranchCleanupSummary, BranchCreateOutcome, BranchDeleteOutcome, BranchName,
    BranchParentSummary, BranchService, BranchStatus, BranchSummary, CacheOpenOptions,
    CloseOutcome, Database, DatabaseOpenOutcome, DatabaseOpenSummary, DatabaseOpenTarget,
    DurableLocalOpenOptions, EngineError, EngineErrorClass, EngineResult, EventAppendOutcome,
    EventBatchAppendEntry, EventBatchAppendItemOutcome, EventBatchAppendOutcome,
    EventChainVerification, EventLength, EventPayload, EventRangeDirection, EventRangePage,
    EventSequence, EventService, EventType, EventTypeList, EventVersionedRecord,
    GraphBatchOpOutcome, GraphBatchOperation, GraphBatchWrite, GraphBatchWriteOutcome,
    GraphBinding, GraphBindingPage, GraphBindingPrimitive, GraphBindingTarget, GraphDeleteOutcome,
    GraphDirection, GraphEdge, GraphEdgeData, GraphEdgeType, GraphEdgeWriteOutcome,
    GraphEntityBinding, GraphInfo, GraphName, GraphNamePage, GraphNeighbor, GraphNeighborPage,
    GraphNode, GraphNodeData, GraphNodeId, GraphNodePage, GraphProperties, GraphService,
    GraphWriteOutcome, JsonBatchDeleteOutcome, JsonBatchSetItemOutcome, JsonBatchSetOutcome,
    JsonDeleteOutcome, JsonDocumentId, JsonGetEntry, JsonHistory, JsonHistoryRow,
    JsonIndexDefinition, JsonIndexName, JsonIndexType, JsonListPage, JsonPath, JsonPathSegment,
    JsonSample, JsonSampleRow, JsonService, JsonSetEntry, JsonValue, JsonVersionedValue,
    JsonWriteOutcome, KvBatchDeleteOutcome, KvDeleteOutcome, KvHistory, KvHistoryRow, KvKey,
    KvListPage, KvSample, KvScanRow, KvService, KvValue, KvVersionedValue, ProductSpace,
    VectorBatchDeleteOutcome, VectorBatchGetOutcome, VectorBatchUpsertOutcome,
    VectorBulkDeleteOutcome, VectorCollectionInfo, VectorCollectionName, VectorConfig,
    VectorDeleteOutcome, VectorDistanceMetric, VectorEmbedding, VectorEntry, VectorFilter,
    VectorFilterCondition, VectorFilterOp, VectorHistory, VectorHistoryRow, VectorKey,
    VectorKeyPage, VectorMetadata, VectorMetadataPatch, VectorMetadataUpdateOutcome, VectorScalar,
    VectorSearchMatch, VectorSearchResult, VectorService, VectorUpsertEntry, VectorVersionedEntry,
    VectorWriteOutcome,
};
