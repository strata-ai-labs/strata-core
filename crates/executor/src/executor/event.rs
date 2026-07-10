use super::{
    engine_event_direction, engine_event_type, event_append_output, event_batch_append_item_result,
    event_batch_entry, event_batch_result, event_chain_verification, event_payload,
    event_range_output, event_records, event_sequence, event_versioned_data,
    optional_engine_event_type, optional_limit, BatchEventEntry, EventRangeDirection, Executor,
    ExecutorResult, Output, PageInfo, Timestamp,
};

impl Executor {
    pub(super) fn execute_event_batch_append(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        entries: Vec<BatchEventEntry>,
    ) -> ExecutorResult<Output> {
        let entries = entries
            .into_iter()
            .map(event_batch_entry)
            .collect::<Vec<_>>();
        let mut service = self.event_service(branch, space)?;
        let outcome = service.batch_append(entries)?;
        Ok(Output::EventBatchAppendResults(event_batch_result(
            outcome
                .items()
                .iter()
                .map(|item| event_batch_append_item_result(item, outcome.commit()))
                .collect(),
        )))
    }

    pub(super) fn execute_event_append(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        event_type: String,
        payload: serde_json::Value,
    ) -> ExecutorResult<Output> {
        let event_type = engine_event_type(event_type)?;
        let payload = event_payload(payload)?;
        let mut service = self.event_service(branch, space)?;
        Ok(event_append_output(&service.append(event_type, payload)?))
    }

    pub(super) fn execute_event_get(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        sequence: u64,
        as_of: Option<u64>,
    ) -> ExecutorResult<Output> {
        let sequence = event_sequence(sequence);
        let mut service = self.event_service(branch, space)?;
        let record = if let Some(as_of) = as_of {
            service.get_at(sequence, Timestamp::from_micros(as_of))?
        } else {
            service.get(sequence)?
        };
        Ok(Output::EventRecord(
            record.as_ref().map(event_versioned_data),
        ))
    }

    pub(super) fn execute_event_exists(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        sequence: u64,
    ) -> ExecutorResult<Output> {
        let sequence = event_sequence(sequence);
        let mut service = self.event_service(branch, space)?;
        Ok(Output::Bool(service.exists(sequence)?))
    }

    pub(super) fn execute_event_len(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        as_of: Option<u64>,
    ) -> ExecutorResult<Output> {
        let mut service = self.event_service(branch, space)?;
        let count = if let Some(as_of) = as_of {
            service.len_at(Timestamp::from_micros(as_of))?.count()
        } else {
            service.len()?.count()
        };
        Ok(Output::EventLength { count })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn execute_event_range(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        start_seq: u64,
        end_seq: Option<u64>,
        limit: Option<u64>,
        direction: EventRangeDirection,
        event_type: Option<String>,
    ) -> ExecutorResult<Output> {
        let end_seq = end_seq.map(event_sequence);
        let limit = optional_limit(limit)?;
        let direction = engine_event_direction(direction);
        let event_type = optional_engine_event_type(event_type)?;
        let mut service = self.event_service(branch, space)?;
        let page = service.range(
            event_sequence(start_seq),
            end_seq,
            limit,
            direction,
            event_type.as_ref(),
        )?;
        Ok(event_range_output(&page))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn execute_event_range_by_time(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        start_ts: u64,
        end_ts: Option<u64>,
        limit: Option<u64>,
        direction: EventRangeDirection,
        event_type: Option<String>,
    ) -> ExecutorResult<Output> {
        let end_ts = end_ts.map(Timestamp::from_micros);
        let limit = optional_limit(limit)?;
        let direction = engine_event_direction(direction);
        let event_type = optional_engine_event_type(event_type)?;
        let mut service = self.event_service(branch, space)?;
        let page = service.range_by_time(
            Timestamp::from_micros(start_ts),
            end_ts,
            limit,
            direction,
            event_type.as_ref(),
        )?;
        Ok(event_range_output(&page))
    }

    pub(super) fn execute_event_list_types(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        as_of: Option<u64>,
    ) -> ExecutorResult<Output> {
        let mut service = self.event_service(branch, space)?;
        let types = if let Some(as_of) = as_of {
            service.list_types_at(Timestamp::from_micros(as_of))?
        } else {
            service.list_types()?
        };
        Ok(Output::EventTypeList {
            items: types
                .event_types()
                .iter()
                .map(|event_type| event_type.as_str().to_owned())
                .collect(),
            page: PageInfo::terminal(),
        })
    }

    pub(super) fn execute_event_list(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        event_type: Option<String>,
        limit: Option<u64>,
        after_sequence: Option<u64>,
        as_of: Option<u64>,
    ) -> ExecutorResult<Output> {
        let event_type = optional_engine_event_type(event_type)?;
        let limit = optional_limit(limit)?;
        let after_sequence = after_sequence.map(event_sequence);
        let as_of = as_of.map(Timestamp::from_micros);
        let mut service = self.event_service(branch, space)?;
        let page = service.list_page(event_type.as_ref(), after_sequence, limit, as_of)?;
        Ok(Output::EventRecords {
            items: event_records(page.events()),
            page: PageInfo::new(
                page.has_more(),
                page.cursor().map(strata_engine::EventSequence::as_u64),
            ),
        })
    }

    pub(super) fn execute_event_verify_chain(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
    ) -> ExecutorResult<Output> {
        let mut service = self.event_service(branch, space)?;
        Ok(Output::EventChainVerification(event_chain_verification(
            &service.verify_chain()?,
        )))
    }
}
