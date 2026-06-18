//! Serializable command outputs.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::types::{
    BatchGetItemResult, BatchItemResult, BranchCleanupItem, BranchItem, Bytes, HistoryItem,
    JsonBatchGetItemResult, JsonBatchItemResult, JsonHistoryItem, JsonIndexDefinition,
    JsonSampleItem, JsonVersionedValue, SampleItem, ScanItem, VersionedValue,
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
}
