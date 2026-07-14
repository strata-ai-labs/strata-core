---
title: "Update vector metadata"
description: "Patch metadata for one vector."
source: strata-core@1.0.0
section: vector
---

Applies a top-level metadata patch to one visible vector. Missing vectors return a no-op mutation acknowledgement.

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `collection` | `string` | yes | Collection name. |
| `key` | `string` | yes | Vector key. |
| `patch` | `any` | yes | Top-level metadata patch. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<VectorMetadataUpdate>`.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space)
- [`invalid_argument.engine.vector_collection`](https://stratadb.org/e/invalid_argument.engine.vector_collection)
- [`invalid_argument.engine.vector_key`](https://stratadb.org/e/invalid_argument.engine.vector_key)
- [`not_found.engine.vector_collection`](https://stratadb.org/e/not_found.engine.vector_collection)
- [`invalid_argument.engine.vector_metadata_patch`](https://stratadb.org/e/invalid_argument.engine.vector_metadata_patch)

## Invocation

- CLI: `strata vector update-metadata`
- Wire type: `vector_update_metadata`
