---
title: "Batch get vectors"
description: "Read multiple vectors by key."
source: strata-core@1.0.0
section: vector
---

Reads several vector keys and returns positional item results. Each item records found or missing state.

Itemwise batches return one positional item result per input item. The outer batch status summarizes whether all, some, or none of the items succeeded.

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `collection` | `string` | yes | Collection name. |
| `keys` | `string[]` | yes | Keys to read. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`BatchResult<Maybe<VectorVersionedData>>`.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space)
- [`invalid_argument.engine.vector_collection`](https://stratadb.org/e/invalid_argument.engine.vector_collection)
- [`invalid_argument.engine.vector_key`](https://stratadb.org/e/invalid_argument.engine.vector_key)
- [`not_found.engine.vector_collection`](https://stratadb.org/e/not_found.engine.vector_collection)
- [`invalid_argument.engine.vector_batch`](https://stratadb.org/e/invalid_argument.engine.vector_batch)

## Invocation

- CLI: via `strata command run` (no dedicated verb)
- Wire type: `vector_batch_get`
