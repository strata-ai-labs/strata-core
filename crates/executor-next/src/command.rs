//! Serializable command vocabulary.

use serde::{Deserialize, Serialize};

use crate::types::{BatchKvEntry, Bytes};

/// Serializable executor command.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum Command {
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
        /// Optional item limit.
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
}

impl Command {
    /// Returns the stable command name.
    pub const fn name(&self) -> &'static str {
        match self {
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
        }
    }
}
