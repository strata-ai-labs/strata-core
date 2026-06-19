//! Serializable command boundary for the rebuilt engine.

#![deny(unsafe_code)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::struct_field_names)]
#![allow(clippy::too_many_lines)]

#[cfg(feature = "arrow")]
mod arrow;
pub mod command;
pub mod error;
pub mod executor;
pub mod output;
pub mod types;

pub use command::Command;
pub use error::{ExecutorError, ExecutorErrorClass, ExecutorResult};
pub use executor::Executor;
pub use output::Output;
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
    ArrowExportPrimitive, ArrowExportResult, ArrowFileFormat, ArrowImportResult, ArrowImportTarget,
    BatchEventEntry, BatchGetItemResult, BatchItemResult, BatchJsonDeleteEntry, BatchJsonEntry,
    BatchJsonGetEntry, BatchKvEntry, BatchVectorEntry, BranchCleanupItem, BranchItem,
    BranchParentItem, BranchStatus, Bytes, EventBatchAppendItemResult, EventChainVerification,
    EventData, EventRangeDirection, EventVersionedData, GraphBatchItemResult, GraphBatchOperation,
    GraphBindingHit, GraphBindingPrimitive, GraphBindingTarget, GraphDirection, GraphEdgeData,
    GraphEdgeDataOutput, GraphEntityBinding, GraphInfoData, GraphNeighborHit, GraphNodeData,
    GraphNodeDataOutput, HistoryItem, JsonBatchGetItemResult, JsonBatchItemResult, JsonHistoryItem,
    JsonIndexDefinition, JsonIndexType, JsonSampleItem, JsonVersionedValue, SampleItem, ScanItem,
    VectorBatchGetItemResult, VectorBatchItemResult, VectorCollectionInfo, VectorData,
    VectorDistanceMetric, VectorFilterCondition, VectorFilterOp, VectorHistoryItem, VectorMatch,
    VectorMetadataFilter, VectorScalar, VectorVersionedData, VersionedValue, DEFAULT_BRANCH,
    DEFAULT_SPACE,
};
