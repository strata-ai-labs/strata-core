# Engine Event Parity Test Plan

## Purpose

Prove that the rebuilt engine owns event semantics with the same structure used
for KV, JSON, and vector: public API types at the engine boundary, product
behavior in the event service, and persistence translation isolated below the
service. The event primitive must behave as a single append-only global log per
branch and space, with event type as a filter over global sequence numbers.

## Test Matrix

| Area | Cache | Durable Local |
| --- | --- | --- |
| Type validation | Required | Required |
| Row-key encoding and decoding | Required | Required |
| Event record envelope | Required | Required |
| Metadata envelope | Required | Required |
| Type index envelope | Required | Required |
| Append and batch append | Required | Required |
| Get, exists, and length | Required | Required |
| Type filtering | Required | Required |
| Sequence ranges | Required | Required |
| Timestamp ranges | Required | Required |
| Event listing and type listing | Required | Required |
| Historical reads | Required | Required |
| Hash-chain verification | Required | Required |
| Branch isolation and fork behavior | Required | Required |
| Space isolation | Required | Required |
| Reopen persistence | Not applicable | Required |
| Source and dependency guards | Required | Required |

## Unit Tests

### Event Type Validation

- Accept ordinary event types such as `user.created`.
- Accept event types at the maximum allowed byte length.
- Reject empty event types.
- Reject event types longer than 256 bytes.
- Document and test whitespace-only event type behavior.
- Preserve the exact event type string on append outcomes and reads.
- Error class is stable and event-owned.

### Event Payload Validation

- Accept an empty object.
- Accept nested objects with arrays and scalar values inside the object.
- Reject root string payloads.
- Reject root number payloads.
- Reject root boolean payloads.
- Reject root null payloads.
- Reject root array payloads.
- Reject NaN anywhere inside the object.
- Reject positive infinity anywhere inside the object.
- Reject negative infinity anywhere inside the object.
- Reject payloads above the configured value limit if the engine has one.
- Validation failure leaves the log unchanged.

### Hash Calculation

- First event uses an all-zero previous hash.
- Same sequence, type, payload, timestamp, and previous hash produce the same
  hash.
- Changing sequence changes the hash.
- Changing event type changes the hash.
- Changing payload changes the hash.
- Changing timestamp changes the hash.
- Changing previous hash changes the hash.
- Fixture hash remains stable across platforms.
- Unsupported or non-canonical payload values fail before hash calculation.

### Row-Key Encoding

- Encodes event record rows deterministically.
- Encodes metadata rows deterministically.
- Encodes type index rows deterministically.
- Sequence byte order sorts keys by sequence.
- Decodes event types containing separators.
- Decodes maximum-length event type fields.
- Rejects unknown key version.
- Rejects unknown discriminator.
- Rejects truncated length fields.
- Rejects truncated field bytes.
- Rejects invalid UTF-8 event type bytes.
- Does not decode KV, JSON, vector, or control rows as event rows.

### Event Record Envelope

- Encodes a fixture event deterministically.
- Decodes the fixture event.
- Preserves sequence, event type, payload, timestamp, previous hash, and hash.
- Rejects unknown format version.
- Rejects mismatched sequence between key and record.
- Rejects corrupt payload.
- Rejects truncated hash bytes.
- Rejects non-finite decoded payload values.
- Rejects an event whose stored hash does not match canonical recomputation
  when verification is requested.

### Metadata Envelope

- Encodes an empty metadata record deterministically.
- Encodes metadata after one event.
- Decodes stream summaries.
- Preserves next sequence, head hash, hash version, counts, first/last
  sequence, and first/last timestamp.
- Rejects unknown format version.
- Rejects unknown hash version.
- Rejects next sequence lower than a stream summary last sequence.
- Rejects malformed stream summary ranges.
- Rejects truncated payload.

### Type Index Envelope

- Encodes an index row for event type and sequence.
- Decodes event type and sequence.
- Preserves ordering by event type then sequence.
- Rejects malformed event type bytes.
- Rejects truncated sequence bytes.
- Missing matching event row follows the documented stale-index behavior.

### Outcome Types

- Append outcome exposes sequence, event type, commit version, and timestamp.
- Batch append outcome preserves input positions.
- Batch append item outcome exposes sequence for success and error for failure.
- Event read outcome exposes sequence, event type, payload, timestamp, commit
  version, previous hash, and hash.
- Range outcome exposes events, `has_more`, and next cursor or continuation
  facts.
- Length outcome exposes count.
- Type-list outcome exposes deterministic event type order.
- Chain verification exposes valid flag, length, first invalid sequence, and
  error message.
- Public outcome structs do not expose persistence or storage request types.

## Engine Behavior Tests

Run each behavior test against both cache and durable-local fixtures unless the
test is specifically about reopen.

### Append

- Append first event returns sequence zero.
- Append second event returns sequence one.
- Appended event is immediately readable.
- Append stores event type and payload exactly.
- Append returns commit version and timestamp.
- Second event previous hash equals first event hash.
- Metadata next sequence advances by one.
- Type summary count advances by one.
- Appending a different event type still uses the next global sequence.
- Appending in a missing branch fails.
- Appending in an invalid space fails.

### Batch Append

- Empty batch returns empty outcome and commits nothing.
- Batch append assigns dense sequences in input order.
- Batch append writes one contiguous hash chain.
- Batch append updates metadata once with the final head hash.
- Batch append updates type summaries for repeated and mixed event types.
- Batch append with invalid event type records an item error.
- Batch append with invalid payload records an item error.
- Batch append with mixed valid and invalid entries commits valid entries only.
- Invalid entries do not consume sequence numbers.
- All-invalid batch commits nothing.
- Durable reads after batch return all committed events.

### Get And Exists

- Get existing sequence returns the event.
- Get missing sequence returns `None`.
- Get before the first sequence returns `None` if represented by a valid input.
- Get after the latest sequence returns `None`.
- Exists returns true for existing sequence.
- Exists returns false for missing sequence.
- Get in an empty log returns `None`.
- Get in a missing branch fails.
- Get in a different space does not see events.

### Length

- Empty log length is zero.
- Length increases after each append.
- Length increases by valid entry count after batch append.
- Length is independent per space.
- Length is independent per branch after divergent appends.
- Length does not change after failed validation.

### Type Filtering

- Get by type returns only matching events.
- Get by type preserves ascending sequence order.
- Get by type applies `after_sequence` strictly.
- Get by type applies `limit`.
- Get by type with `limit == 0` returns empty.
- Get by type for missing type returns empty.
- Get by type still returns correct results if the service falls back to event
  row scanning.
- Type filtering never creates independent per-type sequence numbers.

### Sequence Ranges

- Forward range uses `[start_seq, end_seq)` semantics.
- Omitted end reads through the latest visible sequence.
- End beyond latest clamps to latest.
- Start equal to end returns empty.
- Start greater than end returns empty.
- Limit truncates results and sets continuation facts.
- `limit == 0` returns empty.
- Reverse range returns descending sequence order.
- Event type filter applies inside the range.
- Range over an empty log returns empty.
- Range does not skip sequence numbers when all events are present.

### Timestamp Ranges

- Forward timestamp range includes events at `start_ts`.
- Forward timestamp range includes events at `end_ts`.
- Omitted end reads through the latest visible event.
- Reverse timestamp range returns descending timestamp order with sequence tie
  stability.
- Event type filter applies inside the timestamp range.
- Limit truncates results and sets continuation facts.
- `limit == 0` returns empty.
- Range with no matching timestamps returns empty.
- Multiple events with the same timestamp remain deterministic by sequence.

### Event List

- List with no filter returns all events in sequence order.
- List with event type filter returns only matching events.
- List applies limit.
- List with `limit == 0` returns empty.
- List on empty log returns empty.
- List at timestamp suppresses later events.
- List at timestamp with type filter combines both filters.

### Type List

- Empty log returns an empty type list.
- One event returns its type.
- Repeated event type appears once.
- Multiple event types are returned in deterministic order.
- Type list after timestamp includes only types introduced at or before the
  timestamp.
- Type list is independent per branch and space.

### Historical Reads

- `get_at` before an event timestamp returns `None`.
- `get_at` at the event timestamp returns the event.
- `get_at` after the event timestamp returns the event.
- `len_at` before first append is zero.
- `len_at` between appends returns the count visible at that time.
- `get_by_type_at` suppresses future matching events.
- `range_by_time` and `list_at` agree on visible event sets.
- Historical behavior remains correct when derived from immutable event rows,
  not only from metadata snapshots.

### Hash-Chain Verification

- Empty log verifies as valid with length zero.
- Single-event log verifies as valid.
- Multi-event log verifies as valid.
- Missing event row reports invalid with the missing sequence.
- Sequence mismatch in record reports invalid.
- Previous-hash mismatch reports invalid.
- Hash mismatch reports invalid.
- Metadata length greater than dense rows reports invalid.
- Verification in a forked branch validates the fork-visible chain.

### Branch Isolation

- Create two branches from the default branch.
- Append in one branch is not visible in the other branch.
- Fork after source events inherits source-visible events.
- Append in fork continues from the inherited latest sequence.
- Append in fork does not mutate source branch length or head hash.
- Append in source after fork does not appear in fork reads unless branch
  semantics explicitly merge later.
- Type list and ranges remain branch-local after divergence.

### Space Isolation

- Append the same event type in two spaces.
- Each space starts at sequence zero.
- Length is independent per space.
- Hash chain is independent per space.
- Type list is independent per space.
- Range in one space does not include events from another space.

### Durable Reopen

- Append events, close, reopen, and read them back.
- Reopened length matches pre-close length.
- Reopened type list matches pre-close type list.
- Reopened chain verification passes.
- Append after reopen continues at the next sequence.
- Append after reopen links to the pre-close head hash.
- Batch append after reopen preserves dense sequence order.

### Error Mapping

- Invalid event type maps to stable invalid-input error.
- Invalid payload maps to stable invalid-input error.
- Corrupt event record maps to stable corruption/data-loss error.
- Corrupt metadata maps to stable corruption/data-loss error.
- Missing branch maps to stable not-found or branch error.
- Internal stale type index behavior is deterministic and documented.

## Source And Dependency Guard Tests

- Public event API files do not expose persistence or storage request types.
- Event service uses the persistence adapter rather than direct lower-layer
  storage APIs.
- Event tests do not mention milestone names or implementation-phase labels.
- Benchmark code does not call event internals.
- Executor event command implementation remains absent from this engine slice.
- Existing executor crates do not depend on `data/event` internals.

## Regression Fixtures

Add fixture tests for:

- canonical first-event hash
- canonical second-event hash linked to the first
- event record byte envelope
- metadata byte envelope after zero, one, and three events
- type index key bytes for one event type and sequence
- branch fork with inherited head and one fork-local append
- durable reopen followed by append

## Completion Criteria

- All event engine unit tests pass in cache and durable-local modes.
- All event behavior tests pass in cache and durable-local modes.
- Reopen tests pass for durable-local mode.
- Source and dependency guards pass.
- Existing KV, JSON, and vector focused tests still pass.
- `cargo fmt`, focused engine tests, and focused clippy pass for touched crates.
