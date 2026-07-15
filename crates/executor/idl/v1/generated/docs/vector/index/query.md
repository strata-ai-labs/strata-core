---
title: "Query vector index"
description: "Search vectors and return index diagnostics."
source: strata-core@1.0.0
section: vector
---

Runs vector search and includes planner diagnostics such as index policy, source usage, artifact status, and fallback facts.

Search responses return a bounded list of matches ordered by the engine. They are not cursor pages unless a later command explicitly advertises pagination.

Diagnostic responses include operational facts intended for debugging and tuning. They should not be required for application correctness.

## Examples

Nearest-neighbor search that also returns index diagnostics.

### CLI

```console
$ strata vector collection create docs 3 cosine
$ strata vector upsert docs a [1.0,0.0,0.0]
$ strata vector upsert docs b [0.0,1.0,0.0]
$ strata command run --command-json '{"collection":"docs","k":2,"query":[1.0,0.0,0.0],"type":"vector_index_query"}'
```

### Wire

```json
{"collection":"docs","dimension":3,"metric":"cosine","type":"vector_create_collection"}
{"collection":"docs","key":"a","type":"vector_upsert","vector":[1.0,0.0,0.0]}
{"collection":"docs","key":"b","type":"vector_upsert","vector":[0.0,1.0,0.0]}
{"collection":"docs","k":2,"query":[1.0,0.0,0.0],"type":"vector_index_query"}
```

### Output

One response per step, in order:

```json
{"data":{"cursor":null,"has_more":false,"items":[{"count":0,"dimension":3,"metric":"cosine","name":"docs"}]},"type":"vector_collection_list"}
{"data":{"collection":"docs","commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"key":"a","vector_revision":1},"type":"vector_write_result"}
{"data":{"collection":"docs","commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":5,"version":5},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"key":"b","vector_revision":1},"type":"vector_write_result"}
{"data":{"diagnostics":{"active_delta_count":0,"active_delta_seal_threshold":16,"active_delta_source_count":0,"artifact_sources":[],"collection":"docs","collection_exact_threshold":64,"derived_bytes":0,"exact_fallback_count":0,"exact_source_count":1,"filtered_underfill_fallback":true,"flat_source_count":0,"hnsw_graph_builds":0,"hnsw_memory_budget_bytes":67108864,"hnsw_source_count":0,"indexed_source_count":0,"indexed_vector_count":0,"last_query_fallback_reason":"collection_below_exact_threshold","last_query_used_index":false,"manifest_inherited_ref_count":0,"manifest_owned_ref_count":0,"manifest_ref_count":0,"manifest_status":"missing","overfetch_factor":4,"policy_mode":"auto","resolved_index_kind_summary":"exact","source_candidate_limit":18446744073709551615,"source_flat_threshold":64,"source_hnsw_threshold":18446744073709551615},"matches":[{"key":"a","score":1.0},{"key":"b","score":0.0}]},"type":"vector_index_query"}
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

`SearchResult<VectorMatch> + IndexDiagnostics`.

| Field | Type | Description |
|---|---|---|
| `diagnostics` | `VectorIndexDiagnostics` |  |
| `matches` | `VectorMatch[]` |  |

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

- CLI: via `strata command run` (no dedicated verb)
- Wire type: `vector_index_query`

## Related

- [Create vector collection](/docs/vector/collection/create) — Create a vector collection with a dimension and metric.
- [Upsert vector](/docs/vector/upsert) — Insert or replace one vector.
- [All `vector` commands](/docs/vector/)
