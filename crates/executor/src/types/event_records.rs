use super::{
    batch_item_error_status, engine_error_status, CommitReceipt, Deserialize, EngineErrorStatus,
    ErrorStatus, ExecutorError, MutationEffect, Serialize, Value,
};

/// Event record payload and chain facts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
pub struct EventData {
    sequence: u64,
    event_type: String,
    payload: Value,
    timestamp: u64,
    previous_hash: String,
    hash: String,
}

impl EventData {
    /// Creates an event record.
    pub fn new(
        sequence: u64,
        event_type: String,
        payload: Value,
        timestamp: u64,
        previous_hash: String,
        hash: String,
    ) -> Self {
        Self {
            sequence,
            event_type,
            payload,
            timestamp,
            previous_hash,
            hash,
        }
    }

    /// Returns the event sequence.
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Returns the event type.
    pub fn event_type(&self) -> &str {
        &self.event_type
    }

    /// Returns the event payload.
    pub const fn payload(&self) -> &Value {
        &self.payload
    }

    /// Returns the event append timestamp.
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }

    /// Returns the previous event hash as lowercase hex.
    pub fn previous_hash(&self) -> &str {
        &self.previous_hash
    }

    /// Returns this event hash as lowercase hex.
    pub fn hash(&self) -> &str {
        &self.hash
    }
}

/// Event record with commit metadata.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
pub struct EventVersionedData {
    event: EventData,
    version: u64,
    timestamp: u64,
}

impl EventVersionedData {
    /// Creates a versioned event record.
    pub fn new(event: EventData, version: u64, timestamp: u64) -> Self {
        Self {
            event,
            version,
            timestamp,
        }
    }

    /// Returns the event record.
    pub const fn event(&self) -> &EventData {
        &self.event
    }

    /// Returns the commit version.
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Returns the commit timestamp.
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }
}

/// Positional event batch append result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
pub struct EventBatchAppendItemResult {
    sequence: Option<u64>,
    event_type: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    effect: Option<MutationEffect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    commit: Option<CommitReceipt>,
    version: Option<u64>,
    timestamp: Option<u64>,
    error: Option<ErrorStatus>,
}

impl EventBatchAppendItemResult {
    /// Creates an event batch append result.
    pub fn new(
        sequence: Option<u64>,
        event_type: Option<String>,
        version: Option<u64>,
        timestamp: Option<u64>,
    ) -> Self {
        Self {
            sequence,
            event_type,
            effect: None,
            commit: None,
            version,
            timestamp,
            error: None,
        }
    }

    /// Creates an event batch append result with shared mutation facts.
    pub fn new_with_effect(
        sequence: Option<u64>,
        event_type: Option<String>,
        effect: Option<MutationEffect>,
        commit: Option<CommitReceipt>,
        version: Option<u64>,
        timestamp: Option<u64>,
    ) -> Self {
        Self {
            sequence,
            event_type,
            effect,
            commit,
            version,
            timestamp,
            error: None,
        }
    }

    /// Creates a failed event batch append result.
    pub fn failed(error: impl Into<String>) -> Self {
        Self::failed_status(batch_item_error_status(error))
    }

    /// Creates a failed event batch append result from an executor error.
    pub fn failed_error(error: ExecutorError) -> Self {
        Self::failed_status(error.into_status())
    }

    /// Creates a failed event batch append result from an engine status.
    pub fn failed_engine_status(error: &EngineErrorStatus) -> Self {
        Self::failed_status(engine_error_status(error))
    }

    /// Creates a failed event batch append result from a public error status.
    pub fn failed_status(error: ErrorStatus) -> Self {
        Self {
            sequence: None,
            event_type: None,
            effect: None,
            commit: None,
            version: None,
            timestamp: None,
            error: Some(error),
        }
    }

    /// Returns the assigned sequence for successful items.
    pub const fn sequence(&self) -> Option<u64> {
        self.sequence
    }

    /// Returns the event type for successful items.
    pub fn event_type(&self) -> Option<&str> {
        self.event_type.as_deref()
    }

    /// Returns mutation effect facts for successful items.
    pub const fn effect(&self) -> Option<&MutationEffect> {
        self.effect.as_ref()
    }

    /// Returns commit receipt when this item applied a mutation.
    pub const fn commit(&self) -> Option<&CommitReceipt> {
        self.commit.as_ref()
    }

    /// Returns the commit version for successful items.
    pub const fn version(&self) -> Option<u64> {
        self.version
    }

    /// Returns the commit timestamp for successful items.
    pub const fn timestamp(&self) -> Option<u64> {
        self.timestamp
    }

    /// Returns the item error when validation failed.
    pub fn error(&self) -> Option<&str> {
        self.error.as_ref().map(ErrorStatus::message)
    }

    /// Returns the structured item error status.
    pub const fn error_status(&self) -> Option<&ErrorStatus> {
        self.error.as_ref()
    }
}

/// Event hash-chain verification result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
pub struct EventChainVerification {
    #[serde(rename = "valid", alias = "is_valid")]
    is_valid: bool,
    length: u64,
    first_invalid: Option<u64>,
    error: Option<String>,
}

impl EventChainVerification {
    /// Creates an event chain verification result.
    pub fn new(
        is_valid: bool,
        length: u64,
        first_invalid: Option<u64>,
        error: Option<String>,
    ) -> Self {
        Self {
            is_valid,
            length,
            first_invalid,
            error,
        }
    }

    /// Returns true when the visible event log is dense and hash-linked.
    pub const fn is_valid(&self) -> bool {
        self.is_valid
    }

    /// Returns the checked event count.
    pub const fn length(&self) -> u64 {
        self.length
    }

    /// Returns the first invalid sequence, if any.
    pub const fn first_invalid(&self) -> Option<u64> {
        self.first_invalid
    }

    /// Returns the verification error, if any.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}
