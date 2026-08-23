//! Executor-boundary branch command behavior (TCP3.9b).
//!
//! The branch commands (create / list / get / fork-current / fork-at-version /
//! fork-at-timestamp / delete) had no focused executor test — they were only
//! used as setup in the data-command suites. This pins their outputs and their
//! fork/delete semantics at the executor boundary, and checks the branch
//! convenience facade against the explicit commands.

use strata_executor::{
    BranchComparisonItem, Bytes, Command, ComparedCapability, Executor, ExecutorErrorClass, Output,
    SpaceComparisonItem, DEFAULT_BRANCH,
};

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
        .execute(Command::BranchList {})
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
    same!(facade.branch_list(), Command::BranchList {});
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

fn diff(
    executor: &mut Executor,
    branch_a: &str,
    branch_b: &str,
    at_timestamp: Option<u64>,
) -> BranchComparisonItem {
    match executor
        .execute(Command::BranchDiff {
            branch_a: branch_a.to_owned(),
            branch_b: branch_b.to_owned(),
            at_timestamp,
        })
        .expect("diff succeeds")
    {
        Output::BranchComparison(comparison) => comparison,
        output => panic!("unexpected diff output: {output:?}"),
    }
}

fn kv_space<'a>(comparison: &'a BranchComparisonItem, space: &str) -> &'a SpaceComparisonItem {
    comparison
        .spaces()
        .iter()
        .find(|entry| entry.capability() == ComparedCapability::KeyValue && entry.space() == space)
        .expect("a key-value diff for the space")
}

#[test]
fn branch_diff_reports_changes_and_honors_as_of() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    executor
        .execute(Command::BranchForkCurrent {
            source: "default".to_owned(),
            branch: "feature".to_owned(),
        })
        .expect("fork succeeds");

    // Feature writes first; default writes only afterward, then changes.
    let (_, feature_timestamp) = put(&mut executor, Some("feature"), "k", "same");
    put(&mut executor, Some("default"), "k", "same");
    put(&mut executor, Some("default"), "k", "changed");

    // Current: default k=changed vs feature k=same -> modified.
    let current = diff(&mut executor, "default", "feature", None);
    let kv_now = kv_space(&current, "default");
    assert_eq!(kv_now.modified().len(), 1);
    assert_eq!(kv_now.modified()[0].identity().as_slice(), b"k");
    assert!(kv_now.added().is_empty());

    // As of the feature write's timestamp, default has no `k` yet -> added.
    let past = diff(&mut executor, "default", "feature", Some(feature_timestamp));
    let kv_past = kv_space(&past, "default");
    assert_eq!(kv_past.added().len(), 1);
    assert_eq!(kv_past.added()[0].identity().as_slice(), b"k");
    assert!(kv_past.modified().is_empty());
}
