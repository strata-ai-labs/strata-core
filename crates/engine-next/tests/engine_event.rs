//! Event log conformance tests.

mod common;

use serde_json::json;
use strata_core_next::Timestamp;
use strata_engine_next::{
    Database, EngineErrorClass, EventBatchAppendEntry, EventPayload, EventRangeDirection,
    EventRangePage, EventSequence, EventService, EventType,
};

use common::{branch, open_cache_database, open_durable_database, space};

#[test]
fn event_contract_runs_in_cache_and_durable_modes() {
    run_database_modes(exercise_event_contract);
}

#[test]
fn empty_event_log_contract_runs_in_cache_and_durable_modes() {
    run_database_modes(|database| {
        let mut events = event_service(database, "default", "default");
        let verification = events.verify_chain().expect("empty verify succeeds");
        assert!(verification.is_valid());
        assert_eq!(verification.length(), 0);
        assert_eq!(
            events
                .list_types()
                .expect("type list succeeds")
                .event_types(),
            &[]
        );
        assert_eq!(
            events
                .list_types_at(Timestamp::from_micros(u64::MAX))
                .expect("historical type list succeeds")
                .event_types(),
            &[]
        );
        assert!(events
            .range(
                EventSequence::new(0),
                None,
                None,
                EventRangeDirection::Forward,
                None,
            )
            .expect("empty range succeeds")
            .events()
            .is_empty());
    });
}

#[test]
fn event_batch_append_edge_cases_run_in_cache_and_durable_modes() {
    run_database_modes(exercise_event_batch_append_edge_cases);
}

#[test]
fn event_range_edge_cases_run_in_cache_and_durable_modes() {
    run_database_modes(exercise_event_range_edge_cases);
}

#[test]
fn event_type_and_payload_validation_are_engine_owned() {
    assert!(EventType::new("e".repeat(256)).is_ok());
    let error = EventType::new("bad\0type").expect_err("control byte rejected");
    assert_eq!(error.class(), EngineErrorClass::InvalidInput);
    assert_eq!(error.code(), "invalid_argument.engine.event_type");

    let error = EventType::new("").expect_err("empty type rejected");
    assert_eq!(error.class(), EngineErrorClass::InvalidInput);
    assert_eq!(error.code(), "invalid_argument.engine.event_type");

    let error = EventType::new(" ".repeat(3)).expect_err("whitespace type rejected");
    assert_eq!(error.class(), EngineErrorClass::InvalidInput);
    assert_eq!(error.code(), "invalid_argument.engine.event_type");

    let error = EventType::new("e".repeat(257)).expect_err("oversized type rejected");
    assert_eq!(error.class(), EngineErrorClass::InvalidInput);
    assert_eq!(error.code(), "invalid_argument.engine.event_type");

    assert_eq!(
        EventPayload::new(json!({}))
            .expect("empty payload")
            .as_inner(),
        &json!({})
    );
    assert_eq!(
        EventPayload::new(json!({"nested": [true, 1, "two"]}))
            .expect("nested payload")
            .as_inner(),
        &json!({"nested": [true, 1, "two"]})
    );

    for rejected in [json!(null), json!(1), json!("x"), json!(false), json!([1])] {
        let error = EventPayload::new(rejected).expect_err("non-object payload rejected");
        assert_eq!(error.class(), EngineErrorClass::InvalidInput);
        assert_eq!(error.code(), "invalid_argument.engine.event_payload");
    }
}

#[test]
fn event_branch_and_space_isolation_match_global_log_semantics() {
    let mut database = open_cache_database().expect("cache open succeeds");
    {
        let mut events = event_service(&mut database, "default", "default");
        events
            .append(
                event_type("base.created"),
                payload(json!({"branch": "default"})),
            )
            .expect("base append succeeds");
    }

    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("branch fork succeeds");

    {
        let mut feature = event_service(&mut database, "feature", "default");
        assert_eq!(feature.len().expect("feature len succeeds").count(), 1);
        let inherited = feature
            .get(EventSequence::new(0))
            .expect("feature read succeeds")
            .expect("inherited event exists");
        assert_eq!(inherited.event_type().as_str(), "base.created");
        let appended = feature
            .append(
                event_type("feature.created"),
                payload(json!({"branch": "feature"})),
            )
            .expect("feature append succeeds");
        assert_eq!(appended.sequence().as_u64(), 1);
    }

    let mut parent = event_service(&mut database, "default", "default");
    assert_eq!(parent.len().expect("parent len succeeds").count(), 1);
    assert!(parent
        .get(EventSequence::new(1))
        .expect("parent read succeeds")
        .is_none());
    let parent_appended = parent
        .append(
            event_type("base.updated"),
            payload(json!({"branch": "default"})),
        )
        .expect("parent append succeeds");
    assert_eq!(parent_appended.sequence().as_u64(), 1);
    drop(parent);

    let mut feature = event_service(&mut database, "feature", "default");
    assert_eq!(feature.len().expect("feature len succeeds").count(), 2);
    let local = feature
        .get(EventSequence::new(1))
        .expect("feature read succeeds")
        .expect("fork-local event exists");
    assert_eq!(local.event_type().as_str(), "feature.created");
    assert!(feature
        .get(EventSequence::new(2))
        .expect("feature read succeeds")
        .is_none());
    drop(feature);

    let mut other_space = event_service(&mut database, "default", "other");
    let appended = other_space
        .append(
            event_type("space.created"),
            payload(json!({"space": "other"})),
        )
        .expect("other-space append succeeds");
    assert_eq!(appended.sequence().as_u64(), 0);
    assert_eq!(other_space.len().expect("other len succeeds").count(), 1);
}

#[test]
fn event_forked_branches_diverge_without_cross_visibility() {
    run_database_modes(|database| {
        {
            let mut events = event_service(database, "default", "default");
            events
                .append(event_type("base.created"), payload(json!({"n": 0})))
                .expect("base append succeeds");
        }

        {
            let mut branches = database.branches().expect("branch service opens");
            branches
                .fork_current(&branch("default"), branch("alpha"))
                .expect("alpha fork succeeds");
            branches
                .fork_current(&branch("default"), branch("beta"))
                .expect("beta fork succeeds");
        }

        let alpha = append_and_read_branch_event(database, "alpha", "alpha.created");
        let beta = append_and_read_branch_event(database, "beta", "beta.created");
        assert_eq!(alpha, "alpha.created");
        assert_eq!(beta, "beta.created");

        let mut default_events = event_service(database, "default", "default");
        assert_eq!(
            default_events.len().expect("default len succeeds").count(),
            1
        );
        assert!(default_events
            .get(EventSequence::new(1))
            .expect("default read succeeds")
            .is_none());
    });
}

#[test]
fn event_missing_branch_errors_are_stable() {
    run_database_modes(|database| {
        let mut events = event_service(database, "missing", "default");
        let error = events.len().expect_err("missing branch rejected");
        assert_eq!(error.class(), EngineErrorClass::NotFound);
        assert_eq!(error.code(), "not_found.engine.branch");

        let error = events
            .batch_append(Vec::<EventBatchAppendEntry>::new())
            .expect_err("empty batch still checks branch");
        assert_eq!(error.class(), EngineErrorClass::NotFound);
        assert_eq!(error.code(), "not_found.engine.branch");

        let error = events
            .batch_append([EventBatchAppendEntry::from_raw("", json!({}))])
            .expect_err("invalid batch still checks branch");
        assert_eq!(error.class(), EngineErrorClass::NotFound);
        assert_eq!(error.code(), "not_found.engine.branch");
    });
}

#[test]
fn event_durable_reopen_preserves_log_and_continues_chain() {
    let tempdir = tempfile::tempdir().expect("tempdir");
    let last_hash;

    {
        let mut database = open_durable_database(tempdir.path()).expect("durable open succeeds");
        {
            let mut events = event_service(&mut database, "default", "default");
            events
                .append(event_type("first"), payload(json!({"n": 1})))
                .expect("first append succeeds");
            let batch = events
                .batch_append([
                    batch_entry("second", json!({"n": 2})),
                    batch_entry("third", json!({"n": 3})),
                ])
                .expect("batch append succeeds");
            assert_eq!(batch.items().len(), 2);
            last_hash = events
                .get(EventSequence::new(2))
                .expect("read succeeds")
                .expect("third event exists")
                .hash();
            assert!(events.verify_chain().expect("verify succeeds").is_valid());
        }
        database.close().expect("close succeeds");
    }

    let mut reopened = open_durable_database(tempdir.path()).expect("reopen succeeds");
    let mut events = event_service(&mut reopened, "default", "default");
    assert_eq!(events.len().expect("len succeeds").count(), 3);
    assert_eq!(
        events
            .list_types()
            .expect("list types succeeds")
            .event_types()
            .iter()
            .map(EventType::as_str)
            .collect::<Vec<_>>(),
        vec!["first", "second", "third"]
    );
    assert!(events.verify_chain().expect("verify succeeds").is_valid());
    let appended = events
        .append(event_type("fourth"), payload(json!({"n": 4})))
        .expect("append after reopen succeeds");
    assert_eq!(appended.sequence().as_u64(), 3);
    let fourth = events
        .get(EventSequence::new(3))
        .expect("read succeeds")
        .expect("fourth exists");
    assert_eq!(fourth.previous_hash(), last_hash);
}

fn exercise_event_batch_append_edge_cases(database: &mut Database) {
    let mut events = event_service(database, "default", "default");
    let empty = events
        .batch_append(Vec::<EventBatchAppendEntry>::new())
        .expect("empty batch succeeds");
    assert!(empty.items().is_empty());
    assert!(empty.commit().is_none());
    assert_eq!(events.len().expect("empty len succeeds").count(), 0);

    let invalid = events
        .batch_append([
            EventBatchAppendEntry::from_raw("", json!({})),
            EventBatchAppendEntry::from_raw("audit.recorded", json!([])),
        ])
        .expect("invalid batch returns item outcomes");
    assert!(invalid.commit().is_none());
    assert_eq!(invalid.items().len(), 2);
    assert!(invalid.items().iter().all(|item| item.sequence().is_none()));
    assert!(invalid
        .items()
        .iter()
        .all(|item| item.error_message().is_some()));
    assert_eq!(events.len().expect("invalid len succeeds").count(), 0);

    let mixed = events
        .batch_append([
            EventBatchAppendEntry::from_raw("user.created", json!({"id": 1})),
            EventBatchAppendEntry::from_raw(" ", json!({"bad": true})),
            EventBatchAppendEntry::from_raw("audit.recorded", json!({"id": 1})),
        ])
        .expect("mixed batch succeeds");
    let commit = mixed.commit().expect("valid entries committed");
    assert_eq!(mixed.items().len(), 3);
    assert_eq!(mixed.items()[0].sequence(), Some(EventSequence::new(0)));
    assert_eq!(mixed.items()[0].commit_version(), Some(commit.version()));
    assert!(mixed.items()[1].sequence().is_none());
    assert!(mixed.items()[1]
        .error_message()
        .expect("validation error recorded")
        .contains("invalid_argument.engine.event_type"));
    assert_eq!(mixed.items()[2].sequence(), Some(EventSequence::new(1)));
    assert_eq!(events.len().expect("mixed len succeeds").count(), 2);

    let first = events
        .get(EventSequence::new(0))
        .expect("first read succeeds")
        .expect("first event exists");
    let second = events
        .get(EventSequence::new(1))
        .expect("second read succeeds")
        .expect("second event exists");
    assert_eq!(second.previous_hash(), first.hash());
    assert!(first.timestamp() <= second.timestamp());
    assert!(events.verify_chain().expect("verify succeeds").is_valid());
}

fn exercise_event_range_edge_cases(database: &mut Database) {
    append_contract_events(database);
    let mut events = event_service(database, "default", "default");
    assert_event_missing_and_type_edges(&mut events);
    assert_event_sequence_range_edges(&mut events);
    assert_event_timestamp_and_list_edges(&mut events);
}

fn assert_event_missing_and_type_edges(events: &mut EventService<'_>) {
    assert!(events
        .get(EventSequence::new(99))
        .expect("missing read succeeds")
        .is_none());
    assert!(!events
        .exists(EventSequence::new(99))
        .expect("missing exists succeeds"));
    assert!(events
        .get_by_type(&event_type("missing"), None, None)
        .expect("missing type query succeeds")
        .is_empty());
    assert!(events
        .get_by_type(&event_type("user.created"), None, Some(0))
        .expect("zero limit type query succeeds")
        .is_empty());
}

fn assert_event_sequence_range_edges(events: &mut EventService<'_>) {
    assert_empty_range(events.range(
        EventSequence::new(2),
        Some(EventSequence::new(2)),
        None,
        EventRangeDirection::Forward,
        None,
    ));
    assert_empty_range(events.range(
        EventSequence::new(3),
        Some(EventSequence::new(2)),
        None,
        EventRangeDirection::Forward,
        None,
    ));
    assert_empty_range(events.range(
        EventSequence::new(0),
        Some(EventSequence::new(4)),
        Some(0),
        EventRangeDirection::Forward,
        None,
    ));

    assert_eq!(
        events
            .range(
                EventSequence::new(0),
                Some(EventSequence::new(4)),
                None,
                EventRangeDirection::Forward,
                Some(&event_type("user.created")),
            )
            .expect("typed range succeeds")
            .events()
            .iter()
            .map(|event| event.sequence().as_u64())
            .collect::<Vec<_>>(),
        vec![0, 2]
    );
    assert_eq!(
        events
            .range(
                EventSequence::new(2),
                Some(EventSequence::new(100)),
                None,
                EventRangeDirection::Forward,
                None,
            )
            .expect("clamped range succeeds")
            .events()
            .iter()
            .map(|event| event.sequence().as_u64())
            .collect::<Vec<_>>(),
        vec![2, 3]
    );
}

fn assert_event_timestamp_and_list_edges(events: &mut EventService<'_>) {
    let third = events
        .get(EventSequence::new(2))
        .expect("third read succeeds")
        .expect("third event exists");
    assert_eq!(
        events
            .range_by_time(
                third.timestamp(),
                Some(third.timestamp()),
                None,
                EventRangeDirection::Forward,
                None,
            )
            .expect("closed time range succeeds")
            .events()
            .iter()
            .map(|event| event.sequence().as_u64())
            .collect::<Vec<_>>(),
        vec![2]
    );
    assert!(events
        .range_by_time(
            Timestamp::from_micros(third.timestamp().as_micros().saturating_add(10)),
            Some(Timestamp::from_micros(
                third.timestamp().as_micros().saturating_add(11),
            )),
            None,
            EventRangeDirection::Forward,
            None,
        )
        .expect("empty time range succeeds")
        .events()
        .is_empty());
    assert!(events
        .list(None, Some(0), None)
        .expect("zero limit list succeeds")
        .is_empty());
}

fn exercise_event_contract(database: &mut Database) {
    append_contract_events(database);
    let reads = assert_latest_event_reads(database);
    assert_event_ranges_and_lists(database, &reads);
    assert_event_history_and_chain(database);
}

struct EventReadFacts {
    first_event: Timestamp,
    third_event: Timestamp,
}

struct EventHistoryFacts {
    first: Timestamp,
    second: Timestamp,
    third: Timestamp,
}

fn append_contract_events(database: &mut Database) {
    let mut events = event_service(database, "default", "default");
    assert_eq!(events.len().expect("empty len succeeds").count(), 0);
    assert!(events
        .get(EventSequence::new(0))
        .expect("missing read succeeds")
        .is_none());
    assert!(!events
        .exists(EventSequence::new(0))
        .expect("missing exists succeeds"));

    let _ = events
        .append(event_type("user.created"), payload(json!({"id": 1})))
        .expect("first append succeeds")
        .commit();
    let _ = events
        .append(
            event_type("user.updated"),
            payload(json!({"id": 1, "name": "Ada"})),
        )
        .expect("second append succeeds")
        .commit();
    let batch = events
        .batch_append([
            batch_entry("user.created", json!({"id": 2})),
            batch_entry("audit.recorded", json!({"ok": true})),
        ])
        .expect("batch append succeeds");
    assert_eq!(batch.items().len(), 2);
    assert_eq!(
        batch.items()[0]
            .sequence()
            .expect("first batch sequence")
            .as_u64(),
        2
    );
    batch.items()[0]
        .commit_timestamp()
        .expect("batch timestamp");
}

fn assert_latest_event_reads(database: &mut Database) -> EventReadFacts {
    let mut events = event_service(database, "default", "default");
    assert_eq!(events.len().expect("len succeeds").count(), 4);
    assert!(events
        .exists(EventSequence::new(3))
        .expect("exists succeeds"));
    let first_event = events
        .get(EventSequence::new(0))
        .expect("get succeeds")
        .expect("event exists");
    let first_event_ts = first_event.timestamp();
    assert_eq!(first_event.event_type().as_str(), "user.created");
    assert_eq!(first_event.payload().as_inner(), &json!({"id": 1}));
    let second_event = events
        .get(EventSequence::new(1))
        .expect("get succeeds")
        .expect("event exists");
    assert_eq!(second_event.previous_hash(), first_event.hash());
    let third_event_ts = events
        .get(EventSequence::new(2))
        .expect("get succeeds")
        .expect("event exists")
        .timestamp();

    assert_eq!(
        events
            .get_by_type(&event_type("user.created"), None, None)
            .expect("type query succeeds")
            .iter()
            .map(|event| event.sequence().as_u64())
            .collect::<Vec<_>>(),
        vec![0, 2]
    );
    assert_eq!(
        events
            .get_by_type(
                &event_type("user.created"),
                Some(EventSequence::new(0)),
                Some(1)
            )
            .expect("type query succeeds")
            .iter()
            .map(|event| event.sequence().as_u64())
            .collect::<Vec<_>>(),
        vec![2]
    );
    EventReadFacts {
        first_event: first_event_ts,
        third_event: third_event_ts,
    }
}

fn assert_event_ranges_and_lists(database: &mut Database, reads: &EventReadFacts) {
    let mut events = event_service(database, "default", "default");
    let page = events
        .range(
            EventSequence::new(0),
            Some(EventSequence::new(4)),
            Some(2),
            EventRangeDirection::Forward,
            None,
        )
        .expect("range succeeds");
    assert_eq!(
        page.events()
            .iter()
            .map(|event| event.sequence().as_u64())
            .collect::<Vec<_>>(),
        vec![0, 1]
    );
    assert!(page.has_more());
    assert_eq!(page.cursor().expect("cursor").as_u64(), 1);

    let reverse = events
        .range(
            EventSequence::new(0),
            Some(EventSequence::new(4)),
            Some(2),
            EventRangeDirection::Reverse,
            None,
        )
        .expect("reverse range succeeds");
    assert_eq!(
        reverse
            .events()
            .iter()
            .map(|event| event.sequence().as_u64())
            .collect::<Vec<_>>(),
        vec![3, 2]
    );

    assert_eq!(
        events
            .range_by_time(
                reads.first_event,
                Some(reads.third_event),
                None,
                EventRangeDirection::Forward,
                Some(&event_type("user.created")),
            )
            .expect("time range succeeds")
            .events()
            .iter()
            .map(|event| event.sequence().as_u64())
            .collect::<Vec<_>>(),
        vec![0, 2]
    );

    let reverse_time = events
        .range_by_time(
            Timestamp::from_micros(0),
            None,
            None,
            EventRangeDirection::Reverse,
            None,
        )
        .expect("reverse time range succeeds");
    assert_eq!(
        reverse_time
            .events()
            .iter()
            .map(|event| event.sequence().as_u64())
            .collect::<Vec<_>>(),
        vec![3, 2, 1, 0]
    );
    assert!(reverse_time
        .events()
        .windows(2)
        .all(|events| events[0].timestamp() >= events[1].timestamp()));

    assert_eq!(
        events
            .list_types()
            .expect("list types succeeds")
            .event_types()
            .iter()
            .map(EventType::as_str)
            .collect::<Vec<_>>(),
        vec!["audit.recorded", "user.created", "user.updated"]
    );
    assert_eq!(
        events
            .list(Some(&event_type("user.created")), Some(1), None)
            .expect("list succeeds")
            .iter()
            .map(|event| event.sequence().as_u64())
            .collect::<Vec<_>>(),
        vec![0]
    );
}

fn assert_event_history_and_chain(database: &mut Database) {
    let facts = assert_event_timestamp_boundaries(database);
    assert_event_historical_type_filters(database, &facts);
    assert_event_historical_type_lists(database, &facts);
    assert_event_historical_lists(database, &facts);
    let mut events = event_service(database, "default", "default");
    assert!(events
        .verify_chain()
        .expect("chain verify succeeds")
        .is_valid());
}

fn assert_event_timestamp_boundaries(database: &mut Database) -> EventHistoryFacts {
    let mut events = event_service(database, "default", "default");
    let first_event_ts = events
        .get(EventSequence::new(0))
        .expect("latest read succeeds")
        .expect("first event exists")
        .timestamp();
    let second_event_ts = events
        .get(EventSequence::new(1))
        .expect("latest read succeeds")
        .expect("second event exists")
        .timestamp();
    let third_event_ts = events
        .get(EventSequence::new(2))
        .expect("latest read succeeds")
        .expect("third event exists")
        .timestamp();
    let before_first = Timestamp::from_micros(first_event_ts.as_micros().saturating_sub(1));
    assert_eq!(
        events
            .len_at(before_first)
            .expect("len_at before first")
            .count(),
        0
    );
    assert!(events
        .get_at(EventSequence::new(0), before_first)
        .expect("historical read succeeds")
        .is_none());
    assert!(events
        .get_at(EventSequence::new(0), first_event_ts)
        .expect("historical read succeeds")
        .is_some());
    assert_eq!(
        events.len_at(first_event_ts).expect("len_at first").count(),
        1
    );
    assert_eq!(
        events
            .len_at(second_event_ts)
            .expect("len_at second")
            .count(),
        2
    );
    assert!(events
        .get_at(EventSequence::new(2), second_event_ts)
        .expect("historical read succeeds")
        .is_none());
    assert!(events
        .get_at(EventSequence::new(2), third_event_ts)
        .expect("historical read succeeds")
        .is_some());
    EventHistoryFacts {
        first: first_event_ts,
        second: second_event_ts,
        third: third_event_ts,
    }
}

fn assert_event_historical_type_filters(database: &mut Database, facts: &EventHistoryFacts) {
    let mut events = event_service(database, "default", "default");
    assert_eq!(
        events
            .get_by_type_at(&event_type("user.created"), facts.second, None, None)
            .expect("historical type query succeeds")
            .iter()
            .map(|event| event.sequence().as_u64())
            .collect::<Vec<_>>(),
        vec![0]
    );
    assert_eq!(
        events
            .get_by_type_at(&event_type("user.created"), facts.third, None, None)
            .expect("historical type query succeeds")
            .iter()
            .map(|event| event.sequence().as_u64())
            .collect::<Vec<_>>(),
        vec![0, 2]
    );
}

fn assert_event_historical_type_lists(database: &mut Database, facts: &EventHistoryFacts) {
    let mut events = event_service(database, "default", "default");
    assert_eq!(
        events
            .list_types_at(facts.first)
            .expect("historical type list succeeds")
            .event_types()
            .iter()
            .map(EventType::as_str)
            .collect::<Vec<_>>(),
        vec!["user.created"]
    );
    assert_eq!(
        events
            .list_types_at(facts.second)
            .expect("historical type list succeeds")
            .event_types()
            .iter()
            .map(EventType::as_str)
            .collect::<Vec<_>>(),
        vec!["user.created", "user.updated"]
    );
}

fn assert_event_historical_lists(database: &mut Database, facts: &EventHistoryFacts) {
    let mut events = event_service(database, "default", "default");
    assert_eq!(
        events
            .list(Some(&event_type("user.created")), None, Some(facts.second))
            .expect("historical list succeeds")
            .iter()
            .map(|event| event.sequence().as_u64())
            .collect::<Vec<_>>(),
        vec![0]
    );
}

fn append_and_read_branch_event(
    database: &mut Database,
    branch_name: &str,
    event_type_name: &str,
) -> String {
    let mut events = event_service(database, branch_name, "default");
    let appended = events
        .append(
            event_type(event_type_name),
            payload(json!({"branch": branch_name})),
        )
        .expect("branch append succeeds");
    assert_eq!(appended.sequence().as_u64(), 1);
    assert_eq!(events.len().expect("branch len succeeds").count(), 2);
    events
        .get(EventSequence::new(1))
        .expect("branch read succeeds")
        .expect("branch event exists")
        .event_type()
        .as_str()
        .to_owned()
}

fn assert_empty_range(page: strata_engine_next::EngineResult<EventRangePage>) {
    let page = page.expect("empty range succeeds");
    assert!(page.events().is_empty());
    assert!(!page.has_more());
    assert!(page.cursor().is_none());
}

fn run_database_modes(exercise: fn(&mut Database)) {
    let mut cache = open_cache_database().expect("cache open succeeds");
    exercise(&mut cache);

    let tempdir = tempfile::tempdir().expect("tempdir");
    let mut durable = open_durable_database(tempdir.path()).expect("durable open succeeds");
    exercise(&mut durable);
}

fn event_service<'a>(
    database: &'a mut Database,
    branch_name: &str,
    space_name: &str,
) -> EventService<'a> {
    database
        .event(branch(branch_name), space(space_name))
        .expect("event service opens")
}

fn event_type(value: &str) -> EventType {
    EventType::new(value).expect("valid event type")
}

fn payload(value: serde_json::Value) -> EventPayload {
    EventPayload::new(value).expect("valid payload")
}

fn batch_entry(event_type: &str, payload: serde_json::Value) -> EventBatchAppendEntry {
    EventBatchAppendEntry::new(self::event_type(event_type), self::payload(payload))
}
