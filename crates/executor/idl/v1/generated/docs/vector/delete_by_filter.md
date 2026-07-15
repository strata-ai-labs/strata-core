---
title: "Delete vectors by filter"
description: "Delete vectors matching a metadata filter."
source: strata-core@1.0.0
section: vector
---

Scans the collection for visible vectors matching the metadata filter and deletes the matching rows as a bulk mutation.

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Examples

Delete every vector whose metadata matches a filter.

### CLI

```console
$ strata vector collection create docs 3 cosine
$ strata vector upsert docs a [1.0,0.0,0.0] --metadata {"tag":"keep"}
$ strata vector upsert docs b [0.0,1.0,0.0] --metadata {"tag":"drop"}
$ strata vector delete-by-filter docs {"conditions":[{"field":"tag","op":"eq","value":{"type":"string","value":"drop"}}]}
$ strata vector count docs
```

### Wire

```json
{"collection":"docs","dimension":3,"metric":"cosine","type":"vector_create_collection"}
{"collection":"docs","key":"a","metadata":{"tag":"keep"},"type":"vector_upsert","vector":[1.0,0.0,0.0]}
{"collection":"docs","key":"b","metadata":{"tag":"drop"},"type":"vector_upsert","vector":[0.0,1.0,0.0]}
{"collection":"docs","filter":{"conditions":[{"field":"tag","op":"eq","value":{"type":"string","value":"drop"}}]},"type":"vector_delete_by_filter"}
{"collection":"docs","type":"vector_count"}
```

### Output

One response per step, in order:

```json
{"data":{"cursor":null,"has_more":false,"items":[{"count":0,"dimension":3,"metric":"cosine","name":"docs"}]},"type":"vector_collection_list"}
{"data":{"collection":"docs","commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"key":"a","vector_revision":1},"type":"vector_write_result"}
{"data":{"collection":"docs","commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":5,"version":5},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"key":"b","vector_revision":1},"type":"vector_write_result"}
{"data":{"collection":"docs","commit":{"delete_count":1,"durable":false,"put_count":0,"timestamp":6,"version":6},"effect":{"affected_count":1,"applied":true,"kind":"deleted","matched":true}},"type":"vector_bulk_delete_result"}
{"data":1,"type":"uint"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `collection` | `string` | yes | — | Collection name. |
| `filter` | `VectorMetadataFilter` | yes | — | Metadata filter. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<VectorBulkDelete>`.

| Field | Type | Description |
|---|---|---|
| `collection` | `string` | Collection name. |
| `effect` | `MutationEffect` | Mutation effect facts. |
| `commit` | `CommitReceipt` | Commit receipt when deletes were applied. |

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

## Invocation

```text
strata vector delete-by-filter <collection> <filter> [--branch <branch>] [--space <space>]
```

- Wire type: `vector_delete_by_filter`

## Related

- [Create vector collection](/docs/vector/collection/create) — Create a vector collection with a dimension and metric.
- [Count vectors](/docs/vector/count) — Count visible vectors in a collection.
- [Upsert vector](/docs/vector/upsert) — Insert or replace one vector.
- [All `vector` commands](/docs/vector/)
