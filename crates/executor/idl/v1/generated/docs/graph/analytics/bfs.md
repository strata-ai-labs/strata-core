---
title: "Traverse graph breadth-first"
description: "Run a bounded breadth-first traversal."
source: strata-core@1.0.0
section: graph
---

Runs a breadth-first traversal from a start node over a consistent snapshot, bounded by `max_depth` (default 100) and `max_nodes` (default 10000). Returns visited node ids in traversal order, a depth per node, and the tree edges in discovery order. Direction defaults to `outgoing`; an optional edge-type list restricts every hop. The start node must exist (`not_found.engine.graph_node`).

Analytics commands compute over a consistent snapshot of the visible graph and return a complete result payload in one response. They accept optional snapshot budgets and an `as_of` timestamp for time travel; results are deterministic for a fixed graph state.

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `as_of` | `integer` | no | Optional timestamp in microseconds. Reads the graph state visible at that instant. |
| `budget` | `GraphAnalyticsBudget` | no | Optional snapshot size bounds. Defaults to the engine limits. |
| `direction` | `GraphDirection` | no | Optional traversal direction. Defaults to outgoing. |
| `edge_types` | `string[]` | no | Optional edge-type restriction applied at every hop. |
| `graph` | `string` | yes | Graph name. |
| `max_depth` | `integer` | no | Optional depth bound. Defaults to 100. |
| `max_nodes` | `integer` | no | Optional visited-node bound. Defaults to 10000. |
| `start` | `string` | yes | Start node id. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`AnalyticsResult<GraphBfsData>`.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space)
- [`invalid_argument.engine.graph_name`](https://stratadb.org/e/invalid_argument.engine.graph_name)
- [`not_found.engine.graph`](https://stratadb.org/e/not_found.engine.graph)
- [`invalid_argument.engine.graph_node_id`](https://stratadb.org/e/invalid_argument.engine.graph_node_id)
- [`invalid_argument.engine.graph_edge_type`](https://stratadb.org/e/invalid_argument.engine.graph_edge_type)
- [`not_found.engine.graph_node`](https://stratadb.org/e/not_found.engine.graph_node)
- [`resource_exhausted.engine.graph_analytics_budget`](https://stratadb.org/e/resource_exhausted.engine.graph_analytics_budget)
- [`invalid_argument.executor.graph_analytics_budget`](https://stratadb.org/e/invalid_argument.executor.graph_analytics_budget)

## Invocation

- CLI: `strata graph bfs`
- Wire type: `graph_bfs`
