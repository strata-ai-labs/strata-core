# V1 Response Quality Readiness Assessment

## Status

Current assessment after the V1 error-contract work and the first success
response-contract implementation slice.

Related documents:

1. `docs/architecture/strata-sdk-quality-playbook.md`
2. `docs/architecture/v1-error-and-diagnostics-contract.md`
3. `docs/architecture/implementation-plans/v1-response-error-contract-implementation-plan.md`
4. `docs/architecture/implementation-plans/v1-success-response-contract-implementation-plan.md`

## Bottom Line

Strata does not yet have everything needed to freeze the V1 response contract.

The failure path is close to the V1 bar. Engine and executor now expose stable
error classes, codes, retry policy, commit outcome, user messages, suggested
fixes, docs URLs, reference IDs, optional trace IDs, details, and hints.

The success path is not yet at the same quality bar. Engine and storage usually
have the facts needed to produce good success responses, but executor still
presents many of those facts through command-specific output variants. SDKs,
CLIs, MCP tools, and AI agents would still need command-specific inference to
answer basic questions:

1. Did this mutation apply?
2. Was this operation a no-op?
3. Did it miss a target?
4. What commit was produced?
5. Was the commit durable?
6. How should pagination continue?
7. Which batch items succeeded, failed, or missed?

Do not freeze the V1 IDL until success responses are normalized.

## Response Quality Bar

A V1 operation should produce one of two high-quality response shapes.

### Successful Operation

The caller should know:

1. operation identity;
2. whether the operation applied a mutation;
3. whether the operation matched existing logical state;
4. the commit receipt when a mutation committed;
5. returned data, if any;
6. missing versus present state without ambiguity;
7. pagination continuation facts through one common shape;
8. per-item batch status through one common shape;
9. primitive-specific diagnostics where useful;
10. no lower-layer implementation details.

### Failed Operation

The caller should know:

1. stable public class;
2. stable public code;
3. retry policy;
4. commit outcome;
5. human-readable message;
6. suggested fix;
7. docs URL;
8. reference ID;
9. optional trace ID;
10. structured details and hints.

## Current Failure Readiness

### Engine

Engine now has the core V1 failure concepts:

1. public error class;
2. stable code;
3. retry policy;
4. commit outcome;
5. message;
6. suggested fix;
7. details;
8. hints;
9. lower-layer source preservation.

Storage errors are mapped through the persistence adapter into engine-level V1
status facts. Ambiguous commit failures can preserve `maybe_committed` rather
than collapsing into generic IO or internal errors.

### Executor

Executor now preserves and renders the V1 failure status at the public boundary:

1. class;
2. code;
3. retry policy;
4. commit outcome;
5. message;
6. suggested fix;
7. docs URL;
8. generated reference ID;
9. optional trace ID;
10. details and hints.

Executor also has guard tests that protect the serialized public shape.

### Remaining Failure Work

The structure is good enough to build against, but the product still needs:

1. a published V1 error code registry;
2. docs pages for the error-code docs URLs;
3. final review of retry policies by code;
4. structured batch item errors instead of string-only item failures;
5. SDK mapping tests for every error class and representative codes.

These are completion tasks, not structural blockers for the error model.

## Current Success Readiness

### Storage

Storage already has the low-level facts needed by engine:

1. commit version;
2. commit timestamp;
3. durability information;
4. ambiguous commit classification on failure;
5. versioned read facts;
6. cursor facts for range reads;
7. branch and source identity needed internally.

Storage should not expose SDK-facing response vocabulary. It should remain a
mechanical persistence boundary.

### Engine

Engine has many product-level success facts:

1. `CommitOutcome` carries version, timestamp, durability, put count, and delete
   count.
2. Primitive write outcomes generally preserve commit version and timestamp.
3. Space/admin outcomes can distinguish mutation from no-op in several paths.
4. Read models preserve version/timestamp facts where versioned reads are
   supported.
5. Page models expose items, `has_more`, and cursor facts.
6. Vector index diagnostics are available for search behavior.

Engine gaps:

1. not every write outcome exposes a full `CommitOutcome`;
2. no-op versus miss versus unchanged is not represented through one shared
   mutation model;
3. batch item outcomes are primitive-specific;
4. durability is available in commits but not consistently surfaced through
   executor-facing outcomes;
5. success diagnostics are not uniformly categorized.

### Executor

Executor currently has stable tagged outputs and serde round-trip tests.

Recent improvements:

1. `CommitReceipt` exists as a public shared type.
2. `MutationEffect` exists as a public shared type.
3. JSON point reads distinguish missing from stored JSON `null`.
4. JSON versioned reads distinguish missing from stored JSON `null`.
5. JSON batch gets distinguish missing from stored JSON `null`.
6. KV and JSON point write/delete outputs expose `commit` and `effect`.
7. KV and JSON batch write/delete item results expose `commit` and `effect`.

Executor gaps:

1. Vector, event, graph, space, and admin write outputs have not all migrated
   to the shared success concepts yet.
2. Batch outputs still use primitive-specific item shapes.
3. Batch item failures are mostly strings, not structured public errors.
4. Page outputs still use command-specific variants.
5. Golden JSON snapshots cover only selected shapes.
6. SDK-ready response models are not yet generated or enforced.

## Readiness Matrix

| Area | Status | Notes |
| --- | --- | --- |
| Top-level error shape | Ready | Structure is stable and tested. |
| Engine error mapping | Ready | Storage and engine errors preserve V1 facts. |
| Executor error rendering | Ready | Adds docs URL and reference ID at boundary. |
| Error code registry | Partial | Codes exist in code, but registry/docs are not final. |
| Commit receipt type | Partial | Type exists and is wired for KV/JSON point and batch write/delete outputs; other primitives remain. |
| Mutation effect type | Partial | Type exists and is wired for KV/JSON point and batch write/delete outputs; other primitives remain. |
| JSON missing/null distinction | Ready | Fixed for point reads and batch gets. |
| KV optional reads | Acceptable | Bytes do not have JSON null ambiguity, but IDL shape is still command-specific. |
| Vector optional reads | Partial | Semantics are clear in Rust, but not normalized with a shared `Maybe`. |
| Pagination | Partial | Common fields exist, common public model does not. |
| Batch success items | Partial | KV/JSON write items now include shared commit/effect facts, but batch wrappers remain primitive-specific. |
| Batch item failures | Not ready | String-only failures are below the V1 error bar. |
| Success diagnostics | Partial | Vector is strong; other primitives are mostly plain acknowledgements. |
| Golden response snapshots | Partial | Serde round-trip is broad; golden contract coverage is not. |
| SDK response ergonomics | Not ready | SDKs would still need command-specific inference. |

## Required V1 Success Concepts

The V1 IDL should expose these once and reuse them across command families.

### CommitReceipt

Use for every successful committed mutation:

```text
CommitReceipt {
  version: u64,
  timestamp: u64,
  durable: bool,
  put_count: u64,
  delete_count: u64
}
```

### MutationEffect

Use for every write, delete, patch, create, drop, and no-op operation:

```text
MutationEffect {
  applied: bool,
  kind: created | updated | deleted | unchanged | not_found,
  matched: bool,
  affected_count: u64
}
```

### Maybe<T>

Use for optional reads where the returned value can be validly null or empty:

```text
Maybe<T> {
  found: bool,
  value: T | null
}
```

JSON must use this shape. Other primitives can use concrete primitive-specific
wrappers if the IDL generator cannot express generics cleanly.

### Page<T, Cursor>

Use for every paginated read:

```text
Page<T, Cursor> {
  items: [T],
  has_more: bool,
  cursor: Cursor | null
}
```

Primitive outputs can name the item field more clearly in SDK facades, but the
wire contract should be uniform before V1 freeze unless there is a strong
reason not to normalize it.

### BatchItem<T>

Use for every batch operation:

```text
BatchItem<T> {
  index: u64,
  status: applied | unchanged | not_found | failed,
  result: T | null,
  error: ErrorStatus | null
}
```

Top-level batch commands should not use string-only item failures in V1.

## Implementation Order

### 1. Freeze Shared Success Types

Finalize the public shape for:

1. `CommitReceipt`;
2. `MutationEffect`;
3. maybe wrappers;
4. page wrappers;
5. batch item wrappers.

Add golden JSON tests for each shared type.

### 2. Wire KV and JSON Writes

Status: implemented for KV/JSON point write/delete and KV/JSON batch
write/delete item success facts. Remaining work in this area is to move
item-level failures to structured public errors and to decide whether the
outer batch wrappers should be normalized before IDL freeze.

Migrate:

1. `WriteResult`;
2. `DeleteResult`;
3. `JsonBatchResults`;
4. `BatchResults`;
5. JSON set/delete point outputs.

Exit criteria:

1. every KV/JSON write has `commit` when applied;
2. every delete/no-op has `effect`;
3. durability is exposed when a commit exists;
4. existing behavior tests pass through the new shape.

### 3. Normalize Batch Item Failures

Replace string-only batch item errors with structured public error status.

Exit criteria:

1. top-level and item-level failures share the same public error vocabulary;
2. item failures include class, code, retry policy, commit outcome, message, and
   suggested fix;
3. positional ordering remains stable.

### 4. Normalize Pagination

Migrate list/range/page outputs across:

1. KV;
2. JSON;
3. vector;
4. event;
5. graph;
6. branch/space/admin list outputs where applicable.

Exit criteria:

1. generated SDKs expose one page concept;
2. cursors remain opaque;
3. terminal and continued page behavior is golden-tested.

### 5. Wire Vector, Event, and Graph Writes

Migrate:

1. vector create/upsert/update/delete/delete-by-filter/delete-all;
2. event append/batch append;
3. graph node/edge/batch writes and deletes.

Exit criteria:

1. write outputs expose `commit`;
2. mutation/no-op/miss outcomes expose `effect`;
3. primitive-specific returned data remains available without losing the common
   envelope.

### 6. Add Full Response Golden Snapshots

For every command family, add stable JSON snapshots for:

1. applied mutation;
2. no-op mutation;
3. missing read;
4. present read;
5. paginated read with continuation;
6. terminal page;
7. partial batch success;
8. batch item failure;
9. top-level failure.

### 7. SDK and CLI Conformance

Before V1 freeze, generated SDKs and the CLI should prove they can answer:

1. whether the operation applied;
2. whether the operation committed;
3. the commit version/timestamp/durability;
4. whether a read found data;
5. how to continue pagination;
6. which batch items failed and why;
7. whether retrying is safe.

## Do Not Do

1. Do not push SDK-facing response concepts into storage.
2. Do not expose storage row families, control keys, artifact paths, or branch
   internal IDs in success responses.
3. Do not rely on `Option<serde_json::Value>` for public JSON reads.
4. Do not freeze command-shaped success variants as the V1 SDK contract.
5. Do not let batch item errors remain string-only in the final V1 IDL.

## V1 Freeze Gates

The response contract is ready to freeze only when:

1. every top-level failure serializes as `ErrorStatus`;
2. every batch item failure serializes as `ErrorStatus`;
3. every applied mutation exposes `CommitReceipt`;
4. every no-op/miss mutation exposes `MutationEffect`;
5. every optional JSON read distinguishes missing from JSON `null`;
6. every page uses one continuation contract;
7. every public response shape has a golden JSON test;
8. SDK facades do not need command-specific inference for common facts;
9. CLI/MCP output can render the same success/error facts consistently;
10. docs describe the stable success and error contracts.

## Recommendation

Proceed with the remaining success-response work before building the final IDL,
SDKs, CLI, and public docs.

The engine and storage are close enough. The response-quality gap is now
primarily at the executor and IDL boundary. Fixing it there keeps storage
mechanical, keeps engine product-focused, and gives downstream users the
predictable response model expected from a high-quality database product.
