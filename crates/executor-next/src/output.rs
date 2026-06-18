//! Serializable command outputs.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::types::{
    BatchGetItemResult, BatchItemResult, BranchCleanupItem, BranchItem, Bytes,
    EventBatchAppendItemResult, EventChainVerification, EventVersionedData, HistoryItem,
    JsonBatchGetItemResult, JsonBatchItemResult, JsonHistoryItem, JsonIndexDefinition,
    JsonSampleItem, JsonVersionedValue, SampleItem, ScanItem, VectorBatchGetItemResult,
    VectorBatchItemResult, VectorCollectionInfo, VectorHistoryItem, VectorMatch,
    VectorVersionedData, VersionedValue,
};

/// Successful executor output.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum Output {
    /// One branch summary.
    Branch(BranchItem),
    /// Branch list.
    Branches(Vec<BranchItem>),
    /// Branch deletion result.
    BranchDeleteResult {
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
    JsonValue(Option<Value>),
    /// Optional JSON value with commit metadata.
    JsonVersionedValue(Option<JsonVersionedValue>),
    /// Full version history for one key.
    VersionHistory(Option<Vec<HistoryItem>>),
    /// Full JSON document version history.
    JsonVersionHistory(Option<Vec<JsonHistoryItem>>),
    /// Key list.
    Keys(Vec<Bytes>),
    /// Paginated key list.
    KeysPage {
        /// Keys in this page.
        keys: Vec<Bytes>,
        /// True when another page is available.
        has_more: bool,
        /// Cursor for the next page.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<Bytes>,
    },
    /// Write acknowledgement.
    WriteResult {
        /// Written key.
        key: Bytes,
        /// Commit version.
        version: u64,
        /// Commit timestamp.
        timestamp: u64,
    },
    /// Delete acknowledgement.
    DeleteResult {
        /// Deleted key.
        key: Bytes,
        /// True when a visible key existed before delete.
        deleted: bool,
        /// Commit version when a delete was applied.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        version: Option<u64>,
        /// Commit timestamp when a delete was applied.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        timestamp: Option<u64>,
    },
    /// KV scan result.
    KvScanResult(Vec<ScanItem>),
    /// Positional batch write/delete results.
    BatchResults(Vec<BatchItemResult>),
    /// Positional batch read results.
    BatchGetResults(Vec<BatchGetItemResult>),
    /// Positional JSON batch write/delete results.
    JsonBatchResults(Vec<JsonBatchItemResult>),
    /// Positional JSON batch read results.
    JsonBatchGetResults(Vec<JsonBatchGetItemResult>),
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
    },
    /// Paginated JSON document key list.
    JsonListResult {
        /// Keys in this page.
        keys: Vec<String>,
        /// True when another page is available.
        has_more: bool,
        /// Cursor for the next page.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<String>,
    },
    /// Sampled JSON documents.
    JsonSampleResult {
        /// Total matching live documents.
        total_count: u64,
        /// Sampled documents.
        items: Vec<JsonSampleItem>,
    },
    /// JSON secondary index definition.
    JsonIndexDefinition(JsonIndexDefinition),
    /// JSON secondary index definitions.
    JsonIndexList(Vec<JsonIndexDefinition>),
    /// Vector write acknowledgement.
    VectorWriteResult {
        /// Collection name.
        collection: String,
        /// Vector key.
        key: String,
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
    VectorVersionHistory(Option<Vec<VectorHistoryItem>>),
    /// Vector search matches.
    VectorMatches(Vec<VectorMatch>),
    /// Paginated vector key list.
    VectorKeyPage {
        /// Keys in this page.
        keys: Vec<String>,
        /// True when another page is available.
        has_more: bool,
        /// Cursor for the next page.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<String>,
    },
    /// Vector collection list.
    VectorCollectionList(Vec<VectorCollectionInfo>),
    /// Positional vector batch write results.
    VectorBatchUpsertResults(Vec<VectorBatchItemResult>),
    /// Positional vector batch read results.
    VectorBatchGetResults(Vec<VectorBatchGetItemResult>),
    /// Positional vector batch delete results.
    VectorBatchDeleteResults(Vec<VectorBatchItemResult>),
    /// Event append acknowledgement.
    EventAppendResult {
        /// Assigned sequence.
        sequence: u64,
        /// Appended event type.
        event_type: String,
        /// Commit version.
        version: u64,
        /// Commit timestamp.
        timestamp: u64,
    },
    /// Optional event record.
    EventRecord(Option<EventVersionedData>),
    /// Event records.
    EventRecords(Vec<EventVersionedData>),
    /// Event log length.
    EventLength {
        /// Visible event count.
        count: u64,
    },
    /// Event type list.
    EventTypeList(Vec<String>),
    /// Paginated event range.
    EventRangeResult {
        /// Events in this page.
        events: Vec<EventVersionedData>,
        /// True when another page is available.
        has_more: bool,
        /// Sequence cursor for the next page.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<u64>,
    },
    /// Positional event batch append results.
    EventBatchAppendResults(Vec<EventBatchAppendItemResult>),
    /// Event hash-chain verification result.
    EventChainVerification(EventChainVerification),
}
