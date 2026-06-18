//! Event outcome types.

use strata_core_next::{CommitVersion, Timestamp};

use crate::commit::CommitOutcome;

use super::{EventPayload, EventSequence, EventType};

/// Event record with product facts.
#[derive(Clone, Debug, PartialEq)]
pub struct EventRecord {
    sequence: EventSequence,
    event_type: EventType,
    payload: EventPayload,
    timestamp: Timestamp,
    previous_hash: [u8; 32],
    hash: [u8; 32],
}

impl EventRecord {
    pub(crate) const fn new(
        sequence: EventSequence,
        event_type: EventType,
        payload: EventPayload,
        timestamp: Timestamp,
        previous_hash: [u8; 32],
        hash: [u8; 32],
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

    #[must_use]
    /// Returns the event sequence.
    pub const fn sequence(&self) -> EventSequence {
        self.sequence
    }

    #[must_use]
    /// Returns the event type.
    pub const fn event_type(&self) -> &EventType {
        &self.event_type
    }

    #[must_use]
    /// Returns the event payload.
    pub const fn payload(&self) -> &EventPayload {
        &self.payload
    }

    #[must_use]
    /// Returns the event append timestamp.
    pub const fn timestamp(&self) -> Timestamp {
        self.timestamp
    }

    #[must_use]
    /// Returns the previous event hash.
    pub const fn previous_hash(&self) -> [u8; 32] {
        self.previous_hash
    }

    #[must_use]
    /// Returns this event hash.
    pub const fn hash(&self) -> [u8; 32] {
        self.hash
    }
}

/// Event record with commit metadata.
#[derive(Clone, Debug, PartialEq)]
pub struct EventVersionedRecord {
    record: EventRecord,
    version: CommitVersion,
    commit_timestamp: Timestamp,
}

impl EventVersionedRecord {
    pub(crate) const fn new(
        record: EventRecord,
        version: CommitVersion,
        commit_timestamp: Timestamp,
    ) -> Self {
        Self {
            record,
            version,
            commit_timestamp,
        }
    }

    #[must_use]
    /// Returns the event record.
    pub const fn record(&self) -> &EventRecord {
        &self.record
    }

    #[must_use]
    /// Returns the event sequence.
    pub const fn sequence(&self) -> EventSequence {
        self.record.sequence()
    }

    #[must_use]
    /// Returns the event type.
    pub const fn event_type(&self) -> &EventType {
        self.record.event_type()
    }

    #[must_use]
    /// Returns the event payload.
    pub const fn payload(&self) -> &EventPayload {
        self.record.payload()
    }

    #[must_use]
    /// Returns the event append timestamp.
    pub const fn timestamp(&self) -> Timestamp {
        self.record.timestamp()
    }

    #[must_use]
    /// Returns the previous event hash.
    pub const fn previous_hash(&self) -> [u8; 32] {
        self.record.previous_hash()
    }

    #[must_use]
    /// Returns this event hash.
    pub const fn hash(&self) -> [u8; 32] {
        self.record.hash()
    }

    #[must_use]
    /// Returns the storage commit version.
    pub const fn version(&self) -> CommitVersion {
        self.version
    }

    #[must_use]
    /// Returns the storage commit timestamp.
    pub const fn commit_timestamp(&self) -> Timestamp {
        self.commit_timestamp
    }
}

/// Event append outcome.
#[derive(Clone, Debug, PartialEq)]
pub struct EventAppendOutcome {
    sequence: EventSequence,
    event_type: EventType,
    commit: CommitOutcome,
}

impl EventAppendOutcome {
    pub(crate) const fn new(
        sequence: EventSequence,
        event_type: EventType,
        commit: CommitOutcome,
    ) -> Self {
        Self {
            sequence,
            event_type,
            commit,
        }
    }

    #[must_use]
    /// Returns the assigned sequence.
    pub const fn sequence(&self) -> EventSequence {
        self.sequence
    }

    #[must_use]
    /// Returns the appended event type.
    pub const fn event_type(&self) -> &EventType {
        &self.event_type
    }

    #[must_use]
    /// Returns the commit outcome.
    pub const fn commit(&self) -> CommitOutcome {
        self.commit
    }
}

/// Positional batch append item outcome.
#[derive(Clone, Debug, PartialEq)]
pub struct EventBatchAppendItemOutcome {
    sequence: Option<EventSequence>,
    event_type: Option<EventType>,
    commit_version: Option<CommitVersion>,
    commit_timestamp: Option<Timestamp>,
    error: Option<String>,
}

impl EventBatchAppendItemOutcome {
    pub(crate) const fn success(
        sequence: EventSequence,
        event_type: EventType,
        commit: CommitOutcome,
    ) -> Self {
        Self {
            sequence: Some(sequence),
            event_type: Some(event_type),
            commit_version: Some(commit.version()),
            commit_timestamp: Some(commit.timestamp()),
            error: None,
        }
    }

    pub(crate) fn failure(error: impl Into<String>) -> Self {
        Self {
            sequence: None,
            event_type: None,
            commit_version: None,
            commit_timestamp: None,
            error: Some(error.into()),
        }
    }

    #[must_use]
    /// Returns the assigned sequence for successful items.
    pub const fn sequence(&self) -> Option<EventSequence> {
        self.sequence
    }

    #[must_use]
    /// Returns the event type for successful items.
    pub const fn event_type(&self) -> Option<&EventType> {
        self.event_type.as_ref()
    }

    #[must_use]
    /// Returns the commit version for successful items.
    pub const fn commit_version(&self) -> Option<CommitVersion> {
        self.commit_version
    }

    #[must_use]
    /// Returns the commit timestamp for successful items.
    pub const fn commit_timestamp(&self) -> Option<Timestamp> {
        self.commit_timestamp
    }

    #[must_use]
    /// Returns the item error for failed items.
    pub fn error_message(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

/// Batch append outcome.
#[derive(Clone, Debug, PartialEq)]
pub struct EventBatchAppendOutcome {
    items: Vec<EventBatchAppendItemOutcome>,
    commit: Option<CommitOutcome>,
}

impl EventBatchAppendOutcome {
    pub(crate) const fn new(
        items: Vec<EventBatchAppendItemOutcome>,
        commit: Option<CommitOutcome>,
    ) -> Self {
        Self { items, commit }
    }

    #[must_use]
    /// Returns positional item outcomes.
    pub fn items(&self) -> &[EventBatchAppendItemOutcome] {
        &self.items
    }

    #[must_use]
    /// Returns the shared commit outcome when valid entries were committed.
    pub const fn commit(&self) -> Option<CommitOutcome> {
        self.commit
    }
}

/// Event range page.
#[derive(Clone, Debug, PartialEq)]
pub struct EventRangePage {
    events: Vec<EventVersionedRecord>,
    has_more: bool,
    cursor: Option<EventSequence>,
}

impl EventRangePage {
    pub(crate) const fn new(
        events: Vec<EventVersionedRecord>,
        has_more: bool,
        cursor: Option<EventSequence>,
    ) -> Self {
        Self {
            events,
            has_more,
            cursor,
        }
    }

    #[must_use]
    /// Returns page events.
    pub fn events(&self) -> &[EventVersionedRecord] {
        &self.events
    }

    #[must_use]
    /// Returns true when more results were available.
    pub const fn has_more(&self) -> bool {
        self.has_more
    }

    #[must_use]
    /// Returns a sequence continuation hint.
    pub const fn cursor(&self) -> Option<EventSequence> {
        self.cursor
    }
}

/// Event type list.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventTypeList {
    event_types: Vec<EventType>,
}

impl EventTypeList {
    pub(crate) const fn new(event_types: Vec<EventType>) -> Self {
        Self { event_types }
    }

    #[must_use]
    /// Returns event types in deterministic order.
    pub fn event_types(&self) -> &[EventType] {
        &self.event_types
    }
}

/// Event log length.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct EventLength {
    count: u64,
}

impl EventLength {
    pub(crate) const fn new(count: u64) -> Self {
        Self { count }
    }

    #[must_use]
    /// Returns the number of visible events.
    pub const fn count(self) -> u64 {
        self.count
    }
}

/// Hash-chain verification outcome.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EventChainVerification {
    valid: bool,
    length: u64,
    first_invalid: Option<EventSequence>,
    error: Option<String>,
}

impl EventChainVerification {
    pub(crate) const fn valid(length: u64) -> Self {
        Self {
            valid: true,
            length,
            first_invalid: None,
            error: None,
        }
    }

    pub(crate) fn invalid(
        length: u64,
        first_invalid: EventSequence,
        error: impl Into<String>,
    ) -> Self {
        Self {
            valid: false,
            length,
            first_invalid: Some(first_invalid),
            error: Some(error.into()),
        }
    }

    #[must_use]
    /// Returns true when the visible log is dense and hash-linked.
    pub const fn is_valid(&self) -> bool {
        self.valid
    }

    #[must_use]
    /// Returns the checked log length.
    pub const fn length(&self) -> u64 {
        self.length
    }

    #[must_use]
    /// Returns the first invalid sequence, if any.
    pub const fn first_invalid(&self) -> Option<EventSequence> {
        self.first_invalid
    }

    #[must_use]
    /// Returns the verification error, if any.
    pub fn error_message(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

pub(crate) fn event_record_from_envelope(
    envelope: &crate::data::event::EventRecordEnvelope,
    version: CommitVersion,
    commit_timestamp: Timestamp,
) -> EventVersionedRecord {
    let record = EventRecord::new(
        envelope.sequence(),
        envelope.event_type().clone(),
        envelope.payload().clone(),
        envelope.timestamp(),
        envelope.previous_hash(),
        envelope.hash(),
    );
    EventVersionedRecord::new(record, version, commit_timestamp)
}
