---
title: "Delete vectors by filter"
description: "Delete vectors matching a metadata filter."
source: strata-core@1.0.0
section: vector
---

Scans the collection for visible vectors matching the metadata filter and deletes the matching rows as a bulk mutation.

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `collection` | `string` | yes | Collection name. |
| `filter` | `VectorMetadataFilter` | yes | Metadata filter. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<VectorBulkDelete>`.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space)
- [`invalid_argument.engine.vector_collection`](https://stratadb.org/e/invalid_argument.engine.vector_collection)
- [`invalid_argument.engine.vector_key`](https://stratadb.org/e/invalid_argument.engine.vector_key)
- [`not_found.engine.vector_collection`](https://stratadb.org/e/not_found.engine.vector_collection)
- [`invalid_argument.engine.vector_filter`](https://stratadb.org/e/invalid_argument.engine.vector_filter)

## Invocation

- CLI: `strata vector delete-by-filter`
- Wire type: `vector_delete_by_filter`
