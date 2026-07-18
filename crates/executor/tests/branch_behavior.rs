//! Executor-boundary branch command behavior (TCP3.9b).
//!
//! The branch commands (create / list / get / fork-current / fork-at-version /
//! fork-at-timestamp / delete) had no focused executor test — they were only
//! used as setup in the data-command suites. This pins their outputs and their
//! fork/delete semantics at the executor boundary, and checks the branch
//! convenience facade against the explicit commands.

use strata_executor::{Bytes, Command, Executor, ExecutorErrorClass, Output, DEFAULT_BRANCH};

fn bytes(value: &str) -> Bytes {
    Bytes::from(value)
}

/// Puts `value` at `key` on `branch` (or the default when `None`); returns the
/// commit's (version, timestamp-micros).
fn put(executor: &mut Executor, branch: Option<&str>, key: &str, value: &str) -> (u64, u64) {
    match executor
        .execute(Command::KvPut {
            branch: branch.map(str::to_owned),
            space: None,
            key: bytes(key),
            value: bytes(value),
        })
        .expect("put succeeds")
    {
        Output::WriteResult { commit, .. } => (commit.version(), commit.timestamp()),
        output => panic!("unexpected put output: {output:?}"),
    }
}

fn get(executor: &mut Executor, branch: Option<&str>, key: &str) -> Option<Bytes> {
    match executor
        .execute(Command::KvGet {
            branch: branch.map(str::to_owned),
            space: None,
            key: bytes(key),
            as_of: None,
        })
        .expect("get succeeds")
    {
        Output::KvVersionedValue(value) => value.into_option().map(|v| v.value().clone()),
        output => panic!("unexpected get output: {output:?}"),
    }
}

fn branch_names(executor: &mut Executor) -> Vec<String> {
    match executor
        .execute(Command::BranchList)
        .expect("list succeeds")
    {
        Output::Branches { items, .. } => items.iter().map(|item| item.name().to_owned()).collect(),
        output => panic!("unexpected list output: {output:?}"),
    }
}

#[test]
fn branch_create_list_get_delete_lifecycle() {
    let mut executor = Executor::open_cache().expect("cache executor opens");

    let Output::Branch(created) = executor
        .execute(Command::BranchCreate {
            branch: "feature".to_owned(),
        })
        .expect("create succeeds")
    else {
        panic!("unexpected create output");
    };
    assert_eq!(created.name(), "feature");

    let names = branch_names(&mut executor);
    assert!(names.iter().any(|name| name == DEFAULT_BRANCH));
    assert!(names.iter().any(|name| name == "feature"));

    let Output::Branch(fetched) = executor
        .execute(Command::BranchGet {
            branch: "feature".to_owned(),
        })
        .expect("get succeeds")
    else {
        panic!("unexpected get output");
    };
    assert_eq!(fetched.name(), "feature");

    let Output::BranchDeleteResult { deleted, .. } = executor
        .execute(Command::BranchDelete {
            branch: "feature".to_owned(),
        })
        .expect("delete succeeds")
    else {
        panic!("unexpected delete output");
    };
    assert!(deleted);

    let error = executor
        .execute(Command::BranchGet {
            branch: "feature".to_owned(),
        })
        .expect_err("get on a deleted branch fails");
    assert_eq!(error.class(), ExecutorErrorClass::NotFound);
    assert_eq!(error.code(), "not_found.engine.branch");

    assert!(!branch_names(&mut executor)
        .iter()
        .any(|name| name == "feature"));
}

#[test]
fn fork_current_inherits_and_isolates() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    put(&mut executor, None, "k", "parent");

    executor
        .execute(Command::BranchForkCurrent {
            source: DEFAULT_BRANCH.to_owned(),
            branch: "feature".to_owned(),
        })
        .expect("fork current succeeds");

    // Child inherits the source state at fork time.
    assert_eq!(
        get(&mut executor, Some("feature"), "k"),
        Some(bytes("parent"))
    );

    // A child write is isolated from the parent, both ways.
    put(&mut executor, Some("feature"), "k", "child");
    assert_eq!(
        get(&mut executor, Some("feature"), "k"),
        Some(bytes("child"))
    );
    assert_eq!(get(&mut executor, None, "k"), Some(bytes("parent")));

    // A later parent write is not visible on the already-forked child.
    put(&mut executor, None, "later", "parent-only");
    assert_eq!(get(&mut executor, Some("feature"), "later"), None);
}

#[test]
fn fork_at_version_reads_source_as_of() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    let (v1, _) = put(&mut executor, None, "k", "one");
    put(&mut executor, None, "k", "two");

    executor
        .execute(Command::BranchForkAtVersion {
            source: DEFAULT_BRANCH.to_owned(),
            branch: "snapshot".to_owned(),
            version: v1,
        })
        .expect("fork at version succeeds");

    assert_eq!(
        get(&mut executor, Some("snapshot"), "k"),
        Some(bytes("one"))
    );
    assert_eq!(get(&mut executor, None, "k"), Some(bytes("two")));
}

#[test]
fn fork_at_timestamp_reads_source_as_of() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    let (_, ts1) = put(&mut executor, None, "k", "one");
    put(&mut executor, None, "k", "two");

    executor
        .execute(Command::BranchForkAtTimestamp {
            source: DEFAULT_BRANCH.to_owned(),
            branch: "snapshot".to_owned(),
            timestamp: ts1,
        })
        .expect("fork at timestamp succeeds");

    assert_eq!(
        get(&mut executor, Some("snapshot"), "k"),
        Some(bytes("one"))
    );
    assert_eq!(get(&mut executor, None, "k"), Some(bytes("two")));
}

#[test]
fn branch_facade_matches_explicit_commands() {
    let mut facade = Executor::open_cache().expect("facade executor opens");
    let mut direct = Executor::open_cache().expect("direct executor opens");

    macro_rules! same {
        ($facade_call:expr, $command:expr) => {{
            let facade_output = $facade_call.expect("facade call succeeds");
            let direct_output = direct.execute($command).expect("explicit command succeeds");
            assert_eq!(
                facade_output, direct_output,
                "branch facade output must equal the explicit command"
            );
        }};
    }

    same!(
        facade.branch_create("feature"),
        Command::BranchCreate {
            branch: "feature".to_owned(),
        }
    );
    same!(facade.branch_list(), Command::BranchList);
    same!(
        facade.branch_get("feature"),
        Command::BranchGet {
            branch: "feature".to_owned(),
        }
    );
    same!(
        facade.branch_fork_current(DEFAULT_BRANCH, "child"),
        Command::BranchForkCurrent {
            source: DEFAULT_BRANCH.to_owned(),
            branch: "child".to_owned(),
        }
    );
    same!(
        facade.branch_delete("child"),
        Command::BranchDelete {
            branch: "child".to_owned(),
        }
    );
}
