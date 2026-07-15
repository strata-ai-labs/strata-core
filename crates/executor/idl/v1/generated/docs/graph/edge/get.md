---
title: "Get graph edge"
description: "Read one graph edge."
source: strata-core@1.0.0
section: graph
---

Reads one edge by its `(src, edge_type, dst)` triple, returning its weight, properties, and last-write commit coordinates. A missing edge reads back as no data. Accepts `as_of` for time travel.

Optional reads distinguish present data from missing data. When version or timestamp facts exist on the executor output, SDK mappings should preserve them.

## Examples

Read an edge, or nothing if absent.

### CLI

```console
$ strata graph create social
$ strata graph add-node social alice
$ strata graph add-node social bob
$ strata graph add-edge social alice knows bob
$ strata graph get-edge social alice knows bob
$ strata graph get-edge social alice knows absent
```

### Wire

```json
{"graph":"social","type":"graph_create"}
{"graph":"social","node_id":"alice","type":"graph_add_node"}
{"graph":"social","node_id":"bob","type":"graph_add_node"}
{"dst":"bob","edge_type":"knows","graph":"social","src":"alice","type":"graph_add_edge"}
{"dst":"bob","edge_type":"knows","graph":"social","src":"alice","type":"graph_get_edge"}
{"dst":"absent","edge_type":"knows","graph":"social","src":"alice","type":"graph_get_edge"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"info":{"created_timestamp":3,"created_version":3,"edge_count":0,"graph":"social","node_count":0,"updated_timestamp":3,"updated_version":3}},"type":"graph_create_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"graph":"social","node_id":"alice"},"type":"graph_node_write_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":5,"version":5},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"graph":"social","node_id":"bob"},"type":"graph_node_write_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":6,"version":6},"dst":"bob","edge_type":"knows","effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"graph":"social","src":"alice"},"type":"graph_edge_write_result"}
{"data":{"found":true,"value":{"dst":"bob","edge_type":"knows","graph":"social","src":"alice","timestamp":6,"version":6,"weight":1.0}},"type":"graph_edge_result"}
{"data":{"found":false,"value":null},"type":"graph_edge_result"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `dst` | `string` | yes | — | Destination node id. |
| `edge_type` | `string` | yes | — | Edge type. |
| `graph` | `string` | yes | — | Graph name. |
| `src` | `string` | yes | — | Source node id. |
| `as_of` | `integer` | no | — | Optional timestamp in microseconds. Reads the graph state visible at that instant. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`Maybe<GraphEdgeDataOutput>` — a miss returns nothing rather than raising.

| Field | Type | Description |
|---|---|---|
| `found` | `boolean` |  |
| `value` | `GraphEdgeDataOutput` |  |

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

## Invocation

```text
strata graph get-edge <graph> <src> <edge_type> <dst> [--as-of <integer>] [--branch <branch>] [--space <space>]
```

- Wire type: `graph_get_edge`

## Related

- [Create graph](/docs/graph/create) — Create a named graph.
- [Add graph edge](/docs/graph/edge/add) — Add or replace a graph edge.
- [Add graph node](/docs/graph/node/add) — Add or replace a graph node.
- [All `graph` commands](/docs/graph/)
