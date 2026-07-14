---
title: "Read graph metadata"
description: "Read graph metadata and counts."
source: strata-core@1.0.0
section: graph
---

Reads a graph's metadata: live node and edge counts plus the create and last-update commit versions and timestamps. Reading a graph that does not exist returns no data rather than an error. Accepts `as_of` for time travel.

Optional reads distinguish present data from missing data. When version or timestamp facts exist on the executor output, SDK mappings should preserve them.

## Examples

Read a graph's node and edge counts.

### CLI

```console
$ strata graph create social
$ strata graph add-node social alice
$ strata graph add-node social bob
$ strata graph meta social
```

### Wire

```json
{"graph":"social","type":"graph_create"}
{"graph":"social","node_id":"alice","type":"graph_add_node"}
{"graph":"social","node_id":"bob","type":"graph_add_node"}
{"graph":"social","type":"graph_get_meta"}
```

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `as_of` | `integer` | no | Optional timestamp in microseconds. Reads the graph state visible at that instant. |
| `graph` | `string` | yes | Graph name. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`Maybe<GraphInfoData>` — a miss returns nothing rather than raising.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space)
- [`invalid_argument.engine.graph_name`](https://stratadb.org/e/invalid_argument.engine.graph_name)
- [`not_found.engine.graph`](https://stratadb.org/e/not_found.engine.graph)

## Invocation

- CLI: `strata graph meta`
- Wire type: `graph_get_meta`
