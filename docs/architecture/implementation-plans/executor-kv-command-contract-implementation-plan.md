# Executor KV Command Contract Implementation Plan

## Problem

The executor layer is the public serialized command boundary. Python SDKs, Node
SDKs, MCP servers, CLIs, IPC clients, and any future process boundary should all
talk to the same command/output contract instead of each binding rediscovering
engine semantics.

The old executor already had the right high-level shape:

```text
client binding -> serialized Command -> executor dispatch -> engine API -> storage
```

The rebuilt engine currently exposes only a narrow branch and byte-KV surface.
Before client work starts, the executor command contract needs to be restored as
a first-class crate boundary and all KV commands need to delegate through engine
APIs without storage bypasses.

## Evidence From The Old Stack

- `crates/executor/src/command.rs`
- `crates/executor/src/executor.rs`
- `crates/executor/src/handlers/kv.rs`
- `crates/executor/src/types.rs`
- `crates/executor/src/output.rs`
- `crates/executor/src/compat.rs`
- `crates/engine/src/primitives/kv.rs`

## New Targets

- `crates/executor-next/`
- `crates/engine-next/src/api/`
- `crates/engine-next/src/data/kv/`
- `crates/executor-next/tests/`

## Design Decisions

1. **Serialized command is the public executor boundary.** The executor crate
   owns the stable `Command` and `Output` vocabulary. SDKs, CLI, MCP, and IPC
   layers build on those types.

2. **Executor is a stateless delegator for normal commands.** It may deserialize,
   default branch/space, validate public request shape, map errors, and shape
   outputs. It must not encode storage keys, build storage commit plans, inspect
   lifecycle internals, or call storage directly.

3. **Engine owns product semantics and persistence semantics.** Snapshot
   visibility, version history, point-in-time reads, branch lookup, row layout,
   and commit behavior remain inside engine.

4. **KV is byte-first at this layer.** The rebuilt engine exposes byte keys and
   byte values. The executor command contract should preserve bytes exactly
   using serializable byte wrappers. Typed product values can be layered above
   the executor contract by SDKs or later product primitives.

5. **Command-to-output mapping is deterministic.** Every command variant maps to
   exactly one output variant. Tests must fail when a new command is added
   without explicit mapping coverage.

6. **Branch and space defaults match the old executor.** Omitted branch resolves
   to the executor handle default branch. Omitted space resolves to `"default"`.

7. **Batch commands are public API, not benchmark helpers.** Bulk loaders,
   SDKs, and tests use `KvBatchPut` rather than lower-layer bypasses.

8. **Error output is executor-shaped.** Public errors should expose stable
   executor error codes/classes and must not leak storage implementation names.

## Public KV Command Set

The executor crate must restore this complete KV command surface:

| Command | Inputs | Output |
| --- | --- | --- |
| `KvPut` | branch?, space?, key, value | `WriteResult` |
| `KvGet` | branch?, space?, key, as_of? | `KvVersionedValue` or `KvValue` |
| `KvDelete` | branch?, space?, key | `DeleteResult` |
| `KvList` | branch?, space?, prefix?, cursor?, limit?, as_of? | `Keys` or `KeysPage` |
| `KvScan` | branch?, space?, start?, limit? | `KvScanResult` |
| `KvBatchPut` | branch?, space?, entries | `BatchResults` |
| `KvBatchGet` | branch?, space?, keys | `BatchGetResults` |
| `KvBatchDelete` | branch?, space?, keys | `BatchResults` |
| `KvBatchExists` | branch?, space?, keys | `BoolList` |
| `KvExists` | branch?, space?, key | `Bool` |
| `KvGetv` | branch?, space?, key | `VersionHistory` |
| `KvCount` | branch?, space?, prefix? | `Uint` |
| `KvSample` | branch?, space?, prefix?, count? | `SampleResult` |

## Implementation Order

### 1. Crate Foundation

- Add `crates/executor-next` to the workspace.
- Add dependencies on `serde`, `serde_json`, `serde_bytes`, `thiserror`,
  `strata-core-next`, and `strata-engine-next`.
- Expose only executor API modules from the crate root:
  - `command`
  - `executor`
  - `output`
  - `types`
  - `error`
- Keep storage crates out of executor dependencies.

### 2. Wire Types

- Add serializable branch, space, key, and value request types.
- Represent KV key/value bytes with byte-preserving serde wrappers.
- Add `VersionedValue`, `BatchKvEntry`, `BatchItemResult`,
  `BatchGetItemResult`, and `SampleItem`.
- Add stable constructors/accessors so callers do not rely on tuple struct
  internals.

### 3. Command And Output Vocabulary

- Add a `Command` enum containing the full KV command set.
- Add an `Output` enum containing only the variants needed by branch + KV:
  `KvValue`, `KvVersionedValue`, `VersionHistory`, `Keys`, `KeysPage`,
  `WriteResult`, `DeleteResult`, `KvScanResult`, `BatchResults`,
  `BatchGetResults`, `Bool`, `BoolList`, `Uint`, and `SampleResult`.
- Add `Command::name()` as an exhaustive match.
- Add helper accessors for branch and space defaults.

### 4. Engine API Completion

Fill the engine API gaps required by the command set. All additions must stay
inside `strata-engine-next` and must not expose storage request types.

- Current reads:
  - `KvService::get`
  - `KvService::batch_get`
  - `KvService::exists`
  - `KvService::batch_exists`
- Range reads:
  - `KvService::list`
  - `KvService::list_page`
  - `KvService::scan`
  - `KvService::count`
  - `KvService::sample`
- Historical reads:
  - `KvService::get_at`
  - `KvService::list_at`
  - `KvService::get_versions`
- Writes:
  - preserve `put`, `put_batch`, `delete`, and `delete_batch`
  - define duplicate-key behavior explicitly for all batch write commands
  - return engine-owned delete existence facts from delete operations so
    executor delete outputs do not read before deleting
- Add public outcome structs for versioned values, scan rows, history rows, and
  pages, plus delete outcome structs for single and batch deletes.

### 5. Executor Handle

- Add `Executor` holding an engine database handle and default branch name.
- Add cache and durable-local open helpers that delegate to engine open APIs.
- Add `execute(command: Command) -> Result<Output>`.
- Resolve branch/space defaults before creating an engine KV service.
- Convert executor wire bytes into engine `KvKey` and `KvValue`.
- Convert engine outcomes into executor outputs.
- Preserve old executor batch shape: empty batch commands return empty outputs
  and invalid batch items produce positional item errors where the command can
  still apply valid items.

### 6. KV Command Delegation

Implement each command through the engine API:

- `KvPut` delegates to `put`.
- `KvGet` delegates to `get` or `get_at`.
- `KvDelete` delegates to `delete`.
- `KvList` delegates to `list`, `list_page`, or `list_at`.
- `KvScan` delegates to `scan`.
- `KvBatchPut` delegates to `put_batch`.
- `KvBatchGet` delegates to `batch_get`.
- `KvBatchDelete` delegates to `delete_batch`.
- `KvBatchExists` delegates to `batch_exists`.
- `KvExists` delegates to `exists`.
- `KvGetv` delegates to `get_versions`.
- `KvCount` delegates to `count`.
- `KvSample` delegates to `sample`.

### 7. Error Boundary

- Add `ExecutorError` with stable class/code/message fields.
- Map engine invalid input, not found, conflict, unavailable, corruption, and
  closed-handle errors into executor classes.
- Ensure serialized errors do not expose storage crate names or internal engine
  type names.

### 8. Compatibility Helpers

- Add a small Rust convenience facade over `Command` for the local CLI and
  smoke tests.
- Keep this facade as sugar over `execute(Command)`.
- Do not add a second semantic path.

### 9. Source Guards

- Executor crate must not depend on storage crates.
- Executor command handlers must not mention storage commit types or storage row
  types.
- Public command/output modules must not import engine storage adapter modules.
- Convenience helpers must call `execute(Command)` rather than direct engine KV
  methods.

## Non-Goals

- JSON, event, vector, search, graph, model, import/export, and maintenance
  commands.
- Stateful transaction command handles.
- SDK implementation.
- IPC server implementation.
- Typed product value semantics above byte KV.

## Exit Gates

- `strata-executor-next` compiles with all features.
- All KV commands round-trip through serde JSON.
- Every KV command delegates through engine APIs only.
- Cache and durable-local executor handles can run the same KV command suite.
- Batch put can write a large load through `execute(Command::KvBatchPut)` without
  benchmark-only bypasses.
- Source guards prove no executor dependency on storage crates.
