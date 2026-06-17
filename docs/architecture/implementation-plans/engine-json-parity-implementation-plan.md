# Engine JSON Parity Implementation Plan

## Problem

The executor command boundary should remain a stateless delegator over engine
APIs. That only works for JSON if the engine owns document identity, path
mutation, point-in-time visibility, version history, branch isolation, space
isolation, list/count/sample behavior, and index metadata. The rebuilt engine
currently has the branch and KV spine only. JSON parity should reuse the KV
structure rather than building a separate shape.

## Old Evidence

- `crates/engine/src/primitives/json/mod.rs`
- `crates/engine/src/primitives/json/index.rs`
- `crates/engine/src/semantics/json.rs`
- `crates/engine/src/transaction/json_state.rs`
- `crates/executor/src/handlers/json.rs`
- `crates/executor/src/command.rs`
- `crates/executor/src/types.rs`
- `crates/executor/src/output.rs`

## Current Targets

- `crates/engine-next/src/api/`
- `crates/engine-next/src/data/json/`
- `crates/engine-next/src/persistence/`
- `crates/engine-next/tests/`
- `crates/executor-next/src/`
- `crates/executor-next/tests/`

## Current Status

Already present in the rebuilt engine:

- cache and durable-local database open
- branch create/list/lookup/delete/fork
- KV public service, outcomes, persistence adapter, and tests
- shared `ProductSpace` validation through the KV module
- persistence row classes for KV and engine control rows

Missing for JSON:

- JSON row class and row-key encoding
- JSON public input/output types
- JSON path parser and path mutation helpers
- document serialization format
- create/set-or-create/set-at-path/delete-at-path/root delete
- latest get, versioned get, timestamp get, full history
- batch set/get/delete
- exists, list, list-at, count, sample
- secondary index metadata and index-entry maintenance
- executor JSON command variants and output variants in the rebuilt executor

## Design Decisions

1. **JSON follows the KV primitive shape.** Public API types live under `api`,
   validated product types and service behavior live under `data/json`,
   persistence translation lives under `persistence`, and storage request types
   never cross the engine public API.

2. **Engine owns JSON semantics.** Path parsing, path traversal, document
   creation, path mutation, deletion semantics, document versioning,
   timestamp/version reads, tombstone suppression, index metadata, branch
   isolation, and space isolation must stay in engine.

3. **Executor owns serialization, not product behavior.** The executor may
   deserialize commands, apply branch/space defaults, validate command shape,
   map errors, and shape outputs. It must not mutate JSON documents itself.

4. **Use engine-owned JSON types.** Add `JsonDocumentId`, `JsonPath`,
   `JsonValue`, `JsonVersionedValue`, `JsonHistory`, `JsonListPage`,
   `JsonSample`, and index DTOs. Do not expose `serde_json::Value` directly
   from every method signature unless wrapped in engine-owned types.

5. **Do not revive generic optional-value architecture.** The old executor used
   generic optional-value outputs. The rebuilt executor should add
   JSON-specific output variants, matching the KV structure:
   `JsonValue`, `JsonVersionedValue`, `JsonVersionHistory`, `JsonListResult`,
   `JsonSampleResult`, and JSON batch result types.

6. **Root path and document path are different operations.** Setting root
   replaces the full document. Setting a non-root path creates or updates a
   nested value. Deleting root destroys the whole document. Deleting a non-root
   path mutates the document and increments its document version.

7. **Batch writes are one engine commit when valid.** A batch command may
   surface item-level validation errors at the executor layer, but the valid
   subset should flow into engine as a single batch operation, not a per-item
   command loop.

8. **Document version is product metadata.** Commit version/timestamp come from
   storage rows; document version increments on create/path update/path delete.
   Public outcomes should expose commit metadata and, where useful, document
   version as separate facts.

9. **Index metadata is JSON-owned, not search-owned.** The JSON primitive owns
   secondary index definitions and index-entry maintenance. Full-text/vector
   search hooks are out of this slice.

10. **List search remains out of JSON parity.** The active old `JsonList`
    command supports prefix, cursor, limit, and timestamp. Structured search is
    a separate `Search` command and should stay with the search primitive.

## Shared Primitive Structure Target

JSON should match the KV structure:

- `api/json.rs` re-exports only public service and outcome types.
- `data/json/types.rs` owns validated document id, path, value, index name, and
  index type wrappers.
- `data/json/service.rs` owns product operations, branch/space semantics, and
  JSON path behavior.
- `data/json/outcome.rs` owns read/list/history/sample/index results.
- `persistence/key.rs` owns JSON row-key encoding and decoding.
- `persistence/space.rs` owns the JSON row class assignment.
- `persistence/adapter.rs` owns storage request construction and row mapping.
- Tests are grouped by public behavior, persistence translation, and guards.

## Public Engine API Target

Add public JSON types:

- `JsonDocumentId`
- `JsonPath`
- `JsonPathSegment`
- `JsonValue`
- `JsonVersionedValue`
- `JsonHistory`
- `JsonHistoryRow`
- `JsonListPage`
- `JsonSample`
- `JsonSampleRow`
- `JsonDeleteOutcome`
- `JsonBatchSetOutcome`
- `JsonBatchDeleteOutcome`
- `JsonIndexType`
- `JsonIndexDefinition`

Add `Database::json(branch, space) -> EngineResult<JsonService>`.

Add `JsonService` methods:

- `create(id, value) -> CommitOutcome`
- `set_or_create(id, path, value) -> JsonWriteOutcome`
- `set(id, path, value) -> JsonWriteOutcome`
- `get(id, path) -> Option<JsonValue>`
- `get_versioned(id, path) -> Option<JsonVersionedValue>`
- `get_at(id, path, timestamp) -> Option<JsonValue>`
- `get_at_version(id, path, version) -> Option<JsonValue>`
- `get_versions(id) -> Option<JsonHistory>`
- `exists(id) -> bool`
- `delete(id, path) -> JsonDeleteOutcome`
- `delete_document(id) -> JsonDeleteOutcome`
- `batch_set_or_create(entries) -> JsonBatchSetOutcome`
- `batch_get(entries) -> Vec<Option<JsonVersionedValue>>`
- `batch_delete(entries) -> JsonBatchDeleteOutcome`
- `list(prefix, cursor, limit) -> JsonListPage`
- `list_at(prefix, cursor, limit, timestamp) -> JsonListPage`
- `count(prefix) -> u64`
- `sample(prefix, count) -> JsonSample`
- `create_index(name, field_path, index_type) -> JsonIndexDefinition`
- `drop_index(name) -> bool`
- `list_indexes() -> Vec<JsonIndexDefinition>`

## Storage Shape

Add a JSON row class with a stable storage-space id. Use the same stable
row-key envelope as KV, with a JSON discriminator:

```text
version byte | json discriminator | space length | space bytes | document id bytes
```

Add separate internal key helpers for index metadata and index entries. Index
metadata must not appear in normal document list/count/sample results. Internal
index rows can either use a separate row class or a reserved JSON-internal space,
but the choice must be documented and guarded.

## Document Encoding

Use a versioned document envelope instead of storing raw JSON values directly:

- format version
- document id
- document version
- updated timestamp
- JSON value

The old engine used a versioned MessagePack envelope and retained a legacy
fallback. The rebuilt engine has no legacy JSON documents, so it can choose a
clean format. The format must be deterministic, explicit, and covered by
fixtures. Do not make the executor aware of the encoding.

## Implementation Order

### 1. JSON Type Skeleton

- Add `data/json` modules: `types`, `outcome`, `service`.
- Add `api/json.rs` and crate-root re-exports.
- Reuse `ProductSpace` or move it to a shared product type module if JSON and
  KV both need it.
- Add `JsonDocumentId` validation aligned with old key validation.
- Add `JsonValue` validation for document size, nesting depth, and array size.
- Add `JsonPath` parsing and validation.

### 2. Persistence Row Class And Key Encoding

- Add `RowClass::Json`.
- Add JSON row-key encoding/decoding helpers.
- Add source guards that JSON service does not construct storage requests
  directly.
- Add malformed-row decode tests matching the KV key decode tests.

### 3. Document Envelope

- Add `JsonDocument` internal struct.
- Add encode/decode helpers with version fixtures.
- Ensure decode failures map to engine corruption/data-loss.
- Add tests for unknown format version, truncated payload, mismatched document
  id, and invalid JSON payload.

### 4. Latest Reads And Basic Writes

- Implement `Database::json`.
- Implement `create`, `set_or_create`, `set`, `get`, `get_versioned`,
  `exists`, root delete, and path delete.
- Root `set_or_create` creates the full document if missing.
- Non-root `set` fails when the document is missing.
- Non-root `set_or_create` creates an object-root document when needed only if
  the path can be materialized unambiguously.
- Keep commit outcomes engine-owned.

### 5. Path Semantics

- Port or rewrite the old path behavior as a JSON-owned helper:
  - root path
  - object key traversal
  - array index traversal
  - set at existing path
  - create missing object fields where allowed
  - reject impossible array expansion unless explicitly supported
  - delete object field
  - delete array element
  - missing path maps to documented no-op or invalid-input behavior
- Add a compatibility table for old path strings such as `$`, `user.name`, and
  array paths.

### 6. Historical Reads

- Implement `get_at`, `get_at_version`, `get_versions`, and `list_at`.
- History is per document, newest-first, and includes document tombstones.
- Path selection happens after selecting the historical document version.
- Missing path returns `None`, not a storage error.

### 7. List, Count, And Sample

- Implement prefix list with cursor and limit.
- Implement timestamp list with the same cursor/limit semantics.
- Implement count by prefix.
- Implement sample by prefix using deterministic selection.
- Ensure internal index rows are excluded.

### 8. Batch APIs

- Implement `batch_set_or_create`.
- Implement `batch_get`.
- Implement `batch_delete`.
- Preserve positional results.
- Preserve one engine commit for valid write entries.
- Define duplicate-document/path behavior explicitly and test it.

### 9. Secondary Index Metadata

- Implement `JsonIndexType`.
- Implement `JsonIndexDefinition`.
- Implement `create_index`, `drop_index`, and `list_indexes`.
- Maintain index entries on create, set, path delete, root delete, and batch
  writes/deletes.
- Keep index metadata hidden from normal document APIs.
- Do not implement JSON search in this slice.

### 10. Executor Command Contract

- Add JSON command variants to `executor-next::Command`.
- Add JSON output variants to `executor-next::Output`.
- Add JSON batch entry/result wire types.
- Implement executor dispatch through `Database::json`.
- Keep item-level validation behavior at the executor boundary.
- Ensure convenience helpers, if added, call `execute(Command::...)`.

### 11. Source Guards

- Engine public JSON APIs must not expose storage request/outcome types.
- JSON service must not import executor command/output types.
- Executor must not import storage crates.
- Benchmarks and smoke loaders must use engine or executor public APIs.
- Test names and comments must not use planning-slice labels.

## Non-Goals

- Search command execution.
- Vector embedding hooks for JSON content.
- Merge, clone, diff, restore, and advanced branch workflows.
- Multi-command transaction sessions.
- Python, Node, MCP, or CLI bindings.
- Lower-layer benchmark bypasses.

## Exit Gates

- Engine JSON API compiles with all features.
- JSON follows the same module and ownership structure as KV.
- Cache and durable-local JSON behavior suites pass.
- Durable reopen preserves JSON documents, history, deletes, and index metadata.
- Executor JSON command variants round-trip through serde JSON.
- Every executor JSON command delegates through engine APIs only.
- Source guards prove no storage types leak above engine persistence.
