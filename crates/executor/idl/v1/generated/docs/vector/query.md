---
title: "Query vectors"
description: "Search a vector collection."
source: strata-core@1.0.0
section: vector
---

Runs vector search through the engine planner and returns the best matches with scores and optional metadata.

Search responses return a bounded list of matches ordered by the engine. They are not cursor pages unless a later command explicitly advertises pagination.

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

`SearchResult<VectorMatch>`.

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

- CLI: `strata vector query`
- Wire type: `vector_query`
