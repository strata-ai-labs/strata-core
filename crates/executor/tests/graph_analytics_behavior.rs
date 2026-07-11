//! Executor graph analytics command behavior tests (GA5): the 6 analytics
//! and traversal commands end-to-end, timestamp time-travel through the
//! command surface, and engine error codes surfacing through the executor.

#![allow(clippy::result_large_err, clippy::too_many_lines)]

use strata_executor::{Command, Executor, GraphDirection, Output};
use tempfile::TempDir;

fn run_modes(mut exercise: impl FnMut(&mut Executor)) {
    let mut cache = Executor::open_cache().expect("cache executor opens");
    exercise(&mut cache);

    let temp = TempDir::new().expect("temp dir");
    let path = temp.path().join("db");
    let mut durable = Executor::open_durable_local(&path).expect("durable executor opens");
    exercise(&mut durable);
}

/// web: a -e(1)-> b, b -e(2)-> c, a -e(10)-> c, c -f(1)-> d, plus the
/// isolated node `lone`. Returns the commit timestamp of the last seed
/// write (every seeded row is visible at that instant).
fn seed_graph(executor: &mut Executor) -> u64 {
    executor.graph_create("web").expect("graph created");
    for id in ["a", "b", "c", "d", "lone"] {
        executor
            .graph_add_node("web", id, None, None)
            .expect("node added");
    }
    let mut last_timestamp = 0;
    for (src, edge_type, dst, weight) in [
        ("a", "e", "b", 1.0),
        ("b", "e", "c", 2.0),
        ("a", "e", "c", 10.0),
        ("c", "f", "d", 1.0),
    ] {
        let output = executor
            .graph_add_edge("web", src, edge_type, dst, Some(weight), None)
            .expect("edge added");
        let Output::GraphEdgeWriteResult { commit, .. } = output else {
            panic!("unexpected edge write output: {output:?}");
        };
        last_timestamp = commit.timestamp();
    }
    last_timestamp
}

#[test]
fn analytics_commands_run_in_cache_and_durable_modes() {
    run_modes(exercise_analytics_commands);
}

fn exercise_analytics_commands(executor: &mut Executor) {
    seed_graph(executor);

    // WCC: {a, b, c, d} share the representative "a"; lone stands alone.
    let Output::GraphWccResult(wcc) = executor.graph_wcc("web").expect("wcc runs") else {
        panic!("unexpected wcc output");
    };
    assert_eq!(wcc.graph(), "web");
    assert_eq!(wcc.component_count(), 2);
    for id in ["a", "b", "c", "d"] {
        assert_eq!(wcc.components()[id], "a");
    }
    assert_eq!(wcc.components()["lone"], "lone");

    // LCC: the a-b-c triangle closes; d and lone score zero.
    let Output::GraphLccResult(lcc) = executor.graph_lcc("web").expect("lcc runs") else {
        panic!("unexpected lcc output");
    };
    assert!((lcc.coefficients()["a"] - 1.0).abs() < 1e-10);
    assert!(lcc.coefficients()["lone"].abs() < 1e-10);

    // SSSP from a: the two-hop route to c beats the direct edge, and the
    // unreachable node is omitted from the wire map.
    let Output::GraphSsspResult(sssp) = executor.graph_sssp("web", "a", None).expect("sssp runs")
    else {
        panic!("unexpected sssp output");
    };
    assert_eq!(sssp.source(), "a");
    assert_eq!(sssp.direction(), GraphDirection::Outgoing);
    assert!((sssp.distances()["c"] - 3.0).abs() < 1e-10);
    assert!((sssp.distances()["d"] - 4.0).abs() < 1e-10);
    assert!(!sssp.distances().contains_key("lone"));

    // PageRank conserves mass; the personalized variant concentrates it.
    let Output::GraphPagerankResult(uniform) =
        executor.graph_pagerank("web", None).expect("pagerank runs")
    else {
        panic!("unexpected pagerank output");
    };
    assert!(!uniform.personalized());
    let sum: f64 = uniform.ranks().values().sum();
    assert!((sum - 1.0).abs() < 1e-6, "mass leaked: {sum}");
    let Output::GraphPagerankResult(seeded) = executor
        .graph_pagerank("web", Some([("a".to_owned(), 1.0)].into_iter().collect()))
        .expect("personalized pagerank runs")
    else {
        panic!("unexpected pagerank output");
    };
    assert!(seeded.personalized());
    assert!(seeded.ranks()["a"] > uniform.ranks()["a"]);

    // CDLP groups every node with a community representative.
    let Output::GraphCdlpResult(cdlp) = executor.graph_cdlp("web").expect("cdlp runs") else {
        panic!("unexpected cdlp output");
    };
    assert_eq!(cdlp.labels().len(), 5);
    assert_eq!(cdlp.labels()["lone"], "lone");

    // BFS from a: level order with depths and tree edges on the wire.
    let Output::GraphBfsResult(bfs) = executor.graph_bfs("web", "a", None).expect("bfs runs")
    else {
        panic!("unexpected bfs output");
    };
    assert_eq!(bfs.start(), "a");
    assert_eq!(bfs.visited(), ["a", "b", "c", "d"]);
    assert_eq!(
        bfs.depths()["d"],
        2,
        "d is reached through the direct a->c edge"
    );
    assert_eq!(bfs.edges().len(), 3);
    assert!(!bfs.depths().contains_key("lone"));
}

#[test]
fn analytics_commands_time_travel_in_cache_and_durable_modes() {
    run_modes(exercise_time_travel);
}

fn exercise_time_travel(executor: &mut Executor) {
    let seeded_at = seed_graph(executor);

    let Output::GraphWccResult(before) = executor.graph_wcc("web").expect("wcc runs") else {
        panic!("unexpected wcc output");
    };

    // Cut c -f-> d: d falls out of the big component at the latest read,
    // while an as_of read at the seed timestamp reproduces the original.
    executor
        .graph_remove_edge("web", "c", "f", "d")
        .expect("edge removed");
    let Output::GraphWccResult(latest) = executor.graph_wcc("web").expect("wcc runs") else {
        panic!("unexpected wcc output");
    };
    assert_eq!(latest.component_count(), 3);
    assert_eq!(latest.components()["d"], "d");

    let Output::GraphWccResult(historical) = executor
        .execute(Command::GraphWcc {
            branch: None,
            space: None,
            graph: "web".to_owned(),
            budget: None,
            as_of: Some(seeded_at),
        })
        .expect("historical wcc runs")
    else {
        panic!("unexpected wcc output");
    };
    assert_eq!(historical, before, "as_of read reproduces the seeded state");

    let Output::GraphBfsResult(historical_bfs) = executor
        .execute(Command::GraphBfs {
            branch: None,
            space: None,
            graph: "web".to_owned(),
            start: "a".to_owned(),
            max_depth: None,
            max_nodes: None,
            edge_types: None,
            direction: None,
            budget: None,
            as_of: Some(seeded_at),
        })
        .expect("historical bfs runs")
    else {
        panic!("unexpected bfs output");
    };
    assert_eq!(historical_bfs.visited(), ["a", "b", "c", "d"]);
}

#[test]
fn analytics_refusals_surface_by_code_in_cache_and_durable_modes() {
    run_modes(exercise_refusals);
}

fn exercise_refusals(executor: &mut Executor) {
    seed_graph(executor);

    // A missing graph refuses before any snapshot is built.
    let error = executor.graph_wcc("ghost").expect_err("missing graph");
    assert_eq!(error.code(), "not_found.engine.graph");

    // A missing traversal start refuses with the node-level code.
    let error = executor
        .graph_bfs("web", "ghost", None)
        .expect_err("missing start");
    assert_eq!(error.code(), "not_found.engine.graph_node");
    let error = executor
        .graph_sssp("web", "ghost", None)
        .expect_err("missing source");
    assert_eq!(error.code(), "not_found.engine.graph_node");

    // A snapshot budget that cannot hold the graph refuses.
    let error = executor
        .execute(Command::GraphWcc {
            branch: None,
            space: None,
            graph: "web".to_owned(),
            budget: Some(strata_executor::GraphAnalyticsBudget::new(Some(2), None)),
            as_of: None,
        })
        .expect_err("budget refusal");
    assert_eq!(
        error.code(),
        "resource_exhausted.engine.graph_analytics_budget"
    );

    // Invalid PageRank options and personalization refuse by code.
    let error = executor
        .execute(Command::GraphPagerank {
            branch: None,
            space: None,
            graph: "web".to_owned(),
            damping: Some(1.5),
            max_iterations: None,
            tolerance: None,
            personalization: None,
            budget: None,
            as_of: None,
        })
        .expect_err("bad damping");
    assert_eq!(
        error.code(),
        "invalid_argument.engine.graph_pagerank_options"
    );
    let error = executor
        .graph_pagerank("web", Some(std::collections::BTreeMap::new()))
        .expect_err("empty personalization");
    assert_eq!(
        error.code(),
        "invalid_argument.engine.graph_personalization"
    );
}
