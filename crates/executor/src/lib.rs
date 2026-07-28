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
// The multi-process access policy (Host / Client / Off). Always compiled so
// every frontend can name it; the socket transport that acts on it is unix-only
// (`ipc` below).
pub mod ipc_mode;
// Multi-process IPC over a Unix domain socket. Default-on for native builds;
// the wasm consumer takes the executor with `default-features = false` and so
// never compiles the socket transport.
#[cfg(all(feature = "ipc", unix))]
pub mod ipc;
pub mod output;
mod time_compat;
pub mod types;
mod wire_json;

pub use command::Command;
pub use error::{
    with_error_render_config, ErrorReferenceIdSource, ErrorRenderConfig, ErrorStatus,
    ExecutorError, ExecutorErrorClass, ExecutorResult, SequentialErrorReferenceIdSource,
};
pub use error_registry::{public_error_code_entries, public_error_code_entry};
pub use executor::Executor;
pub use ipc_mode::IpcMode;
pub use output::{Output, RemoteOriginFrontierInfo, RemoteOriginInfo};
pub use strata_engine::{
    CommitOutcomeStatus, DurabilityMode, DurableLocalOpenOptions, ErrorClass,
    ErrorCodeRegistryEntry, ErrorDetail, RetryPolicy,
};
#[cfg(feature = "inference")]
pub use strata_inference::{
    provider_key_info as inference_provider_key_info, ProviderKeyInfo as InferenceProviderKeyInfo,
    CLOUD_PROVIDER_KEYS as INFERENCE_CLOUD_PROVIDER_KEYS,
};
#[cfg(feature = "inference")]
pub use strata_inference::{
    ChatMessage as InferenceChatMessage, ChatRequest as InferenceChatRequest,
    ChatResponse as InferenceChatResponse, EmbedInput as InferenceEmbedInput,
    EmbeddingsRequest as InferenceEmbeddingsRequest,
    EmbeddingsResponse as InferenceEmbeddingsResponse, FunctionDef as InferenceFunctionDef,
    InferenceCapability, InferenceRuntime, InferenceRuntimeConfig, InputType as InferenceInputType,
    JsonSchemaSpec as InferenceJsonSchemaSpec, LogProbs as InferenceLogProbs,
    ModelCacheStatus as InferenceModelCacheStatus, ModelConfig as InferenceModelConfig,
    ModelInfo as InferenceModelInfo, NamedToolChoice as InferenceNamedToolChoice,
    PullModelOutput as InferencePullModelOutput, RankRequest as InferenceRankRequest,
    RankResponse as InferenceRankResponse, ResponseFormat as InferenceResponseFormat,
    Role as InferenceRole, TokenLogProb as InferenceTokenLogProb, Tool as InferenceTool,
    ToolCall as InferenceToolCall, ToolCallFunction as InferenceToolCallFunction,
    ToolChoice as InferenceToolChoice, ToolChoiceFunction as InferenceToolChoiceFunction,
    ToolChoiceMode as InferenceToolChoiceMode, TopLogProb as InferenceTopLogProb,
};
pub use types::{
    AdminCapabilities, AdminConfig, AdminControlStatus, AdminDatabaseInfo, AdminDescribe,
    AdminGraph, AdminHealth, AdminHealthStatus, AdminMetrics, AdminOpenTarget, AdminPrimitives,
    AdminVectorCollection, ArrowExportPrimitive, ArrowExportResult, ArrowFileFormat,
    ArrowImportResult, ArrowImportTarget, BatchEventEntry, BatchExistsItemResult,
    BatchExistsPresence, BatchGetItemResult, BatchItem, BatchItemResult, BatchItemStatus,
    BatchJsonDeleteEntry, BatchJsonEntry, BatchJsonGetEntry, BatchKvEntry, BatchMode, BatchResult,
    BatchStatus, BatchVectorEntry, BranchCleanupItem, BranchItem, BranchParentItem, BranchStatus,
    Bytes, CommitDurability, CommitReceipt, EventBatchAppendItemResult, EventChainVerification,
    EventData, EventRangeDirection, EventVersionedData, GraphAnalyticsBudget, GraphBatchItemResult,
    GraphBatchOperation, GraphBfsData, GraphBfsEdgeData, GraphBindingHit, GraphBindingPrimitive,
    GraphBindingTarget, GraphBulkEdge, GraphBulkNode, GraphCdlpData, GraphDeletePolicy,
    GraphDirection, GraphEdgeData, GraphEdgeDataOutput, GraphEntityBinding, GraphInfoData,
    GraphLccData, GraphLinkTypeDefData, GraphLinkTypeSummaryData, GraphNeighborHit, GraphNodeData,
    GraphNodeDataOutput, GraphObjectTypeDefData, GraphObjectTypeSummaryData, GraphOntologyData,
    GraphOntologySummaryData, GraphPagerankData, GraphPropertyDef, GraphSsspData, GraphWccData,
    HistoryItem, HistoryResult, JsonBatchGetItemResult, JsonBatchItemResult, JsonHistoryItem,
    JsonIndexDefinition, JsonIndexType, JsonSampleItem, JsonVersionedValue, Maybe, MaybeJsonValue,
    MaybeJsonVersionedValue, MutationEffect, MutationEffectKind, PageInfo, SampleItem, ScanItem,
    VectorBatchGetItemResult, VectorBatchItemResult, VectorCollectionInfo, VectorData,
    VectorDistanceMetric, VectorFilterCondition, VectorFilterOp, VectorHistoryItem,
    VectorHistoryResult, VectorIndexArtifactSource, VectorIndexDiagnostics, VectorIndexQueryResult,
    VectorMatch, VectorMetadataFilter, VectorScalar, VectorVersionedData, VersionedValue,
    DEFAULT_BRANCH, DEFAULT_SPACE,
};
pub use wire_json::guard_json_integers;
