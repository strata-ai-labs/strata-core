use super::{
    commit_receipt, BatchEventEntry, CommitOutcome, CommitVersion, EngineEventAppendOutcome,
    EngineEventBatchAppendEntry, EngineEventBatchAppendItemOutcome, EngineEventChainVerification,
    EngineEventPayload, EngineEventRangeDirection, EngineEventRangePage, EngineEventSequence,
    EngineEventType, EngineEventVersionedRecord, EventBatchAppendItemResult, EventData,
    EventRangeDirection, EventVersionedData, ExecutorError, ExecutorResult, MutationEffect, Output,
    OutputEventChainVerification, PageInfo, Timestamp,
};

pub(super) fn engine_event_type(event_type: String) -> ExecutorResult<EngineEventType> {
    EngineEventType::new(event_type).map_err(ExecutorError::from)
}

pub(super) fn optional_engine_event_type(
    event_type: Option<String>,
) -> ExecutorResult<Option<EngineEventType>> {
    event_type.map(engine_event_type).transpose()
}

pub(super) fn event_payload(payload: serde_json::Value) -> ExecutorResult<EngineEventPayload> {
    EngineEventPayload::new(payload).map_err(ExecutorError::from)
}

pub(super) const fn event_sequence(sequence: u64) -> EngineEventSequence {
    EngineEventSequence::new(sequence)
}

pub(super) fn event_batch_entry(entry: BatchEventEntry) -> EngineEventBatchAppendEntry {
    let (event_type, payload) = entry.into_parts();
    EngineEventBatchAppendEntry::from_raw(event_type, payload)
}

pub(super) const fn engine_event_direction(
    direction: EventRangeDirection,
) -> EngineEventRangeDirection {
    match direction {
        EventRangeDirection::Forward => EngineEventRangeDirection::Forward,
        EventRangeDirection::Reverse => EngineEventRangeDirection::Reverse,
    }
}

pub(super) fn event_append_output(outcome: &EngineEventAppendOutcome) -> Output {
    let commit = outcome.commit();
    Output::EventAppendResult {
        sequence: outcome.sequence().as_u64(),
        event_type: outcome.event_type().as_str().to_owned(),
        effect: MutationEffect::created(),
        commit: commit_receipt(commit),
        version: commit.version().as_u64(),
        timestamp: commit.timestamp().as_micros(),
    }
}

pub(super) fn event_records(records: &[EngineEventVersionedRecord]) -> Vec<EventVersionedData> {
    records.iter().map(event_versioned_data).collect()
}

pub(super) fn event_versioned_data(record: &EngineEventVersionedRecord) -> EventVersionedData {
    EventVersionedData::new(
        event_data(record),
        record.version().as_u64(),
        record.commit_timestamp().as_micros(),
    )
}

pub(super) fn event_data(record: &EngineEventVersionedRecord) -> EventData {
    EventData::new(
        record.sequence().as_u64(),
        record.event_type().as_str().to_owned(),
        record.payload().as_inner().clone(),
        record.timestamp().as_micros(),
        hash_hex(record.previous_hash()),
        hash_hex(record.hash()),
    )
}

pub(super) fn hash_hex(hash: [u8; 32]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(hash.len().saturating_mul(2));
    for byte in hash {
        output.push(char::from(HEX[usize::from(byte >> 4)]));
        output.push(char::from(HEX[usize::from(byte & 0x0f)]));
    }
    output
}

pub(super) fn event_range_output(page: &EngineEventRangePage) -> Output {
    Output::EventRangeResult {
        items: event_records(page.events()),
        page: PageInfo::new(
            page.has_more(),
            page.cursor().map(EngineEventSequence::as_u64),
        ),
    }
}

pub(super) fn event_batch_append_item_result(
    item: &EngineEventBatchAppendItemOutcome,
    batch_commit: Option<CommitOutcome>,
) -> EventBatchAppendItemResult {
    if let Some(error) = item.error_status() {
        return EventBatchAppendItemResult::failed_engine_status(error);
    }
    if let Some(error) = item.error_message() {
        return EventBatchAppendItemResult::failed(error);
    }
    let commit = item
        .sequence()
        .and_then(|_| batch_commit.map(commit_receipt));
    EventBatchAppendItemResult::new_with_effect(
        item.sequence().map(EngineEventSequence::as_u64),
        item.event_type()
            .map(|event_type| event_type.as_str().to_owned()),
        item.sequence().map(|_| MutationEffect::created()),
        commit,
        item.commit_version().map(CommitVersion::as_u64),
        item.commit_timestamp().map(Timestamp::as_micros),
    )
}

pub(super) fn event_chain_verification(
    verification: &EngineEventChainVerification,
) -> OutputEventChainVerification {
    OutputEventChainVerification::new(
        verification.is_valid(),
        verification.length(),
        verification
            .first_invalid()
            .map(EngineEventSequence::as_u64),
        verification.error_message().map(str::to_owned),
    )
}
