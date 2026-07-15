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

### Output

One response per step, in order:

```json
{"data":{"cursor":null,"has_more":false,"items":[{"count":0,"dimension":3,"metric":"cosine","name":"docs"}]},"type":"vector_collection_list"}
{"data":{"collection":"docs","commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"key":"a","vector_revision":1},"type":"vector_write_result"}
{"data":true,"type":"bool"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `collection` | `string` | yes | — | Collection name. |
| `key` | `string` | yes | — | Vector key. |
| `vector` | `number[]` | yes | — | Dense embedding. |
| `metadata` | `any` | no | — | Optional metadata. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<VectorWrite>`.

| Field | Type | Description |
|---|---|---|
| `collection` | `string` | Collection name. |
| `commit` | `CommitReceipt` | Commit receipt. |
| `effect` | `MutationEffect` | Mutation effect facts. |
| `key` | `string` | Vector key. |
| `vector_revision` | `integer` | Product vector revision. |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.vector_collection`](https://stratadb.org/e/invalid_argument.engine.vector_collection) | The vector request is invalid. |
| [`invalid_argument.engine.vector_key`](https://stratadb.org/e/invalid_argument.engine.vector_key) | The vector request is invalid. |
| [`not_found.engine.vector_collection`](https://stratadb.org/e/not_found.engine.vector_collection) | The requested vector collection was not found. |
| [`invalid_argument.engine.vector_dimension`](https://stratadb.org/e/invalid_argument.engine.vector_dimension) | The vector request is invalid. |
| [`invalid_argument.engine.vector_embedding`](https://stratadb.org/e/invalid_argument.engine.vector_embedding) | The vector request is invalid. |
| [`invalid_argument.engine.vector_metadata`](https://stratadb.org/e/invalid_argument.engine.vector_metadata) | The vector request is invalid. |
| [`invalid_argument.executor.vector_dimension`](https://stratadb.org/e/invalid_argument.executor.vector_dimension) | The vector dimension is invalid. |

## Invocation

```text
strata vector upsert <collection> <key> <vector> [--metadata <any>] [--branch <branch>] [--space <space>]
```

- Wire type: `vector_upsert`

## Related

- [Create vector collection](/docs/vector/collection/create) — Create a vector collection with a dimension and metric.
- [Check vector existence](/docs/vector/exists) — Check whether one vector key exists.
- [All `vector` commands](/docs/vector/)
