//! Serializable command vocabulary.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::types::{
    BatchJsonDeleteEntry, BatchJsonEntry, BatchJsonGetEntry, BatchKvEntry, Bytes, JsonIndexType,
};

/// Serializable executor command.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
    /// Lists active branches.
    BranchList,
    /// Reads one branch summary.
    BranchGet {
        /// Branch name.
        branch: String,
    },
    /// Creates an empty root branch.
    BranchCreate {
        /// Branch name.
        branch: String,
    },
    /// Forks a branch from the current source head.
    BranchForkCurrent {
        /// Source branch name.
        source: String,
        /// Destination branch name.
        branch: String,
    },
    /// Forks a branch from a retained source version.
    BranchForkAtVersion {
        /// Source branch name.
        source: String,
        /// Destination branch name.
        branch: String,
        /// Source version.
        version: u64,
    },
    /// Forks a branch from a retained source timestamp.
    BranchForkAtTimestamp {
        /// Source branch name.
        source: String,
        /// Destination branch name.
        branch: String,
        /// Source timestamp in microseconds.
        timestamp: u64,
    },
    /// Deletes an active branch.
    BranchDelete {
        /// Branch name.
        branch: String,
    },
    /// Writes one KV entry.
    KvPut {
        /// Target branch. Defaults to the executor handle branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Target product space. Defaults to `"default"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Key bytes.
        key: Bytes,
        /// Value bytes.
        value: Bytes,
    },
    /// Reads one KV entry.
    KvGet {
        /// Target branch. Defaults to the executor handle branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Target product space. Defaults to `"default"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Key bytes.
        key: Bytes,
        /// Optional timestamp in microseconds.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        as_of: Option<u64>,
    },
    /// Deletes one KV entry.
    KvDelete {
        /// Target branch. Defaults to the executor handle branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Target product space. Defaults to `"default"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Key bytes.
        key: Bytes,
    },
    /// Lists KV keys.
    KvList {
        /// Target branch. Defaults to the executor handle branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Target product space. Defaults to `"default"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Optional key prefix.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prefix: Option<Bytes>,
        /// Optional key cursor.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<Bytes>,
        /// Optional item limit. Defaults to 100.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<u64>,
        /// Optional timestamp in microseconds.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        as_of: Option<u64>,
    },
    /// Scans KV rows.
    KvScan {
        /// Target branch. Defaults to the executor handle branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Target product space. Defaults to `"default"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Optional inclusive start key.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        start: Option<Bytes>,
        /// Optional row limit.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<u64>,
    },
    /// Writes multiple KV entries in one engine commit.
    KvBatchPut {
        /// Target branch. Defaults to the executor handle branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Target product space. Defaults to `"default"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Entries to write.
        entries: Vec<BatchKvEntry>,
    },
    /// Reads multiple KV entries.
    KvBatchGet {
        /// Target branch. Defaults to the executor handle branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Target product space. Defaults to `"default"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Keys to read.
        keys: Vec<Bytes>,
    },
    /// Deletes multiple KV entries in one engine commit.
    KvBatchDelete {
        /// Target branch. Defaults to the executor handle branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Target product space. Defaults to `"default"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Keys to delete.
        keys: Vec<Bytes>,
    },
    /// Checks multiple keys for existence.
    KvBatchExists {
        /// Target branch. Defaults to the executor handle branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Target product space. Defaults to `"default"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Keys to check.
        keys: Vec<Bytes>,
    },
    /// Checks one key for existence.
    KvExists {
        /// Target branch. Defaults to the executor handle branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Target product space. Defaults to `"default"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Key to check.
        key: Bytes,
    },
    /// Reads full version history for one key.
    KvGetv {
        /// Target branch. Defaults to the executor handle branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Target product space. Defaults to `"default"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Key to read.
        key: Bytes,
    },
    /// Counts keys.
    KvCount {
        /// Target branch. Defaults to the executor handle branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Target product space. Defaults to `"default"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Optional key prefix.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prefix: Option<Bytes>,
    },
    /// Samples keys and values.
    KvSample {
        /// Target branch. Defaults to the executor handle branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Target product space. Defaults to `"default"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Optional key prefix.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prefix: Option<Bytes>,
        /// Optional sample count. Defaults to 10.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        count: Option<u64>,
    },
    /// Sets a JSON value at a document path, creating the document when missing.
    JsonSet {
        /// Target branch. Defaults to the executor handle branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Target product space. Defaults to `"default"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Document key.
        key: String,
        /// JSON path.
        path: String,
        /// JSON value.
        value: Value,
    },
    /// Reads a JSON value at a document path.
    JsonGet {
        /// Target branch. Defaults to the executor handle branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Target product space. Defaults to `"default"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Document key.
        key: String,
        /// JSON path.
        path: String,
        /// Optional timestamp in microseconds.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        as_of: Option<u64>,
    },
    /// Deletes a whole JSON document or one JSON path.
    JsonDelete {
        /// Target branch. Defaults to the executor handle branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Target product space. Defaults to `"default"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Document key.
        key: String,
        /// JSON path.
        path: String,
    },
    /// Reads full JSON document version history.
    JsonGetv {
        /// Target branch. Defaults to the executor handle branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Target product space. Defaults to `"default"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Document key.
        key: String,
    },
    /// Checks whether a JSON document exists.
    JsonExists {
        /// Target branch. Defaults to the executor handle branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Target product space. Defaults to `"default"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Document key.
        key: String,
    },
    /// Sets multiple JSON values in one engine commit.
    JsonBatchSet {
        /// Target branch. Defaults to the executor handle branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Target product space. Defaults to `"default"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Entries to set.
        entries: Vec<BatchJsonEntry>,
    },
    /// Reads multiple JSON values.
    JsonBatchGet {
        /// Target branch. Defaults to the executor handle branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Target product space. Defaults to `"default"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Entries to read.
        entries: Vec<BatchJsonGetEntry>,
    },
    /// Deletes multiple JSON documents or paths.
    JsonBatchDelete {
        /// Target branch. Defaults to the executor handle branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Target product space. Defaults to `"default"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Entries to delete.
        entries: Vec<BatchJsonDeleteEntry>,
    },
    /// Lists JSON document keys.
    JsonList {
        /// Target branch. Defaults to the executor handle branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Target product space. Defaults to `"default"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Optional document key prefix.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prefix: Option<String>,
        /// Optional document key cursor.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        cursor: Option<String>,
        /// Optional item limit.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        limit: Option<u64>,
        /// Optional timestamp in microseconds.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        as_of: Option<u64>,
    },
    /// Counts JSON documents.
    JsonCount {
        /// Target branch. Defaults to the executor handle branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Target product space. Defaults to `"default"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Optional document key prefix.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prefix: Option<String>,
    },
    /// Samples JSON documents.
    JsonSample {
        /// Target branch. Defaults to the executor handle branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Target product space. Defaults to `"default"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Optional document key prefix.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        prefix: Option<String>,
        /// Optional sample count. Defaults to 10.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        count: Option<u64>,
    },
    /// Creates a JSON secondary index.
    JsonCreateIndex {
        /// Target branch. Defaults to the executor handle branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Target product space. Defaults to `"default"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Index name.
        name: String,
        /// Indexed field path.
        field_path: String,
        /// Index kind.
        index_type: JsonIndexType,
    },
    /// Drops a JSON secondary index.
    JsonDropIndex {
        /// Target branch. Defaults to the executor handle branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Target product space. Defaults to `"default"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
        /// Index name.
        name: String,
    },
    /// Lists JSON secondary indexes.
    JsonListIndexes {
        /// Target branch. Defaults to the executor handle branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        branch: Option<String>,
        /// Target product space. Defaults to `"default"`.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        space: Option<String>,
    },
}

impl Command {
    /// Returns the stable command name.
    pub const fn name(&self) -> &'static str {
        match self {
            Self::BranchList => "branch_list",
            Self::BranchGet { .. } => "branch_get",
            Self::BranchCreate { .. } => "branch_create",
            Self::BranchForkCurrent { .. } => "branch_fork_current",
            Self::BranchForkAtVersion { .. } => "branch_fork_at_version",
            Self::BranchForkAtTimestamp { .. } => "branch_fork_at_timestamp",
            Self::BranchDelete { .. } => "branch_delete",
            Self::KvPut { .. } => "kv_put",
            Self::KvGet { .. } => "kv_get",
            Self::KvDelete { .. } => "kv_delete",
            Self::KvList { .. } => "kv_list",
            Self::KvScan { .. } => "kv_scan",
            Self::KvBatchPut { .. } => "kv_batch_put",
            Self::KvBatchGet { .. } => "kv_batch_get",
            Self::KvBatchDelete { .. } => "kv_batch_delete",
            Self::KvBatchExists { .. } => "kv_batch_exists",
            Self::KvExists { .. } => "kv_exists",
            Self::KvGetv { .. } => "kv_getv",
            Self::KvCount { .. } => "kv_count",
            Self::KvSample { .. } => "kv_sample",
            Self::JsonSet { .. } => "json_set",
            Self::JsonGet { .. } => "json_get",
            Self::JsonDelete { .. } => "json_delete",
            Self::JsonGetv { .. } => "json_getv",
            Self::JsonExists { .. } => "json_exists",
            Self::JsonBatchSet { .. } => "json_batch_set",
            Self::JsonBatchGet { .. } => "json_batch_get",
            Self::JsonBatchDelete { .. } => "json_batch_delete",
            Self::JsonList { .. } => "json_list",
            Self::JsonCount { .. } => "json_count",
            Self::JsonSample { .. } => "json_sample",
            Self::JsonCreateIndex { .. } => "json_create_index",
            Self::JsonDropIndex { .. } => "json_drop_index",
            Self::JsonListIndexes { .. } => "json_list_indexes",
        }
    }
}
