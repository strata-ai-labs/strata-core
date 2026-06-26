use super::{
    batch_item_error_status, CommitReceipt, Deserialize, ErrorStatus, ExecutorError,
    MutationEffect, Serialize, Value,
};

/// Graph neighbor traversal direction.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphDirection {
    /// Outgoing edges from the selected node.
    #[default]
    Outgoing,
    /// Incoming edges into the selected node.
    Incoming,
    /// Incoming and outgoing edges.
    Both,
}

/// Product primitive kind used by graph entity bindings.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphBindingPrimitive {
    /// KV primitive.
    Kv,
    /// JSON primitive.
    Json,
    /// Vector primitive.
    Vector,
    /// Event primitive.
    Event,
    /// Graph primitive.
    Graph,
}

/// Typed product identity attached to a graph node.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GraphBindingTarget {
    primitive: GraphBindingPrimitive,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    branch: Option<String>,
    space: String,
    key: String,
}

impl GraphBindingTarget {
    /// Creates a graph binding target.
    pub fn new(
        primitive: GraphBindingPrimitive,
        branch: Option<String>,
        space: impl Into<String>,
        key: impl Into<String>,
    ) -> Self {
        Self {
            primitive,
            branch,
            space: space.into(),
            key: key.into(),
        }
    }

    /// Returns the primitive kind.
    pub const fn primitive(&self) -> GraphBindingPrimitive {
        self.primitive
    }

    /// Returns the optional target branch.
    pub fn branch(&self) -> Option<&str> {
        self.branch.as_deref()
    }

    /// Returns the target product space.
    pub fn space(&self) -> &str {
        &self.space
    }

    /// Returns the target key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Consumes the target.
    pub fn into_parts(self) -> (GraphBindingPrimitive, Option<String>, String, String) {
        (self.primitive, self.branch, self.space, self.key)
    }
}

/// Node-to-entity binding.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GraphEntityBinding {
    target: GraphBindingTarget,
}

impl GraphEntityBinding {
    /// Creates a graph entity binding.
    pub const fn new(target: GraphBindingTarget) -> Self {
        Self { target }
    }

    /// Returns the bound target.
    pub const fn target(&self) -> &GraphBindingTarget {
        &self.target
    }

    /// Consumes the binding.
    pub fn into_target(self) -> GraphBindingTarget {
        self.target
    }
}

/// Graph node input payload.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GraphNodeData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    properties: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    binding: Option<GraphEntityBinding>,
}

impl GraphNodeData {
    /// Creates graph node data.
    pub const fn new(properties: Option<Value>, binding: Option<GraphEntityBinding>) -> Self {
        Self {
            properties,
            binding,
        }
    }

    /// Returns optional node properties.
    pub const fn properties(&self) -> Option<&Value> {
        self.properties.as_ref()
    }

    /// Returns optional entity binding.
    pub const fn binding(&self) -> Option<&GraphEntityBinding> {
        self.binding.as_ref()
    }

    /// Consumes the payload.
    pub fn into_parts(self) -> (Option<Value>, Option<GraphEntityBinding>) {
        (self.properties, self.binding)
    }
}

/// Graph edge input payload.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct GraphEdgeData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    weight: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    properties: Option<Value>,
}

impl GraphEdgeData {
    /// Creates graph edge data.
    pub const fn new(weight: Option<f64>, properties: Option<Value>) -> Self {
        Self { weight, properties }
    }

    /// Returns optional edge weight.
    pub const fn weight(&self) -> Option<f64> {
        self.weight
    }

    /// Returns optional edge properties.
    pub const fn properties(&self) -> Option<&Value> {
        self.properties.as_ref()
    }

    /// Consumes the payload.
    pub fn into_parts(self) -> (Option<f64>, Option<Value>) {
        (self.weight, self.properties)
    }
}

/// One graph batch write operation.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum GraphBatchOperation {
    /// Upserts one node.
    UpsertNode {
        /// Node id.
        node_id: String,
        /// Node payload.
        data: GraphNodeData,
    },
    /// Deletes one node and incident edges.
    DeleteNode {
        /// Node id.
        node_id: String,
    },
    /// Upserts one edge.
    UpsertEdge {
        /// Source node id.
        src: String,
        /// Edge type.
        edge_type: String,
        /// Destination node id.
        dst: String,
        /// Edge payload.
        data: GraphEdgeData,
    },
    /// Deletes one edge.
    DeleteEdge {
        /// Source node id.
        src: String,
        /// Edge type.
        edge_type: String,
        /// Destination node id.
        dst: String,
    },
}

/// Serializable graph metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct GraphInfoData {
    graph: String,
    node_count: u64,
    edge_count: u64,
    created_version: u64,
    created_timestamp: u64,
    updated_version: u64,
    updated_timestamp: u64,
}

impl GraphInfoData {
    /// Creates graph metadata output.
    pub const fn new(
        graph: String,
        node_count: u64,
        edge_count: u64,
        created_version: u64,
        created_timestamp: u64,
        updated_version: u64,
        updated_timestamp: u64,
    ) -> Self {
        Self {
            graph,
            node_count,
            edge_count,
            created_version,
            created_timestamp,
            updated_version,
            updated_timestamp,
        }
    }

    /// Returns the graph name.
    pub fn graph(&self) -> &str {
        &self.graph
    }

    /// Returns visible node count.
    pub const fn node_count(&self) -> u64 {
        self.node_count
    }

    /// Returns visible edge count.
    pub const fn edge_count(&self) -> u64 {
        self.edge_count
    }

    /// Returns metadata creation version.
    pub const fn created_version(&self) -> u64 {
        self.created_version
    }

    /// Returns metadata creation timestamp.
    pub const fn created_timestamp(&self) -> u64 {
        self.created_timestamp
    }

    /// Returns latest graph-state update version.
    pub const fn updated_version(&self) -> u64 {
        self.updated_version
    }

    /// Returns latest graph-state update timestamp.
    pub const fn updated_timestamp(&self) -> u64 {
        self.updated_timestamp
    }
}

/// Serializable graph node output.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GraphNodeDataOutput {
    graph: String,
    node_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    properties: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    binding: Option<GraphEntityBinding>,
    version: u64,
    timestamp: u64,
}

impl GraphNodeDataOutput {
    /// Creates graph node output.
    pub const fn new(
        graph: String,
        node_id: String,
        properties: Option<Value>,
        binding: Option<GraphEntityBinding>,
        version: u64,
        timestamp: u64,
    ) -> Self {
        Self {
            graph,
            node_id,
            properties,
            binding,
            version,
            timestamp,
        }
    }

    /// Returns the graph name.
    pub fn graph(&self) -> &str {
        &self.graph
    }

    /// Returns the node id.
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// Returns optional properties.
    pub const fn properties(&self) -> Option<&Value> {
        self.properties.as_ref()
    }

    /// Returns optional entity binding.
    pub const fn binding(&self) -> Option<&GraphEntityBinding> {
        self.binding.as_ref()
    }

    /// Returns commit version.
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Returns commit timestamp.
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }
}

/// Serializable graph edge output.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GraphEdgeDataOutput {
    graph: String,
    src: String,
    edge_type: String,
    dst: String,
    weight: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    properties: Option<Value>,
    version: u64,
    timestamp: u64,
}

impl GraphEdgeDataOutput {
    /// Creates graph edge output.
    #[allow(clippy::too_many_arguments)]
    pub const fn new(
        graph: String,
        src: String,
        edge_type: String,
        dst: String,
        weight: f64,
        properties: Option<Value>,
        version: u64,
        timestamp: u64,
    ) -> Self {
        Self {
            graph,
            src,
            edge_type,
            dst,
            weight,
            properties,
            version,
            timestamp,
        }
    }

    /// Returns the graph name.
    pub fn graph(&self) -> &str {
        &self.graph
    }

    /// Returns the source node id.
    pub fn src(&self) -> &str {
        &self.src
    }

    /// Returns the edge type.
    pub fn edge_type(&self) -> &str {
        &self.edge_type
    }

    /// Returns the destination node id.
    pub fn dst(&self) -> &str {
        &self.dst
    }

    /// Returns the edge weight.
    pub const fn weight(&self) -> f64 {
        self.weight
    }

    /// Returns optional properties.
    pub const fn properties(&self) -> Option<&Value> {
        self.properties.as_ref()
    }

    /// Returns commit version.
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Returns commit timestamp.
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }
}

/// Serializable graph neighbor hit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GraphNeighborHit {
    node: GraphNodeDataOutput,
    edge: GraphEdgeDataOutput,
    direction: GraphDirection,
}

impl GraphNeighborHit {
    /// Creates a graph neighbor hit.
    pub const fn new(
        node: GraphNodeDataOutput,
        edge: GraphEdgeDataOutput,
        direction: GraphDirection,
    ) -> Self {
        Self {
            node,
            edge,
            direction,
        }
    }

    /// Returns the neighboring node.
    pub const fn node(&self) -> &GraphNodeDataOutput {
        &self.node
    }

    /// Returns the connecting edge.
    pub const fn edge(&self) -> &GraphEdgeDataOutput {
        &self.edge
    }

    /// Returns which direction produced the hit.
    pub const fn direction(&self) -> GraphDirection {
        self.direction
    }
}

/// Serializable graph entity binding hit.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GraphBindingHit {
    graph: String,
    node_id: String,
    binding: GraphEntityBinding,
    version: u64,
    timestamp: u64,
}

impl GraphBindingHit {
    /// Creates a graph binding hit.
    pub const fn new(
        graph: String,
        node_id: String,
        binding: GraphEntityBinding,
        version: u64,
        timestamp: u64,
    ) -> Self {
        Self {
            graph,
            node_id,
            binding,
            version,
            timestamp,
        }
    }

    /// Returns the graph name.
    pub fn graph(&self) -> &str {
        &self.graph
    }

    /// Returns the node id.
    pub fn node_id(&self) -> &str {
        &self.node_id
    }

    /// Returns the entity binding.
    pub const fn binding(&self) -> &GraphEntityBinding {
        &self.binding
    }

    /// Returns commit version.
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Returns commit timestamp.
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }
}

/// Positional graph batch write result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GraphBatchItemResult {
    operation_index: u64,
    operation: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    created: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    deleted: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    effect: Option<MutationEffect>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    commit: Option<CommitReceipt>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    version: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    timestamp: Option<u64>,
    error: Option<ErrorStatus>,
}

impl GraphBatchItemResult {
    /// Creates a successful graph batch item result.
    pub fn new(
        operation_index: u64,
        operation: impl Into<String>,
        created: Option<bool>,
        deleted: Option<bool>,
        version: Option<u64>,
        timestamp: Option<u64>,
    ) -> Self {
        Self {
            operation_index,
            operation: operation.into(),
            created,
            deleted,
            effect: None,
            commit: None,
            version,
            timestamp,
            error: None,
        }
    }

    /// Creates a successful graph batch item result with shared mutation facts.
    #[allow(clippy::too_many_arguments)]
    pub fn new_with_effect(
        operation_index: u64,
        operation: impl Into<String>,
        created: Option<bool>,
        deleted: Option<bool>,
        effect: MutationEffect,
        commit: Option<CommitReceipt>,
        version: Option<u64>,
        timestamp: Option<u64>,
    ) -> Self {
        Self {
            operation_index,
            operation: operation.into(),
            created,
            deleted,
            effect: Some(effect),
            commit,
            version,
            timestamp,
            error: None,
        }
    }

    /// Creates a failed graph batch item result.
    pub fn failed(
        operation_index: u64,
        operation: impl Into<String>,
        error: impl Into<String>,
    ) -> Self {
        Self::failed_status(operation_index, operation, batch_item_error_status(error))
    }

    /// Creates a failed graph batch item result from an executor error.
    pub fn failed_error(
        operation_index: u64,
        operation: impl Into<String>,
        error: ExecutorError,
    ) -> Self {
        Self::failed_status(operation_index, operation, error.into_status())
    }

    /// Creates a failed graph batch item result from a public error status.
    pub fn failed_status(
        operation_index: u64,
        operation: impl Into<String>,
        error: ErrorStatus,
    ) -> Self {
        Self {
            operation_index,
            operation: operation.into(),
            created: None,
            deleted: None,
            effect: None,
            commit: None,
            version: None,
            timestamp: None,
            error: Some(error),
        }
    }

    /// Returns the input operation index.
    pub const fn operation_index(&self) -> u64 {
        self.operation_index
    }

    /// Returns the operation kind.
    pub fn operation(&self) -> &str {
        &self.operation
    }

    /// Returns create/update fact.
    pub const fn created(&self) -> Option<bool> {
        self.created
    }

    /// Returns delete/no-op fact.
    pub const fn deleted(&self) -> Option<bool> {
        self.deleted
    }

    /// Returns mutation effect facts for successful items.
    pub const fn effect(&self) -> Option<&MutationEffect> {
        self.effect.as_ref()
    }

    /// Returns commit receipt when this item applied a mutation.
    pub const fn commit(&self) -> Option<&CommitReceipt> {
        self.commit.as_ref()
    }

    /// Returns commit version when present.
    pub const fn version(&self) -> Option<u64> {
        self.version
    }

    /// Returns commit timestamp when present.
    pub const fn timestamp(&self) -> Option<u64> {
        self.timestamp
    }

    /// Returns item error when present.
    pub fn error(&self) -> Option<&str> {
        self.error.as_ref().map(ErrorStatus::message)
    }

    /// Returns the structured item error status.
    pub const fn error_status(&self) -> Option<&ErrorStatus> {
        self.error.as_ref()
    }
}
