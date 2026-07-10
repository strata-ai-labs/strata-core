# Engine Event Parity Implementation Plan

## Problem

The rebuilt engine has branch, KV, JSON, and vector services following a common
shape: public engine API types, product-owned behavior, persistence translation
below the service, and executor commands as a later serialized boundary. Event
needs the same treatment.

The old event primitive is not a collection of independent streams. It is a
single append-only log per branch and space. Event type is a filter over that
global sequence. The important parity points are ordered sequence allocation,
object-only payload validation, hash-chain integrity, type filtering, sequence
and time ranges, batch append, branch/space isolation, and durable reopen
behavior.

The rebuilt implementation should preserve those product semantics without
copying incidental old storage mechanics. In particular, historical reads
should be provable from immutable event rows, while metadata remains an
append-coordination and summary fact.

## Old Evidence

- `crates/engine/src/primitives/event.rs`
- `crates/engine/src/semantics/event.rs`
- `crates/engine/src/primitives/extensions.rs`
- `crates/engine/src/primitives/branch/handle.rs`
- `crates/executor/src/command.rs`
- `crates/executor/src/handlers/event.rs`
- `crates/executor/src/handlers/export.rs`
- `crates/executor/src/output.rs`
- `crates/executor/src/types.rs`
- `crates/storage/src/key_encoding.rs`
- `crates/storage/src/compaction.rs`

## Current Targets

- `crates/engine-next/src/api/`
- `crates/engine-next/src/data/event/`
- `crates/engine-next/src/persistence/`
- `crates/engine-next/tests/`
- later executor slice:
  - `crates/executor-next/src/`
  - `crates/executor-next/tests/`

## Current Status

Already present in the rebuilt engine:

- cache and durable-local database open
- branch create/list/lookup/delete/fork
- KV, JSON, and vector public services
- branch/space validation patterns
- persistence adapter and row-class structure
- latest, timestamp, version, history, batch, and source-guard patterns

Missing for event:

- event public API module and crate-root re-exports
- event product type wrappers and validation
- event row class and key encoding
- event metadata row encoding
- event type index row encoding
- event record envelope with hash-chain facts
- append and batch append sequence allocation
- get, exists, length, list, type list, sequence range, timestamp range
- timestamp/as-of behavior
- hash-chain verification
- branch/space isolation behavior
- durable reopen behavior
- source/dependency guards

## Non-Goals

This slice must not implement:

- consumer groups, stream cursors with acknowledgements, subscriptions, or
  broker behavior
- event delete, event update, retention policies, or stream compaction
- event search, BM25, shadow embedding, or export integration
- executor command variants, unless the engine slice is complete and the user
  explicitly asks to continue into the executor slice
- benchmark-specific APIs or storage bypasses
- a new event stream model where each event type owns its own sequence

## Design Decisions

1. **Event follows the established primitive shape.** Public API types live
   under `api`; validated product types and behavior live under `data/event`;
   persistence translation lives under `persistence`; storage request types do
   not cross the public engine API.

2. **There is one global sequence per branch and space.** Event type is a
   filter over the log. Sequence numbers are dense, zero-based, and monotonic
   within a branch and space.

3. **Events are append-only.** There is no event update or delete operation in
   this parity slice. Event rows are immutable product facts and must remain
   readable for chain verification and historical queries.

4. **Payloads are object-only.** An event payload must be a JSON object and
   must not contain non-finite floats. Empty objects are valid.

5. **Event type validation is engine-owned.** Event type must not be empty after
   validation and must not exceed 256 bytes. The executor may mirror validation
   for earlier error messages later, but the engine remains authoritative.

6. **Hash-chain integrity is product behavior.** Each event stores sequence,
   event type, payload, timestamp, previous hash, and hash. The canonical hash
   is SHA-256 over the stable old-compatible fields and byte order.

7. **Metadata is not the only historical source of truth.** Latest metadata can
   store next sequence, head hash, and per-type summaries for fast append and
   fast latest queries. Historical APIs must remain correct even if they derive
   from immutable event rows instead of relying exclusively on retained
   metadata row versions.

8. **Type indexes are derived lookup aids with durable parity.** Type index
   rows accelerate type-filtered reads. Missing or stale derived index entries
   must never create incorrect event visibility. The service should be able to
   fall back to event-row scans for correctness.

9. **Batch append is one engine commit for valid rows.** The service should
   pre-validate entries, append all valid entries in order, write one metadata
   update, and return positional per-entry outcomes. Invalid entries report
   errors without consuming sequences.

10. **Branch and space are part of log identity.** A forked branch sees
    inherited event rows through branch visibility. Later appends on the fork
    are branch-local and continue from the fork-visible head. Separate spaces
    have independent sequences and hash chains.

11. **Timestamp APIs use event append timestamp.** Sequence ranges are ordered
    by sequence. Time ranges filter by the event timestamp and preserve stable
    ordering by sequence for ties.

12. **Executor remains a delegator.** The future executor event command slice
    should only deserialize commands, apply defaults, call engine APIs, and
    shape outputs. It must not allocate sequences, compute hashes, scan storage
    keys, or maintain type summaries.

## Shared Primitive Structure Target

Event should match the established primitive layout:

- `api/event.rs` re-exports public service and outcome types.
- `data/event/types.rs` owns validated event type, payload, range, cursor, and
  hash wrappers.
- `data/event/outcome.rs` owns append, batch append, read, range, length,
  type-list, and chain verification outcomes.
- `data/event/record.rs` owns internal event record and metadata envelopes.
- `data/event/hash.rs` owns canonical hash calculation and verification
  helpers.
- `data/event/service.rs` owns product operations, branch/space semantics,
  type filtering, ranges, and chain verification.
- `persistence/key.rs` owns event row-key encoding and decoding helpers.
- `persistence/row.rs` and `persistence/space.rs` own row-class assignment.
- Tests are grouped by public behavior, persistence translation, historical
  behavior, branch behavior, and source guards.

## Public Engine API Target

Add public event types:

- `EventType`
- `EventPayload`
- `EventSequence`
- `EventRecord`
- `EventVersionedRecord`
- `EventRangeDirection`
- `EventRangePage`
- `EventTypeList`
- `EventAppendOutcome`
- `EventBatchAppendEntry`
- `EventBatchAppendItemOutcome`
- `EventBatchAppendOutcome`
- `EventLength`
- `EventChainVerification`

Add `Database::event(branch, space) -> EngineResult<EventService>`.

Add `EventService` methods:

- `append(event_type, payload) -> EventAppendOutcome`
- `batch_append(entries) -> EventBatchAppendOutcome`
- `get(sequence) -> Option<EventVersionedRecord>`
- `get_at(sequence, timestamp) -> Option<EventVersionedRecord>`
- `exists(sequence) -> bool`
- `len() -> EventLength`
- `len_at(timestamp) -> EventLength`
- `get_by_type(event_type, after_sequence, limit) -> Vec<EventVersionedRecord>`
- `get_by_type_at(event_type, timestamp, after_sequence, limit) -> Vec<EventVersionedRecord>`
- `range(start_seq, end_seq, limit, direction, event_type) -> EventRangePage`
- `range_by_time(start_ts, end_ts, limit, direction, event_type) -> EventRangePage`
- `list_types() -> EventTypeList`
- `list_types_at(timestamp) -> EventTypeList`
- `list(event_type, limit, as_of) -> Vec<EventVersionedRecord>`
- `verify_chain() -> EventChainVerification`

Do not expose storage keys, row classes, storage read sets, or internal record
bytes through this API.

## Storage Shape

Add durable row families for event data:

```text
event record:
  version byte | event record discriminator | space length | space bytes |
  sequence big-endian bytes

event metadata:
  version byte | event metadata discriminator | space length | space bytes

event type index:
  version byte | event type index discriminator | space length | space bytes |
  event type length | event type bytes | sequence big-endian bytes
```

Use length-delimited fields for space and event type. Do not rely on textual
separators. Keep event sequence as big-endian bytes so key order matches
sequence order.

The event record row is the source of truth. The metadata row stores the latest
append head and summaries. The type index row is a derived lookup aid.

## Event Record Encoding

Use a versioned event record envelope:

- format version
- sequence
- event type
- payload
- timestamp
- previous hash
- hash

The encoding must be deterministic enough for fixture tests and hash-chain
replay. Hash calculation should use a canonical JSON representation that is
stable across platforms for supported `Value` variants.

## Metadata Encoding

Use a versioned event metadata envelope:

- format version
- next sequence
- head hash
- hash version
- per-event-type summary:
  - count
  - first sequence
  - last sequence
  - first timestamp
  - last timestamp

The metadata row can be reconstructed from event rows if missing or corrupt only
when an explicit recovery path is implemented. Normal operations should treat
corrupt metadata as data corruption and fail rather than silently forking the
hash chain.

## Implementation Order

### 1. Old Contract Audit

- Record old command variants, handler behavior, and primitive behavior.
- Confirm zero-based sequence semantics.
- Confirm object-only payload validation and 256-byte event type limit.
- Confirm timestamp range inclusivity.
- Confirm batch append invalid-item behavior.
- Confirm branch and space behavior from old tests.
- Decide whether event type list ordering should be sorted in the rebuilt API.
  Prefer deterministic sorted output even if old metadata iteration was not
  stable.

### 2. Event Type Skeleton

- Add `data/event` modules: `types`, `outcome`, `record`, `hash`, `service`.
- Add `api/event.rs` and crate-root re-exports.
- Add `EventType` validation.
- Add `EventPayload` validation.
- Add sequence and timestamp range wrappers if useful.
- Add public outcome structs with private fields and accessors.

### 3. Persistence Row Classes And Keys

- Add event row classes or event discriminators under the existing product row
  class scheme.
- Add event record key encoding/decoding.
- Add event metadata key encoding/decoding.
- Add event type index key encoding/decoding.
- Add malformed-key tests matching the KV/JSON/vector decode style.
- Add guards that event service does not build lower-layer requests directly.

### 4. Record, Metadata, And Hash Envelopes

- Add event record encode/decode helpers.
- Add metadata encode/decode helpers.
- Add canonical hash helper.
- Add chain verification helper over visible event rows.
- Map unknown format versions, malformed payloads, mismatched sequence, and
  corrupt hashes to stable engine errors.

### 5. Append And Batch Append

- Implement append through one engine commit.
- Read latest metadata or default metadata for a new log.
- Allocate the current `next_sequence`.
- Compute timestamp, previous hash, and hash.
- Write the event record, type index row, and updated metadata row together.
- Ensure space registration follows the same pattern as other primitives.
- Implement batch append by validating all entries first, appending valid
  entries in input order, and writing one metadata update.
- Return positional per-entry outcomes without consuming sequence numbers for
  invalid entries.

### 6. Latest Reads And Type Queries

- Implement `get`, `exists`, and `len`.
- Implement `get_by_type` using type index rows when available.
- Keep a correctness fallback over event rows.
- Apply `after_sequence` and `limit` after type filtering.
- Ensure missing logs return empty results instead of errors.

### 7. Ranges And Historical Reads

- Implement sequence ranges with `[start_seq, end_seq)` semantics.
- Implement timestamp ranges with inclusive `[start_ts, end_ts]` semantics.
- Implement forward and reverse ordering.
- Implement `limit == 0` as an empty result.
- Implement `len_at`, `get_at`, `get_by_type_at`, `list_at`, and
  `list_types_at`.
- Prefer deriving historical results from immutable event rows at the requested
  timestamp. Do not make historical correctness depend only on metadata
  version-chain retention.

### 8. Branch, Space, And Reopen Behavior

- Ensure branch forks inherit visible event rows and metadata.
- Ensure branch-local appends continue from the fork-visible head and do not
  mutate the parent branch.
- Ensure independent spaces have independent sequences and hash chains.
- Ensure durable reopen reads metadata and existing rows correctly.
- Ensure appending after durable reopen continues the sequence and hash chain.

### 9. Public API Integration

- Add `Database::event`.
- Add API re-exports.
- Update crate-root exports.
- Add source guards for public API boundaries.
- Do not update executor commands in this engine slice.

## Edge Cases

- Empty log length is zero.
- First append sequence is zero and previous hash is all zeros.
- Missing sequence returns `None`.
- Range with `start_seq >= end_seq` returns empty.
- Range end beyond latest clamps to latest.
- Reverse range starts at the highest visible sequence below end.
- Timestamp range with no matching timestamps returns empty.
- Multiple events with the same timestamp remain ordered by sequence.
- Type filter with no matching rows returns empty.
- Type index row without matching event row is ignored or reported as
  corruption according to a documented rule.
- Metadata next sequence lower than visible event rows is corruption.
- Metadata next sequence higher than visible dense rows fails chain
  verification.
- Batch append with all invalid entries returns item errors and commits
  nothing.
- Batch append with mixed valid and invalid entries commits only valid entries
  in input order.
- Payload arrays, scalars, and non-finite floats fail validation.
- Event type containing only whitespace follows the documented validation rule.
- Event type longer than 256 bytes fails validation.

## Source And Dependency Guards

- Event public API must not expose persistence or storage request types.
- Event service must use the persistence adapter, not direct storage APIs.
- Executor crates must not depend on event internals when the executor event
  slice is implemented.
- Benchmarks must use normal engine APIs.
- Existing search/export code should not be pulled into this slice.

## Completion Criteria

- Event engine API exists and follows the KV/JSON/vector structure.
- Append, batch append, reads, ranges, type filters, length, type listing, and
  chain verification work in cache and durable-local modes.
- Branch forks and spaces are isolated.
- Durable reopen continues sequence and hash-chain state.
- Historical APIs are correct from immutable event rows.
- Source/dependency guards pass.
- `cargo fmt`, focused engine tests, and focused clippy pass for touched crates.
