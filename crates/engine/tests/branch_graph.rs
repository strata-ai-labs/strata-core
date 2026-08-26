//! Graph capability branch compare (diff-only) + promotion exclusion (M12G-graph).

mod common;

use strata_engine::{
    BranchComparison, BranchStateSelector, ComparedCapability, Database, GraphEdgeData,
    GraphEdgeType, GraphName, GraphNodeData, GraphNodeId, PromotionStrategy, SpaceComparison,
};

use common::{branch, key, open_cache_database, space, value};

fn graph() -> GraphName {
    GraphName::new("deps").expect("valid graph")
}

fn add_node(database: &mut Database, branch_name: &str, node: &str) {
    database
        .graph(branch(branch_name), space("default"))
        .expect("graph service opens")
        .upsert_node(
            &graph(),
            GraphNodeId::new(node).expect("node id"),
            GraphNodeData::new(None, None),
        )
        .expect("upsert node");
}

fn capability(comparison: &BranchComparison, want: ComparedCapability) -> Option<&SpaceComparison> {
    comparison
        .comparisons()
        .iter()
        .find(|space| space.capability() == want)
}

#[test]
fn graph_is_compared_per_row_class_but_never_promoted() {
    let mut database = open_cache_database().expect("cache open succeeds");
    database
        .graph(branch("default"), space("default"))
        .expect("graph service opens")
        .create_graph(graph())
        .expect("create graph");
    add_node(&mut database, "default", "doc");
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("fork succeeds");

    // Feature adds a node, an edge, and changes a KV key; default is unchanged.
    add_node(&mut database, "feature", "chunk");
    database
        .graph(branch("feature"), space("default"))
        .expect("graph service opens")
        .upsert_edge(
            &graph(),
            GraphNodeId::new("doc").expect("src"),
            GraphEdgeType::new("contains").expect("edge type"),
            GraphNodeId::new("chunk").expect("dst"),
            GraphEdgeData::new(1.0, None).expect("edge data"),
        )
        .expect("upsert edge");
    database
        .kv(branch("feature"), space("default"))
        .expect("kv service opens")
        .put(key(b"k"), value(b"v"))
        .expect("kv put");

    // Compare reports graph changes per row class: the new node and the new edge.
    let before = database
        .branches()
        .expect("branch service opens")
        .compare(
            &branch("default"),
            &branch("feature"),
            BranchStateSelector::Current,
        )
        .expect("compare succeeds");
    assert_eq!(
        capability(&before, ComparedCapability::GraphNode)
            .expect("a graph node diff")
            .added()
            .len(),
        1,
        "feature added one node",
    );
    assert_eq!(
        capability(&before, ComparedCapability::GraphEdge)
            .expect("a graph edge diff")
            .added()
            .len(),
        1,
        "feature added one edge",
    );

    // Promote feature → default applies the KV change but never touches graph.
    let outcome = database
        .branches()
        .expect("branch service opens")
        .promote(
            &branch("feature"),
            &branch("default"),
            PromotionStrategy::Strict,
        )
        .expect("promote succeeds");
    assert!(!outcome.is_noop(), "the KV change is promoted");
    assert!(
        outcome.applied().iter().all(|entity| {
            !matches!(
                entity.capability(),
                ComparedCapability::GraphNode
                    | ComparedCapability::GraphEdge
                    | ComparedCapability::GraphOntology
            )
        }),
        "graph facts are never promoted",
    );

    // After the promote the graph still differs — proof it was left untouched.
    let after = database
        .branches()
        .expect("branch service opens")
        .compare(
            &branch("default"),
            &branch("feature"),
            BranchStateSelector::Current,
        )
        .expect("compare after promote");
    assert_eq!(
        capability(&after, ComparedCapability::GraphNode)
            .expect("graph node still differs")
            .added()
            .len(),
        1,
        "graph nodes were not promoted",
    );
}
