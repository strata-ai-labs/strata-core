---
title: "Batch delete vectors"
description: "Delete multiple vectors by key."
source: strata-core@1.0.0
section: vector
---

Deletes multiple vector keys and returns one positional mutation result per input key.

Itemwise batches return one positional item result per input item. The outer batch status summarizes whether all, some, or none of the items succeeded.

## Examples

Delete many vectors in one commit.

### CLI

```console
$ strata vector collection create docs 3 cosine
$ strata command run --command-json '{"collection":"docs","entries":[{"key":"a","vector":[1.0,0.0,0.0]},{"key":"b","vector":[0.0,1.0,0.0]}],"type":"vector_batch_upsert"}'
$ strata command run --command-json '{"collection":"docs","keys":["a","b"],"type":"vector_batch_delete"}'
$ strata vector count docs
```

### Wire

```json
{"collection":"docs","dimension":3,"metric":"cosine","type":"vector_create_collection"}
{"collection":"docs","entries":[{"key":"a","vector":[1.0,0.0,0.0]},{"key":"b","vector":[0.0,1.0,0.0]}],"type":"vector_batch_upsert"}
{"collection":"docs","keys":["a","b"],"type":"vector_batch_delete"}
{"collection":"docs","type":"vector_count"}
```

### Output

One response per step, in order:

```json
{"data":{"cursor":null,"has_more":false,"items":[{"count":0,"dimension":3,"metric":"cosine","name":"docs"}]},"type":"vector_collection_list"}
{"data":{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":2,"timestamp":4,"version":4},"items":[{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":2,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":0,"result":{"vector_revision":1},"status":"ok"},{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":2,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":1,"result":{"vector_revision":1},"status":"ok"}],"mode":"itemwise","status":"ok"},"type":"vector_batch_upsert_results"}
{"data":{"applied":true,"commit":{"delete_count":2,"durable":false,"put_count":0,"timestamp":5,"version":5},"items":[{"applied":true,"commit":{"delete_count":2,"durable":false,"put_count":0,"timestamp":5,"version":5},"effect":{"affected_count":1,"applied":true,"kind":"deleted","matched":true},"error":null,"index":0,"result":{"vector_revision":null},"status":"ok"},{"applied":true,"commit":{"delete_count":2,"durable":false,"put_count":0,"timestamp":5,"version":5},"effect":{"affected_count":1,"applied":true,"kind":"deleted","matched":true},"error":null,"index":1,"result":{"vector_revision":null},"status":"ok"}],"mode":"itemwise","status":"ok"},"type":"vector_batch_delete_results"}
{"data":0,"type":"uint"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `collection` | `string` | yes | — | Collection name. |
| `keys` | `string[]` | yes | — | Keys to delete. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`BatchResult<VectorMutationItem>`.

| Field | Type | Description |
|---|---|---|
| `applied` | `boolean` |  |
| `items` | `BatchItem7[]` |  |
| `mode` | `BatchMode` |  |
| `status` | `BatchStatus` |  |
| `commit` | `CommitReceipt` |  |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.vector_collection`](https://stratadb.org/e/invalid_argument.engine.vector_collection) | The vector request is invalid. |
| [`invalid_argument.engine.vector_key`](https://stratadb.org/e/invalid_argument.engine.vector_key) | The vector request is invalid. |
| [`not_found.engine.vector_collection`](https://stratadb.org/e/not_found.engine.vector_collection) | The requested vector collection was not found. |
| [`invalid_argument.engine.vector_batch`](https://stratadb.org/e/invalid_argument.engine.vector_batch) | The vector request is invalid. |
| [`invalid_argument.executor.vector_batch_duplicate_key`](https://stratadb.org/e/invalid_argument.executor.vector_batch_duplicate_key) | The vector batch contains duplicate keys. |

## Invocation

- CLI: via `strata command run` (no dedicated verb)
- Wire type: `vector_batch_delete`

## Related

- [Batch upsert vectors](/docs/vector/batch_upsert) — Upsert multiple vectors in one itemwise batch.
- [Create vector collection](/docs/vector/collection/create) — Create a vector collection with a dimension and metric.
- [Count vectors](/docs/vector/count) — Count visible vectors in a collection.
- [All `vector` commands](/docs/vector/)
