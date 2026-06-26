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
pub mod command;
pub mod error;
pub mod error_registry;
pub mod executor;
pub mod output;
pub mod types;

pub use command::Command;
pub use error::{
    with_error_render_config, ErrorReferenceIdSource, ErrorRenderConfig, ErrorStatus,
    ExecutorError, ExecutorErrorClass, ExecutorResult, SequentialErrorReferenceIdSource,
};
pub use error_registry::{public_error_code_entries, public_error_code_entry};
pub use executor::Executor;
pub use output::Output;
pub use strata_engine_next::{
    CommitOutcomeStatus, ErrorClass, ErrorCodeRegistryEntry, ErrorDetail, RetryPolicy,
};
#[cfg(feature = "inference")]
pub use strata_inference_next::{
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
    GraphBatchItemResult, GraphBatchOperation, GraphBindingHit, GraphBindingPrimitive,
    GraphBindingTarget, GraphDirection, GraphEdgeData, GraphEdgeDataOutput, GraphEntityBinding,
    GraphInfoData, GraphNeighborHit, GraphNodeData, GraphNodeDataOutput, HistoryItem,
    JsonBatchGetItemResult, JsonBatchItemResult, JsonHistoryItem, JsonIndexDefinition,
    JsonIndexType, JsonSampleItem, JsonVersionedValue, MaybeJsonValue, MaybeJsonVersionedValue,
    MutationEffect, MutationEffectKind, PageInfo, SampleItem, ScanItem, VectorBatchGetItemResult,
    VectorBatchItemResult, VectorCollectionInfo, VectorData, VectorDistanceMetric,
    VectorFilterCondition, VectorFilterOp, VectorHistoryItem, VectorIndexArtifactSource,
    VectorIndexDiagnostics, VectorIndexQueryResult, VectorMatch, VectorMetadataFilter,
    VectorScalar, VectorVersionedData, VersionedValue, DEFAULT_BRANCH, DEFAULT_SPACE,
};
