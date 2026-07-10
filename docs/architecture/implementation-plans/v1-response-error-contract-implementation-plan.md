# V1 Response And Error Contract Implementation Plan

## Status

The first error-contract slice is implemented:

1. `engine-next` now has V1 public error classes, retry policies, commit
   outcome status, suggested fixes, structured details, and compatibility
   accessors.
2. `engine-next` maps storage failures into public status facts while retaining
   the source chain for diagnostics.
3. `executor-next` now serializes a public `ErrorStatus` with class, code,
   retry policy, commit outcome, message, suggested fix, docs URL, reference ID,
   optional trace ID, details, and hints.
4. Inference errors are mapped through the same executor status boundary.
5. Focused contract, redaction, fault-injection, and behavior tests cover the
   implemented slice.

Review follow-up fixes are also implemented:

1. Executor boundary rendering now accepts an injectable
   `ErrorRenderConfig { docs_base_url, reference_id_source }`.
2. Engine error codes are preserved at the executor boundary instead of being
   rewritten from `.engine.` to `.executor.`.
3. Storage failures are translated into product persistence details and
   suggested fixes without public `storage_*` detail keys or raw storage
   remediation text.
4. Inference retry policies match the V1 contract for download verification
   failures and local runtime failures.
5. Legacy `retryable()` documentation now points callers to `retry_policy()`
   for same-request versus after-state-change decisions.

The remaining slices in this plan are still open: success-output normalization,
batch item error normalization, golden public JSON snapshots, and IDL readiness
inventory.

## Problem

`storage-next`, `engine-next`, and `executor-next` now have enough behavior to
serve as the source surface for the V1 IDL, SDKs, CLI, and MCP server. The
remaining gap is response quality.

Successful executor outputs are mostly structured and serde-tested, but the
shape is not yet normalized into a product contract that downstream generators
can rely on without command-specific guesswork.

Failure responses are not yet Stripe-grade. Storage exposes stable mechanical
errors and remediation hints, and engine preserves redaction and retryable
state, but the public executor error still serializes only:

```text
class
code
retryable
message
```

That is not enough for SDKs, CLI, MCP, or AI agents to decide whether to retry,
what action to take, whether a write may have committed, which docs page to
open, or how to correlate the user-visible error with diagnostics.

This plan closes the response/error contract gap before freezing the IDL.

## Related Documents

- `docs/architecture/v1-error-and-diagnostics-contract.md`
- `docs/architecture/engine/error-and-diagnostics-contract.md`
- `docs/architecture/strata-sdk-quality-playbook.md`
- `crates/storage-next/src/api/error.rs`
- `crates/engine-next/src/diagnostics/error.rs`
- `crates/engine-next/src/diagnostics/registry.rs`
- `crates/engine-next/src/persistence/adapter.rs`
- `crates/executor-next/src/error.rs`
- `crates/executor-next/src/output.rs`
- `crates/executor-next/tests/command_contract.rs`
- `crates/executor-next/tests/error_and_guards.rs`

## Current State

### Success Path

`executor-next::Output` already provides typed variants for admin, spaces,
branches, KV, JSON, vector, event, graph, Arrow, and inference.

Strengths:

1. Outputs are serializable with stable tagged JSON.
2. Write outputs generally include commit version and timestamp.
3. Idempotent/no-op operations generally expose `created`, `deleted`, or
   `updated`.
4. Paginated outputs generally expose `has_more` and `cursor`.
5. Vector indexed queries expose planner diagnostics.
6. Command/output serde contract tests cover every output fixture.

Gaps:

1. Commit facts are repeated ad hoc instead of using one product concept.
2. Durable commit facts are not part of primitive write acknowledgements.
3. Batch item success/failure shape is not normalized across primitives.
4. Page field names are mostly consistent but not expressed as one contract.
5. Success JSON has round-trip tests, but not golden status snapshots for each
   public command family.
6. IDL generation would need to infer repeated concepts from many variants.

### Failure Path

`storage-next::StorageApiError` is the strongest layer today.

Strengths:

1. Stable storage class and code.
2. Redacted public display strings.
3. Mechanical `remediation()`.
4. Structured variant facts for branch IDs, conflicts, memory pressure, and
   resource exhaustion.
5. Source-chain preservation for lower-layer and durable-uncertain errors.
6. Contract tests for class/code/remediation/message redaction.

`engine-next::EngineError` is still thinner.

Strengths:

1. Stable engine class and code.
2. Source-chain preservation.
3. Debug registry check for constructed engine codes.
4. Redaction and retryable tests for mapped storage failures.

Gaps:

1. `retryable: bool` instead of `RetryPolicy`.
2. No `CommitOutcomeStatus`.
3. No structured public details.
4. No user-facing hints or suggested fixes.
5. Storage remediation does not survive as a structured field.
6. Some V1 classes are collapsed into current coarse classes, for example
   resource exhaustion and unsupported capability are represented through
   unavailable-style engine errors.

`executor-next::ExecutorError` is not yet a V1 public status.

Gaps:

1. No `ErrorStatus`.
2. No `retry_policy`.
3. No `commit_outcome`.
4. No `suggested_fix`.
5. No `docs_url`.
6. No `reference_id`.
7. No structured `details`.
8. No structured `hints`.
9. No boundary-owned reference-id source.
10. No public error JSON golden snapshots.

## Goals

1. Make every public failure machine-readable and useful to humans, SDKs, CLI,
   MCP, and AI agents.
2. Preserve storage-owned mechanical facts while letting engine compose product
   meaning.
3. Expose explicit retry and ambiguous-commit semantics.
4. Keep reference IDs and docs URLs boundary-owned and deterministic in tests.
5. Normalize repeated success concepts without forcing every command into an
   awkward universal envelope.
6. Produce IDL-ready response and error vocabulary.
7. Add conformance tests that fail when new commands or errors bypass the
   contract.

## Non-Goals

1. Do not build the full IDL generator in this slice.
2. Do not build CLI-next or SDKs in this slice.
3. Do not move the database error taxonomy into `core-next`.
4. Do not make prose messages compatibility contracts.
5. Do not expose storage-internal type names or lower-layer details publicly.
6. Do not require remote telemetry, OpenTelemetry, or cross-process tracing.
7. Do not change primitive command semantics except where success/error shape
   requires additional metadata.

## Target Public Error Shape

The public boundary should serialize failures as:

```text
ErrorStatus {
  class,
  code,
  retry_policy,
  commit_outcome,
  message,
  suggested_fix,
  docs_url,
  reference_id,
  trace_id,
  details,
  hints,
}
```

`reference_id` is the user-visible support/debug token required by the SDK
quality playbook. `trace_id` is optional and exists only when a caller,
transport, or local boundary provides a separate request/trace identifier. In a
pure embedded executor call, `reference_id` may be the only correlation token.

Field ownership:

| Field | Owner |
| --- | --- |
| `class` | Failing semantic layer, normalized by engine |
| `code` | Failing semantic layer, registered |
| `retry_policy` | Engine, using storage/inference facts where relevant |
| `commit_outcome` | Engine/persistence boundary |
| `message` | Engine product phrasing, redacted |
| `suggested_fix` | Engine product guidance from mechanical remediation |
| `docs_url` | Executor/IPC/CLI boundary, derived from code |
| `reference_id` | Executor/IPC/CLI boundary, from injected ID source |
| `trace_id` | Optional caller or transport trace context |
| `details` | Engine, redacted structured facts |
| `hints` | Engine, optional user-facing action hints |

Storage must not assign `reference_id`, derive docs URLs, or phrase
end-user guidance. It should continue to expose mechanical class/code/message,
structured facts, source chain, and mechanical remediation.

## V1 Error Class Vocabulary

The implementation must converge on the V1 class list from
`docs/architecture/v1-error-and-diagnostics-contract.md`.

Required public classes:

- `not_found`
- `already_exists`
- `invalid_argument`
- `failed_precondition`
- `access_denied`
- `conflict`
- `ambiguous_commit`
- `history_unavailable`
- `unsupported`
- `resource_exhausted`
- `unavailable`
- `io`
- `corruption`
- `serialization`
- `internal`

Current `EngineErrorClass` and `ExecutorErrorClass` are narrower. The
implementation must either expand them or add a V1-facing class type that maps
from legacy classes during the migration. The final public `ErrorStatus` must
not collapse `unsupported`, `resource_exhausted`, `history_unavailable`, `io`,
`serialization`, or `access_denied` into generic `unavailable` or `internal`.

## Target Success Concepts

Do not replace `Output` with one global envelope in this slice. Keep the
existing command-specific output variants, but define and reuse common DTOs so
the IDL can recognize stable concepts.

Common concepts:

```text
CommitAck {
  version,
  timestamp,
  durable,
  rows_written,
  rows_deleted,
}

MutationOutcome {
  applied,
  created,
  updated,
  deleted,
  commit,
}

Page<T> {
  items,
  has_more,
  cursor,
}

BatchItem<T> {
  index,
  ok,
  result,
  error,
}
```

Rust output variants do not need to literally use generic DTOs everywhere, but
their serialized shape and IDL metadata must map cleanly to these concepts.

## Design Decisions

1. **Engine owns product error meaning.** Engine translates storage mechanics
   into product class, code, retry policy, commit outcome, message, details,
   and suggested fix.

2. **Storage stays mechanical.** Storage keeps `StorageApiError` and
   `remediation()` mechanical. It may expose additional structured facts, but
   it does not mention SDKs, CLI, docs URLs, or product command names.

3. **Executor owns boundary rendering.** Executor converts engine errors into
   serializable `ErrorStatus`, injects `reference_id`, derives `docs_url`, and
   remains independent of storage.

4. **Reference IDs are injected.** No constructor should mint random IDs.
   Tests must be able to use a deterministic source.

5. **Docs URLs are derived, not stored.** The boundary derives
   `<docs_base>/errors/<code>`. The registry controls which codes are valid.

6. **Retry policy is an enum.** `retryable: bool` may remain as a deprecated
   helper, but it cannot be the primary contract.

7. **Ambiguous commit is first-class.** Any uncertain durable write outcome must
   render `commit_outcome=maybe_committed`.

8. **Success commit metadata must be consistent.** Every applied write should
   expose the same commit facts either directly or through a common DTO.

9. **Batch item errors use the same status shape.** Primitive-local batch item
   errors must not remain raw strings where the engine can provide structured
   statuses.

10. **The IDL consumes this contract.** The final shapes should be described
    once and reused by IDL, CLI, SDK, MCP, docs, and snapshots.

## Implementation Order

### 1. Add Contract Types

Add engine-owned status primitives under `crates/engine-next/src/diagnostics/`.

Types:

- `ErrorClass`
- `RetryPolicy`
- `CommitOutcomeStatus`
- `ErrorDetail`
- `ErrorHint`
- `SuggestedFix`
- `EngineErrorStatus`

`ErrorClass` must use the V1 vocabulary listed above. During migration, keep
old `EngineErrorClass`/`ExecutorErrorClass` accessors as compatibility helpers
only.

Initial enum values:

`RetryPolicy`:

- `never`
- `after_state_change`
- `same_request`
- `idempotent_only`
- `unknown`

`CommitOutcomeStatus`:

- `not_applicable`
- `not_started`
- `definitely_not_committed`
- `maybe_committed`
- `committed_post_commit_failed`

`ErrorDetail` should be a constrained structured enum, not arbitrary
`serde_json::Value` as the first implementation. Start with:

- branch name/id
- space
- key fingerprint or safe key display
- collection/document/graph/event type
- version/timestamp
- limit/used/requested
- backend/storage mode
- operation
- source code/class

Exit criteria:

1. New types compile without changing public behavior.
2. Serialization names match V1 docs.
3. Unit tests cover serde round trip for each enum.
4. Public status types are `Send + Sync + 'static` where applicable.
5. Public enums that may grow after V1 are `#[non_exhaustive]`.

### 2. Upgrade `EngineError`

Extend `EngineError` to carry the new status fields.

Required fields:

- class
- code
- retry_policy
- commit_outcome
- message
- suggested_fix
- details
- hints
- source

Keep compatibility helpers temporarily:

- `retryable()` returns a derived boolean.
- `class()` maps to the current `EngineErrorClass` while old call sites are
  being migrated.

Add constructors for common cases:

- `invalid_input`
- `not_found`
- `already_exists`
- `history_unavailable`
- `conflict`
- `failed_precondition`
- `access_denied`
- `unsupported`
- `resource_exhausted`
- `io`
- `serialization`
- `unavailable`
- `ambiguous_commit`
- `corruption`
- `internal`

Exit criteria:

1. Existing engine tests continue to pass.
2. Every constructed `EngineError` has retry policy and commit outcome.
3. No call site constructs a generic error without suggested fix or explicit
   reason to use a default internal hint.
4. Source chains remain available through `std::error::Error::source`.
5. Display output remains non-empty and redacted.

### 3. Preserve Storage Remediation And Facts

Update `crates/engine-next/src/persistence/adapter.rs` so storage errors map
into `EngineErrorStatus` without losing mechanical facts.

Mapping requirements:

| Storage class | Engine class | Retry policy | Commit outcome |
| --- | --- | --- | --- |
| `InvalidArgument` | `invalid_argument` | `never` | `not_started` |
| `Unsupported` | `unsupported` | `after_state_change` | `not_applicable` |
| `NotFound` | `not_found` | `never` or `after_state_change` by context | `not_applicable` |
| `AlreadyExists` | `already_exists` or `conflict` by operation | `never` | `not_started` |
| `Conflict` | `conflict` | `after_state_change` | `definitely_not_committed` |
| `HistoryUnavailable` | `history_unavailable` | `after_state_change` | `not_applicable` |
| `AmbiguousCommit` | `ambiguous_commit` | `unknown` or `idempotent_only` | `maybe_committed` |
| `ResourceExhausted` | `resource_exhausted` | `after_state_change` | operation-specific |
| `FailedPrecondition` | `failed_precondition` or `unavailable` by reason | context-specific | context-specific |
| `Internal` | `unavailable`, `io`, `corruption`, or `internal` by source | context-specific | context-specific |

Add a storage-remediation detail or hint field to the engine error. Translate
mechanical remediation into product `suggested_fix`; retain the mechanical text
only in safe structured details or source diagnostics.

Rules from the V1 contract:

1. Do not use blanket `From<StorageApiError>` for write-path phases where
   commit outcome matters.
2. Read-path conversion may be shared only when it cannot lose class, code,
   retry policy, commit outcome, or operation context.
3. Include operation phase for WAL, manifest, table, snapshot, recovery,
   publish, sync, and backend errors.
4. Map capability mismatch to `unsupported.*`, not generic unavailable.
5. Map writer-lock conflict to `unavailable.writer_lock` unless engine can make
   a stronger product classification.
6. Map corruption to `corruption.*`, never `io.*`.
7. Map serialization/format failures to `serialization.*`, `unsupported.*`, or
   `corruption.*` according to the format spec.
8. Map unknown durable publish outcome to `ambiguous_commit.*` and
   `maybe_committed`.

Exit criteria:

1. No storage remediation is discarded without an explicit test.
2. Resource exhaustion no longer appears publicly as generic unavailable.
3. Ambiguous durable commit always renders `maybe_committed`.
4. Lower-layer storage failures remain redacted.

### 4. Add Commit Outcome Classification To Persistence Calls

Audit every engine write path that calls persistence commit.

For each path, classify failures:

- validation before commit: `not_started`
- admission rejected before commit starts: `definitely_not_committed`
- storage conflict before visibility: `definitely_not_committed`
- durable uncertainty: `maybe_committed`
- commit succeeds but post-commit index/diagnostic work fails:
  `committed_post_commit_failed`
- read-only failures: `not_applicable`

Targets:

- KV write/delete/batch
- JSON set/delete/batch/index mutations
- vector upsert/delete/update/index artifact maintenance
- event append/batch append
- graph create/delete/node/edge/batch
- branch create/fork/delete
- space create/delete
- Arrow import
- admin/config mutations if restored later

Exit criteria:

1. Every mutating engine service has tests for validation failure before commit.
2. Persistence fault tests assert `commit_outcome`.
3. Ambiguous commit tests assert both class and outcome.

### 5. Build Public Executor `ErrorStatus`

Replace or wrap `ExecutorError` with a serializable public status type.

Types:

```text
ExecutorError {
  status: ErrorStatus
}

ErrorStatus {
  class,
  code,
  retry_policy,
  commit_outcome,
  message,
  suggested_fix,
  docs_url,
  reference_id,
  trace_id,
  details,
  hints,
}
```

Compatibility:

- Preserve `ExecutorError::class()`, `code()`, `message()`, and `retryable()`
  accessors for current tests and callers.
- Add `status()`.
- Deprecate direct dependence on `retryable()`.

Boundary inputs:

- `ErrorRenderConfig { docs_base_url, reference_id_source }`
- optional caller/request trace context
- default docs base: `https://strata.dev/docs/errors`
- deterministic test reference source

Exit criteria:

1. Executor errors serialize to the full V1 shape.
2. `reference_id` appears exactly once in the public status.
3. `docs_url` is derived from code.
4. Existing convenience accessors still work.
5. Executor-owned command/serde failures use `invalid_argument.command`,
   `serialization.command_payload`, `unsupported.command_version`, or another
   registered command/protocol code rather than generic internal errors.

### 6. Add Reference ID And Docs URL Rendering

Add a small boundary renderer in `executor-next`.

Responsibilities:

1. Convert `EngineError` into `ErrorStatus`.
2. Convert inference errors into `ErrorStatus`.
3. Convert executor-owned validation errors into `ErrorStatus`.
4. Mint deterministic or random reference IDs through an injected source.
5. Derive docs URLs from code.
6. Attach optional trace/request ID when supplied by a caller or transport.
7. Apply final redaction checks.

Reference ID format:

```text
err_<base32-or-hex-token>
```

Tests should use:

```text
err_test_000001
err_test_000002
```

Exit criteria:

1. No random IDs in unit tests.
2. Repeated rendering of the same pure error with the same injected source is
   deterministic.
3. A docs base override is tested.
4. Reference IDs are assigned at rendering/logging time, never during error
   construction.
5. The same boundary-minted ID is available to public status and correlated
   logs/diagnostics.

### 7. Create The Error Registry Source

Promote the current engine Rust registry into an IDL-ready registry.

Minimum viable registry fields:

```text
code
class
retry_policy_default
commit_outcome_default
commit_outcome_can_vary
message_template
suggested_fix_template
docs_slug
details_schema
owner_layer
reserved
```

Implementation options:

1. Keep the Rust registry as the enforcement mechanism for this slice and add
   a generated/exported JSON registry later.
2. Add a machine-readable registry now under `docs/architecture/error-registry.yaml`
   and test that engine/executor code uses only registered codes.

Preferred path:

Start with a machine-readable registry because the SDK/IDL milestone needs it
anyway. The Rust registry can be generated from it later, but it does not need
to be generated in the first slice.

Exit criteria:

1. Every engine and executor code is registered.
2. Registry class matches constructed error class.
3. Every registry row has suggested fix and docs slug.
4. No dead registry codes unless explicitly marked reserved.
5. Codes are URL-path-safe lowercase ASCII and belong to exactly one class.
6. `internal.*` rows are rare and justified.

### 8. Normalize Success Response Concepts

Keep `Output` variants, but introduce shared DTOs in `executor-next::types`
where the shape is repeated.

Add:

- `CommitAck`
- `PageInfo`
- `MutationFlags`
- `BatchErrorStatus` or reuse `ErrorStatus` inside batch items

Apply where compatible without causing excessive churn:

- write acknowledgements should expose `commit` or map cleanly to it in IDL
- page outputs should expose `items`, `has_more`, `cursor` in IDL metadata
- batch item errors should use structured status where failures are produced by
  engine validation rather than primitive-local raw strings

Specific audit items:

1. KV `WriteResult`, `DeleteResult`, `BatchResults`
2. JSON write/delete/batch/index outputs
3. vector write/update/delete/bulk/batch outputs
4. event append/batch outputs
5. graph create/write/delete/batch outputs
6. Arrow import output
7. spaces create/delete output

Exit criteria:

1. Every applied mutation returns commit version and timestamp.
2. Durable mode can report durability where product-relevant.
3. No-op mutations clearly report `applied=false` or equivalent fields.
4. Batch item failures are structured or explicitly documented as
   primitive-local validation errors that will be upgraded before SDK freeze.

### 9. Add Golden Public JSON Snapshots

Add stable JSON fixture tests for response/error contracts.

Targets:

- `crates/executor-next/tests/response_contract.rs`
- `crates/executor-next/testdata/response-contract/*.json`

Snapshot cases:

Success:

1. KV put/delete/no-op delete
2. JSON set/get/delete
3. vector upsert/query/index query
4. event append/range
5. graph add node/add edge/delete
6. branch create/fork/delete
7. space create/delete
8. admin health/describe
9. Arrow import/export summaries

Error:

1. invalid argument
2. not found
3. already exists
4. conflict
5. history unavailable
6. unsupported capability
7. resource exhausted
8. unavailable
9. ambiguous commit
10. corruption
11. closed handle
12. inference provider unavailable

Exit criteria:

1. Public JSON shape is locked by fixtures.
2. Fixtures do not include secret or lower-layer terms.
3. New command variants fail tests until a fixture is added or explicitly
   deferred.

### 10. Add Exhaustive Mapping Tests

Storage tests:

- every `StorageApiError` variant has class, code, message, remediation,
  structured facts, and redaction
- every source-bearing storage error preserves source chain

Engine tests:

- every storage error maps to an engine status preserving semantic class,
  retry policy, commit outcome, and suggested fix
- every engine code is registered
- every engine error constructor supplies retry policy and commit outcome
- redaction applies to display, details, hints, suggested fix, and source chain

Executor tests:

- every engine status maps to executor `ErrorStatus`
- docs URL is derived from code
- reference ID is injected
- old accessors reflect status fields
- serialized errors include all required fields
- executor-owned validation errors use the same status shape

Exit criteria:

1. Tests assert on codes/classes/policies/outcomes, not prose.
2. Missing mapping arms are compile failures or explicit test failures.
3. No `retryable: bool` remains as the only assertion in new tests.

### 11. Add Redaction And Detail Guards

Expand redaction tests to cover:

- message
- suggested fix
- hints
- details
- source-chain display
- serialized executor status
- batch item errors

Forbidden examples:

- API keys
- bearer tokens
- local absolute paths unless intentionally redacted
- object-store credentials
- raw WAL/table/manifest internal keys
- native provider response bodies
- lower-layer Rust type names

Exit criteria:

1. Redaction tests cover every public error field.
2. Unsafe details are dropped or fingerprinted.
3. Source errors may be retained internally but do not leak through normal
   serialization.

### 12. Update Inference Error Mapping

Inference is part of executor output when enabled, so it must join the same
status contract.

Tasks:

1. Map inference error classes to V1 classes.
2. Add retry policy per provider/model/network/cache case.
3. Add suggested fixes for missing API keys, missing local model, network
   disabled, provider unavailable, provider response invalid, and local runtime
   failure.
4. Redact provider messages before executor rendering.
5. Ensure local inference CMake/llama.cpp failures do not leak raw native
   internals in public status.

Exit criteria:

1. Inference executor errors render `ErrorStatus`.
2. Cloud provider missing-key errors include actionable suggested fix without
   exposing key names beyond public env var names.
3. Provider retryability is explicit.

### 13. Wire IDL Readiness Checks

Before building the IDL generator, add a contract export test that proves the
current code can produce the core IDL response/error inventory.

Inventory:

- command name
- output variant
- success concepts used
- possible error codes
- error classes
- retry policies
- commit outcomes
- docs slugs

This does not need to generate SDKs yet. It should produce a deterministic JSON
or Markdown artifact for review.

Exit criteria:

1. The artifact is deterministic.
2. Every command is represented.
3. Every public error code is represented.
4. Missing success/error metadata fails the test.

## File-Level Work Plan

### Storage-Next

Likely targets:

- `crates/storage-next/src/api/error.rs`
- `crates/storage-next/tests/api_error_contract.rs`

Expected work:

1. Add structured accessor methods for remediation-relevant facts where engine
   currently has to parse display strings or match variants manually.
2. Keep mechanical remediation complete.
3. Add tests for structured fact accessors and redaction.

### Engine-Next

Likely targets:

- `crates/engine-next/src/diagnostics/error.rs`
- `crates/engine-next/src/diagnostics/registry.rs`
- `crates/engine-next/src/diagnostics/mod.rs`
- `crates/engine-next/src/persistence/adapter.rs`
- primitive service files under `crates/engine-next/src/data/`
- branch/control/admin APIs under `crates/engine-next/src/api/`
- `crates/engine-next/tests/error_redaction.rs`
- `crates/engine-next/tests/persistence_faults.rs`
- `crates/engine-next/tests/common/mod.rs`

Expected work:

1. Add V1 status fields.
2. Replace boolean retry decisions with `RetryPolicy`.
3. Classify commit outcome at persistence boundaries.
4. Preserve storage remediation as product suggested fixes.
5. Expand registry checks.

### Executor-Next

Likely targets:

- `crates/executor-next/src/error.rs`
- `crates/executor-next/src/output.rs`
- `crates/executor-next/src/types.rs`
- `crates/executor-next/src/executor.rs`
- `crates/executor-next/tests/command_contract.rs`
- `crates/executor-next/tests/error_and_guards.rs`
- new `crates/executor-next/tests/response_contract.rs`

Expected work:

1. Add `ErrorStatus`.
2. Add boundary renderer and injected reference ID source.
3. Preserve old accessors.
4. Normalize success DTOs where useful.
5. Add public JSON fixtures.

### Docs And Registry

Likely targets:

- `docs/architecture/v1-error-and-diagnostics-contract.md`
- `docs/architecture/engine/error-and-diagnostics-contract.md`
- new `docs/architecture/error-registry.yaml`
- generated or curated `docs/reference/errors/*.md` later

Expected work:

1. Keep docs aligned with implemented field names.
2. Add registry rows for every implemented code.
3. Add docs URL slug policy.

## Test Plan

### Required Unit Tests

1. Error enum serde round trips.
2. Retry policy boolean compatibility helper.
3. Commit outcome defaults.
4. Error detail redaction.
5. Reference ID deterministic source.
6. Docs URL derivation.
7. Registry code/class consistency.
8. Success DTO serde round trips.

### Required Integration Tests

1. Storage error to engine status mappings.
2. Engine error to executor status mappings.
3. Executor-owned validation errors.
4. Durable ambiguous commit fault.
5. Resource budget exhaustion fault.
6. History unavailable read.
7. Missing branch/key/document/collection/graph/model.
8. Batch partial failure status shape.
9. Success response snapshots.
10. Error response snapshots.

### Required Fault-Injection And Recovery Tests

These are contract requirements, not optional stress tests. Each case must
assert class, code, retry policy, commit outcome, source-chain presence when
applicable, and redaction.

1. Failed read.
2. Failed write before visibility.
3. Failed write after possible visibility.
4. Failed durable sync.
5. Failed WAL append.
6. Failed manifest publish.
7. Partial WAL tail.
8. Corrupt WAL record.
9. Corrupt table block.
10. Corrupt snapshot.
11. Stale object metadata.
12. Writer lock conflict.
13. IPC disconnect during write, once IPC exists.
14. Provider timeout.
15. Provider invalid response.
16. Rejected writes remain absent after recovery.
17. Ambiguous publish windows surface `maybe_committed`.
18. Recovery health facts remain available as diagnostics.

### Required Parser And Fuzz Tests

1. Invalid durable bytes never panic.
2. Invalid durable bytes return `serialization.*`, `corruption.*`, or
   `unsupported.*` as appropriate.
3. Huge declared lengths do not allocate unbounded memory.
4. Trailing bytes are rejected with a typed code or accepted only when the
   format spec explicitly allows them.
5. Malformed command payloads fail with registered command/protocol codes.

### Required Guards

1. No public serialized error omits required fields.
2. No new `EngineError::new` call bypasses registry/status metadata.
3. No executor source imports storage error types.
4. No error code is unregistered.
5. No registered code is dead unless marked reserved.
6. No lower-layer type names appear in public error JSON.
7. No batch item errors remain raw strings after the batch item is upgraded.
8. No test asserts on prose when code/class/policy/outcome is available.
9. No public error enum lacks `#[non_exhaustive]` without an explicit reason.
10. No source chain is rendered twice in display output and serialized status.

## Rollout Strategy

### Slice 1: Contract Types Only

Add new types and tests without changing public behavior.

### Slice 2: Engine Status Internals

Upgrade `EngineError` and persistence mapping. Keep executor output shape
compatible through old accessors.

### Slice 3: Executor Public ErrorStatus

Switch serialized executor errors to full status. Add reference IDs and docs
URLs.

### Slice 4: Success Output Normalization

Add shared DTOs and update output fixtures/snapshots where needed.

### Slice 5: Batch Item Structured Errors

Upgrade primitive batch item failure fields to use structured status where
engine can provide one.

### Slice 6: Registry And IDL Inventory

Add machine-readable registry and deterministic contract export.

### Slice 7: Closeout

Run focused and workspace tests, update architecture docs, and mark the
response/error surface ready for IDL generation.

## Exit Criteria

This work is complete when:

1. Every public executor failure serializes as `ErrorStatus`.
2. Every error status includes class, code, retry policy, commit outcome,
   message, suggested fix, docs URL, reference ID, details, and hints.
3. Every public error code is registered and has a docs slug.
4. Storage remediation survives engine mapping as product suggested fixes.
5. Ambiguous commit outcomes are never collapsed into generic retryability.
6. Success responses expose consistent mutation, page, batch, and diagnostic
   facts.
7. Golden success and error JSON fixtures cover every command family.
8. Redaction tests cover every public error field.
9. Executor remains storage-independent.
10. The IDL work can consume a deterministic response/error inventory.

## Open Questions

1. Should `ErrorStatus` live only in `executor-next`, or should a pure
   non-executor status type also be exported from `engine-next` for embedded
   Rust callers?
2. Should success write responses expose `durable` for all durable-mode writes,
   or only admin/diagnostic surfaces?
3. Should batch item errors immediately use full `ErrorStatus`, or should this
   be staged behind an IDL compatibility flag?
4. Should the registry start as YAML now, or should the Rust registry remain
   authoritative until the IDL crate exists?
5. Should docs URLs point at code-specific pages from day one, or at a single
   registry page with anchors until generated docs exist?
