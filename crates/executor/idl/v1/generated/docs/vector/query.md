---
title: "Query vectors"
description: "Search a vector collection."
source: strata-core@1.0.0
section: vector
---

Runs vector search through the engine planner and returns the best matches with scores and optional metadata.

Search responses return a bounded list of matches ordered by the engine. They are not cursor pages unless a later command explicitly advertises pagination.

## Examples

Find the nearest vectors to a query vector.

### CLI

```console
$ strata vector collection create docs 3 cosine
$ strata vector upsert docs a [1.0,0.0,0.0]
$ strata vector upsert docs b [0.0,1.0,0.0]
$ strata vector query docs [1.0,0.0,0.0] 2
```

### Wire

```json
{"collection":"docs","dimension":3,"metric":"cosine","type":"vector_create_collection"}
{"collection":"docs","key":"a","type":"vector_upsert","vector":[1.0,0.0,0.0]}
{"collection":"docs","key":"b","type":"vector_upsert","vector":[0.0,1.0,0.0]}
{"collection":"docs","k":2,"query":[1.0,0.0,0.0],"type":"vector_query"}
```

### Output

One response per step, in order:

```json
{"data":{"cursor":null,"has_more":false,"items":[{"count":0,"dimension":3,"metric":"cosine","name":"docs"}]},"type":"vector_collection_list"}
{"data":{"collection":"docs","commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"key":"a","vector_revision":1},"type":"vector_write_result"}
{"data":{"collection":"docs","commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":5,"version":5},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"key":"b","vector_revision":1},"type":"vector_write_result"}
{"data":[{"key":"a","score":1.0},{"key":"b","score":0.0}],"type":"vector_matches"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `collection` | `string` | yes | — | Collection name. |
| `k` | `integer` | yes | — | Maximum number of matches. |
| `query` | `number[]` | yes | — | Query embedding. |
| `as_of` | `integer` | no | — | Optional timestamp in microseconds. |
| `filter` | `VectorMetadataFilter` | no | — | Optional metadata filter. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`SearchResult<VectorMatch>`.

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.vector_collection`](https://stratadb.org/e/invalid_argument.engine.vector_collection) | The vector request is invalid. |
| [`invalid_argument.engine.vector_key`](https://stratadb.org/e/invalid_argument.engine.vector_key) | The vector request is invalid. |
| [`not_found.engine.vector_collection`](https://stratadb.org/e/not_found.engine.vector_collection) | The requested vector collection was not found. |
| [`invalid_argument.engine.vector_filter`](https://stratadb.org/e/invalid_argument.engine.vector_filter) | The vector request is invalid. |
| [`invalid_argument.executor.vector_limit`](https://stratadb.org/e/invalid_argument.executor.vector_limit) | The vector query limit is invalid. |

## Invocation

```text
strata vector query <collection> <query> <k> [--as-of <integer>] [--filter <VectorMetadataFilter>] [--branch <branch>] [--space <space>]
```

- Wire type: `vector_query`

## Related

- [Create vector collection](/docs/vector/collection/create) — Create a vector collection with a dimension and metric.
- [Upsert vector](/docs/vector/upsert) — Insert or replace one vector.
- [All `vector` commands](/docs/vector/)
