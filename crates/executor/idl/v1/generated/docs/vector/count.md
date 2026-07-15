---
title: "Count vectors"
description: "Count visible vectors in a collection."
source: strata-core@1.0.0
section: vector
---

Counts vectors visible in the selected collection, branch, and space.

Status commands return a scalar or compact status payload and do not mutate database state.

## Examples

Count vectors in a collection.

### CLI

```console
$ strata vector collection create docs 3 cosine
$ strata vector upsert docs a [1.0,0.0,0.0]
$ strata vector upsert docs b [0.0,1.0,0.0]
$ strata vector count docs
```

### Wire

```json
{"collection":"docs","dimension":3,"metric":"cosine","type":"vector_create_collection"}
{"collection":"docs","key":"a","type":"vector_upsert","vector":[1.0,0.0,0.0]}
{"collection":"docs","key":"b","type":"vector_upsert","vector":[0.0,1.0,0.0]}
{"collection":"docs","type":"vector_count"}
```

### Output

One response per step, in order:

```json
{"data":{"cursor":null,"has_more":false,"items":[{"count":0,"dimension":3,"metric":"cosine","name":"docs"}]},"type":"vector_collection_list"}
{"data":{"collection":"docs","commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"key":"a","vector_revision":1},"type":"vector_write_result"}
{"data":{"collection":"docs","commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":5,"version":5},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"key":"b","vector_revision":1},"type":"vector_write_result"}
{"data":2,"type":"uint"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `collection` | `string` | yes | — | Collection name. |
| `as_of` | `integer` | no | — | Optional timestamp in microseconds. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`StatusValue<u64>`.

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
strata vector count <collection> [--as-of <integer>] [--branch <branch>] [--space <space>]
```

- Wire type: `vector_count`

## Related

- [Create vector collection](/docs/vector/collection/create) — Create a vector collection with a dimension and metric.
- [Upsert vector](/docs/vector/upsert) — Insert or replace one vector.
- [All `vector` commands](/docs/vector/)
