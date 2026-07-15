---
title: "Bulk insert graph data"
description: "Bulk-load nodes and edges in chunks."
source: strata-core@1.0.0
section: graph
---

Ingests a payload of nodes and edges in chunked commits: nodes first, then edges, so edges may reference nodes from the same payload. Node objects use the key `node_id`; edges use `src`, `edge_type`, `dst`, and optional `weight` (default 1.0) and `properties`. `chunk_size` bounds items per commit (default 512, clamped at 800). The acknowledgement reports inserted counts, the number of chunk commits, and the final chunk's commit receipt.

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Examples

Insert many nodes and edges in one commit.

### CLI

```console
$ strata graph create g
$ strata graph bulk-insert g --edges [{"dst":"b","edge_type":"knows","src":"a"}] --nodes [{"node_id":"a","object_type":"person"},{"node_id":"b","object_type":"person"}]
$ strata graph meta g
```

### Wire

```json
{"graph":"g","type":"graph_create"}
{"edges":[{"dst":"b","edge_type":"knows","src":"a"}],"graph":"g","nodes":[{"node_id":"a","object_type":"person"},{"node_id":"b","object_type":"person"}],"type":"graph_bulk_insert"}
{"graph":"g","type":"graph_get_meta"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"info":{"created_timestamp":3,"created_version":3,"edge_count":0,"graph":"g","node_count":0,"updated_timestamp":3,"updated_version":3}},"type":"graph_create_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":5,"version":5},"commits":2,"edges_inserted":1,"graph":"g","nodes_inserted":2},"type":"graph_bulk_insert_result"}
{"data":{"created_timestamp":3,"created_version":3,"edge_count":1,"graph":"g","node_count":2,"updated_timestamp":5,"updated_version":5},"type":"graph_info_result"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `graph` | `string` | yes | — | Graph name. |
| `chunk_size` | `integer` | no | 512; values above 800 clamp so one chunk fits one storage commit | Optional items-per-commit chunk size. |
| `edges` | `GraphBulkEdge[]` | no | — | Edges to upsert; endpoints must exist or arrive in `nodes`. |
| `nodes` | `GraphBulkNode[]` | no | — | Nodes to upsert (committed before edges). |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<GraphBulkInsert>`.

| Field | Type | Description |
|---|---|---|
| `commits` | `integer` | How many chunk commits the ingest produced. |
| `edges_inserted` | `integer` | How many edge upserts the ingest applied. |
| `graph` | `string` | Graph name. |
| `nodes_inserted` | `integer` | How many node upserts the ingest applied. |
| `commit` | `CommitReceipt` | Final chunk's commit receipt, when any chunk committed. |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.graph_name`](https://stratadb.org/e/invalid_argument.engine.graph_name) | The graph request is invalid. |
| [`not_found.engine.graph`](https://stratadb.org/e/not_found.engine.graph) | The requested graph was not found. |
| [`invalid_argument.engine.graph_node_id`](https://stratadb.org/e/invalid_argument.engine.graph_node_id) | The graph request is invalid. |
| [`invalid_argument.engine.graph_edge_type`](https://stratadb.org/e/invalid_argument.engine.graph_edge_type) | The graph request is invalid. |
| [`invalid_argument.engine.graph_edge_weight`](https://stratadb.org/e/invalid_argument.engine.graph_edge_weight) | The graph request is invalid. |
| [`invalid_argument.engine.graph_edge_endpoint`](https://stratadb.org/e/invalid_argument.engine.graph_edge_endpoint) | The graph request is invalid. |
| [`invalid_argument.engine.graph_properties`](https://stratadb.org/e/invalid_argument.engine.graph_properties) | The graph request is invalid. |
| [`invalid_argument.engine.graph_properties_too_large`](https://stratadb.org/e/invalid_argument.engine.graph_properties_too_large) | The graph request is invalid. |
| [`failed_precondition.engine.graph_negative_weight`](https://stratadb.org/e/failed_precondition.engine.graph_negative_weight) | The graph request is invalid. |
| [`failed_precondition.engine.graph_ontology_node_type`](https://stratadb.org/e/failed_precondition.engine.graph_ontology_node_type) | The graph request is invalid. |
| [`failed_precondition.engine.graph_ontology_edge_type`](https://stratadb.org/e/failed_precondition.engine.graph_ontology_edge_type) | The graph request is invalid. |

## Invocation

```text
strata graph bulk-insert <graph> [--chunk-size <integer>] [--edges <GraphBulkEdge[]>] [--nodes <GraphBulkNode[]>] [--branch <branch>] [--space <space>]
```

- Wire type: `graph_bulk_insert`

## Related

- [Create graph](/docs/graph/create) — Create a named graph.
- [Read graph metadata](/docs/graph/meta) — Read graph metadata and counts.
- [All `graph` commands](/docs/graph/)
