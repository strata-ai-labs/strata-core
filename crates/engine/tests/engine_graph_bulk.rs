//! GI3 exit gate: chunked bulk ingest — upsert semantics with derived
//! index maintenance, node-then-edge ordering, endpoint validation,
//! frozen-ontology enforcement, and mid-stream refusals leaving no
//! partial state.

#![allow(clippy::too_many_lines)]

mod common;

use strata_engine::{
    Database, GraphAnalyticsBudget, GraphBfsOptions, GraphBindingPrimitive, GraphBindingTarget,
    GraphDeletePolicy, GraphEdgeData, GraphEdgeType, GraphEntityBinding, GraphName, GraphNodeData,
    GraphNodeId, ProductSpace,
};

use common::{branch, open_cache_database, open_durable_database, space};

fn run_database_modes(exercise: fn(Database)) {
    exercise(open_cache_database().expect("cache open succeeds"));

    let tempdir = tempfile::tempdir().expect("tempdir");
    exercise(open_durable_database(tempdir.path()).expect("durable open succeeds"));
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

fn nodes(ids: &[&str]) -> Vec<(GraphNodeId, GraphNodeData)> {
    ids.iter()
        .map(|id| (node_id(id), GraphNodeData::default()))
        .collect()
}

fn edge(
    src: &str,
    kind: &str,
    dst: &str,
    weight: f64,
) -> (GraphNodeId, GraphEdgeType, GraphNodeId, GraphEdgeData) {
    (
        node_id(src),
        edge_type(kind),
        node_id(dst),
        GraphEdgeData::new(weight, None).expect("edge data"),
    )
}

#[test]
fn bulk_insert_ingests_in_cache_and_durable_modes() {
    run_database_modes(exercise_bulk_insert);
}

fn exercise_bulk_insert(mut database: Database) {
    let mut graph = database
        .graph(branch("default"), space("default"))
        .expect("graph service opens");
    let name = graph_name("bulk");
    graph.create_graph(name.clone()).expect("graph created");

    // Empty input commits nothing.
    let outcome = graph
        .bulk_insert(&name, &[], &[], None)
        .expect("empty ingest");
    assert_eq!(outcome.nodes_inserted(), 0);
    assert_eq!(outcome.edges_inserted(), 0);
    assert_eq!(outcome.commits(), 0);
    assert!(outcome.last_commit().is_none());

    // Chunk size 1 forces one commit per item: 4 nodes + 3 edges = 7.
    let outcome = graph
        .bulk_insert(
            &name,
            &nodes(&["a", "b", "c", "lone"]),
            &[
                edge("a", "e", "b", 1.0),
                edge("b", "e", "c", 2.0),
                edge("a", "f", "c", 3.0),
            ],
            Some(1),
        )
        .expect("chunked ingest");
    assert_eq!(outcome.nodes_inserted(), 4);
    assert_eq!(outcome.edges_inserted(), 3);
    assert_eq!(outcome.commits(), 7);
    assert!(outcome.last_commit().is_some());

    // The ingested graph is fully queryable: traversal and analytics
    // see the same rows any other write path would produce.
    let index = graph
        .adjacency_index(&name, &GraphAnalyticsBudget::default())
        .expect("index builds");
    assert_eq!(index.node_count(), 4);
    assert_eq!(index.edge_count(), 3);
    let bfs = index
        .bfs(&node_id("a"), &GraphBfsOptions::default())
        .expect("bfs runs");
    assert_eq!(bfs.visited().len(), 3);

    // Edges may lead with endpoints arriving in the same call.
    let outcome = graph
        .bulk_insert(&name, &nodes(&["d"]), &[edge("d", "e", "a", 1.0)], None)
        .expect("same-call endpoint ingest");
    assert_eq!(outcome.commits(), 2, "one node chunk, one edge chunk");
    assert!(graph
        .get_edge(&name, &node_id("d"), &edge_type("e"), &node_id("a"))
        .expect("read")
        .is_some());

    // Re-ingesting is an upsert: same rows, updated weight.
    graph
        .bulk_insert(&name, &nodes(&["a"]), &[edge("a", "e", "b", 9.0)], None)
        .expect("upsert ingest");
    let updated = graph
        .get_edge(&name, &node_id("a"), &edge_type("e"), &node_id("b"))
        .expect("read")
        .expect("edge visible");
    assert!((updated.data().weight() - 9.0).abs() < f64::EPSILON);
}

#[test]
fn bulk_chunks_respect_the_storage_commit_budget_in_cache_and_durable_modes() {
    run_database_modes(exercise_storage_commit_budget);
}

/// Regression (found ingesting the ASOIAF dataset): every edge writes a
/// forward and a reverse row, so an unchunked commit of a few thousand
/// edges exceeds the storage per-commit mutation budget. The default
/// and the clamp must keep every chunk inside one commit.
fn exercise_storage_commit_budget(mut database: Database) {
    let mut graph = database
        .graph(branch("default"), space("default"))
        .expect("graph service opens");
    let name = graph_name("wide");
    graph.create_graph(name.clone()).expect("graph created");

    let ids: Vec<String> = (0..2_501).map(|index| format!("n{index}")).collect();
    let node_items: Vec<(GraphNodeId, GraphNodeData)> = ids
        .iter()
        .map(|id| (node_id(id), GraphNodeData::default()))
        .collect();
    // 2,500 edges = 5,000 row mutations: over the 4,096 budget if they
    // ever landed in one commit.
    let edge_items: Vec<(GraphNodeId, GraphEdgeType, GraphNodeId, GraphEdgeData)> =
        ids[1..].iter().map(|id| edge("n0", "e", id, 1.0)).collect();

    // Default chunking succeeds and spreads the work over many commits.
    let outcome = graph
        .bulk_insert(&name, &node_items, &edge_items, None)
        .expect("default chunking ingests");
    assert_eq!(outcome.edges_inserted(), 2_500);
    assert!(outcome.commits() > 2, "default chunking splits commits");

    // An absurd explicit chunk size clamps instead of building a commit
    // the storage layer refuses.
    let outcome = graph
        .bulk_insert(&name, &node_items, &edge_items, Some(250_000))
        .expect("clamped chunking ingests");
    assert!(outcome.commits() >= 4, "oversized chunk request clamps");
}

#[test]
fn bulk_insert_refuses_before_committing_in_cache_and_durable_modes() {
    run_database_modes(exercise_bulk_refusals);
}

fn exercise_bulk_refusals(mut database: Database) {
    let mut graph = database
        .graph(branch("default"), space("default"))
        .expect("graph service opens");
    let name = graph_name("bulk");
    graph.create_graph(name.clone()).expect("graph created");

    // Missing graph refuses.
    let error = graph
        .bulk_insert(&graph_name("ghost"), &nodes(&["a"]), &[], None)
        .expect_err("missing graph");
    assert_eq!(error.code(), "not_found.engine.graph");

    // An edge naming an endpoint that neither exists nor arrives in the
    // call refuses up front — no chunk commits, no partial state.
    let error = graph
        .bulk_insert(
            &name,
            &nodes(&["a", "b"]),
            &[edge("a", "e", "ghost", 1.0)],
            None,
        )
        .expect_err("missing endpoint");
    assert_eq!(error.code(), "invalid_argument.engine.graph_edge_endpoint");
    assert!(
        graph
            .get_node(&name, &node_id("a"))
            .expect("read")
            .is_none(),
        "validation failure commits nothing"
    );

    // Frozen-ontology enforcement applies to bulk exactly like single
    // upserts: an undeclared object type refuses before any commit.
    graph
        .define_object_type(
            &name,
            strata_engine::GraphObjectTypeDef::new(
                strata_engine::GraphTypeName::new("Document").expect("type name"),
                std::iter::empty::<(String, strata_engine::GraphPropertyDef)>(),
            )
            .expect("object type"),
        )
        .expect("type defined");
    graph.freeze_ontology(&name).expect("ontology frozen");
    let typed = vec![(
        node_id("typed"),
        GraphNodeData::default()
            .with_object_type(strata_engine::GraphTypeName::new("Undeclared").expect("type name")),
    )];
    let error = graph
        .bulk_insert(&name, &typed, &[], None)
        .expect_err("undeclared type");
    assert_eq!(
        error.code(),
        "failed_precondition.engine.graph_ontology_node_type"
    );
    assert!(
        graph
            .get_node(&name, &node_id("typed"))
            .expect("read")
            .is_none(),
        "ontology refusal commits nothing"
    );

    // Bindings ingested in bulk feed the same reverse map the delete
    // policies consume.
    let target = GraphBindingTarget::new(
        GraphBindingPrimitive::Kv,
        None,
        ProductSpace::new("default").expect("space"),
        "doc-bulk",
    )
    .expect("target");
    graph
        .bulk_insert(
            &name,
            &[(
                node_id("bound"),
                GraphNodeData::new(None, Some(GraphEntityBinding::new(target.clone()))),
            )],
            &[],
            None,
        )
        .expect("bound ingest");
    let outcome = graph
        .apply_binding_delete_policy(&target, GraphDeletePolicy::Cascade)
        .expect("cascade applies");
    assert_eq!(outcome.nodes_affected(), 1);
}
