use crate::support::*;

pub(super) fn event_commands() -> Vec<Command> {
    vec![
        Command::EventBatchAppend {
            branch: None,
            space: None,
            entries: vec![BatchEventEntry::new("user.created", json!({"id": 1}))],
        },
        Command::EventAppend {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            event_type: "user.updated".to_owned(),
            payload: json!({"id": 1, "name": "Ada"}),
        },
        Command::EventGet {
            branch: None,
            space: None,
            sequence: 0,
            as_of: Some(99),
            as_of_time: None,
        },
        Command::EventExists {
            branch: None,
            space: None,
            sequence: 0,
        },
        Command::EventCount {
            branch: None,
            space: None,
            as_of: Some(99),
            as_of_time: None,
        },
        Command::EventRange {
            branch: None,
            space: None,
            start_seq: 0,
            end_seq: Some(10),
            limit: Some(5),
            direction: EventRangeDirection::Forward,
            event_type: Some("user.created".to_owned()),
        },
        Command::EventRangeByTime {
            branch: None,
            space: None,
            start_ts: 1,
            end_ts: Some(99),
            limit: Some(5),
            direction: EventRangeDirection::Reverse,
            event_type: Some("user.created".to_owned()),
        },
        Command::EventListTypes {
            branch: None,
            space: None,
            as_of: Some(99),
            as_of_time: None,
        },
        Command::EventList {
            branch: None,
            space: None,
            event_type: Some("user.created".to_owned()),
            limit: Some(5),
            after_sequence: Some(1),
            as_of: Some(99),
            as_of_time: None,
        },
        Command::EventVerifyChain {
            branch: None,
            space: None,
        },
    ]
}

#[allow(clippy::too_many_lines)]
pub(super) fn event_round_trip_edge_commands() -> Vec<Command> {
    vec![
        Command::EventAppend {
            branch: None,
            space: None,
            event_type: "empty.object".to_owned(),
            payload: json!({}),
        },
        Command::EventAppend {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            event_type: "nested.object".to_owned(),
            payload: json!({
                "scalars": [true, false, 7, "value", null],
                "object": {"nested": [{"id": 1}, {"id": 2}]},
            }),
        },
        Command::EventBatchAppend {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            entries: Vec::new(),
        },
        Command::EventBatchAppend {
            branch: None,
            space: None,
            entries: vec![
                BatchEventEntry::new("", json!({"bad": true})),
                BatchEventEntry::new("bad.payload", json!(["not", "an", "object"])),
                BatchEventEntry::new("audit.recorded", json!({"ok": true})),
            ],
        },
        Command::EventRange {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            start_seq: 0,
            end_seq: None,
            limit: Some(0),
            direction: EventRangeDirection::Reverse,
            event_type: None,
        },
        Command::EventRangeByTime {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            start_ts: 0,
            end_ts: None,
            limit: Some(0),
            direction: EventRangeDirection::Forward,
            event_type: None,
        },
        Command::EventList {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            event_type: None,
            limit: Some(0),
            after_sequence: None,
            as_of: None,
            as_of_time: None,
        },
    ]
}
