---
title: "List graph neighbors"
description: "List a node's neighbors."
source: strata-core@1.0.0
section: graph
---

Walks a node's edges and returns one hit per traversed edge. Direction is `outgoing`, `incoming`, or `both`; an optional edge-type filter restricts the walk. Each hit embeds both the traversed edge and the neighbor node in full, so a follow-up read is rarely needed. A missing node yields an empty page.

Paginated responses use opaque cursors. Clients should pass the returned cursor back to the same command shape and must not parse cursor contents.

## Examples

Find a node's neighbors along outgoing edges.

### CLI

```console
$ strata graph create social
$ strata graph add-node social alice
$ strata graph add-node social bob
$ strata graph add-edge social alice knows bob
$ strata graph neighbors social alice outgoing
```

### Wire

```json
{"graph":"social","type":"graph_create"}
{"graph":"social","node_id":"alice","type":"graph_add_node"}
{"graph":"social","node_id":"bob","type":"graph_add_node"}
{"dst":"bob","edge_type":"knows","graph":"social","src":"alice","type":"graph_add_edge"}
{"direction":"outgoing","graph":"social","node_id":"alice","type":"graph_neighbors"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"info":{"created_timestamp":3,"created_version":3,"edge_count":0,"graph":"social","node_count":0,"updated_timestamp":3,"updated_version":3}},"type":"graph_create_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"graph":"social","node_id":"alice"},"type":"graph_node_write_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":5,"version":5},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"graph":"social","node_id":"bob"},"type":"graph_node_write_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":6,"version":6},"dst":"bob","edge_type":"knows","effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"graph":"social","src":"alice"},"type":"graph_edge_write_result"}
{"data":{"cursor":null,"has_more":false,"items":[{"direction":"outgoing","dst":"bob","edge":{"dst":"bob","edge_type":"knows","graph":"social","src":"alice","timestamp":6,"version":6,"weight":1.0},"edge_type":"knows","graph":"social","node":{"graph":"social","node_id":"bob","timestamp":5,"version":5},"node_id":"bob","src":"alice"}]},"type":"graph_neighbor_page"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `direction` | `GraphDirection` | yes | — | Traversal direction. |
| `graph` | `string` | yes | — | Graph name. |
| `node_id` | `string` | yes | — | Node id. |
| `as_of` | `integer` | no | — | Optional timestamp in microseconds. Reads the graph state visible at that instant. |
| `cursor` | `string` | no | — | Optional exclusive cursor. |
| `edge_type` | `string` | no | — | Optional edge type filter. |
| `limit` | `integer` | no | 100 | Optional item limit. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`Page<GraphNeighborHit, String>`.

| Field | Type | Description |
|---|---|---|
| `has_more` | `boolean` |  |
| `items` | `GraphNeighborHit[]` | Neighbor hits in this page. |
| `cursor` | `string` |  |

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
strata graph neighbors <graph> <node_id> <direction> [--as-of <integer>] [--cursor <string>] [--edge-type <string>] [--limit <integer>] [--branch <branch>] [--space <space>]
```

- Wire type: `graph_neighbors`

## Related

- [Create graph](/docs/graph/create) — Create a named graph.
- [Add graph edge](/docs/graph/edge/add) — Add or replace a graph edge.
- [Add graph node](/docs/graph/node/add) — Add or replace a graph node.
- [All `graph` commands](/docs/graph/)
