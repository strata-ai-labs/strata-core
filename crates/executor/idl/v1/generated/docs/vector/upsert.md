---
title: "Upsert vector"
description: "Insert or replace one vector."
source: strata-core@1.0.0
section: vector
---

Upserts one vector key with a dense embedding and optional metadata. The vector dimension must match the collection configuration.

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Examples

Insert or replace a vector with optional metadata.

### CLI

```console
$ strata vector collection create docs 3 cosine
$ strata vector upsert docs a [1.0,0.0,0.0] --metadata {"tag":"x"}
$ strata vector exists docs a
```

### Wire

```json
{"collection":"docs","dimension":3,"metric":"cosine","type":"vector_create_collection"}
{"collection":"docs","key":"a","metadata":{"tag":"x"},"type":"vector_upsert","vector":[1.0,0.0,0.0]}
{"collection":"docs","key":"a","type":"vector_exists"}
```

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `collection` | `string` | yes | Collection name. |
| `key` | `string` | yes | Vector key. |
| `metadata` | `any` | no | Optional metadata. |
| `vector` | `number[]` | yes | Dense embedding. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<VectorWrite>`.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space)
- [`invalid_argument.engine.vector_collection`](https://stratadb.org/e/invalid_argument.engine.vector_collection)
- [`invalid_argument.engine.vector_key`](https://stratadb.org/e/invalid_argument.engine.vector_key)
- [`not_found.engine.vector_collection`](https://stratadb.org/e/not_found.engine.vector_collection)
- [`invalid_argument.engine.vector_dimension`](https://stratadb.org/e/invalid_argument.engine.vector_dimension)
- [`invalid_argument.engine.vector_embedding`](https://stratadb.org/e/invalid_argument.engine.vector_embedding)
- [`invalid_argument.engine.vector_metadata`](https://stratadb.org/e/invalid_argument.engine.vector_metadata)
- [`invalid_argument.executor.vector_dimension`](https://stratadb.org/e/invalid_argument.executor.vector_dimension)

## Invocation

- CLI: `strata vector upsert`
- Wire type: `vector_upsert`
