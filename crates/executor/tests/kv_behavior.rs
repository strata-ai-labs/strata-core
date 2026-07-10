//! Executor KV behavior tests.

use strata_engine::{CacheOpenOptions, Database};
use strata_executor::{
    BatchKvEntry, Bytes, Command, Executor, ExecutorErrorClass, MutationEffectKind, Output,
    VersionedValue, DEFAULT_BRANCH,
};
use tempfile::TempDir;

#[test]
fn cache_executor_runs_complete_kv_command_suite() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    run_kv_command_suite(&mut executor);
}

#[test]
fn kv_write_outputs_report_commit_receipts_and_effects() {
    let mut executor = Executor::open_cache().expect("cache executor opens");

    let created = executor
        .execute(Command::KvPut {
            branch: None,
            space: None,
            key: bytes("effect-key"),
            value: bytes("one"),
        })
        .expect("create succeeds");
    let Output::WriteResult { effect, commit, .. } = created else {
        panic!("unexpected create output: {created:?}");
    };
    assert_eq!(effect.kind(), MutationEffectKind::Created);
    assert!(effect.applied());
    assert!(!effect.matched());
    assert_eq!(effect.affected_count(), 1);
    assert_eq!(commit.put_count(), 1);
    assert_eq!(commit.delete_count(), 0);

    let updated = executor
        .execute(Command::KvPut {
            branch: None,
            space: None,
            key: bytes("effect-key"),
            value: bytes("two"),
        })
        .expect("update succeeds");
    let Output::WriteResult { effect, commit, .. } = updated else {
        panic!("unexpected update output: {updated:?}");
    };
    assert_eq!(effect.kind(), MutationEffectKind::Updated);
    assert!(effect.applied());
    assert!(effect.matched());
    assert_eq!(commit.put_count(), 1);
    assert_eq!(commit.delete_count(), 0);

    let deleted = executor
        .execute(Command::KvDelete {
            branch: None,
            space: None,
            key: bytes("effect-key"),
        })
        .expect("delete succeeds");
    let Output::DeleteResult { effect, commit, .. } = deleted else {
        panic!("unexpected delete output: {deleted:?}");
    };
    assert_eq!(effect.kind(), MutationEffectKind::Deleted);
    assert!(effect.applied());
    assert!(effect.matched());
    assert!(commit.is_some());

    let missing = executor
        .execute(Command::KvDelete {
            branch: None,
            space: None,
            key: bytes("effect-key"),
        })
        .expect("missing delete succeeds");
    let Output::DeleteResult { effect, commit, .. } = missing else {
        panic!("unexpected missing delete output: {missing:?}");
    };
    assert_eq!(effect.kind(), MutationEffectKind::NotFound);
    assert!(!effect.applied());
    assert!(!effect.matched());
    assert_eq!(effect.affected_count(), 0);
    assert!(commit.is_none());
}

#[test]
fn kv_batch_write_outputs_report_per_item_commit_receipts_and_effects() {
    let mut executor = Executor::open_cache().expect("cache executor opens");

    write(&mut executor, None, None, "batch-effect-existing", "old");

    let output = executor
        .execute(Command::KvBatchPut {
            branch: None,
            space: None,
            entries: vec![
                BatchKvEntry::new(bytes("batch-effect-created"), bytes("new")),
                BatchKvEntry::new(bytes("batch-effect-existing"), bytes("updated")),
            ],
        })
        .expect("batch put succeeds");
    let Output::BatchResults(results) = output else {
        panic!("unexpected batch put output: {output:?}");
    };
    assert_eq!(results.len(), 2);
    let created = results[0].effect().expect("created effect");
    assert_eq!(created.kind(), MutationEffectKind::Created);
    assert!(created.applied());
    assert!(!created.matched());
    let updated = results[1].effect().expect("updated effect");
    assert_eq!(updated.kind(), MutationEffectKind::Updated);
    assert!(updated.applied());
    assert!(updated.matched());
    let batch_commit = results[0].commit().expect("created commit");
    assert_eq!(batch_commit.put_count(), 2);
    assert_eq!(batch_commit.delete_count(), 0);
    assert_eq!(
        results[1].commit().expect("updated commit").version(),
        batch_commit.version()
    );

    let output = executor
        .execute(Command::KvBatchDelete {
            branch: None,
            space: None,
            keys: vec![bytes("batch-effect-created"), bytes("batch-effect-missing")],
        })
        .expect("batch delete succeeds");
    let Output::BatchResults(results) = output else {
        panic!("unexpected batch delete output: {output:?}");
    };
    assert_eq!(results.len(), 2);
    let deleted = results[0].effect().expect("deleted effect");
    assert_eq!(deleted.kind(), MutationEffectKind::Deleted);
    assert!(deleted.applied());
    assert!(deleted.matched());
    let missing = results[1].effect().expect("missing effect");
    assert_eq!(missing.kind(), MutationEffectKind::NotFound);
    assert!(!missing.applied());
    assert!(!missing.matched());
    assert!(results[0].commit().is_some());
    assert!(results[1].commit().is_none());
}

#[test]
fn durable_executor_reopens_values_lists_and_history() {
    let temp = TempDir::new().expect("temp dir");
    let path = temp.path().join("db");

    {
        let mut executor = Executor::open_durable_local(&path).expect("durable executor opens");
        run_kv_command_suite(&mut executor);
        executor.close().expect("durable executor closes");
    }

    let mut reopened = Executor::open_durable_local(&path).expect("durable executor reopens");
    assert_eq!(
        execute_get(&mut reopened, "alpha"),
        Some(bytes("one-updated"))
    );
    assert_eq!(execute_count(&mut reopened, None), 6);
    assert_history_has_tombstone(&mut reopened, "delete-me");
}

#[test]
fn branch_and_space_defaults_are_isolated() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    executor
        .create_branch_from_head(DEFAULT_BRANCH, "feature")
        .expect("branch creates");

    write(&mut executor, None, None, "shared", "default-branch");
    write(
        &mut executor,
        Some("feature"),
        None,
        "shared",
        "feature-branch",
    );
    write(&mut executor, None, Some("tenant-a"), "shared", "space-a");

    assert_eq!(
        execute_get(&mut executor, "shared"),
        Some(bytes("default-branch"))
    );
    assert_eq!(
        execute_get_in(&mut executor, Some("feature"), None, "shared"),
        Some(bytes("feature-branch"))
    );
    assert_eq!(
        execute_get_in(&mut executor, None, Some("tenant-a"), "shared"),
        Some(bytes("space-a"))
    );
}

#[test]
fn executor_inherits_configured_database_default_branch() {
    let options = CacheOpenOptions::new()
        .with_default_branch("main")
        .expect("valid branch");
    let database = Database::open_cache(options)
        .expect("cache database opens")
        .into_database();
    let mut executor = Executor::from_database(database);

    assert_eq!(executor.default_branch(), "main");
    write(&mut executor, None, None, "shared", "main-value");
    assert_eq!(
        execute_get(&mut executor, "shared"),
        Some(bytes("main-value"))
    );

    let error = executor
        .execute(Command::KvGet {
            branch: Some(DEFAULT_BRANCH.to_owned()),
            space: None,
            key: bytes("shared"),
            as_of: None,
        })
        .expect_err("literal default branch is absent");
    assert_eq!(error.class(), ExecutorErrorClass::NotFound);
}

#[test]
fn branch_commands_delegate_to_engine_branch_service() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    write(&mut executor, None, None, "shared", "base");

    let created = executor
        .execute(Command::BranchCreate {
            branch: "scratch".to_owned(),
        })
        .expect("branch create succeeds");
    let Output::Branch(created) = created else {
        panic!("branch create output");
    };
    assert_eq!(created.name(), "scratch");
    assert!(created.parent().is_none());

    let forked = executor
        .execute(Command::BranchForkCurrent {
            source: DEFAULT_BRANCH.to_owned(),
            branch: "feature".to_owned(),
        })
        .expect("branch fork succeeds");
    let Output::Branch(forked) = forked else {
        panic!("branch fork output");
    };
    assert_eq!(forked.name(), "feature");
    assert_eq!(
        forked.parent().expect("parent facts").name(),
        DEFAULT_BRANCH
    );
    assert_eq!(
        execute_get_in(&mut executor, Some("feature"), None, "shared"),
        Some(bytes("base"))
    );

    let listed = executor
        .execute(Command::BranchList)
        .expect("branch list succeeds");
    let Output::Branches {
        items: branches, ..
    } = listed
    else {
        panic!("branch list output");
    };
    assert!(branches
        .iter()
        .any(|branch| branch.name() == DEFAULT_BRANCH));
    assert!(branches.iter().any(|branch| branch.name() == "scratch"));
    assert!(branches.iter().any(|branch| branch.name() == "feature"));

    let deleted = executor
        .execute(Command::BranchDelete {
            branch: "scratch".to_owned(),
        })
        .expect("branch delete succeeds");
    let Output::BranchDeleteResult {
        deleted,
        effect,
        branch,
        ..
    } = deleted
    else {
        panic!("branch delete output");
    };
    assert!(deleted);
    assert_eq!(effect.kind(), MutationEffectKind::Deleted);
    assert!(effect.applied());
    assert!(effect.matched());
    assert_eq!(branch.name(), "scratch");

    let error = executor
        .execute(Command::KvPut {
            branch: Some("scratch".to_owned()),
            space: None,
            key: bytes("blocked"),
            value: bytes("blocked"),
        })
        .expect_err("deleted branch write fails");
    assert_eq!(error.class(), ExecutorErrorClass::NotFound);
}

#[test]
fn command_to_output_mapping_is_explicit_for_every_variant() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    write(&mut executor, None, None, "map-a", "one");
    write(&mut executor, None, None, "map-b", "two");

    let commands = vec![
        Command::KvPut {
            branch: None,
            space: None,
            key: bytes("map-put"),
            value: bytes("value"),
        },
        Command::KvGet {
            branch: None,
            space: None,
            key: bytes("map-a"),
            as_of: None,
        },
        Command::KvDelete {
            branch: None,
            space: None,
            key: bytes("map-delete-missing"),
        },
        Command::KvList {
            branch: None,
            space: None,
            prefix: Some(bytes("map-")),
            cursor: None,
            limit: None,
            as_of: None,
        },
        Command::KvScan {
            branch: None,
            space: None,
            start: Some(bytes("map-")),
            limit: Some(2),
        },
        Command::KvBatchPut {
            branch: None,
            space: None,
            entries: vec![BatchKvEntry::new(bytes("map-c"), bytes("three"))],
        },
        Command::KvBatchGet {
            branch: None,
            space: None,
            keys: vec![bytes("map-a"), bytes("missing")],
        },
        Command::KvBatchDelete {
            branch: None,
            space: None,
            keys: vec![bytes("map-c"), bytes("missing")],
        },
        Command::KvBatchExists {
            branch: None,
            space: None,
            keys: vec![bytes("map-a"), bytes("missing")],
        },
        Command::KvExists {
            branch: None,
            space: None,
            key: bytes("map-a"),
        },
        Command::KvGetv {
            branch: None,
            space: None,
            key: bytes("map-a"),
        },
        Command::KvCount {
            branch: None,
            space: None,
            prefix: Some(bytes("map-")),
        },
        Command::KvSample {
            branch: None,
            space: None,
            prefix: Some(bytes("map-")),
            count: Some(1),
        },
    ];

    let outputs = commands
        .into_iter()
        .map(|command| executor.execute(command).expect("command succeeds"))
        .collect::<Vec<_>>();

    assert!(matches!(outputs[0], Output::WriteResult { .. }));
    assert!(matches!(outputs[1], Output::KvVersionedValue(_)));
    assert!(matches!(outputs[2], Output::DeleteResult { .. }));
    assert!(matches!(outputs[3], Output::Keys { .. }));
    assert!(matches!(outputs[4], Output::KvScanResult { .. }));
    assert!(matches!(outputs[5], Output::BatchResults(_)));
    assert!(matches!(outputs[6], Output::BatchGetResults(_)));
    assert!(matches!(outputs[7], Output::BatchResults(_)));
    assert!(matches!(outputs[8], Output::BoolList(_)));
    assert!(matches!(outputs[9], Output::Bool(_)));
    assert!(matches!(outputs[10], Output::VersionHistory(_)));
    assert!(matches!(outputs[11], Output::Uint(_)));
    assert!(matches!(outputs[12], Output::SampleResult { .. }));
}

#[test]
fn duplicate_batch_writes_fail_before_partial_application() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    let error = executor
        .execute(Command::KvBatchPut {
            branch: None,
            space: None,
            entries: vec![
                BatchKvEntry::new(bytes("dupe"), bytes("one")),
                BatchKvEntry::new(bytes("dupe"), bytes("two")),
            ],
        })
        .expect_err("duplicate batch put fails");

    assert_eq!(error.class(), ExecutorErrorClass::InvalidInput);
    assert!(execute_get(&mut executor, "dupe").is_none());
}

#[test]
fn empty_batches_return_empty_outputs() {
    let mut executor = Executor::open_cache().expect("cache executor opens");

    assert!(matches!(
        executor
            .execute(Command::KvBatchPut {
                branch: None,
                space: None,
                entries: Vec::new(),
            })
            .expect("empty batch put succeeds"),
        Output::BatchResults(results) if results.is_empty() && !results.applied()
    ));
    assert!(matches!(
        executor
            .execute(Command::KvBatchDelete {
                branch: None,
                space: None,
                keys: Vec::new(),
            })
            .expect("empty batch delete succeeds"),
        Output::BatchResults(results) if results.is_empty() && !results.applied()
    ));
    assert!(matches!(
        executor
            .execute(Command::KvBatchGet {
                branch: None,
                space: None,
                keys: Vec::new(),
            })
            .expect("empty batch get succeeds"),
        Output::BatchGetResults(results) if results.is_empty() && !results.applied()
    ));
}

#[test]
fn invalid_batch_items_are_positional_errors() {
    let mut executor = Executor::open_cache().expect("cache executor opens");

    let output = executor
        .execute(Command::KvBatchPut {
            branch: None,
            space: None,
            entries: vec![
                BatchKvEntry::new(Bytes::new(Vec::new()), bytes("bad")),
                BatchKvEntry::new(bytes("valid"), bytes("good")),
            ],
        })
        .expect("batch put returns positional errors");
    let Output::BatchResults(results) = output else {
        panic!("unexpected batch put output: {output:?}");
    };
    assert_eq!(results.len(), 2);
    assert!(!results[0].applied());
    assert!(results[0].error().is_some());
    assert_eq!(
        results[0].error_status().expect("item error status").code(),
        "invalid_argument.engine.kv_key"
    );
    assert!(results[1].applied());
    assert_eq!(execute_get(&mut executor, "valid"), Some(bytes("good")));

    let output = executor
        .execute(Command::KvBatchGet {
            branch: None,
            space: None,
            keys: vec![Bytes::new(Vec::new()), bytes("valid")],
        })
        .expect("batch get returns positional errors");
    let Output::BatchGetResults(results) = output else {
        panic!("unexpected batch get output: {output:?}");
    };
    assert_eq!(results.len(), 2);
    assert!(results[0].error().is_some());
    assert_eq!(
        results[0].error_status().expect("item error status").code(),
        "invalid_argument.engine.kv_key"
    );
    assert_eq!(results[1].value(), Some(&bytes("good")));

    let output = executor
        .execute(Command::KvBatchDelete {
            branch: None,
            space: None,
            keys: vec![Bytes::new(Vec::new()), bytes("valid")],
        })
        .expect("batch delete returns positional errors");
    let Output::BatchResults(results) = output else {
        panic!("unexpected batch delete output: {output:?}");
    };
    assert_eq!(results.len(), 2);
    assert!(results[0].error().is_some());
    assert_eq!(
        results[0].error_status().expect("item error status").code(),
        "invalid_argument.engine.kv_key"
    );
    assert!(results[1].applied());
    assert!(execute_get(&mut executor, "valid").is_none());
}

#[test]
fn batch_commands_validate_branch_before_returning_item_results() {
    let mut executor = Executor::open_cache().expect("cache executor opens");

    for command in [
        Command::KvBatchPut {
            branch: Some("missing".to_owned()),
            space: None,
            entries: Vec::new(),
        },
        Command::KvBatchPut {
            branch: Some("missing".to_owned()),
            space: None,
            entries: vec![BatchKvEntry::new(Bytes::new(Vec::new()), bytes("bad"))],
        },
        Command::KvBatchGet {
            branch: Some("missing".to_owned()),
            space: None,
            keys: Vec::new(),
        },
        Command::KvBatchDelete {
            branch: Some("missing".to_owned()),
            space: None,
            keys: Vec::new(),
        },
        Command::KvBatchExists {
            branch: Some("missing".to_owned()),
            space: None,
            keys: Vec::new(),
        },
    ] {
        let error = executor.execute(command).expect_err("missing branch fails");
        assert_eq!(error.class(), ExecutorErrorClass::NotFound);
    }
}

fn run_kv_command_suite(executor: &mut Executor) {
    let first = write(executor, None, None, "alpha", "one");
    let second = write(executor, None, None, "alpha", "one-updated");
    assert!(second.version > first.version);

    write(executor, None, None, "bravo", "two");
    write(executor, None, None, "prefix-a", "three");
    write(executor, None, None, "prefix-b", "four");
    write(executor, None, None, "delete-me", "gone");

    assert_eq!(execute_get(executor, "alpha"), Some(bytes("one-updated")));
    let first_as_of =
        execute_get_as_of(executor, "alpha", first.timestamp).expect("historical value exists");
    assert_eq!(first_as_of.value(), &bytes("one"));
    assert_eq!(first_as_of.version(), first.version);
    assert_eq!(first_as_of.timestamp(), first.timestamp);
    assert!(execute_exists(executor, "alpha"));
    assert!(!execute_exists(executor, "missing"));

    assert_eq!(
        execute_list(executor, Some("prefix-")),
        vec![bytes("prefix-a"), bytes("prefix-b")]
    );
    assert_eq!(
        execute_list_page(executor, Some("prefix-"), None, 1),
        (vec![bytes("prefix-a")], true)
    );
    assert_eq!(
        execute_list_as_of(executor, None, first.timestamp),
        vec![bytes("alpha")]
    );

    assert_eq!(
        execute_scan(executor, Some("prefix-"), Some(10)),
        vec![
            (bytes("prefix-a"), bytes("three")),
            (bytes("prefix-b"), bytes("four"))
        ]
    );

    batch_put(
        executor,
        vec![
            ("batch-a", "five"),
            ("batch-b", "six"),
            ("sample-a", "seven"),
        ],
    );
    assert_eq!(
        execute_batch_get(executor, vec!["batch-a", "missing", "batch-b"]),
        vec![Some(bytes("five")), None, Some(bytes("six"))]
    );
    assert_eq!(
        execute_batch_exists(executor, vec!["batch-a", "missing", "batch-b"]),
        vec![true, false, true]
    );

    let deleted = execute_delete(executor, "delete-me");
    assert!(deleted);
    let missing_deleted = execute_delete(executor, "delete-me");
    assert!(!missing_deleted);
    assert_history_has_tombstone(executor, "delete-me");

    assert_eq!(execute_count(executor, Some("batch-")), 2);
    let sample = execute_sample(executor, Some("batch-"), 1);
    assert_eq!(sample.0, 2);
    assert_eq!(sample.1.len(), 1);
    assert!(sample.1[0].as_slice().starts_with(b"batch-"));

    let results = execute_batch_delete(executor, vec!["batch-a", "missing"]);
    assert_eq!(results, vec![true, false]);
    assert!(!execute_exists(executor, "batch-a"));
}

fn write(
    executor: &mut Executor,
    branch: Option<&str>,
    space: Option<&str>,
    key: &str,
    value: &str,
) -> WriteFacts {
    match executor
        .execute(Command::KvPut {
            branch: branch.map(str::to_owned),
            space: space.map(str::to_owned),
            key: bytes(key),
            value: bytes(value),
        })
        .expect("put succeeds")
    {
        Output::WriteResult { effect, commit, .. } => {
            assert!(effect.applied());
            WriteFacts {
                version: commit.version(),
                timestamp: commit.timestamp(),
            }
        }
        output => panic!("unexpected put output: {output:?}"),
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct WriteFacts {
    version: u64,
    timestamp: u64,
}

fn batch_put(executor: &mut Executor, entries: Vec<(&str, &str)>) {
    let entries = entries
        .into_iter()
        .map(|(key, value)| BatchKvEntry::new(bytes(key), bytes(value)))
        .collect();
    match executor
        .execute(Command::KvBatchPut {
            branch: None,
            space: None,
            entries,
        })
        .expect("batch put succeeds")
    {
        Output::BatchResults(results) => {
            assert!(results.iter().all(strata_executor::BatchItem::applied));
        }
        output => panic!("unexpected batch put output: {output:?}"),
    }
}

#[test]
fn commands_resolve_omitted_space_from_the_executor_session_default() {
    // CLI-4: the executor owns space session context. A command that omits its
    // space resolves to the session default — the mechanism `command run`
    // relies on so raw JSON honors --space — not the literal "default" space.
    let mut executor = Executor::open_cache()
        .expect("cache executor opens")
        .with_default_space("app")
        .expect("session space set");
    assert_eq!(executor.default_space(), "app");

    executor
        .execute(Command::KvPut {
            branch: None,
            space: None,
            key: bytes("k"),
            value: bytes("v"),
        })
        .expect("put succeeds");

    // The write landed in the session space, not the literal "default" space.
    assert_eq!(
        execute_get_in(&mut executor, None, Some("app"), "k"),
        Some(bytes("v"))
    );
    assert_eq!(
        execute_get_in(&mut executor, None, Some("default"), "k"),
        None
    );
    // A read that also omits the space resolves to "app" and finds it.
    assert_eq!(execute_get(&mut executor, "k"), Some(bytes("v")));
}

fn execute_get(executor: &mut Executor, key: &str) -> Option<Bytes> {
    match executor
        .execute(Command::KvGet {
            branch: None,
            space: None,
            key: bytes(key),
            as_of: None,
        })
        .expect("get succeeds")
    {
        Output::KvVersionedValue(Some(value)) => Some(value.value().clone()),
        Output::KvVersionedValue(None) => None,
        output => panic!("unexpected get output: {output:?}"),
    }
}

fn execute_get_in(
    executor: &mut Executor,
    branch: Option<&str>,
    space: Option<&str>,
    key: &str,
) -> Option<Bytes> {
    match executor
        .execute(Command::KvGet {
            branch: branch.map(str::to_owned),
            space: space.map(str::to_owned),
            key: bytes(key),
            as_of: None,
        })
        .expect("get succeeds")
    {
        Output::KvVersionedValue(Some(value)) => Some(value.value().clone()),
        Output::KvVersionedValue(None) => None,
        output => panic!("unexpected get output: {output:?}"),
    }
}

fn execute_get_as_of(executor: &mut Executor, key: &str, as_of: u64) -> Option<VersionedValue> {
    match executor
        .execute(Command::KvGet {
            branch: None,
            space: None,
            key: bytes(key),
            as_of: Some(as_of),
        })
        .expect("historical get succeeds")
    {
        Output::KvVersionedValue(value) => value,
        output => panic!("unexpected historical get output: {output:?}"),
    }
}

fn execute_delete(executor: &mut Executor, key: &str) -> bool {
    match executor
        .execute(Command::KvDelete {
            branch: None,
            space: None,
            key: bytes(key),
        })
        .expect("delete succeeds")
    {
        Output::DeleteResult { effect, .. } => effect.applied(),
        output => panic!("unexpected delete output: {output:?}"),
    }
}

fn execute_list(executor: &mut Executor, prefix: Option<&str>) -> Vec<Bytes> {
    match executor
        .execute(Command::KvList {
            branch: None,
            space: None,
            prefix: prefix.map(bytes),
            cursor: None,
            limit: None,
            as_of: None,
        })
        .expect("list succeeds")
    {
        Output::Keys { items: keys, .. } => keys,
        output => panic!("unexpected list output: {output:?}"),
    }
}

fn execute_list_page(
    executor: &mut Executor,
    prefix: Option<&str>,
    cursor: Option<&str>,
    limit: u64,
) -> (Vec<Bytes>, bool) {
    match executor
        .execute(Command::KvList {
            branch: None,
            space: None,
            prefix: prefix.map(bytes),
            cursor: cursor.map(bytes),
            limit: Some(limit),
            as_of: None,
        })
        .expect("list page succeeds")
    {
        Output::KeysPage { items: keys, page } => (keys, page.has_more()),
        output => panic!("unexpected list page output: {output:?}"),
    }
}

fn execute_list_as_of(executor: &mut Executor, prefix: Option<&str>, as_of: u64) -> Vec<Bytes> {
    match executor
        .execute(Command::KvList {
            branch: None,
            space: None,
            prefix: prefix.map(bytes),
            cursor: None,
            limit: None,
            as_of: Some(as_of),
        })
        .expect("historical list succeeds")
    {
        Output::Keys { items: keys, .. } => keys,
        output => panic!("unexpected historical list output: {output:?}"),
    }
}

#[test]
fn kv_scan_paginates_honestly_with_a_cursor() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    for i in 0..6 {
        executor
            .execute(Command::KvPut {
                branch: None,
                space: None,
                key: bytes(&format!("k{i}")),
                value: bytes("v"),
            })
            .expect("put succeeds");
    }

    // DSGN-2: page through in chunks of 2. The union of pages must be all six
    // keys with no overlap and no gap, and only the final page ends (cursor
    // None). Previously every scan lied with a terminal page (cursor None),
    // truncating callers to the first page.
    let mut seen: Vec<Vec<u8>> = Vec::new();
    let mut start: Option<Bytes> = None;
    let mut pages = 0;
    loop {
        let (keys, cursor) = scan_page(&mut executor, start, 2);
        assert!(keys.len() <= 2);
        seen.extend(keys.iter().map(|key| key.as_slice().to_vec()));
        pages += 1;
        assert!(pages <= 6, "pagination did not terminate");
        match cursor {
            Some(next) => start = Some(next),
            None => break,
        }
    }
    assert_eq!(pages, 3);
    seen.sort();
    let mut deduped = seen.clone();
    deduped.dedup();
    assert_eq!(deduped.len(), seen.len(), "no duplicate keys across pages");
    let mut expected: Vec<Vec<u8>> = (0..6).map(|i| format!("k{i}").into_bytes()).collect();
    expected.sort();
    assert_eq!(seen, expected);
}

fn scan_page(
    executor: &mut Executor,
    start: Option<Bytes>,
    limit: u64,
) -> (Vec<Bytes>, Option<Bytes>) {
    match executor
        .execute(Command::KvScan {
            branch: None,
            space: None,
            start,
            limit: Some(limit),
        })
        .expect("scan succeeds")
    {
        Output::KvScanResult { items, page } => (
            items.iter().map(|item| item.key().clone()).collect(),
            page.cursor().cloned(),
        ),
        output => panic!("unexpected scan output: {output:?}"),
    }
}

fn execute_scan(
    executor: &mut Executor,
    start: Option<&str>,
    limit: Option<u64>,
) -> Vec<(Bytes, Bytes)> {
    match executor
        .execute(Command::KvScan {
            branch: None,
            space: None,
            start: start.map(bytes),
            limit,
        })
        .expect("scan succeeds")
    {
        Output::KvScanResult { items: rows, .. } => rows
            .into_iter()
            .take_while(|row| row.key().as_slice().starts_with(b"prefix-"))
            .map(|row| (row.key().clone(), row.value().clone()))
            .collect(),
        output => panic!("unexpected scan output: {output:?}"),
    }
}

fn execute_batch_get(executor: &mut Executor, keys: Vec<&str>) -> Vec<Option<Bytes>> {
    match executor
        .execute(Command::KvBatchGet {
            branch: None,
            space: None,
            keys: keys.into_iter().map(bytes).collect(),
        })
        .expect("batch get succeeds")
    {
        Output::BatchGetResults(results) => results
            .into_iter()
            .map(|result| result.value().cloned())
            .collect(),
        output => panic!("unexpected batch get output: {output:?}"),
    }
}

fn execute_batch_delete(executor: &mut Executor, keys: Vec<&str>) -> Vec<bool> {
    match executor
        .execute(Command::KvBatchDelete {
            branch: None,
            space: None,
            keys: keys.into_iter().map(bytes).collect(),
        })
        .expect("batch delete succeeds")
    {
        Output::BatchResults(results) => {
            results.into_iter().map(|result| result.applied()).collect()
        }
        output => panic!("unexpected batch delete output: {output:?}"),
    }
}

fn execute_batch_exists(executor: &mut Executor, keys: Vec<&str>) -> Vec<bool> {
    match executor
        .execute(Command::KvBatchExists {
            branch: None,
            space: None,
            keys: keys.into_iter().map(bytes).collect(),
        })
        .expect("batch exists succeeds")
    {
        Output::BoolList(values) => values,
        output => panic!("unexpected batch exists output: {output:?}"),
    }
}

fn execute_exists(executor: &mut Executor, key: &str) -> bool {
    match executor
        .execute(Command::KvExists {
            branch: None,
            space: None,
            key: bytes(key),
        })
        .expect("exists succeeds")
    {
        Output::Bool(value) => value,
        output => panic!("unexpected exists output: {output:?}"),
    }
}

fn execute_count(executor: &mut Executor, prefix: Option<&str>) -> u64 {
    match executor
        .execute(Command::KvCount {
            branch: None,
            space: None,
            prefix: prefix.map(bytes),
        })
        .expect("count succeeds")
    {
        Output::Uint(count) => count,
        output => panic!("unexpected count output: {output:?}"),
    }
}

fn execute_sample(executor: &mut Executor, prefix: Option<&str>, count: u64) -> (u64, Vec<Bytes>) {
    match executor
        .execute(Command::KvSample {
            branch: None,
            space: None,
            prefix: prefix.map(bytes),
            count: Some(count),
        })
        .expect("sample succeeds")
    {
        Output::SampleResult {
            total_count, items, ..
        } => (
            total_count,
            items.into_iter().map(|item| item.key().clone()).collect(),
        ),
        output => panic!("unexpected sample output: {output:?}"),
    }
}

fn assert_history_has_tombstone(executor: &mut Executor, key: &str) {
    match executor
        .execute(Command::KvGetv {
            branch: None,
            space: None,
            key: bytes(key),
        })
        .expect("history succeeds")
    {
        Output::VersionHistory(Some(history)) => assert!(
            history
                .items()
                .iter()
                .any(strata_executor::HistoryItem::is_tombstone),
            "history should include a tombstone"
        ),
        output => panic!("unexpected history output: {output:?}"),
    }
}

fn bytes(value: &str) -> Bytes {
    Bytes::from(value)
}
