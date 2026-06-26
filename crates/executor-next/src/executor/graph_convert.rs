use super::{
    branch_name, commit_receipt, delete_effect, graph_batch_result, product_space, upsert_effect,
    usize_to_u64, CommitOutcome, EngineGraphBatchOpOutcome, EngineGraphBatchOperation,
    EngineGraphBatchWriteOutcome, EngineGraphBinding, EngineGraphBindingPage,
    EngineGraphBindingPrimitive, EngineGraphBindingTarget, EngineGraphDeleteOutcome,
    EngineGraphDirection, EngineGraphEdge, EngineGraphEdgeData, EngineGraphEdgeType,
    EngineGraphEdgeWriteOutcome, EngineGraphEntityBinding, EngineGraphInfo, EngineGraphName,
    EngineGraphNamePage, EngineGraphNeighbor, EngineGraphNeighborPage, EngineGraphNode,
    EngineGraphNodeData, EngineGraphNodeId, EngineGraphNodePage, EngineGraphProperties,
    EngineGraphWriteOutcome, ExecutorError, ExecutorResult, GraphBatchItemResult,
    GraphBatchOperation, GraphBindingHit, GraphBindingPrimitive, GraphBindingTarget,
    GraphDirection, GraphEdgeData, GraphEdgeDataOutput, GraphEntityBinding, GraphInfoData,
    GraphNeighborHit, GraphNodeData, GraphNodeDataOutput, MutationEffect, Output, PageInfo,
    DEFAULT_BRANCH,
};

pub(super) fn graph_name(name: String) -> ExecutorResult<EngineGraphName> {
    EngineGraphName::new(name).map_err(ExecutorError::from)
}

pub(super) fn optional_graph_name(name: Option<String>) -> ExecutorResult<Option<EngineGraphName>> {
    name.map(graph_name).transpose()
}

pub(super) fn graph_node_id(node_id: String) -> ExecutorResult<EngineGraphNodeId> {
    EngineGraphNodeId::new(node_id).map_err(ExecutorError::from)
}

pub(super) fn optional_graph_node_id(
    node_id: Option<String>,
) -> ExecutorResult<Option<EngineGraphNodeId>> {
    node_id.map(graph_node_id).transpose()
}

pub(super) fn graph_edge_type(edge_type: String) -> ExecutorResult<EngineGraphEdgeType> {
    EngineGraphEdgeType::new(edge_type).map_err(ExecutorError::from)
}

pub(super) fn optional_graph_edge_type(
    edge_type: Option<String>,
) -> ExecutorResult<Option<EngineGraphEdgeType>> {
    edge_type.map(graph_edge_type).transpose()
}

pub(super) fn engine_graph_properties(
    properties: Option<serde_json::Value>,
) -> ExecutorResult<Option<EngineGraphProperties>> {
    properties
        .map(EngineGraphProperties::new)
        .transpose()
        .map_err(ExecutorError::from)
}

pub(super) fn engine_graph_node_data(data: GraphNodeData) -> ExecutorResult<EngineGraphNodeData> {
    let (properties, binding) = data.into_parts();
    Ok(EngineGraphNodeData::new(
        engine_graph_properties(properties)?,
        binding.map(engine_graph_entity_binding).transpose()?,
    ))
}

pub(super) fn engine_graph_edge_data(data: GraphEdgeData) -> ExecutorResult<EngineGraphEdgeData> {
    let (weight, properties) = data.into_parts();
    let properties = engine_graph_properties(properties)?;
    if let Some(weight) = weight {
        return EngineGraphEdgeData::new(weight, properties).map_err(ExecutorError::from);
    }
    Ok(EngineGraphEdgeData::default_weight(properties))
}

pub(super) const fn engine_graph_direction(direction: GraphDirection) -> EngineGraphDirection {
    match direction {
        GraphDirection::Outgoing => EngineGraphDirection::Outgoing,
        GraphDirection::Incoming => EngineGraphDirection::Incoming,
        GraphDirection::Both => EngineGraphDirection::Both,
    }
}

pub(super) const fn output_graph_direction(direction: EngineGraphDirection) -> GraphDirection {
    match direction {
        EngineGraphDirection::Outgoing => GraphDirection::Outgoing,
        EngineGraphDirection::Incoming => GraphDirection::Incoming,
        EngineGraphDirection::Both => GraphDirection::Both,
    }
}

pub(super) const fn engine_graph_binding_primitive(
    primitive: GraphBindingPrimitive,
) -> EngineGraphBindingPrimitive {
    match primitive {
        GraphBindingPrimitive::Kv => EngineGraphBindingPrimitive::Kv,
        GraphBindingPrimitive::Json => EngineGraphBindingPrimitive::Json,
        GraphBindingPrimitive::Vector => EngineGraphBindingPrimitive::Vector,
        GraphBindingPrimitive::Event => EngineGraphBindingPrimitive::Event,
        GraphBindingPrimitive::Graph => EngineGraphBindingPrimitive::Graph,
    }
}

pub(super) const fn output_graph_binding_primitive(
    primitive: EngineGraphBindingPrimitive,
) -> GraphBindingPrimitive {
    match primitive {
        EngineGraphBindingPrimitive::Kv => GraphBindingPrimitive::Kv,
        EngineGraphBindingPrimitive::Json => GraphBindingPrimitive::Json,
        EngineGraphBindingPrimitive::Vector => GraphBindingPrimitive::Vector,
        EngineGraphBindingPrimitive::Event => GraphBindingPrimitive::Event,
        EngineGraphBindingPrimitive::Graph => GraphBindingPrimitive::Graph,
    }
}

pub(super) fn engine_graph_binding_target(
    target: GraphBindingTarget,
) -> ExecutorResult<EngineGraphBindingTarget> {
    let (primitive, branch, space, key) = target.into_parts();
    let branch = branch
        .as_deref()
        .map(|branch| branch_name(Some(branch), DEFAULT_BRANCH))
        .transpose()?;
    let space = product_space(Some(&space))?;
    EngineGraphBindingTarget::new(
        engine_graph_binding_primitive(primitive),
        branch,
        space,
        key,
    )
    .map_err(ExecutorError::from)
}

pub(super) fn engine_graph_entity_binding(
    binding: GraphEntityBinding,
) -> ExecutorResult<EngineGraphEntityBinding> {
    Ok(EngineGraphEntityBinding::new(engine_graph_binding_target(
        binding.into_target(),
    )?))
}

pub(super) fn engine_graph_batch_operation(
    operation: GraphBatchOperation,
) -> ExecutorResult<EngineGraphBatchOperation> {
    match operation {
        GraphBatchOperation::UpsertNode { node_id, data } => {
            Ok(EngineGraphBatchOperation::UpsertNode {
                node_id: graph_node_id(node_id)?,
                data: engine_graph_node_data(data)?,
            })
        }
        GraphBatchOperation::DeleteNode { node_id } => Ok(EngineGraphBatchOperation::DeleteNode {
            node_id: graph_node_id(node_id)?,
        }),
        GraphBatchOperation::UpsertEdge {
            src,
            edge_type,
            dst,
            data,
        } => Ok(EngineGraphBatchOperation::UpsertEdge {
            src: graph_node_id(src)?,
            edge_type: graph_edge_type(edge_type)?,
            dst: graph_node_id(dst)?,
            data: engine_graph_edge_data(data)?,
        }),
        GraphBatchOperation::DeleteEdge {
            src,
            edge_type,
            dst,
        } => Ok(EngineGraphBatchOperation::DeleteEdge {
            src: graph_node_id(src)?,
            edge_type: graph_edge_type(edge_type)?,
            dst: graph_node_id(dst)?,
        }),
    }
}

pub(super) const fn graph_batch_operation_name(operation: &GraphBatchOperation) -> &'static str {
    match operation {
        GraphBatchOperation::UpsertNode { .. } => "upsert_node",
        GraphBatchOperation::DeleteNode { .. } => "delete_node",
        GraphBatchOperation::UpsertEdge { .. } => "upsert_edge",
        GraphBatchOperation::DeleteEdge { .. } => "delete_edge",
    }
}

pub(super) fn output_graph_binding_target(target: &EngineGraphBindingTarget) -> GraphBindingTarget {
    GraphBindingTarget::new(
        output_graph_binding_primitive(target.primitive()),
        target.branch().map(|branch| branch.as_str().to_owned()),
        target.space().as_str().to_owned(),
        target.key().to_owned(),
    )
}

pub(super) fn output_graph_entity_binding(
    binding: &EngineGraphEntityBinding,
) -> GraphEntityBinding {
    GraphEntityBinding::new(output_graph_binding_target(binding.target()))
}

pub(super) fn graph_info_data(info: &EngineGraphInfo) -> GraphInfoData {
    GraphInfoData::new(
        info.name().as_str().to_owned(),
        info.node_count(),
        info.edge_count(),
        info.created_version().as_u64(),
        info.created_timestamp().as_micros(),
        info.updated_version().as_u64(),
        info.updated_timestamp().as_micros(),
    )
}

pub(super) fn graph_node_data_output(node: &EngineGraphNode) -> GraphNodeDataOutput {
    GraphNodeDataOutput::new(
        node.graph().as_str().to_owned(),
        node.node_id().as_str().to_owned(),
        node.data()
            .properties()
            .map(|properties| properties.as_inner().clone()),
        node.data().binding().map(output_graph_entity_binding),
        node.version().as_u64(),
        node.timestamp().as_micros(),
    )
}

pub(super) fn graph_edge_data_output(edge: &EngineGraphEdge) -> GraphEdgeDataOutput {
    GraphEdgeDataOutput::new(
        edge.graph().as_str().to_owned(),
        edge.src().as_str().to_owned(),
        edge.edge_type().as_str().to_owned(),
        edge.dst().as_str().to_owned(),
        edge.data().weight(),
        edge.data()
            .properties()
            .map(|properties| properties.as_inner().clone()),
        edge.version().as_u64(),
        edge.timestamp().as_micros(),
    )
}

pub(super) fn graph_neighbor_hit(neighbor: &EngineGraphNeighbor) -> GraphNeighborHit {
    GraphNeighborHit::new(
        graph_node_data_output(neighbor.node()),
        graph_edge_data_output(neighbor.edge()),
        output_graph_direction(neighbor.direction()),
    )
}

pub(super) fn graph_binding_hit(binding: &EngineGraphBinding) -> GraphBindingHit {
    GraphBindingHit::new(
        binding.graph().as_str().to_owned(),
        binding.node_id().as_str().to_owned(),
        output_graph_entity_binding(binding.binding()),
        binding.version().as_u64(),
        binding.timestamp().as_micros(),
    )
}

pub(super) fn graph_name_page_output(page: &EngineGraphNamePage) -> Output {
    Output::GraphNamePage {
        items: page
            .graphs()
            .iter()
            .map(|graph| graph.as_str().to_owned())
            .collect(),
        page: PageInfo::new(
            page.has_more(),
            page.cursor().map(|cursor| cursor.as_str().to_owned()),
        ),
    }
}

pub(super) fn graph_node_page_output(page: &EngineGraphNodePage) -> Output {
    Output::GraphNodePage {
        items: page.nodes().iter().map(graph_node_data_output).collect(),
        page: PageInfo::new(
            page.has_more(),
            page.cursor().map(|cursor| cursor.as_str().to_owned()),
        ),
    }
}

pub(super) fn graph_neighbor_page_output(page: &EngineGraphNeighborPage) -> Output {
    Output::GraphNeighborPage {
        items: page.neighbors().iter().map(graph_neighbor_hit).collect(),
        page: PageInfo::new(page.has_more(), page.cursor().map(str::to_owned)),
    }
}

pub(super) fn graph_binding_page_output(page: &EngineGraphBindingPage) -> Output {
    Output::GraphBindingPage {
        items: page.bindings().iter().map(graph_binding_hit).collect(),
        page: PageInfo::new(page.has_more(), page.cursor().map(str::to_owned)),
    }
}

pub(super) fn graph_node_write_output(outcome: &EngineGraphWriteOutcome) -> Output {
    let commit = outcome.commit();
    Output::GraphNodeWriteResult {
        graph: outcome.graph().as_str().to_owned(),
        node_id: outcome.node_id().as_str().to_owned(),
        created: outcome.created(),
        effect: upsert_effect(!outcome.created()),
        commit: commit_receipt(commit),
        version: commit.version().as_u64(),
        timestamp: commit.timestamp().as_micros(),
    }
}

pub(super) fn graph_edge_write_output(outcome: &EngineGraphEdgeWriteOutcome) -> Output {
    let commit = outcome.commit();
    Output::GraphEdgeWriteResult {
        graph: outcome.graph().as_str().to_owned(),
        src: outcome.src().as_str().to_owned(),
        edge_type: outcome.edge_type().as_str().to_owned(),
        dst: outcome.dst().as_str().to_owned(),
        created: outcome.created(),
        effect: upsert_effect(!outcome.created()),
        commit: commit_receipt(commit),
        version: commit.version().as_u64(),
        timestamp: commit.timestamp().as_micros(),
    }
}

pub(super) fn graph_delete_output(
    outcome: &EngineGraphDeleteOutcome,
    node_id: Option<String>,
    src: Option<String>,
    edge_type: Option<String>,
    dst: Option<String>,
) -> Output {
    Output::GraphDeleteResult {
        graph: outcome.graph().as_str().to_owned(),
        node_id,
        src,
        edge_type,
        dst,
        deleted: outcome.deleted(),
        effect: delete_effect(outcome.deleted()),
        commit: outcome.commit().map(commit_receipt),
        version: outcome.commit().map(|commit| commit.version().as_u64()),
        timestamp: outcome
            .commit()
            .map(|commit| commit.timestamp().as_micros()),
    }
}

pub(super) fn graph_batch_write_output(
    outcome: &EngineGraphBatchWriteOutcome,
    operation_names: &[&'static str],
) -> Output {
    let commit = outcome.commit();
    let version = commit.map(|commit| commit.version().as_u64());
    let timestamp = commit.map(|commit| commit.timestamp().as_micros());
    Output::GraphBatchWriteResult {
        graph: outcome.graph().as_str().to_owned(),
        batch: graph_batch_result(
            outcome
                .results()
                .iter()
                .map(|item| {
                    graph_batch_item_result(item, operation_names, commit, version, timestamp)
                })
                .collect(),
        ),
    }
}

pub(super) fn graph_batch_item_result(
    item: &EngineGraphBatchOpOutcome,
    operation_names: &[&'static str],
    commit: Option<CommitOutcome>,
    version: Option<u64>,
    timestamp: Option<u64>,
) -> GraphBatchItemResult {
    let operation_index = usize_to_u64(item.operation_index());
    let operation = operation_names
        .get(item.operation_index())
        .copied()
        .unwrap_or("unknown");
    let applied = graph_batch_item_applied(item);
    GraphBatchItemResult::new_with_effect(
        operation_index,
        operation,
        item.created_flag(),
        item.deleted_flag(),
        graph_batch_item_effect(item),
        applied.then(|| commit.map(commit_receipt)).flatten(),
        applied.then_some(version).flatten(),
        applied.then_some(timestamp).flatten(),
    )
}

pub(super) const fn graph_batch_item_applied(item: &EngineGraphBatchOpOutcome) -> bool {
    item.created_flag().is_some() || matches!(item.deleted_flag(), Some(true))
}

pub(super) fn graph_batch_item_effect(item: &EngineGraphBatchOpOutcome) -> MutationEffect {
    if let Some(created) = item.created_flag() {
        return upsert_effect(!created);
    }
    delete_effect(item.deleted_flag() == Some(true))
}
