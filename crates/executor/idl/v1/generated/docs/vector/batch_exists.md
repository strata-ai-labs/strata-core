---
title: "Batch check vector existence"
description: "Check existence for multiple vector keys."
source: strata-core@1.0.0
section: vector
---

Checks several vector keys in one collection and returns positional boolean status values. The response preserves the input order.

Itemwise batches return one positional item result per input item. The outer batch status summarizes whether all, some, or none of the items succeeded.

## Examples

Check existence for many keys at once.

### CLI

```console
$ strata vector collection create docs 3 cosine
$ strata command run --command-json '{"collection":"docs","entries":[{"key":"a","vector":[1.0,0.0,0.0]}],"type":"vector_batch_upsert"}'
$ strata command run --command-json '{"collection":"docs","keys":["a","missing"],"type":"vector_batch_exists"}'
```

### Wire

```json
{"collection":"docs","dimension":3,"metric":"cosine","type":"vector_create_collection"}
{"collection":"docs","entries":[{"key":"a","vector":[1.0,0.0,0.0]}],"type":"vector_batch_upsert"}
{"collection":"docs","keys":["a","missing"],"type":"vector_batch_exists"}
```

### Output

One response per step, in order:

```json
{"data":{"cursor":null,"has_more":false,"items":[{"count":0,"dimension":3,"metric":"cosine","name":"docs"}]},"type":"vector_collection_list"}
{"data":{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":4,"version":4},"items":[{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":0,"result":{"vector_revision":1},"status":"ok"}],"mode":"itemwise","status":"ok"},"type":"vector_batch_upsert_results"}
{"data":{"applied":false,"commit":null,"items":[{"applied":false,"commit":null,"effect":null,"error":null,"index":0,"result":{"exists":true},"status":"ok"},{"applied":false,"commit":null,"effect":null,"error":null,"index":1,"result":{"exists":false},"status":"ok"}],"mode":"itemwise","status":"ok"},"type":"vector_batch_exists_results"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `collection` | `string` | yes | — | Collection name. |
| `keys` | `string[]` | yes | — | Vector keys to check. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`BatchResult<BatchExistsPresence>`.

| Field | Type | Description |
|---|---|---|
| `applied` | `boolean` |  |
| `items` | `BatchItem6[]` |  |
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

## Invocation

- CLI: via `strata command run` (no dedicated verb)
- Wire type: `vector_batch_exists`

## Related

- [Batch upsert vectors](/docs/vector/batch_upsert) — Upsert multiple vectors in one itemwise batch.
- [Create vector collection](/docs/vector/collection/create) — Create a vector collection with a dimension and metric.
- [All `vector` commands](/docs/vector/)
