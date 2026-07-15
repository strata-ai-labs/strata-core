---
title: "Compute graph connected components"
description: "Compute weakly connected components."
source: strata-core@1.0.0
section: graph
---

Computes weakly connected components over a consistent snapshot of the graph, ignoring edge direction. Every node maps to its component representative - the smallest node id in its component - and the response carries the distinct component count. Accepts an optional snapshot budget and `as_of` for time travel.

Analytics commands compute over a consistent snapshot of the visible graph and return a complete result payload in one response. They accept optional snapshot budgets and an `as_of` timestamp for time travel; results are deterministic for a fixed graph state.

## Examples

Weakly-connected components.

### CLI

```console
$ strata graph create g
$ strata graph add-node g a
$ strata graph add-node g b
$ strata graph add-node g c
$ strata graph add-edge g a knows b
$ strata graph add-edge g b knows c
$ strata graph wcc g
```

### Wire

```json
{"graph":"g","type":"graph_create"}
{"graph":"g","node_id":"a","type":"graph_add_node"}
{"graph":"g","node_id":"b","type":"graph_add_node"}
{"graph":"g","node_id":"c","type":"graph_add_node"}
{"dst":"b","edge_type":"knows","graph":"g","src":"a","type":"graph_add_edge"}
{"dst":"c","edge_type":"knows","graph":"g","src":"b","type":"graph_add_edge"}
{"graph":"g","type":"graph_wcc"}
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
{"data":{"component_count":1,"components":{"a":"a","b":"a","c":"a"},"graph":"g"},"type":"graph_wcc_result"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `graph` | `string` | yes | — | Graph name. |
| `as_of` | `integer` | no | — | Optional timestamp in microseconds. Reads the graph state visible at that instant. |
| `budget` | `GraphAnalyticsBudget` | no | the engine limits | Optional snapshot size bounds. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`AnalyticsResult<GraphWccData>`.

| Field | Type | Description |
|---|---|---|
| `component_count` | `integer` |  |
| `components` | `object` |  |
| `graph` | `string` |  |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.graph_name`](https://stratadb.org/e/invalid_argument.engine.graph_name) | The graph request is invalid. |
| [`not_found.engine.graph`](https://stratadb.org/e/not_found.engine.graph) | The requested graph was not found. |
| [`resource_exhausted.engine.graph_analytics_budget`](https://stratadb.org/e/resource_exhausted.engine.graph_analytics_budget) | The graph request is invalid. |
| [`invalid_argument.executor.graph_analytics_budget`](https://stratadb.org/e/invalid_argument.executor.graph_analytics_budget) | A graph analytics budget value is out of range for this platform. |

## Invocation

```text
strata graph wcc <graph> [--as-of <integer>] [--budget <GraphAnalyticsBudget>] [--branch <branch>] [--space <space>]
```

- Wire type: `graph_wcc`

## Related

- [Create graph](/docs/graph/create) — Create a named graph.
- [Add graph edge](/docs/graph/edge/add) — Add or replace a graph edge.
- [Add graph node](/docs/graph/node/add) — Add or replace a graph node.
- [All `graph` commands](/docs/graph/)
