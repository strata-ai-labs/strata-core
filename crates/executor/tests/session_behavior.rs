//! Executor session-default behavior (TCP3.9b).
//!
//! Data commands carry `branch`/`space` as `Option<String>`. Omitting them
//! (`None`) must resolve to the default branch and default space, identically
//! to naming them explicitly, and an operation on a branch that does not exist
//! must be rejected rather than silently creating or ignoring it.

use strata_executor::{Bytes, Command, Executor, ExecutorErrorClass, Output, DEFAULT_BRANCH};

const DEFAULT_SPACE: &str = "default";

fn bytes(value: &str) -> Bytes {
    Bytes::from(value)
}

fn put(executor: &mut Executor, branch: Option<&str>, space: Option<&str>, key: &str, value: &str) {
    match executor
        .execute(Command::KvPut {
            branch: branch.map(str::to_owned),
            space: space.map(str::to_owned),
            key: bytes(key),
            value: bytes(value),
        })
        .expect("put succeeds")
    {
        Output::WriteResult { .. } => {}
        output => panic!("unexpected put output: {output:?}"),
    }
}

fn get(
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
        Output::KvVersionedValue(value) => value.into_option().map(|v| v.value().clone()),
        output => panic!("unexpected get output: {output:?}"),
    }
}

#[test]
fn omitted_branch_resolves_to_default() {
    let mut executor = Executor::open_cache().expect("cache executor opens");

    // Written with the branch omitted, read back with the default named.
    put(&mut executor, None, None, "a", "one");
    assert_eq!(
        get(&mut executor, Some(DEFAULT_BRANCH), None, "a"),
        Some(bytes("one"))
    );

    // Written with the default named, read back with the branch omitted.
    put(&mut executor, Some(DEFAULT_BRANCH), None, "b", "two");
    assert_eq!(get(&mut executor, None, None, "b"), Some(bytes("two")));
}

#[test]
fn omitted_space_resolves_to_default() {
    let mut executor = Executor::open_cache().expect("cache executor opens");

    put(&mut executor, None, None, "a", "one");
    assert_eq!(
        get(&mut executor, None, Some(DEFAULT_SPACE), "a"),
        Some(bytes("one"))
    );

    put(&mut executor, None, Some(DEFAULT_SPACE), "b", "two");
    assert_eq!(get(&mut executor, None, None, "b"), Some(bytes("two")));
}

#[test]
fn omitted_and_explicit_defaults_produce_equal_output() {
    let mut omitted = Executor::open_cache().expect("omitted executor opens");
    let mut explicit = Executor::open_cache().expect("explicit executor opens");

    let omitted_output = omitted
        .execute(Command::KvPut {
            branch: None,
            space: None,
            key: bytes("k"),
            value: bytes("v"),
        })
        .expect("omitted put succeeds");
    let explicit_output = explicit
        .execute(Command::KvPut {
            branch: Some(DEFAULT_BRANCH.to_owned()),
            space: Some(DEFAULT_SPACE.to_owned()),
            key: bytes("k"),
            value: bytes("v"),
        })
        .expect("explicit put succeeds");
    // `committed_at` is a per-commit wall-clock instant (#3112), so two separate
    // commits differ there; every other fact must be identical whether the
    // defaults are omitted or named. Compare modulo that one field.
    let Output::WriteResult {
        key: omitted_key,
        effect: omitted_effect,
        commit: omitted_commit,
    } = &omitted_output
    else {
        panic!("unexpected omitted output: {omitted_output:?}");
    };
    let Output::WriteResult {
        key: explicit_key,
        effect: explicit_effect,
        commit: explicit_commit,
    } = &explicit_output
    else {
        panic!("unexpected explicit output: {explicit_output:?}");
    };
    assert_eq!(omitted_key, explicit_key);
    assert_eq!(omitted_effect, explicit_effect);
    assert_eq!(omitted_commit.version(), explicit_commit.version());
    assert_eq!(omitted_commit.timestamp(), explicit_commit.timestamp());
    assert_eq!(omitted_commit.durability(), explicit_commit.durability());
    assert_eq!(omitted_commit.put_count(), explicit_commit.put_count());
    assert_eq!(
        omitted_commit.delete_count(),
        explicit_commit.delete_count()
    );
    // Both carry a real wall-clock instant; only its value may differ.
    assert!(omitted_commit.committed_at().is_some());
    assert!(explicit_commit.committed_at().is_some());
}

#[test]
fn operation_on_missing_branch_is_rejected() {
    let mut executor = Executor::open_cache().expect("cache executor opens");

    let read_error = executor
        .execute(Command::KvGet {
            branch: Some("ghost".to_owned()),
            space: None,
            key: bytes("a"),
            as_of: None,
        })
        .expect_err("read on a missing branch fails");
    assert_eq!(read_error.class(), ExecutorErrorClass::NotFound);
    assert_eq!(read_error.code(), "not_found.engine.branch");

    let write_error = executor
        .execute(Command::KvPut {
            branch: Some("ghost".to_owned()),
            space: None,
            key: bytes("a"),
            value: bytes("one"),
        })
        .expect_err("write on a missing branch fails");
    assert_eq!(write_error.class(), ExecutorErrorClass::NotFound);
    assert_eq!(write_error.code(), "not_found.engine.branch");
}
