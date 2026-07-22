//! TCP4.10 compound graph metamorphic relations (the GAMERA analog).
//!
//! Compound MRs — fusing and partitioning query patterns, and mutating the
//! data under an invariant — carried 30 of GAMERA's 39 graph-engine bugs.
//! The verdicts are relational identities over Strata's own graph surface.

use serde_json::json;

#[path = "parity/support.rs"]
mod support;

fn neighbor_edges(output: &serde_json::Value) -> Vec<String> {
    output["data"]["items"]
        .as_array()
        .unwrap_or_else(|| panic!("neighbors carries items: {output}"))
        .iter()
        .map(|item| {
            format!(
                "{}->{}:{}",
                item["edge"]["src"].as_str().unwrap_or("?"),
                item["dst"].as_str().unwrap_or("?"),
                item["edge"]["edge_type"].as_str().unwrap_or("?")
            )
        })
        .collect()
}

fn seeded_graph(executor: &mut strata_executor::Executor) {
    support::run(executor, &json!({"type": "graph_create", "graph": "mr"}));
    for (node, object_type) in [
        ("a", "Person"),
        ("b", "Person"),
        ("c", "Doc"),
        ("d", "Doc"),
        ("e", "Person"),
    ] {
        support::run(
            executor,
            &json!({"type": "graph_add_node", "graph": "mr", "node_id": node,
                    "properties": {}, "object_type": object_type}),
        );
    }
    for (src, dst, edge_type) in [
        ("a", "b", "knows"),
        ("a", "c", "wrote"),
        ("a", "d", "wrote"),
        ("b", "c", "read"),
        ("e", "a", "knows"),
    ] {
        support::run(
            executor,
            &json!({"type": "graph_add_edge", "graph": "mr", "src": src, "dst": dst,
                    "edge_type": edge_type, "properties": {}}),
        );
    }
}

/// Partition MR: a node's outgoing neighbors partition exactly by edge type
/// — the union of the per-type queries is the unfiltered query, disjointly.
#[test]
fn neighbors_partition_exactly_by_edge_type() {
    let mut executor = support::executor();
    seeded_graph(&mut executor);

    let mut unfiltered = neighbor_edges(&support::run(
        &mut executor,
        &json!({"type": "graph_neighbors", "graph": "mr", "node_id": "a",
                "direction": "outgoing", "limit": 100}),
    ));
    let mut reassembled = Vec::new();
    for edge_type in ["knows", "wrote", "read"] {
        reassembled.extend(neighbor_edges(&support::run(
            &mut executor,
            &json!({"type": "graph_neighbors", "graph": "mr", "node_id": "a",
                    "direction": "outgoing", "edge_type": edge_type, "limit": 100}),
        )));
    }
    unfiltered.sort();
    let deduped = reassembled.len();
    reassembled.sort();
    reassembled.dedup();
    assert_eq!(
        reassembled.len(),
        deduped,
        "per-type partitions are disjoint"
    );
    assert_eq!(
        reassembled, unfiltered,
        "per-type neighbor queries reassemble into the unfiltered query"
    );
}

/// Partition MR: `nodes_by_type` over the planted types reassembles exactly
/// into the full node listing.
#[test]
fn nodes_partition_exactly_by_object_type() {
    let mut executor = support::executor();
    seeded_graph(&mut executor);

    let all = support::run(
        &mut executor,
        &json!({"type": "graph_list_nodes", "graph": "mr", "limit": 100}),
    );
    let total = all["data"]["items"]
        .as_array()
        .unwrap_or_else(|| panic!("list_nodes carries items: {all}"))
        .len();

    let mut partitioned = 0;
    for object_type in ["Person", "Doc"] {
        let part = support::run(
            &mut executor,
            &json!({"type": "graph_nodes_by_type", "graph": "mr",
                    "object_type": object_type, "limit": 100}),
        );
        partitioned += part["data"]["items"]
            .as_array()
            .unwrap_or_else(|| panic!("nodes_by_type carries items: {part}"))
            .len();
    }
    assert_eq!(
        partitioned, total,
        "typed node partitions reassemble into the full listing"
    );
}

/// Path MR: widening the BFS depth bound only grows the visited set, and
/// depths agree on the intersection.
#[test]
fn bfs_visited_set_grows_monotonically_with_depth() {
    let mut executor = support::executor();
    seeded_graph(&mut executor);
    let mut previous: Option<serde_json::Map<String, serde_json::Value>> = None;
    for max_depth in 1..=4_u32 {
        let bfs = support::run(
            &mut executor,
            &json!({"type": "graph_bfs", "graph": "mr", "start": "a",
                    "max_depth": max_depth}),
        );
        let depths = bfs["data"]["depths"]
            .as_object()
            .unwrap_or_else(|| panic!("bfs carries depths: {bfs}"))
            .clone();
        if let Some(narrower) = &previous {
            for (node, depth) in narrower {
                assert_eq!(
                    depths.get(node),
                    Some(depth),
                    "widening max_depth must preserve node `{node}`'s depth"
                );
            }
            assert!(
                depths.len() >= narrower.len(),
                "widening max_depth must not shrink the visited set"
            );
        }
        previous = Some(depths);
    }
}

/// Data-mutation MR: WCC assigns every node a component, and bridging two
/// components with one edge reduces the component count by exactly one.
#[test]
fn wcc_components_merge_by_exactly_one_on_a_bridge_edge() {
    let mut executor = support::executor();
    seeded_graph(&mut executor);
    // "f" starts isolated: its own component.
    support::run(
        &mut executor,
        &json!({"type": "graph_add_node", "graph": "mr", "node_id": "f",
                "properties": {}, "object_type": "Person"}),
    );
    let budget = json!({"max_nodes": 1000, "max_edges": 1000});
    let before = support::run(
        &mut executor,
        &json!({"type": "graph_wcc", "graph": "mr", "budget": budget}),
    );
    let components_before = before["data"]["component_count"]
        .as_u64()
        .unwrap_or_else(|| panic!("wcc carries component_count: {before}"));
    let assigned = before["data"]["components"]
        .as_object()
        .unwrap_or_else(|| panic!("wcc carries components: {before}"))
        .len();
    assert_eq!(assigned, 6, "every node is assigned a component");
    assert_eq!(components_before, 2, "the seeded graph plus the isolate");

    support::run(
        &mut executor,
        &json!({"type": "graph_add_edge", "graph": "mr", "src": "f", "dst": "a",
                "edge_type": "knows", "properties": {}}),
    );
    let after = support::run(
        &mut executor,
        &json!({"type": "graph_wcc", "graph": "mr", "budget": budget}),
    );
    assert_eq!(
        after["data"]["component_count"].as_u64(),
        Some(components_before - 1),
        "one bridge edge merges exactly two components"
    );
}
