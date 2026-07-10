# V1 Response Contract Completion Implementation Plan

## Status

Status: proposed final response-contract implementation plan.

This plan covers the remaining V1 response-quality work after structured
errors, mutation effects, commit receipts, JSON missing/null responses, and
batch item error status were introduced.

Related documents:

1. `docs/architecture/strata-sdk-quality-playbook.md`
2. `docs/architecture/v1-error-and-diagnostics-contract.md`
3. `docs/architecture/v1-response-contract-completion-plan.md`
4. `docs/architecture/v1-response-quality-readiness-assessment.md`
5. `docs/architecture/implementation-plans/v1-response-error-contract-implementation-plan.md`
6. `docs/architecture/implementation-plans/v1-success-response-contract-implementation-plan.md`
7. `docs/architecture/v1-public-output-inventory.md`

## Goal

Freeze the response model that SDKs, CLI, MCP tools, and AI agents consume.

After this work, a caller should be able to answer these questions without
command-specific inference:

1. Did the operation apply?
2. What commit was produced?
3. Was returned data found?
4. Is there another page?
5. What happened to each batch item?
6. Did the command fail, is it retryable, and what should the user do next?

## Non-Goals

1. Reworking storage-next into an SDK-facing response layer.
2. Replacing engine primitive services with generic dynamic dispatch.
3. Adding a query DSL.
4. Changing primitive semantics while normalizing response shape.
5. Preserving ambiguous pre-V1 wire shapes for compatibility.

## Target Shared Concepts

### Page

Every paginated response should expose one shape:

```text
Page<T, Cursor> {
  items: Vec<T>,
  has_more: bool,
  cursor: Option<Cursor>
}
```

Rules:

1. `items` is always present.
2. `has_more=false` means the page is terminal.
3. `has_more=true` requires `cursor`.
4. Cursors are opaque to SDK callers.
5. Empty non-terminal pages are invalid unless a primitive explicitly documents
   a filtered pagination reason; V1 should avoid that case.

### BatchResult

Every batch response should expose one wrapper shape:

```text
BatchResult<T> {
  mode: atomic | itemwise,
  status: ok | partial | error,
  applied: bool,
  commit: Option<CommitReceipt>,
  items: Vec<BatchItem<T>>
}
```

Rules:

1. `mode=atomic` means the command either commits all valid operations or fails
   as a top-level error.
2. `mode=itemwise` means valid items may succeed while invalid items return
   item-level errors.
3. `status=ok` means all items completed without item-level errors. An all
   no-op or all-miss batch is still `ok` when each item was valid and
   intentionally reported `applied=false`.
4. `status=partial` means at least one item succeeded and at least one item
   failed, missed, or did not apply.
5. `status=error` means every item failed with an item-level error.
6. `commit` is present when the batch produced one shared commit.
7. Item-level commit facts remain present when they are easier for SDKs to use.

### BatchItem

Every batch item should expose one shape:

```text
BatchItem<T> {
  index: u64,
  status: ok | error,
  applied: bool,
  effect: Option<MutationEffect>,
  commit: Option<CommitReceipt>,
  result: Option<T>,
  error: Option<ErrorStatus>
}
```

Rules:

1. `index` is the original input position.
2. `status=ok` requires `error=None`.
3. `status=error` requires `error`.
4. Successful no-op and missing-target items are `status=ok`,
   `applied=false`, and use `MutationEffect`.
5. Failed validation items are `status=error` and do not expose effect/commit.

### Maybe

Every optional read should expose one concept:

```text
Maybe<T> {
  found: bool,
  value: Option<T>,
  version: Option<u64>,
  timestamp: Option<u64>
}
```

Rules:

1. Missing data is `found=false`.
2. Present JSON `null` is `found=true, value=null`.
3. Present empty bytes, empty arrays, empty objects, and empty strings are still
   `found=true`.
4. Version and timestamp are present when the primitive has a visible row.
5. Existing primitive-specific read facts may remain in the payload, but SDKs
   should expose one `.found` pattern.

### ErrorCodeRegistry

Every public `ErrorStatus.code` should have a registry entry:

```text
ErrorCodeEntry {
  code: String,
  class: ErrorClass,
  retry: RetryPolicy,
  commit_outcome: CommitOutcome,
  message_template: String,
  suggested_fix: String,
  docs_slug: String,
  details_schema: Option<SchemaRef>
}
```

Rules:

1. No public code is emitted unless it exists in the registry.
2. Registry defaults must match runtime-rendered status facts.
3. Docs URLs must resolve to stable generated or hand-written pages.
4. Details schemas should be stable enough for automation.

## Layer Responsibilities

### Storage-Next

Storage remains mechanical. It should provide commit, version, cursor, and
error-causality facts. It should not know about SDK `Page`, `Maybe`,
`BatchResult`, or docs URLs.

### Engine-Next

Engine owns primitive meaning. It should continue returning typed outcomes, but
those outcomes must expose enough facts for executor to construct shared V1
responses without guessing.

Engine should add facts only when executor cannot infer them reliably:

1. authoritative found/missing facts;
2. authoritative batch mode and item application facts;
3. page continuation facts;
4. stable product error status;
5. primitive diagnostics.

### Executor-Next

Executor owns the public response shape until the IDL layer exists. It should
normalize current command outputs into the shared concepts and keep serde
fixtures stable.

### IDL, SDK, CLI

The IDL should define the shared concepts once. SDKs should expose idiomatic
helpers. The CLI should render the same concepts in human and JSON modes.

## Implementation Order

### 1. Inventory Current Public Responses

Status: implemented in
`docs/architecture/v1-public-output-inventory.md`.

Create a generated or hand-maintained inventory of every `Output` variant and
whether it is:

1. mutation acknowledgement;
2. optional read;
3. page;
4. batch;
5. diagnostics;
6. admin/status;
7. import/export;
8. inference.

For each output, record:

1. current fields;
2. target shared concept;
3. whether the wire shape changes before V1;
4. whether SDK mapping alone is sufficient;
5. golden fixture path.

Exit criteria:

1. Every output variant is categorized.
2. Every variant has a target V1 model.
3. Deferred variants are explicitly marked non-V1 or internal.

### 2. Pagination Normalization

Status: implemented at the executor boundary.

Introduce a shared executor `PageInfo` or `Page<T>` model and migrate all page
responses to one continuation contract.

Scope:

1. KV key lists and scans where pagination applies.
2. JSON list/sample/index list pages where pagination applies.
3. Vector key lists and collection list pages where pagination applies.
4. Event range pages.
5. Graph list, node list, edge list, neighbor pages, and binding pages.
6. Branch, space, and admin list outputs where pagination exists or should
   exist.

Implementation steps:

1. Add shared `PageInfo { has_more, cursor }`.
2. Prefer `items` on the V1 wire shape.
3. For existing variants that currently expose `keys`, `events`, `nodes`, or
   `graphs`, decide whether to rename to `items` now or map in the IDL.
4. Make terminal page behavior consistent.
5. Reject or prevent `has_more=true` with `cursor=None`.
6. Keep cursor strings opaque and primitive-scoped.
7. Document cursor stability and invalidation rules.

Exit criteria:

1. SDK pagination helpers do not inspect command names. Complete for
   executor-next page outputs.
2. Every page has `items`, `has_more`, and `cursor` in the V1 model. Complete
   through `items` plus flattened `PageInfo`.
3. Empty terminal pages are consistent across primitives. Complete: terminal
   pages use `has_more=false, cursor=null`.
4. Golden snapshots cover first, continued, and terminal pages. Complete for
   the shared page shape in
   `crates/executor-next/tests/fixtures/responses/v1/pages/`.

Completed implementation notes:

1. Added shared `PageInfo<Cursor> { has_more, cursor }`.
2. Re-exported `PageInfo` from executor-next because it is now part of the
   public response contract.
3. Flattened `PageInfo` into page output variants so public JSON has
   `items`, `has_more`, and `cursor` at the page payload level.
4. Replaced page item fields named `keys`, `events`, `graphs`, `nodes`,
   `neighbors`, and `bindings` with `items` on the V1 wire shape.
5. Converted finite lists, scans, samples, and catalog lists to terminal pages
   where no cursor exists yet.
6. Deserialization rejects impossible page facts:
   `has_more=true, cursor=null` and `has_more=false, cursor!=null`.

Cursor stability and invalidation rules:

1. Cursors are opaque values scoped to the command, primitive, branch, space,
   and filter arguments that produced them.
2. Callers must not reuse a cursor across a different command, branch, space,
   primitive, prefix, filter, graph, collection, event type, or timestamp.
3. Cursors are stable for forward progress within a single logical listing,
   but they are not durable bookmarks across arbitrary writes unless the
   command documents snapshot or timestamp semantics.
4. Timestamped reads keep their cursor scoped to the same timestamp.
5. A stale or malformed cursor should fail as invalid input rather than being
   treated as a missing page.

### 3. Batch Wrapper Normalization

Status: implemented in `executor-next`.

Keep the item-level improvements already made, then normalize outer batch
wrappers.

Scope:

1. KV batch put/delete/get.
2. JSON batch set/delete/get.
3. Vector batch upsert/delete/get.
4. Event batch append.
5. Graph batch write.
6. Arrow import paths that call batch commands.

Implementation steps:

1. Add `BatchMode`, `BatchStatus`, `BatchResult<T>`, and `BatchItem<T>`.
2. Define current command modes:
   - KV batch put/delete: itemwise validation with shared commit for valid
     items; duplicate valid keys remain top-level errors.
   - JSON batch set/delete: itemwise validation with shared commit for valid
     items.
   - Vector batch upsert/delete: itemwise response over a shared commit.
   - Event batch append: itemwise validation with shared commit for valid
     items.
   - Graph batch write: atomic command; invalid graph operation is top-level
     failure.
3. Add batch-level `mode`, `status`, `applied`, and optional `commit`.
4. Add item `index` and explicit item `status`.
5. Preserve existing primitive-specific item result payloads under `result`.
6. Keep item-level `error: null` on success.
7. Keep top-level errors for invalid batch structure or unsafe atomic
   semantics.

Exit criteria:

1. Every batch response answers whether the batch is atomic or itemwise.
2. Every item has stable position and status.
3. SDKs can use one batch helper across primitives.
4. Arrow import/export code does not depend on old primitive batch wrappers.

Implementation notes:

1. `BatchMode`, `BatchStatus`, `BatchItemStatus`, `BatchItem<T>`, and
   `BatchResult<T>` live in `crates/executor-next/src/types.rs`.
2. KV, JSON, vector, and event batches are `mode=itemwise`.
3. Graph batch write is `mode=atomic`; invalid graph operations remain
   top-level failures.
4. `Output` batch variants now carry `BatchResult<T>`.
5. Existing primitive item DTOs are preserved under `items[].result` so V1 SDKs
   can migrate to shared fields without losing primitive-specific facts.
6. Arrow import/export uses `BatchResult.items()` and shared item facts instead
   of matching old primitive `Vec<T>` wrappers.

### 4. Optional Read Normalization

Status: implemented at the executor boundary for V1 wire ambiguity.

Normalize found/missing semantics without losing primitive-specific read facts.

Scope:

1. KV get and batch get.
2. JSON get, get-at, batch get, and path reads.
3. Vector get, get-at, history optional responses, and batch get.
4. Event get optional responses.
5. Graph get-meta, get-node, get-edge, and binding reads.
6. Admin reads that can return optional facts.

Implementation decision:

1. Keep JSON explicit maybe wrappers on the wire because JSON `null` is a real
   value.
2. For KV/vector/event/graph, prefer IDL-level `Maybe<T>` mapping unless the
   current wire shape is ambiguous or hard for non-generated clients.
3. If a primitive can return a valid `null`-like value, use a wire-level
   `Maybe<T>`.
4. If the primitive cannot return null and the wire shape is stable, SDK
   mapping is acceptable for V1 if golden fixtures prove it.

Exit criteria:

1. Missing and present are unambiguous in generated SDKs.
2. JSON stored null and missing are distinct on the wire.
3. Batch get item results expose found/missing consistently.
4. Optional reads preserve version/timestamp when available.

Implementation notes:

1. JSON top-level reads continue using `MaybeJsonValue` and
   `MaybeJsonVersionedValue` on the wire so stored `null` and missing remain
   distinct for generated and non-generated clients.
2. KV, vector, event, graph, and admin top-level optional reads keep their
   existing `Option<T>` wire shape for V1 because those value domains do not
   use JSON `null` as a stored value. The V1 IDL maps them to shared
   `Maybe<T>` accessors.
3. KV and vector batch get item payloads now include an explicit `found` field
   because itemwise batch responses are consumed directly by plain JSON
   clients and should not require `value != null` inference.
4. KV batch get preserves `version` and `timestamp` on found values, including
   present empty byte values.
5. Vector batch get preserves `VectorVersionedData` on found values, including
   version, timestamp, vector revision, vector payload, and metadata.
6. Optional-read golden fixtures cover JSON missing/null, KV missing,
   vector missing, and event missing examples. Broader fixture expansion is
   handled by the golden snapshot slice.

### 5. Golden Response Snapshots

Status: implemented for the executor-next V1 representative fixture matrix.

Add fixture-backed public JSON snapshots for the V1 response contract.

Implementation steps:

1. Create `crates/executor-next/tests/fixtures/responses/v1/`.
2. Add a fixture loader that serializes representative outputs and compares
   them to checked-in JSON.
3. Keep fixtures pretty-printed and stable.
4. Include success, no-op, miss, page, batch, diagnostics, and failure cases.
5. Include cache and durable commit receipt examples when durability differs.
6. Keep test data deterministic; avoid wall-clock timestamps except when
   captured as fixed engine test timestamps.
7. Add a review rule: public response shape changes require fixture updates.

Exit criteria:

1. Every public response family has at least one golden fixture.
2. Every shared concept has direct fixtures.
3. CI fails on accidental public JSON drift.
4. Fixture names map cleanly to IDL model names.

Implementation notes:

1. `crates/executor-next/tests/command_contract.rs` contains a shared fixture
   loader that serializes public values and compares them against checked-in
   JSON under `crates/executor-next/tests/fixtures/responses/v1/`.
2. Shared-concept fixtures cover `CommitReceipt` in durable and cache forms,
   `MutationEffect`, JSON `Maybe`, `PageInfo`, `BatchResult`, and
   `ErrorStatus`.
3. Public response family fixtures cover admin, spaces, branches, KV, JSON,
   vector, vector-index diagnostics, event, graph, Arrow import/export,
   status helpers, and inference text output.
4. Fixture tests parse every checked-in response fixture and require
   newline-terminated, indented JSON.
5. Public response shape changes must update the relevant fixture in the same
   change and explain whether the change is intended V1 contract movement or a
   bug fix.

### 6. Error Code Registry And Docs

Status: implemented for engine-next and executor-next.

Move stable public error codes from convention to registry.

Implementation steps:

1. Create a registry source file for all public error codes emitted by
   engine-next and executor-next.
2. Include class, retry policy, commit outcome, suggested fix, docs slug, and
   details schema.
3. Add a runtime or test-only lookup API.
4. Make executor error rendering use registry defaults unless an error carries
   a more specific override.
5. Generate docs stubs from the registry.
6. Add a guard test that every emitted public code exists in the registry.
7. Add a guard test that every registry docs URL has a target.

Exit criteria:

1. No unregistered public error code can be emitted.
2. Retry policies are reviewed per code.
3. Suggested fixes are useful and not generic.
4. Docs URLs are stable.
5. Support can search by reference ID and code.

Implementation notes:

1. Engine-owned codes are registered in
   `crates/engine-next/src/diagnostics/registry.rs` with public class, retry
   policy, commit outcome, suggested fix, docs slug, and details schema.
2. Executor-owned and inference-boundary codes are registered in
   `crates/executor-next/src/error_registry.rs`; the executor registry delegates
   to the engine registry for `.engine.` codes.
3. Executor error rendering resolves every emitted code through the registry.
   Unknown executor-rendered codes are converted to
   `internal.executor.unregistered_code` with the original code attached as a
   structured detail.
4. Public docs URLs resolve to
   `https://strata.dev/docs/errors/registry#<code>` and the hand-written docs
   target lives at `docs/errors/registry.md`.
5. Guard tests prove source-emitted executor and inference codes are
   registered, registry metadata is complete, and docs URL targets exist.

### 7. IDL, SDK, CLI Conformance

Define the V1 IDL and prove downstream ergonomics.

Implementation steps:

1. Create an IDL inventory from executor commands, outputs, and error status.
2. Define shared models once: `CommitReceipt`, `MutationEffect`, `Maybe`,
   `Page`, `BatchResult`, `BatchItem`, `ErrorStatus`, diagnostics envelopes.
3. Map every command to one output model.
4. Generate or hand-maintain Python, TypeScript, Rust, and Go response models.
5. Add SDK conformance tests that answer:
   - applied?
   - committed?
   - found?
   - next page?
   - item failed?
   - retryable?
6. Add CLI JSON rendering tests that use the same fixture expectations.
7. Add CLI human rendering rules for common success and failure states.
8. Add MCP/agent examples that consume the structured response without
   command-specific parsing.

Exit criteria:

1. SDKs expose one ergonomic pattern for mutation, optional reads, pages,
   batches, and errors.
2. CLI JSON output matches golden fixtures.
3. CLI human output remains readable without hiding structured facts.
4. MCP tools can use the IDL response model directly.

## Rollout Strategy

1. Complete executor-next response normalization before creating downstream
   package APIs.
2. Keep old Rust constructors only as internal compatibility helpers when
   useful.
3. Do not freeze generated SDKs until golden fixtures and registry checks pass.
4. Treat public response JSON changes as breaking after V1 freeze.

## Definition Of Done

The V1 response contract is complete when:

1. every page maps to `Page<T>`;
2. every batch maps to `BatchResult<T>`;
3. every optional read maps to `Maybe<T>`;
4. every public response family has golden fixtures;
5. every public error code is registered and documented;
6. every SDK can answer applied/committed/found/paginated/failed/retryable with
   shared helpers;
7. CLI JSON output uses the same response model;
8. no response leaks storage row keys, system-space keys, artifact paths, or
   lower-layer implementation details.
