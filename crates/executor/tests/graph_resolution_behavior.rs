//! GI1 executor behavior: neighbor hits carry the bound entity's
//! resolution status on the wire — dangling references are explicit
//! through the command surface.

#![allow(clippy::result_large_err)]

use strata_executor::{
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

#[test]
fn delete_policy_command_applies_in_cache_and_durable_modes() {
    run_modes(exercise_delete_policy_command);
}

fn exercise_delete_policy_command(executor: &mut Executor) {
    use strata_executor::GraphDeletePolicy;

    executor.graph_create("facts").expect("graph created");
    executor
        .graph_add_node("facts", "hub", None, None)
        .expect("hub added");
    for (node, key) in [("c1", "doc-x"), ("c2", "doc-x"), ("d1", "doc-y")] {
        executor
            .execute(Command::GraphAddNode {
                branch: None,
                space: None,
                graph: "facts".to_owned(),
                node_id: node.to_owned(),
                properties: None,
                binding: Some(kv_binding(key)),
                object_type: None,
            })
            .expect("node added");
        executor
            .graph_add_edge("facts", "hub", "links", node, None, None)
            .expect("edge added");
    }

    let output = executor
        .graph_apply_delete_policy(
            kv_binding("doc-x").into_target(),
            GraphDeletePolicy::Cascade,
        )
        .expect("cascade applies");
    let Output::GraphDeletePolicyResult {
        policy,
        nodes_affected,
        commit,
        ..
    } = output
    else {
        panic!("unexpected delete-policy output");
    };
    assert_eq!(policy, "cascade");
    assert_eq!(nodes_affected, 2);
    assert!(commit.is_some());

    let output = executor
        .graph_apply_delete_policy(
            kv_binding("doc-y").into_target(),
            GraphDeletePolicy::KeepDangling,
        )
        .expect("keep-dangling applies");
    let Output::GraphDeletePolicyResult {
        policy,
        nodes_affected,
        commit,
        ..
    } = output
    else {
        panic!("unexpected delete-policy output");
    };
    assert_eq!(policy, "keep_dangling");
    assert_eq!(nodes_affected, 1);
    assert!(commit.is_none());
    // The kept binding surfaces as missing through traversal (doc-y was
    // never written to KV).
    let output = executor
        .graph_neighbors("facts", "hub", GraphDirection::Outgoing, None, None, None)
        .expect("neighbors read");
    let Output::GraphNeighborPage { items, .. } = output else {
        panic!("unexpected neighbors output");
    };
    let kept = items
        .iter()
        .find(|hit| hit.node_id() == "d1")
        .expect("kept neighbor");
    assert_eq!(kept.target_status(), Some("missing"));
    assert!(!items.iter().any(|hit| hit.node_id() == "c1"));
}
