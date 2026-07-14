---
title: "Query vector index"
description: "Search vectors and return index diagnostics."
source: strata-core@1.0.0
section: vector
---

Runs vector search and includes planner diagnostics such as index policy, source usage, artifact status, and fallback facts.

Search responses return a bounded list of matches ordered by the engine. They are not cursor pages unless a later command explicitly advertises pagination.

Diagnostic responses include operational facts intended for debugging and tuning. They should not be required for application correctness.

## Examples

Nearest-neighbor search that also returns index diagnostics.

### CLI

```console
$ strata vector collection create docs 3 cosine
$ strata vector upsert docs a [1.0,0.0,0.0]
$ strata vector upsert docs b [0.0,1.0,0.0]
$ strata command run --command-json '{"collection":"docs","k":2,"query":[1.0,0.0,0.0],"type":"vector_index_query"}'
```

### Wire

```json
{"collection":"docs","dimension":3,"metric":"cosine","type":"vector_create_collection"}
{"collection":"docs","key":"a","type":"vector_upsert","vector":[1.0,0.0,0.0]}
{"collection":"docs","key":"b","type":"vector_upsert","vector":[0.0,1.0,0.0]}
{"collection":"docs","k":2,"query":[1.0,0.0,0.0],"type":"vector_index_query"}
```

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `as_of` | `integer` | no | Optional timestamp in microseconds. |
| `collection` | `string` | yes | Collection name. |
| `filter` | `VectorMetadataFilter` | no | Optional metadata filter. |
| `k` | `integer` | yes | Maximum number of matches. |
| `query` | `number[]` | yes | Query embedding. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`SearchResult<VectorMatch> + IndexDiagnostics`.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space)
- [`invalid_argument.engine.vector_collection`](https://stratadb.org/e/invalid_argument.engine.vector_collection)
- [`invalid_argument.engine.vector_key`](https://stratadb.org/e/invalid_argument.engine.vector_key)
- [`not_found.engine.vector_collection`](https://stratadb.org/e/not_found.engine.vector_collection)
- [`invalid_argument.engine.vector_filter`](https://stratadb.org/e/invalid_argument.engine.vector_filter)
- [`invalid_argument.executor.vector_limit`](https://stratadb.org/e/invalid_argument.executor.vector_limit)

## Invocation

- CLI: via `strata command run` (no dedicated verb)
- Wire type: `vector_index_query`
