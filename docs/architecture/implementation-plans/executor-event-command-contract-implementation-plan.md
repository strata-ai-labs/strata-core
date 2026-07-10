# Executor Event Command Contract Implementation Plan

## Problem

The executor crate is the serialized command boundary for SDKs, MCP servers,
CLIs, IPC clients, and smoke tools. Event commands should use the same command
dispatch architecture restored for KV, JSON, and vector: clients send a
serialized `Command`, executor applies command-boundary defaults and wire
conversion, engine performs product semantics, and executor returns a
serialized `Output`.

The old executor exposed the useful event command set, but it also shaped event
reads through generic optional-value outputs and performed some validation and
side effects in the handler. The rebuilt executor should preserve command names
and field names where possible while keeping event behavior inside engine.

## Old Evidence

- `crates/executor/src/command.rs`
- `crates/executor/src/output.rs`
- `crates/executor/src/types.rs`
- `crates/executor/src/executor.rs`
- `crates/executor/src/handlers/event.rs`
- `crates/executor/src/session.rs`
- `crates/executor/src/handlers/export.rs`
- `crates/engine/src/primitives/event.rs`
- `crates/engine/src/semantics/event.rs`

## Current Targets

- `crates/executor-next/src/command.rs`
- `crates/executor-next/src/output.rs`
- `crates/executor-next/src/types.rs`
- `crates/executor-next/src/executor.rs`
- `crates/executor-next/tests/`
- `crates/engine-next/src/api/event.rs`
- `crates/engine-next/src/data/event/`

## Required Engine Surface

Do not implement executor event dispatch until the engine event API exists. The
executor implementation depends on these engine methods from
`engine-event-parity-implementation-plan.md`:

- `append`
- `batch_append`
- `get`
- `get_at`
- `exists`
- `len`
- `len_at`
- `get_by_type`
- `get_by_type_at`
- `range`
- `range_by_time`
- `list_types`
- `list_types_at`
- `list`
- `verify_chain`

## Design Decisions

1. **Serialized command remains the public executor path.** Rust convenience
   methods for event must build and execute `Command::Event...` variants.

2. **Executor is a stateless delegator.** It may deserialize command payloads,
   default branch/space, validate public request shape, convert wire types,
   map errors, and shape outputs. It must not allocate event sequences, compute
   hashes, maintain type summaries, scan event storage rows, or repair indexes.

3. **Engine owns event semantics.** Object-only payload validation, event type
   validation, timestamp visibility, global sequence allocation, batch append,
   hash-chain integrity, type filtering, branch isolation, and space isolation
   stay in engine.

4. **Keep old command names and field names.** Preserve the existing serialized
   command names and fields for append, batch append, get, exists, type query,
   length, sequence range, timestamp range, type listing, and event listing.

5. **Use event-specific output variants.** Do not bring back generic
   `MaybeVersioned` or generic heterogeneous value list outputs as the rebuilt
   architecture. Event reads, event lists, event ranges, batch append results,
   and chain verification should have event-specific output shapes.

6. **Shared primitive-neutral outputs may remain shared.** `Bool` can represent
   `EventExists`. If the executor keeps a shared numeric output, `EventLen` may
   map to it, but an event-specific `EventLength` output is preferred because it
   serializes named facts.

7. **Branch and space defaults match other primitives.** Omitted branch
   resolves to the executor handle default branch. Omitted space resolves to
   `"default"`.

8. **Batch append preserves positional reporting.** The command returns one
   item result per input entry. Invalid entries report item errors and do not
   consume sequence numbers. Valid entries are appended in input order through
   one engine batch operation.

9. **Timestamp values stay microsecond integers on the wire.** Executor converts
   `as_of`, `start_ts`, and `end_ts` into the engine timestamp type. The wire
   contract keeps microseconds since epoch.

10. **Search and shadow embedding are out of this slice.** The old event
    handler could feed shadow embedding and search indexing. This executor
    event slice must not depend on embed runtime, search indexes, or export
    hooks.

11. **Chain verification is a first-class diagnostic command.** The old engine
    had hash-chain verification. The rebuilt executor should expose
    `EventVerifyChain` so SDKs and operational tools can verify event
    integrity without reaching below the engine API.

## Public Event Command Set

Add these command variants:

| Command | Inputs | Output |
| --- | --- | --- |
| `EventBatchAppend` | branch?, space?, entries | `EventBatchAppendResults` |
| `EventAppend` | branch?, space?, event_type, payload | `EventAppendResult` |
| `EventGet` | branch?, space?, sequence, as_of? | `EventRecord` |
| `EventExists` | branch?, space?, sequence | `Bool` |
| `EventGetByType` | branch?, space?, event_type, limit?, after_sequence?, as_of? | `EventRecords` |
| `EventLen` | branch?, space?, as_of? | `EventLength` |
| `EventRange` | branch?, space?, start_seq, end_seq?, limit?, direction, event_type? | `EventRangeResult` |
| `EventRangeByTime` | branch?, space?, start_ts, end_ts?, limit?, direction, event_type? | `EventRangeResult` |
| `EventListTypes` | branch?, space?, as_of? | `EventTypeList` |
| `EventList` | branch?, space?, event_type?, limit?, as_of? | `EventRecords` |
| `EventVerifyChain` | branch?, space? | `EventChainVerification` |

Preserve old field names where they exist: `branch`, `space`, `entries`,
`event_type`, `payload`, `sequence`, `as_of`, `limit`, `after_sequence`,
`start_seq`, `end_seq`, `start_ts`, `end_ts`, and `direction`.

Use `forward` and `reverse` scan direction values if the rebuilt executor
already has a primitive-neutral direction type. Otherwise add
`EventRangeDirection`.

## Wire Types

Add serializable request types:

- `BatchEventEntry`
  - event_type
  - payload
- `EventRangeDirection`
  - forward
  - reverse

Add serializable output helper types:

- `EventData`
  - sequence
  - event_type
  - payload
  - timestamp
  - previous_hash
  - hash
- `EventVersionedData`
  - event
  - version
  - timestamp
- `EventBatchAppendItemResult`
  - sequence
  - event_type
  - version
  - timestamp
  - error
- `EventChainVerification`
  - is_valid
  - length
  - first_invalid
  - error

Hash bytes should serialize as a stable hex string or a stable byte array.
Prefer hex if the current executor wire style already uses strings for opaque
binary facts.

## Output Variants

Add event-specific output variants:

- `EventAppendResult { sequence, event_type, version, timestamp }`
- `EventRecord(Option<EventVersionedData>)`
- `EventRecords(Vec<EventVersionedData>)`
- `EventLength { count }`
- `EventTypeList(Vec<String>)`
- `EventRangeResult { events, has_more, cursor }`
- `EventBatchAppendResults(Vec<EventBatchAppendItemResult>)`
- `EventChainVerification(EventChainVerification)`

Shared variants may remain shared when primitive-neutral:

- `Bool`

Do not reuse JSON or KV value wrappers for event payloads. Event payload is part
of an event record with sequence, type, timestamp, and hash-chain facts.

## Implementation Order

### 1. Engine Event API Gate

- Ensure `Database::event` and the required service methods exist.
- Ensure engine event outcomes expose the facts needed by executor outputs.
- Ensure engine errors include stable categories for invalid event type,
  invalid payload, missing branch, corrupt event record, corrupt metadata,
  stale type index, and closed handle.

### 2. Wire Types

- Add `BatchEventEntry`, range direction, event data, versioned event data,
  batch append item result, and chain verification wire types to `types.rs`.
- Keep fields private with constructors/accessors if matching current executor
  style.
- Add conversion helpers from executor wire types to engine event types.
- Add conversion helpers from engine event outcomes to executor output helpers.

### 3. Command Variants

- Add every event command variant to `Command`.
- Add `Command::name()` coverage for every event command.
- Add branch/space default helper coverage for every event command.
- Preserve serde tagged command shape and `deny_unknown_fields`.
- Preserve old event command field names.

### 4. Output Variants

- Add event-specific output variants.
- Ensure every output variant serializes and deserializes through serde JSON.
- Ensure payloads serialize as normal JSON values.
- Ensure hash-chain fields serialize deterministically.

### 5. Dispatch Helpers

- Add `Executor::event_service(branch, space)`.
- Add event type and payload conversion helpers.
- Add timestamp conversion helper from microseconds.
- Add range direction conversion helper.
- Add event outcome conversion helpers.

### 6. Single Event Commands

- `EventAppend` delegates to engine `append`.
- `EventGet` delegates to latest `get` or timestamp `get_at`.
- `EventExists` delegates to `exists`.
- `EventLen` delegates to latest `len` or timestamp `len_at`.
- `EventVerifyChain` delegates to `verify_chain`.

### 7. Type And List Commands

- `EventGetByType` delegates to latest `get_by_type` or timestamp
  `get_by_type_at`.
- Apply `after_sequence` and `limit` through the engine API, not by fetching
  all rows in executor.
- `EventListTypes` delegates to latest `list_types` or timestamp
  `list_types_at`.
- `EventList` delegates to engine `list`.

### 8. Range Commands

- `EventRange` delegates to engine `range`.
- `EventRangeByTime` delegates to engine `range_by_time`.
- Convert engine continuation facts into the executor cursor field.
- Do not build cursors by inspecting event storage keys.

### 9. Batch Append Command

- Empty input returns an empty `EventBatchAppendResults`.
- Convert every item into engine event input.
- Prefer delegating full positional validation to engine `batch_append` if that
  API accepts raw request entries and returns per-item outcomes.
- If engine type wrappers require executor-side conversion first, build a valid
  subset and re-map engine outcomes back to original positions without
  consuming sequence numbers for invalid items.
- Do not loop over `EventAppend` for batch append.

### 10. Convenience Methods

- Add optional Rust convenience methods only after command dispatch works.
- Convenience methods must construct commands and call `execute`.
- Convenience methods must not call engine event service directly.

### 11. Source Guards

- Guard executor event sources against direct storage imports.
- Guard executor event sources against persistence adapter imports.
- Guard executor event sources against SHA-256/hash helper imports.
- Guard executor event sources against search, embed runtime, export, WAL,
  table, compaction, and lifecycle imports.
- Guard benchmarks and smoke tools against event lower-layer bypasses.

## Error Mapping

Map engine errors into stable executor errors:

- invalid event type -> invalid input
- invalid payload -> invalid input
- missing branch -> not found
- invalid space -> invalid input
- corrupt event row -> internal or data corruption, according to executor
  error taxonomy
- corrupt metadata -> internal or data corruption, according to executor error
  taxonomy
- stale type index with recoverable fallback -> no executor error
- closed handle -> failed precondition

Do not translate missing event sequence into an error. It should return
`EventRecord(None)` or `Bool(false)` depending on the command.

## Completion Criteria

- Every event command variant exists and round-trips through serde JSON.
- Event dispatch delegates to engine event APIs.
- Event outputs are event-specific and preserve sequence, type, payload,
  timestamp, version, and hash-chain facts where relevant.
- Batch append preserves positional outcomes and dense sequence assignment.
- Branch and space defaults match KV, JSON, and vector.
- Source guards prove executor does not own event product behavior.
- `cargo fmt`, focused executor tests, and focused clippy pass for touched
  crates.
