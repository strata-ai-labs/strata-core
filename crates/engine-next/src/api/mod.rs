//! Executor-facing engine API.

mod branch;
mod database;
mod event;
mod json;
mod kv;
mod options;
mod vector;

pub use branch::{
    BranchCleanupSummary, BranchCreateOutcome, BranchDeleteOutcome, BranchParentSummary,
    BranchStatus, BranchSummary,
};
pub use database::{
    CloseOutcome, Database, DatabaseOpenOutcome, DatabaseOpenSummary, DatabaseOpenTarget,
};
pub use event::{
    EventAppendOutcome, EventBatchAppendEntry, EventBatchAppendItemOutcome,
    EventBatchAppendOutcome, EventChainVerification, EventLength, EventPayload,
    EventRangeDirection, EventRangePage, EventSequence, EventService, EventType, EventTypeList,
    EventVersionedRecord,
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
pub use options::{CacheOpenOptions, DurableLocalOpenOptions};
pub use vector::{
    VectorBatchDeleteOutcome, VectorBatchGetOutcome, VectorBatchUpsertOutcome,
    VectorBulkDeleteOutcome, VectorCollectionInfo, VectorCollectionName, VectorConfig,
    VectorDeleteOutcome, VectorDistanceMetric, VectorEmbedding, VectorEntry, VectorFilter,
    VectorFilterCondition, VectorFilterOp, VectorHistory, VectorHistoryRow, VectorKey,
    VectorKeyPage, VectorMetadata, VectorMetadataPatch, VectorMetadataUpdateOutcome, VectorScalar,
    VectorSearchMatch, VectorSearchResult, VectorService, VectorUpsertEntry, VectorVersionedEntry,
    VectorWriteOutcome,
};

pub use crate::branch::{BranchName, BranchService};
pub use crate::commit::CommitOutcome;
pub use crate::diagnostics::{EngineError, EngineErrorClass, EngineResult};
