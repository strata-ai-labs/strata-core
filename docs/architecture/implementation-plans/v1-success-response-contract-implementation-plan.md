# V1 Success Response Contract Implementation Plan

## Status

Active follow-on to
`docs/architecture/implementation-plans/v1-response-error-contract-implementation-plan.md`.

The failure path now has the core V1 facts needed for Stripe-grade SDK, CLI,
MCP, and agent responses:

1. stable public class and code;
2. retry policy;
3. commit outcome;
4. message;
5. suggested fix;
6. docs URL;
7. reference ID;
8. optional trace ID;
9. structured details and hints;
10. source-chain preservation for diagnostics.

The remaining response-quality gap is the higher-level success contract.
Executor success outputs are typed and serde-tested, and mutation outputs now
expose shared commit/effect facts across rebuilt primitives. Downstream SDKs can
consume those mutation facts directly, but they still need normalized optional
read, pagination, shared batch wrapper, and golden fixture coverage before the
IDL can freeze.

This plan closes that gap before freezing the V1 IDL.

## Problem

Strata should give the caller a high-quality answer every time:

1. If an operation succeeds, the caller should know exactly what happened.
2. If an operation did not mutate anything, the caller should know whether that
   was an intentional no-op or a missing target.
3. If an operation committed, the caller should know the commit version,
   timestamp, durability fact, and affected-row counts where available.
4. If an operation returns a page, the caller should see one consistent page
   contract.
5. If an operation returns optional data, missing must not be confused with a
   valid `null` value.
6. If an operation runs a batch, every item should have a consistent positional
   status and a normalized success or error payload.

Today the engine and storage have most of the underlying facts. The executor
does not yet present them through one public response vocabulary.

## Quality Bar

Use the same bar as the error contract and SDK playbook:

1. **Predictable for AI agents.** An agent should not need command-specific
   guesswork to determine whether a write applied, whether a read found data, or
   how to continue pagination.
2. **Idiomatic for SDKs.** Generated SDKs should expose common concepts once:
   `CommitReceipt`, `Page`, `Maybe`, `BatchItem`, and primitive-specific result
   models.
3. **Stable on the wire.** Public JSON shapes must be golden-tested before IDL
   freeze.
4. **No lower-layer leakage.** Success responses must not expose storage row
   families, storage branch IDs, internal control keys, artifact paths, or other
   implementation details.
5. **No ambiguity.** Missing, no-op, empty collection, JSON null, and durable
   uncertainty must be distinct states.

## Current State

### Storage

Storage already returns the low-level facts that engine needs:

1. commit version;
2. commit timestamp;
3. durability summary;
4. read row version and timestamp;
5. range/page cursors;
6. branch fork version and timestamp;
7. explicit error classifications for ambiguous durable commits.

Storage should remain mechanical. It should not grow SDK-facing success
vocabulary.

### Engine

Engine has product-level facts in several places:

1. `CommitOutcome` carries version, timestamp, put count, delete count, and
   durable.
2. Database open and close outcomes carry durable/open/close facts.
3. Space outcomes carry optional commit facts for create/delete/no-op paths.
4. Primitive write outcomes generally expose version and timestamp.
5. Read models carry version/timestamp facts where the primitive supports
   versioned reads.

Engine gaps:

1. Some primitive outcomes expose commit facts directly instead of via one
   shared product type.
2. Some no-op paths expose only optional version/timestamp, not a normalized
   mutation effect.
3. Durability is present in `CommitOutcome` but is not consistently surfaced
   through all engine-facing outcome accessors used by executor.
4. Batch item outcomes are not normalized across primitives.

### Executor

Executor has a stable tagged `Output` enum and command/output serde tests.

Strengths:

1. Every output variant round-trips through serde.
2. KV, JSON, vector, event, graph, and space mutation outputs expose shared
   `commit` and `effect` facts where applicable.
3. KV, JSON, vector, event, and graph batch mutation item results expose shared
   `commit` and `effect` facts where applicable.
4. Page-like outputs usually have `has_more` and `cursor`.
5. Batch outputs are positional.
6. Vector indexed queries include planner diagnostics.

Executor gaps:

1. Optional reads are represented with `Option<T>`, which is not always safe on
   the wire. In particular, `JsonValue(Option<Value>)` serializes a missing
   value and a stored JSON `null` as the same JSON `null`.
2. Pagination fields use consistent names but not one public model.
3. Batch wrapper shapes differ across KV, JSON, vector, event, and graph.
4. Success outputs have round-trip tests, but not golden JSON snapshots for
   every command family.
5. Future admin mutations must join the same `commit` and `effect` contract;
   the current admin surface is read/status oriented.

## Target Public Concepts

The V1 IDL should define these concepts once and reuse them everywhere.

### CommitReceipt

Represents a successful committed mutation.

```text
CommitReceipt {
  version: u64,
  timestamp: u64,
  durable: bool,
  put_count: u64,
  delete_count: u64,
}
```

Rules:

1. Present only when a mutation committed.
2. Absent for read-only operations.
3. Absent for no-op mutations that did not commit.
4. `durable` means the backing storage reported durable commit semantics for
   this write.
5. Counts are product-level affected row facts. They are not storage row family
   names and must remain safe to show publicly.

### MutationEffect

Represents what the successful command did at product level.

```text
MutationEffect {
  applied: bool,
  kind: "created" | "updated" | "deleted" | "unchanged" | "not_found",
  matched: bool,
  affected_count: u64,
}
```

Rules:

1. `applied=false` means no commit was attempted or needed.
2. `matched=false` distinguishes missing target from no-op update.
3. `affected_count` is a product count, such as visible keys deleted, events
   appended, vectors updated, or graph facts changed.
4. Primitive-specific flags such as `created`, `deleted`, and `updated` may
   remain as convenience fields, but they must be derivable from this shared
   effect.

### Maybe<T>

Represents an optional read result without null ambiguity.

```text
Maybe<T> {
  found: bool,
  value: optional<T>,
  version: optional<u64>,
  timestamp: optional<u64>,
}
```

Rules:

1. Missing data is `found=false`.
2. Stored JSON `null` is `found=true, value=null`.
3. Version and timestamp are present when the primitive can identify the
   visible row.
4. SDKs should expose idiomatic helpers, such as `.found`, `.value`, and
   `.unwrap_or_none()`.

### Page<T, Cursor>

Represents any paginated response.

```text
Page<T, Cursor> {
  items: list<T>,
  has_more: bool,
  cursor: optional<Cursor>,
}
```

Rules:

1. Field names are consistent in the IDL.
2. Executor JSON may retain domain aliases only if the IDL maps them
   unambiguously. Prefer normalizing to `items`.
3. Empty page with `has_more=false` is the terminal page.
4. `cursor=null` with `has_more=true` is invalid.

### BatchItem<T>

Represents one positional batch result.

```text
BatchItem<T> {
  index: u64,
  status: "ok" | "error",
  result: optional<T>,
  error: optional<ErrorStatus>,
}
```

Rules:

1. Every item has its original input index.
2. `status=ok` requires `result`.
3. `status=error` requires `error`.
4. Item errors use the same public `ErrorStatus` shape as top-level failures.
5. The batch response must state whether the command was atomic or item-wise.

### BatchResult<T>

Represents the full batch response.

```text
BatchResult<T> {
  mode: "atomic" | "itemwise",
  applied: bool,
  commit: optional<CommitReceipt>,
  items: list<BatchItem<T>>,
}
```

Rules:

1. Atomic batches either commit all valid mutations or fail before returning
   item successes.
2. Item-wise batches may return mixed success and error statuses.
3. If the batch committed as one transaction, `commit` appears at the batch
   level and each item may carry item-specific product facts.
4. If each item commits independently, each item carries its own commit.

## Target Response Families

### Read Responses

Use `Maybe<T>` for point reads where data may be absent:

1. KV get;
2. KV get versioned;
3. JSON get;
4. JSON get versioned;
5. vector get;
6. graph get node/edge/info;
7. event get by sequence.

Use `Page<T, Cursor>` for paginated reads:

1. KV list page;
2. JSON list;
3. vector list keys;
4. event range;
5. graph names;
6. graph nodes;
7. graph neighbors;
8. graph bindings.

Use explicit count/list/search summaries for aggregate reads, but they should
still reuse `Page` and common `total_count` naming where appropriate.

### Write Responses

Every successful mutation response should include:

1. resource identity;
2. `effect`;
3. `commit` when applied;
4. primitive-specific facts.

Examples:

```text
KvPutResult {
  key,
  effect,
  commit,
}

JsonSetResult {
  document,
  path,
  effect,
  commit,
  document_version,
}

VectorUpsertResult {
  collection,
  key,
  effect,
  commit,
  vector_revision,
}

EventAppendResult {
  sequence,
  event_type,
  effect,
  commit,
}

GraphNodeWriteResult {
  graph,
  node_id,
  effect,
  commit,
}
```

No-op mutation responses should include `effect.applied=false` and omit
`commit`.

### Lifecycle Responses

Database and admin responses should remain compact but should use consistent
facts:

1. open target;
2. created;
3. durable;
4. close durable synced;
5. idempotent close;
6. engine version;
7. enabled capabilities.

These are already close to target.

### Inference Responses

Inference success responses may remain provider-shaped where the provider
domain requires it, but they should still use common response facts when
available:

1. model;
2. provider;
3. usage/token counts;
4. cache hit/miss when applicable;
5. item-wise success/error for embedding and ranking batches.

Do not force inference token streams into the database mutation vocabulary.

## Implementation Plan

### 1. Response Inventory

Create an inventory table covering every `Command` and `Output` variant.

For each command record:

1. read/write/admin/inference/arrow classification;
2. atomicity;
3. whether it can no-op;
4. whether it returns optional data;
5. whether it returns a page;
6. whether it returns batch items;
7. whether commit facts are available;
8. whether durability facts are available;
9. proposed V1 response model.

Exit criteria:

1. Every command is classified.
2. Every current output variant has a target response family.
3. JSON null versus missing ambiguity is explicitly resolved.

### 2. Shared Response Types

Add executor-facing shared types:

1. `CommitReceipt`;
2. `MutationEffect`;
3. `Maybe<T>` or concrete non-generic equivalents if serde/IDL tooling needs
   named structs;
4. page metadata or concrete page types;
5. `BatchItem<T>` or concrete per-primitive item wrappers;
6. `BatchResult<T>`.

Keep type names stable and IDL-friendly.

Exit criteria:

1. Shared response types serialize deterministically.
2. Shared response types have constructors/accessors.
3. Tests cover null, missing, no-op, applied, and page terminal states.

### 3. Engine Commit Receipt Plumbing

Make engine outcomes expose enough product facts for executor to fill
`CommitReceipt`.

Work items:

1. Add a shared engine-side commit receipt adapter over `CommitOutcome`.
2. Ensure all primitive write outcomes preserve durable, put-count, and
   delete-count facts where meaningful.
3. Ensure no-op outcomes explicitly distinguish no commit from committed
   zero-row changes.
4. Keep storage durability vocabulary inside persistence; expose only product
   `durable: bool`.

Exit criteria:

1. Executor can build `CommitReceipt` without reaching into storage.
2. Durable and cache writes both produce correct `durable` facts.
3. Existing behavior tests still pass.

### 4. Normalize Point Reads

Replace ambiguous optional read wire shapes with explicit found wrappers.

Priority:

1. JSON get and JSON versioned get, because JSON null is currently ambiguous.
2. KV get and versioned get.
3. Vector get.
4. Graph get node/edge/info.
5. Event sequence get.

Exit criteria:

1. Missing JSON and stored JSON null have different serialized forms.
2. SDK generators can represent optional reads uniformly.
3. Golden JSON snapshots lock the shape.

### 5. Normalize Write Acknowledgements

Update write outputs to include `effect` and `commit`.

Priority:

1. KV put/delete;
2. JSON set/delete/index mutations;
3. vector collection/write/delete/metadata/index mutations;
4. event append;
5. graph create/write/delete;
6. space and branch mutations.

Exit criteria:

1. Every applied mutation carries a `commit`.
2. Every no-op mutation carries `effect.applied=false` and no `commit`.
3. Durability is visible for writes without exposing storage internals.
4. Existing primitive-specific facts are preserved.

### 6. Normalize Pagination

Standardize page response concepts.

Options:

1. **Wire normalization:** change executor JSON to use `items`, `has_more`,
   `cursor` everywhere.
2. **IDL normalization:** keep domain-specific field names on the wire but map
   them to a shared `Page<T, Cursor>` concept in generated SDKs.

Recommendation: prefer wire normalization before V1 freeze. It is simpler for
agents, MCP, and non-generated integrations.

Exit criteria:

1. Every page has consistent terminal-page semantics.
2. `has_more=true` always includes a non-empty cursor.
3. Golden snapshots cover one empty, one terminal, and one continued page per
   primitive.

### 7. Normalize Batch Results

Status: partially implemented. Executor-next batch item failures now serialize
`ErrorStatus`, successful batch items serialize `error: null`, KV/JSON/event
item validation preserves stable engine codes, and KV/JSON/vector/event/graph
mutation item successes expose commit/effect facts where applicable. Batch
wrappers and explicit item status fields remain primitive-specific.

Define batch mode and item status consistently.

Work items:

1. Identify which existing batches are atomic versus item-wise.
2. Add item `index`.
3. Use `status=ok/error`.
4. Use `ErrorStatus` for item failures.
5. Use batch-level `commit` for single-transaction batches.
6. Preserve positional behavior in SDKs.

Exit criteria:

1. Every batch item can be handled by the same SDK helper pattern.
2. Atomic batches never report partial committed success.
3. Item-wise batches can report mixed outcomes safely.
4. Golden snapshots cover all batch families.

### 8. Golden Public JSON Snapshots

Status: implemented for executor-next representative V1 response families in
`crates/executor-next/tests/fixtures/responses/v1/`.

Add fixture-driven golden tests for public success responses.

Snapshot families:

1. admin and spaces;
2. branch lifecycle;
3. KV read/write/list/batch;
4. JSON null/missing/write/list/batch/index;
5. vector write/search/index/list/batch;
6. event append/range/batch/verification;
7. graph node/edge/binding/page/batch;
8. Arrow import/export;
9. inference model/generate/embed/rank/cache.

Exit criteria:

1. Every public response family has at least one golden JSON fixture.
2. Fixtures are stable under `cargo test`.
3. Unknown fields remain rejected for commands where applicable.
4. The IDL can be generated from the same source concepts.

### 9. IDL Readiness Pass

After response normalization, create the IDL inventory for response models.

Work items:

1. Map every command to input and output type.
2. Map every output to shared concepts and primitive-specific types.
3. Map every error code to public status facts.
4. Mark streaming and long-running operations separately.
5. Add SDK naming overrides where generic output names would feel awkward.

Exit criteria:

1. No output type requires command-specific inference to understand basic
   success state.
2. Every output type has a stable JSON fixture.
3. Every output type has an SDK-friendly model name.

## Test Plan

### Unit Tests

1. `CommitReceipt` serialization and accessors.
2. `MutationEffect` applied/no-op/missing/affected-count cases.
3. `Maybe<JsonValue>` missing versus stored null.
4. Page terminal and continued invariants.
5. Batch item ok/error invariants.

### Engine Tests

1. Commit outcome durable facts in cache and durable modes.
2. No-op writes omit commit facts.
3. Applied writes include commit facts and affected counts.
4. Branch, space, and primitive write outcomes expose enough data for executor
   response construction.

### Executor Tests

1. Every command output round-trips.
2. Golden JSON snapshots for all response families.
3. Optional read wrappers distinguish missing from null.
4. Batch results are positional and normalized.
5. Pagination semantics are consistent.
6. No success response leaks storage/internal/control-plane implementation
   details.

### SDK/IDL Tests

1. Generated model names are stable.
2. Common helpers exist for commit receipts, pages, optional reads, and batch
   items.
3. Python, TypeScript, Rust, and Go examples can handle equivalent responses
   using the same conceptual flow.

## Backward Compatibility

V1 is not frozen yet, so prefer fixing the wire shape now instead of preserving
ambiguous legacy output variants.

If compatibility is required during transition:

1. keep existing `Output` variants as internal compatibility constructors;
2. add V1 response variants in parallel;
3. mark old variants as deprecated in Rust docs;
4. make IDL and public docs use only the V1 response shapes.

Do not carry ambiguous JSON optional responses into the V1 IDL.

## Non-Goals

1. No new query language.
2. No search/indexing work.
3. No CLI command design.
4. No SDK generator implementation.
5. No storage API redesign.
6. No change to the V1 error status shape unless batch item errors uncover a
   concrete missing field.

## Exit Criteria

The response contract is ready for IDL freeze when:

1. all successful outputs use shared response concepts for commit, effect,
   optional read, page, and batch state;
2. missing and null are never ambiguous;
3. durable facts are visible for applied durable writes;
4. no-op and missing-target successes are explicit;
5. batch item successes and failures are normalized;
6. golden public JSON fixtures cover all command families;
7. engine, executor, and IDL response inventories agree;
8. no public success response leaks storage or internal control-plane details.
