//! Executor event command behavior tests.

use serde_json::{json, Value};
use strata_engine::{CacheOpenOptions, Database};
use strata_executor::{
    BatchEventEntry, Command, EventRangeDirection, EventVersionedData, Executor,
    ExecutorErrorClass, MutationEffect, Output, PageInfo, DEFAULT_BRANCH,
};
use tempfile::TempDir;

#[test]
fn cache_executor_runs_complete_event_command_suite() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    run_event_command_suite(&mut executor);
}

#[test]
fn durable_executor_runs_event_error_contract() {
    let temp = TempDir::new().expect("temp dir");
    let path = temp.path().join("db");
    let mut executor = Executor::open_durable_local(&path).expect("durable executor opens");
    run_event_error_contract(&mut executor);
}

#[test]
fn durable_executor_reopens_event_log() {
    let temp = TempDir::new().expect("temp dir");
    let path = temp.path().join("db");

    {
        let mut executor = Executor::open_durable_local(&path).expect("durable executor opens");
        run_event_command_suite(&mut executor);
        executor.close().expect("durable executor closes");
    }

    let mut reopened = Executor::open_durable_local(&path).expect("durable executor reopens");
    assert_eq!(event_len(&mut reopened, None, None, None), 4);
    let first = get_event(&mut reopened, None, None, 0, None).expect("event exists");
    assert_eq!(first.event().event_type(), "user.created");
    assert_eq!(first.event().payload(), &json!({"id": 1}));
    let verification = verify_chain(&mut reopened, None, None);
    assert!(verification.is_valid());
    assert_eq!(verification.length(), 4);

    let old_head = get_event(&mut reopened, None, None, 3, None)
        .expect("head event exists")
        .event()
        .hash()
        .to_owned();
    let reopened_append =
        append_event(&mut reopened, None, None, "user.reopened", json!({"id": 3}));
    assert_eq!(reopened_append.sequence, 4);
    let new_head = get_event(&mut reopened, None, None, 4, None).expect("new head exists");
    assert_eq!(new_head.event().previous_hash(), old_head);
    assert_eq!(event_len(&mut reopened, None, None, None), 5);
}

#[test]
fn event_branch_and_space_defaults_are_isolated() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    populate_branch_and_space_fixture(&mut executor);
    assert_branch_defaults_and_isolation(&mut executor);
    assert_space_defaults_and_isolation(&mut executor);
}

#[test]
fn durable_event_branch_and_space_defaults_are_isolated() {
    let temp = TempDir::new().expect("temp dir");
    let path = temp.path().join("db");
    let mut executor = Executor::open_durable_local(&path).expect("durable executor opens");
    populate_branch_and_space_fixture(&mut executor);
    assert_branch_defaults_and_isolation(&mut executor);
    assert_space_defaults_and_isolation(&mut executor);
}

fn populate_branch_and_space_fixture(executor: &mut Executor) {
    append_event(
        executor,
        None,
        None,
        "base.event",
        json!({"branch": "base"}),
    );
    executor
        .create_branch_from_head(DEFAULT_BRANCH, "feature")
        .expect("branch creates");

    let feature_append = append_event(
        executor,
        Some("feature"),
        None,
        "feature.event",
        json!({"branch": "feature"}),
    );
    assert_eq!(feature_append.sequence, 1);
    append_event(
        executor,
        None,
        Some("tenant-a"),
        "space.event",
        json!({"space": "tenant-a"}),
    );
    append_event(
        executor,
        None,
        Some("tenant-b"),
        "base.event",
        json!({"space": "tenant-b"}),
    );
    executor
        .execute(Command::BranchCreate {
            branch: "scratch".to_owned(),
        })
        .expect("independent branch creates");
    let scratch_append = append_event(
        executor,
        Some("scratch"),
        None,
        "scratch.event",
        json!({"branch": "scratch"}),
    );
    assert_eq!(scratch_append.sequence, 0);
}

fn assert_branch_defaults_and_isolation(executor: &mut Executor) {
    assert_eq!(event_len(executor, None, None, None), 1);
    assert_eq!(event_len(executor, Some("feature"), None, None), 2);
    assert_eq!(event_len(executor, Some("scratch"), None, None), 1);
    assert_eq!(
        get_event(executor, None, None, 0, None)
            .expect("default event")
            .event()
            .event_type(),
        "base.event"
    );
    assert!(get_event(executor, None, None, 1, None).is_none());
    assert_eq!(
        get_event(executor, Some("feature"), None, 1, None)
            .expect("branch event")
            .event()
            .event_type(),
        "feature.event"
    );
    assert_eq!(
        event_sequences(
            &event_range_in(
                executor,
                Some("feature"),
                None,
                0,
                None,
                None,
                EventRangeDirection::Forward,
                None,
            )
            .0
        ),
        vec![0, 1]
    );
}

fn assert_space_defaults_and_isolation(executor: &mut Executor) {
    assert_eq!(event_len(executor, None, Some("tenant-a"), None), 1);
    assert_eq!(event_len(executor, None, Some("tenant-b"), None), 1);
    assert_eq!(
        get_event(executor, None, Some("tenant-a"), 0, None)
            .expect("space event")
            .event()
            .event_type(),
        "space.event"
    );
    assert_eq!(
        event_types(executor, None, None, None),
        vec!["base.event".to_owned()]
    );
    assert_eq!(
        event_types(executor, None, Some("tenant-a"), None),
        vec!["space.event".to_owned()]
    );
    assert_eq!(
        event_types(executor, None, Some("tenant-b"), None),
        vec!["base.event".to_owned()]
    );
    assert_eq!(
        event_sequences(
            &event_range_in(
                executor,
                None,
                Some("tenant-a"),
                0,
                None,
                None,
                EventRangeDirection::Forward,
                None,
            )
            .0
        ),
        vec![0]
    );
}

#[test]
fn event_executor_inherits_configured_database_default_branch() {
    let options = CacheOpenOptions::new()
        .with_default_branch("main")
        .expect("valid branch");
    let database = Database::open_cache(options)
        .expect("cache database opens")
        .into_database();
    let mut executor = Executor::from_database(database);

    assert_eq!(executor.default_branch(), "main");
    append_event(
        &mut executor,
        None,
        None,
        "main.event",
        json!({"where": "main"}),
    );
    assert_eq!(event_len(&mut executor, None, None, None), 1);

    let error = executor
        .execute(Command::EventGet {
            branch: Some(DEFAULT_BRANCH.to_owned()),
            space: None,
            sequence: 0,
            as_of: None,
        })
        .expect_err("literal default branch is absent");
    assert_eq!(error.class(), ExecutorErrorClass::NotFound);
}

#[test]
fn event_batch_append_preserves_positional_results() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    run_event_batch_append_edges(&mut executor);
}

#[test]
fn durable_event_batch_append_preserves_positional_results() {
    let temp = TempDir::new().expect("temp dir");
    let path = temp.path().join("db");
    let mut executor = Executor::open_durable_local(&path).expect("durable executor opens");
    run_event_batch_append_edges(&mut executor);
}

fn run_event_batch_append_edges(executor: &mut Executor) {
    assert!(matches!(
        executor
            .execute(Command::EventBatchAppend {
                branch: None,
                space: None,
                entries: Vec::new(),
            })
            .expect("empty batch succeeds"),
        Output::EventBatchAppendResults(results) if results.is_empty() && !results.applied()
    ));

    let output = executor
        .execute(Command::EventBatchAppend {
            branch: None,
            space: None,
            entries: vec![
                BatchEventEntry::new("", json!({"bad": true})),
                BatchEventEntry::new("user.created", json!({"id": 1})),
                BatchEventEntry::new("bad.payload", json!(["not", "object"])),
                BatchEventEntry::new("user.updated", json!({"id": 1})),
            ],
        })
        .expect("mixed batch succeeds");
    let Output::EventBatchAppendResults(results) = output else {
        panic!("unexpected batch output");
    };
    assert_eq!(results.len(), 4);
    assert!(results[0].error().is_some());
    assert_eq!(
        results[0].error_status().expect("event type status").code(),
        "invalid_argument.engine.event_type"
    );
    assert_eq!(results[1].sequence(), Some(0));
    assert_eq!(results[1].event_type(), Some("user.created"));
    assert!(results[1]
        .commit()
        .map(strata_executor::CommitReceipt::version)
        .is_some());
    assert!(results[1]
        .commit()
        .map(strata_executor::CommitReceipt::timestamp)
        .is_some());
    assert_eq!(results[1].effect(), Some(&MutationEffect::created()));
    assert_eq!(
        results[1]
            .commit()
            .expect("first valid item commit")
            .version(),
        results[1]
            .commit()
            .map(strata_executor::CommitReceipt::version)
            .expect("first valid item version")
    );
    assert!(results[1].error().is_none());
    assert_eq!(results[0].effect(), None);
    assert_eq!(results[0].commit(), None);
    assert!(results[2].error().is_some());
    assert_eq!(
        results[2]
            .error_status()
            .expect("event payload status")
            .code(),
        "invalid_argument.engine.event_payload"
    );
    assert_eq!(results[2].effect(), None);
    assert_eq!(results[2].commit(), None);
    assert_eq!(results[3].sequence(), Some(1));
    assert_eq!(results[3].event_type(), Some("user.updated"));
    assert!(results[3]
        .commit()
        .map(strata_executor::CommitReceipt::version)
        .is_some());
    assert!(results[3]
        .commit()
        .map(strata_executor::CommitReceipt::timestamp)
        .is_some());
    assert_eq!(results[3].effect(), Some(&MutationEffect::created()));
    assert_eq!(
        results[3]
            .commit()
            .expect("second valid item commit")
            .version(),
        results[1]
            .commit()
            .expect("first valid item commit")
            .version()
    );
    assert!(results[3].error().is_none());
    assert_eq!(event_len(executor, None, None, None), 2);

    let first = get_event(executor, None, None, 0, None).expect("first valid item exists");
    let second = get_event(executor, None, None, 1, None).expect("second valid item exists");
    assert_eq!(second.event().previous_hash(), first.event().hash());
}

#[test]
fn event_error_contract_runs_in_cache_mode() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    run_event_error_contract(&mut executor);
}

#[test]
fn event_convenience_facade_runs_complete_event_command_suite() {
    let mut executor = Executor::open_cache().expect("cache executor opens");

    assert_eq!(
        executor.event_len().expect("len succeeds"),
        Output::EventLength { count: 0 }
    );
    assert!(matches!(
        executor
            .event_batch_append(Vec::new())
            .expect("empty batch succeeds"),
        Output::EventBatchAppendResults(results) if results.is_empty() && !results.applied()
    ));

    assert!(matches!(
        executor
            .event_append("facade.created", json!({"id": 1}))
            .expect("append succeeds"),
        Output::EventAppendResult {
            sequence: 0,
            event_type,
            ..
        } if event_type == "facade.created"
    ));
    assert!(matches!(
        executor
            .event_batch_append(vec![BatchEventEntry::new(
                "facade.updated",
                json!({"id": 1}),
            )])
            .expect("batch append succeeds"),
        Output::EventBatchAppendResults(results) if results.len() == 1
            && results[0].sequence() == Some(1)
    ));
    assert!(matches!(
        executor.event_get(0).expect("get succeeds"),
        Output::EventRecord(Some(_))
    ));
    assert_eq!(
        executor.event_exists(0).expect("exists succeeds"),
        Output::Bool(true)
    );
    assert!(matches!(
        executor
            .event_get_by_type("facade.created", Some(10), None)
            .expect("type read succeeds"),
        Output::EventRecords { items: records, .. } if event_sequences(&records) == vec![0]
    ));
    assert_eq!(
        executor.event_len().expect("len succeeds"),
        Output::EventLength { count: 2 }
    );
    assert!(matches!(
        executor
            .event_range(0, None, None, EventRangeDirection::Forward, None)
            .expect("range succeeds"),
        Output::EventRangeResult { items: events, .. } if event_sequences(&events) == vec![0, 1]
    ));
    assert!(matches!(
        executor
            .event_range_by_time(0, None, None, EventRangeDirection::Forward, None)
            .expect("time range succeeds"),
        Output::EventRangeResult { items: events, .. } if event_sequences(&events) == vec![0, 1]
    ));
    assert_eq!(
        executor.event_list_types().expect("type list succeeds"),
        Output::EventTypeList {
            items: vec!["facade.created".to_owned(), "facade.updated".to_owned()],
            page: PageInfo::terminal(),
        }
    );
    assert!(matches!(
        executor.event_list(None, None).expect("list succeeds"),
        Output::EventRecords { items: records, .. } if event_sequences(&records) == vec![0, 1]
    ));
    assert!(matches!(
        executor.event_verify_chain().expect("verify succeeds"),
        Output::EventChainVerification(verification) if verification.is_valid()
            && verification.length() == 2
    ));
}

fn run_event_error_contract(executor: &mut Executor) {
    assert_eq!(event_len(executor, None, None, None), 0);

    for command in invalid_input_event_commands() {
        let before = event_len(executor, None, None, None);
        let error = executor.execute(command).expect_err("command fails");
        assert_eq!(error.class(), ExecutorErrorClass::InvalidInput);
        assert_eq!(event_len(executor, None, None, None), before);
    }

    let missing_branch = executor
        .execute(Command::EventAppend {
            branch: Some("missing".to_owned()),
            space: None,
            event_type: "user.created".to_owned(),
            payload: json!({"id": 1}),
        })
        .expect_err("missing branch fails");
    assert_eq!(missing_branch.class(), ExecutorErrorClass::NotFound);

    executor.close().expect("close succeeds");
    let closed = executor
        .execute(Command::EventLen {
            branch: None,
            space: None,
            as_of: None,
        })
        .expect_err("closed command fails");
    assert_eq!(closed.class(), ExecutorErrorClass::ClosedHandle);
}

#[test]
fn event_command_to_output_mapping_is_explicit_for_every_variant() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    append_event(&mut executor, None, None, "map.created", json!({"id": 1}));
    append_event(&mut executor, None, None, "map.updated", json!({"id": 1}));

    let outputs = event_mapping_commands()
        .into_iter()
        .map(|command| executor.execute(command).expect("command succeeds"))
        .collect::<Vec<_>>();

    assert_eq!(outputs.len(), 11);
    assert!(matches!(outputs[0], Output::EventBatchAppendResults(_)));
    assert!(matches!(outputs[1], Output::EventAppendResult { .. }));
    assert!(matches!(outputs[2], Output::EventRecord(_)));
    assert!(matches!(outputs[3], Output::Bool(_)));
    assert!(matches!(outputs[4], Output::EventRecords { .. }));
    assert!(matches!(outputs[5], Output::EventLength { .. }));
    assert!(matches!(outputs[6], Output::EventRangeResult { .. }));
    assert!(matches!(outputs[7], Output::EventRangeResult { .. }));
    assert!(matches!(outputs[8], Output::EventTypeList { .. }));
    assert!(matches!(outputs[9], Output::EventRecords { .. }));
    assert!(matches!(outputs[10], Output::EventChainVerification(_)));
}

fn run_event_command_suite(executor: &mut Executor) {
    assert_eq!(event_len(executor, None, None, None), 0);
    assert!(get_event(executor, None, None, 0, None).is_none());
    assert!(!event_exists(executor, None, None, 0));
    assert_eq!(
        event_types(executor, None, None, None),
        Vec::<String>::new()
    );
    assert_eq!(
        verify_chain(executor, None, None),
        strata_executor::EventChainVerification::new(true, 0, None, None)
    );

    let first_append = append_event(executor, None, None, "user.created", json!({"id": 1}));
    assert_eq!(first_append.sequence, 0);
    assert_eq!(first_append.event_type, "user.created");
    assert!(first_append.version > 0);
    assert!(first_append.timestamp > 0);
    let second_append = append_event(
        executor,
        None,
        None,
        "user.updated",
        json!({"id": 1, "name": "Ada"}),
    );
    assert_eq!(second_append.sequence, 1);
    assert_eq!(second_append.event_type, "user.updated");
    assert!(second_append.version > first_append.version);
    assert!(second_append.timestamp >= first_append.timestamp);
    let batch = event_batch_append(
        executor,
        None,
        None,
        vec![
            BatchEventEntry::new("user.created", json!({"id": 2})),
            BatchEventEntry::new("audit.recorded", json!({"ok": true})),
        ],
    );
    assert_eq!(batch.len(), 2);
    assert_eq!(batch[0].sequence(), Some(2));
    assert_eq!(batch[1].sequence(), Some(3));
    assert_eq!(batch[0].event_type(), Some("user.created"));
    assert_eq!(batch[1].event_type(), Some("audit.recorded"));
    assert!(batch[0]
        .commit()
        .map(strata_executor::CommitReceipt::version)
        .is_some());
    assert!(batch[0]
        .commit()
        .map(strata_executor::CommitReceipt::timestamp)
        .is_some());
    assert!(batch[1]
        .commit()
        .map(strata_executor::CommitReceipt::version)
        .is_some());
    assert!(batch[1]
        .commit()
        .map(strata_executor::CommitReceipt::timestamp)
        .is_some());
    let third = get_event(executor, None, None, 2, None).expect("third event exists");
    let fourth = get_event(executor, None, None, 3, None).expect("fourth event exists");
    assert_eq!(
        third.event().previous_hash(),
        get_event(executor, None, None, 1, None)
            .expect("second event exists")
            .event()
            .hash()
    );
    assert_eq!(fourth.event().previous_hash(), third.event().hash());

    assert_latest_event_reads(executor);
    assert_event_ranges_and_lists(executor);
    assert_event_history_and_chain(executor);
}

fn assert_latest_event_reads(executor: &mut Executor) {
    assert_eq!(event_len(executor, None, None, None), 4);
    assert!(event_exists(executor, None, None, 3));

    let first = get_event(executor, None, None, 0, None).expect("first event exists");
    assert_eq!(first.event().event_type(), "user.created");
    assert_eq!(first.event().payload(), &json!({"id": 1}));
    assert!(first.version() > 0);
    assert!(first.timestamp() > 0);
    assert_eq!(first.event().previous_hash(), "00".repeat(32));
    assert_eq!(first.event().hash().len(), 64);
    assert!(first.event().hash().chars().all(|c| c.is_ascii_hexdigit()));
    // as_of selects by the branch commit timeline (the versioned record's
    // commit timestamp), not by the event's occurrence timestamp — the same
    // domain every other primitive's as_of uses.
    assert_eq!(
        get_event(
            executor,
            None,
            None,
            0,
            Some(first.timestamp().saturating_sub(1)),
        ),
        None
    );
    assert!(get_event(executor, None, None, 0, Some(first.timestamp())).is_some());

    let second = get_event(executor, None, None, 1, None).expect("second event exists");
    assert_eq!(second.event().previous_hash(), first.event().hash());
    assert!(second.version() > first.version());
    assert!(second.timestamp() >= first.timestamp());

    let created = event_records_by_type(executor, "user.created", None, None, None);
    assert_eq!(event_sequences(&created), vec![0, 2]);
    let created_page = event_records_by_type(executor, "user.created", Some(1), Some(0), None);
    assert_eq!(event_sequences(&created_page), vec![2]);
    assert!(event_records_by_type(executor, "user.created", Some(0), None, None).is_empty());
    assert!(event_records_by_type(executor, "missing.type", None, None, None).is_empty());
    let historical = event_records_by_type(
        executor,
        "user.created",
        None,
        None,
        Some(first.timestamp()),
    );
    assert_eq!(event_sequences(&historical), vec![0]);
}

fn assert_event_ranges_and_lists(executor: &mut Executor) {
    let first = get_event(executor, None, None, 0, None).expect("first event exists");
    let third = get_event(executor, None, None, 2, None).expect("third event exists");
    let fourth = get_event(executor, None, None, 3, None).expect("fourth event exists");

    assert_event_sequence_ranges(executor);
    assert_event_timestamp_ranges(executor, &first, &third, &fourth);
    assert_event_lists(executor);
}

fn assert_event_sequence_ranges(executor: &mut Executor) {
    let (events, has_more, cursor) =
        event_range(executor, 0, None, None, EventRangeDirection::Forward, None);
    assert_eq!(event_sequences(&events), vec![0, 1, 2, 3]);
    assert!(!has_more);
    assert_eq!(cursor, None);

    let (events, has_more, cursor) = event_range(
        executor,
        0,
        Some(4),
        Some(2),
        EventRangeDirection::Forward,
        None,
    );
    assert_eq!(event_sequences(&events), vec![0, 1]);
    assert!(has_more);
    assert_eq!(cursor, Some(1));

    let (events, _, _) = event_range(
        executor,
        2,
        None,
        Some(2),
        EventRangeDirection::Reverse,
        None,
    );
    assert_eq!(event_sequences(&events), vec![2, 1]);

    let (events, _, _) = event_range(
        executor,
        3,
        Some(1),
        None,
        EventRangeDirection::Reverse,
        None,
    );
    assert_eq!(event_sequences(&events), vec![3, 2]);

    let (events, has_more, cursor) = event_range(
        executor,
        2,
        Some(99),
        None,
        EventRangeDirection::Forward,
        None,
    );
    assert_eq!(event_sequences(&events), vec![2, 3]);
    assert!(!has_more);
    assert_eq!(cursor, None);

    let (events, has_more, cursor) = event_range(
        executor,
        2,
        Some(2),
        None,
        EventRangeDirection::Forward,
        None,
    );
    assert!(events.is_empty());
    assert!(!has_more);
    assert_eq!(cursor, None);

    let (events, has_more, cursor) = event_range(
        executor,
        0,
        None,
        Some(0),
        EventRangeDirection::Forward,
        None,
    );
    assert!(events.is_empty());
    assert!(!has_more);
    assert_eq!(cursor, None);

    let (events, _, _) = event_range(
        executor,
        0,
        None,
        None,
        EventRangeDirection::Forward,
        Some("user.created"),
    );
    assert_eq!(event_sequences(&events), vec![0, 2]);
}

fn assert_event_timestamp_ranges(
    executor: &mut Executor,
    first: &EventVersionedData,
    third: &EventVersionedData,
    fourth: &EventVersionedData,
) {
    let (events, _, _) = event_range_by_time(
        executor,
        first.event().timestamp(),
        Some(third.event().timestamp()),
        None,
        EventRangeDirection::Forward,
        Some("user.created"),
    );
    assert_eq!(event_sequences(&events), vec![0, 2]);

    let (events, _, _) =
        event_range_by_time(executor, 0, None, None, EventRangeDirection::Reverse, None);
    assert_eq!(event_sequences(&events), vec![3, 2, 1, 0]);

    let (events, has_more, cursor) = event_range_by_time(
        executor,
        0,
        None,
        Some(2),
        EventRangeDirection::Forward,
        None,
    );
    assert_eq!(event_sequences(&events), vec![0, 1]);
    assert!(has_more);
    assert_eq!(cursor, Some(1));

    let (events, has_more, cursor) = event_range_by_time(
        executor,
        0,
        None,
        Some(0),
        EventRangeDirection::Forward,
        None,
    );
    assert!(events.is_empty());
    assert!(!has_more);
    assert_eq!(cursor, None);

    let (events, has_more, cursor) = event_range_by_time(
        executor,
        fourth.event().timestamp().saturating_add(1),
        None,
        None,
        EventRangeDirection::Forward,
        None,
    );
    assert!(events.is_empty());
    assert!(!has_more);
    assert_eq!(cursor, None);
}

fn assert_event_lists(executor: &mut Executor) {
    assert_eq!(
        event_types(executor, None, None, None),
        vec![
            "audit.recorded".to_owned(),
            "user.created".to_owned(),
            "user.updated".to_owned(),
        ]
    );
    assert_eq!(
        event_sequences(&event_list(executor, None, None, None)),
        vec![0, 1, 2, 3]
    );
    let (events, has_more, cursor) = event_list_page(executor, None, Some(2), None, None);
    assert_eq!(event_sequences(&events), vec![0, 1]);
    assert!(has_more);
    assert_eq!(cursor, Some(1));
    let (events, has_more, cursor) = event_list_page(executor, None, Some(2), cursor, None);
    assert_eq!(event_sequences(&events), vec![2, 3]);
    assert!(!has_more);
    assert_eq!(cursor, None);
    assert!(event_list(executor, None, Some(0), None).is_empty());
    assert_eq!(
        event_sequences(&event_list(executor, Some("user.created"), Some(1), None)),
        vec![0]
    );
    let (events, has_more, cursor) =
        event_list_page(executor, Some("user.created"), Some(10), Some(0), None);
    assert_eq!(event_sequences(&events), vec![2]);
    assert!(!has_more);
    assert_eq!(cursor, None);
}

fn assert_event_history_and_chain(executor: &mut Executor) {
    // Historical facts use commit timestamps (the as_of domain shared with
    // every other primitive), not the events' occurrence timestamps.
    let first = get_event(executor, None, None, 0, None)
        .expect("first event exists")
        .timestamp();
    let second = get_event(executor, None, None, 1, None)
        .expect("second event exists")
        .timestamp();
    let third = get_event(executor, None, None, 2, None)
        .expect("third event exists")
        .timestamp();

    assert_eq!(event_len(executor, None, None, Some(first)), 1);
    assert_eq!(event_len(executor, None, None, Some(second)), 2);
    assert_eq!(
        event_sequences(&event_records_by_type(
            executor,
            "user.created",
            None,
            None,
            Some(third),
        )),
        vec![0, 2]
    );
    assert_eq!(
        event_types(executor, None, None, Some(first)),
        vec!["user.created".to_owned()]
    );
    assert_eq!(
        event_sequences(&event_list(
            executor,
            Some("user.created"),
            None,
            Some(second)
        )),
        vec![0]
    );

    let verification = verify_chain(executor, None, None);
    assert!(verification.is_valid());
    assert_eq!(verification.length(), 4);
}

fn event_mapping_commands() -> Vec<Command> {
    vec![
        Command::EventBatchAppend {
            branch: None,
            space: None,
            entries: vec![BatchEventEntry::new("map.batch", json!({"id": 2}))],
        },
        Command::EventAppend {
            branch: None,
            space: None,
            event_type: "map.appended".to_owned(),
            payload: json!({"id": 3}),
        },
        Command::EventGet {
            branch: None,
            space: None,
            sequence: 0,
            as_of: None,
        },
        Command::EventExists {
            branch: None,
            space: None,
            sequence: 0,
        },
        Command::EventList {
            branch: None,
            space: None,
            event_type: Some("map.created".to_owned()),
            limit: None,
            after_sequence: None,
            as_of: None,
        },
        Command::EventLen {
            branch: None,
            space: None,
            as_of: None,
        },
        Command::EventRange {
            branch: None,
            space: None,
            start_seq: 0,
            end_seq: None,
            limit: Some(2),
            direction: EventRangeDirection::Forward,
            event_type: None,
        },
        Command::EventRangeByTime {
            branch: None,
            space: None,
            start_ts: 0,
            end_ts: None,
            limit: Some(2),
            direction: EventRangeDirection::Reverse,
            event_type: None,
        },
        Command::EventListTypes {
            branch: None,
            space: None,
            as_of: None,
        },
        Command::EventList {
            branch: None,
            space: None,
            event_type: Some("map.created".to_owned()),
            limit: Some(1),
            after_sequence: None,
            as_of: None,
        },
        Command::EventVerifyChain {
            branch: None,
            space: None,
        },
    ]
}

fn invalid_input_event_commands() -> Vec<Command> {
    let long_event_type = "x".repeat(257);
    vec![
        Command::EventAppend {
            branch: None,
            space: None,
            event_type: String::new(),
            payload: json!({"id": 1}),
        },
        Command::EventAppend {
            branch: None,
            space: None,
            event_type: long_event_type.clone(),
            payload: json!({"id": 1}),
        },
        Command::EventAppend {
            branch: None,
            space: None,
            event_type: "bad.payload".to_owned(),
            payload: json!(["not", "object"]),
        },
        Command::EventAppend {
            branch: None,
            space: None,
            event_type: "bad.payload".to_owned(),
            payload: json!("not-object"),
        },
        Command::EventAppend {
            branch: None,
            space: None,
            event_type: "bad.payload".to_owned(),
            payload: json!(7),
        },
        Command::EventAppend {
            branch: None,
            space: None,
            event_type: "bad.payload".to_owned(),
            payload: json!(null),
        },
        Command::EventList {
            branch: None,
            space: None,
            event_type: Some(String::new()),
            limit: None,
            after_sequence: None,
            as_of: None,
        },
        Command::EventList {
            branch: None,
            space: None,
            event_type: Some(long_event_type),
            limit: None,
            after_sequence: None,
            as_of: None,
        },
        Command::EventRange {
            branch: None,
            space: None,
            start_seq: 0,
            end_seq: None,
            limit: None,
            direction: EventRangeDirection::Forward,
            event_type: Some(String::new()),
        },
        Command::EventRangeByTime {
            branch: None,
            space: None,
            start_ts: 0,
            end_ts: None,
            limit: None,
            direction: EventRangeDirection::Forward,
            event_type: Some(String::new()),
        },
    ]
}

fn append_event(
    executor: &mut Executor,
    branch: Option<&str>,
    space: Option<&str>,
    event_type: &str,
    payload: Value,
) -> AppendFacts {
    match executor
        .execute(Command::EventAppend {
            branch: branch.map(str::to_owned),
            space: space.map(str::to_owned),
            event_type: event_type.to_owned(),
            payload,
        })
        .expect("append succeeds")
    {
        Output::EventAppendResult {
            sequence,
            event_type,
            effect,
            commit,
        } => {
            assert_eq!(effect, MutationEffect::created());
            let version = commit.version();
            let timestamp = commit.timestamp();
            AppendFacts {
                sequence,
                event_type,
                version,
                timestamp,
            }
        }
        output => panic!("unexpected append output: {output:?}"),
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct AppendFacts {
    sequence: u64,
    event_type: String,
    version: u64,
    timestamp: u64,
}

fn event_batch_append(
    executor: &mut Executor,
    branch: Option<&str>,
    space: Option<&str>,
    entries: Vec<BatchEventEntry>,
) -> strata_executor::BatchResult<strata_executor::EventBatchAppendItemResult> {
    match executor
        .execute(Command::EventBatchAppend {
            branch: branch.map(str::to_owned),
            space: space.map(str::to_owned),
            entries,
        })
        .expect("batch append succeeds")
    {
        Output::EventBatchAppendResults(results) => results,
        output => panic!("unexpected batch append output: {output:?}"),
    }
}

fn get_event(
    executor: &mut Executor,
    branch: Option<&str>,
    space: Option<&str>,
    sequence: u64,
    as_of: Option<u64>,
) -> Option<EventVersionedData> {
    match executor
        .execute(Command::EventGet {
            branch: branch.map(str::to_owned),
            space: space.map(str::to_owned),
            sequence,
            as_of,
        })
        .expect("get succeeds")
    {
        Output::EventRecord(record) => record,
        output => panic!("unexpected get output: {output:?}"),
    }
}

fn event_exists(
    executor: &mut Executor,
    branch: Option<&str>,
    space: Option<&str>,
    sequence: u64,
) -> bool {
    match executor
        .execute(Command::EventExists {
            branch: branch.map(str::to_owned),
            space: space.map(str::to_owned),
            sequence,
        })
        .expect("exists succeeds")
    {
        Output::Bool(exists) => exists,
        output => panic!("unexpected exists output: {output:?}"),
    }
}

fn event_records_by_type(
    executor: &mut Executor,
    event_type: &str,
    limit: Option<u64>,
    after_sequence: Option<u64>,
    as_of: Option<u64>,
) -> Vec<EventVersionedData> {
    match executor
        .execute(Command::EventList {
            branch: None,
            space: None,
            event_type: Some(event_type.to_owned()),
            limit,
            after_sequence,
            as_of,
        })
        .expect("type read succeeds")
    {
        Output::EventRecords { items: records, .. } => records,
        output => panic!("unexpected type read output: {output:?}"),
    }
}

fn event_len(
    executor: &mut Executor,
    branch: Option<&str>,
    space: Option<&str>,
    as_of: Option<u64>,
) -> u64 {
    match executor
        .execute(Command::EventLen {
            branch: branch.map(str::to_owned),
            space: space.map(str::to_owned),
            as_of,
        })
        .expect("len succeeds")
    {
        Output::EventLength { count } => count,
        output => panic!("unexpected len output: {output:?}"),
    }
}

fn event_range(
    executor: &mut Executor,
    start_seq: u64,
    end_seq: Option<u64>,
    limit: Option<u64>,
    direction: EventRangeDirection,
    event_type: Option<&str>,
) -> (Vec<EventVersionedData>, bool, Option<u64>) {
    event_range_in(
        executor, None, None, start_seq, end_seq, limit, direction, event_type,
    )
}

fn event_range_in(
    executor: &mut Executor,
    branch: Option<&str>,
    space: Option<&str>,
    start_seq: u64,
    end_seq: Option<u64>,
    limit: Option<u64>,
    direction: EventRangeDirection,
    event_type: Option<&str>,
) -> (Vec<EventVersionedData>, bool, Option<u64>) {
    match executor
        .execute(Command::EventRange {
            branch: branch.map(str::to_owned),
            space: space.map(str::to_owned),
            start_seq,
            end_seq,
            limit,
            direction,
            event_type: event_type.map(str::to_owned),
        })
        .expect("range succeeds")
    {
        Output::EventRangeResult {
            items: events,
            page,
        } => (events, page.has_more(), page.cursor().copied()),
        output => panic!("unexpected range output: {output:?}"),
    }
}

fn event_range_by_time(
    executor: &mut Executor,
    start_ts: u64,
    end_ts: Option<u64>,
    limit: Option<u64>,
    direction: EventRangeDirection,
    event_type: Option<&str>,
) -> (Vec<EventVersionedData>, bool, Option<u64>) {
    event_range_by_time_in(
        executor, None, None, start_ts, end_ts, limit, direction, event_type,
    )
}

fn event_range_by_time_in(
    executor: &mut Executor,
    branch: Option<&str>,
    space: Option<&str>,
    start_ts: u64,
    end_ts: Option<u64>,
    limit: Option<u64>,
    direction: EventRangeDirection,
    event_type: Option<&str>,
) -> (Vec<EventVersionedData>, bool, Option<u64>) {
    match executor
        .execute(Command::EventRangeByTime {
            branch: branch.map(str::to_owned),
            space: space.map(str::to_owned),
            start_ts,
            end_ts,
            limit,
            direction,
            event_type: event_type.map(str::to_owned),
        })
        .expect("time range succeeds")
    {
        Output::EventRangeResult {
            items: events,
            page,
        } => (events, page.has_more(), page.cursor().copied()),
        output => panic!("unexpected time range output: {output:?}"),
    }
}

fn event_types(
    executor: &mut Executor,
    branch: Option<&str>,
    space: Option<&str>,
    as_of: Option<u64>,
) -> Vec<String> {
    match executor
        .execute(Command::EventListTypes {
            branch: branch.map(str::to_owned),
            space: space.map(str::to_owned),
            as_of,
        })
        .expect("type list succeeds")
    {
        Output::EventTypeList { items: types, .. } => types,
        output => panic!("unexpected type list output: {output:?}"),
    }
}

fn event_list(
    executor: &mut Executor,
    event_type: Option<&str>,
    limit: Option<u64>,
    as_of: Option<u64>,
) -> Vec<EventVersionedData> {
    event_list_page(executor, event_type, limit, None, as_of).0
}

fn event_list_page(
    executor: &mut Executor,
    event_type: Option<&str>,
    limit: Option<u64>,
    after_sequence: Option<u64>,
    as_of: Option<u64>,
) -> (Vec<EventVersionedData>, bool, Option<u64>) {
    match executor
        .execute(Command::EventList {
            branch: None,
            space: None,
            event_type: event_type.map(str::to_owned),
            limit,
            after_sequence,
            as_of,
        })
        .expect("list succeeds")
    {
        Output::EventRecords {
            items: records,
            page,
        } => (records, page.has_more(), page.cursor().copied()),
        output => panic!("unexpected list output: {output:?}"),
    }
}

fn verify_chain(
    executor: &mut Executor,
    branch: Option<&str>,
    space: Option<&str>,
) -> strata_executor::EventChainVerification {
    match executor
        .execute(Command::EventVerifyChain {
            branch: branch.map(str::to_owned),
            space: space.map(str::to_owned),
        })
        .expect("chain verification succeeds")
    {
        Output::EventChainVerification(verification) => verification,
        output => panic!("unexpected chain verification output: {output:?}"),
    }
}

fn event_sequences(events: &[EventVersionedData]) -> Vec<u64> {
    events
        .iter()
        .map(|event| event.event().sequence())
        .collect()
}
