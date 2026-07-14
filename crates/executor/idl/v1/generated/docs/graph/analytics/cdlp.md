---
title: "Detect graph communities"
description: "Detect communities via label propagation."
source: strata-core@1.0.0
section: graph
---

Detects communities by label propagation over a consistent snapshot: every node repeatedly adopts the most common label among its neighbors until labels stabilize or the iteration bound (default 10) is reached. Every node maps to its community representative node id. Propagation direction defaults to `both`. Accepts an optional snapshot budget and `as_of` for time travel.

Analytics commands compute over a consistent snapshot of the visible graph and return a complete result payload in one response. They accept optional snapshot budgets and an `as_of` timestamp for time travel; results are deterministic for a fixed graph state.

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `as_of` | `integer` | no | Optional timestamp in microseconds. Reads the graph state visible at that instant. |
| `budget` | `GraphAnalyticsBudget` | no | Optional snapshot size bounds. Defaults to the engine limits. |
| `direction` | `GraphDirection` | no | Optional propagation direction. Defaults to both. |
| `graph` | `string` | yes | Graph name. |
| `max_iterations` | `integer` | no | Optional iteration bound. Defaults to 10. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`AnalyticsResult<GraphCdlpData>`.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space)
- [`invalid_argument.engine.graph_name`](https://stratadb.org/e/invalid_argument.engine.graph_name)
- [`not_found.engine.graph`](https://stratadb.org/e/not_found.engine.graph)
- [`resource_exhausted.engine.graph_analytics_budget`](https://stratadb.org/e/resource_exhausted.engine.graph_analytics_budget)
- [`invalid_argument.executor.graph_analytics_budget`](https://stratadb.org/e/invalid_argument.executor.graph_analytics_budget)

## Invocation

- CLI: `strata graph cdlp`
- Wire type: `graph_cdlp`
