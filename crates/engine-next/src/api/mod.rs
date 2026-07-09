//! Executor-facing engine API.

mod admin;
mod branch;
mod control;
mod database;
mod event;
mod graph;
mod json;
mod kv;
mod options;
mod space;
mod vector;

pub use admin::{
    AdminCapabilitySummary, AdminConfigSummary, AdminDatabaseInfo, AdminDescribeSummary,
    AdminGraphSummary, AdminHealthStatus, AdminHealthSummary, AdminMetricsSummary,
    AdminPingSummary, AdminPrimitiveSummary, AdminService, AdminVectorCollectionSummary,
};
pub use branch::{
    BranchCleanupSummary, BranchCreateOutcome, BranchDeleteOutcome, BranchParentSummary,
    BranchStatus, BranchSummary,
};
pub use control::{ControlDiagnostics, ControlHealthStatus, SpaceCatalogDiagnostics};
pub use database::{
    CloseOutcome, Database, DatabaseOpenOutcome, DatabaseOpenSummary, DatabaseOpenTarget,
};
pub use event::{
    EventAppendOutcome, EventBatchAppendEntry, EventBatchAppendItemOutcome,
    EventBatchAppendOutcome, EventChainVerification, EventLength, EventPayload,
    EventRangeDirection, EventRangePage, EventSequence, EventService, EventType, EventTypeList,
    EventVersionedRecord,
};
pub use graph::{
    GraphAdjacencyEdge, GraphAdjacencyIndex, GraphAnalyticsBudget, GraphBatchOpOutcome,
    GraphBatchOperation, GraphBatchWrite, GraphBatchWriteOutcome, GraphBinding, GraphBindingPage,
    GraphBindingPrimitive, GraphBindingTarget, GraphDeleteOutcome, GraphDirection, GraphEdge,
    GraphEdgeData, GraphEdgeType, GraphEdgeWriteOutcome, GraphEntityBinding, GraphInfo,
    GraphLinkTypeDef, GraphLinkTypeSummary, GraphName, GraphNamePage, GraphNeighbor,
    GraphNeighborPage, GraphNode, GraphNodeData, GraphNodeId, GraphNodePage, GraphObjectTypeDef,
    GraphObjectTypeSummary, GraphOntology, GraphOntologyFreezeOutcome, GraphOntologyStatus,
    GraphOntologySummary, GraphOntologyWriteOutcome, GraphProperties, GraphPropertyDef,
    GraphService, GraphTypeName, GraphWriteOutcome,
};
pub use json::{
    JsonBatchDeleteOutcome, JsonBatchSetItemOutcome, JsonBatchSetOutcome, JsonDeleteOutcome,
    JsonDocumentId, JsonGetEntry, JsonHistory, JsonHistoryRow, JsonIndexDefinition, JsonIndexName,
    JsonIndexType, JsonListPage, JsonPath, JsonPathSegment, JsonSample, JsonSampleRow, JsonService,
    JsonSetEntry, JsonValue, JsonVersionedValue, JsonWriteOutcome,
};
pub use kv::{
    KvBatchDeleteOutcome, KvDeleteOutcome, KvHistory, KvHistoryRow, KvKey, KvListPage, KvSample,
    KvScanRow, KvService, KvValue, KvVersionedValue, ProductSpace,
};
pub use options::{CacheOpenOptions, CachePreheat, DurableLocalOpenOptions};
pub use space::{SpaceCreateOutcome, SpaceDeleteOutcome, SpaceService, SpaceUsageSummary};
pub use vector::{
    VectorArtifactSourceDiagnostic, VectorBatchDeleteOutcome, VectorBatchGetOutcome,
    VectorBatchUpsertOutcome, VectorBulkDeleteOutcome, VectorCollectionInfo, VectorCollectionName,
    VectorConfig, VectorDeleteOutcome, VectorDistanceMetric, VectorEmbedding, VectorEntry,
    VectorFilter, VectorFilterCondition, VectorFilterOp, VectorHistory, VectorHistoryRow,
    VectorIndexDiagnostics, VectorKey, VectorKeyPage, VectorMetadata, VectorMetadataPatch,
    VectorMetadataUpdateOutcome, VectorScalar, VectorSearchMatch, VectorSearchResult,
    VectorService, VectorUpsertEntry, VectorVersionedEntry, VectorWriteOutcome,
};

pub use crate::branch::{BranchName, BranchService};
pub use crate::commit::CommitOutcome;
pub use crate::diagnostics::{
    error_code_registry_entries, error_code_registry_entry, CommitOutcomeStatus, EngineError,
    EngineErrorClass, EngineErrorStatus, EngineResult, ErrorClass, ErrorCodeRegistryEntry,
    ErrorDetail, RetryPolicy,
};
