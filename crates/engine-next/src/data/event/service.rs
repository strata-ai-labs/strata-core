//! Event service.

use std::collections::BTreeSet;
use std::time::{SystemTime, UNIX_EPOCH};

use strata_core_next::Timestamp;

use crate::branch::catalog::BranchCatalogRecord;
use crate::branch::BranchName;
use crate::commit::CommitOutcome;
use crate::control::ControlPlane;
use crate::data::kv::ProductSpace;
use crate::diagnostics::{EngineError, EngineResult};
use crate::persistence::{
    decode_event_key_sequence, encode_event_key, encode_event_meta_key, encode_event_space_prefix,
    encode_event_type_index_key, CommitPlan, PersistenceReadRow, ReadSelector, RowAddress,
    RowClass, RowMutation, StoragePersistence,
};

use super::{
    compute_event_hash, decode_event_metadata, decode_event_record, encode_event_metadata,
    encode_event_record, EventAppendOutcome, EventBatchAppendEntry, EventBatchAppendItemOutcome,
    EventBatchAppendOutcome, EventChainVerification, EventLength, EventLogMetadata,
    EventRangeDirection, EventRangePage, EventRecordEnvelope, EventSequence, EventType,
    EventTypeList, EventVersionedRecord,
};

const EVENT_RANGE_RAW_PAGE_MAX: usize = 4096;
const TYPE_INDEX_VALUE: &[u8] = b"\x01";

/// Service for event log operations.
pub struct EventService<'a> {
    persistence: &'a mut StoragePersistence,
    control: &'a mut ControlPlane,
    branch: BranchName,
    space: ProductSpace,
}

impl<'a> EventService<'a> {
    pub(crate) const fn new(
        persistence: &'a mut StoragePersistence,
        control: &'a mut ControlPlane,
        branch: BranchName,
        space: ProductSpace,
    ) -> Self {
        Self {
            persistence,
            control,
            branch,
            space,
        }
    }

    /// Appends one event to the log.
    pub fn append(
        &mut self,
        event_type: EventType,
        payload: super::EventPayload,
    ) -> EngineResult<EventAppendOutcome> {
        let outcome =
            self.batch_append([EventBatchAppendEntry::new(event_type.clone(), payload)])?;
        let item = outcome.items().first().expect("one append item");
        if let Some(error) = item.error_message() {
            return Err(EngineError::invalid_input(
                "invalid_argument.engine.event_append",
                error,
            ));
        }
        Ok(EventAppendOutcome::new(
            item.sequence().expect("sequence assigned"),
            event_type,
            outcome.commit().expect("commit exists"),
        ))
    }

    /// Appends multiple events in one commit.
    pub fn batch_append<I>(&mut self, entries: I) -> EngineResult<EventBatchAppendOutcome>
    where
        I: IntoIterator<Item = EventBatchAppendEntry>,
    {
        let entries = entries.into_iter().collect::<Vec<_>>();
        let record = self.branch_record()?;
        if entries.is_empty() {
            return Ok(EventBatchAppendOutcome::new(Vec::new(), None));
        }
        let mut items = Vec::with_capacity(entries.len());
        let mut valid_entries = Vec::new();
        for entry in &entries {
            match entry.validate() {
                Ok((event_type, payload)) => {
                    valid_entries.push((items.len(), event_type, payload));
                    items.push(None);
                }
                Err(error) => {
                    items.push(Some(EventBatchAppendItemOutcome::failure(&error)));
                }
            }
        }
        if valid_entries.is_empty() {
            return Ok(EventBatchAppendOutcome::new(
                finalize_batch_items(items),
                None,
            ));
        }
        let mut metadata = self.read_metadata(&record, ReadSelector::Latest)?;
        let mut mutations =
            Vec::with_capacity(valid_entries.len().saturating_mul(2).saturating_add(1));
        let mut appended = Vec::with_capacity(valid_entries.len());
        for (position, event_type, payload) in valid_entries {
            let sequence = metadata.next_sequence();
            let timestamp = next_event_timestamp(&metadata);
            let previous_hash = metadata.head_hash();
            let hash = compute_event_hash(
                sequence,
                &event_type,
                &payload,
                timestamp.as_micros(),
                &previous_hash,
            )?;
            let event = EventRecordEnvelope::new(
                EventSequence::new(sequence),
                event_type,
                payload,
                timestamp,
                previous_hash,
                hash,
            );
            metadata.push(&event);
            mutations.push(RowMutation::put(
                self.event_address(&record, event.sequence()),
                encode_event_record(&event)?,
            ));
            mutations.push(RowMutation::put(
                self.type_index_address(&record, event.event_type(), event.sequence()),
                TYPE_INDEX_VALUE.to_vec(),
            ));
            appended.push((position, event));
        }
        mutations.push(RowMutation::put(
            self.metadata_address(&record),
            encode_event_metadata(&metadata)?,
        ));
        let commit = self.commit_batch(&record, mutations)?;
        for (position, event) in appended {
            items[position] = Some(EventBatchAppendItemOutcome::success(
                event.sequence(),
                event.event_type().clone(),
                commit,
            ));
        }
        Ok(EventBatchAppendOutcome::new(
            finalize_batch_items(items),
            Some(commit),
        ))
    }

    /// Reads one latest event by sequence.
    pub fn get(&mut self, sequence: EventSequence) -> EngineResult<Option<EventVersionedRecord>> {
        self.get_with_selector(sequence, ReadSelector::Latest)
    }

    /// Reads one event visible at a timestamp.
    pub fn get_at(
        &mut self,
        sequence: EventSequence,
        timestamp: Timestamp,
    ) -> EngineResult<Option<EventVersionedRecord>> {
        Ok(self
            .get_with_selector(sequence, ReadSelector::Latest)?
            .filter(|event| event.timestamp() <= timestamp))
    }

    /// Returns true if an event sequence exists.
    pub fn exists(&mut self, sequence: EventSequence) -> EngineResult<bool> {
        Ok(self.get(sequence)?.is_some())
    }

    /// Returns latest log length.
    pub fn len(&mut self) -> EngineResult<EventLength> {
        let record = self.branch_record()?;
        Ok(EventLength::new(self.latest_event_count(&record)?))
    }

    /// Returns log length visible at a timestamp.
    pub fn len_at(&mut self, timestamp: Timestamp) -> EngineResult<EventLength> {
        let record = self.branch_record()?;
        let rows = self
            .event_rows(&record, ReadSelector::Latest, None)?
            .into_iter()
            .filter(|event| event.timestamp() <= timestamp)
            .count();
        Ok(EventLength::new(u64::try_from(rows).unwrap_or(u64::MAX)))
    }

    /// Reads latest events filtered by type.
    pub fn get_by_type(
        &mut self,
        event_type: &EventType,
        after_sequence: Option<EventSequence>,
        limit: Option<usize>,
    ) -> EngineResult<Vec<EventVersionedRecord>> {
        self.get_by_type_with_selector(event_type, after_sequence, limit, ReadSelector::Latest)
    }

    /// Reads timestamp-visible events filtered by type.
    pub fn get_by_type_at(
        &mut self,
        event_type: &EventType,
        timestamp: Timestamp,
        after_sequence: Option<EventSequence>,
        limit: Option<usize>,
    ) -> EngineResult<Vec<EventVersionedRecord>> {
        let mut events = self.get_by_type_with_selector(
            event_type,
            after_sequence,
            limit,
            ReadSelector::Latest,
        )?;
        events.retain(|event| event.timestamp() <= timestamp);
        Ok(events)
    }

    /// Reads a sequence range.
    ///
    /// Forward ranges read `[start_seq, end_seq)`. Reverse ranges walk
    /// backward from `start_seq` and treat `end_seq` as an exclusive lower
    /// bound when present.
    pub fn range(
        &mut self,
        start_seq: EventSequence,
        end_seq: Option<EventSequence>,
        limit: Option<usize>,
        direction: EventRangeDirection,
        event_type: Option<&EventType>,
    ) -> EngineResult<EventRangePage> {
        if limit == Some(0) {
            return Ok(EventRangePage::new(Vec::new(), false, None));
        }
        let record = self.branch_record()?;
        let latest_len = self.latest_event_count(&record)?;
        let events = match direction {
            EventRangeDirection::Forward => {
                let upper = end_seq
                    .map_or(latest_len, EventSequence::as_u64)
                    .min(latest_len);
                if start_seq.as_u64() >= upper {
                    return Ok(EventRangePage::new(Vec::new(), false, None));
                }
                self.scan_sequence_window(
                    &record,
                    start_seq.as_u64(),
                    upper,
                    ReadSelector::Latest,
                    event_type,
                )?
            }
            EventRangeDirection::Reverse => {
                if latest_len == 0 || start_seq.as_u64() >= latest_len {
                    return Ok(EventRangePage::new(Vec::new(), false, None));
                }
                let lower = end_seq.map_or(0, |end| end.as_u64().saturating_add(1));
                let upper = start_seq.as_u64().saturating_add(1).min(latest_len);
                if lower >= upper {
                    return Ok(EventRangePage::new(Vec::new(), false, None));
                }
                let mut events = self.scan_sequence_window(
                    &record,
                    lower,
                    upper,
                    ReadSelector::Latest,
                    event_type,
                )?;
                events.reverse();
                events
            }
        };
        Ok(page_from_events(events, limit))
    }

    /// Reads a timestamp range.
    pub fn range_by_time(
        &mut self,
        start_ts: Timestamp,
        end_ts: Option<Timestamp>,
        limit: Option<usize>,
        direction: EventRangeDirection,
        event_type: Option<&EventType>,
    ) -> EngineResult<EventRangePage> {
        if limit == Some(0) {
            return Ok(EventRangePage::new(Vec::new(), false, None));
        }
        let record = self.branch_record()?;
        let mut events = self.event_rows(&record, ReadSelector::Latest, None)?;
        events.retain(|event| {
            event.timestamp() >= start_ts
                && end_ts.is_none_or(|end| event.timestamp() <= end)
                && event_type.is_none_or(|expected| event.event_type() == expected)
        });
        events.sort_by_key(|event| (event.timestamp(), event.sequence()));
        if direction == EventRangeDirection::Reverse {
            events.reverse();
        }
        Ok(page_from_events(events, limit))
    }

    /// Lists latest event types.
    pub fn list_types(&mut self) -> EngineResult<EventTypeList> {
        let record = self.branch_record()?;
        let metadata = self.read_metadata(&record, ReadSelector::Latest)?;
        let event_types = metadata
            .event_types()
            .map(EventType::new)
            .collect::<EngineResult<Vec<_>>>()?;
        Ok(EventTypeList::new(event_types))
    }

    /// Lists event types visible at a timestamp.
    pub fn list_types_at(&mut self, timestamp: Timestamp) -> EngineResult<EventTypeList> {
        let record = self.branch_record()?;
        let events = self.event_rows(&record, ReadSelector::Latest, None)?;
        let event_types = events
            .into_iter()
            .filter(|event| event.timestamp() <= timestamp)
            .map(|event| event.event_type().clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        Ok(EventTypeList::new(event_types))
    }

    /// Lists events up to a timestamp.
    pub fn list(
        &mut self,
        event_type: Option<&EventType>,
        limit: Option<usize>,
        as_of: Option<Timestamp>,
    ) -> EngineResult<Vec<EventVersionedRecord>> {
        Ok(self
            .list_page(event_type, None, limit, as_of)?
            .events()
            .to_vec())
    }

    /// Lists events with sequence-cursor pagination.
    pub fn list_page(
        &mut self,
        event_type: Option<&EventType>,
        after_sequence: Option<EventSequence>,
        limit: Option<usize>,
        as_of: Option<Timestamp>,
    ) -> EngineResult<EventRangePage> {
        if limit == Some(0) {
            return Ok(EventRangePage::new(Vec::new(), false, None));
        }
        let record = self.branch_record()?;
        let mut events = self.event_rows(&record, ReadSelector::Latest, None)?;
        events.retain(|event| {
            event_type.is_none_or(|expected| event.event_type() == expected)
                && after_sequence.is_none_or(|after| event.sequence() > after)
                && as_of.is_none_or(|timestamp| event.timestamp() <= timestamp)
        });
        Ok(page_from_events(events, limit))
    }

    /// Verifies visible event density and hash linkage.
    pub fn verify_chain(&mut self) -> EngineResult<EventChainVerification> {
        let record = self.branch_record()?;
        let metadata = self.read_metadata(&record, ReadSelector::Latest)?;
        let rows = self
            .event_raw_rows(&record, ReadSelector::Latest, None)?
            .into_iter()
            .filter(|row| !row.is_tombstone())
            .collect::<Vec<_>>();
        Ok(verify_chain_rows(&self.space, &metadata, &rows))
    }

    fn get_with_selector(
        &mut self,
        sequence: EventSequence,
        selector: ReadSelector,
    ) -> EngineResult<Option<EventVersionedRecord>> {
        let record = self.branch_record()?;
        let address = self.event_address(&record, sequence);
        let Some(row) = self.persistence.read_row(&address, selector)? else {
            return Ok(None);
        };
        event_from_row(&self.space, &row).map(Some)
    }

    fn get_by_type_with_selector(
        &mut self,
        event_type: &EventType,
        after_sequence: Option<EventSequence>,
        limit: Option<usize>,
        selector: ReadSelector,
    ) -> EngineResult<Vec<EventVersionedRecord>> {
        if limit == Some(0) {
            return Ok(Vec::new());
        }
        let record = self.branch_record()?;
        let mut events = self.event_rows(&record, selector, None)?;
        events.retain(|event| {
            event.event_type() == event_type
                && after_sequence.is_none_or(|after| event.sequence() > after)
        });
        if let Some(limit) = limit {
            events.truncate(limit);
        }
        Ok(events)
    }

    fn event_rows(
        &mut self,
        record: &BranchCatalogRecord,
        selector: ReadSelector,
        limit: Option<usize>,
    ) -> EngineResult<Vec<EventVersionedRecord>> {
        self.event_raw_rows(record, selector, limit)?
            .into_iter()
            .filter(|row| !row.is_tombstone())
            .map(|row| event_from_row(&self.space, &row))
            .collect()
    }

    fn event_raw_rows(
        &mut self,
        record: &BranchCatalogRecord,
        selector: ReadSelector,
        limit: Option<usize>,
    ) -> EngineResult<Vec<PersistenceReadRow>> {
        self.persistence.scan_prefix(
            record.storage_branch_id(),
            RowClass::Event,
            encode_event_space_prefix(&self.space),
            selector,
            limit,
        )
    }

    fn scan_sequence_window(
        &mut self,
        record: &BranchCatalogRecord,
        start: u64,
        end: u64,
        selector: ReadSelector,
        event_type: Option<&EventType>,
    ) -> EngineResult<Vec<EventVersionedRecord>> {
        let mut current = encode_event_key(&self.space, EventSequence::new(start));
        let upper = encode_event_key(&self.space, EventSequence::new(end));
        let mut events = Vec::new();
        while current < upper {
            let rows = self.persistence.scan_range(
                record.storage_branch_id(),
                RowClass::Event,
                Some(current.clone()),
                Some(upper.clone()),
                selector,
                Some(EVENT_RANGE_RAW_PAGE_MAX),
            )?;
            if rows.is_empty() {
                break;
            }
            for row in &rows {
                if row.is_tombstone() {
                    continue;
                }
                let event = event_from_row(&self.space, row)?;
                if event_type.is_none_or(|expected| event.event_type() == expected) {
                    events.push(event);
                }
            }
            let last_key = rows.last().expect("non-empty raw page").key();
            current = exclusive_after_key(last_key);
        }
        Ok(events)
    }

    fn latest_event_count(&mut self, record: &BranchCatalogRecord) -> EngineResult<u64> {
        Ok(self
            .read_metadata(record, ReadSelector::Latest)?
            .next_sequence())
    }

    fn read_metadata(
        &mut self,
        record: &BranchCatalogRecord,
        selector: ReadSelector,
    ) -> EngineResult<EventLogMetadata> {
        let address = self.metadata_address(record);
        let Some(row) = self.persistence.read_row(&address, selector)? else {
            return Ok(EventLogMetadata::default());
        };
        if row.is_tombstone() {
            return Ok(EventLogMetadata::default());
        }
        let value = row.value().ok_or_else(|| {
            EngineError::corruption(
                "data_loss.engine.event_metadata",
                "stored event metadata row is missing a value",
            )
        })?;
        decode_event_metadata(value)
    }

    fn branch_record(&self) -> EngineResult<BranchCatalogRecord> {
        self.control.require_healthy()?;
        self.control
            .lookup_branch(&self.branch)
            .cloned()
            .ok_or_else(|| {
                EngineError::not_found(
                    "not_found.engine.branch",
                    format!("branch `{}` does not exist", self.branch),
                )
            })
    }

    fn event_address(&self, record: &BranchCatalogRecord, sequence: EventSequence) -> RowAddress {
        RowAddress::new(
            record.storage_branch_id(),
            RowClass::Event,
            encode_event_key(&self.space, sequence),
        )
    }

    fn metadata_address(&self, record: &BranchCatalogRecord) -> RowAddress {
        RowAddress::new(
            record.storage_branch_id(),
            RowClass::EventMetadata,
            encode_event_meta_key(&self.space),
        )
    }

    fn type_index_address(
        &self,
        record: &BranchCatalogRecord,
        event_type: &EventType,
        sequence: EventSequence,
    ) -> RowAddress {
        RowAddress::new(
            record.storage_branch_id(),
            RowClass::EventIndex,
            encode_event_type_index_key(&self.space, event_type, sequence),
        )
    }

    fn commit_batch(
        &mut self,
        record: &BranchCatalogRecord,
        mutations: Vec<RowMutation>,
    ) -> EngineResult<CommitOutcome> {
        let mut mutations = mutations;
        if mutations.is_empty() {
            return Err(EngineError::invalid_input(
                "invalid_argument.engine.event_batch",
                "event batch must contain at least one mutation",
            ));
        }
        let user_put_count = mutations
            .iter()
            .filter(|mutation| mutation.is_put())
            .count();
        let user_delete_count = mutations
            .iter()
            .filter(|mutation| mutation.is_delete())
            .count();
        let mut space_mutations =
            ControlPlane::space_registration_mutations(self.persistence, record, &self.space)?;
        if !space_mutations.is_empty() {
            space_mutations.extend(mutations);
            mutations = space_mutations;
        }
        let plan = CommitPlan::new(
            record.storage_branch_id(),
            mutations,
            Some(record.generation()),
        );
        Ok(self
            .persistence
            .commit(&plan)?
            .with_counts(user_put_count, user_delete_count))
    }
}

fn event_from_row(
    space: &ProductSpace,
    row: &PersistenceReadRow,
) -> EngineResult<EventVersionedRecord> {
    let sequence = decode_event_key_sequence(space, row.key())?;
    let value = row.value().ok_or_else(|| {
        EngineError::corruption(
            "data_loss.engine.event_record",
            "stored event row is missing a value",
        )
    })?;
    let envelope = decode_event_record(sequence, value)?;
    Ok(super::outcome::event_record_from_envelope(
        &envelope,
        row.commit_version(),
        row.commit_timestamp(),
    ))
}

fn verify_chain_rows(
    space: &ProductSpace,
    metadata: &EventLogMetadata,
    rows: &[PersistenceReadRow],
) -> EventChainVerification {
    let mut previous_hash = [0; 32];
    for (expected, row) in rows.iter().enumerate() {
        let expected = u64::try_from(expected).unwrap_or(u64::MAX);
        let expected_sequence = EventSequence::new(expected);
        let sequence = match decode_event_key_sequence(space, row.key()) {
            Ok(sequence) => sequence,
            Err(error) => {
                return EventChainVerification::invalid(
                    metadata.next_sequence(),
                    expected_sequence,
                    error.to_string(),
                );
            }
        };
        if sequence != expected_sequence {
            return EventChainVerification::invalid(
                metadata.next_sequence(),
                expected_sequence,
                "event sequence is not dense",
            );
        }
        let Some(value) = row.value() else {
            return EventChainVerification::invalid(
                metadata.next_sequence(),
                sequence,
                "stored event row is missing a value",
            );
        };
        let event = match decode_event_record(sequence, value) {
            Ok(event) => event,
            Err(error) => {
                return EventChainVerification::invalid(
                    metadata.next_sequence(),
                    sequence,
                    error.to_string(),
                );
            }
        };
        if event.previous_hash() != previous_hash {
            return EventChainVerification::invalid(
                metadata.next_sequence(),
                event.sequence(),
                "event previous hash does not match prior event",
            );
        }
        previous_hash = event.hash();
    }
    let row_count = u64::try_from(rows.len()).unwrap_or(u64::MAX);
    if row_count != metadata.next_sequence() {
        return EventChainVerification::invalid(
            metadata.next_sequence(),
            EventSequence::new(row_count.min(metadata.next_sequence())),
            "event log length does not match metadata",
        );
    }
    if previous_hash != metadata.head_hash() {
        return EventChainVerification::invalid(
            metadata.next_sequence(),
            EventSequence::new(metadata.next_sequence().saturating_sub(1)),
            "event metadata head hash does not match visible chain",
        );
    }
    EventChainVerification::valid(metadata.next_sequence())
}

fn page_from_events(mut events: Vec<EventVersionedRecord>, limit: Option<usize>) -> EventRangePage {
    let Some(limit) = limit else {
        return EventRangePage::new(events, false, None);
    };
    let has_more = events.len() > limit;
    if has_more {
        events.truncate(limit);
    }
    let cursor = has_more.then(|| events.last().expect("non-empty event page").sequence());
    EventRangePage::new(events, has_more, cursor)
}

fn finalize_batch_items(
    items: Vec<Option<EventBatchAppendItemOutcome>>,
) -> Vec<EventBatchAppendItemOutcome> {
    items
        .into_iter()
        .map(|item| item.expect("batch item outcome recorded"))
        .collect()
}

fn exclusive_after_key(key: &[u8]) -> Vec<u8> {
    let mut next = key.to_vec();
    next.push(0);
    next
}

fn next_event_timestamp(metadata: &EventLogMetadata) -> Timestamp {
    normalized_event_timestamp(metadata, unix_timestamp())
}

fn normalized_event_timestamp(metadata: &EventLogMetadata, observed: Timestamp) -> Timestamp {
    let timestamp_micros = observed.as_micros().max(
        metadata
            .last_timestamp_micros()
            .map_or(0, |last| last.saturating_add(1)),
    );
    Timestamp::from_micros(timestamp_micros)
}

fn unix_timestamp() -> Timestamp {
    Timestamp::from_duration_since_epoch(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default(),
    )
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{
        normalized_event_timestamp, verify_chain_rows, EventLogMetadata, EventRecordEnvelope,
    };
    use crate::data::event::{compute_event_hash, EventPayload, EventSequence, EventType};
    use crate::persistence::{encode_event_key, PersistenceReadRow};
    use strata_core_next::Timestamp;

    #[test]
    fn normalized_event_timestamp_never_moves_backward() {
        let mut metadata = EventLogMetadata::default();
        assert_eq!(
            normalized_event_timestamp(&metadata, Timestamp::from_micros(5)).as_micros(),
            5
        );

        let event_type = EventType::new("clock.rollback").expect("valid type");
        let payload = EventPayload::new(json!({})).expect("valid payload");
        let timestamp = Timestamp::from_micros(10);
        let hash = compute_event_hash(0, &event_type, &payload, timestamp.as_micros(), &[0; 32])
            .expect("hash");
        metadata.push(&EventRecordEnvelope::new(
            EventSequence::new(0),
            event_type,
            payload,
            timestamp,
            [0; 32],
            hash,
        ));

        assert_eq!(
            normalized_event_timestamp(&metadata, Timestamp::from_micros(1)).as_micros(),
            11
        );
    }

    #[test]
    fn chain_verification_reports_corrupt_rows_as_invalid() {
        let space = crate::data::kv::ProductSpace::new("default").expect("valid space");
        let event_type = EventType::new("corrupt.row").expect("valid type");
        let payload = EventPayload::new(json!({})).expect("valid payload");
        let timestamp = Timestamp::from_micros(10);
        let correct_hash =
            compute_event_hash(0, &event_type, &payload, timestamp.as_micros(), &[0; 32])
                .expect("hash");
        let correct = EventRecordEnvelope::new(
            EventSequence::new(0),
            event_type.clone(),
            payload.clone(),
            timestamp,
            [0; 32],
            correct_hash,
        );
        let mut metadata = EventLogMetadata::default();
        metadata.push(&correct);

        let corrupt = EventRecordEnvelope::new(
            EventSequence::new(0),
            event_type,
            payload,
            timestamp,
            [0; 32],
            [1; 32],
        );
        let row = PersistenceReadRow::for_test(
            encode_event_key(&space, EventSequence::new(0)),
            Some(super::encode_event_record(&corrupt).expect("encoded corrupt row")),
            false,
        );
        let verification = verify_chain_rows(&space, &metadata, &[row]);
        assert!(!verification.is_valid());
        assert_eq!(verification.first_invalid(), Some(EventSequence::new(0)));
        assert!(verification
            .error_message()
            .expect("error message")
            .contains("stored event hash does not match"));
    }

    #[test]
    fn chain_verification_reports_sparse_rows_as_invalid() {
        let space = crate::data::kv::ProductSpace::new("default").expect("valid space");
        let event_type = EventType::new("sparse.row").expect("valid type");
        let payload = EventPayload::new(json!({})).expect("valid payload");
        let timestamp = Timestamp::from_micros(10);
        let hash = compute_event_hash(0, &event_type, &payload, timestamp.as_micros(), &[0; 32])
            .expect("hash");
        let event = EventRecordEnvelope::new(
            EventSequence::new(0),
            event_type,
            payload,
            timestamp,
            [0; 32],
            hash,
        );
        let mut metadata = EventLogMetadata::default();
        metadata.push(&event);

        let row = PersistenceReadRow::for_test(
            encode_event_key(&space, EventSequence::new(1)),
            Some(super::encode_event_record(&event).expect("encoded event")),
            false,
        );
        let verification = verify_chain_rows(&space, &metadata, &[row]);
        assert!(!verification.is_valid());
        assert_eq!(verification.first_invalid(), Some(EventSequence::new(0)));
        assert_eq!(
            verification.error_message(),
            Some("event sequence is not dense")
        );
    }

    #[test]
    fn chain_verification_reports_previous_hash_mismatch_as_invalid() {
        let space = crate::data::kv::ProductSpace::new("default").expect("valid space");
        let first = event_envelope(0, "chain.first", json!({"n": 1}), 10, [0; 32]);
        let second = event_envelope(1, "chain.second", json!({"n": 2}), 11, [7; 32]);
        let mut metadata = EventLogMetadata::default();
        metadata.push(&first);
        metadata.push(&second);
        let rows = [event_row(&space, &first), event_row(&space, &second)];

        let verification = verify_chain_rows(&space, &metadata, &rows);
        assert!(!verification.is_valid());
        assert_eq!(verification.first_invalid(), Some(EventSequence::new(1)));
        assert_eq!(
            verification.error_message(),
            Some("event previous hash does not match prior event")
        );
    }

    #[test]
    fn chain_verification_reports_metadata_head_mismatch_as_invalid() {
        let space = crate::data::kv::ProductSpace::new("default").expect("valid space");
        let row_event = event_envelope(0, "chain.first", json!({"n": 1}), 10, [0; 32]);
        let metadata_event = event_envelope(0, "chain.first", json!({"n": 2}), 10, [0; 32]);
        let mut metadata = EventLogMetadata::default();
        metadata.push(&metadata_event);
        let row = event_row(&space, &row_event);

        let verification = verify_chain_rows(&space, &metadata, &[row]);
        assert!(!verification.is_valid());
        assert_eq!(verification.first_invalid(), Some(EventSequence::new(0)));
        assert_eq!(
            verification.error_message(),
            Some("event metadata head hash does not match visible chain")
        );
    }

    fn event_envelope(
        sequence: u64,
        event_type: &str,
        payload: serde_json::Value,
        timestamp: u64,
        previous_hash: [u8; 32],
    ) -> EventRecordEnvelope {
        let event_type = EventType::new(event_type).expect("valid type");
        let payload = EventPayload::new(payload).expect("valid payload");
        let hash = compute_event_hash(sequence, &event_type, &payload, timestamp, &previous_hash)
            .expect("hash");
        EventRecordEnvelope::new(
            EventSequence::new(sequence),
            event_type,
            payload,
            Timestamp::from_micros(timestamp),
            previous_hash,
            hash,
        )
    }

    fn event_row(
        space: &crate::data::kv::ProductSpace,
        event: &EventRecordEnvelope,
    ) -> PersistenceReadRow {
        PersistenceReadRow::for_test(
            encode_event_key(space, event.sequence()),
            Some(super::encode_event_record(event).expect("encoded event")),
            false,
        )
    }
}
