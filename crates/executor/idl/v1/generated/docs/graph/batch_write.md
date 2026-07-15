---
title: "Batch write graph"
description: "Apply graph mutations atomically."
source: strata-core@1.0.0
section: graph
---

Applies a list of graph operations - `upsert_node`, `delete_node`, `upsert_edge`, `delete_edge` - in one engine commit. Validation failures (bad ids, missing edge endpoints, frozen-ontology violations) reject the whole batch; nothing is partially applied. The response reports one positional item result per operation, all sharing the same commit receipt.

Atomic batches validate every operation up front and apply all of them in one engine commit, or none at all. The response still reports one positional item result per operation; all item results share the same commit receipt.

## Examples

Apply several node and edge mutations in one atomic commit.

### CLI

```console
$ strata graph create g
$ strata command run --command-json '{"graph":"g","operations":[{"data":{"object_type":"person"},"node_id":"a","type":"upsert_node"},{"data":{"object_type":"person"},"node_id":"b","type":"upsert_node"},{"data":{},"dst":"b","edge_type":"knows","src":"a","type":"upsert_edge"}],"type":"graph_batch_write"}'  # All operations land in one engine commit, or none do.
$ strata graph meta g
```

### Wire

```json
{"graph":"g","type":"graph_create"}
{"graph":"g","operations":[{"data":{"object_type":"person"},"node_id":"a","type":"upsert_node"},{"data":{"object_type":"person"},"node_id":"b","type":"upsert_node"},{"data":{},"dst":"b","edge_type":"knows","src":"a","type":"upsert_edge"}],"type":"graph_batch_write"}
{"graph":"g","type":"graph_get_meta"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"info":{"created_timestamp":3,"created_version":3,"edge_count":0,"graph":"g","node_count":0,"updated_timestamp":3,"updated_version":3}},"type":"graph_create_result"}
{"data":{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":3,"timestamp":4,"version":4},"graph":"g","items":[{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":3,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":0,"result":{"created":true,"operation":"upsert_node","operation_index":0},"status":"ok"},{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":3,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":1,"result":{"created":true,"operation":"upsert_node","operation_index":1},"status":"ok"},{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":3,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":2,"result":{"created":true,"operation":"upsert_edge","operation_index":2},"status":"ok"}],"mode":"atomic","status":"ok"},"type":"graph_batch_write_result"}
{"data":{"created_timestamp":3,"created_version":3,"edge_count":1,"graph":"g","node_count":2,"updated_timestamp":4,"updated_version":4},"type":"graph_info_result"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `graph` | `string` | yes | — | Graph name. |
| `operations` | `GraphBatchOperation[]` | yes | — | Batch operations. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`BatchResult<GraphBatchItemResult>`.

| Field | Type | Description |
|---|---|---|
| `applied` | `boolean` |  |
| `graph` | `string` | Graph name. |
| `items` | `BatchItem10[]` |  |
| `mode` | `BatchMode` |  |
| `status` | `BatchStatus` |  |
| `commit` | `CommitReceipt` |  |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.graph_name`](https://stratadb.org/e/invalid_argument.engine.graph_name) | The graph request is invalid. |
| [`not_found.engine.graph`](https://stratadb.org/e/not_found.engine.graph) | The requested graph was not found. |
| [`invalid_argument.engine.graph_batch`](https://stratadb.org/e/invalid_argument.engine.graph_batch) | The graph request is invalid. |
| [`invalid_argument.engine.graph_node_id`](https://stratadb.org/e/invalid_argument.engine.graph_node_id) | The graph request is invalid. |
| [`invalid_argument.engine.graph_edge_type`](https://stratadb.org/e/invalid_argument.engine.graph_edge_type) | The graph request is invalid. |
| [`invalid_argument.engine.graph_edge_endpoint`](https://stratadb.org/e/invalid_argument.engine.graph_edge_endpoint) | The graph request is invalid. |
| [`failed_precondition.engine.graph_ontology_node_type`](https://stratadb.org/e/failed_precondition.engine.graph_ontology_node_type) | The graph request is invalid. |
| [`failed_precondition.engine.graph_ontology_edge_type`](https://stratadb.org/e/failed_precondition.engine.graph_ontology_edge_type) | The graph request is invalid. |

## Invocation

- CLI: via `strata command run` (no dedicated verb)
- Wire type: `graph_batch_write`

## Related

- [Create graph](/docs/graph/create) — Create a named graph.
- [Read graph metadata](/docs/graph/meta) — Read graph metadata and counts.
- [All `graph` commands](/docs/graph/)
