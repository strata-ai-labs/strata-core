---
title: "Sample vectors"
description: "Sample vectors with values and version facts."
source: strata-core@1.0.0
section: vector
---

Returns a deterministic representative sample of vectors from a collection: the total live count and up to `count` entries (default 10), evenly strided over the ordered keys. Latest state only.

Paginated responses use opaque cursors. Clients should pass the returned cursor back to the same command shape and must not parse cursor contents.

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `collection` | `string` | yes | Collection name. |
| `count` | `integer` | no | Optional sample count. Defaults to 10. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`SamplePage<VectorVersionedData>`.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space)
- [`invalid_argument.engine.vector_collection`](https://stratadb.org/e/invalid_argument.engine.vector_collection)
- [`invalid_argument.engine.vector_key`](https://stratadb.org/e/invalid_argument.engine.vector_key)
- [`not_found.engine.vector_collection`](https://stratadb.org/e/not_found.engine.vector_collection)

## Invocation

- CLI: `strata vector sample`
- Wire type: `vector_sample`
