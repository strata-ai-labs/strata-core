# Executor JSON Command Contract Implementation Plan

## Problem

The executor crate is the serialized command boundary for SDKs, MCP servers,
CLIs, IPC clients, and smoke tools. JSON commands should use the same command
dispatch architecture already restored for KV: clients send a serialized
`Command`, executor applies command-boundary validation and defaults, engine
performs product semantics, and executor returns a serialized `Output`.

The old executor exposed a complete JSON command set. The rebuilt executor
currently exposes branch and KV commands only. This plan restores JSON command
parity without letting executor recreate JSON document behavior.

## Old Evidence

- `crates/executor/src/command.rs`
- `crates/executor/src/output.rs`
- `crates/executor/src/types.rs`
- `crates/executor/src/executor.rs`
- `crates/executor/src/handlers/json.rs`
- `crates/engine/src/primitives/json/mod.rs`
- `crates/engine/src/primitives/json/index.rs`

## Current Targets

- `crates/executor-next/src/command.rs`
- `crates/executor-next/src/output.rs`
- `crates/executor-next/src/types.rs`
- `crates/executor-next/src/executor.rs`
- `crates/executor-next/tests/`
- `crates/engine-next/src/api/json.rs`
- `crates/engine-next/src/data/json/`

## Design Decisions

1. **Serialized command remains the only public executor path.** Any Rust
   convenience methods added for JSON must build and execute `Command` variants.

2. **Executor is a stateless delegator.** It may deserialize JSON command
   payloads, default branch/space, validate public request shape, map errors,
   and shape output. It must not parse or mutate JSON paths by hand after the
   engine JSON API exists.

3. **Engine owns JSON document semantics.** Path parsing, document mutation,
   timestamp visibility, history, list, sample, and index metadata stay in
   engine.

4. **JSON values are serialized values, not byte KV values.** Add JSON-specific
   wire types that wrap `serde_json::Value` where needed. Do not force JSON
   through `Bytes`.

5. **Use JSON-specific output variants.** Do not reintroduce old generic
   optional-value outputs as the architecture. Use variants that make the
   primitive explicit.

6. **Batch item validation stays positional.** Invalid item keys, paths, or
   values should produce positional item errors when valid entries can still be
   applied. Engine batch methods should receive only validated entries.

7. **Branch and space defaults match KV.** Omitted branch resolves to the
   executor handle default branch. Omitted space resolves to `"default"`.

8. **Index commands are JSON commands.** `JsonCreateIndex`, `JsonDropIndex`, and
   `JsonListIndexes` delegate to engine JSON APIs. They should not depend on
   search command implementation.

9. **Search remains separate.** The active old `JsonList` command uses prefix,
   cursor, limit, and optional timestamp. Structured search belongs to the
   separate search command set.

## Public JSON Command Set

Restore these command variants:

| Command | Inputs | Output |
| --- | --- | --- |
| `JsonSet` | branch?, space?, key, path, value | `WriteResult` |
| `JsonGet` | branch?, space?, key, path, as_of? | `JsonVersionedValue` or `JsonValue` |
| `JsonDelete` | branch?, space?, key, path | `DeleteResult` |
| `JsonGetv` | branch?, space?, key | `JsonVersionHistory` |
| `JsonExists` | branch?, space?, key | `Bool` |
| `JsonBatchSet` | branch?, space?, entries | `JsonBatchResults` |
| `JsonBatchGet` | branch?, space?, entries | `JsonBatchGetResults` |
| `JsonBatchDelete` | branch?, space?, entries | `JsonBatchResults` |
| `JsonList` | branch?, space?, prefix?, cursor?, limit, as_of? | `JsonListResult` |
| `JsonCount` | branch?, space?, prefix? | `Uint` |
| `JsonSample` | branch?, space?, prefix?, count? | `JsonSampleResult` |
| `JsonCreateIndex` | branch?, space?, name, field_path, index_type | `JsonIndexDefinition` |
| `JsonDropIndex` | branch?, space?, name | `Bool` |
| `JsonListIndexes` | branch?, space? | `JsonIndexList` |

## Wire Types

Add serializable request types:

- `BatchJsonEntry`
  - key
  - path
  - value
- `BatchJsonGetEntry`
  - key
  - path
- `BatchJsonDeleteEntry`
  - key
  - path
- `JsonIndexType`
  - numeric
  - tag
  - text

Add serializable output helper types:

- `JsonVersionedValue`
  - value
  - version
  - timestamp
  - document_version
- `JsonHistoryItem`
  - value
  - version
  - timestamp
  - document_version
  - tombstone
- `JsonBatchItemResult`
  - version
  - timestamp
  - document_version
  - error
- `JsonBatchGetItemResult`
  - value
  - version
  - timestamp
  - document_version
  - error
- `JsonSampleItem`
  - key
  - value
- `JsonIndexDefinition`
  - name
  - space
  - field_path
  - index_type

## Output Variants

Add JSON-specific output variants:

- `JsonValue(Option<serde_json::Value>)`
- `JsonVersionedValue(Option<JsonVersionedValue>)`
- `JsonVersionHistory(Option<Vec<JsonHistoryItem>>)`
- `JsonListResult { keys, has_more, cursor }`
- `JsonBatchResults(Vec<JsonBatchItemResult>)`
- `JsonBatchGetResults(Vec<JsonBatchGetItemResult>)`
- `JsonSampleResult { total_count, items }`
- `JsonIndexDefinition(JsonIndexDefinition)`
- `JsonIndexList(Vec<JsonIndexDefinition>)`

Shared variants already used by KV may remain shared where they are truly
primitive-neutral:

- `WriteResult`
- `DeleteResult`
- `Bool`
- `Uint`

## Implementation Order

### 1. Engine JSON API Gate

- Do not implement executor JSON dispatch until `Database::json` exists.
- Engine must expose JSON service methods for the command set.
- Engine methods must return engine-owned JSON outcomes.

### 2. Wire Types

- Add JSON batch entry types to `types.rs`.
- Add JSON output helper types to `types.rs`.
- Add `JsonIndexType` to the executor wire layer and conversion to engine
  `JsonIndexType`.
- Keep fields private where the current executor style uses constructors and
  accessors.

### 3. Command Variants

- Add every JSON command variant to `Command`.
- Add `Command::name()` coverage for every JSON command.
- Add branch/space default helper coverage for every JSON command.
- Preserve old field names: `key`, `path`, `value`, `entries`, `prefix`,
  `cursor`, `limit`, `as_of`, `name`, `field_path`, and `index_type`.

### 4. Output Variants

- Add JSON-specific output variants.
- Ensure every output variant serializes and deserializes through serde JSON.
- Keep JSON values as normal JSON values in the command payload, not base64
  bytes.

### 5. Dispatch Helpers

- Add `Executor::json_service(branch, space)`.
- Add `json_document_id`, `json_path`, `json_value`, and `json_index_name`
  conversion helpers.
- Convert timestamp micros into engine timestamp type.
- Convert engine outcomes into executor outputs.

### 6. Single JSON Commands

- `JsonSet` delegates to `set_or_create`.
- `JsonGet` delegates to `get_versioned` for latest reads and `get_at` for
  timestamp reads.
- `JsonDelete` delegates to root delete or path delete through engine.
- `JsonGetv` delegates to `get_versions`.
- `JsonExists` delegates to `exists`.

### 7. Batch JSON Commands

- Validate each item at the executor boundary.
- Build a valid-entry list preserving original positions.
- Delegate valid `JsonBatchSet` items to one engine batch set call.
- Delegate valid `JsonBatchGet` items to one engine batch get call.
- Delegate valid `JsonBatchDelete` items to one engine batch delete call.
- Fill positional item results from engine outcomes.
- Return empty batch outputs for empty input.

### 8. List, Count, And Sample Commands

- `JsonList` delegates to latest `list` or timestamp `list_at`.
- `JsonCount` delegates to `count`.
- `JsonSample` delegates to `sample`.
- Preserve old defaults for `JsonSample` count.
- Validate non-empty prefixes through the same document-id validation rules.

### 9. Index Commands

- `JsonCreateIndex` delegates to `create_index`.
- `JsonDropIndex` delegates to `drop_index`.
- `JsonListIndexes` delegates to `list_indexes`.
- Invalid index type maps to executor invalid-input.
- Output index definitions as structured JSON-specific output, not serialized
  strings.

### 10. Error Boundary

- Map engine invalid input, not found, conflict, unavailable, corruption, and
  closed-handle errors into executor error classes.
- Positional item errors must include stable messages and must not leak storage
  implementation details.
- Command-level errors must serialize through the existing executor error type.

### 11. Source Guards

- Executor crate must not depend on storage crates.
- Executor JSON dispatch must not mention storage row, storage commit, table,
  WAL, lifecycle, or compaction types.
- Executor JSON convenience helpers must call `execute(Command::...)`.
- Benchmarks and smoke loaders must not call engine persistence internals.

## Non-Goals

- Search command implementation.
- Embedding side effects.
- Multi-command transaction sessions.
- SDKs, CLI, MCP, or IPC servers.
- JSON merge/diff/restore workflows.
- Lower-layer benchmark bypasses.

## Exit Gates

- `strata-executor-next` compiles with JSON commands.
- Every JSON command round-trips through serde JSON.
- Every JSON output round-trips through serde JSON.
- Cache and durable-local executor handles pass the JSON command behavior suite.
- Source guards prove executor JSON commands do not depend on storage crates or
  engine persistence internals.
- JSON batch set can load a large dataset through executor commands without a
  lower-layer bypass.
