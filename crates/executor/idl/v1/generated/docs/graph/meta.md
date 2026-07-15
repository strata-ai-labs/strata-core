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

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"info":{"created_timestamp":3,"created_version":3,"edge_count":0,"graph":"social","node_count":0,"updated_timestamp":3,"updated_version":3}},"type":"graph_create_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"graph":"social","node_id":"alice"},"type":"graph_node_write_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":5,"version":5},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"graph":"social","node_id":"bob"},"type":"graph_node_write_result"}
{"data":{"created_timestamp":3,"created_version":3,"edge_count":0,"graph":"social","node_count":2,"updated_timestamp":5,"updated_version":5},"type":"graph_info_result"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `graph` | `string` | yes | — | Graph name. |
| `as_of` | `integer` | no | — | Optional timestamp in microseconds. Reads the graph state visible at that instant. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`Maybe<GraphInfoData>` — a miss returns nothing rather than raising.

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.graph_name`](https://stratadb.org/e/invalid_argument.engine.graph_name) | The graph request is invalid. |
| [`not_found.engine.graph`](https://stratadb.org/e/not_found.engine.graph) | The requested graph was not found. |

## Invocation

```text
strata graph meta <graph> [--as-of <integer>] [--branch <branch>] [--space <space>]
```

- Wire type: `graph_get_meta`

## Related

- [Create graph](/docs/graph/create) — Create a named graph.
- [Add graph node](/docs/graph/node/add) — Add or replace a graph node.
- [All `graph` commands](/docs/graph/)
