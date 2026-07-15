---
title: "Delete vector collection"
description: "Delete a vector collection."
source: strata-core@1.0.0
section: vector
---

Deletes the selected vector collection from the current branch and space. The current wire response is a transitional boolean status.

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Examples

Delete a collection.

### CLI

```console
$ strata vector collection create temp 3 cosine
$ strata vector collection delete temp
$ strata vector collection list
```

### Wire

```json
{"collection":"temp","dimension":3,"metric":"cosine","type":"vector_create_collection"}
{"collection":"temp","type":"vector_delete_collection"}
{"type":"vector_list_collections"}
```

### Output

One response per step, in order:

```json
{"data":{"cursor":null,"has_more":false,"items":[{"count":0,"dimension":3,"metric":"cosine","name":"temp"}]},"type":"vector_collection_list"}
{"data":true,"type":"bool"}
{"data":{"cursor":null,"has_more":false,"items":[]},"type":"vector_collection_list"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `collection` | `string` | yes | — | Collection name. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<VectorCollectionDelete>`.

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
strata vector collection delete <collection> [--branch <branch>] [--space <space>]
```

- Wire type: `vector_delete_collection`

## Related

- [Create vector collection](/docs/vector/collection/create) — Create a vector collection with a dimension and metric.
- [List vector collections](/docs/vector/collection/list) — List vector collections.
- [All `vector` commands](/docs/vector/)
