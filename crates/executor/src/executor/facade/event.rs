use super::super::{
    BatchEventEntry, Command, EventRangeDirection, Executor, ExecutorResult, Output,
};

impl Executor {
    /// Executes a default-branch event batch-append command.
    pub fn event_batch_append(&mut self, entries: Vec<BatchEventEntry>) -> ExecutorResult<Output> {
        self.execute(Command::EventBatchAppend {
            branch: None,
            space: None,
            entries,
        })
    }

    /// Executes a default-branch event append command.
    pub fn event_append(
        &mut self,
        event_type: impl Into<String>,
        payload: serde_json::Value,
    ) -> ExecutorResult<Output> {
        self.execute(Command::EventAppend {
            branch: None,
            space: None,
            event_type: event_type.into(),
            payload,
        })
    }

    /// Executes a default-branch event get command.
    pub fn event_get(&mut self, sequence: u64) -> ExecutorResult<Output> {
        self.execute(Command::EventGet {
            branch: None,
            space: None,
            sequence,
            as_of: None,
        })
    }

    /// Executes a default-branch event exists command.
    pub fn event_exists(&mut self, sequence: u64) -> ExecutorResult<Output> {
        self.execute(Command::EventExists {
            branch: None,
            space: None,
            sequence,
        })
    }

    /// Executes a default-branch event type-filter command (a type-filtered
    /// event list — the canonical path for reading events by type).
    pub fn event_get_by_type(
        &mut self,
        event_type: impl Into<String>,
        limit: Option<u64>,
        after_sequence: Option<u64>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::EventList {
            branch: None,
            space: None,
            event_type: Some(event_type.into()),
            limit,
            after_sequence,
            as_of: None,
        })
    }

    /// Executes a default-branch event count command.
    pub fn event_count(&mut self) -> ExecutorResult<Output> {
        self.execute(Command::EventCount {
            branch: None,
            space: None,
            as_of: None,
        })
    }

    /// Executes a default-branch event sequence-range command.
    pub fn event_range(
        &mut self,
        start_seq: u64,
        end_seq: Option<u64>,
        limit: Option<u64>,
        direction: EventRangeDirection,
        event_type: Option<String>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::EventRange {
            branch: None,
            space: None,
            start_seq,
            end_seq,
            limit,
            direction,
            event_type,
        })
    }

    /// Executes a default-branch event timestamp-range command.
    pub fn event_range_by_time(
        &mut self,
        start_ts: u64,
        end_ts: Option<u64>,
        limit: Option<u64>,
        direction: EventRangeDirection,
        event_type: Option<String>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::EventRangeByTime {
            branch: None,
            space: None,
            start_ts,
            end_ts,
            limit,
            direction,
            event_type,
        })
    }

    /// Executes a default-branch event type-list command.
    pub fn event_list_types(&mut self) -> ExecutorResult<Output> {
        self.execute(Command::EventListTypes {
            branch: None,
            space: None,
            as_of: None,
        })
    }

    /// Executes a default-branch event list command.
    pub fn event_list(
        &mut self,
        event_type: Option<String>,
        limit: Option<u64>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::EventList {
            branch: None,
            space: None,
            event_type,
            limit,
            after_sequence: None,
            as_of: None,
        })
    }

    /// Executes a default-branch event chain-verify command.
    pub fn event_verify_chain(&mut self) -> ExecutorResult<Output> {
        self.execute(Command::EventVerifyChain {
            branch: None,
            space: None,
        })
    }
}
