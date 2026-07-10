# Executor Vector Command Contract Implementation Plan

## Problem

The executor crate is the serialized command boundary for SDKs, MCP servers,
CLIs, IPC clients, and smoke tools. Vector commands should use the same command
dispatch architecture restored for KV and JSON: clients send a serialized
`Command`, executor applies command-boundary validation and defaults, engine
performs product semantics, and executor returns a serialized `Output`.

The old executor exposed a useful vector command set. The rebuilt executor
currently exposes branch, KV, and JSON commands only. This plan restores vector
command coverage while keeping vector behavior inside engine.

## Old Evidence

- `crates/executor/src/command.rs`
- `crates/executor/src/output.rs`
- `crates/executor/src/types.rs`
- `crates/executor/src/executor.rs`
- `crates/executor/src/handlers/vector.rs`
- `crates/engine/src/semantics/vector.rs`
- `crates/engine/src/vector/store/collections.rs`
- `crates/engine/src/vector/store/crud.rs`
- `crates/engine/src/vector/store/search.rs`

## Current Targets

- `crates/executor-next/src/command.rs`
- `crates/executor-next/src/output.rs`
- `crates/executor-next/src/types.rs`
- `crates/executor-next/src/executor.rs`
- `crates/executor-next/tests/`
- `crates/engine-next/src/api/vector.rs`
- `crates/engine-next/src/data/vector/`

## Required Engine Surface

Do not implement executor vector dispatch until the engine vector API exists.
The executor implementation depends on these engine methods from
`engine-vector-parity-implementation-plan.md`:

- `create_collection`
- `delete_collection`
- `list_collections`
- `collection_info`
- `count`
- `upsert`
- `get`
- `get_versioned`
- `get_at`
- `history`
- `exists`
- `list_keys`
- `update_metadata`
- `delete`
- `delete_by_filter`
- `delete_all`
- `batch_upsert`
- `batch_get`
- `batch_delete`
- `query`
- `query_at`

## Design Decisions

1. **Serialized command remains the public executor path.** Rust convenience
   methods for vector must build and execute `Command::Vector...` variants.

2. **Executor is a stateless delegator.** It may deserialize command payloads,
   default branch/space, validate public request shape, convert wire types,
   map errors, and shape outputs. It must not compute vector scores, apply
   metadata filters, patch metadata, maintain counts, or scan storage rows.

3. **Engine owns vector semantics.** Collection config, dimension checks,
   finite embedding checks, exact search, score normalization, filter matching,
   metadata patch behavior, tombstones, history, branch isolation, and
   timestamp visibility stay in engine.

4. **Vector values stay numeric.** Embeddings are serialized as JSON arrays of
   numbers in the command payload and as `Vec<f32>` in Rust wire types. Do not
   route vector values through byte KV payloads.

5. **Use vector-specific output variants.** Do not force vector reads, matches,
   key pages, history, or batch results through generic KV or JSON variants.

6. **Branch and space defaults match KV and JSON.** Omitted branch resolves to
   the executor handle default branch. Omitted space resolves to `"default"`.

7. **Batch validation is positional where useful.** Batch get and batch delete
   preserve positional output. Batch upsert should either fail the whole command
   for invalid shape or return positional errors only if the engine batch API
   supports a valid-subset contract. The engine vector plan currently prefers
   validate-all-before-commit.

8. **Filtered delete requires an explicit filter.** Empty filter is invalid for
   `VectorDeleteByFilter`. Full collection deletion must use
   `VectorDeleteAll`.

9. **Metadata patch is intentionally narrow.** The wire patch is a top-level
   JSON object. Executor does not interpret field paths or null deletion rules;
   it delegates patch semantics to engine.

10. **Query-by-key and projection flags are deferred.** The first executor
    vector slice searches by supplied dense query vector and returns the
    structured match output. Query-by-existing-key and include/exclude
    projection controls can be added after the core surface is stable.

## Public Vector Command Set

Add these command variants:

| Command | Inputs | Output |
| --- | --- | --- |
| `VectorCreateCollection` | branch?, space?, collection, dimension, metric | `VectorCollectionInfo` |
| `VectorDeleteCollection` | branch?, space?, collection | `Bool` |
| `VectorListCollections` | branch?, space? | `VectorCollectionList` |
| `VectorCollectionStats` | branch?, space?, collection | `VectorCollectionList` |
| `VectorCount` | branch?, space?, collection | `Uint` |
| `VectorUpsert` | branch?, space?, collection, key, vector, metadata? | `VectorWriteResult` |
| `VectorGet` | branch?, space?, collection, key, as_of? | `VectorData` |
| `VectorGetv` | branch?, space?, collection, key | `VectorVersionHistory` |
| `VectorExists` | branch?, space?, collection, key | `Bool` |
| `VectorListKeys` | branch?, space?, collection, prefix?, cursor?, limit? | `VectorKeyPage` |
| `VectorUpdateMetadata` | branch?, space?, collection, key, patch | `VectorMetadataUpdateResult` |
| `VectorDelete` | branch?, space?, collection, key | `VectorDeleteResult` |
| `VectorDeleteByFilter` | branch?, space?, collection, filter | `VectorBulkDeleteResult` |
| `VectorDeleteAll` | branch?, space?, collection | `VectorBulkDeleteResult` |
| `VectorQuery` | branch?, space?, collection, query, k, filter?, as_of? | `VectorMatches` |
| `VectorBatchUpsert` | branch?, space?, collection, entries | `VectorBatchUpsertResults` |
| `VectorBatchGet` | branch?, space?, collection, keys | `VectorBatchGetResults` |
| `VectorBatchDelete` | branch?, space?, collection, keys | `VectorBatchDeleteResults` |

Preserve old field names where they exist: `collection`, `key`, `vector`,
`metadata`, `query`, `k`, `filter`, `metric`, `dimension`, `entries`, `keys`,
`as_of`, `branch`, and `space`.

Use `prefix`, `cursor`, `limit`, and `patch` for the new operations.

## Wire Types

Add serializable request types:

- `VectorDistanceMetric`
  - cosine
  - euclidean
  - dot_product
- `VectorScalar`
  - null
  - bool
  - number
  - string
- `VectorFilterOp`
  - eq for the first slice
  - optionally ne/gt/gte/lt/lte/in/nin/exists only after engine supports them
- `VectorFilterCondition`
  - field
  - op
  - value
- `VectorMetadataFilter`
  - conditions with AND semantics for the first slice
- `BatchVectorEntry`
  - key
  - vector
  - metadata
- `VectorMetadataPatch`
  - top-level JSON object

Add serializable output helper types:

- `VectorData`
  - embedding
  - metadata
- `VectorVersionedData`
  - key
  - data
  - version
  - timestamp
  - vector_revision
- `VectorHistoryItem`
  - key
  - data
  - version
  - timestamp
  - vector_revision
  - tombstone
- `VectorMatch`
  - key
  - score
  - metadata
- `VectorCollectionInfo`
  - name
  - dimension
  - metric
  - count
- `VectorBatchItemResult`
  - version
  - timestamp
  - vector_revision
  - error
- `VectorBatchGetItemResult`
  - value
  - error

## Output Variants

Add vector-specific output variants:

- `VectorWriteResult { collection, key, version, timestamp, vector_revision }`
- `VectorMetadataUpdateResult { collection, key, updated, version, timestamp, vector_revision }`
- `VectorDeleteResult { collection, key, deleted, version, timestamp }`
- `VectorBulkDeleteResult { collection, deleted_count, version, timestamp }`
- `VectorData(Option<VectorVersionedData>)`
- `VectorVersionHistory(Option<Vec<VectorHistoryItem>>)`
- `VectorMatches(Vec<VectorMatch>)`
- `VectorKeyPage { keys, has_more, cursor }`
- `VectorCollectionList(Vec<VectorCollectionInfo>)`
- `VectorBatchUpsertResults(Vec<VectorBatchItemResult>)`
- `VectorBatchGetResults(Vec<VectorBatchGetItemResult>)`
- `VectorBatchDeleteResults(Vec<VectorBatchItemResult>)`

Shared variants may remain shared when primitive-neutral:

- `Bool`
- `Uint`

Do not reuse `KeysPage` for vector keys unless its byte-oriented shape is
changed or a string-key alias is added. Vector keys are product strings.

## Implementation Order

### 1. Engine Vector API Gate

- Ensure `Database::vector` and the required service methods exist.
- Ensure engine vector outcomes expose the facts needed by executor outputs.
- Ensure engine errors include stable categories for invalid collection,
  invalid key, invalid embedding, dimension mismatch, missing collection,
  invalid metadata patch, invalid filter, and closed handle.

### 2. Wire Types

- Add vector distance metric, scalar, filter, patch, batch entry, collection,
  vector data, history, match, and batch result wire types to `types.rs`.
- Use private fields with constructors/accessors if matching current executor
  style.
- Add conversion helpers from executor wire types to engine vector types.

### 3. Command Variants

- Add every vector command variant to `Command`.
- Add `Command::name()` coverage for every vector command.
- Add branch/space default helper coverage for every vector command.
- Preserve serde tagged command shape and `deny_unknown_fields`.

### 4. Output Variants

- Add vector-specific output variants.
- Ensure every output variant serializes and deserializes through serde JSON.
- Keep embeddings as numeric arrays, metadata as JSON values, and keys as
  strings.

### 5. Dispatch Helpers

- Add `Executor::vector_service(branch, space)`.
- Add conversion helpers for collection name, vector key, embedding, metadata,
  metadata patch, filters, metric, timestamp, cursor, and limit.
- Convert engine outcomes into executor outputs without exposing storage facts.

### 6. Collection Commands

- `VectorCreateCollection` delegates to `create_collection`.
- `VectorDeleteCollection` delegates to `delete_collection`.
- `VectorListCollections` delegates to `list_collections`.
- `VectorCollectionStats` delegates to `collection_info`.
- `VectorCount` delegates to `count`.

### 7. Single Vector Commands

- `VectorUpsert` delegates to `upsert`.
- `VectorGet` delegates to `get_versioned` for latest reads and `get_at` for
  timestamp reads.
- `VectorGetv` delegates to `history`.
- `VectorExists` delegates to `exists`.
- `VectorListKeys` delegates to `list_keys`.
- `VectorUpdateMetadata` delegates to `update_metadata`.
- `VectorDelete` delegates to `delete`.

### 8. Bulk Delete Commands

- `VectorDeleteByFilter` validates that a filter was supplied and delegates to
  `delete_by_filter`.
- `VectorDeleteAll` delegates to `delete_all`.
- Executor must not scan keys and issue per-key deletes for either command.

### 9. Query Command

- `VectorQuery` delegates to `query` for latest search.
- `VectorQuery` with `as_of` delegates to `query_at`.
- Executor converts filter wire types and query vector shape only.
- Executor does not compute scores, sort matches, or apply filters.

### 10. Batch Commands

- `VectorBatchUpsert` validates command shape and delegates to one engine batch
  upsert call.
- `VectorBatchGet` delegates to one engine batch get call.
- `VectorBatchDelete` delegates to one engine batch delete call.
- Empty batches return empty vector batch outputs.
- Outputs preserve input order.

### 11. Error Boundary

- Map engine invalid input, not found, conflict, unavailable, corruption, and
  closed-handle errors into executor error classes.
- Dimension mismatch should include expected and actual dimension when engine
  exposes them.
- Public error messages must not include storage keys, row classes, table names,
  WAL paths, or persistence adapter internals.

### 12. Convenience API

- Add typed convenience methods only after command dispatch works.
- Convenience methods must build `Command::Vector...` and call `execute`.
- Do not expose engine vector service directly from executor.

### 13. Source Guards

- Executor crate must not depend on storage crates.
- Executor vector code must not mention storage row, storage commit, table, WAL,
  lifecycle, compaction, HNSW, mmap, quantization, or sidecar modules.
- Executor vector convenience helpers must call `execute(Command::Vector...)`.
- Benchmarks and smoke loaders must use executor vector commands or public
  engine vector APIs, not lower-layer persistence internals.

## Non-Goals

- Implementing engine vector semantics.
- Implementing ANN indexes, sparse vectors, hybrid ranking, reranking, or
  integrated embedding.
- Implementing query-by-existing-key.
- Implementing projection flags for vector values or metadata.
- Implementing import/export.
- Implementing storage-level bulk ingestion shortcuts.

## Completion Criteria

- Every vector command variant serializes, deserializes, has a stable name, and
  resolves branch/space defaults.
- Executor vector dispatch delegates through public engine vector APIs.
- Cache and durable-local behavior tests pass for collections, CRUD, list-keys,
  metadata patch, batch, filtered delete, delete-all, query, history, branch,
  space, and reopen behavior.
- Source guards prove executor does not own vector semantics or storage access.
- The executor vector API is ready for SDK, CLI, MCP, and benchmark callers.
