---
title: "Sample vectors"
description: "Sample vectors with values and version facts."
source: strata-core@1.0.0
section: vector
---

Returns a deterministic representative sample of vectors from a collection: the total live count and up to `count` entries (default 10), evenly strided over the ordered keys. Latest state only.

Paginated responses use opaque cursors. Clients should pass the returned cursor back to the same command shape and must not parse cursor contents.

## Examples

A representative sample of stored vectors plus the total count.

### CLI

```console
$ strata vector collection create docs 3 cosine
$ strata vector upsert docs a [1.0,0.0,0.0]
$ strata vector upsert docs b [0.0,1.0,0.0]
$ strata vector sample docs
```

### Wire

```json
{"collection":"docs","dimension":3,"metric":"cosine","type":"vector_create_collection"}
{"collection":"docs","key":"a","type":"vector_upsert","vector":[1.0,0.0,0.0]}
{"collection":"docs","key":"b","type":"vector_upsert","vector":[0.0,1.0,0.0]}
{"collection":"docs","type":"vector_sample"}
```

### Output

One response per step, in order:

```json
{"data":{"cursor":null,"has_more":false,"items":[{"count":0,"dimension":3,"metric":"cosine","name":"docs"}]},"type":"vector_collection_list"}
{"data":{"collection":"docs","commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"key":"a","vector_revision":1},"type":"vector_write_result"}
{"data":{"collection":"docs","commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":5,"version":5},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"key":"b","vector_revision":1},"type":"vector_write_result"}
{"data":{"cursor":null,"has_more":false,"items":[{"data":{"embedding":[1.0,0.0,0.0]},"key":"a","timestamp":4,"vector_revision":1,"version":4},{"data":{"embedding":[0.0,1.0,0.0]},"key":"b","timestamp":5,"vector_revision":1,"version":5}],"total_count":2},"type":"vector_sample_result"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `collection` | `string` | yes | — | Collection name. |
| `count` | `integer` | no | 10 | Optional sample count. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`SamplePage<VectorVersionedData>`.

| Field | Type | Description |
|---|---|---|
| `has_more` | `boolean` |  |
| `items` | `VectorVersionedData[]` | Sampled vectors. |
| `total_count` | `integer` | Total live vectors in the collection. |
| `cursor` | `string` |  |

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

```text
strata vector sample <collection> [--count <integer>] [--branch <branch>] [--space <space>]
```

- Wire type: `vector_sample`

## Related

- [Create vector collection](/docs/vector/collection/create) — Create a vector collection with a dimension and metric.
- [Upsert vector](/docs/vector/upsert) — Insert or replace one vector.
- [All `vector` commands](/docs/vector/)
