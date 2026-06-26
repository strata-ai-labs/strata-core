# V1 Response Contract Completion Test Plan

## Status

Status: proposed final response-contract test plan.

This test plan validates
`docs/architecture/implementation-plans/v1-response-contract-completion-implementation-plan.md`.

## Test Goals

1. Prove response normalization does not change primitive behavior.
2. Prove SDK-facing concepts are stable and complete.
3. Prove public JSON response shapes cannot drift accidentally.
4. Prove every public error code is registered and documented.
5. Prove SDKs, CLI, and MCP tools can answer common response questions without
   command-specific inference.

## Test Fixtures

Create deterministic fixture groups under:

```text
crates/executor-next/tests/fixtures/responses/v1/
```

Recommended layout:

```text
responses/v1/
  shared/
    commit_receipt.json
    mutation_effect_created.json
    mutation_effect_not_found.json
    maybe_found_json_null.json
    maybe_missing.json
    page_terminal.json
    page_continued.json
    batch_result_partial.json
    error_status_invalid_argument.json
  kv/
  json/
  vector/
  event/
  graph/
  admin/
  arrow/
  inference/
```

Rules:

1. Fixtures must be pretty-printed.
2. Fixture field ordering should be deterministic.
3. Fixture names should map to IDL model names.
4. Fixture changes require intentional review.

## Shared Concept Tests

### CommitReceipt

Tests:

1. Serializes `version`, `timestamp`, `durable`, `put_count`, and
   `delete_count`.
2. Round-trips through JSON.
3. Rejects or cannot represent missing required fields in command outputs.
4. Durable and cache examples are covered when durability differs.

### MutationEffect

Tests:

1. `created`: applied true, matched false, affected count 1.
2. `updated`: applied true, matched true, affected count 1.
3. `deleted`: applied true, matched true, affected count 1.
4. `unchanged`: applied false, matched true, affected count 0.
5. `not_found`: applied false, matched false, affected count 0.
6. Multi-row deletes use affected count greater than 1.
7. Batch aggregate effects preserve homogeneous create/delete/update where
   possible and use the documented mixed-batch fallback.

### Page

Tests:

1. Terminal page: `items=[]`, `has_more=false`, `cursor=null`.
2. Non-empty terminal page: `has_more=false`, `cursor=null`.
3. Continued page: `has_more=true`, non-empty cursor.
4. `has_more=true` with `cursor=null` is impossible or rejected.
5. Cursor is treated as opaque in SDK tests.

### Maybe

Tests:

1. Missing read is `found=false`, `value=null`.
2. Present JSON null is `found=true`, `value=null`.
3. Present empty bytes is `found=true`.
4. Present empty array/object/string is `found=true`.
5. Version and timestamp are present when the primitive has row facts.

### BatchResult And BatchItem

Tests:

1. Empty batch returns `items=[]`, `applied=false`, and a terminal status.
2. Fully successful itemwise batch is `status=ok`.
3. Mixed itemwise batch is `status=partial`.
4. All-failed itemwise batch is `status=error`.
5. Atomic batch with invalid operation returns top-level error, not partial
   item success.
6. Every item preserves original input index.
7. Successful no-op item is `status=ok`, `applied=false`, `error=null`.
8. Failed item is `status=error`, `error=ErrorStatus`, `effect=null`,
   `commit=null`.
9. Batch-level commit matches item commit when a shared commit exists.

## Pagination Normalization Tests

### Unit Tests

1. `PageInfo` constructor rejects invalid `has_more/cursor` combinations.
2. Cursor serialization is stable.
3. Page helper functions preserve empty, terminal, and continued states.

### Executor Behavior Tests

Cover first page, continued page, and terminal page for:

1. KV key list.
2. KV scan if V1 exposes it as a page.
3. JSON list.
4. Vector key list.
5. Event range.
6. Graph list.
7. Graph node list.
8. Graph edge list.
9. Graph neighbor/binding pages.
10. Branch list if paginated.
11. Space list if paginated.
12. Admin list/status outputs if paginated.

Assertions:

1. Every output maps to `items`, `has_more`, and `cursor`.
2. Continued page cursor can be passed back to fetch the next page.
3. Terminal cursor is absent.
4. Branch and space isolation do not change pagination semantics.
5. Durable reopen does not invalidate opaque cursor format for stable data when
   the primitive promises cursor reuse.

### Edge Cases

1. Limit zero.
2. Limit one.
3. Limit greater than item count.
4. Cursor after last item.
5. Invalid cursor.
6. Prefix filters with pagination.
7. Deletes between pages where the primitive supports live pagination.

## Batch Wrapper Normalization Tests

Status: implemented for executor-next behavior and command-contract tests.

### KV

1. Batch put creates and updates in one shared commit.
2. Batch put with invalid key returns item-level error for that item.
3. Duplicate valid keys remain top-level error if atomic rejection is required.
4. Batch delete with missing key returns item `not_found`.
5. Empty batch returns normalized empty batch wrapper.

### JSON

1. Batch set create/update/repeated update reports correct effects.
2. Batch delete missing document reports `not_found`.
3. Invalid document id returns item-level `ErrorStatus`.
4. Stored JSON null in batch get is found, not missing.
5. Duplicate valid operations preserve current semantics.

### Vector

1. Batch upsert create/update facts are positional.
2. Duplicate vector keys preserve existing positional revision behavior.
3. Batch delete reports deleted and missing items.
4. Batch get reports found/missing values.
5. Empty batch returns normalized empty wrapper.

### Event

1. Empty batch returns normalized empty wrapper.
2. Mixed valid and invalid batch is itemwise partial.
3. Successful valid items share one commit.
4. Invalid event type and invalid payload preserve structured item errors.
5. Sequence ordering remains stable.

### Graph

1. Successful graph batch is atomic `status=ok`.
2. Invalid graph batch is top-level error and leaves no partial writes.
3. Delete miss is successful item `not_found` when the batch itself is valid.
4. Homogeneous create/update/delete batches expose precise aggregate effect.
5. Mixed applied graph batch uses documented aggregate kind.

### Arrow Import

1. Arrow import counts rows imported/skipped using normalized batch wrappers.
2. Invalid rows remain skipped without corrupting imported rows.
3. Import behavior is unchanged after wrapper normalization.

### Shared Contract Coverage

1. `command_contract.rs` asserts itemwise and atomic batch modes.
2. `command_contract.rs` asserts `ok`, `partial`, and `error` batch statuses.
3. `command_contract.rs` asserts stable item `index` values.
4. V1 response fixtures cover `batch_result_ok.json` and
   `batch_result_partial.json`.
5. `BatchItem<T>` preserves primitive item payloads under `result` while
   exposing shared `status`, `applied`, `effect`, `commit`, and `error`.

## Optional Read Normalization Tests

Status: implemented for executor-next command-contract coverage, JSON
wire-level maybe wrappers, and KV/vector batch get item `found` fields.

### KV

1. Missing key maps to SDK-facing `Maybe { found=false }`.
2. Present empty bytes maps to SDK-facing `found=true`.
3. Present bytes preserve version/timestamp.
4. Batch get preserves positional found/missing state with explicit item
   `found`.

### JSON

1. Missing document is `found=false`.
2. Stored root JSON null is `found=true`, `value=null`.
3. Missing path is distinct from present null if path reads are V1 surfaced.
4. Batch get distinguishes missing from stored null.
5. Historical reads preserve found/missing at timestamp.

### Vector

1. Missing vector maps to SDK-facing `Maybe { found=false }`.
2. Present vector with empty metadata maps to SDK-facing `found=true`.
3. Historical read before create is missing.
4. Historical read after delete is missing or tombstone per documented model.
5. Batch get preserves positional found/missing state with explicit item
   `found`.

### Event

1. Missing sequence maps to SDK-facing `Maybe { found=false }`.
2. Present event maps to SDK-facing `found=true` with version/timestamp.
3. Historical branch/space reads preserve isolation.

### Graph

1. Missing graph metadata maps to SDK-facing `Maybe { found=false }`.
2. Missing node maps to SDK-facing `Maybe { found=false }`.
3. Missing edge maps to SDK-facing `Maybe { found=false }`.
4. Present node/edge with empty properties maps to SDK-facing `found=true`.
5. Binding lookup no hits is a found empty page, not missing.

### Shared Contract Coverage

1. `command_contract.rs` asserts JSON missing and stored null serialize to
   different wire payloads.
2. `command_contract.rs` asserts KV, JSON, and vector batch get items expose
   explicit `found` facts and preserve row metadata on found items.
3. Golden fixtures under `responses/v1/optional_reads/` cover representative
   missing and null wire shapes.

## Golden Snapshot Tests

Status: implemented for executor-next representative response families and
shared V1 response concepts.

### Snapshot Families

Every family should include success, no-op/miss, page, batch, diagnostic, and
failure examples where applicable:

1. Admin and spaces.
2. Branch lifecycle.
3. KV.
4. JSON.
5. Vector, including index diagnostics.
6. Event.
7. Graph.
8. Arrow import/export.
9. Inference model/generate/embed/rank/cache.

Implemented representative fixtures currently cover:

1. Direct shared concepts: durable/cache `CommitReceipt`, `MutationEffect`,
   JSON `Maybe`, `PageInfo`, `BatchResult`, and `ErrorStatus`.
2. Admin, spaces, branches, KV, JSON, vector, event, graph, Arrow,
   status-helper, and inference output families.
3. Pages, optional reads, batches, diagnostics, mutation success, mutation
   miss/no-op, and failure status examples.

### Snapshot Assertions

1. Public JSON exactly matches fixture.
2. Unknown fields are rejected where commands require strict input.
3. Optional fields are omitted or included according to the V1 contract.
4. Successful batch items serialize `error: null`.
5. Failed batch items serialize structured `ErrorStatus`.
6. Commit receipts include durability.
7. No fixture contains storage row keys, system-space keys, artifact paths, WAL
   paths, or internal branch IDs.

### Snapshot Update Policy

1. Fixture updates require review.
2. Fixture update PRs must explain whether the change is intended V1 contract
   movement or an implementation bug fix.
3. After V1 freeze, fixture changes are breaking unless explicitly versioned.
4. Any public response shape change in `Output`, shared response helper types,
   or `ErrorStatus` should update the affected fixture in the same change.

## Error Code Registry Tests

Status: implemented for the engine and executor public error boundary.

### Registry Integrity

1. Registry has no duplicate codes.
2. Every code has class, retry policy, commit outcome, suggested fix, docs slug,
   and details schema field.
3. Every docs slug maps to the stable code anchor.
4. Every details schema reference is non-empty and versioned.

### Runtime Coverage

1. Every engine-next emitted public code exists in the registry.
2. Every executor-next emitted public code exists in the registry.
3. Batch item error codes are registered.
4. Inference provider error codes are registered.
5. Arrow import/export error codes are registered.

Implemented coverage:

1. `crates/engine-next/src/diagnostics/registry.rs` scans engine source files
   and proves every emitted `.engine.` code is registered.
2. `crates/executor-next/tests/error_and_guards.rs` scans executor source files
   and proves every emitted `.executor.` code is registered.
3. The same executor guard scans `crates/inference-next/src/error.rs` for
   `inference.*` codes and proves they are registered at the executor boundary.

### Rendering

1. Executor-rendered status matches registry defaults.
2. Runtime-specific details may add context but must not change stable code
   semantics.
3. Docs URL generated from registry resolves to a fixture or docs target.
4. Reference IDs are present and stable in shape.
5. Redaction tests prove secrets and filesystem internals do not leak.

Implemented coverage:

1. Executor rendering uses registry defaults for class, retry policy, commit
   outcome, suggested fix, and docs slug.
2. Unregistered executor-rendered codes are normalized to
   `internal.executor.unregistered_code` before they can cross the public
   boundary.
3. Docs URL guards verify the registry page target for every registry entry.

### Retry Policy

1. Invalid input is not retryable.
2. Not found is not retryable unless a documented eventual-consistency case
   exists.
3. Conflict is not retryable without request changes.
4. Resource exhausted may be retryable after backoff only when documented.
5. Transient provider/network failures are retryable according to provider
   policy.
6. Ambiguous commit errors expose `maybe_committed`.

## IDL, SDK, CLI, MCP Conformance Tests

### IDL

1. Every `Command` variant maps to one request model.
2. Every `Output` variant maps to one response model.
3. Shared concepts are defined once.
4. No public model exposes lower-layer implementation details.
5. Generated schema validates all golden fixtures.

### SDKs

Run the same response questions in Python, TypeScript, Rust, and Go examples:

1. `response.applied()`
2. `response.commit()`
3. `response.found()`
4. `response.next_cursor()`
5. `response.items()`
6. `response.failed_items()`
7. `response.retryable()`
8. `response.error_code()`

Each SDK should cover:

1. successful mutation;
2. missing delete;
3. optional read found/missing;
4. continued page;
5. partial itemwise batch;
6. top-level error;
7. batch item error.

### CLI

1. `--json` output matches golden fixtures.
2. Human output includes applied/commit/found/page/error facts.
3. Human output does not print raw internal implementation details.
4. Non-zero exit codes are stable for top-level failures.
5. Partial itemwise batch succeeds at command level but reports failed items.

### MCP And Agent Examples

1. Agent can retry only when `retryable=true`.
2. Agent can continue pagination without knowing primitive names.
3. Agent can summarize partial batch failures from item errors.
4. Agent can distinguish missing read from present null.
5. Agent can cite `reference_id` and `code` in support-facing output.

## Regression Guard Tests

Add source-level guard tests for:

1. no public response type imports storage-next internals;
2. no public response fixture contains internal control-plane paths;
3. no public response fixture contains artifact file paths;
4. no public error code is emitted as a raw string outside registry helpers;
5. no command output serializes string-only item errors;
6. no optional JSON read serializes missing and stored null identically;
7. no page serializes `has_more=true` with missing cursor.

## Required Test Commands

Minimum local gate for this milestone:

```sh
cargo fmt --check
cargo test -p strata-executor-next --features inference
cargo test -p strata-engine-next
cargo test -p strata-core-next
cargo test -p strata-inference-next
cargo clippy -p strata-engine-next -p strata-executor-next -p strata-core-next -p strata-inference-next --features strata-executor-next/inference --all-targets -- -D warnings
git diff --check
```

Before V1 freeze, add SDK and CLI commands once those packages exist.

## Exit Criteria

This milestone is complete when:

1. pagination is normalized and golden-tested;
2. batch wrappers are normalized and golden-tested;
3. optional reads have one SDK-facing `Maybe` model;
4. golden fixtures cover every public response family;
5. all emitted error codes are registered and have docs targets;
6. IDL validates every fixture;
7. SDKs and CLI pass conformance tests;
8. response fixtures contain no lower-layer implementation details;
9. CI fails on accidental response contract drift.
