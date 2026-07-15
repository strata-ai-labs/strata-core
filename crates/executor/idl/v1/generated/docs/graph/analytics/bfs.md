---
title: "Traverse graph breadth-first"
description: "Run a bounded breadth-first traversal."
source: strata-core@1.0.0
section: graph
---

Runs a breadth-first traversal from a start node over a consistent snapshot, bounded by `max_depth` (default 100) and `max_nodes` (default 10000). Returns visited node ids in traversal order, a depth per node, and the tree edges in discovery order. Direction defaults to `outgoing`; an optional edge-type list restricts every hop. The start node must exist (`not_found.engine.graph_node`).

Analytics commands compute over a consistent snapshot of the visible graph and return a complete result payload in one response. They accept optional snapshot budgets and an `as_of` timestamp for time travel; results are deterministic for a fixed graph state.

## Examples

Breadth-first traversal from a start node.

### CLI

```console
$ strata graph create g
$ strata graph add-node g a
$ strata graph add-node g b
$ strata graph add-node g c
$ strata graph add-edge g a knows b
$ strata graph add-edge g b knows c
$ strata graph bfs g a
```

### Wire

```json
{"graph":"g","type":"graph_create"}
{"graph":"g","node_id":"a","type":"graph_add_node"}
{"graph":"g","node_id":"b","type":"graph_add_node"}
{"graph":"g","node_id":"c","type":"graph_add_node"}
{"dst":"b","edge_type":"knows","graph":"g","src":"a","type":"graph_add_edge"}
{"dst":"c","edge_type":"knows","graph":"g","src":"b","type":"graph_add_edge"}
{"graph":"g","start":"a","type":"graph_bfs"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"info":{"created_timestamp":3,"created_version":3,"edge_count":0,"graph":"g","node_count":0,"updated_timestamp":3,"updated_version":3}},"type":"graph_create_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"graph":"g","node_id":"a"},"type":"graph_node_write_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":5,"version":5},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"graph":"g","node_id":"b"},"type":"graph_node_write_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":6,"version":6},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"graph":"g","node_id":"c"},"type":"graph_node_write_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":7,"version":7},"dst":"b","edge_type":"knows","effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"graph":"g","src":"a"},"type":"graph_edge_write_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":8,"version":8},"dst":"c","edge_type":"knows","effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"graph":"g","src":"b"},"type":"graph_edge_write_result"}
{"data":{"depths":{"a":0,"b":1,"c":2},"edges":[{"dst":"b","edge_type":"knows","src":"a","weight":1.0},{"dst":"c","edge_type":"knows","src":"b","weight":1.0}],"graph":"g","start":"a","visited":["a","b","c"]},"type":"graph_bfs_result"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `graph` | `string` | yes | — | Graph name. |
| `start` | `string` | yes | — | Start node id. |
| `as_of` | `integer` | no | — | Optional timestamp in microseconds. Reads the graph state visible at that instant. |
| `budget` | `GraphAnalyticsBudget` | no | the engine limits | Optional snapshot size bounds. |
| `direction` | `GraphDirection` | no | outgoing | Optional traversal direction. |
| `edge_types` | `string[]` | no | — | Optional edge-type restriction applied at every hop. |
| `max_depth` | `integer` | no | 100 | Optional depth bound. |
| `max_nodes` | `integer` | no | 10000 | Optional visited-node bound. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`AnalyticsResult<GraphBfsData>`.

| Field | Type | Description |
|---|---|---|
| `depths` | `object` |  |
| `edges` | `GraphBfsEdgeData[]` |  |
| `graph` | `string` |  |
| `start` | `string` |  |
| `visited` | `string[]` |  |

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
| [`not_found.engine.graph_node`](https://stratadb.org/e/not_found.engine.graph_node) | The requested graph node was not found. |
| [`resource_exhausted.engine.graph_analytics_budget`](https://stratadb.org/e/resource_exhausted.engine.graph_analytics_budget) | The graph request is invalid. |
| [`invalid_argument.executor.graph_analytics_budget`](https://stratadb.org/e/invalid_argument.executor.graph_analytics_budget) | A graph analytics budget value is out of range for this platform. |

## Invocation

```text
strata graph bfs <graph> <start> [--as-of <integer>] [--budget <GraphAnalyticsBudget>] [--direction <GraphDirection>] [--edge-types <string[]>] [--max-depth <integer>] [--max-nodes <integer>] [--branch <branch>] [--space <space>]
```

- Wire type: `graph_bfs`

## Related

- [Create graph](/docs/graph/create) — Create a named graph.
- [Add graph edge](/docs/graph/edge/add) — Add or replace a graph edge.
- [Add graph node](/docs/graph/node/add) — Add or replace a graph node.
- [All `graph` commands](/docs/graph/)
