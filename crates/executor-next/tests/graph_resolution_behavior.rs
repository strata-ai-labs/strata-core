//! GI1 executor behavior: neighbor hits carry the bound entity's
//! resolution status on the wire — dangling references are explicit
//! through the command surface.

#![allow(clippy::result_large_err)]

use strata_executor_next::{
    Command, Executor, GraphBindingPrimitive, GraphBindingTarget, GraphDirection,
    GraphEntityBinding, Output,
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

fn kv_binding(key: &str) -> GraphEntityBinding {
    GraphEntityBinding::new(GraphBindingTarget::new(
        GraphBindingPrimitive::Kv,
        None,
        "default",
        key,
    ))
}

fn neighbor_status(executor: &mut Executor, node: &str) -> Option<String> {
    let output = executor
        .graph_neighbors("refs", "hub", GraphDirection::Outgoing, None, None, None)
        .expect("neighbors read");
    let Output::GraphNeighborPage { items, .. } = output else {
        panic!("unexpected neighbors output");
    };
    items
        .iter()
        .find(|hit| hit.node_id() == node)
        .expect("neighbor present")
        .target_status()
        .map(str::to_owned)
}

#[test]
fn neighbor_hits_carry_target_status_in_cache_and_durable_modes() {
    run_modes(exercise_wire_target_status);
}

fn exercise_wire_target_status(executor: &mut Executor) {
    executor.kv_put("doc-a", "payload").expect("kv put");
    executor.graph_create("refs").expect("graph created");
    executor
        .graph_add_node("refs", "hub", None, None)
        .expect("hub added");
    for (node, binding) in [
        ("bound", Some(kv_binding("doc-a"))),
        ("ghost", Some(kv_binding("doc-never"))),
        ("plain", None),
    ] {
        executor
            .execute(Command::GraphAddNode {
                branch: None,
                space: None,
                graph: "refs".to_owned(),
                node_id: node.to_owned(),
                properties: None,
                binding,
                object_type: None,
            })
            .expect("node added");
        executor
            .graph_add_edge("refs", "hub", "links", node, None, None)
            .expect("edge added");
    }

    assert_eq!(
        neighbor_status(executor, "bound").as_deref(),
        Some("present")
    );
    assert_eq!(
        neighbor_status(executor, "ghost").as_deref(),
        Some("missing")
    );
    assert_eq!(neighbor_status(executor, "plain"), None);

    // Deleting the bound entity flips the wire status to deleted; the
    // graph fact itself is preserved.
    executor.kv_delete("doc-a").expect("kv delete");
    assert_eq!(
        neighbor_status(executor, "bound").as_deref(),
        Some("deleted")
    );
}
