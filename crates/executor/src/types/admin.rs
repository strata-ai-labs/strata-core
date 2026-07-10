use super::{Deserialize, Serialize, VectorDistanceMetric};

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
