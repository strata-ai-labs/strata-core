---
title: "Count vectors"
description: "Count visible vectors in a collection."
source: strata-core@1.0.0
section: vector
---

Counts vectors visible in the selected collection, branch, and space.

Status commands return a scalar or compact status payload and do not mutate database state.

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `as_of` | `integer` | no | Optional timestamp in microseconds. |
| `collection` | `string` | yes | Collection name. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`StatusValue<u64>`.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space)
- [`invalid_argument.engine.vector_collection`](https://stratadb.org/e/invalid_argument.engine.vector_collection)
- [`invalid_argument.engine.vector_key`](https://stratadb.org/e/invalid_argument.engine.vector_key)
- [`not_found.engine.vector_collection`](https://stratadb.org/e/not_found.engine.vector_collection)

## Invocation

- CLI: `strata vector count`
- Wire type: `vector_count`
