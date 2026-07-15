---
title: "Update vector metadata"
description: "Patch metadata for one vector."
source: strata-core@1.0.0
section: vector
---

Applies a top-level metadata patch to one visible vector. Missing vectors return a no-op mutation acknowledgement.

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Examples

Patch the metadata of an existing vector.

### CLI

```console
$ strata vector collection create docs 3 cosine
$ strata vector upsert docs a [1.0,0.0,0.0] --metadata {"tag":"x"}
$ strata vector update-metadata docs a {"tag":"z"}
$ strata vector get docs a
```

### Wire

```json
{"collection":"docs","dimension":3,"metric":"cosine","type":"vector_create_collection"}
{"collection":"docs","key":"a","metadata":{"tag":"x"},"type":"vector_upsert","vector":[1.0,0.0,0.0]}
{"collection":"docs","key":"a","patch":{"tag":"z"},"type":"vector_update_metadata"}
{"collection":"docs","key":"a","type":"vector_get"}
```

### Output

One response per step, in order:

```json
{"data":{"cursor":null,"has_more":false,"items":[{"count":0,"dimension":3,"metric":"cosine","name":"docs"}]},"type":"vector_collection_list"}
{"data":{"collection":"docs","commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"key":"a","vector_revision":1},"type":"vector_write_result"}
{"data":{"collection":"docs","commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":5,"version":5},"effect":{"affected_count":1,"applied":true,"kind":"updated","matched":true},"key":"a","vector_revision":2},"type":"vector_metadata_update_result"}
{"data":{"found":true,"value":{"data":{"embedding":[1.0,0.0,0.0],"metadata":{"tag":"z"}},"key":"a","timestamp":5,"vector_revision":2,"version":5}},"type":"vector_data"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `collection` | `string` | yes | — | Collection name. |
| `key` | `string` | yes | — | Vector key. |
| `patch` | `any` | yes | — | Top-level metadata patch. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<VectorMetadataUpdate>`.

| Field | Type | Description |
|---|---|---|
| `collection` | `string` | Collection name. |
| `effect` | `MutationEffect` | Mutation effect facts. |
| `key` | `string` | Vector key. |
| `commit` | `CommitReceipt` | Commit receipt when an update was applied. |
| `vector_revision` | `integer` | Product vector revision when an update was applied. |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.vector_collection`](https://stratadb.org/e/invalid_argument.engine.vector_collection) | The vector request is invalid. |
| [`invalid_argument.engine.vector_key`](https://stratadb.org/e/invalid_argument.engine.vector_key) | The vector request is invalid. |
| [`not_found.engine.vector_collection`](https://stratadb.org/e/not_found.engine.vector_collection) | The requested vector collection was not found. |
| [`invalid_argument.engine.vector_metadata_patch`](https://stratadb.org/e/invalid_argument.engine.vector_metadata_patch) | The vector request is invalid. |

## Invocation

```text
strata vector update-metadata <collection> <key> <patch> [--branch <branch>] [--space <space>]
```

- Wire type: `vector_update_metadata`

## Related

- [Create vector collection](/docs/vector/collection/create) — Create a vector collection with a dimension and metric.
- [Get vector](/docs/vector/get) — Read one vector by key.
- [Upsert vector](/docs/vector/upsert) — Insert or replace one vector.
- [All `vector` commands](/docs/vector/)
