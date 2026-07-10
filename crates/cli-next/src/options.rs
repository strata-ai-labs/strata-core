//! Command-line option model.

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use strata_executor_next::JsonIndexType;

/// Strata V1 command-line interface.
#[derive(Debug, Parser)]
#[command(name = "strata", version, about = "Strata database CLI")]
pub(crate) struct Cli {
    /// Durable database path.
    #[arg(value_name = "DB")]
    pub(crate) db_path: Option<PathBuf>,
    /// Durable database path. Cannot be combined with the positional DB path.
    #[arg(long, value_name = "PATH")]
    pub(crate) db: Option<PathBuf>,
    /// Use an in-memory cache database for this process.
    #[arg(long)]
    pub(crate) cache: bool,
    /// Default branch for commands that accept a branch.
    #[arg(long, global = true)]
    pub(crate) branch: Option<String>,
    /// Product space for commands that accept a space.
    #[arg(long, global = true)]
    pub(crate) space: Option<String>,
    /// Emit compact JSON.
    #[arg(long, global = true, conflicts_with_all = ["raw", "format"])]
    pub(crate) json: bool,
    /// Emit script-friendly raw output where possible.
    #[arg(long, global = true, conflicts_with_all = ["json", "format"])]
    pub(crate) raw: bool,
    /// Transitional output format flag.
    #[arg(long = "output-format", value_enum, global = true, hide = true)]
    pub(crate) format: Option<Format>,
    /// Command to run.
    #[command(subcommand)]
    pub(crate) command: Option<TopCommand>,
}

/// Output format.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum Format {
    /// Concise human output.
    Human,
    /// Compact JSON.
    Json,
    /// Pretty-printed JSON.
    Pretty,
    /// Script-friendly raw output.
    Raw,
}

impl Cli {
    pub(crate) fn output_format(&self) -> Format {
        if self.raw {
            Format::Raw
        } else if self.json {
            Format::Json
        } else {
            self.format.unwrap_or(Format::Human)
        }
    }
}

/// Top-level command families.
#[derive(Debug, Subcommand)]
pub(crate) enum TopCommand {
    /// Prepare the Strata home directory and print next steps.
    Init,
    /// Check the installation and, when a database is targeted, its health.
    Doctor,
    /// Self-describing surface for agents: guide, catalogs, repo onboarding.
    Agents(AgentsArgs),
    /// Model Context Protocol server commands.
    Mcp(McpArgs),
    /// Lightweight liveness check.
    Ping,
    /// Print database information.
    Info,
    /// Print health facts.
    Health,
    /// Print metrics facts.
    Metrics,
    /// Print a compact database description.
    Describe,
    /// Configuration reads.
    Config(ConfigArgs),
    /// Branch lifecycle commands.
    Branch(BranchArgs),
    /// Product space commands.
    Space(SpaceArgs),
    /// KV commands.
    Kv(KvArgs),
    /// JSON document commands.
    Json(JsonArgs),
    /// Vector commands.
    Vector(VectorArgs),
    /// Event log commands.
    Event(EventArgs),
    /// Graph core commands.
    Graph(GraphArgs),
    /// Arrow import/export commands.
    Arrow(ArrowArgs),
    /// Model execution: local GGUF models and cloud providers.
    #[cfg(feature = "inference")]
    Inference(InferenceArgs),
    /// Raw serialized executor command.
    Command(CommandArgs),
    /// Deferred old search surface.
    #[command(hide = true)]
    Search(DeferredArgs),
    /// Deferred old recipe surface.
    #[command(hide = true)]
    Recipe(DeferredArgs),
    /// Deferred transaction helper.
    #[command(hide = true)]
    Txn(DeferredArgs),
    /// Deferred transaction begin.
    #[command(hide = true)]
    Begin,
    /// Deferred transaction commit.
    #[command(hide = true)]
    Commit,
    /// Deferred transaction rollback.
    #[command(hide = true)]
    Rollback,
    /// Intentionally unavailable maintenance command.
    #[command(hide = true)]
    Flush,
    /// Intentionally unavailable maintenance command.
    #[command(hide = true)]
    Compact,
    /// Deferred daemon lifecycle command.
    #[command(hide = true)]
    Up(DeferredArgs),
    /// Deferred daemon lifecycle command.
    #[command(hide = true)]
    Down(DeferredArgs),
    /// Deferred daemon lifecycle command.
    #[command(hide = true)]
    Uninstall(DeferredArgs),
}

/// Arguments accepted for known deferred commands.
#[derive(Debug, Args)]
pub(crate) struct DeferredArgs {
    /// Deferred command arguments.
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub(crate) args: Vec<String>,
}

/// Config command wrapper.
#[derive(Debug, Args)]
pub(crate) struct ConfigArgs {
    /// Config command.
    #[command(subcommand)]
    pub(crate) command: ConfigCommand,
}

/// MCP command arguments.
#[derive(Debug, Args)]
pub(crate) struct McpArgs {
    /// MCP command.
    #[command(subcommand)]
    pub(crate) command: McpCommand,
}

/// MCP server commands.
#[derive(Debug, Subcommand)]
pub(crate) enum McpCommand {
    /// Serve MCP over stdio (newline-delimited JSON-RPC; logs on stderr).
    Serve,
}

/// Agent-surface command arguments.
#[derive(Debug, Args)]
pub(crate) struct AgentsArgs {
    /// Agents command.
    #[command(subcommand)]
    pub(crate) command: AgentsCommand,
}

/// Agent-facing self-description commands.
#[derive(Debug, Subcommand)]
pub(crate) enum AgentsCommand {
    /// Print the complete offline usage guide (markdown, version-matched).
    Guide,
    /// Print the machine-readable command catalog.
    Commands,
    /// Print the public error-code registry.
    Errors,
    /// Write repo onboarding files (.strata/AGENTS.md; --apply appends the
    /// pointer block to the repo's AGENTS.md or CLAUDE.md).
    Init {
        /// Append the pointer block to an existing AGENTS.md/CLAUDE.md.
        #[arg(long)]
        apply: bool,
    },
}

/// Config commands.
#[derive(Debug, Subcommand)]
pub(crate) enum ConfigCommand {
    /// Print sanitized config.
    Get,
    /// Print one sanitized config value.
    GetKey {
        /// Config key.
        key: String,
    },
}

/// Branch command wrapper.
#[derive(Debug, Args)]
pub(crate) struct BranchArgs {
    /// Branch command.
    #[command(subcommand)]
    pub(crate) command: BranchCommand,
}

/// Branch commands.
#[derive(Debug, Subcommand)]
pub(crate) enum BranchCommand {
    /// List branches.
    List,
    /// Read one branch.
    Get {
        /// Branch name.
        branch: String,
    },
    /// Create an empty root branch.
    Create {
        /// Branch name.
        branch: String,
    },
    /// Fork a branch.
    Fork {
        /// Source branch.
        source: String,
        /// New branch name.
        branch: String,
        /// Fork from a retained source version.
        #[arg(long, conflicts_with = "timestamp")]
        version: Option<u64>,
        /// Fork from a retained source timestamp.
        #[arg(long, conflicts_with = "version")]
        timestamp: Option<u64>,
    },
    /// Delete a branch.
    #[command(alias = "del")]
    Delete {
        /// Branch name.
        branch: String,
    },
    /// Deferred branch diff command.
    #[command(hide = true)]
    Diff(DeferredArgs),
    /// Deferred branch merge command.
    #[command(hide = true)]
    Merge(DeferredArgs),
    /// Deferred branch tag command.
    #[command(hide = true)]
    Tag(DeferredArgs),
    /// Deferred branch note command.
    #[command(hide = true)]
    Note(DeferredArgs),
}

/// Product space command wrapper.
#[derive(Debug, Args)]
pub(crate) struct SpaceArgs {
    /// Space command.
    #[command(subcommand)]
    pub(crate) command: SpaceCommand,
}

/// Product space commands.
#[derive(Debug, Subcommand)]
pub(crate) enum SpaceCommand {
    /// List spaces.
    List,
    /// Create a space.
    Create {
        /// Space name.
        space: String,
    },
    /// Check whether a space exists.
    Exists {
        /// Space name.
        space: String,
    },
    /// Delete a space.
    #[command(alias = "del")]
    Delete {
        /// Space name.
        space: String,
        /// Delete visible data in the space before dropping the catalog entry.
        #[arg(long)]
        force: bool,
    },
}

/// KV command wrapper.
#[derive(Debug, Args)]
pub(crate) struct KvArgs {
    /// KV command.
    #[command(subcommand)]
    pub(crate) command: KvCommand,
}

/// KV commands.
#[derive(Debug, Subcommand)]
pub(crate) enum KvCommand {
    /// Write one key.
    Put {
        /// Key.
        key: String,
        /// Value. Use @path to read bytes from a file.
        value: Option<String>,
        /// Read value bytes from a file.
        #[arg(short = 'f', long, conflicts_with = "value")]
        file: Option<PathBuf>,
    },
    /// Read one key.
    Get {
        /// Key.
        key: String,
        /// Optional read timestamp in microseconds.
        #[arg(long)]
        as_of: Option<u64>,
    },
    /// Delete one key.
    #[command(alias = "del")]
    Delete {
        /// Key.
        key: String,
    },
    /// List keys.
    List {
        /// Optional key prefix.
        #[arg(long)]
        prefix: Option<String>,
        /// Optional base64 continuation cursor as printed by the previous page.
        #[arg(long)]
        cursor: Option<String>,
        /// Optional item limit.
        #[arg(long)]
        limit: Option<u64>,
        /// Optional read timestamp in microseconds.
        #[arg(long)]
        as_of: Option<u64>,
    },
    /// Scan rows.
    Scan {
        /// Optional inclusive start key.
        #[arg(long, conflicts_with = "cursor")]
        start: Option<String>,
        /// Optional base64 continuation cursor as printed by the previous page.
        #[arg(long)]
        cursor: Option<String>,
        /// Optional item limit.
        #[arg(long)]
        limit: Option<u64>,
    },
    /// Check key existence.
    Exists {
        /// Key.
        key: String,
    },
    /// Read version history.
    History {
        /// Key.
        key: String,
    },
    /// Count keys.
    Count {
        /// Optional key prefix.
        #[arg(long)]
        prefix: Option<String>,
    },
    /// Sample rows.
    Sample {
        /// Optional key prefix.
        #[arg(long)]
        prefix: Option<String>,
        /// Optional sample count.
        #[arg(long)]
        count: Option<u64>,
    },
}

/// JSON command wrapper.
#[derive(Debug, Args)]
pub(crate) struct JsonArgs {
    /// JSON command.
    #[command(subcommand)]
    pub(crate) command: JsonCommand,
}

/// JSON commands.
#[derive(Debug, Subcommand)]
pub(crate) enum JsonCommand {
    /// Set a JSON path.
    Set {
        /// Document key.
        key: String,
        /// JSON path.
        path: String,
        /// JSON value. Non-JSON text is stored as a string. Use @path to read from a file.
        value: Option<String>,
        /// Read JSON value from a file.
        #[arg(short = 'f', long, conflicts_with = "value")]
        file: Option<PathBuf>,
    },
    /// Read a JSON path.
    Get {
        /// Document key.
        key: String,
        /// JSON path.
        path: String,
        /// Optional read timestamp in microseconds.
        #[arg(long)]
        as_of: Option<u64>,
    },
    /// Delete a JSON path.
    #[command(alias = "del")]
    Delete {
        /// Document key.
        key: String,
        /// JSON path.
        path: String,
    },
    /// List JSON document keys.
    List {
        /// Optional document prefix.
        #[arg(long)]
        prefix: Option<String>,
        /// Optional continuation cursor.
        #[arg(long)]
        cursor: Option<String>,
        /// Optional item limit.
        #[arg(long)]
        limit: Option<u64>,
        /// Optional read timestamp in microseconds.
        #[arg(long)]
        as_of: Option<u64>,
    },
    /// Check whether a document exists.
    Exists {
        /// Document key.
        key: String,
    },
    /// Read document history.
    History {
        /// Document key.
        key: String,
    },
    /// Count documents.
    Count {
        /// Optional document prefix.
        #[arg(long)]
        prefix: Option<String>,
    },
    /// Sample documents.
    Sample {
        /// Optional document prefix.
        #[arg(long)]
        prefix: Option<String>,
        /// Optional sample count.
        #[arg(long)]
        count: Option<u64>,
    },
    /// JSON secondary index commands.
    Index {
        /// Index command.
        #[command(subcommand)]
        command: JsonIndexCommand,
    },
}

/// JSON index commands.
#[derive(Debug, Subcommand)]
pub(crate) enum JsonIndexCommand {
    /// Create an index.
    Create {
        /// Index name.
        name: String,
        /// Indexed field path.
        field_path: String,
        /// Index type.
        #[arg(long, value_enum, default_value_t = CliJsonIndexType::Tag)]
        index_type: CliJsonIndexType,
    },
    /// Drop an index.
    Drop {
        /// Index name.
        name: String,
    },
    /// List indexes.
    List,
}

/// JSON index kind.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum CliJsonIndexType {
    /// Numeric index.
    Numeric,
    /// Tag/string index.
    Tag,
    /// Lowercase text index.
    Text,
}

impl From<CliJsonIndexType> for JsonIndexType {
    fn from(value: CliJsonIndexType) -> Self {
        match value {
            CliJsonIndexType::Numeric => Self::Numeric,
            CliJsonIndexType::Tag => Self::Tag,
            CliJsonIndexType::Text => Self::Text,
        }
    }
}

/// Vector command wrapper.
#[derive(Debug, Args)]
pub(crate) struct VectorArgs {
    /// Vector command.
    #[command(subcommand)]
    pub(crate) command: VectorCommand,
}

/// Vector commands.
#[derive(Debug, Subcommand)]
pub(crate) enum VectorCommand {
    /// Vector collection commands.
    Collection {
        /// Collection command.
        #[command(subcommand)]
        command: VectorCollectionCommand,
    },
    /// Upsert one vector.
    Upsert {
        /// Collection name.
        collection: String,
        /// Vector key.
        key: String,
        /// Vector as JSON array, comma-separated floats, or @path.
        vector: Option<String>,
        /// Read vector from a file.
        #[arg(short = 'f', long, conflicts_with = "vector")]
        file: Option<PathBuf>,
        /// Optional metadata JSON object.
        #[arg(long)]
        metadata: Option<String>,
        /// Read metadata JSON object from a file.
        #[arg(long, conflicts_with = "metadata")]
        metadata_file: Option<PathBuf>,
    },
    /// Read one vector.
    Get {
        /// Collection name.
        collection: String,
        /// Vector key.
        key: String,
        /// Optional read timestamp in microseconds.
        #[arg(long)]
        as_of: Option<u64>,
    },
    /// Read vector history.
    History {
        /// Collection name.
        collection: String,
        /// Vector key.
        key: String,
    },
    /// Check vector existence.
    Exists {
        /// Collection name.
        collection: String,
        /// Vector key.
        key: String,
    },
    /// List vector keys.
    Keys {
        /// Collection name.
        collection: String,
        /// Optional key prefix.
        #[arg(long)]
        prefix: Option<String>,
        /// Optional continuation cursor.
        #[arg(long)]
        cursor: Option<String>,
        /// Optional item limit.
        #[arg(long)]
        limit: Option<u64>,
    },
    /// Patch vector metadata.
    UpdateMetadata {
        /// Collection name.
        collection: String,
        /// Vector key.
        key: String,
        /// Top-level metadata patch JSON or @path.
        patch: Option<String>,
        /// Read metadata patch from a file.
        #[arg(short = 'f', long, conflicts_with = "patch")]
        file: Option<PathBuf>,
    },
    /// Delete one vector.
    #[command(alias = "del")]
    Delete {
        /// Collection name.
        collection: String,
        /// Vector key.
        key: String,
    },
    /// Delete all vectors in a collection.
    DeleteAll {
        /// Collection name.
        collection: String,
    },
    /// Delete vectors matching a metadata filter JSON object.
    DeleteByFilter {
        /// Collection name.
        collection: String,
        /// Serialized `VectorMetadataFilter` JSON.
        #[arg(long, conflicts_with = "filter_file")]
        filter: Option<String>,
        /// Read filter JSON from a file.
        #[arg(long)]
        filter_file: Option<PathBuf>,
    },
    /// Search vectors.
    Query {
        /// Collection name.
        collection: String,
        /// Query vector as JSON array, comma-separated floats, or @path.
        query: Option<String>,
        /// Read query vector from a file.
        #[arg(short = 'f', long, conflicts_with = "query")]
        file: Option<PathBuf>,
        /// Maximum number of matches.
        #[arg(short = 'k', long, default_value_t = 10)]
        k: u64,
        /// Serialized `VectorMetadataFilter` JSON.
        #[arg(long, conflicts_with = "filter_file")]
        filter: Option<String>,
        /// Read filter JSON from a file.
        #[arg(long)]
        filter_file: Option<PathBuf>,
        /// Optional read timestamp in microseconds.
        #[arg(long)]
        as_of: Option<u64>,
        /// Include vector index diagnostics.
        #[arg(long)]
        diagnostics: bool,
    },
    /// Count vectors.
    Count {
        /// Collection name.
        collection: String,
    },
}

/// Vector collection commands.
#[derive(Debug, Subcommand)]
pub(crate) enum VectorCollectionCommand {
    /// Create a collection.
    Create {
        /// Collection name.
        collection: String,
        /// Embedding dimension.
        dimension: u64,
        /// Distance metric.
        #[arg(long, value_enum, default_value_t = CliVectorMetric::Cosine)]
        metric: CliVectorMetric,
    },
    /// Delete a collection.
    #[command(alias = "del")]
    Delete {
        /// Collection name.
        collection: String,
    },
    /// List collections.
    List,
    /// Read collection stats.
    Stats {
        /// Collection name.
        collection: String,
    },
}

/// Vector metric.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum CliVectorMetric {
    /// Cosine similarity.
    Cosine,
    /// Euclidean similarity.
    Euclidean,
    /// Dot product.
    DotProduct,
}

impl From<CliVectorMetric> for strata_executor_next::VectorDistanceMetric {
    fn from(value: CliVectorMetric) -> Self {
        match value {
            CliVectorMetric::Cosine => Self::Cosine,
            CliVectorMetric::Euclidean => Self::Euclidean,
            CliVectorMetric::DotProduct => Self::DotProduct,
        }
    }
}

/// Event command wrapper.
#[derive(Debug, Args)]
pub(crate) struct EventArgs {
    /// Event command.
    #[command(subcommand)]
    pub(crate) command: EventCommand,
}

/// Event commands.
#[derive(Debug, Subcommand)]
pub(crate) enum EventCommand {
    /// Append one event.
    Append {
        /// Event type.
        event_type: String,
        /// Event payload JSON or @path.
        payload: Option<String>,
        /// Read event payload from a file.
        #[arg(short = 'f', long, conflicts_with = "payload")]
        file: Option<PathBuf>,
    },
    /// Read one event.
    Get {
        /// Event sequence.
        sequence: u64,
        /// Optional read timestamp in microseconds.
        #[arg(long)]
        as_of: Option<u64>,
    },
    /// Check event existence.
    Exists {
        /// Event sequence.
        sequence: u64,
    },
    /// Count visible events.
    Len {
        /// Optional read timestamp in microseconds.
        #[arg(long)]
        as_of: Option<u64>,
    },
    /// List events.
    List {
        /// Optional event type filter.
        #[arg(long)]
        event_type: Option<String>,
        /// Optional item limit.
        #[arg(long)]
        limit: Option<u64>,
        /// Optional exclusive sequence cursor.
        #[arg(long, alias = "cursor")]
        after_sequence: Option<u64>,
        /// Optional read timestamp in microseconds.
        #[arg(long)]
        as_of: Option<u64>,
    },
    /// List event types.
    Types {
        /// Optional read timestamp in microseconds.
        #[arg(long)]
        as_of: Option<u64>,
    },
    /// List events by type.
    ByType {
        /// Event type.
        event_type: String,
        /// Optional item limit.
        #[arg(long)]
        limit: Option<u64>,
        /// Optional exclusive sequence cursor.
        #[arg(long)]
        after_sequence: Option<u64>,
        /// Optional read timestamp in microseconds.
        #[arg(long)]
        as_of: Option<u64>,
    },
    /// Read an event sequence range.
    Range {
        /// Inclusive start sequence; with reverse direction, walk backward from this sequence.
        start_seq: u64,
        /// Optional exclusive end sequence; with reverse direction, exclusive lower bound.
        #[arg(long)]
        end_seq: Option<u64>,
        /// Optional item limit.
        #[arg(long)]
        limit: Option<u64>,
        /// Range direction.
        #[arg(long, value_enum, default_value_t = CliEventDirection::Forward)]
        direction: CliEventDirection,
        /// Optional event type filter.
        #[arg(long)]
        event_type: Option<String>,
    },
    /// Read events by timestamp range.
    RangeTime {
        /// Inclusive start timestamp.
        start_ts: u64,
        /// Optional inclusive end timestamp.
        #[arg(long)]
        end_ts: Option<u64>,
        /// Optional item limit.
        #[arg(long)]
        limit: Option<u64>,
        /// Range direction.
        #[arg(long, value_enum, default_value_t = CliEventDirection::Forward)]
        direction: CliEventDirection,
        /// Optional event type filter.
        #[arg(long)]
        event_type: Option<String>,
    },
    /// Verify sequence density and hash linkage.
    VerifyChain,
}

/// Event range direction.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum CliEventDirection {
    /// Forward order.
    Forward,
    /// Reverse order.
    Reverse,
}

impl From<CliEventDirection> for strata_executor_next::EventRangeDirection {
    fn from(value: CliEventDirection) -> Self {
        match value {
            CliEventDirection::Forward => Self::Forward,
            CliEventDirection::Reverse => Self::Reverse,
        }
    }
}

/// Graph command wrapper.
#[derive(Debug, Args)]
pub(crate) struct GraphArgs {
    /// Graph command.
    #[command(subcommand)]
    pub(crate) command: GraphCommand,
}

/// Graph commands.
#[derive(Debug, Subcommand)]
pub(crate) enum GraphCommand {
    /// Create a graph.
    Create {
        /// Graph name.
        graph: String,
    },
    /// Delete a graph.
    #[command(alias = "del")]
    Delete {
        /// Graph name.
        graph: String,
    },
    /// List graphs.
    List {
        /// Optional graph cursor.
        #[arg(long)]
        cursor: Option<String>,
        /// Optional item limit.
        #[arg(long)]
        limit: Option<u64>,
        /// Optional read timestamp in microseconds.
        #[arg(long)]
        as_of: Option<u64>,
    },
    /// Read graph metadata.
    Meta {
        /// Graph name.
        graph: String,
        /// Optional read timestamp in microseconds.
        #[arg(long)]
        as_of: Option<u64>,
    },
    /// Add or replace a node.
    AddNode {
        /// Graph name.
        graph: String,
        /// Node id.
        node_id: String,
        /// Optional properties JSON.
        #[arg(long, conflicts_with = "properties_file")]
        properties: Option<String>,
        /// Read properties JSON from a file.
        #[arg(long)]
        properties_file: Option<PathBuf>,
        /// Optional declared object type (validated once the ontology is frozen).
        #[arg(long = "type")]
        object_type: Option<String>,
    },
    /// Read a node.
    GetNode {
        /// Graph name.
        graph: String,
        /// Node id.
        node_id: String,
        /// Optional read timestamp in microseconds.
        #[arg(long)]
        as_of: Option<u64>,
    },
    /// Remove a node.
    RemoveNode {
        /// Graph name.
        graph: String,
        /// Node id.
        node_id: String,
    },
    /// List nodes.
    ListNodes {
        /// Graph name.
        graph: String,
        /// Optional node prefix.
        #[arg(long)]
        prefix: Option<String>,
        /// Optional node cursor.
        #[arg(long)]
        cursor: Option<String>,
        /// Optional item limit.
        #[arg(long)]
        limit: Option<u64>,
        /// Optional read timestamp in microseconds.
        #[arg(long)]
        as_of: Option<u64>,
    },
    /// Add or replace an edge.
    AddEdge {
        /// Graph name.
        graph: String,
        /// Source node id.
        src: String,
        /// Edge type.
        edge_type: String,
        /// Destination node id.
        dst: String,
        /// Optional edge weight.
        #[arg(long)]
        weight: Option<f64>,
        /// Optional properties JSON.
        #[arg(long, conflicts_with = "properties_file")]
        properties: Option<String>,
        /// Read properties JSON from a file.
        #[arg(long)]
        properties_file: Option<PathBuf>,
    },
    /// Read an edge.
    GetEdge {
        /// Graph name.
        graph: String,
        /// Source node id.
        src: String,
        /// Edge type.
        edge_type: String,
        /// Destination node id.
        dst: String,
        /// Optional read timestamp in microseconds.
        #[arg(long)]
        as_of: Option<u64>,
    },
    /// Remove an edge.
    RemoveEdge {
        /// Graph name.
        graph: String,
        /// Source node id.
        src: String,
        /// Edge type.
        edge_type: String,
        /// Destination node id.
        dst: String,
    },
    /// List neighbors.
    Neighbors {
        /// Graph name.
        graph: String,
        /// Node id.
        node_id: String,
        /// Traversal direction.
        #[arg(long, value_enum, default_value_t = CliGraphDirection::Outgoing)]
        direction: CliGraphDirection,
        /// Optional edge type filter.
        #[arg(long)]
        edge_type: Option<String>,
        /// Optional continuation cursor.
        #[arg(long)]
        cursor: Option<String>,
        /// Optional item limit.
        #[arg(long)]
        limit: Option<u64>,
        /// Optional read timestamp in microseconds.
        #[arg(long)]
        as_of: Option<u64>,
    },
    /// List nodes declaring an object type.
    NodesByType {
        /// Graph name.
        graph: String,
        /// Object type name.
        object_type: String,
        /// Optional node cursor.
        #[arg(long)]
        cursor: Option<String>,
        /// Optional item limit.
        #[arg(long)]
        limit: Option<u64>,
        /// Optional read timestamp in microseconds.
        #[arg(long)]
        as_of: Option<u64>,
    },
    /// Graph ontology commands.
    Ontology(GraphOntologyArgs),
    /// Compute weakly connected components.
    Wcc {
        /// Graph name.
        graph: String,
        /// Optional read timestamp in microseconds.
        #[arg(long)]
        as_of: Option<u64>,
    },
    /// Compute local clustering coefficients.
    Lcc {
        /// Graph name.
        graph: String,
        /// Optional read timestamp in microseconds.
        #[arg(long)]
        as_of: Option<u64>,
    },
    /// Compute shortest-path distances from a source node.
    Sssp {
        /// Graph name.
        graph: String,
        /// Source node id.
        source: String,
        /// Traversal direction.
        #[arg(long, value_enum, default_value_t = CliGraphDirection::Outgoing)]
        direction: CliGraphDirection,
        /// Optional read timestamp in microseconds.
        #[arg(long)]
        as_of: Option<u64>,
    },
    /// Compute pagerank importance scores, optionally personalized.
    Pagerank {
        /// Graph name.
        graph: String,
        /// Optional damping factor (default 0.85).
        #[arg(long)]
        damping: Option<f64>,
        /// Optional iteration bound (default 20).
        #[arg(long)]
        max_iterations: Option<u64>,
        /// Optional convergence tolerance (default 1e-6).
        #[arg(long)]
        tolerance: Option<f64>,
        /// Optional seed weights as JSON, e.g. '{"node": 1.0}'.
        #[arg(long)]
        personalization: Option<String>,
        /// Optional read timestamp in microseconds.
        #[arg(long)]
        as_of: Option<u64>,
    },
    /// Detect communities via label propagation.
    Cdlp {
        /// Graph name.
        graph: String,
        /// Optional iteration bound (default 10).
        #[arg(long)]
        max_iterations: Option<u64>,
        /// Propagation direction.
        #[arg(long, value_enum, default_value_t = CliGraphDirection::Both)]
        direction: CliGraphDirection,
        /// Optional read timestamp in microseconds.
        #[arg(long)]
        as_of: Option<u64>,
    },
    /// Run a bounded breadth-first traversal.
    Bfs {
        /// Graph name.
        graph: String,
        /// Start node id.
        start: String,
        /// Optional depth bound (default 100).
        #[arg(long)]
        max_depth: Option<u64>,
        /// Optional visited-node bound (default 10000).
        #[arg(long)]
        max_nodes: Option<u64>,
        /// Optional edge-type restriction (repeatable).
        #[arg(long = "edge-type")]
        edge_types: Vec<String>,
        /// Traversal direction.
        #[arg(long, value_enum, default_value_t = CliGraphDirection::Outgoing)]
        direction: CliGraphDirection,
        /// Optional read timestamp in microseconds.
        #[arg(long)]
        as_of: Option<u64>,
    },
}

/// Graph ontology command wrapper.
#[derive(Debug, Args)]
pub(crate) struct GraphOntologyArgs {
    /// Ontology command.
    #[command(subcommand)]
    pub(crate) command: GraphOntologyCommand,
}

/// Graph ontology commands.
#[derive(Debug, Subcommand)]
pub(crate) enum GraphOntologyCommand {
    /// Define (or, while draft, redefine) an object type.
    DefineObjectType {
        /// Graph name.
        graph: String,
        /// Object type name.
        name: String,
        /// Properties JSON, e.g. `{"prop": {"value_type": "string", "required": true}}`.
        #[arg(long, conflicts_with = "properties_file")]
        properties: Option<String>,
        /// Read properties JSON from a file.
        #[arg(long)]
        properties_file: Option<PathBuf>,
    },
    /// Define (or, while draft, redefine) a link type.
    DefineLinkType {
        /// Graph name.
        graph: String,
        /// Link type name.
        name: String,
        /// Declared source object type.
        source: String,
        /// Declared target object type.
        target: String,
        /// Optional cardinality hint (e.g. one-to-many).
        #[arg(long)]
        cardinality: Option<String>,
        /// Properties JSON, e.g. `{"prop": {"value_type": "string", "required": true}}`.
        #[arg(long, conflicts_with = "properties_file")]
        properties: Option<String>,
        /// Read properties JSON from a file.
        #[arg(long)]
        properties_file: Option<PathBuf>,
    },
    /// Delete a draft object type.
    DeleteObjectType {
        /// Graph name.
        graph: String,
        /// Object type name.
        name: String,
    },
    /// Delete a draft link type.
    DeleteLinkType {
        /// Graph name.
        graph: String,
        /// Link type name.
        name: String,
    },
    /// Freeze the ontology; writes then validate against it.
    Freeze {
        /// Graph name.
        graph: String,
    },
    /// Read the ontology (status plus every declared type).
    Get {
        /// Graph name.
        graph: String,
        /// Optional read timestamp in microseconds.
        #[arg(long)]
        as_of: Option<u64>,
    },
    /// Read the ontology with per-type usage counts.
    Summary {
        /// Graph name.
        graph: String,
        /// Optional read timestamp in microseconds.
        #[arg(long)]
        as_of: Option<u64>,
    },
}

/// Graph direction.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum CliGraphDirection {
    /// Outgoing edges.
    Outgoing,
    /// Incoming edges.
    Incoming,
    /// Incoming and outgoing edges.
    Both,
}

impl From<CliGraphDirection> for strata_executor_next::GraphDirection {
    fn from(value: CliGraphDirection) -> Self {
        match value {
            CliGraphDirection::Outgoing => Self::Outgoing,
            CliGraphDirection::Incoming => Self::Incoming,
            CliGraphDirection::Both => Self::Both,
        }
    }
}

/// Arrow command wrapper.
#[derive(Debug, Args)]
pub(crate) struct ArrowArgs {
    /// Arrow command.
    #[command(subcommand)]
    pub(crate) command: ArrowCommand,
}

/// Arrow commands.
#[derive(Debug, Subcommand)]
pub(crate) enum ArrowCommand {
    /// Import an Arrow-compatible file.
    Import {
        /// Input file path.
        file_path: String,
        /// Optional input format.
        #[arg(long, value_enum, id = "arrow_import_format")]
        format: Option<CliArrowFormat>,
        /// Import target.
        #[arg(long, value_enum)]
        target: CliArrowImportTarget,
        /// Optional key column override.
        #[arg(long)]
        key_column: Option<String>,
        /// Optional value/document/embedding column override.
        #[arg(long)]
        value_column: Option<String>,
        /// Target vector collection for vector imports.
        #[arg(long)]
        collection: Option<String>,
    },
    /// Export a primitive to an Arrow-compatible file.
    Export {
        /// Export primitive.
        #[arg(long, value_enum)]
        primitive: CliArrowExportPrimitive,
        /// Output format.
        #[arg(long, value_enum, id = "arrow_export_format")]
        format: CliArrowFormat,
        /// Output file path. Graph exports use this as a stem for node and edge files.
        path: String,
        /// Optional key/document/vector/node prefix.
        #[arg(long)]
        prefix: Option<String>,
        /// Optional row limit.
        #[arg(long)]
        limit: Option<u64>,
        /// Target vector collection for vector exports.
        #[arg(long)]
        collection: Option<String>,
        /// Target graph for graph exports.
        #[arg(long)]
        graph: Option<String>,
        /// Optional event type filter for event exports.
        #[arg(long)]
        event_type: Option<String>,
    },
}

/// Arrow file format.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum CliArrowFormat {
    /// Parquet.
    Parquet,
    /// CSV.
    Csv,
    /// JSON lines.
    Jsonl,
}

impl From<CliArrowFormat> for strata_executor_next::ArrowFileFormat {
    fn from(value: CliArrowFormat) -> Self {
        match value {
            CliArrowFormat::Parquet => Self::Parquet,
            CliArrowFormat::Csv => Self::Csv,
            CliArrowFormat::Jsonl => Self::Jsonl,
        }
    }
}

/// Arrow import target.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum CliArrowImportTarget {
    /// KV primitive.
    Kv,
    /// JSON primitive.
    Json,
    /// Vector primitive.
    Vector,
}

impl From<CliArrowImportTarget> for strata_executor_next::ArrowImportTarget {
    fn from(value: CliArrowImportTarget) -> Self {
        match value {
            CliArrowImportTarget::Kv => Self::Kv,
            CliArrowImportTarget::Json => Self::Json,
            CliArrowImportTarget::Vector => Self::Vector,
        }
    }
}

/// Arrow export primitive.
#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum CliArrowExportPrimitive {
    /// KV primitive.
    Kv,
    /// JSON primitive.
    Json,
    /// Event primitive.
    Event,
    /// Vector primitive.
    Vector,
    /// Graph primitive.
    Graph,
}

impl From<CliArrowExportPrimitive> for strata_executor_next::ArrowExportPrimitive {
    fn from(value: CliArrowExportPrimitive) -> Self {
        match value {
            CliArrowExportPrimitive::Kv => Self::Kv,
            CliArrowExportPrimitive::Json => Self::Json,
            CliArrowExportPrimitive::Event => Self::Event,
            CliArrowExportPrimitive::Vector => Self::Vector,
            CliArrowExportPrimitive::Graph => Self::Graph,
        }
    }
}

/// Raw serialized command wrapper.
#[derive(Debug, Args)]
pub(crate) struct CommandArgs {
    /// Raw command operation.
    #[command(subcommand)]
    pub(crate) command: CommandCommand,
}

/// Raw serialized command operations.
#[derive(Debug, Subcommand)]
pub(crate) enum CommandCommand {
    /// Execute a serialized executor command.
    Run {
        /// Serialized command JSON.
        #[arg(
            long = "command-json",
            id = "run_command_json",
            conflicts_with = "run_command_file"
        )]
        json: Option<String>,
        /// File containing serialized command JSON.
        #[arg(long, id = "run_command_file", conflicts_with = "run_command_json")]
        file: Option<PathBuf>,
    },
    /// Validate and print a serialized executor command without opening a database.
    Print {
        /// Serialized command JSON.
        #[arg(
            long = "command-json",
            id = "print_command_json",
            conflicts_with = "print_command_file"
        )]
        json: Option<String>,
        /// File containing serialized command JSON.
        #[arg(long, id = "print_command_file", conflicts_with = "print_command_json")]
        file: Option<PathBuf>,
    },
}

/// Inference command arguments.
#[cfg(feature = "inference")]
#[derive(Debug, Args)]
pub(crate) struct InferenceArgs {
    /// Inference command.
    #[command(subcommand)]
    pub(crate) command: InferenceCommand,
}

/// Model execution commands.
///
/// Model specs are catalog names (`tinyllama`, `qwen3:1.7b`), catalog
/// name:quant pairs (`tinyllama:q8_0`), local GGUF paths, or provider specs
/// (`anthropic:claude-...`) depending on the enabled features.
#[cfg(feature = "inference")]
#[derive(Debug, Subcommand)]
pub(crate) enum InferenceCommand {
    /// Model catalog and local model management.
    Models(InferenceModelsArgs),
    /// Show capability facts for one model spec.
    Capability {
        /// Model spec.
        model: String,
    },
    /// Generate text with a model.
    Generate {
        /// Model spec.
        model: String,
        /// Prompt text (chat models expect their chat template verbatim).
        prompt: String,
        /// Maximum completion tokens (default 256).
        #[arg(long)]
        max_tokens: Option<usize>,
        /// Sampling temperature (default 0.0, greedy).
        #[arg(long)]
        temperature: Option<f32>,
        /// Top-k sampling cutoff.
        #[arg(long)]
        top_k: Option<usize>,
        /// Nucleus sampling cutoff.
        #[arg(long)]
        top_p: Option<f32>,
        /// Deterministic sampling seed.
        #[arg(long)]
        seed: Option<u64>,
        /// Stop sequence (repeatable).
        #[arg(long = "stop", value_name = "TEXT")]
        stop_sequences: Vec<String>,
        /// Stop token id (repeatable; local models only).
        #[arg(long = "stop-token", value_name = "ID")]
        stop_tokens: Vec<u32>,
        /// GBNF grammar for constrained generation (local models).
        #[arg(long)]
        grammar: Option<String>,
    },
    /// Tokenize text with a local model.
    Tokenize {
        /// Model spec.
        model: String,
        /// Text to tokenize.
        text: String,
        /// Add the model's special tokens.
        #[arg(long)]
        special: bool,
    },
    /// Detokenize token ids with a local model.
    Detokenize {
        /// Model spec.
        model: String,
        /// Token ids.
        #[arg(required = true)]
        ids: Vec<u32>,
    },
    /// Embed one text.
    Embed {
        /// Model spec.
        model: String,
        /// Text to embed.
        text: String,
    },
    /// Embed multiple texts in order.
    EmbedBatch {
        /// Model spec.
        model: String,
        /// Texts to embed.
        #[arg(required = true)]
        texts: Vec<String>,
    },
    /// Rank passages against a query.
    Rank {
        /// Model spec.
        model: String,
        /// Query text.
        query: String,
        /// Candidate passages.
        #[arg(required = true)]
        passages: Vec<String>,
    },
    /// Unload one cached model, or all cached models when omitted.
    Unload {
        /// Model spec (omit to unload everything).
        model: Option<String>,
    },
    /// Show runtime model-cache diagnostics.
    CacheStatus,
}

/// Inference model management arguments.
#[cfg(feature = "inference")]
#[derive(Debug, Args)]
pub(crate) struct InferenceModelsArgs {
    /// Models command.
    #[command(subcommand)]
    pub(crate) command: InferenceModelsCommand,
}

/// Inference model management commands.
#[cfg(feature = "inference")]
#[derive(Debug, Subcommand)]
pub(crate) enum InferenceModelsCommand {
    /// List catalog models.
    List,
    /// List locally available models.
    Local,
    /// Download a model into the local model directory.
    ///
    /// Honors `STRATA_MODELS_DIR`, `STRATA_HF_ENDPOINT`, and
    /// `STRATA_HF_TOKEN` (or `HF_TOKEN`) for gated repositories.
    Pull {
        /// Model spec or catalog name.
        model: String,
    },
}
