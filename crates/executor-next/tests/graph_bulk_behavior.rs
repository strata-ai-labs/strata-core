//! GI3 executor behavior: bulk ingest through the command surface —
//! counts, chunked commits, and refusal codes.

#![allow(clippy::result_large_err)]

use strata_executor_next::{
    Command, Executor, GraphBulkEdge, GraphBulkNode, GraphDirection, Output,
};
use tempfile::TempDir;

fn run_modes(mut exercise: impl FnMut(&mut Executor)) {
    let mut cache = Executor::open_cache().expect("cache executor opens");
    exercise(&mut cache);

    let temp = TempDir::new().expect("temp dir");
    let path = temp.path().join("db");
    let mut durable = Executor::open_durable_local(&path).expect("durable executor opens");
    exercise(&mut durable);
}

fn bulk_node(id: &str) -> GraphBulkNode {
    GraphBulkNode::new(id.to_owned(), None, None, None)
}

fn bulk_edge(src: &str, kind: &str, dst: &str, weight: f64) -> GraphBulkEdge {
    GraphBulkEdge::new(
        src.to_owned(),
        kind.to_owned(),
        dst.to_owned(),
        Some(weight),
        None,
    )
}

#[test]
fn bulk_insert_command_ingests_in_cache_and_durable_modes() {
    run_modes(exercise_bulk_command);
}

fn exercise_bulk_command(executor: &mut Executor) {
    executor.graph_create("bulk").expect("graph created");

    let output = executor
        .graph_bulk_insert(
            "bulk",
            vec![bulk_node("a"), bulk_node("b"), bulk_node("c")],
            vec![bulk_edge("a", "e", "b", 1.0), bulk_edge("b", "e", "c", 2.0)],
        )
        .expect("bulk ingest");
    let Output::GraphBulkInsertResult {
        graph,
        nodes_inserted,
        edges_inserted,
        commits,
        commit,
        ..
    } = output
    else {
        panic!("unexpected bulk output");
    };
    assert_eq!(graph, "bulk");
    assert_eq!(nodes_inserted, 3);
    assert_eq!(edges_inserted, 2);
    assert_eq!(commits, 2, "one node chunk, one edge chunk");
    assert!(commit.is_some());

    // The ingested rows serve every read surface.
    let output = executor
        .graph_neighbors("bulk", "a", GraphDirection::Outgoing, None, None, None)
        .expect("neighbors read");
    let Output::GraphNeighborPage { items, .. } = output else {
        panic!("unexpected neighbors output");
    };
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].node_id(), "b");

    let Output::GraphWccResult(wcc) = executor.graph_wcc("bulk").expect("wcc runs") else {
        panic!("unexpected wcc output");
    };
    assert_eq!(wcc.component_count(), 1);

    // A dangling endpoint refuses by code, with an explicit chunk size
    // on the wire.
    let error = executor
        .execute(Command::GraphBulkInsert {
            branch: None,
            space: None,
            graph: "bulk".to_owned(),
            nodes: Vec::new(),
            edges: vec![bulk_edge("a", "e", "ghost", 1.0)],
            chunk_size: Some(10),
        })
        .expect_err("dangling endpoint");
    assert_eq!(error.code(), "invalid_argument.engine.graph_edge_endpoint");
}
