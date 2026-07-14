---
title: "Delete vector"
description: "Delete one vector key."
source: strata-core@1.0.0
section: vector
---

Deletes one visible vector entry from a collection. Missing keys are represented as no-op mutation acknowledgements.

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `collection` | `string` | yes | Collection name. |
| `key` | `string` | yes | Vector key. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<VectorDelete>`.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space)
- [`invalid_argument.engine.vector_collection`](https://stratadb.org/e/invalid_argument.engine.vector_collection)
- [`invalid_argument.engine.vector_key`](https://stratadb.org/e/invalid_argument.engine.vector_key)
- [`not_found.engine.vector_collection`](https://stratadb.org/e/not_found.engine.vector_collection)

## Invocation

- CLI: `strata vector delete`
- Wire type: `vector_delete`
