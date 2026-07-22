use super::{Deserialize, Serialize};

/// Arrow file format selected for import/export.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum ArrowImportTarget {
    /// Import rows into the KV primitive.
    Kv,
    /// Import rows into the JSON primitive.
    Json,
    /// Import rows into a vector collection.
    Vector,
    /// Import nodes and edges into a graph (from the `_nodes`/`_edges` files).
    Graph,
}

/// Product primitive selected by Arrow export.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
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
    /// Export one graph as node and edge files derived from the output stem.
    Graph,
}

/// Arrow import summary.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
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
#[cfg_attr(feature = "idl-tooling", derive(schemars::JsonSchema))]
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

    /// Returns the concrete output paths written by the export.
    ///
    /// Graph exports return separate node and edge table paths; callers should
    /// use this list rather than assuming the requested path is itself a file.
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
