# V1 Response Contract Completion Plan

## Status

Status: architecture follow-on plan

This document captures the remaining work needed before Strata can freeze the
V1 success and failure response contract for SDKs, CLI output, MCP tools, and
AI-agent use.

Related documents:

1. `docs/architecture/strata-sdk-quality-playbook.md`
2. `docs/architecture/v1-error-and-diagnostics-contract.md`
3. `docs/architecture/v1-response-quality-readiness-assessment.md`
4. `docs/architecture/implementation-plans/v1-response-error-contract-implementation-plan.md`
5. `docs/architecture/implementation-plans/v1-success-response-contract-implementation-plan.md`
6. `docs/architecture/implementation-plans/v1-response-contract-completion-implementation-plan.md`
7. `docs/architecture/implementation-plans/v1-response-contract-completion-test-plan.md`
8. `docs/architecture/v1-public-output-inventory.md`

## Bottom Line

Strata is not yet ready to freeze the V1 response contract.

The failure path is structurally close to the desired bar. Top-level executor
errors now expose stable classes, stable codes, retry policy, commit outcome,
message, suggested fix, docs URL, reference ID, optional trace ID, details, and
hints.

The success path is structurally close for mutation responses. KV, JSON,
vector, event, graph, and space mutations now expose shared commit and
mutation-effect facts while retaining primitive-specific convenience fields.
Batch item failures carry structured public error status, successful batch
items serialize an explicit `error: null`, and rebuilt primitive batch item
successes expose commit/effect facts where the item can mutate state. Page
outputs now expose shared `items`, `has_more`, and `cursor` facts, and batch
outputs now expose shared `BatchResult<T>` and `BatchItem<T>` wrappers.
Optional reads now have an explicit V1 mapping decision: JSON uses wire-level
maybe wrappers because `null` is a valid stored value; KV/vector batch get
items expose explicit `found` facts; non-JSON top-level optional reads remain
IDL-mapped `Maybe<T>` values. The remaining gaps are broader golden response
fixtures, error-code registry documentation, and SDK/IDL models that expose
these common facts without variant-specific parsing.

## Product Bar

The V1 response contract should let a caller answer these questions without
knowing which primitive produced the response.

For a success:

1. Did the operation apply a mutation?
2. Did the operation match an existing logical entity?
3. Was the operation a no-op or miss?
4. What commit was produced?
5. Is the commit durable?
6. What data was returned?
7. Was an optional read missing, or was the stored value actually null/empty?
8. Is there another page?
9. Which batch items applied, missed, failed, or were unchanged?
10. What primitive-specific diagnostics matter for this response?

For a failure:

1. What stable public class describes the failure?
2. What stable public code identifies it?
3. Is retrying the same request safe?
4. Could the mutation have committed?
5. What should the user do next?
6. Where are the docs?
7. What reference ID can support use to find the event?
8. What structured details can automation inspect?

## Layer Contract

### Storage-Next

Storage-next should stay mechanical.

It should provide:

1. commit version;
2. commit timestamp;
3. durability facts;
4. ambiguous commit facts on failure;
5. cursor and row-version facts;
6. enough structured error detail for engine-next to classify failures.

It should not provide:

1. SDK response vocabulary;
2. product mutation effects;
3. primitive-specific user guidance;
4. public error messages;
5. docs URLs.

Storage is not the main blocker for response quality.

### Engine-Next

Engine-next should own product meaning.

It should expose:

1. `CommitOutcome` or equivalent commit facts for every committed mutation;
2. applied/no-op/miss facts where the engine has authoritative knowledge;
3. versioned read facts;
4. page facts;
5. primitive diagnostics;
6. V1 error status facts after interpreting storage failures.

Current gap:

1. Engine outcomes remain primitive-specific, even when they carry the facts
   executor needs.
2. Batch item outcomes are still primitive-specific.
3. Pagination and optional-read facts are not represented through one shared
   engine-facing response model.

### Executor-Next

Executor-next should own the public command response shape until the IDL layer
exists.

It should expose:

1. shared mutation facts through `MutationEffect`;
2. shared commit facts through `CommitReceipt`;
3. shared optional-read facts for ambiguous value domains;
4. shared page facts;
5. shared batch item success and failure facts;
6. top-level and item-level errors through the same public status vocabulary.

Current gap:

1. Mutation outputs are aligned at the executor boundary for KV, JSON, vector,
   event, graph, and space.
2. Admin currently exposes read/status outputs rather than user mutations.
3. Batch wrappers use a shared executor `BatchResult<T>` model.
4. Page outputs use a shared executor `PageInfo` model.
5. Golden response snapshots now cover representative public response
   families and shared V1 concepts.

## Current Readiness Matrix

| Area | Status | Notes |
| --- | --- | --- |
| Top-level error status | Ready | Shape exists and is tested at executor boundary. |
| Error class/code propagation | Ready | Engine and executor preserve structured facts. |
| Retry policy | Ready | Public status exposes retry policy. |
| Commit outcome on failure | Ready | Public status can express maybe committed. |
| Docs URL/reference ID | Ready | Executor renders boundary-specific docs and references. |
| Error code registry | Partial | Codes exist, but registry/docs are not final. |
| KV write success | Ready for this slice | Uses `CommitReceipt` and `MutationEffect`. |
| JSON write success | Ready for this slice | Uses `CommitReceipt` and `MutationEffect`. |
| JSON missing/null reads | Ready | Uses explicit maybe wrappers. |
| Vector write success | Ready for this slice | Exposes `CommitReceipt` and `MutationEffect` plus vector revision facts. |
| Event write success | Ready for this slice | Append and batch append expose commit/effect facts. |
| Graph write success | Ready for this slice | Node, edge, delete, and batch write expose commit/effect facts. |
| Space/admin mutation success | Ready for this slice | Space create/delete expose effect and optional commit; admin has no user mutation output in this slice. |
| Batch item success | Partial | KV/JSON/vector/event/graph write items expose shared facts where applicable; wrappers remain primitive-specific. |
| Batch item failure | Ready for this slice | Batch item failures carry structured `ErrorStatus`; executor-created KV/JSON and engine-created event item errors preserve stable engine codes. |
| Pagination | Ready for this slice | Executor page outputs expose `items`, `has_more`, and `cursor` through shared `PageInfo`. |
| Optional read normalization | Ready for this slice | JSON wire-level maybe wrappers; KV/vector batch get item `found`; top-level non-JSON optional reads map through IDL `Maybe<T>`. |
| Golden response snapshots | Ready for this slice | Representative public response family fixtures and direct shared-concept fixtures are checked in and tested. |
| SDK response ergonomics | Not ready | SDKs still need command-specific inference. |

## Completion Slices

### 1. Structured Batch Item Errors

Status: implemented for executor-next batch item result types. Batch item
failures now serialize `ErrorStatus`, successful items serialize `error: null`,
and existing message accessors remain for compatibility.

This is the highest priority gap because it is the most visible failure-quality
defect after the top-level error contract work.

Completed work items:

1. Added an item-level public error shape that reuses `ErrorStatus`.
2. Replaced string-only KV batch write/delete failures.
3. Replaced string-only JSON batch set/delete/get failures.
4. Replaced string-only vector, event, and graph batch item failures where they
   exist.
5. Preserved positional ordering.
6. Preserved the distinction between item validation failure and top-level batch
   rejection.
7. Preserved engine validation status for KV, JSON, and event item failures.

Rules:

1. Invalid individual items may produce item-level `ErrorStatus`.
2. Invalid batch structure that prevents safe execution may remain a top-level
   error.
3. Duplicate valid mutation keys should remain top-level errors when the engine
   requires atomic rejection.
4. Item-level errors must include class, code, retry policy, commit outcome,
   message, suggested fix, docs URL, reference ID, details, and hints.

Exit criteria:

1. No public batch item failure contains only a string.
2. Top-level and item-level errors share the same public vocabulary.
3. Golden JSON tests cover item validation failures.
4. Existing batch behavior and positional ordering remain stable.

### 2. Mutation Output Normalization

Status: implemented at the executor boundary for rebuilt primitive mutation
outputs.

Every current executor mutation response now exposes shared commit and effect
facts where applicable. Primitive-specific fields remain for compatibility and
ergonomics, but SDKs no longer need to infer basic mutation state from variant
names alone.

Completed work items:

1. Vector create/upsert/update/delete/delete-by-filter/delete-all.
2. Vector batch upsert/delete item success effects and commit receipts.
3. Event append and batch append item success effects and commit receipts.
4. Graph node write, edge write, delete, and batch write.
5. Graph batch item success effects and commit receipts.
6. Space create/delete.
7. Empty graph batch reports `unchanged` instead of `not_found`.
8. Admin mutation commands remain deferred because no user-facing admin
   mutation output exists in this slice.

Target model:

```text
MutationEffect {
  applied: bool,
  kind: created | updated | deleted | unchanged | not_found,
  matched: bool,
  affected_count: u64
}

CommitReceipt {
  version: u64,
  timestamp: u64,
  durable: bool,
  put_count: u64,
  delete_count: u64
}
```

Primitive-specific facts should remain available, but they should not replace
the common facts.

Examples:

1. Vector write should expose collection, key, vector revision, `effect`, and
   `commit`.
2. Vector metadata update should expose updated/missing through `effect`.
3. Event append should expose sequence and event type plus `commit`.
4. Graph node write should expose node ID and graph name plus `effect` and
   `commit`.
5. Space delete should expose space name, force facts, deleted row count,
   `effect`, and optional `commit`.

Exit criteria:

1. Every applied mutation has `commit`.
2. Every miss/no-op mutation has `effect`.
3. Durability is visible whenever a commit exists.
4. SDKs can determine applied/missed/committed without inspecting variant names.

### 3. Optional Read Normalization

Status: implemented at the executor boundary for V1.

JSON has the highest-risk ambiguity and uses explicit maybe wrappers on the
wire. KV, vector, event, graph, and admin top-level optional reads keep their
existing `Option<T>` wire shape and map to shared SDK/IDL `Maybe<T>` accessors.
KV and vector batch get item payloads expose explicit `found` fields so
itemwise batch reads do not require plain JSON clients to infer misses from a
nullable `value` field.

Work items:

1. Keep JSON on explicit maybe wrappers.
2. Keep KV `Option<Bytes>` top-level outputs as SDK-mapped `Maybe<Bytes>`.
3. Keep vector, event, graph node, graph edge, graph info, and admin optional
   top-level outputs as SDK-mapped `Maybe<T>`.
4. Add explicit `found` to KV and vector batch get item payloads.
5. Ensure generated SDKs do not collapse missing into null where null is a
   valid returned value.

Rules:

1. JSON must never use `Option<Value>` as a public missing/null signal.
2. Other primitives may keep command-specific wrappers only if SDKs produce a
   consistent found/missing ergonomic model.
3. Optional read wrappers must preserve version facts when present.
4. Batch get item DTOs should expose `found` directly when consumed outside a
   generated SDK.

Exit criteria:

1. Missing versus present is unambiguous for every optional read.
2. SDKs expose one ergonomic pattern for optional reads.
3. Golden JSON tests cover missing and present reads for every primitive.

### 4. Pagination Normalization

Current page outputs generally expose `items`, `has_more`, and `cursor`, but the
public model is repeated across command-specific variants.

Work items:

1. Define a shared page metadata concept.
2. Decide whether the wire shape uses a generic page or concrete named page
   wrappers per primitive.
3. Normalize cursor naming and terminal-page behavior.
4. Keep cursors opaque.
5. Cover KV, JSON, vector, event, graph, branch, space, and admin list outputs.

Exit criteria:

1. Every page exposes `items`, `has_more`, and `cursor`.
2. Terminal pages serialize consistently.
3. SDK pagination helpers do not require command-specific cursor logic.
4. Golden JSON tests cover first page, continued page, and terminal page.

### 5. Success Diagnostics

Vector index query diagnostics are strong. Other primitives should not invent
verbose diagnostics without a reason, but they need enough structured facts for
debugging and automation.

Work items:

1. Define when success diagnostics are required.
2. Keep plain acknowledgements plain when no extra facts help the user.
3. Add diagnostic categories for index/search/import/export/inference paths.
4. Ensure diagnostics never expose storage row keys, artifact paths, or internal
   control-plane IDs.

Exit criteria:

1. Diagnostics are useful where present.
2. Diagnostics do not leak lower-layer implementation details.
3. SDK and CLI can display diagnostics consistently.

### 6. Golden Response Snapshots

Status: implemented for the executor-next representative fixture matrix.

Serde round-trip tests are necessary but not sufficient for an SDK freeze.

Work items:

1. Added golden JSON fixtures for every public response family.
2. Covered top-level success, top-level failure status, item success, item
   failure, optional reads, pages, and diagnostics.
3. Covered durable and cache commit receipts.
4. Covered no-op/miss responses.
5. Added a review rule that public response shape changes require fixture
   updates.

Exit criteria:

1. Public JSON shape changes require explicit fixture updates.
2. SDK generators consume the same contract represented by fixtures.
3. CI fails on accidental response drift.

### 7. Error Code Registry And Docs

The error shape is strong, but Stripe-quality errors need stable codes and docs.

Work items:

1. Create the public error code registry.
2. Assign default retry policy per code.
3. Assign default suggested fix per code.
4. Create docs targets for executor-rendered docs URLs.
5. Add tests that every emitted public code exists in the registry.

Exit criteria:

1. No public error code is undocumented.
2. Docs URLs resolve to a stable page.
3. Retry policy is reviewed per code, not only per class.

### 8. SDK And CLI Conformance

The response contract is not finished until downstream callers can use it
without special-case inference.

Work items:

1. Generate or hand-build SDK response models from the IDL.
2. Add SDK conformance tests for common response questions.
3. Add CLI rendering tests for success and error output.
4. Add MCP/agent-facing response examples.

Exit criteria:

1. SDKs can answer applied/missed/committed/found/continued/failed consistently.
2. CLI output renders the same structured facts.
3. MCP tools do not need hidden command-specific response parsing.

## Implementation Order

1. Structured batch item errors. Complete.
2. Mutation output normalization for vector/event/graph. Complete.
3. Mutation output normalization for space/admin/branch. Space complete; admin
   and branch user mutations are deferred until those commands exist in the
   public V1 surface.
4. Pagination normalization. Complete.
5. Batch wrapper normalization. Complete.
6. Optional read normalization.
7. Golden response snapshots.
8. Error registry and docs URL publication.
9. SDK and CLI conformance.

This order fixes the most visible quality gap first, then makes mutation
responses consistent, then freezes read/page ergonomics, then locks the public
wire contract.

## Edge Cases To Preserve

1. Missing delete must be a successful no-op/miss unless the command itself is
   invalid.
2. A no-op/miss should not fabricate a commit receipt.
3. A batch with invalid item inputs should preserve item order.
4. A batch with duplicate valid mutation targets may still be rejected
   top-level if atomic semantics require it.
5. JSON `null` must remain distinguishable from missing.
6. Durable and cache modes must produce the same response shape.
7. Ambiguous commit failures must not be downgraded to generic IO errors.
8. Commit receipt durability must reflect the engine/storage mode honestly.
9. Response models must not expose storage row IDs, internal branch IDs, system
   space keys, artifact file names, or WAL/table internals.

## Freeze Gates

The V1 response contract is ready to freeze only when:

1. every top-level error serializes as `ErrorStatus`;
2. every batch item failure serializes as `ErrorStatus`;
3. every applied mutation exposes `CommitReceipt`;
4. every no-op, miss, unchanged, created, updated, and deleted response exposes
   `MutationEffect`;
5. every optional read has unambiguous found/missing semantics;
6. every page has one continuation contract;
7. public error codes have registry entries and docs targets;
8. public response families have golden JSON snapshots;
9. SDKs and CLI can consume common response facts without command-specific
   inference;
10. storage and engine lower-layer implementation details do not leak into
   public responses.

## Definition Of Done

The response contract is complete when a user, SDK, CLI, or AI agent can inspect
any Strata command response and answer:

1. Did it succeed or fail?
2. If it failed, what stable code explains it and what should happen next?
3. If it mutated state, what changed and what commit was produced?
4. If it did not mutate state, was that because the target was missing,
   unchanged, or invalid?
5. If it read data, was the data found?
6. If it returned a page, how does the caller continue?
7. If it returned a batch, what happened to every item?

without knowing the internal storage model or writing command-specific response
parsers for common success and failure facts.
