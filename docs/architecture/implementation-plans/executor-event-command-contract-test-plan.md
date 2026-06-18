# Executor Event Command Contract Test Plan

## Purpose

Prove that the executor crate exposes a stable serialized event command
boundary and remains a thin delegator over engine event APIs. Event command
tests should verify global sequence behavior, type filtering, range behavior,
as-of reads, batch append, branch/space defaults, and hash-chain diagnostics
without reimplementing event semantics in executor.

## Test Matrix

| Area | Cache | Durable Local |
| --- | --- | --- |
| Command serde round-trip | Required | Required |
| Output serde round-trip | Required | Required |
| Append and batch append | Required | Required |
| Get, exists, and length | Required | Required |
| Type filtering | Required | Required |
| Sequence ranges | Required | Required |
| Timestamp ranges | Required | Required |
| Event listing and type listing | Required | Required |
| Chain verification | Required | Required |
| Branch and space defaults | Required | Required |
| Branch and space isolation | Required | Required |
| Error mapping | Required | Required |
| Reopen persistence | Not applicable | Required |
| Source guards | Required | Required |

## Contract Tests

### Command JSON Round Trip

- Serialize and deserialize `EventAppend`.
- Serialize and deserialize `EventBatchAppend`.
- Serialize and deserialize `EventGet`.
- Serialize and deserialize `EventExists`.
- Serialize and deserialize `EventGetByType`.
- Serialize and deserialize `EventLen`.
- Serialize and deserialize `EventRange`.
- Serialize and deserialize `EventRangeByTime`.
- Serialize and deserialize `EventListTypes`.
- Serialize and deserialize `EventList`.
- Serialize and deserialize `EventVerifyChain`.
- Include omitted branch/space.
- Include explicit branch/space.
- Include object payloads with nested arrays and scalar fields.
- Include empty object payload.
- Include `as_of`, `limit`, `after_sequence`, range bounds, and both
  directions.
- Assert deserialized command equality.

### Output JSON Round Trip

- Serialize and deserialize `EventAppendResult`.
- Serialize and deserialize missing `EventRecord`.
- Serialize and deserialize present `EventRecord`.
- Serialize and deserialize `EventRecords`.
- Serialize and deserialize `EventLength`.
- Serialize and deserialize `EventTypeList`.
- Serialize and deserialize `EventRangeResult` with and without cursor.
- Serialize and deserialize `EventBatchAppendResults`.
- Serialize and deserialize valid `EventChainVerification`.
- Serialize and deserialize invalid `EventChainVerification`.
- Include previous hash and hash fields.
- Include payloads with nested JSON values.

### Command Name Coverage

- Assert `Command::name()` returns the stable name for every event command.
- The match must be exhaustive so adding a command without naming it fails
  compilation.

### Command-To-Output Mapping

- Execute each event command on a small cache database.
- Assert the output variant exactly matches the documented mapping.
- Latest `EventGet` returns `EventRecord`.
- Timestamp `EventGet` returns `EventRecord`.
- `EventGetByType` returns `EventRecords`.
- `EventList` returns `EventRecords`.
- `EventRange` and `EventRangeByTime` return `EventRangeResult`.
- `EventVerifyChain` returns `EventChainVerification`.

## Delegation Tests

### Executor Uses Engine APIs

- Source guard rejects storage crate imports in executor event sources.
- Source guard rejects storage row, storage commit, table, WAL, lifecycle, and
  compaction type names in executor event sources.
- Source guard rejects persistence adapter imports in executor event sources.
- Source guard rejects SHA-256 and event hash helper imports in executor event
  sources.
- Source guard rejects search, embed runtime, and export imports in executor
  event sources.

### Convenience Facade Uses Commands

- Any event convenience method must call `execute(Command::Event...)`.
- Convenience methods must not directly call engine event service methods.
- Convenience methods must not compute hashes, allocate sequences, apply type
  filters, scan rows, or maintain metadata.

### No Lower-Layer Bypass

- Event smoke loaders and benchmarks use executor event batch commands or
  public engine event APIs.
- Source guard rejects direct storage writes from those binaries.

## Behavior Tests

Run behavior tests in both cache and durable-local executor fixtures unless the
test specifically targets reopen.

### Append

- Execute `EventAppend` with an empty object payload.
- Assert `EventAppendResult.sequence == 0`.
- Assert returned event type matches input.
- Execute a second append.
- Assert second sequence is one.
- Read both events through `EventGet`.
- Assert payload, event type, sequence, timestamp, version, previous hash, and
  hash fields are present.
- Assert second previous hash equals first hash.

### Batch Append

- Execute `EventBatchAppend` with multiple valid entries.
- Assert positional result count equals input count.
- Assert sequences are dense and ordered by valid input position.
- Read all appended events back.
- Assert hash-chain linkage across the batch.
- Execute empty batch and assert an empty result list.
- Execute mixed valid and invalid entries.
- Assert invalid entries report errors.
- Assert invalid entries do not consume sequence numbers.
- Assert valid entries commit in input order.
- Assert command does not loop over single append by source guard or call count
  instrumentation if available.

### Get And Exists

- Get an existing sequence and assert present `EventRecord`.
- Get a missing sequence and assert `EventRecord(None)`.
- Exists returns true for existing sequence.
- Exists returns false for missing sequence.
- Get in an empty log returns `EventRecord(None)`.
- Missing branch maps to the documented executor error.

### Length

- `EventLen` on an empty log returns zero.
- Length increments after append.
- Length increments by valid batch append count.
- Length does not change after invalid append.
- Length is independent per branch.
- Length is independent per space.

### Type Filtering

- Append multiple event types into one log.
- Execute `EventGetByType` for one type.
- Assert only matching events are returned.
- Assert returned events keep global sequence numbers.
- Assert result order is ascending sequence order.
- Apply `after_sequence`.
- Apply `limit`.
- Apply `limit == 0`.
- Query a missing event type and assert empty results.

### Sequence Range

- Append several events.
- Execute forward `EventRange` with `[start_seq, end_seq)` bounds.
- Execute open-ended `EventRange`.
- Execute `EventRange` with end beyond latest and assert clamp behavior.
- Execute `EventRange` with start equal to end and assert empty results.
- Execute reverse `EventRange`.
- Apply event type filter inside the range.
- Apply limit and assert `has_more` and cursor facts.
- Execute with `limit == 0` and assert empty results.

### Timestamp Range

- Capture append timestamps from returned outputs or reads.
- Execute `EventRangeByTime` including start and end timestamps.
- Assert inclusive timestamp bounds.
- Execute reverse timestamp range.
- Assert deterministic order when timestamps tie.
- Apply event type filter.
- Apply limit and assert continuation facts.
- Execute with `limit == 0` and assert empty results.
- Execute range with no matching timestamps and assert empty results.

### Event List

- Execute `EventList` with no type filter.
- Assert all events are returned in sequence order.
- Execute `EventList` with event type filter.
- Assert limit behavior.
- Execute `EventList` with `as_of`.
- Assert events after `as_of` are suppressed.
- Execute `EventList` with type filter and `as_of`.

### Type List

- Empty log returns an empty type list.
- Repeated type appears once.
- Multiple types return deterministic ordering.
- `EventListTypes` with `as_of` includes only types introduced by that time.
- Type list is branch-local.
- Type list is space-local.

### As-Of Reads

- Append event A and capture timestamp.
- Append event B and capture timestamp.
- `EventGet` for B before B timestamp returns none.
- `EventGet` for B at or after B timestamp returns B.
- `EventLen` before A returns zero.
- `EventLen` between A and B returns one.
- `EventGetByType` with `as_of` suppresses future matching events.
- `EventList` with `as_of` agrees with length and point reads.

### Chain Verification

- Empty log verifies as valid with length zero.
- Log after append verifies as valid.
- Log after batch append verifies as valid.
- Verification output includes length.
- Invalid-chain cases can be covered in engine tests; executor test only needs
  to prove output mapping unless a safe corruption fixture exists.

### Branch And Space Defaults

- Omit branch and space and assert executor default branch and `"default"`
  space.
- Repeat with explicit branch and explicit space.
- Set the executor default branch and assert omitted branch uses it.
- Explicit branch overrides executor default branch.

### Branch Isolation

- Create a second branch through executor branch commands.
- Append events to both branches.
- Assert each branch starts from sequence zero if created independently.
- Fork after source events and assert inherited events are visible.
- Append in fork and assert sequence continues from inherited head.
- Assert source branch length and head remain unchanged after fork append.
- Type filters and ranges remain branch-local.

### Space Isolation

- Append the same event type in two spaces.
- Assert each space starts from sequence zero.
- Assert reads stay space-local.
- Assert length stays space-local.
- Assert type list stays space-local.
- Assert range results stay space-local.

## Durable Tests

### Durable Open/Reopen

- Open durable-local executor handle.
- Append several events.
- Batch append several events.
- Verify chain.
- Close and reopen.
- Read all events back.
- Assert length matches pre-close length.
- Assert type list matches pre-close type list.
- Verify chain after reopen.
- Append after reopen.
- Assert new sequence continues from previous length.
- Assert new previous hash links to the pre-close head.

## Error Contract Tests

### Invalid Event Type

- Empty event type maps to executor invalid-input.
- Event type longer than 256 bytes maps to executor invalid-input.
- Failed append leaves length unchanged.
- Failed batch item reports positional error when batch contract allows partial
  success.

### Invalid Payload

- Root string payload maps to invalid-input.
- Root number payload maps to invalid-input.
- Root array payload maps to invalid-input.
- Root null payload maps to invalid-input.
- Non-finite float inside object maps to invalid-input.
- Failed append leaves length unchanged.

### Invalid Ranges

- Start greater than end returns empty or invalid-input according to the engine
  contract and is documented.
- `limit == 0` returns empty.
- Invalid cursor format, if cursors become input later, maps to invalid-input.

### Corruption Mapping

- Corrupt event row maps to the documented executor corruption/internal error.
- Corrupt metadata maps to the documented executor corruption/internal error.
- Recoverable stale type index does not leak an executor error.

## Source Guard Tests

- Executor event files do not import storage crates.
- Executor event files do not import engine persistence modules.
- Executor event files do not import hash libraries.
- Executor event files do not import search, embedding, export, WAL, table,
  compaction, or lifecycle modules.
- Tests do not mention milestone names or implementation-phase labels.
- Command contract tests cover every event command variant.

## Completion Criteria

- All event command serde tests pass.
- All event output serde tests pass.
- All event behavior tests pass in cache and durable-local modes.
- Reopen tests pass for durable-local mode.
- Source guard tests pass.
- Existing KV, JSON, and vector executor focused tests still pass.
- `cargo fmt`, focused executor tests, and focused clippy pass for touched
  crates.
