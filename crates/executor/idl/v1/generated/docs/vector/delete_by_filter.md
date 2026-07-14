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

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `collection` | `string` | yes | Collection name. |
| `filter` | `VectorMetadataFilter` | yes | Metadata filter. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<VectorBulkDelete>`.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space)
- [`invalid_argument.engine.vector_collection`](https://stratadb.org/e/invalid_argument.engine.vector_collection)
- [`invalid_argument.engine.vector_key`](https://stratadb.org/e/invalid_argument.engine.vector_key)
- [`not_found.engine.vector_collection`](https://stratadb.org/e/not_found.engine.vector_collection)
- [`invalid_argument.engine.vector_filter`](https://stratadb.org/e/invalid_argument.engine.vector_filter)

## Invocation

- CLI: `strata vector delete-by-filter`
- Wire type: `vector_delete_by_filter`
