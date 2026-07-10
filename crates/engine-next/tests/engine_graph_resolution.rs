//! GI1 exit gate: binding target resolution — dangling references are
//! explicit. Neighbors report the bound entity's status (present /
//! deleted / missing / malformed / unsupported) at the read's temporal
//! context, and the standalone resolver answers for arbitrary targets.

#![allow(clippy::too_many_lines)]

mod common;

use strata_engine_next::{
    Database, GraphBindingPrimitive, GraphBindingTarget, GraphDirection, GraphEdgeData,
    GraphEdgeType, GraphEntityBinding, GraphName, GraphNodeData, GraphNodeId, GraphTargetStatus,
    KvKey, KvValue, ProductSpace,
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

fn target(primitive: GraphBindingPrimitive, key: &str) -> GraphBindingTarget {
    GraphBindingTarget::new(
        primitive,
        None,
        ProductSpace::new("default").expect("space"),
        key,
    )
    .expect("valid target")
}

fn bound_node_data(primitive: GraphBindingPrimitive, key: &str) -> GraphNodeData {
    GraphNodeData::new(None, Some(GraphEntityBinding::new(target(primitive, key))))
}

/// Seeds `hub -links-> <node>` so neighbor reads surface the node's
/// binding status.
fn link_from_hub(
    graph: &mut strata_engine_next::GraphService<'_>,
    name: &GraphName,
    node: &str,
    data: GraphNodeData,
) {
    graph
        .upsert_node(name, node_id(node), data)
        .expect("node upserts");
    graph
        .upsert_edge(
            name,
            node_id("hub"),
            GraphEdgeType::new("links").expect("edge type"),
            node_id(node),
            GraphEdgeData::default_weight(None),
        )
        .expect("edge upserts");
}

fn neighbor_status(
    graph: &mut strata_engine_next::GraphService<'_>,
    name: &GraphName,
    node: &str,
) -> Option<GraphTargetStatus> {
    let page = graph
        .neighbors(
            name,
            &node_id("hub"),
            GraphDirection::Outgoing,
            None,
            None,
            100,
        )
        .expect("neighbors read");
    page.neighbors()
        .iter()
        .find(|neighbor| neighbor.node().node_id() == &node_id(node))
        .expect("neighbor present")
        .target_status()
}

#[test]
fn neighbors_report_target_status_in_cache_and_durable_modes() {
    run_database_modes(exercise_target_status);
}

fn exercise_target_status(mut database: Database) {
    // Author the bound KV entity first.
    {
        let mut kv = database
            .kv(branch("default"), space("default"))
            .expect("kv service opens");
        kv.put(
            KvKey::new("doc-live").expect("key"),
            KvValue::new(b"payload".to_vec()),
        )
        .expect("kv put");
        kv.put(
            KvKey::new("doc-doomed").expect("key"),
            KvValue::new(b"payload".to_vec()),
        )
        .expect("kv put");
    }

    let mut graph = database
        .graph(branch("default"), space("default"))
        .expect("graph service opens");
    let name = graph_name("refs");
    graph.create_graph(name.clone()).expect("graph created");
    graph
        .upsert_node(&name, node_id("hub"), GraphNodeData::default())
        .expect("hub upserts");

    link_from_hub(
        &mut graph,
        &name,
        "live",
        bound_node_data(GraphBindingPrimitive::Kv, "doc-live"),
    );
    link_from_hub(
        &mut graph,
        &name,
        "doomed",
        bound_node_data(GraphBindingPrimitive::Kv, "doc-doomed"),
    );
    link_from_hub(
        &mut graph,
        &name,
        "ghost",
        bound_node_data(GraphBindingPrimitive::Kv, "doc-never-written"),
    );
    link_from_hub(
        &mut graph,
        &name,
        "malformed",
        // Event targets are addressed by sequence number; a non-numeric
        // key cannot address an event row.
        bound_node_data(GraphBindingPrimitive::Event, "not-a-sequence"),
    );
    link_from_hub(
        &mut graph,
        &name,
        "vectorish",
        bound_node_data(GraphBindingPrimitive::Vector, "collection"),
    );
    link_from_hub(&mut graph, &name, "unbound", GraphNodeData::default());

    // Live target: present. Never-written: missing. Composite: unsupported.
    assert_eq!(
        neighbor_status(&mut graph, &name, "live"),
        Some(GraphTargetStatus::Present)
    );
    assert_eq!(
        neighbor_status(&mut graph, &name, "ghost"),
        Some(GraphTargetStatus::Missing)
    );
    assert_eq!(
        neighbor_status(&mut graph, &name, "malformed"),
        Some(GraphTargetStatus::MalformedTarget)
    );
    assert_eq!(
        neighbor_status(&mut graph, &name, "vectorish"),
        Some(GraphTargetStatus::Unsupported)
    );
    assert_eq!(neighbor_status(&mut graph, &name, "unbound"), None);

    // The standalone resolver answers for arbitrary targets.
    assert_eq!(
        graph
            .resolve_binding_target(&target(GraphBindingPrimitive::Kv, "doc-live"))
            .expect("resolves"),
        GraphTargetStatus::Present
    );

    // Capture the pre-delete version, then delete the bound entity.
    let live_version = graph
        .graph_info(&name)
        .expect("info reads")
        .expect("graph visible")
        .updated_version();
    drop(graph);
    {
        let mut kv = database
            .kv(branch("default"), space("default"))
            .expect("kv service opens");
        kv.delete(KvKey::new("doc-doomed").expect("key"))
            .expect("kv delete");
    }
    let mut graph = database
        .graph(branch("default"), space("default"))
        .expect("graph service opens");

    // The binding survives; traversal now reports the deletion instead
    // of silently keeping a dangling reference.
    assert_eq!(
        neighbor_status(&mut graph, &name, "doomed"),
        Some(GraphTargetStatus::Deleted)
    );
    assert_eq!(
        graph
            .resolve_binding_target(&target(GraphBindingPrimitive::Kv, "doc-doomed"))
            .expect("resolves"),
        GraphTargetStatus::Deleted
    );

    // Historical reads resolve at the historical context: the target
    // was still present when the graph last changed.
    assert_eq!(
        graph
            .resolve_binding_target_at_version(
                &target(GraphBindingPrimitive::Kv, "doc-doomed"),
                live_version,
            )
            .expect("resolves"),
        GraphTargetStatus::Present
    );
    let page = graph
        .neighbors_at_version(
            &name,
            &node_id("hub"),
            GraphDirection::Outgoing,
            None,
            None,
            100,
            live_version,
        )
        .expect("historical neighbors read");
    let doomed = page
        .neighbors()
        .iter()
        .find(|neighbor| neighbor.node().node_id() == &node_id("doomed"))
        .expect("neighbor present");
    assert_eq!(doomed.target_status(), Some(GraphTargetStatus::Present));
}

#[test]
fn delete_policies_apply_in_cache_and_durable_modes() {
    run_database_modes(exercise_delete_policies);
}

fn exercise_delete_policies(mut database: Database) {
    let mut graph = database
        .graph(branch("default"), space("default"))
        .expect("graph service opens");
    let name = graph_name("facts");
    graph.create_graph(name.clone()).expect("graph created");
    graph
        .upsert_node(&name, node_id("hub"), GraphNodeData::default())
        .expect("hub upserts");
    // Two nodes bound to doc-cascade (one with an incident edge), one to
    // doc-detach, one to doc-keep, one bound elsewhere as a control.
    for (node, key) in [
        ("c1", "doc-cascade"),
        ("c2", "doc-cascade"),
        ("d1", "doc-detach"),
        ("k1", "doc-keep"),
        ("other", "doc-other"),
    ] {
        link_from_hub(
            &mut graph,
            &name,
            node,
            bound_node_data(GraphBindingPrimitive::Kv, key),
        );
    }

    // Cascade: bound nodes and their incident edges disappear.
    let outcome = graph
        .apply_binding_delete_policy(
            &target(GraphBindingPrimitive::Kv, "doc-cascade"),
            strata_engine_next::GraphDeletePolicy::Cascade,
        )
        .expect("cascade applies");
    assert_eq!(outcome.nodes_affected(), 2);
    assert!(outcome.commit().is_some());
    assert!(graph
        .get_node(&name, &node_id("c1"))
        .expect("read")
        .is_none());
    let hub_neighbors = graph
        .neighbors(
            &name,
            &node_id("hub"),
            GraphDirection::Outgoing,
            None,
            None,
            100,
        )
        .expect("neighbors read");
    assert!(
        hub_neighbors
            .neighbors()
            .iter()
            .all(|n| n.node().node_id() != &node_id("c1") && n.node().node_id() != &node_id("c2")),
        "cascaded nodes leave traversal"
    );
    let page = graph
        .bindings_for_entity(&target(GraphBindingPrimitive::Kv, "doc-cascade"), None, 100)
        .expect("bindings read");
    assert!(page.bindings().is_empty(), "cascade clears the reverse map");

    // Detach: the node survives without its binding.
    let outcome = graph
        .apply_binding_delete_policy(
            &target(GraphBindingPrimitive::Kv, "doc-detach"),
            strata_engine_next::GraphDeletePolicy::Detach,
        )
        .expect("detach applies");
    assert_eq!(outcome.nodes_affected(), 1);
    assert!(outcome.commit().is_some());
    let detached = graph
        .get_node(&name, &node_id("d1"))
        .expect("read")
        .expect("node survives");
    assert!(detached.data().binding().is_none(), "binding removed");
    let page = graph
        .bindings_for_entity(&target(GraphBindingPrimitive::Kv, "doc-detach"), None, 100)
        .expect("bindings read");
    assert!(page.bindings().is_empty(), "detach clears the reverse map");

    // Keep-dangling: nothing mutates; traversal reports the target.
    let outcome = graph
        .apply_binding_delete_policy(
            &target(GraphBindingPrimitive::Kv, "doc-keep"),
            strata_engine_next::GraphDeletePolicy::KeepDangling,
        )
        .expect("keep-dangling applies");
    assert_eq!(outcome.nodes_affected(), 1);
    assert!(outcome.commit().is_none(), "no rows change");
    let kept = graph
        .get_node(&name, &node_id("k1"))
        .expect("read")
        .expect("node survives");
    assert!(kept.data().binding().is_some(), "binding preserved");
    assert_eq!(
        neighbor_status(&mut graph, &name, "k1"),
        Some(GraphTargetStatus::Missing),
        "traversal reports the unwritten target"
    );

    // The control node bound to a different target is untouched.
    assert!(graph
        .get_node(&name, &node_id("other"))
        .expect("read")
        .expect("node survives")
        .data()
        .binding()
        .is_some());

    // An unbound target is a clean no-op.
    let outcome = graph
        .apply_binding_delete_policy(
            &target(GraphBindingPrimitive::Kv, "doc-nobody"),
            strata_engine_next::GraphDeletePolicy::Cascade,
        )
        .expect("no-op applies");
    assert_eq!(outcome.nodes_affected(), 0);
    assert!(outcome.commit().is_none());
}
