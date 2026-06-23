//! Serializable request and response helper types.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Default product branch used when a command omits its branch.
pub const DEFAULT_BRANCH: &str = "default";

/// Default product space used when a command omits its space.
pub const DEFAULT_SPACE: &str = "default";

/// Byte-preserving command payload.
#[derive(Clone, Debug, Default, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct Bytes(#[serde(with = "serde_bytes")] Vec<u8>);

impl Bytes {
    /// Creates a byte payload.
    pub fn new(bytes: impl Into<Vec<u8>>) -> Self {
        Self(bytes.into())
    }

    /// Returns the raw bytes.
    pub fn as_slice(&self) -> &[u8] {
        &self.0
    }

    /// Consumes the wrapper and returns raw bytes.
    pub fn into_vec(self) -> Vec<u8> {
        self.0
    }

    /// Returns true when this payload has no bytes.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<Vec<u8>> for Bytes {
    fn from(value: Vec<u8>) -> Self {
        Self::new(value)
    }
}

impl From<&[u8]> for Bytes {
    fn from(value: &[u8]) -> Self {
        Self::new(value)
    }
}

impl<const N: usize> From<&[u8; N]> for Bytes {
    fn from(value: &[u8; N]) -> Self {
        Self::new(value.as_slice())
    }
}

impl From<&str> for Bytes {
    fn from(value: &str) -> Self {
        Self::new(value.as_bytes())
    }
}

/// Commit facts returned by mutating operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommitReceipt {
    version: u64,
    timestamp: u64,
    durable: bool,
    put_count: u64,
    delete_count: u64,
}

impl CommitReceipt {
    /// Creates a commit receipt.
    pub const fn new(
        version: u64,
        timestamp: u64,
        durable: bool,
        put_count: u64,
        delete_count: u64,
    ) -> Self {
        Self {
            version,
            timestamp,
            durable,
            put_count,
            delete_count,
        }
    }

    /// Returns the commit version.
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Returns the commit timestamp in microseconds.
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }

    /// Returns true when the commit reached durable storage.
    pub const fn durable(&self) -> bool {
        self.durable
    }

    /// Returns the number of put rows in the commit.
    pub const fn put_count(&self) -> u64 {
        self.put_count
    }

    /// Returns the number of delete rows in the commit.
    pub const fn delete_count(&self) -> u64 {
        self.delete_count
    }
}

/// High-level mutation effect for idempotent and conditional operations.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationEffectKind {
    /// A new logical entity was created.
    Created,
    /// An existing logical entity was updated.
    Updated,
    /// An existing logical entity was deleted.
    Deleted,
    /// The operation matched but left state unchanged.
    Unchanged,
    /// The operation did not match a visible entity.
    NotFound,
}

/// Normalized mutation effect facts.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MutationEffect {
    applied: bool,
    kind: MutationEffectKind,
    matched: bool,
    affected_count: u64,
}

impl MutationEffect {
    /// Creates mutation effect facts.
    pub const fn new(
        applied: bool,
        kind: MutationEffectKind,
        matched: bool,
        affected_count: u64,
    ) -> Self {
        Self {
            applied,
            kind,
            matched,
            affected_count,
        }
    }

    /// Returns true when the operation changed durable logical state.
    pub const fn applied(&self) -> bool {
        self.applied
    }

    /// Returns the normalized mutation kind.
    pub const fn kind(&self) -> MutationEffectKind {
        self.kind
    }

    /// Returns true when the operation matched an existing logical entity.
    pub const fn matched(&self) -> bool {
        self.matched
    }

    /// Returns the number of affected logical entities.
    pub const fn affected_count(&self) -> u64 {
        self.affected_count
    }
}

/// Database open target exposed in admin outputs.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdminOpenTarget {
    /// Volatile cache-backed database.
    Cache,
    /// Durable local filesystem-backed database.
    DurableLocal,
}

/// Health status exposed in admin outputs.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdminHealthStatus {
    /// All required facts are healthy.
    Healthy,
    /// Database is available with warnings.
    Degraded,
    /// A required subsystem is missing, corrupt, unavailable, or closed.
    Unhealthy,
}

/// Control-plane status exposed in admin health outputs.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdminControlStatus {
    /// Required facts are healthy.
    Healthy,
    /// Requested facts are missing.
    Missing,
    /// Required facts are corrupt.
    Corrupt,
    /// Required facts are unavailable.
    Unavailable,
}

/// Database information output.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdminDatabaseInfo {
    /// Engine package version.
    pub version: String,
    /// Open target.
    pub target: AdminOpenTarget,
    /// True when this open created a new database.
    pub created: bool,
    /// True when storage is durable.
    pub durable: bool,
    /// Default product branch.
    pub default_branch: String,
    /// Active branch count.
    pub branch_count: u64,
    /// Registered space count for the selected branch.
    pub space_count: u64,
    /// True while the database handle is open.
    pub open: bool,
}

/// Health output.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdminHealth {
    /// Worst health status.
    pub status: AdminHealthStatus,
    /// Database identity status.
    pub identity: AdminControlStatus,
    /// Registry status.
    pub registry: AdminControlStatus,
    /// Branch catalog status.
    pub branch_catalog: AdminControlStatus,
    /// Optional branch-local space catalog status.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub space_catalog: Option<AdminControlStatus>,
    /// Default product branch.
    pub default_branch: String,
    /// Active branch count.
    pub branch_count: u64,
}

/// Metrics output.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdminMetrics {
    /// Open target.
    pub target: AdminOpenTarget,
    /// True when storage is durable.
    pub durable: bool,
    /// True while the database handle is open.
    pub open: bool,
    /// Active branch count.
    pub branch_count: u64,
    /// Registered space count for the selected branch.
    pub space_count: u64,
    /// Control-plane health status.
    pub control_status: AdminHealthStatus,
}

/// Sanitized config output.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdminConfig {
    /// Open target.
    pub target: AdminOpenTarget,
    /// True when this open created a new database.
    pub created: bool,
    /// True when storage is durable.
    pub durable: bool,
    /// Default product branch.
    pub default_branch: String,
}

/// Capability flags in describe output.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdminCapabilities {
    /// KV primitive is available.
    pub kv: bool,
    /// JSON primitive is available.
    pub json: bool,
    /// Event primitive is available.
    pub event: bool,
    /// Vector primitive is available.
    pub vector: bool,
    /// Vector index query path is available.
    pub vector_index: bool,
    /// Graph core primitive is available.
    pub graph_core: bool,
    /// Arrow command surface is available.
    pub arrow: bool,
    /// Inference command surface is available.
    pub inference: bool,
}

/// Vector collection summary in describe output.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdminVectorCollection {
    /// Collection name.
    pub name: String,
    /// Embedding dimension.
    pub dimension: usize,
    /// Distance metric.
    pub metric: VectorDistanceMetric,
    /// Visible vector count.
    pub count: u64,
}

/// Graph summary in describe output.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdminGraph {
    /// Graph name.
    pub name: String,
    /// Visible node count.
    pub node_count: u64,
    /// Visible edge count.
    pub edge_count: u64,
}

/// Primitive summaries in describe output.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdminPrimitives {
    /// Visible KV row count in the described space.
    pub kv_count: u64,
    /// Visible JSON document count in the described space.
    pub json_count: u64,
    /// Visible event count in the described space.
    pub event_count: u64,
    /// Vector collection summaries.
    pub vector_collections: Vec<AdminVectorCollection>,
    /// Graph summaries.
    pub graphs: Vec<AdminGraph>,
}

/// Database describe output.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdminDescribe {
    /// Engine package version.
    pub version: String,
    /// Open target.
    pub target: AdminOpenTarget,
    /// Default product branch.
    pub default_branch: String,
    /// Described branch.
    pub branch: String,
    /// Active branches.
    pub branches: Vec<String>,
    /// Registered product spaces on the described branch.
    pub spaces: Vec<String>,
    /// Primitive summaries.
    pub primitives: AdminPrimitives,
    /// Sanitized config.
    pub config: AdminConfig,
    /// Available rebuilt capabilities.
    pub capabilities: AdminCapabilities,
}

/// Arrow file format selected for import/export.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArrowFileFormat {
    /// Apache Parquet.
    #[default]
    Parquet,
    /// Comma-separated values with a header row.
    Csv,
    /// Line-delimited JSON objects.
    Jsonl,
}

/// Product primitive targeted by Arrow import.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArrowImportTarget {
    /// Import rows into the KV primitive.
    Kv,
    /// Import rows into the JSON primitive.
    Json,
    /// Import rows into a vector collection.
    Vector,
}

/// Product primitive selected by Arrow export.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArrowExportPrimitive {
    /// Export the KV primitive.
    Kv,
    /// Export the JSON primitive.
    Json,
    /// Export the event primitive.
    Event,
    /// Export one vector collection.
    Vector,
    /// Export one graph as node and edge files.
    Graph,
}

/// Arrow import summary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ArrowImportResult {
    target: ArrowImportTarget,
    file_path: String,
    rows_imported: u64,
    rows_skipped: u64,
    batches_processed: u64,
}

impl ArrowImportResult {
    /// Creates an Arrow import summary.
    pub fn new(
        target: ArrowImportTarget,
        file_path: String,
        rows_imported: u64,
        rows_skipped: u64,
        batches_processed: u64,
    ) -> Self {
        Self {
            target,
            file_path,
            rows_imported,
            rows_skipped,
            batches_processed,
        }
    }

    /// Returns the imported primitive.
    pub const fn target(&self) -> ArrowImportTarget {
        self.target
    }

    /// Returns the input file path.
    pub fn file_path(&self) -> &str {
        &self.file_path
    }

    /// Returns imported row count.
    pub const fn rows_imported(&self) -> u64 {
        self.rows_imported
    }

    /// Returns skipped row count.
    pub const fn rows_skipped(&self) -> u64 {
        self.rows_skipped
    }

    /// Returns processed batch count.
    pub const fn batches_processed(&self) -> u64 {
        self.batches_processed
    }
}

/// Arrow export summary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ArrowExportResult {
    primitive: ArrowExportPrimitive,
    format: ArrowFileFormat,
    paths: Vec<String>,
    row_count: u64,
    size_bytes: u64,
}

impl ArrowExportResult {
    /// Creates an Arrow export summary.
    pub fn new(
        primitive: ArrowExportPrimitive,
        format: ArrowFileFormat,
        paths: Vec<String>,
        row_count: u64,
        size_bytes: u64,
    ) -> Self {
        Self {
            primitive,
            format,
            paths,
            row_count,
            size_bytes,
        }
    }

    /// Returns the exported primitive.
    pub const fn primitive(&self) -> ArrowExportPrimitive {
        self.primitive
    }

    /// Returns the written file format.
    pub const fn format(&self) -> ArrowFileFormat {
        self.format
    }

    /// Returns output paths.
    pub fn paths(&self) -> &[String] {
        &self.paths
    }

    /// Returns exported row count.
    pub const fn row_count(&self) -> u64 {
        self.row_count
    }

    /// Returns total output size in bytes.
    pub const fn size_bytes(&self) -> u64 {
        self.size_bytes
    }
}

/// Vector distance metric exposed through the command boundary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VectorDistanceMetric {
    /// Cosine similarity.
    #[default]
    Cosine,
    /// Euclidean similarity.
    Euclidean,
    /// Raw dot product.
    DotProduct,
}

/// Scalar value used by vector metadata filters.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type", content = "value")]
pub enum VectorScalar {
    /// JSON null.
    Null,
    /// Boolean scalar.
    Bool(bool),
    /// Numeric scalar.
    Number(f64),
    /// String scalar.
    String(String),
}

impl From<bool> for VectorScalar {
    fn from(value: bool) -> Self {
        Self::Bool(value)
    }
}

impl From<i32> for VectorScalar {
    fn from(value: i32) -> Self {
        Self::Number(f64::from(value))
    }
}

impl From<f64> for VectorScalar {
    fn from(value: f64) -> Self {
        Self::Number(value)
    }
}

impl From<&str> for VectorScalar {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

/// Vector metadata filter operation.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VectorFilterOp {
    /// Top-level equality.
    Eq,
}

/// One vector metadata filter condition.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VectorFilterCondition {
    field: String,
    op: VectorFilterOp,
    value: VectorScalar,
}

impl VectorFilterCondition {
    /// Creates a vector metadata filter condition.
    pub fn new(field: impl Into<String>, op: VectorFilterOp, value: VectorScalar) -> Self {
        Self {
            field: field.into(),
            op,
            value,
        }
    }

    /// Creates an equality condition.
    pub fn eq(field: impl Into<String>, value: impl Into<VectorScalar>) -> Self {
        Self::new(field, VectorFilterOp::Eq, value.into())
    }

    /// Returns the metadata field.
    pub fn field(&self) -> &str {
        &self.field
    }

    /// Returns the filter operation.
    pub const fn op(&self) -> VectorFilterOp {
        self.op
    }

    /// Returns the comparison value.
    pub const fn value(&self) -> &VectorScalar {
        &self.value
    }

    /// Consumes the condition.
    pub fn into_parts(self) -> (String, VectorFilterOp, VectorScalar) {
        (self.field, self.op, self.value)
    }
}

/// AND-composed vector metadata filter.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct VectorMetadataFilter {
    conditions: Vec<VectorFilterCondition>,
}

impl VectorMetadataFilter {
    /// Creates a vector metadata filter.
    pub fn new(conditions: Vec<VectorFilterCondition>) -> Self {
        Self { conditions }
    }

    /// Returns filter conditions.
    pub fn conditions(&self) -> &[VectorFilterCondition] {
        &self.conditions
    }

    /// Consumes the filter.
    pub fn into_conditions(self) -> Vec<VectorFilterCondition> {
        self.conditions
    }
}

/// One vector batch upsert entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BatchVectorEntry {
    key: String,
    vector: Vec<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    metadata: Option<Value>,
}

impl BatchVectorEntry {
    /// Creates a vector batch upsert entry.
    pub fn new(key: impl Into<String>, vector: Vec<f32>, metadata: Option<Value>) -> Self {
        Self {
            key: key.into(),
            vector,
            metadata,
        }
    }

    /// Returns the vector key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the embedding.
    pub fn vector(&self) -> &[f32] {
        &self.vector
    }

    /// Returns optional metadata.
    pub const fn metadata(&self) -> Option<&Value> {
        self.metadata.as_ref()
    }

    /// Consumes the entry.
    pub fn into_parts(self) -> (String, Vec<f32>, Option<Value>) {
        (self.key, self.vector, self.metadata)
    }
}

/// Event range direction exposed through the command boundary.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventRangeDirection {
    /// Increasing sequence or timestamp order.
    #[default]
    Forward,
    /// Decreasing sequence or timestamp order.
    Reverse,
}

/// One event batch append entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BatchEventEntry {
    event_type: String,
    payload: Value,
}

impl BatchEventEntry {
    /// Creates an event batch append entry.
    pub fn new(event_type: impl Into<String>, payload: Value) -> Self {
        Self {
            event_type: event_type.into(),
            payload,
        }
    }

    /// Returns the event type.
    pub fn event_type(&self) -> &str {
        &self.event_type
    }

    /// Returns the event payload.
    pub const fn payload(&self) -> &Value {
        &self.payload
    }

    /// Consumes the entry.
    pub fn into_parts(self) -> (String, Value) {
        (self.event_type, self.payload)
    }
}

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
    version: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    timestamp: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    error: Option<String>,
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
        Self {
            operation_index,
            operation: operation.into(),
            created: None,
            deleted: None,
            version: None,
            timestamp: None,
            error: Some(error.into()),
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
        self.error.as_deref()
    }
}

/// Event record payload and chain facts.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EventData {
    sequence: u64,
    event_type: String,
    payload: Value,
    timestamp: u64,
    previous_hash: String,
    hash: String,
}

impl EventData {
    /// Creates an event record.
    pub fn new(
        sequence: u64,
        event_type: String,
        payload: Value,
        timestamp: u64,
        previous_hash: String,
        hash: String,
    ) -> Self {
        Self {
            sequence,
            event_type,
            payload,
            timestamp,
            previous_hash,
            hash,
        }
    }

    /// Returns the event sequence.
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Returns the event type.
    pub fn event_type(&self) -> &str {
        &self.event_type
    }

    /// Returns the event payload.
    pub const fn payload(&self) -> &Value {
        &self.payload
    }

    /// Returns the event append timestamp.
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }

    /// Returns the previous event hash as lowercase hex.
    pub fn previous_hash(&self) -> &str {
        &self.previous_hash
    }

    /// Returns this event hash as lowercase hex.
    pub fn hash(&self) -> &str {
        &self.hash
    }
}

/// Event record with commit metadata.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EventVersionedData {
    event: EventData,
    version: u64,
    timestamp: u64,
}

impl EventVersionedData {
    /// Creates a versioned event record.
    pub fn new(event: EventData, version: u64, timestamp: u64) -> Self {
        Self {
            event,
            version,
            timestamp,
        }
    }

    /// Returns the event record.
    pub const fn event(&self) -> &EventData {
        &self.event
    }

    /// Returns the commit version.
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Returns the commit timestamp.
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }
}

/// Positional event batch append result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EventBatchAppendItemResult {
    sequence: Option<u64>,
    event_type: Option<String>,
    version: Option<u64>,
    timestamp: Option<u64>,
    error: Option<String>,
}

impl EventBatchAppendItemResult {
    /// Creates an event batch append result.
    pub fn new(
        sequence: Option<u64>,
        event_type: Option<String>,
        version: Option<u64>,
        timestamp: Option<u64>,
    ) -> Self {
        Self {
            sequence,
            event_type,
            version,
            timestamp,
            error: None,
        }
    }

    /// Creates a failed event batch append result.
    pub fn failed(error: impl Into<String>) -> Self {
        Self {
            sequence: None,
            event_type: None,
            version: None,
            timestamp: None,
            error: Some(error.into()),
        }
    }

    /// Returns the assigned sequence for successful items.
    pub const fn sequence(&self) -> Option<u64> {
        self.sequence
    }

    /// Returns the event type for successful items.
    pub fn event_type(&self) -> Option<&str> {
        self.event_type.as_deref()
    }

    /// Returns the commit version for successful items.
    pub const fn version(&self) -> Option<u64> {
        self.version
    }

    /// Returns the commit timestamp for successful items.
    pub const fn timestamp(&self) -> Option<u64> {
        self.timestamp
    }

    /// Returns the item error when validation failed.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

/// Event hash-chain verification result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct EventChainVerification {
    is_valid: bool,
    length: u64,
    first_invalid: Option<u64>,
    error: Option<String>,
}

impl EventChainVerification {
    /// Creates an event chain verification result.
    pub fn new(
        is_valid: bool,
        length: u64,
        first_invalid: Option<u64>,
        error: Option<String>,
    ) -> Self {
        Self {
            is_valid,
            length,
            first_invalid,
            error,
        }
    }

    /// Returns true when the visible event log is dense and hash-linked.
    pub const fn is_valid(&self) -> bool {
        self.is_valid
    }

    /// Returns the checked event count.
    pub const fn length(&self) -> u64 {
        self.length
    }

    /// Returns the first invalid sequence, if any.
    pub const fn first_invalid(&self) -> Option<u64> {
        self.first_invalid
    }

    /// Returns the verification error, if any.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

/// Vector collection facts.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VectorCollectionInfo {
    name: String,
    dimension: u64,
    metric: VectorDistanceMetric,
    count: u64,
}

impl VectorCollectionInfo {
    /// Creates vector collection facts.
    pub fn new(name: String, dimension: u64, metric: VectorDistanceMetric, count: u64) -> Self {
        Self {
            name,
            dimension,
            metric,
            count,
        }
    }

    /// Returns the collection name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the embedding dimension.
    pub const fn dimension(&self) -> u64 {
        self.dimension
    }

    /// Returns the distance metric.
    pub const fn metric(&self) -> VectorDistanceMetric {
        self.metric
    }

    /// Returns the visible vector count.
    pub const fn count(&self) -> u64 {
        self.count
    }
}

/// Vector value payload.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VectorData {
    embedding: Vec<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    metadata: Option<Value>,
}

impl VectorData {
    /// Creates a vector value payload.
    pub fn new(embedding: Vec<f32>, metadata: Option<Value>) -> Self {
        Self {
            embedding,
            metadata,
        }
    }

    /// Returns the embedding.
    pub fn embedding(&self) -> &[f32] {
        &self.embedding
    }

    /// Returns optional metadata.
    pub const fn metadata(&self) -> Option<&Value> {
        self.metadata.as_ref()
    }
}

/// Vector value with commit metadata.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VectorVersionedData {
    key: String,
    data: VectorData,
    version: u64,
    timestamp: u64,
    vector_revision: u64,
}

impl VectorVersionedData {
    /// Creates a versioned vector value.
    pub fn new(
        key: String,
        data: VectorData,
        version: u64,
        timestamp: u64,
        vector_revision: u64,
    ) -> Self {
        Self {
            key,
            data,
            version,
            timestamp,
            vector_revision,
        }
    }

    /// Returns the vector key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the vector payload.
    pub const fn data(&self) -> &VectorData {
        &self.data
    }

    /// Returns the commit version.
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Returns the commit timestamp.
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }

    /// Returns the vector revision.
    pub const fn vector_revision(&self) -> u64 {
        self.vector_revision
    }
}

/// Vector history item.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VectorHistoryItem {
    key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    data: Option<VectorData>,
    version: u64,
    timestamp: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    vector_revision: Option<u64>,
    tombstone: bool,
}

impl VectorHistoryItem {
    /// Creates a vector history item.
    pub fn new(
        key: String,
        data: Option<VectorData>,
        version: u64,
        timestamp: u64,
        vector_revision: Option<u64>,
        tombstone: bool,
    ) -> Self {
        Self {
            key,
            data,
            version,
            timestamp,
            vector_revision,
            tombstone,
        }
    }

    /// Returns the vector key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns vector data when this item is not a tombstone.
    pub const fn data(&self) -> Option<&VectorData> {
        self.data.as_ref()
    }

    /// Returns the commit version.
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Returns the commit timestamp.
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }

    /// Returns the vector revision when present.
    pub const fn vector_revision(&self) -> Option<u64> {
        self.vector_revision
    }

    /// Returns true for delete tombstones.
    pub const fn is_tombstone(&self) -> bool {
        self.tombstone
    }
}

/// One vector search match.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VectorMatch {
    key: String,
    score: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    metadata: Option<Value>,
}

impl VectorMatch {
    /// Creates a vector search match.
    pub fn new(key: String, score: f32, metadata: Option<Value>) -> Self {
        Self {
            key,
            score,
            metadata,
        }
    }

    /// Returns the vector key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the score.
    pub const fn score(&self) -> f32 {
        self.score
    }

    /// Returns optional metadata.
    pub const fn metadata(&self) -> Option<&Value> {
        self.metadata.as_ref()
    }
}

/// One vector index artifact diagnostic.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VectorIndexArtifactSource {
    artifact_id: String,
    status: String,
    searched: bool,
}

impl VectorIndexArtifactSource {
    /// Creates one vector index artifact diagnostic.
    pub const fn new(artifact_id: String, status: String, searched: bool) -> Self {
        Self {
            artifact_id,
            status,
            searched,
        }
    }

    /// Returns the artifact identity recorded in the engine manifest.
    pub fn artifact_id(&self) -> &str {
        &self.artifact_id
    }

    /// Returns the artifact load status.
    pub fn status(&self) -> &str {
        &self.status
    }

    /// Returns true when this artifact was searched.
    pub const fn searched(&self) -> bool {
        self.searched
    }
}

/// Vector index planner diagnostics.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VectorIndexDiagnostics {
    collection: String,
    manifest_status: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    manifest_generation: Option<u64>,
    manifest_ref_count: u64,
    manifest_inherited_ref_count: u64,
    manifest_owned_ref_count: u64,
    active_delta_count: u64,
    policy_mode: String,
    collection_exact_threshold: u64,
    source_flat_threshold: u64,
    source_hnsw_threshold: u64,
    overfetch_factor: u64,
    filtered_underfill_fallback: bool,
    active_delta_seal_threshold: u64,
    hnsw_memory_budget_bytes: u64,
    source_candidate_limit: u64,
    resolved_index_kind_summary: String,
    exact_fallback_count: u64,
    hnsw_graph_builds: u64,
    indexed_source_count: u64,
    exact_source_count: u64,
    flat_source_count: u64,
    hnsw_source_count: u64,
    active_delta_source_count: u64,
    indexed_vector_count: u64,
    derived_bytes: u64,
    last_query_used_index: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    last_query_fallback_reason: Option<String>,
    artifact_sources: Vec<VectorIndexArtifactSource>,
}

impl VectorIndexDiagnostics {
    /// Creates vector index planner diagnostics.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        collection: String,
        manifest_status: String,
        manifest_generation: Option<u64>,
        manifest_ref_count: u64,
        manifest_inherited_ref_count: u64,
        manifest_owned_ref_count: u64,
        active_delta_count: u64,
        policy_mode: String,
        collection_exact_threshold: u64,
        source_flat_threshold: u64,
        source_hnsw_threshold: u64,
        overfetch_factor: u64,
        filtered_underfill_fallback: bool,
        active_delta_seal_threshold: u64,
        hnsw_memory_budget_bytes: u64,
        source_candidate_limit: u64,
        resolved_index_kind_summary: String,
        exact_fallback_count: u64,
        hnsw_graph_builds: u64,
        indexed_source_count: u64,
        exact_source_count: u64,
        flat_source_count: u64,
        hnsw_source_count: u64,
        active_delta_source_count: u64,
        indexed_vector_count: u64,
        derived_bytes: u64,
        last_query_used_index: bool,
        last_query_fallback_reason: Option<String>,
        artifact_sources: Vec<VectorIndexArtifactSource>,
    ) -> Self {
        Self {
            collection,
            manifest_status,
            manifest_generation,
            manifest_ref_count,
            manifest_inherited_ref_count,
            manifest_owned_ref_count,
            active_delta_count,
            policy_mode,
            collection_exact_threshold,
            source_flat_threshold,
            source_hnsw_threshold,
            overfetch_factor,
            filtered_underfill_fallback,
            active_delta_seal_threshold,
            hnsw_memory_budget_bytes,
            source_candidate_limit,
            resolved_index_kind_summary,
            exact_fallback_count,
            hnsw_graph_builds,
            indexed_source_count,
            exact_source_count,
            flat_source_count,
            hnsw_source_count,
            active_delta_source_count,
            indexed_vector_count,
            derived_bytes,
            last_query_used_index,
            last_query_fallback_reason,
            artifact_sources,
        }
    }

    /// Returns the searched collection.
    pub fn collection(&self) -> &str {
        &self.collection
    }

    /// Returns the branch-local index manifest status.
    pub fn manifest_status(&self) -> &str {
        &self.manifest_status
    }

    /// Returns the loaded index manifest generation, when available.
    pub const fn manifest_generation(&self) -> Option<u64> {
        self.manifest_generation
    }

    /// Returns the total artifact ref count in the loaded manifest.
    pub const fn manifest_ref_count(&self) -> u64 {
        self.manifest_ref_count
    }

    /// Returns the inherited artifact ref count in the loaded manifest.
    pub const fn manifest_inherited_ref_count(&self) -> u64 {
        self.manifest_inherited_ref_count
    }

    /// Returns the branch-owned artifact ref count in the loaded manifest.
    pub const fn manifest_owned_ref_count(&self) -> u64 {
        self.manifest_owned_ref_count
    }

    /// Returns the active delta count advertised by the loaded manifest.
    pub const fn active_delta_count(&self) -> u64 {
        self.active_delta_count
    }

    /// Returns the resolved planner policy mode.
    pub fn policy_mode(&self) -> &str {
        &self.policy_mode
    }

    /// Returns the collection-level exact-search threshold.
    pub const fn collection_exact_threshold(&self) -> u64 {
        self.collection_exact_threshold
    }

    /// Returns the flat-source threshold.
    pub const fn source_flat_threshold(&self) -> u64 {
        self.source_flat_threshold
    }

    /// Returns the approximate-source threshold.
    pub const fn source_hnsw_threshold(&self) -> u64 {
        self.source_hnsw_threshold
    }

    /// Returns the candidate overfetch factor.
    pub const fn overfetch_factor(&self) -> u64 {
        self.overfetch_factor
    }

    /// Returns true when selective filters can fall back to exact search on underfill.
    pub const fn filtered_underfill_fallback(&self) -> bool {
        self.filtered_underfill_fallback
    }

    /// Returns the active-delta sealing threshold.
    pub const fn active_delta_seal_threshold(&self) -> u64 {
        self.active_delta_seal_threshold
    }

    /// Returns the approximate index memory budget.
    pub const fn hnsw_memory_budget_bytes(&self) -> u64 {
        self.hnsw_memory_budget_bytes
    }

    /// Returns the resolved per-source candidate limit.
    pub const fn source_candidate_limit(&self) -> u64 {
        self.source_candidate_limit
    }

    /// Returns the resolved source kind summary.
    pub fn resolved_index_kind_summary(&self) -> &str {
        &self.resolved_index_kind_summary
    }

    /// Returns true when the query used at least one indexed source.
    pub const fn last_query_used_index(&self) -> bool {
        self.last_query_used_index
    }

    /// Returns the planner fallback reason, if one was recorded.
    pub fn last_query_fallback_reason(&self) -> Option<&str> {
        self.last_query_fallback_reason.as_deref()
    }

    /// Returns the number of exact fallbacks taken by this query.
    pub const fn exact_fallback_count(&self) -> u64 {
        self.exact_fallback_count
    }

    /// Returns the number of approximate index graphs built while answering this query.
    pub const fn hnsw_graph_builds(&self) -> u64 {
        self.hnsw_graph_builds
    }

    /// Returns the number of indexed sources searched by this query.
    pub const fn indexed_source_count(&self) -> u64 {
        self.indexed_source_count
    }

    /// Returns the number of exact sources searched by this query.
    pub const fn exact_source_count(&self) -> u64 {
        self.exact_source_count
    }

    /// Returns the number of flat sources searched by this query.
    pub const fn flat_source_count(&self) -> u64 {
        self.flat_source_count
    }

    /// Returns the number of approximate sources searched by this query.
    pub const fn hnsw_source_count(&self) -> u64 {
        self.hnsw_source_count
    }

    /// Returns the number of active delta sources searched by this query.
    pub const fn active_delta_source_count(&self) -> u64 {
        self.active_delta_source_count
    }

    /// Returns the number of vectors covered by indexed sources.
    pub const fn indexed_vector_count(&self) -> u64 {
        self.indexed_vector_count
    }

    /// Returns the estimated derived bytes touched by indexed sources.
    pub const fn derived_bytes(&self) -> u64 {
        self.derived_bytes
    }

    /// Returns artifact load and search facts.
    pub fn artifact_sources(&self) -> &[VectorIndexArtifactSource] {
        &self.artifact_sources
    }
}

/// Vector index search output.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VectorIndexQueryResult {
    matches: Vec<VectorMatch>,
    diagnostics: VectorIndexDiagnostics,
}

impl VectorIndexQueryResult {
    /// Creates vector index search output.
    pub const fn new(matches: Vec<VectorMatch>, diagnostics: VectorIndexDiagnostics) -> Self {
        Self {
            matches,
            diagnostics,
        }
    }

    /// Returns search matches.
    pub fn matches(&self) -> &[VectorMatch] {
        &self.matches
    }

    /// Returns index planner diagnostics.
    pub const fn diagnostics(&self) -> &VectorIndexDiagnostics {
        &self.diagnostics
    }
}

/// Positional vector batch write/delete result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VectorBatchItemResult {
    applied: bool,
    version: Option<u64>,
    timestamp: Option<u64>,
    vector_revision: Option<u64>,
    error: Option<String>,
}

impl VectorBatchItemResult {
    /// Creates a vector batch result.
    pub const fn new(
        applied: bool,
        version: Option<u64>,
        timestamp: Option<u64>,
        vector_revision: Option<u64>,
    ) -> Self {
        Self {
            applied,
            version,
            timestamp,
            vector_revision,
            error: None,
        }
    }

    /// Creates a failed vector batch result.
    pub fn failed(error: impl Into<String>) -> Self {
        Self {
            applied: false,
            version: None,
            timestamp: None,
            vector_revision: None,
            error: Some(error.into()),
        }
    }

    /// Returns true when this item changed a visible row.
    pub const fn applied(&self) -> bool {
        self.applied
    }

    /// Returns the commit version when present.
    pub const fn version(&self) -> Option<u64> {
        self.version
    }

    /// Returns the commit timestamp when present.
    pub const fn timestamp(&self) -> Option<u64> {
        self.timestamp
    }

    /// Returns the vector revision when present.
    pub const fn vector_revision(&self) -> Option<u64> {
        self.vector_revision
    }

    /// Returns the item error when validation failed.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

/// Positional vector batch read result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct VectorBatchGetItemResult {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    value: Option<VectorVersionedData>,
    error: Option<String>,
}

impl VectorBatchGetItemResult {
    /// Creates a vector batch read result.
    pub const fn new(value: Option<VectorVersionedData>) -> Self {
        Self { value, error: None }
    }

    /// Creates a failed vector batch read result.
    pub fn failed(error: impl Into<String>) -> Self {
        Self {
            value: None,
            error: Some(error.into()),
        }
    }

    /// Returns the value when present.
    pub const fn value(&self) -> Option<&VectorVersionedData> {
        self.value.as_ref()
    }

    /// Returns the item error when validation failed.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

/// Entry for a batch KV write.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BatchKvEntry {
    key: Bytes,
    value: Bytes,
}

impl BatchKvEntry {
    /// Creates a batch write entry.
    pub fn new(key: Bytes, value: Bytes) -> Self {
        Self { key, value }
    }

    /// Returns the entry key.
    pub const fn key(&self) -> &Bytes {
        &self.key
    }

    /// Returns the entry value.
    pub const fn value(&self) -> &Bytes {
        &self.value
    }

    /// Consumes the entry.
    pub fn into_parts(self) -> (Bytes, Bytes) {
        (self.key, self.value)
    }
}

/// Entry for a batch JSON set.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BatchJsonEntry {
    key: String,
    path: String,
    value: Value,
}

impl BatchJsonEntry {
    /// Creates a batch JSON set entry.
    pub fn new(key: impl Into<String>, path: impl Into<String>, value: Value) -> Self {
        Self {
            key: key.into(),
            path: path.into(),
            value,
        }
    }

    /// Returns the document key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the JSON path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Returns the JSON value.
    pub const fn value(&self) -> &Value {
        &self.value
    }

    /// Consumes the entry.
    pub fn into_parts(self) -> (String, String, Value) {
        (self.key, self.path, self.value)
    }
}

/// Entry for a batch JSON get.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BatchJsonGetEntry {
    key: String,
    path: String,
}

impl BatchJsonGetEntry {
    /// Creates a batch JSON get entry.
    pub fn new(key: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            path: path.into(),
        }
    }

    /// Returns the document key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the JSON path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Consumes the entry.
    pub fn into_parts(self) -> (String, String) {
        (self.key, self.path)
    }
}

/// Entry for a batch JSON delete.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BatchJsonDeleteEntry {
    key: String,
    path: String,
}

impl BatchJsonDeleteEntry {
    /// Creates a batch JSON delete entry.
    pub fn new(key: impl Into<String>, path: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            path: path.into(),
        }
    }

    /// Returns the document key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the JSON path.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Consumes the entry.
    pub fn into_parts(self) -> (String, String) {
        (self.key, self.path)
    }
}

/// JSON secondary index kind exposed through the command boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JsonIndexType {
    /// Numeric field index.
    Numeric,
    /// Exact tag/string field index.
    Tag,
    /// Lowercase text field index.
    Text,
}

/// Stored JSON value with commit metadata.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonVersionedValue {
    value: Value,
    version: u64,
    timestamp: u64,
    document_version: u64,
}

impl JsonVersionedValue {
    /// Creates a JSON versioned value.
    pub fn new(value: Value, version: u64, timestamp: u64, document_version: u64) -> Self {
        Self {
            value,
            version,
            timestamp,
            document_version,
        }
    }

    /// Returns the selected JSON value.
    pub const fn value(&self) -> &Value {
        &self.value
    }

    /// Returns the commit version.
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Returns the commit timestamp.
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }

    /// Returns the document version.
    pub const fn document_version(&self) -> u64 {
        self.document_version
    }
}

/// JSON point-read result that distinguishes absence from a stored JSON null.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MaybeJsonValue {
    found: bool,
    value: Value,
}

impl MaybeJsonValue {
    /// Creates a present JSON value result.
    pub const fn found(value: Value) -> Self {
        Self { found: true, value }
    }

    /// Creates a missing JSON value result.
    pub const fn missing() -> Self {
        Self {
            found: false,
            value: Value::Null,
        }
    }

    /// Creates a result from an optional engine value.
    pub fn from_option(value: Option<Value>) -> Self {
        match value {
            Some(value) => Self::found(value),
            None => Self::missing(),
        }
    }

    /// Returns true when the selected JSON value exists.
    pub const fn found_flag(&self) -> bool {
        self.found
    }

    /// Returns true when the selected JSON value exists.
    pub const fn is_found(&self) -> bool {
        self.found
    }

    /// Returns the selected JSON value when it exists.
    pub const fn value(&self) -> Option<&Value> {
        if self.found {
            Some(&self.value)
        } else {
            None
        }
    }

    /// Consumes the result and returns the selected JSON value when it exists.
    pub fn into_option(self) -> Option<Value> {
        if self.found {
            Some(self.value)
        } else {
            None
        }
    }
}

/// JSON versioned point-read result that distinguishes absence from a stored JSON null.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MaybeJsonVersionedValue {
    found: bool,
    #[serde(default)]
    value: Option<JsonVersionedValue>,
}

impl MaybeJsonVersionedValue {
    /// Creates a present JSON versioned value result.
    pub fn found(value: JsonVersionedValue) -> Self {
        Self {
            found: true,
            value: Some(value),
        }
    }

    /// Creates a missing JSON versioned value result.
    pub const fn missing() -> Self {
        Self {
            found: false,
            value: None,
        }
    }

    /// Creates a result from an optional engine value.
    pub fn from_option(value: Option<JsonVersionedValue>) -> Self {
        match value {
            Some(value) => Self::found(value),
            None => Self::missing(),
        }
    }

    /// Returns true when the selected JSON value exists.
    pub const fn found_flag(&self) -> bool {
        self.found
    }

    /// Returns true when the selected JSON value exists.
    pub const fn is_found(&self) -> bool {
        self.found
    }

    /// Returns the selected JSON value with version metadata when it exists.
    pub const fn value(&self) -> Option<&JsonVersionedValue> {
        if self.found {
            self.value.as_ref()
        } else {
            None
        }
    }

    /// Consumes the result and returns the selected JSON value with version metadata when it exists.
    pub fn into_option(self) -> Option<JsonVersionedValue> {
        if self.found {
            self.value
        } else {
            None
        }
    }
}

/// JSON version-history item.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonHistoryItem {
    value: Option<Value>,
    version: u64,
    timestamp: u64,
    document_version: Option<u64>,
    tombstone: bool,
}

impl JsonHistoryItem {
    /// Creates a JSON history item.
    pub fn new(
        value: Option<Value>,
        version: u64,
        timestamp: u64,
        document_version: Option<u64>,
        tombstone: bool,
    ) -> Self {
        Self {
            value,
            version,
            timestamp,
            document_version,
            tombstone,
        }
    }

    /// Returns the full document value when this row is not a tombstone.
    pub const fn value(&self) -> Option<&Value> {
        self.value.as_ref()
    }

    /// Returns the commit version.
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Returns the commit timestamp.
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }

    /// Returns the document version, when present.
    pub const fn document_version(&self) -> Option<u64> {
        self.document_version
    }

    /// Returns true when this item represents a delete.
    pub const fn is_tombstone(&self) -> bool {
        self.tombstone
    }
}

/// Positional JSON batch write/delete result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct JsonBatchItemResult {
    version: Option<u64>,
    timestamp: Option<u64>,
    document_version: Option<u64>,
    error: Option<String>,
}

impl JsonBatchItemResult {
    /// Creates a successful JSON batch result.
    pub const fn new(
        version: Option<u64>,
        timestamp: Option<u64>,
        document_version: Option<u64>,
    ) -> Self {
        Self {
            version,
            timestamp,
            document_version,
            error: None,
        }
    }

    /// Creates a failed JSON batch result.
    pub fn failed(error: impl Into<String>) -> Self {
        Self {
            version: None,
            timestamp: None,
            document_version: None,
            error: Some(error.into()),
        }
    }

    /// Returns the commit version, when present.
    pub const fn version(&self) -> Option<u64> {
        self.version
    }

    /// Returns the commit timestamp, when present.
    pub const fn timestamp(&self) -> Option<u64> {
        self.timestamp
    }

    /// Returns the document version, when present.
    pub const fn document_version(&self) -> Option<u64> {
        self.document_version
    }

    /// Returns the item error, when this item failed validation.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

/// Positional JSON batch read result.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonBatchGetItemResult {
    found: bool,
    value: Value,
    version: Option<u64>,
    timestamp: Option<u64>,
    document_version: Option<u64>,
    error: Option<String>,
}

impl JsonBatchGetItemResult {
    /// Creates a JSON batch read result.
    pub fn new(
        value: Option<Value>,
        version: Option<u64>,
        timestamp: Option<u64>,
        document_version: Option<u64>,
    ) -> Self {
        match value {
            Some(value) => Self {
                found: true,
                value,
                version,
                timestamp,
                document_version,
                error: None,
            },
            None => Self {
                found: false,
                value: Value::Null,
                version,
                timestamp,
                document_version,
                error: None,
            },
        }
    }

    /// Creates a failed JSON batch read result.
    pub fn failed(error: impl Into<String>) -> Self {
        Self {
            found: false,
            value: Value::Null,
            version: None,
            timestamp: None,
            document_version: None,
            error: Some(error.into()),
        }
    }

    /// Returns true when the selected JSON value exists.
    pub const fn found(&self) -> bool {
        self.found
    }

    /// Returns the selected JSON value, when present.
    pub const fn value(&self) -> Option<&Value> {
        if self.found {
            Some(&self.value)
        } else {
            None
        }
    }

    /// Returns the commit version, when present.
    pub const fn version(&self) -> Option<u64> {
        self.version
    }

    /// Returns the commit timestamp, when present.
    pub const fn timestamp(&self) -> Option<u64> {
        self.timestamp
    }

    /// Returns the document version, when present.
    pub const fn document_version(&self) -> Option<u64> {
        self.document_version
    }

    /// Returns the item error, when this item failed validation.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

/// Sampled JSON document.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct JsonSampleItem {
    key: String,
    value: Value,
    version: u64,
    timestamp: u64,
    document_version: u64,
}

impl JsonSampleItem {
    /// Creates a sampled JSON document.
    pub fn new(
        key: String,
        value: Value,
        version: u64,
        timestamp: u64,
        document_version: u64,
    ) -> Self {
        Self {
            key,
            value,
            version,
            timestamp,
            document_version,
        }
    }

    /// Returns the document key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Returns the full document value.
    pub const fn value(&self) -> &Value {
        &self.value
    }

    /// Returns the commit version.
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Returns the commit timestamp.
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }

    /// Returns the document version.
    pub const fn document_version(&self) -> u64 {
        self.document_version
    }
}

/// JSON secondary index definition exposed through the command boundary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct JsonIndexDefinition {
    name: String,
    space: String,
    field_path: String,
    index_type: JsonIndexType,
    created_version: u64,
    created_timestamp: u64,
}

impl JsonIndexDefinition {
    /// Creates a JSON index definition.
    pub fn new(
        name: String,
        space: String,
        field_path: String,
        index_type: JsonIndexType,
        created_version: u64,
        created_timestamp: u64,
    ) -> Self {
        Self {
            name,
            space,
            field_path,
            index_type,
            created_version,
            created_timestamp,
        }
    }

    /// Returns the index name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the product space.
    pub fn space(&self) -> &str {
        &self.space
    }

    /// Returns the indexed field path.
    pub fn field_path(&self) -> &str {
        &self.field_path
    }

    /// Returns the index kind.
    pub const fn index_type(&self) -> JsonIndexType {
        self.index_type
    }

    /// Returns the creation commit version.
    pub const fn created_version(&self) -> u64 {
        self.created_version
    }

    /// Returns the creation commit timestamp.
    pub const fn created_timestamp(&self) -> u64 {
        self.created_timestamp
    }
}

/// Stored value with commit metadata.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VersionedValue {
    value: Bytes,
    version: u64,
    timestamp: u64,
}

/// Branch status exposed through the command boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchStatus {
    /// Branch accepts reads and writes.
    Active,
    /// Branch was deleted and is hidden from normal listing.
    Deleted,
}

/// Fork parent facts exposed through the command boundary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BranchParentItem {
    name: String,
    branch_id: String,
    generation: u64,
    fork_version: u64,
    fork_timestamp: Option<u64>,
}

impl BranchParentItem {
    /// Creates branch parent facts.
    pub fn new(
        name: String,
        branch_id: String,
        generation: u64,
        fork_version: u64,
        fork_timestamp: Option<u64>,
    ) -> Self {
        Self {
            name,
            branch_id,
            generation,
            fork_version,
            fork_timestamp,
        }
    }

    /// Returns the parent branch name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the parent branch id.
    pub fn branch_id(&self) -> &str {
        &self.branch_id
    }

    /// Returns the parent branch generation at fork time.
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Returns the fork version.
    pub const fn fork_version(&self) -> u64 {
        self.fork_version
    }

    /// Returns the timestamp used to resolve the fork point.
    pub const fn fork_timestamp(&self) -> Option<u64> {
        self.fork_timestamp
    }
}

/// Branch summary exposed through the command boundary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BranchItem {
    name: String,
    branch_id: String,
    generation: u64,
    status: BranchStatus,
    parent: Option<BranchParentItem>,
    created_at: Option<u64>,
    deleted_at: Option<u64>,
    state_revision: u64,
}

impl BranchItem {
    /// Creates a branch item.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        branch_id: String,
        generation: u64,
        status: BranchStatus,
        parent: Option<BranchParentItem>,
        created_at: Option<u64>,
        deleted_at: Option<u64>,
        state_revision: u64,
    ) -> Self {
        Self {
            name,
            branch_id,
            generation,
            status,
            parent,
            created_at,
            deleted_at,
            state_revision,
        }
    }

    /// Returns the branch name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the branch id.
    pub fn branch_id(&self) -> &str {
        &self.branch_id
    }

    /// Returns the branch generation.
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Returns the branch status.
    pub const fn status(&self) -> BranchStatus {
        self.status
    }

    /// Returns fork parent facts, when any.
    pub const fn parent(&self) -> Option<&BranchParentItem> {
        self.parent.as_ref()
    }

    /// Returns the storage creation version, when known.
    pub const fn created_at(&self) -> Option<u64> {
        self.created_at
    }

    /// Returns the storage deletion version, when known.
    pub const fn deleted_at(&self) -> Option<u64> {
        self.deleted_at
    }

    /// Returns the storage state revision.
    pub const fn state_revision(&self) -> u64 {
        self.state_revision
    }
}

/// Cleanup facts for branch deletion.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct BranchCleanupItem {
    removed_refs: u64,
    releasable_tables: u64,
    protected_tables: u64,
}

impl BranchCleanupItem {
    /// Creates branch cleanup facts.
    pub const fn new(removed_refs: u64, releasable_tables: u64, protected_tables: u64) -> Self {
        Self {
            removed_refs,
            releasable_tables,
            protected_tables,
        }
    }

    /// Returns the number of removed references.
    pub const fn removed_refs(self) -> u64 {
        self.removed_refs
    }

    /// Returns the number of releasable tables.
    pub const fn releasable_tables(self) -> u64 {
        self.releasable_tables
    }

    /// Returns the number of protected tables.
    pub const fn protected_tables(self) -> u64 {
        self.protected_tables
    }
}

impl VersionedValue {
    /// Creates a versioned value.
    pub fn new(value: Bytes, version: u64, timestamp: u64) -> Self {
        Self {
            value,
            version,
            timestamp,
        }
    }

    /// Returns the stored value.
    pub const fn value(&self) -> &Bytes {
        &self.value
    }

    /// Returns the commit version.
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Returns the commit timestamp.
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }
}

/// Positional batch write result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BatchItemResult {
    key: Bytes,
    applied: bool,
    version: Option<u64>,
    timestamp: Option<u64>,
    error: Option<String>,
}

impl BatchItemResult {
    /// Creates a batch item result.
    pub fn new(key: Bytes, applied: bool, version: Option<u64>, timestamp: Option<u64>) -> Self {
        Self {
            key,
            applied,
            version,
            timestamp,
            error: None,
        }
    }

    /// Creates a failed batch item result.
    pub fn failed(key: Bytes, error: impl Into<String>) -> Self {
        Self {
            key,
            applied: false,
            version: None,
            timestamp: None,
            error: Some(error.into()),
        }
    }

    /// Returns the input key.
    pub const fn key(&self) -> &Bytes {
        &self.key
    }

    /// Returns true when this item was applied.
    pub const fn applied(&self) -> bool {
        self.applied
    }

    /// Returns the commit version, when an item was applied.
    pub const fn version(&self) -> Option<u64> {
        self.version
    }

    /// Returns the commit timestamp, when an item was applied.
    pub const fn timestamp(&self) -> Option<u64> {
        self.timestamp
    }

    /// Returns the item error, when this item failed validation.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

/// Positional batch read result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BatchGetItemResult {
    key: Bytes,
    value: Option<Bytes>,
    version: Option<u64>,
    timestamp: Option<u64>,
    error: Option<String>,
}

impl BatchGetItemResult {
    /// Creates a batch read result.
    pub fn new(
        key: Bytes,
        value: Option<Bytes>,
        version: Option<u64>,
        timestamp: Option<u64>,
    ) -> Self {
        Self {
            key,
            value,
            version,
            timestamp,
            error: None,
        }
    }

    /// Creates a failed batch read result.
    pub fn failed(key: Bytes, error: impl Into<String>) -> Self {
        Self {
            key,
            value: None,
            version: None,
            timestamp: None,
            error: Some(error.into()),
        }
    }

    /// Returns the input key.
    pub const fn key(&self) -> &Bytes {
        &self.key
    }

    /// Returns the stored value, when present.
    pub const fn value(&self) -> Option<&Bytes> {
        self.value.as_ref()
    }

    /// Returns the commit version, when present.
    pub const fn version(&self) -> Option<u64> {
        self.version
    }

    /// Returns the commit timestamp, when present.
    pub const fn timestamp(&self) -> Option<u64> {
        self.timestamp
    }

    /// Returns the item error, when this item failed validation.
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }
}

/// KV scan item.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ScanItem {
    key: Bytes,
    value: Bytes,
    version: u64,
    timestamp: u64,
}

impl ScanItem {
    /// Creates a scan item.
    pub fn new(key: Bytes, value: Bytes, version: u64, timestamp: u64) -> Self {
        Self {
            key,
            value,
            version,
            timestamp,
        }
    }

    /// Returns the item key.
    pub const fn key(&self) -> &Bytes {
        &self.key
    }

    /// Returns the item value.
    pub const fn value(&self) -> &Bytes {
        &self.value
    }

    /// Returns the commit version.
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Returns the commit timestamp.
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }
}

/// Version-history item.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct HistoryItem {
    value: Option<Bytes>,
    tombstone: bool,
    version: u64,
    timestamp: u64,
}

impl HistoryItem {
    /// Creates a history item.
    pub fn new(value: Option<Bytes>, tombstone: bool, version: u64, timestamp: u64) -> Self {
        Self {
            value,
            tombstone,
            version,
            timestamp,
        }
    }

    /// Returns the item value, when this is not a tombstone.
    pub const fn value(&self) -> Option<&Bytes> {
        self.value.as_ref()
    }

    /// Returns true when this item represents a delete.
    pub const fn is_tombstone(&self) -> bool {
        self.tombstone
    }

    /// Returns the commit version.
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Returns the commit timestamp.
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }
}

/// Sampled KV item.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SampleItem {
    key: Bytes,
    value: Bytes,
    version: u64,
    timestamp: u64,
}

impl SampleItem {
    /// Creates a sample item.
    pub fn new(key: Bytes, value: Bytes, version: u64, timestamp: u64) -> Self {
        Self {
            key,
            value,
            version,
            timestamp,
        }
    }

    /// Returns the item key.
    pub const fn key(&self) -> &Bytes {
        &self.key
    }

    /// Returns the item value.
    pub const fn value(&self) -> &Bytes {
        &self.value
    }

    /// Returns the commit version.
    pub const fn version(&self) -> u64 {
        self.version
    }

    /// Returns the commit timestamp.
    pub const fn timestamp(&self) -> u64 {
        self.timestamp
    }
}
