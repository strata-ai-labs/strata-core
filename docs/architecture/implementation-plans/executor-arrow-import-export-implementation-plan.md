# Executor Arrow Import/Export Implementation Plan

## Purpose

Port the old executor Arrow import/export support onto the rebuilt executor
command boundary. Arrow is a default product data interchange feature for
Parquet, CSV, and JSONL files, with an explicit no-default-features opt-out for
minimal builds. It must move data through normal executor
commands and batch APIs rather than reaching into engine services, storage rows,
or primitive internals.

The goal is operational bulk data movement over the five core primitives:

- KV
- JSON
- Vector
- Event
- Graph core

The first port should preserve the useful old behavior while correcting the
architectural boundary: executor Arrow code may parse files, map columns, build
commands, execute commands, and shape import/export reports. Product semantics
stay in engine.

## Old Architecture Evidence

- `crates/executor/Cargo.toml`
- `Cargo.toml`
- `crates/executor/src/lib.rs`
- `crates/executor/src/command.rs`
- `crates/executor/src/output.rs`
- `crates/executor/src/types.rs`
- `crates/executor/src/handlers/arrow_import.rs`
- `crates/executor/src/handlers/export.rs`
- `crates/executor/src/arrow/mod.rs`
- `crates/executor/src/arrow/reader.rs`
- `crates/executor/src/arrow/writer.rs`
- `crates/executor/src/arrow/schema.rs`
- `crates/executor/src/arrow/ingest.rs`
- `crates/executor/src/arrow/export.rs`

## Old Behavior Summary

### Feature Wiring

- Root feature `arrow` enabled `strata-executor/arrow`.
- `strata-executor/arrow` enabled optional `arrow` and `parquet`
  dependencies.
- Arrow support was not part of the executor default feature set.

### File Formats

- Input and output file formats:
  - Parquet
  - CSV with header row
  - JSONL, with `.jsonl` and `.json` extensions treated as JSONL
- Format detection used file extension unless the command supplied a format.
- Parquet writing used Snappy compression.
- Reader implementations eagerly collected `RecordBatch` values because
  Arrow reader iterators borrow file handles.

### Import Surface

Old `Command::ArrowImport` accepted:

- `branch`
- `space`
- `file_path`
- `target`
- `key_column`
- `value_column`
- `collection`
- `format`

Supported import targets:

- `kv`
- `json`
- `vector`

Old import did not support event import or graph import.

Column mapping rules:

- Key column auto-detected in this order: `key`, `_id`, `id`.
- Explicit key column overrode auto-detection.
- KV value column auto-detected as `value`; two-column input used the non-key
  column as value; otherwise non-key columns became a JSON object.
- JSON document column auto-detected in this order:
  `document`, `value`, `doc`, `body`.
- JSON import parsed string documents as JSON when possible and otherwise
  stored the string value.
- Vector embedding column auto-detected in this order:
  `embedding`, `vector`, `embeddings`, `emb`.
- Vector embedding columns had to be `FixedSizeList<Float32|Float64>` or
  `List<Float32|Float64>`.
- Vector import required a collection name.
- Vector import auto-created a missing collection using cosine distance and the
  first valid embedding dimension.
- Null keys and invalid embeddings were skipped and counted.
- KV, JSON, and vector import wrote one engine batch per Arrow `RecordBatch`.

### Export Surface

Old `Command::DbExport` accepted:

- `branch`
- `space`
- `primitive`
- `format`
- `prefix`
- `limit`
- `path`
- `collection`
- `graph`

Supported export primitives:

- KV
- JSON
- Events
- Vector
- Graph

Old export behavior:

- Inline CSV, JSON array, and JSONL rendering existed without Arrow.
- Parquet required the Arrow feature and an output path.
- CSV and JSONL used Arrow writers when an output path was present and the
  Arrow feature was enabled.
- JSON array format was not supported through the Arrow writer path.
- Vector file export required the Arrow feature and an output path.
- Graph file export produced two files from one requested path:
  `<stem>_nodes.<ext>` and `<stem>_edges.<ext>`.

Old export schemas:

- KV: `key`, `value`, `version`, `timestamp`
- JSON: `key`, `document`
- Event: `sequence`, `event_type`, `payload`, `timestamp`
- Vector: `key`, `embedding`, `metadata`
- Graph nodes: `node_id`, `object_type`, `properties`
- Graph edges: `source`, `target`, `edge_type`, `weight`, `properties`

## Current Targets

- `crates/executor-next/Cargo.toml`
- `crates/executor-next/src/lib.rs`
- `crates/executor-next/src/command.rs`
- `crates/executor-next/src/output.rs`
- `crates/executor-next/src/types.rs`
- `crates/executor-next/src/executor.rs`
- `crates/executor-next/src/arrow/`
- `crates/executor-next/tests/`
- Workspace `Cargo.toml` feature wiring

## Design Decisions

1. **Arrow is enabled by default.**
   Add an `arrow` feature to `strata-executor-next` that enables optional
   `arrow` and `parquet` dependencies, and include it in the default feature
   set. Keep `--no-default-features` as the explicit minimal-build opt-out.

2. **Commands stay serializable in the minimal build.**
   Arrow command and output variants should compile without the `arrow`
   feature. Executing Arrow commands without the feature returns a stable
   feature-disabled error.

3. **Executor Arrow code must use executor commands.**
   Import code builds `Command::KvBatchPut`, `Command::JsonBatchSet`,
   `Command::VectorBatchUpsert`, and related setup commands. Export code uses
   list, scan, batch get, range, neighbor, and graph read commands. It must not
   import engine data modules, primitive services, persistence adapters, or
   storage crates.

4. **Port import support for old targets first.**
   Implement import for KV, JSON, and vector. Event and graph import are
   deferred because the old architecture did not support them and their
   semantics need separate design.

5. **Export all five core primitives.**
   Implement export for KV, JSON, event, vector, and graph core. Graph export
   remains split into node and edge files when the public primitive is graph.

6. **Preserve old mapping where it still fits.**
   Keep key-column detection, document-column detection, embedding-column
   detection, format detection, JSONL behavior, and vector auto-create behavior.

7. **Adapt KV carefully because the rebuilt KV primitive is byte-shaped.**
   The old KV primitive stored typed `Value`. The rebuilt KV primitive stores
   bytes. The port must make byte encoding explicit:
   - UTF-8 string columns import as raw UTF-8 bytes.
   - Binary columns import as raw bytes.
   - Numeric and boolean value columns import as their display text encoded as
     UTF-8.
   - Extra-column JSON object import writes the JSON object bytes.
   - KV Parquet export should include lossless binary `key` and `value`
     columns.
   - KV CSV and JSONL export should expose string-safe encodings documented by
     the output schema.

8. **Do not make Arrow a primitive.**
   Arrow import/export is an executor interchange layer. It does not own
   product identity, storage shape, branch semantics, or commit semantics.

9. **Graph export uses the public graph core surface.**
   Nodes come from `GraphListNodes`. Edges come from outgoing
   `GraphNeighbors` for each visible node. The exporter deduplicates only as a
   cursor/page safety measure; graph semantics still belong to engine.

10. **Inline export is a separate compatibility choice.**
    The first Arrow slice should prioritize file import/export. If inline CSV,
    JSON array, and JSONL compatibility is restored, route it through the same
    row builders and document it as data export, not Arrow file export.

## Public Command Surface

Add these command variants to `strata_executor_next::Command`:

| Command | Inputs | Output |
| --- | --- | --- |
| `ArrowImport` | branch?, space?, file_path, target, key_column?, value_column?, collection?, format? | `ArrowImportResult` |
| `ArrowExport` | branch?, space?, primitive, format, path, prefix?, limit?, collection?, graph? | `ArrowExportResult` |

Use typed enums instead of free-form strings:

- `ArrowFileFormat`
  - `parquet`
  - `csv`
  - `jsonl`
- `ArrowImportTarget`
  - `kv`
  - `json`
  - `vector`
- `ArrowExportPrimitive`
  - `kv`
  - `json`
  - `event`
  - `vector`
  - `graph`

Command names:

- `arrow_import`
- `arrow_export`

## Public Output Surface

Add output variants:

- `ArrowImportResult`
- `ArrowExportResult`

Suggested result fields:

```text
ArrowImportResult {
  target,
  file_path,
  rows_imported,
  rows_skipped,
  batches_processed,
}

ArrowExportResult {
  primitive,
  format,
  row_count,
  paths,
  size_bytes,
}
```

`paths` is a vector because graph export writes both node and edge files.

## Internal Module Layout

Create `crates/executor-next/src/arrow/`:

- `mod.rs`
- `format.rs`
- `reader.rs`
- `writer.rs`
- `schema.rs`
- `import.rs`
- `export.rs`

Responsibilities:

- `format.rs`: `ArrowFileFormat`, parsing, extension detection.
- `reader.rs`: file readers to `Vec<RecordBatch>`.
- `writer.rs`: file writers from `RecordBatch` slices.
- `schema.rs`: import mapping, Arrow scalar conversion, row JSON helpers,
  schema constructors.
- `import.rs`: command-driven import loops.
- `export.rs`: command-driven export collectors and batch builders.

## Import Pipeline

1. Validate file path exists.
2. Resolve file format from command or extension.
3. Read `(Schema, Vec<RecordBatch>)`.
4. Resolve import mapping from schema, target, key column, and value column.
5. For each batch:
   - Convert rows into command entries.
   - Skip rows with null keys or invalid embeddings.
   - Execute one executor batch command per input `RecordBatch`.
6. Return `ArrowImportResult`.

### KV Import

Use `Command::KvBatchPut`.

Mapping:

- key column to `Bytes`
- value column to `Bytes`
- no value column means extra columns become a JSON object encoded as bytes
- if generated `key_encoding` or `value_encoding` columns are present, decode
  `base64` rows and pass `utf8` rows through as UTF-8 bytes

Rows with null keys are skipped.

### JSON Import

Use `Command::JsonBatchSet` at the root path.

Mapping:

- key column to JSON document key
- document column to `serde_json::Value`
- no document column means extra columns become an object

String document cells parse as JSON when possible. If parsing fails, store the
string as a JSON string value, matching old behavior.

### Vector Import

Use:

- `Command::VectorListCollections`
- `Command::VectorCreateCollection` if missing
- `Command::VectorBatchUpsert`

Mapping:

- key column to vector key
- embedding column to `Vec<f32>`
- extra columns to metadata object

The first valid embedding determines dimension for auto-create. Metric defaults
to cosine to preserve old behavior.

## Export Pipeline

1. Validate path and parent directory.
2. Resolve output format.
3. Use executor commands to collect rows for the selected primitive.
4. Build one or more Arrow `RecordBatch` values.
5. Write file(s).
6. Return `ArrowExportResult`.

### KV Export

Use `Command::KvScan` or paginated `KvList` plus `KvBatchGet`.

Recommended schema:

- Parquet:
  - `key`: Binary
  - `value`: Binary
  - `version`: UInt64
  - `timestamp`: UInt64
- CSV and JSONL:
  - `key`: Utf8
  - `key_encoding`: Utf8
  - `value`: Utf8
  - `value_encoding`: Utf8
  - `version`: UInt64
  - `timestamp`: UInt64

Encoding values:

- `utf8` when bytes are valid UTF-8
- `base64` otherwise

The KV importer must recognize this exported schema so a CSV or JSONL export
can be imported into a fresh database without losing non-UTF-8 bytes.

### JSON Export

Use `Command::JsonList` and `Command::JsonBatchGet`.

Schema:

- `key`: Utf8
- `document`: Utf8
- `version`: UInt64, if available from selected command path
- `timestamp`: UInt64, if available from selected command path

If version and timestamp are not available through the first command path,
either use versioned reads or omit the metadata fields consistently. Do not
read storage rows to recover metadata.

### Event Export

Use `Command::EventRange` with pagination.

Schema:

- `sequence`: UInt64
- `event_type`: Utf8
- `payload`: Utf8
- `timestamp`: UInt64
- `version`: UInt64
- `hash`: Utf8
- `prev_hash`: Utf8

### Vector Export

Use `Command::VectorListKeys` and `Command::VectorBatchGet`.

Schema:

- `key`: Utf8
- `embedding`: FixedSizeList<Float32>
- `metadata`: Utf8 nullable
- `version`: UInt64
- `timestamp`: UInt64
- `vector_revision`: UInt64

### Graph Export

Use:

- `Command::GraphListNodes`
- `Command::GraphNeighbors` with outgoing direction

Graph export writes two files:

- `<stem>_nodes.<ext>`
- `<stem>_edges.<ext>`

Node schema:

- `node_id`: Utf8
- `properties`: Utf8 nullable
- `binding_primitive`: Utf8 nullable
- `binding_branch`: Utf8 nullable
- `binding_space`: Utf8 nullable
- `binding_key`: Utf8 nullable
- `version`: UInt64
- `timestamp`: UInt64

Edge schema:

- `source`: Utf8
- `target`: Utf8
- `edge_type`: Utf8
- `weight`: Float64
- `properties`: Utf8 nullable
- `version`: UInt64
- `timestamp`: UInt64

## Error Handling

Map errors through `ExecutorError` using the existing public classifications:

- missing file: invalid input
- unsupported extension: invalid input
- unsupported format string: invalid input
- Arrow feature disabled: invalid input or failed precondition with a stable
  code and rebuild hint
- missing vector collection argument: invalid input
- missing graph argument: invalid input
- missing embedding column: invalid input
- unsupported embedding type: invalid input
- malformed Arrow file: IO or internal, depending on existing executor error
  vocabulary
- command execution failure: preserve executor error class

Do not expose storage row keys, table names, WAL facts, engine persistence
records, or lower-layer type names in public Arrow errors.

## Source Guard Requirements

Extend `crates/executor-next/tests/error_and_guards.rs`:

- Arrow source may import `arrow` and `parquet` behind the `arrow` feature.
- Arrow source must not import `strata_storage_next`.
- Arrow source must not import engine data modules such as
  `strata_engine_next::data::*`.
- Arrow source must not call `database.kv(...)`, `database.json(...)`,
  `database.vector(...)`, `database.event(...)`, or `database.graph(...)`
  directly.
- Arrow source must not name storage rows, storage commits, object tables, WAL,
  lifecycle, compaction, or persistence key codecs.
- Arrow import/export behavior tests must use `Executor::execute` or public
  executor convenience methods that themselves call `execute(Command::...)`.

## Implementation Order

1. Add `arrow` feature and optional dependencies to `crates/executor-next`.
2. Add Arrow command, output, and helper enum types with serde coverage.
3. Add disabled-feature command handlers that return stable feature errors.
4. Port file format detection, reader, and writer modules.
5. Port schema resolution and Arrow-to-public-value conversion.
6. Implement KV import through `KvBatchPut`.
7. Implement JSON import through `JsonBatchSet`.
8. Implement vector import through collection commands and
   `VectorBatchUpsert`.
9. Implement KV, JSON, event, vector, and graph export through executor
   commands.
10. Add source guards for no lower-layer bypass.
11. Add command contract and behavior tests.
12. Update root feature wiring only after the executor crate feature is stable.

## Non-Goals

- No event import in this slice.
- No graph import in this slice.
- No ontology import/export in this slice.
- No inference, embedding, reranking, or search integration.
- No direct engine service or storage API usage from Arrow code.
- No migration of old inline non-Arrow renderer unless explicitly included
  after file import/export is stable.

## Done Criteria

- `strata-executor-next` builds with Arrow enabled by default.
- The explicit no-default-features build still compiles without the `arrow`
  feature.
- Arrow commands serialize and deserialize without the `arrow` feature.
- Executing Arrow commands without the feature returns stable feature-disabled
  errors.
- KV, JSON, and vector import work for Parquet, CSV, and JSONL.
- KV, JSON, event, vector, and graph export work for Parquet, CSV, and JSONL.
- Import and export only use executor commands and batch APIs.
- Graph export produces node and edge files and reports both paths.
- Source guards reject storage and engine-internal bypasses.
- Focused Arrow tests, executor tests, formatting, and clippy pass.
