//! #3112 S3b: `as_of_time` across every capability that can time-travel.
//!
//! S3a proved the resolver and wired one pilot command. S3b rolls the input
//! out to the remaining 30, which is mechanical enough that the compiler
//! catches most mistakes — but not the one that matters: a handler that
//! accepts `as_of_time` and then quietly reads the wrong thing, or reads
//! `latest` because the resolved value never reached the engine.
//!
//! So each capability gets the same two proofs the pilot got:
//!
//! 1. **Equivalence** — reading at a commit's `committed_at` returns exactly
//!    what reading at that commit's logical `timestamp` returns.
//! 2. **Discrimination** — the wall-clock read actually distinguishes history,
//!    rather than both sides agreeing because both return current state.
//!
//! Without (2), (1) passes vacuously on a handler that ignores `as_of`
//! entirely — which is precisely the regression this file exists to catch.

use strata_executor::{Bytes, Command, Executor, ExecutorErrorClass, Output, VectorDistanceMetric};

/// Separates commits in wall-clock time before committing, so each lands in
/// its own microsecond and is individually addressable. Waiting *before* the
/// commit matters: retrying until the clock ticks would leave the rejected
/// attempts at instants the test then reads at.
fn spaced<T>(executor: &mut Executor, commit: impl FnOnce(&mut Executor) -> T) -> T {
    std::thread::sleep(std::time::Duration::from_millis(2));
    commit(executor)
}

/// A commit's two clocks: `(logical timestamp, committed_at)`.
fn stamp(executor: &mut Executor, command: Command) -> (u64, u64) {
    let output = executor.execute(command).expect("write succeeds");
    // Each capability has its own write-result variant; all of them carry the
    // same receipt, which is where both clocks live.
    let commit = match &output {
        Output::WriteResult { commit, .. }
        | Output::JsonWriteResult { commit, .. }
        | Output::VectorWriteResult { commit, .. }
        | Output::EventAppendResult { commit, .. }
        | Output::GraphNodeWriteResult { commit, .. } => commit,
        Output::DeleteResult { commit, .. } | Output::JsonDeleteResult { commit, .. } => commit
            .as_ref()
            .expect("an applied delete carries a commit receipt"),
        other => panic!("unexpected write output: {other:?}"),
    };
    (
        commit.timestamp(),
        commit
            .committed_at()
            .expect("a live commit records a wall-clock instant"),
    )
}

fn run(executor: &mut Executor, command: Command) -> Output {
    executor.execute(command).expect("command succeeds")
}

/// The shared body: write two distinct states, then assert both clocks agree
/// at each, and that they see different things.
fn assert_clocks_agree(
    capability: &str,
    executor: &mut Executor,
    first: Command,
    second: Command,
    read_at: impl Fn(Option<u64>, Option<u64>) -> Command,
) {
    let (first_ts, first_instant) = spaced(executor, |e| stamp(e, first));
    let (second_ts, second_instant) = spaced(executor, |e| stamp(e, second));
    assert!(
        second_instant > first_instant,
        "{capability}: fixture needs distinct wall-clock instants"
    );

    for (label, timestamp, instant) in [
        ("first", first_ts, first_instant),
        ("second", second_ts, second_instant),
    ] {
        let by_logical = run(executor, read_at(Some(timestamp), None));
        let by_wall_clock = run(executor, read_at(None, Some(instant)));
        assert_eq!(
            by_logical, by_wall_clock,
            "{capability} at {label} commit: as_of_time({instant}) diverged from as_of({timestamp})"
        );
    }

    // Non-vacuity: the two commits must be observably different through the
    // wall-clock path, or the equivalence above proves nothing.
    assert_ne!(
        run(executor, read_at(None, Some(first_instant))),
        run(executor, read_at(None, Some(second_instant))),
        "{capability}: wall-clock reads did not distinguish the two commits"
    );
}

#[test]
fn kv_as_of_time_matches_as_of() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    assert_clocks_agree(
        "kv",
        &mut executor,
        Command::KvPut {
            branch: None,
            space: None,
            key: Bytes::from("k"),
            value: Bytes::from("one"),
        },
        Command::KvPut {
            branch: None,
            space: None,
            key: Bytes::from("k"),
            value: Bytes::from("two"),
        },
        |as_of, as_of_time| Command::KvGet {
            branch: None,
            space: None,
            key: Bytes::from("k"),
            as_of,
            as_of_time,
        },
    );
}

#[test]
fn json_as_of_time_matches_as_of() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    let set = |value: &str| Command::JsonSet {
        branch: None,
        space: None,
        key: "doc".to_owned(),
        path: "$.field".to_owned(),
        value: serde_json::json!(value),
    };
    assert_clocks_agree(
        "json",
        &mut executor,
        set("one"),
        set("two"),
        |as_of, as_of_time| Command::JsonGet {
            branch: None,
            space: None,
            key: "doc".to_owned(),
            path: "$.field".to_owned(),
            as_of,
            as_of_time,
        },
    );
}

#[test]
fn event_as_of_time_matches_as_of() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    let append = |payload: &str| Command::EventAppend {
        branch: None,
        space: None,
        event_type: "kind".to_owned(),
        payload: serde_json::json!({ "v": payload }),
    };
    // Events are append-only, so history is distinguished by COUNT rather than
    // by a changing value at one key.
    assert_clocks_agree(
        "event",
        &mut executor,
        append("one"),
        append("two"),
        |as_of, as_of_time| Command::EventCount {
            branch: None,
            space: None,
            as_of,
            as_of_time,
        },
    );
}

#[test]
fn vector_as_of_time_matches_as_of() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    run(
        &mut executor,
        Command::VectorCreateCollection {
            branch: None,
            space: None,
            collection: "c".to_owned(),
            dimension: 2,
            metric: VectorDistanceMetric::Cosine,
        },
    );
    let upsert = |vector: Vec<f64>| Command::VectorUpsert {
        branch: None,
        space: None,
        collection: "c".to_owned(),
        key: "v".to_owned(),
        vector,
        metadata: None,
    };
    assert_clocks_agree(
        "vector",
        &mut executor,
        upsert(vec![1.0, 0.0]),
        upsert(vec![0.0, 1.0]),
        |as_of, as_of_time| Command::VectorGet {
            branch: None,
            space: None,
            collection: "c".to_owned(),
            key: "v".to_owned(),
            as_of,
            as_of_time,
        },
    );
}

#[test]
fn graph_as_of_time_matches_as_of() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    run(
        &mut executor,
        Command::GraphCreate {
            branch: None,
            space: None,
            graph: "g".to_owned(),
        },
    );
    let add = |node: &str| Command::GraphAddNode {
        branch: None,
        space: None,
        graph: "g".to_owned(),
        node_id: node.to_owned(),
        properties: None,
        binding: None,
        object_type: None,
    };
    // Node count changes across the two commits, so a `latest`-reading handler
    // would return the same page for both instants and fail non-vacuity.
    assert_clocks_agree(
        "graph",
        &mut executor,
        add("n1"),
        add("n2"),
        |as_of, as_of_time| Command::GraphListNodes {
            branch: None,
            space: None,
            graph: "g".to_owned(),
            prefix: None,
            cursor: None,
            limit: None,
            as_of,
            as_of_time,
        },
    );
}

/// The mutual-exclusion refusal comes from one shared helper, so it must hold
/// identically on every capability — including the analytics family, which
/// reaches resolution through a different path than the direct handlers.
#[test]
fn supplying_both_clocks_is_refused_on_every_capability() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    run(
        &mut executor,
        Command::GraphCreate {
            branch: None,
            space: None,
            graph: "g".to_owned(),
        },
    );

    let mut both = |command: Command| {
        let error = executor
            .execute(command)
            .expect_err("supplying both clocks must be refused");
        assert_eq!(error.class(), ExecutorErrorClass::InvalidInput);
        assert_eq!(error.code(), "invalid_argument.executor.as_of_conflict");
    };

    both(Command::KvCount {
        branch: None,
        space: None,
        prefix: None,
        as_of: Some(1),
        as_of_time: Some(2),
    });
    both(Command::JsonCount {
        branch: None,
        space: None,
        prefix: None,
        as_of: Some(1),
        as_of_time: Some(2),
    });
    both(Command::EventCount {
        branch: None,
        space: None,
        as_of: Some(1),
        as_of_time: Some(2),
    });
    both(Command::VectorCount {
        branch: None,
        space: None,
        collection: "c".to_owned(),
        as_of: Some(1),
        as_of_time: Some(2),
    });
    both(Command::GraphListNodes {
        branch: None,
        space: None,
        graph: "g".to_owned(),
        prefix: None,
        cursor: None,
        limit: None,
        as_of: Some(1),
        as_of_time: Some(2),
    });
    // The analytics family resolves inside the shared adjacency-index helper
    // rather than in the handler, so it needs its own check.
    both(Command::GraphWcc {
        branch: None,
        space: None,
        graph: "g".to_owned(),
        budget: None,
        as_of: Some(1),
        as_of_time: Some(2),
    });
}

/// Callers keep using `as_of` exactly as before; `as_of_time` is additive. The
/// cost of two fields is that someone can put a number in the wrong one — so
/// the important property is that doing so RAISES rather than silently
/// returning data from the wrong point in history.
///
/// It holds because the two clocks occupy wildly separated ranges: the logical
/// clock is a small per-commit counter, wall-clock instants are ~1.8e15. A
/// logical timestamp offered as an instant lands in 1970 (before any dated
/// commit); an instant offered as a logical timestamp lands far past the tip.
///
/// This is why the fields are separate rather than one auto-detecting input.
/// Auto-detection would have to guess from magnitude, and the engine supports
/// externally-supplied timestamp bases — so the ranges are not guaranteed
/// disjoint, and a wrong guess would silently answer the wrong question.
#[test]
fn using_the_wrong_clocks_field_raises_rather_than_misreading_history() {
    let mut executor = Executor::open_cache().expect("cache executor opens");
    let (logical, instant) = spaced(&mut executor, |e| {
        stamp(
            e,
            Command::KvPut {
                branch: None,
                space: None,
                key: Bytes::from("k"),
                value: Bytes::from("v"),
            },
        )
    });

    let read = |as_of, as_of_time| Command::KvGet {
        branch: None,
        space: None,
        key: Bytes::from("k"),
        as_of,
        as_of_time,
    };

    // A wall-clock instant mistakenly passed as a logical timestamp.
    let error = executor
        .execute(read(Some(instant), None))
        .expect_err("an instant is far past the logical tip");
    assert_eq!(error.class(), ExecutorErrorClass::NotFound);

    // A logical timestamp mistakenly passed as a wall-clock instant.
    let error = executor
        .execute(read(None, Some(logical)))
        .expect_err("a logical counter lands before any dated commit");
    assert_eq!(error.class(), ExecutorErrorClass::NotFound);

    // Each in its own field still works.
    executor
        .execute(read(Some(logical), None))
        .expect("as_of with a logical timestamp reads");
    executor
        .execute(read(None, Some(instant)))
        .expect("as_of_time with an instant reads");
}
