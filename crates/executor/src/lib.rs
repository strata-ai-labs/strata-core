//! Serializable command boundary for the rebuilt engine.

#![deny(unsafe_code)]
#![allow(clippy::missing_const_for_fn)]
// `ExecutorResult<T>` is the stable serialized command-boundary API; boxing
// `ExecutorError` would be a deliberate contract change.
#![allow(clippy::result_large_err)]
#![allow(clippy::struct_field_names)]
#![allow(clippy::too_many_lines)]

#[cfg(feature = "arrow")]
mod arrow;
pub mod cli_metadata;
pub mod command;
pub mod error;
pub mod error_registry;
pub mod executor;
#[cfg(feature = "idl-tooling")]
#[doc(hidden)]
pub mod idl_tooling;
pub mod output;
mod time_compat;
pub mod types;

pub use command::Command;
pub use error::{
    with_error_render_config, ErrorReferenceIdSource, ErrorRenderConfig, ErrorStatus,
    ExecutorError, ExecutorErrorClass, ExecutorResult, SequentialErrorReferenceIdSource,
};
pub use error_registry::{public_error_code_entries, public_error_code_entry};
pub use executor::Executor;
pub use output::{Output, RemoteOriginFrontierInfo, RemoteOriginInfo};
pub use strata_engine::{
    CommitOutcomeStatus, ErrorClass, ErrorCodeRegistryEntry, ErrorDetail, RetryPolicy,
};
#[cfg(feature = "inference")]
pub use strata_inference::{
    EmbedRequest as InferenceEmbedRequest, EmbedResponse as InferenceEmbedResponse,
    GenerateRequest as InferenceGenerateRequest, GenerateResponse as InferenceGenerateResponse,
    InferenceCapability, InferenceRuntime, InferenceRuntimeConfig,
    ModelCacheStatus as InferenceModelCacheStatus, ModelInfo as InferenceModelInfo,
    PullModelOutput as InferencePullModelOutput, RankRequest as InferenceRankRequest,
    RankResponse as InferenceRankResponse,
};
pub use types::{
    AdminCapabilities, AdminConfig, AdminControlStatus, AdminDatabaseInfo, AdminDescribe,
    AdminGraph, AdminHealth, AdminHealthStatus, AdminMetrics, AdminOpenTarget, AdminPrimitives,
    AdminVectorCollection, ArrowExportPrimitive, ArrowExportResult, ArrowFileFormat,
    ArrowImportResult, ArrowImportTarget, BatchEventEntry, BatchGetItemResult, BatchItem,
    BatchItemResult, BatchItemStatus, BatchJsonDeleteEntry, BatchJsonEntry, BatchJsonGetEntry,
    BatchKvEntry, BatchMode, BatchResult, BatchStatus, BatchVectorEntry, BranchCleanupItem,
    BranchItem, BranchParentItem, BranchStatus, Bytes, CommitReceipt, EventBatchAppendItemResult,
    EventChainVerification, EventData, EventRangeDirection, EventVersionedData,
    GraphAnalyticsBudget, GraphBatchItemResult, GraphBatchOperation, GraphBfsData,
    GraphBfsEdgeData, GraphBindingHit, GraphBindingPrimitive, GraphBindingTarget, GraphBulkEdge,
    GraphBulkNode, GraphCdlpData, GraphDeletePolicy, GraphDirection, GraphEdgeData,
    GraphEdgeDataOutput, GraphEntityBinding, GraphInfoData, GraphLccData, GraphLinkTypeDefData,
    GraphLinkTypeSummaryData, GraphNeighborHit, GraphNodeData, GraphNodeDataOutput,
    GraphObjectTypeDefData, GraphObjectTypeSummaryData, GraphOntologyData,
    GraphOntologySummaryData, GraphPagerankData, GraphPropertyDef, GraphSsspData, GraphWccData,
    HistoryItem, HistoryResult, JsonBatchGetItemResult, JsonBatchItemResult, JsonHistoryItem,
    JsonIndexDefinition, JsonIndexType, JsonSampleItem, JsonVersionedValue, MaybeJsonValue,
    MaybeJsonVersionedValue, MutationEffect, MutationEffectKind, PageInfo, SampleItem, ScanItem,
    VectorBatchGetItemResult, VectorBatchItemResult, VectorCollectionInfo, VectorData,
    VectorDistanceMetric, VectorFilterCondition, VectorFilterOp, VectorHistoryItem,
    VectorHistoryResult, VectorIndexArtifactSource, VectorIndexDiagnostics, VectorIndexQueryResult,
    VectorMatch, VectorMetadataFilter, VectorScalar, VectorVersionedData, VersionedValue,
    DEFAULT_BRANCH, DEFAULT_SPACE,
};
