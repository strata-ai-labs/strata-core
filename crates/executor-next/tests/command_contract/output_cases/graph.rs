use crate::support::*;

pub(super) fn graph_outputs() -> Vec<Output> {
    let mut outputs = graph_read_outputs();
    outputs.extend(graph_write_outputs());
    outputs
}

#[allow(clippy::too_many_lines)]
pub(super) fn graph_read_outputs() -> Vec<Output> {
    vec![
        Output::GraphInfo(GraphInfoData::new("deps".to_owned(), 2, 1, 1, 10, 4, 40)),
        Output::GraphInfoResult(Some(GraphInfoData::new(
            "deps".to_owned(),
            2,
            1,
            1,
            10,
            4,
            40,
        ))),
        Output::GraphInfoResult(None),
        Output::GraphNamePage {
            items: vec!["deps".to_owned()],
            page: PageInfo::new(true, Some("deps".to_owned())),
        },
        Output::GraphNamePage {
            items: Vec::new(),
            page: PageInfo::terminal(),
        },
        Output::GraphNodeResult(Some(graph_node_output("deps", "node-a"))),
        Output::GraphNodeResult(None),
        Output::GraphNodePage {
            items: vec![graph_node_output("deps", "node-a")],
            page: PageInfo::new(true, Some("node-a".to_owned())),
        },
        Output::GraphNodePage {
            items: Vec::new(),
            page: PageInfo::terminal(),
        },
        Output::GraphEdgeResult(Some(graph_edge_output(
            "deps",
            "node-a",
            "depends_on",
            "node-b",
        ))),
        Output::GraphEdgeResult(None),
        Output::GraphNeighborPage {
            items: vec![GraphNeighborHit::new(
                graph_node_output("deps", "node-b"),
                graph_edge_output("deps", "node-a", "depends_on", "node-b"),
                GraphDirection::Outgoing,
                Some("present".to_owned()),
            )],
            page: PageInfo::terminal(),
        },
        Output::GraphNeighborPage {
            items: vec![GraphNeighborHit::new(
                graph_node_output("deps", "node-a"),
                graph_edge_output("deps", "node-a", "depends_on", "node-b"),
                GraphDirection::Incoming,
                None,
            )],
            page: PageInfo::new(true, Some("incoming:node-a".to_owned())),
        },
        Output::GraphBindingPage {
            items: vec![GraphBindingHit::new(
                "deps".to_owned(),
                "node-a".to_owned(),
                graph_binding(),
                2,
                20,
            )],
            page: PageInfo::terminal(),
        },
        Output::GraphBindingPage {
            items: Vec::new(),
            page: PageInfo::terminal(),
        },
        Output::GraphOntologyResult(Some(graph_ontology_output("draft"))),
        Output::GraphOntologyResult(None),
        Output::GraphOntologySummaryResult(Some(graph_ontology_summary_output())),
        Output::GraphOntologySummaryResult(None),
        Output::GraphWccResult(graph_wcc_output()),
        Output::GraphLccResult(GraphLccData::new(
            "deps".to_owned(),
            [("node-a".to_owned(), 1.0), ("node-b".to_owned(), 0.0)]
                .into_iter()
                .collect(),
        )),
        Output::GraphSsspResult(GraphSsspData::new(
            "deps".to_owned(),
            "node-a".to_owned(),
            GraphDirection::Outgoing,
            [("node-a".to_owned(), 0.0), ("node-b".to_owned(), 1.5)]
                .into_iter()
                .collect(),
        )),
        Output::GraphPagerankResult(GraphPagerankData::new(
            "deps".to_owned(),
            [("node-a".to_owned(), 0.6), ("node-b".to_owned(), 0.4)]
                .into_iter()
                .collect(),
            12,
            true,
        )),
        Output::GraphCdlpResult(GraphCdlpData::new(
            "deps".to_owned(),
            [
                ("node-a".to_owned(), "node-a".to_owned()),
                ("node-b".to_owned(), "node-a".to_owned()),
            ]
            .into_iter()
            .collect(),
        )),
        Output::GraphBfsResult(graph_bfs_output()),
        Output::GraphDeletePolicyResult {
            policy: "cascade".to_owned(),
            nodes_affected: 2,
            effect: MutationEffect::new(true, MutationEffectKind::Deleted, true, 2),
            commit: Some(commit_receipt(3, 30, 3, 0)),
            version: Some(3),
            timestamp: Some(30),
        },
        Output::GraphDeletePolicyResult {
            policy: "keep_dangling".to_owned(),
            nodes_affected: 2,
            effect: MutationEffect::new(false, MutationEffectKind::Unchanged, true, 2),
            commit: None,
            version: None,
            timestamp: None,
        },
    ]
}

#[allow(clippy::too_many_lines)]
pub(super) fn graph_write_outputs() -> Vec<Output> {
    vec![
        Output::GraphNodeWriteResult {
            graph: "deps".to_owned(),
            node_id: "node-a".to_owned(),
            created: true,
            effect: MutationEffect::created(),
            commit: commit_receipt(2, 20, 2, 0),
            version: 2,
            timestamp: 20,
        },
        Output::GraphNodeWriteResult {
            graph: "deps".to_owned(),
            node_id: "node-a".to_owned(),
            created: false,
            effect: MutationEffect::updated(),
            commit: commit_receipt(3, 30, 2, 0),
            version: 3,
            timestamp: 30,
        },
        Output::GraphEdgeWriteResult {
            graph: "deps".to_owned(),
            src: "node-a".to_owned(),
            edge_type: "depends_on".to_owned(),
            dst: "node-b".to_owned(),
            created: true,
            effect: MutationEffect::created(),
            commit: commit_receipt(3, 30, 2, 0),
            version: 3,
            timestamp: 30,
        },
        Output::GraphEdgeWriteResult {
            graph: "deps".to_owned(),
            src: "node-a".to_owned(),
            edge_type: "depends_on".to_owned(),
            dst: "node-b".to_owned(),
            created: false,
            effect: MutationEffect::updated(),
            commit: commit_receipt(4, 40, 2, 0),
            version: 4,
            timestamp: 40,
        },
        Output::GraphDeleteResult {
            graph: "deps".to_owned(),
            node_id: Some("node-a".to_owned()),
            src: None,
            edge_type: None,
            dst: None,
            deleted: true,
            effect: MutationEffect::deleted(),
            commit: Some(commit_receipt(4, 40, 0, 1)),
            version: Some(4),
            timestamp: Some(40),
        },
        Output::GraphDeleteResult {
            graph: "deps".to_owned(),
            node_id: None,
            src: Some("node-a".to_owned()),
            edge_type: Some("depends_on".to_owned()),
            dst: Some("node-b".to_owned()),
            deleted: false,
            effect: MutationEffect::not_found(),
            commit: None,
            version: None,
            timestamp: None,
        },
        Output::GraphBatchWriteResult {
            graph: "deps".to_owned(),
            batch: graph_batch(vec![
                GraphBatchItemResult::new_with_effect(
                    0,
                    "upsert_node",
                    Some(true),
                    None,
                    MutationEffect::created(),
                    Some(commit_receipt(5, 50, 2, 1)),
                    Some(5),
                    Some(50),
                ),
                GraphBatchItemResult::new_with_effect(
                    1,
                    "delete_edge",
                    None,
                    Some(false),
                    MutationEffect::not_found(),
                    None,
                    None,
                    None,
                ),
                GraphBatchItemResult::failed(2, "upsert_edge", "invalid graph edge"),
            ]),
        },
        Output::GraphOntologyWriteResult {
            graph: "deps".to_owned(),
            kind: "object".to_owned(),
            type_name: "Document".to_owned(),
            created: true,
            effect: MutationEffect::created(),
            commit: commit_receipt(5, 50, 1, 0),
            version: 5,
            timestamp: 50,
        },
        Output::GraphOntologyDeleteResult {
            graph: "deps".to_owned(),
            kind: "link".to_owned(),
            type_name: "wrote".to_owned(),
            deleted: true,
            effect: MutationEffect::deleted(),
            commit: Some(commit_receipt(6, 60, 1, 0)),
            version: Some(6),
            timestamp: Some(60),
        },
        Output::GraphOntologyDeleteResult {
            graph: "deps".to_owned(),
            kind: "object".to_owned(),
            type_name: "Missing".to_owned(),
            deleted: false,
            effect: MutationEffect::not_found(),
            commit: None,
            version: None,
            timestamp: None,
        },
        Output::GraphOntologyFreezeResult {
            graph: "deps".to_owned(),
            object_types: 2,
            link_types: 1,
            commit: commit_receipt(7, 70, 1, 0),
            version: 7,
            timestamp: 70,
        },
    ]
}
