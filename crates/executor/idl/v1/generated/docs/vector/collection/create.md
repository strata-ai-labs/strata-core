---
title: "Create vector collection"
description: "Create a vector collection with a dimension and metric."
source: strata-core@1.0.0
section: vector
---

Creates a collection for dense vectors. The dimension and metric become part of the collection contract for future upserts and queries.

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Examples

Create a vector collection, then confirm its dimension.

### CLI

```console
$ strata vector collection create docs 3 cosine
$ strata vector collection stats docs
```

### Wire

```json
{"collection":"docs","dimension":3,"metric":"cosine","type":"vector_create_collection"}
{"collection":"docs","type":"vector_collection_stats"}
```

### Output

One response per step, in order:

```json
{"data":{"cursor":null,"has_more":false,"items":[{"count":0,"dimension":3,"metric":"cosine","name":"docs"}]},"type":"vector_collection_list"}
{"data":{"cursor":null,"has_more":false,"items":[{"count":0,"dimension":3,"metric":"cosine","name":"docs"}]},"type":"vector_collection_list"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `collection` | `string` | yes | — | Collection name. |
| `dimension` | `integer` | yes | — | Embedding dimension. |
| `metric` | `VectorDistanceMetric` | yes | — | Distance metric. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<VectorCollectionCreate>`.

| Field | Type | Description |
|---|---|---|
| `has_more` | `boolean` |  |
| `items` | `VectorCollectionInfo[]` | Collections in this page. |
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
| [`invalid_argument.engine.vector_dimension`](https://stratadb.org/e/invalid_argument.engine.vector_dimension) | The vector request is invalid. |
| [`invalid_argument.executor.vector_dimension`](https://stratadb.org/e/invalid_argument.executor.vector_dimension) | The vector dimension is invalid. |

## Invocation

```text
strata vector collection create <collection> <dimension> <metric> [--branch <branch>] [--space <space>]
```

- Wire type: `vector_create_collection`

## Related

- [Read vector collection stats](/docs/vector/collection/stats) — Read facts for one vector collection.
- [All `vector` commands](/docs/vector/)
