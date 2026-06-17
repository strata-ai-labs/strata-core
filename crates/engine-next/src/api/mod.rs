//! Executor-facing engine API.

mod branch;
mod database;
mod json;
mod kv;
mod options;

pub use branch::{
    BranchCleanupSummary, BranchCreateOutcome, BranchDeleteOutcome, BranchParentSummary,
    BranchStatus, BranchSummary,
};
pub use database::{
    CloseOutcome, Database, DatabaseOpenOutcome, DatabaseOpenSummary, DatabaseOpenTarget,
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

pub use crate::branch::{BranchName, BranchService};
pub use crate::commit::CommitOutcome;
pub use crate::diagnostics::{EngineError, EngineErrorClass, EngineResult};
