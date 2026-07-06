use super::super::{
    Command, Executor, ExecutorResult, GraphBatchOperation, GraphBindingTarget, GraphDirection,
    GraphEntityBinding, Output,
};

impl Executor {
    /// Executes a default-branch graph-create command.
    pub fn graph_create(&mut self, graph: impl Into<String>) -> ExecutorResult<Output> {
        self.execute(Command::GraphCreate {
            branch: None,
            space: None,
            graph: graph.into(),
        })
    }

    /// Executes a default-branch graph-delete command.
    pub fn graph_delete(&mut self, graph: impl Into<String>) -> ExecutorResult<Output> {
        self.execute(Command::GraphDelete {
            branch: None,
            space: None,
            graph: graph.into(),
        })
    }

    /// Executes a default-branch graph-list command.
    pub fn graph_list(
        &mut self,
        cursor: Option<String>,
        limit: Option<u64>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::GraphList {
            branch: None,
            space: None,
            cursor,
            limit,
            as_of: None,
        })
    }

    /// Executes a default-branch graph-metadata command.
    pub fn graph_get_meta(&mut self, graph: impl Into<String>) -> ExecutorResult<Output> {
        self.execute(Command::GraphGetMeta {
            branch: None,
            space: None,
            graph: graph.into(),
            as_of: None,
        })
    }

    /// Executes a default-branch graph node upsert command.
    pub fn graph_add_node(
        &mut self,
        graph: impl Into<String>,
        node_id: impl Into<String>,
        properties: Option<serde_json::Value>,
        binding: Option<GraphEntityBinding>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::GraphAddNode {
            branch: None,
            space: None,
            graph: graph.into(),
            node_id: node_id.into(),
            properties,
            binding,
        })
    }

    /// Executes a default-branch graph node get command.
    pub fn graph_get_node(
        &mut self,
        graph: impl Into<String>,
        node_id: impl Into<String>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::GraphGetNode {
            branch: None,
            space: None,
            graph: graph.into(),
            node_id: node_id.into(),
            as_of: None,
        })
    }

    /// Executes a default-branch graph node delete command.
    pub fn graph_remove_node(
        &mut self,
        graph: impl Into<String>,
        node_id: impl Into<String>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::GraphRemoveNode {
            branch: None,
            space: None,
            graph: graph.into(),
            node_id: node_id.into(),
        })
    }

    /// Executes a default-branch graph node-list command.
    pub fn graph_list_nodes(
        &mut self,
        graph: impl Into<String>,
        prefix: Option<String>,
        cursor: Option<String>,
        limit: Option<u64>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::GraphListNodes {
            branch: None,
            space: None,
            graph: graph.into(),
            prefix,
            cursor,
            limit,
            as_of: None,
        })
    }

    /// Executes a default-branch graph edge upsert command.
    #[allow(clippy::too_many_arguments)]
    pub fn graph_add_edge(
        &mut self,
        graph: impl Into<String>,
        src: impl Into<String>,
        edge_type: impl Into<String>,
        dst: impl Into<String>,
        weight: Option<f64>,
        properties: Option<serde_json::Value>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::GraphAddEdge {
            branch: None,
            space: None,
            graph: graph.into(),
            src: src.into(),
            edge_type: edge_type.into(),
            dst: dst.into(),
            weight,
            properties,
        })
    }

    /// Executes a default-branch graph edge get command.
    pub fn graph_get_edge(
        &mut self,
        graph: impl Into<String>,
        src: impl Into<String>,
        edge_type: impl Into<String>,
        dst: impl Into<String>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::GraphGetEdge {
            branch: None,
            space: None,
            graph: graph.into(),
            src: src.into(),
            edge_type: edge_type.into(),
            dst: dst.into(),
            as_of: None,
        })
    }

    /// Executes a default-branch graph edge delete command.
    pub fn graph_remove_edge(
        &mut self,
        graph: impl Into<String>,
        src: impl Into<String>,
        edge_type: impl Into<String>,
        dst: impl Into<String>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::GraphRemoveEdge {
            branch: None,
            space: None,
            graph: graph.into(),
            src: src.into(),
            edge_type: edge_type.into(),
            dst: dst.into(),
        })
    }

    /// Executes a default-branch graph neighbors command.
    pub fn graph_neighbors(
        &mut self,
        graph: impl Into<String>,
        node_id: impl Into<String>,
        direction: GraphDirection,
        edge_type: Option<String>,
        cursor: Option<String>,
        limit: Option<u64>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::GraphNeighbors {
            branch: None,
            space: None,
            graph: graph.into(),
            node_id: node_id.into(),
            direction,
            edge_type,
            cursor,
            limit,
            as_of: None,
        })
    }

    /// Executes a default-branch graph binding lookup command.
    pub fn graph_bindings_for_entity(
        &mut self,
        target: GraphBindingTarget,
        cursor: Option<String>,
        limit: Option<u64>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::GraphBindingsForEntity {
            branch: None,
            space: None,
            target,
            cursor,
            limit,
            as_of: None,
        })
    }

    /// Executes a default-branch graph batch write command.
    pub fn graph_batch_write(
        &mut self,
        graph: impl Into<String>,
        operations: Vec<GraphBatchOperation>,
    ) -> ExecutorResult<Output> {
        self.execute(Command::GraphBatchWrite {
            branch: None,
            space: None,
            graph: graph.into(),
            operations,
        })
    }
}
