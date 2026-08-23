//! Executor-facing engine contract built over the storage persistence boundary.

#![deny(unsafe_code)]
// `EngineResult<T>` is the stable product API shape; boxing `EngineError`
// would be a deliberate contract change rather than a local lint cleanup.
#![allow(clippy::result_large_err)]

pub mod api;
pub mod artifact;

mod branch;
mod commit;
mod config;
mod control;
mod data;
mod diagnostics;
mod persistence;
mod runtime;
mod time_compat;

#[cfg(any(test, feature = "testkit"))]
pub mod test_support;

#[cfg(any(test, feature = "testkit"))]
pub mod testkit;

pub use api::{
    error_code_registry_entries, error_code_registry_entry, AdminCapabilitySummary,
    AdminConfigSummary, AdminDatabaseInfo, AdminDescribeSummary, AdminGraphSummary,
    AdminHealthStatus, AdminHealthSummary, AdminMetricsSummary, AdminPingSummary,
    AdminPrimitiveSummary, AdminService, AdminVectorCollectionSummary, BranchCleanupSummary,
    BranchComparison, BranchCreateOutcome, BranchDeleteOutcome, BranchName, BranchParentSummary,
    BranchService, BranchStateSelector, BranchStatus, BranchSummary, CacheOpenOptions,
    CachePreheat, CloseOutcome, CommitDurability, CommitOutcomeStatus, ComparedCapability,
    ComparedEntity, ControlDiagnostics, ControlHealthStatus, Database, DatabaseOpenOutcome,
    DatabaseOpenSummary, DatabaseOpenTarget, DurabilityMode, DurableLocalOpenOptions, EngineError,
    EngineErrorClass, EngineErrorStatus, EngineResult, ErrorClass, ErrorCodeRegistryEntry,
    ErrorDetail, EventAppendOutcome, EventBatchAppendEntry, EventBatchAppendItemOutcome,
    EventBatchAppendOutcome, EventChainVerification, EventLength, EventPayload,
    EventRangeDirection, EventRangePage, EventSequence, EventService, EventType, EventTypeList,
    EventVersionedRecord, GraphAdjacencyEdge, GraphAdjacencyIndex, GraphAnalyticsBudget,
    GraphBatchOpOutcome, GraphBatchOperation, GraphBatchWrite, GraphBatchWriteOutcome,
    GraphBfsOptions, GraphBfsResult, GraphBinding, GraphBindingPage, GraphBindingPrimitive,
    GraphBindingTarget, GraphBulkInsertOutcome, GraphCdlpOptions, GraphCdlpResult,
    GraphDeleteOutcome, GraphDeletePolicy, GraphDeletePolicyOutcome, GraphDirection, GraphEdge,
    GraphEdgeData, GraphEdgeType, GraphEdgeWriteOutcome, GraphEntityBinding, GraphInfo,
    GraphLccResult, GraphLinkTypeDef, GraphLinkTypeSummary, GraphName, GraphNamePage,
    GraphNeighbor, GraphNeighborPage, GraphNode, GraphNodeData, GraphNodeId, GraphNodePage,
    GraphObjectTypeDef, GraphObjectTypeSummary, GraphOntology, GraphOntologyFreezeOutcome,
    GraphOntologyStatus, GraphOntologySummary, GraphOntologyWriteOutcome, GraphPageRankOptions,
    GraphPageRankResult, GraphProperties, GraphPropertyDef, GraphService, GraphSsspResult,
    GraphSubgraphResult, GraphTargetStatus, GraphTraversalEdge, GraphTypeName, GraphWccResult,
    GraphWriteOutcome, JsonBatchDeleteOutcome, JsonBatchSetItemOutcome, JsonBatchSetOutcome,
    JsonDeleteOutcome, JsonDocumentId, JsonGetEntry, JsonHistory, JsonHistoryRow,
    JsonIndexDefinition, JsonIndexName, JsonIndexType, JsonListPage, JsonPath, JsonPathSegment,
    JsonSample, JsonSampleRow, JsonService, JsonSetEntry, JsonValue, JsonVersionedValue,
    JsonWriteOutcome, KvBatchDeleteOutcome, KvBatchPutOutcome, KvDeleteOutcome, KvHistory,
    KvHistoryRow, KvKey, KvListPage, KvSample, KvScanRow, KvService, KvValue, KvVersionedValue,
    KvWriteOutcome, MemoryBudgetSource, ProductSpace, RetryPolicy, SpaceCatalogDiagnostics,
    SpaceComparison, SpaceCreateOutcome, SpaceDeleteOutcome, SpaceService, SpaceUsageSummary,
    VectorArtifactSourceDiagnostic, VectorBatchDeleteOutcome, VectorBatchGetOutcome,
    VectorBatchUpsertOutcome, VectorBulkDeleteOutcome, VectorCollectionInfo, VectorCollectionName,
    VectorConfig, VectorDeleteOutcome, VectorDistanceMetric, VectorEmbedding, VectorEntry,
    VectorFilter, VectorFilterCondition, VectorFilterOp, VectorHistory, VectorHistoryRow,
    VectorIndexDiagnostics, VectorKey, VectorKeyPage, VectorMetadata, VectorMetadataPatch,
    VectorMetadataUpdateOutcome, VectorScalar, VectorSearchMatch, VectorSearchResult,
    VectorService, VectorUpsertEntry, VectorVersionedEntry, VectorWriteOutcome,
};
