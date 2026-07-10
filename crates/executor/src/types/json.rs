use super::{
    batch_item_error_status, CommitReceipt, Deserialize, ErrorStatus, ExecutorError,
    MutationEffect, Serialize, Value,
};

/// JSON secondary index kind exposed through the command boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JsonIndexType {
    /// Numeric field index.
    Numeric,
    /// Exact tag/string field index.
    Tag,
    /// Lowercase text field index.
    Text,
}

/// Stored JSON value with commit metadata.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonVersionedValue {
    value: Value,
    version: u64,
    timestamp: u64,
    document_version: u64,
}

impl JsonVersionedValue {
    /// Creates a JSON versioned value.
    pub fn new(value: Value, version: u64, timestamp: u64, document_version: u64) -> Self {
        Self {
            value,
            version,
            timestamp,
            document_version,
        }
    }

    /// Returns the selected JSON value.
    pub const fn value(&self) -> &Value {
        &self.value
    }

    /// Returns the commit version.
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Returns the commit timestamp.
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }

    /// Returns the document version.
    pub const fn document_version(&self) -> u64 {
        self.document_version
    }
}

/// JSON point-read result that distinguishes absence from a stored JSON null.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MaybeJsonValue {
    found: bool,
    value: Value,
}

impl MaybeJsonValue {
    /// Creates a present JSON value result.
    pub const fn found(value: Value) -> Self {
        Self { found: true, value }
    }

    /// Creates a missing JSON value result.
    pub const fn missing() -> Self {
        Self {
            found: false,
            value: Value::Null,
        }
    }

    /// Creates a result from an optional engine value.
    pub fn from_option(value: Option<Value>) -> Self {
        match value {
            Some(value) => Self::found(value),
            None => Self::missing(),
        }
    }

    /// Returns true when the selected JSON value exists.
    pub const fn found_flag(&self) -> bool {
        self.found
    }

    /// Returns true when the selected JSON value exists.
    pub const fn is_found(&self) -> bool {
        self.found
    }

    /// Returns the selected JSON value when it exists.
    pub const fn value(&self) -> Option<&Value> {
        if self.found {
            Some(&self.value)
        } else {
            None
        }
    }

    /// Consumes the result and returns the selected JSON value when it exists.
    pub fn into_option(self) -> Option<Value> {
        if self.found {
            Some(self.value)
        } else {
            None
        }
    }
}

/// JSON versioned point-read result that distinguishes absence from a stored JSON null.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MaybeJsonVersionedValue {
    found: bool,
    #[serde(default)]
    value: Option<JsonVersionedValue>,
}

impl MaybeJsonVersionedValue {
    /// Creates a present JSON versioned value result.
    pub fn found(value: JsonVersionedValue) -> Self {
        Self {
            found: true,
            value: Some(value),
        }
    }

    /// Creates a missing JSON versioned value result.
    pub const fn missing() -> Self {
        Self {
            found: false,
            value: None,
        }
    }

    /// Creates a result from an optional engine value.
    pub fn from_option(value: Option<JsonVersionedValue>) -> Self {
        match value {
            Some(value) => Self::found(value),
            None => Self::missing(),
        }
    }

    /// Returns true when the selected JSON value exists.
    pub const fn found_flag(&self) -> bool {
        self.found
    }

    /// Returns true when the selected JSON value exists.
    pub const fn is_found(&self) -> bool {
        self.found
    }

    /// Returns the selected JSON value with version metadata when it exists.
    pub const fn value(&self) -> Option<&JsonVersionedValue> {
        if self.found {
            self.value.as_ref()
        } else {
            None
        }
    }

    /// Consumes the result and returns the selected JSON value with version metadata when it exists.
    pub fn into_option(self) -> Option<JsonVersionedValue> {
        if self.found {
            self.value
        } else {
            None
        }
    }
}

/// JSON version-history item.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonHistoryItem {
    value: Option<Value>,
    version: u64,
    timestamp: u64,
    document_version: Option<u64>,
    tombstone: bool,
}

impl JsonHistoryItem {
    /// Creates a JSON history item.
    pub fn new(
        value: Option<Value>,
        version: u64,
        timestamp: u64,
        document_version: Option<u64>,
        tombstone: bool,
    ) -> Self {
        Self {
            value,
            version,
            timestamp,
            document_version,
            tombstone,
        }
    }

    /// Returns the full document value when this row is not a tombstone.
    pub const fn value(&self) -> Option<&Value> {
        self.value.as_ref()
    }

    /// Returns the commit version.
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Returns the commit timestamp.
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }

    /// Returns the document version, when present.
    pub const fn document_version(&self) -> Option<u64> {
        self.document_version
    }

    /// Returns true when this item represents a delete.
    pub const fn is_tombstone(&self) -> bool {
        self.tombstone
    }
}

/// Positional JSON batch write/delete result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct JsonBatchItemResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    effect: Option<MutationEffect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    commit: Option<CommitReceipt>,
    document_version: Option<u64>,
    error: Option<ErrorStatus>,
}

impl JsonBatchItemResult {
    /// Creates a successful JSON batch result.
    pub const fn new(
        effect: MutationEffect,
        commit: Option<CommitReceipt>,
        document_version: Option<u64>,
    ) -> Self {
        Self {
            effect: Some(effect),
            commit,
            document_version,
            error: None,
        }
    }

    /// Creates a failed JSON batch result.
    pub fn failed(error: impl Into<String>) -> Self {
        Self::failed_status(batch_item_error_status(error))
    }

    /// Creates a failed JSON batch result from an executor error.
    pub fn failed_error(error: ExecutorError) -> Self {
        Self::failed_status(error.into_status())
    }

    /// Creates a failed JSON batch result from a public error status.
    pub fn failed_status(error: ErrorStatus) -> Self {
        Self {
            effect: None,
            commit: None,
            document_version: None,
            error: Some(error),
        }
    }

    /// Returns mutation effect facts for successful items.
    pub const fn effect(&self) -> Option<&MutationEffect> {
        self.effect.as_ref()
    }

    /// Returns true when this item applied a mutation.
    pub const fn applied(&self) -> bool {
        match self.effect {
            Some(effect) => effect.applied(),
            None => false,
        }
    }

    /// Returns the commit receipt, when this item applied a commit.
    pub const fn commit(&self) -> Option<&CommitReceipt> {
        self.commit.as_ref()
    }

    /// Returns the commit version, when present.
    pub const fn version(&self) -> Option<u64> {
        match self.commit {
            Some(commit) => Some(commit.version()),
            None => None,
        }
    }

    /// Returns the commit timestamp, when present.
    pub const fn timestamp(&self) -> Option<u64> {
        match self.commit {
            Some(commit) => Some(commit.timestamp()),
            None => None,
        }
    }

    /// Returns the document version, when present.
    pub const fn document_version(&self) -> Option<u64> {
        self.document_version
    }

    /// Returns the item error, when this item failed validation.
    pub fn error(&self) -> Option<&str> {
        self.error.as_ref().map(ErrorStatus::message)
    }

    /// Returns the structured item error status.
    pub const fn error_status(&self) -> Option<&ErrorStatus> {
        self.error.as_ref()
    }
}

/// Positional JSON batch read result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonBatchGetItemResult {
    found: bool,
    value: Value,
    version: Option<u64>,
    timestamp: Option<u64>,
    document_version: Option<u64>,
    error: Option<ErrorStatus>,
}

impl JsonBatchGetItemResult {
    /// Creates a JSON batch read result.
    pub fn new(
        value: Option<Value>,
        version: Option<u64>,
        timestamp: Option<u64>,
        document_version: Option<u64>,
    ) -> Self {
        match value {
            Some(value) => Self {
                found: true,
                value,
                version,
                timestamp,
                document_version,
                error: None,
            },
            None => Self {
                found: false,
                value: Value::Null,
                version,
                timestamp,
                document_version,
                error: None,
            },
        }
    }

    /// Creates a failed JSON batch read result.
    pub fn failed(error: impl Into<String>) -> Self {
        Self::failed_status(batch_item_error_status(error))
    }

    /// Creates a failed JSON batch read result from an executor error.
    pub fn failed_error(error: ExecutorError) -> Self {
        Self::failed_status(error.into_status())
    }

    /// Creates a failed JSON batch read result from a public error status.
    pub fn failed_status(error: ErrorStatus) -> Self {
        Self {
            found: false,
            value: Value::Null,
            version: None,
            timestamp: None,
            document_version: None,
            error: Some(error),
        }
    }

    /// Returns true when the selected JSON value exists.
    pub const fn found(&self) -> bool {
        self.found
    }

    /// Returns the selected JSON value, when present.
    pub const fn value(&self) -> Option<&Value> {
        if self.found {
            Some(&self.value)
        } else {
            None
        }
    }

    /// Returns the commit version, when present.
    pub const fn version(&self) -> Option<u64> {
        self.version
    }

    /// Returns the commit timestamp, when present.
    pub const fn timestamp(&self) -> Option<u64> {
        self.timestamp
    }

    /// Returns the document version, when present.
    pub const fn document_version(&self) -> Option<u64> {
        self.document_version
    }

    /// Returns the item error, when this item failed validation.
    pub fn error(&self) -> Option<&str> {
        self.error.as_ref().map(ErrorStatus::message)
    }

    /// Returns the structured item error status.
    pub const fn error_status(&self) -> Option<&ErrorStatus> {
        self.error.as_ref()
    }
}

/// Sampled JSON document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonSampleItem {
    key: String,
    value: Value,
    version: u64,
    timestamp: u64,
    document_version: u64,
}

impl JsonSampleItem {
    /// Creates a sampled JSON document.
    pub fn new(
        key: String,
        value: Value,
        version: u64,
        timestamp: u64,
        document_version: u64,
    ) -> Self {
        Self {
            key,
            value,
            version,
            timestamp,
            document_version,
        }
    }

    /// Returns the document key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the full document value.
    pub const fn value(&self) -> &Value {
        &self.value
    }

    /// Returns the commit version.
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Returns the commit timestamp.
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }

    /// Returns the document version.
    pub const fn document_version(&self) -> u64 {
        self.document_version
    }
}

/// JSON secondary index definition exposed through the command boundary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct JsonIndexDefinition {
    name: String,
    space: String,
    field_path: String,
    index_type: JsonIndexType,
    created_version: u64,
    created_timestamp: u64,
}

impl JsonIndexDefinition {
    /// Creates a JSON index definition.
    pub fn new(
        name: String,
        space: String,
        field_path: String,
        index_type: JsonIndexType,
        created_version: u64,
        created_timestamp: u64,
    ) -> Self {
        Self {
            name,
            space,
            field_path,
            index_type,
            created_version,
            created_timestamp,
        }
    }

    /// Returns the index name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the product space.
    pub fn space(&self) -> &str {
        &self.space
    }

    /// Returns the indexed field path.
    pub fn field_path(&self) -> &str {
        &self.field_path
    }

    /// Returns the index kind.
    pub const fn index_type(&self) -> JsonIndexType {
        self.index_type
    }

    /// Returns the creation commit version.
    pub const fn created_version(&self) -> u64 {
        self.created_version
    }

    /// Returns the creation commit timestamp.
    pub const fn created_timestamp(&self) -> u64 {
        self.created_timestamp
    }
}
