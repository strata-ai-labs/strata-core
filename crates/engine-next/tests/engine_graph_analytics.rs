//! GA1 exit gate: adjacency snapshots built from storage at a consistent
//! read — correctness (isolated nodes, self-loops, weights, edge types),
//! temporal snapshots, determinism, and budget refusals by error code.

mod common;

use strata_engine_next::{
    Database, GraphAdjacencyEdge, GraphAnalyticsBudget, GraphEdgeData, GraphEdgeType, GraphName,
    GraphNodeData, GraphNodeId,
};

use common::{branch, open_cache_database, open_durable_database, space};

fn run_database_modes(exercise: fn(Database)) {
    exercise(open_cache_database().expect("cache open succeeds"));

    let tempdir = tempfile::tempdir().expect("tempdir");
    exercise(open_durable_database(tempdir.path()).expect("durable open succeeds"));
}

fn graph_service<'a>(
    database: &'a mut Database,
    branch_name: &str,
    space_name: &str,
) -> strata_engine_next::GraphService<'a> {
    database
        .graph(branch(branch_name), space(space_name))
        .expect("graph service opens")
}

fn graph_name(value: &str) -> GraphName {
    GraphName::new(value).expect("valid graph")
}

fn node_id(value: &str) -> GraphNodeId {
    GraphNodeId::new(value).expect("valid node id")
}

fn edge_type(value: &str) -> GraphEdgeType {
    GraphEdgeType::new(value).expect("valid edge type")
}

fn edge_data(weight: f64) -> GraphEdgeData {
    GraphEdgeData::new(weight, None).expect("edge data")
}

/// deps: a -links(1.0)-> b, a -cites(2.5)-> c, b -links(0.5)-> b (self
/// loop), plus isolated node `lone`.
fn seed_graph(graph: &mut strata_engine_next::GraphService<'_>, name: &GraphName) {
    graph.create_graph(name.clone()).expect("graph created");
    for id in ["a", "b", "c", "lone"] {
        graph
            .upsert_node(name, node_id(id), GraphNodeData::default())
            .expect("node");
    }
    graph
        .upsert_edge(
            name,
            node_id("a"),
            edge_type("links"),
            node_id("b"),
            edge_data(1.0),
        )
        .expect("edge");
    graph
        .upsert_edge(
            name,
            node_id("a"),
            edge_type("cites"),
            node_id("c"),
            edge_data(2.5),
        )
        .expect("edge");
    graph
        .upsert_edge(
            name,
            node_id("b"),
            edge_type("links"),
            node_id("b"),
            edge_data(0.5),
        )
        .expect("self loop");
}

#[test]
fn adjacency_snapshot_reflects_visible_state_in_cache_and_durable_modes() {
    run_database_modes(exercise_snapshot_correctness);
}

fn exercise_snapshot_correctness(mut database: Database) {
    let mut graph = graph_service(&mut database, "default", "default");
    let name = graph_name("deps");

    let error = graph
        .adjacency_index(&name, &GraphAnalyticsBudget::default())
        .expect_err("missing graph refuses");
    assert_eq!(error.code(), "not_found.engine.graph");

    seed_graph(&mut graph, &name);
    let index = graph
        .adjacency_index(&name, &GraphAnalyticsBudget::default())
        .expect("index builds");

    assert_eq!(index.graph().as_str(), "deps");
    assert_eq!(index.node_count(), 4, "isolated node included");
    assert_eq!(index.edge_count(), 3, "self loop counts once");
    let ids: Vec<&str> = index.node_ids().iter().map(GraphNodeId::as_str).collect();
    assert_eq!(ids, vec!["a", "b", "c", "lone"], "ascending id order");

    let a = index.node_index(&node_id("a")).expect("a indexed");
    let b = index.node_index(&node_id("b")).expect("b indexed");
    let c = index.node_index(&node_id("c")).expect("c indexed");
    let lone = index.node_index(&node_id("lone")).expect("lone indexed");

    // a's outgoing edges, sorted by (edge type, neighbor): cites < links.
    let out = index.outgoing(a);
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].neighbor(), c);
    assert_eq!(
        index
            .edge_type_name(out[0].edge_type())
            .expect("type")
            .as_str(),
        "cites"
    );
    assert!((out[0].weight() - 2.5).abs() < f64::EPSILON);
    assert_eq!(out[1].neighbor(), b);
    assert!((out[1].weight() - 1.0).abs() < f64::EPSILON);

    // b: one incoming from a plus the self loop in both directions.
    assert_eq!(index.outgoing(b).len(), 1);
    assert_eq!(index.outgoing(b)[0].neighbor(), b);
    let incoming: Vec<usize> = index
        .incoming(b)
        .iter()
        .map(GraphAdjacencyEdge::neighbor)
        .collect();
    assert_eq!(incoming, vec![a, b]);

    // Isolated node has no edges; the index stays queryable at the edges.
    assert!(index.outgoing(lone).is_empty());
    assert!(index.incoming(lone).is_empty());
    assert!(index.outgoing(999).is_empty(), "out of range is empty");
    assert_eq!(index.node_index(&node_id("ghost")), None);

    // Determinism: a second build over the same state is equal.
    let again = graph
        .adjacency_index(&name, &GraphAnalyticsBudget::default())
        .expect("index rebuilds");
    assert_eq!(again, index);
}

#[test]
fn adjacency_snapshots_are_temporal_in_cache_and_durable_modes() {
    run_database_modes(exercise_temporal_snapshots);
}

fn exercise_temporal_snapshots(mut database: Database) {
    let mut graph = graph_service(&mut database, "default", "default");
    let name = graph_name("deps");
    seed_graph(&mut graph, &name);

    let before = graph
        .adjacency_index(&name, &GraphAnalyticsBudget::default())
        .expect("index builds");
    let seeded_version = graph
        .graph_info(&name)
        .expect("info reads")
        .expect("graph visible")
        .updated_version();

    // Mutate: delete the self loop, add a node and an edge.
    graph
        .delete_edge(&name, &node_id("b"), &edge_type("links"), &node_id("b"))
        .expect("delete edge");
    graph
        .upsert_node(&name, node_id("d"), GraphNodeData::default())
        .expect("node");
    graph
        .upsert_edge(
            &name,
            node_id("d"),
            edge_type("links"),
            node_id("a"),
            edge_data(4.0),
        )
        .expect("edge");

    let latest = graph
        .adjacency_index(&name, &GraphAnalyticsBudget::default())
        .expect("index builds");
    assert_eq!(latest.node_count(), 5);
    assert_eq!(latest.edge_count(), 3);
    let b = latest.node_index(&node_id("b")).expect("b indexed");
    assert!(latest.outgoing(b).is_empty(), "self loop gone");

    // The historical snapshot equals the one built before the mutations.
    let historical = graph
        .adjacency_index_at_version(&name, &GraphAnalyticsBudget::default(), seeded_version)
        .expect("historical index builds");
    assert_eq!(historical, before, "time-traveled snapshot matches");
}

#[test]
fn adjacency_budget_refusals_surface_by_code_in_cache_and_durable_modes() {
    run_database_modes(exercise_budget_refusals);
}

fn exercise_budget_refusals(mut database: Database) {
    let mut graph = graph_service(&mut database, "default", "default");
    let name = graph_name("deps");
    seed_graph(&mut graph, &name);

    let error = graph
        .adjacency_index(&name, &GraphAnalyticsBudget::new(2, 100))
        .expect_err("node budget refuses");
    assert_eq!(
        error.code(),
        "resource_exhausted.engine.graph_analytics_budget"
    );

    let error = graph
        .adjacency_index(&name, &GraphAnalyticsBudget::new(100, 2))
        .expect_err("edge budget refuses");
    assert_eq!(
        error.code(),
        "resource_exhausted.engine.graph_analytics_budget"
    );

    // A budget that fits succeeds.
    assert!(graph
        .adjacency_index(&name, &GraphAnalyticsBudget::new(4, 3))
        .is_ok());
}
