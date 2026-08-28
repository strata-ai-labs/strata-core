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
fn test_promotion_keeps_a_deleted_space_with_target_only_graph_rows() {
    let mut database = open_cache_database().expect("cache open succeeds");
    let gspace = space("graphs");
    // The `graphs` space exists on both branches at the fork (part of the base).
    database
        .spaces(branch("default"))
        .expect("space service opens")
        .create(gspace.clone())
        .expect("create space");
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("fork succeeds");
    // The source deletes the space; the target adds target-only graph nodes and an
    // edge — graph rows are not promotable, so the space-deletion retain guard must
    // still see them.
    database
        .spaces(branch("feature"))
        .expect("space service opens")
        .delete(&gspace, true)
        .expect("delete space");
    {
        let mut graph_service = database
            .graph(branch("default"), gspace.clone())
            .expect("graph service opens");
        graph_service.create_graph(graph()).expect("create graph");
        graph_service
            .upsert_node(
                &graph(),
                GraphNodeId::new("a").expect("node id"),
                GraphNodeData::new(None, None),
            )
            .expect("upsert node a");
        graph_service
            .upsert_node(
                &graph(),
                GraphNodeId::new("b").expect("node id"),
                GraphNodeData::new(None, None),
            )
            .expect("upsert node b");
        graph_service
            .upsert_edge(
                &graph(),
                GraphNodeId::new("a").expect("node id"),
                GraphEdgeType::new("contains").expect("edge type"),
                GraphNodeId::new("b").expect("node id"),
                GraphEdgeData::new(1.0, None).expect("edge data"),
            )
            .expect("upsert edge");
    }

    let outcome = database
        .branches()
        .expect("branch service opens")
        .promote(
            &branch("feature"),
            &branch("default"),
            PromotionStrategy::Strict,
        )
        .expect("strict promote succeeds");
    assert!(outcome.conflicts().is_empty());

    // Deregistering the space would orphan the target-only graph, so a space the
    // target still holds graph rows in stays registered.
    assert!(
        database
            .spaces(branch("default"))
            .expect("space service opens")
            .exists(&gspace)
            .expect("exists succeeds"),
        "a space with target-only graph rows must stay registered"
    );
}

#[test]
fn test_empty_graph_creation_is_visible_in_the_diff() {
    let mut database = open_cache_database().expect("cache open succeeds");
    database
        .branches()
        .expect("branch service opens")
        .fork_current(&branch("default"), branch("feature"))
        .expect("fork succeeds");
    // feature creates an empty graph — a metadata row only, no nodes or edges.
    database
        .graph(branch("feature"), space("default"))
        .expect("graph service opens")
        .create_graph(graph())
        .expect("create graph");

    let comparison = database
        .branches()
        .expect("branch service opens")
        .compare(
            &branch("default"),
            &branch("feature"),
            BranchStateSelector::Current,
        )
        .expect("compare succeeds");

    // An empty graph's creation surfaces as a graph-metadata addition — the diff
    // is not empty just because the graph has no nodes or edges yet.
    let metadata = capability(&comparison, ComparedCapability::GraphMetadata)
        .expect("a graph metadata comparison is present");
    assert_eq!(metadata.added().len(), 1, "the created graph is added");
    assert!(metadata.modified().is_empty());
    assert!(metadata.removed().is_empty());
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
