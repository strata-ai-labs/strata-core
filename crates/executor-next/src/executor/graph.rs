use super::{
    engine_graph_batch_operation, engine_graph_binding_target, engine_graph_direction,
    engine_graph_edge_data, engine_graph_node_data, graph_batch_operation_name,
    graph_batch_write_output, graph_binding_page_output, graph_delete_output,
    graph_edge_data_output, graph_edge_type, graph_edge_write_output, graph_info_data, graph_name,
    graph_name_page_output, graph_neighbor_page_output, graph_node_data_output, graph_node_id,
    graph_node_page_output, graph_node_write_output, optional_graph_edge_type, optional_graph_name,
    optional_graph_node_id, optional_limit, EngineGraphBatchWrite, Executor, ExecutorResult,
    GraphBatchOperation, GraphBindingTarget, GraphDirection, GraphEdgeData, GraphEntityBinding,
    GraphNodeData, Output, DEFAULT_GRAPH_LIST_LIMIT,
};

impl Executor {
    pub(super) fn execute_graph_create(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        graph: String,
    ) -> ExecutorResult<Output> {
        let graph = graph_name(graph)?;
        let mut service = self.graph_service(branch, space)?;
        Ok(Output::GraphInfo(graph_info_data(
            &service.create_graph(graph)?,
        )))
    }

    pub(super) fn execute_graph_delete(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        graph: String,
    ) -> ExecutorResult<Output> {
        let graph = graph_name(graph)?;
        let mut service = self.graph_service(branch, space)?;
        Ok(graph_delete_output(
            &service.delete_graph(&graph)?,
            None,
            None,
            None,
            None,
        ))
    }

    pub(super) fn execute_graph_list(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        cursor: Option<String>,
        limit: Option<u64>,
    ) -> ExecutorResult<Output> {
        let cursor = optional_graph_name(cursor)?;
        let limit = optional_limit(limit)?.unwrap_or(DEFAULT_GRAPH_LIST_LIMIT);
        let mut service = self.graph_service(branch, space)?;
        Ok(graph_name_page_output(
            &service.list_graphs(cursor.as_ref(), limit)?,
        ))
    }

    pub(super) fn execute_graph_get_meta(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        graph: String,
    ) -> ExecutorResult<Output> {
        let graph = graph_name(graph)?;
        let mut service = self.graph_service(branch, space)?;
        Ok(Output::GraphInfoResult(
            service.graph_info(&graph)?.as_ref().map(graph_info_data),
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn execute_graph_add_node(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        graph: String,
        node_id: String,
        properties: Option<serde_json::Value>,
        binding: Option<GraphEntityBinding>,
    ) -> ExecutorResult<Output> {
        let graph = graph_name(graph)?;
        let node_id = graph_node_id(node_id)?;
        let data = engine_graph_node_data(GraphNodeData::new(properties, binding))?;
        let mut service = self.graph_service(branch, space)?;
        Ok(graph_node_write_output(
            &service.upsert_node(&graph, node_id, data)?,
        ))
    }

    pub(super) fn execute_graph_get_node(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        graph: String,
        node_id: String,
    ) -> ExecutorResult<Output> {
        let graph = graph_name(graph)?;
        let node_id = graph_node_id(node_id)?;
        let mut service = self.graph_service(branch, space)?;
        Ok(Output::GraphNodeResult(
            service
                .get_node(&graph, &node_id)?
                .as_ref()
                .map(graph_node_data_output),
        ))
    }

    pub(super) fn execute_graph_remove_node(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        graph: String,
        node_id: String,
    ) -> ExecutorResult<Output> {
        let graph = graph_name(graph)?;
        let node_id = graph_node_id(node_id)?;
        let mut service = self.graph_service(branch, space)?;
        Ok(graph_delete_output(
            &service.delete_node(&graph, &node_id)?,
            Some(node_id.as_str().to_owned()),
            None,
            None,
            None,
        ))
    }

    pub(super) fn execute_graph_list_nodes(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        graph: String,
        prefix: Option<String>,
        cursor: Option<String>,
        limit: Option<u64>,
    ) -> ExecutorResult<Output> {
        let graph = graph_name(graph)?;
        let prefix = optional_graph_node_id(prefix)?;
        let cursor = optional_graph_node_id(cursor)?;
        let limit = optional_limit(limit)?.unwrap_or(DEFAULT_GRAPH_LIST_LIMIT);
        let mut service = self.graph_service(branch, space)?;
        Ok(graph_node_page_output(&service.list_nodes(
            &graph,
            prefix.as_ref(),
            cursor.as_ref(),
            limit,
        )?))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn execute_graph_add_edge(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        graph: String,
        src: String,
        edge_type: String,
        dst: String,
        weight: Option<f64>,
        properties: Option<serde_json::Value>,
    ) -> ExecutorResult<Output> {
        let graph = graph_name(graph)?;
        let src = graph_node_id(src)?;
        let edge_type = graph_edge_type(edge_type)?;
        let dst = graph_node_id(dst)?;
        let data = engine_graph_edge_data(GraphEdgeData::new(weight, properties))?;
        let mut service = self.graph_service(branch, space)?;
        Ok(graph_edge_write_output(
            &service.upsert_edge(&graph, src, edge_type, dst, data)?,
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn execute_graph_get_edge(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        graph: String,
        src: String,
        edge_type: String,
        dst: String,
    ) -> ExecutorResult<Output> {
        let graph = graph_name(graph)?;
        let src = graph_node_id(src)?;
        let edge_type = graph_edge_type(edge_type)?;
        let dst = graph_node_id(dst)?;
        let mut service = self.graph_service(branch, space)?;
        Ok(Output::GraphEdgeResult(
            service
                .get_edge(&graph, &src, &edge_type, &dst)?
                .as_ref()
                .map(graph_edge_data_output),
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn execute_graph_remove_edge(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        graph: String,
        src: String,
        edge_type: String,
        dst: String,
    ) -> ExecutorResult<Output> {
        let graph = graph_name(graph)?;
        let src = graph_node_id(src)?;
        let edge_type = graph_edge_type(edge_type)?;
        let dst = graph_node_id(dst)?;
        let mut service = self.graph_service(branch, space)?;
        Ok(graph_delete_output(
            &service.delete_edge(&graph, &src, &edge_type, &dst)?,
            None,
            Some(src.as_str().to_owned()),
            Some(edge_type.as_str().to_owned()),
            Some(dst.as_str().to_owned()),
        ))
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn execute_graph_neighbors(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        graph: String,
        node_id: String,
        direction: GraphDirection,
        edge_type: Option<String>,
        cursor: Option<&str>,
        limit: Option<u64>,
    ) -> ExecutorResult<Output> {
        let graph = graph_name(graph)?;
        let node_id = graph_node_id(node_id)?;
        let direction = engine_graph_direction(direction);
        let edge_type = optional_graph_edge_type(edge_type)?;
        let limit = optional_limit(limit)?.unwrap_or(DEFAULT_GRAPH_LIST_LIMIT);
        let mut service = self.graph_service(branch, space)?;
        Ok(graph_neighbor_page_output(&service.neighbors(
            &graph,
            &node_id,
            direction,
            edge_type.as_ref(),
            cursor,
            limit,
        )?))
    }

    pub(super) fn execute_graph_bindings_for_entity(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        target: GraphBindingTarget,
        cursor: Option<&str>,
        limit: Option<u64>,
    ) -> ExecutorResult<Output> {
        let target = engine_graph_binding_target(target)?;
        let limit = optional_limit(limit)?.unwrap_or(DEFAULT_GRAPH_LIST_LIMIT);
        let mut service = self.graph_service(branch, space)?;
        Ok(graph_binding_page_output(
            &service.bindings_for_entity(&target, cursor, limit)?,
        ))
    }

    pub(super) fn execute_graph_batch_write(
        &mut self,
        branch: Option<&str>,
        space: Option<&str>,
        graph: String,
        operations: Vec<GraphBatchOperation>,
    ) -> ExecutorResult<Output> {
        let graph = graph_name(graph)?;
        let operation_names = operations
            .iter()
            .map(graph_batch_operation_name)
            .collect::<Vec<_>>();
        let operations = operations
            .into_iter()
            .map(engine_graph_batch_operation)
            .collect::<ExecutorResult<Vec<_>>>()?;
        let batch = EngineGraphBatchWrite::new(operations);
        let mut service = self.graph_service(branch, space)?;
        Ok(graph_batch_write_output(
            &service.batch_write(&graph, &batch)?,
            &operation_names,
        ))
    }
}
