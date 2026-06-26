use crate::support::*;

pub(super) fn graph_commands() -> Vec<Command> {
    vec![
        Command::GraphCreate {
            branch: None,
            space: None,
            graph: "deps".to_owned(),
        },
        Command::GraphDelete {
            branch: None,
            space: None,
            graph: "deps".to_owned(),
        },
        Command::GraphList {
            branch: None,
            space: None,
            cursor: Some("deps".to_owned()),
            limit: Some(5),
        },
        Command::GraphGetMeta {
            branch: None,
            space: None,
            graph: "deps".to_owned(),
        },
        Command::GraphAddNode {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            graph: "deps".to_owned(),
            node_id: "node-a".to_owned(),
            properties: Some(json!({"kind": "root"})),
            binding: Some(graph_binding()),
        },
        Command::GraphGetNode {
            branch: None,
            space: None,
            graph: "deps".to_owned(),
            node_id: "node-a".to_owned(),
        },
        Command::GraphRemoveNode {
            branch: None,
            space: None,
            graph: "deps".to_owned(),
            node_id: "node-a".to_owned(),
        },
        Command::GraphListNodes {
            branch: None,
            space: None,
            graph: "deps".to_owned(),
            prefix: Some("node-".to_owned()),
            cursor: Some("node-a".to_owned()),
            limit: Some(5),
        },
        Command::GraphAddEdge {
            branch: None,
            space: None,
            graph: "deps".to_owned(),
            src: "node-a".to_owned(),
            edge_type: "depends_on".to_owned(),
            dst: "node-b".to_owned(),
            weight: Some(2.5),
            properties: Some(json!({"source": "test"})),
        },
        Command::GraphGetEdge {
            branch: None,
            space: None,
            graph: "deps".to_owned(),
            src: "node-a".to_owned(),
            edge_type: "depends_on".to_owned(),
            dst: "node-b".to_owned(),
        },
        Command::GraphRemoveEdge {
            branch: None,
            space: None,
            graph: "deps".to_owned(),
            src: "node-a".to_owned(),
            edge_type: "depends_on".to_owned(),
            dst: "node-b".to_owned(),
        },
        Command::GraphNeighbors {
            branch: None,
            space: None,
            graph: "deps".to_owned(),
            node_id: "node-a".to_owned(),
            direction: GraphDirection::Outgoing,
            edge_type: Some("depends_on".to_owned()),
            cursor: None,
            limit: Some(5),
        },
        Command::GraphBindingsForEntity {
            branch: None,
            space: None,
            target: graph_binding_target(),
            cursor: None,
            limit: Some(5),
        },
        Command::GraphBatchWrite {
            branch: None,
            space: None,
            graph: "deps".to_owned(),
            operations: Vec::new(),
        },
    ]
}

pub(super) fn graph_round_trip_edge_commands() -> Vec<Command> {
    vec![
        Command::GraphCreate {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            graph: "wide".to_owned(),
        },
        Command::GraphAddNode {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            graph: "wide".to_owned(),
            node_id: "node-empty".to_owned(),
            properties: Some(json!({})),
            binding: None,
        },
        Command::GraphAddEdge {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            graph: "wide".to_owned(),
            src: "node-a".to_owned(),
            edge_type: "relates_to".to_owned(),
            dst: "node-b".to_owned(),
            weight: None,
            properties: None,
        },
        Command::GraphNeighbors {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            graph: "wide".to_owned(),
            node_id: "node-a".to_owned(),
            direction: GraphDirection::Incoming,
            edge_type: None,
            cursor: Some("cursor".to_owned()),
            limit: Some(0),
        },
        Command::GraphNeighbors {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            graph: "wide".to_owned(),
            node_id: "node-a".to_owned(),
            direction: GraphDirection::Both,
            edge_type: Some("relates_to".to_owned()),
            cursor: None,
            limit: Some(1),
        },
        Command::GraphBatchWrite {
            branch: Some("feature".to_owned()),
            space: Some("space-a".to_owned()),
            graph: "wide".to_owned(),
            operations: vec![
                GraphBatchOperation::UpsertNode {
                    node_id: "node-a".to_owned(),
                    data: GraphNodeData::new(None, Some(graph_binding())),
                },
                GraphBatchOperation::DeleteNode {
                    node_id: "node-old".to_owned(),
                },
                GraphBatchOperation::UpsertEdge {
                    src: "node-a".to_owned(),
                    edge_type: "relates_to".to_owned(),
                    dst: "node-b".to_owned(),
                    data: GraphEdgeData::new(Some(1.25), Some(json!({"batch": true}))),
                },
                GraphBatchOperation::DeleteEdge {
                    src: "node-a".to_owned(),
                    edge_type: "relates_to".to_owned(),
                    dst: "node-b".to_owned(),
                },
            ],
        },
    ]
}

pub(super) fn graph_binding_target_round_trip_commands() -> Vec<Command> {
    [
        GraphBindingPrimitive::Kv,
        GraphBindingPrimitive::Json,
        GraphBindingPrimitive::Vector,
        GraphBindingPrimitive::Event,
        GraphBindingPrimitive::Graph,
    ]
    .into_iter()
    .enumerate()
    .map(|(index, primitive)| Command::GraphBindingsForEntity {
        branch: Some("feature".to_owned()),
        space: Some("space-a".to_owned()),
        target: GraphBindingTarget::new(
            primitive,
            (index % 2 == 0).then(|| "entity-branch".to_owned()),
            "entity-space",
            format!("entity-{index}"),
        ),
        cursor: Some(format!("cursor-{index}")),
        limit: Some(10),
    })
    .collect()
}
