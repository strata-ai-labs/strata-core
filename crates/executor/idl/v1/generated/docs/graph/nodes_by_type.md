---
title: "List graph nodes by type"
description: "List nodes declaring an object type."
source: strata-core@1.0.0
section: graph
---

Lists the nodes that declare a given object type, in node-id order. The type index is maintained from each node's declared `object_type`, so this works whether the ontology is draft or frozen. Accepts a limit, an exclusive cursor, and `as_of` for time travel.

Paginated responses use opaque cursors. Clients should pass the returned cursor back to the same command shape and must not parse cursor contents.

## Examples

List nodes of a given object type.

### CLI

```console
$ strata graph create g
$ strata graph add-node g a --object-type person
$ strata graph add-node g b --object-type person
$ strata graph nodes-by-type g person
```

### Wire

```json
{"graph":"g","type":"graph_create"}
{"graph":"g","node_id":"a","object_type":"person","type":"graph_add_node"}
{"graph":"g","node_id":"b","object_type":"person","type":"graph_add_node"}
{"graph":"g","object_type":"person","type":"graph_nodes_by_type"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"info":{"created_timestamp":3,"created_version":3,"edge_count":0,"graph":"g","node_count":0,"updated_timestamp":3,"updated_version":3}},"type":"graph_create_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"graph":"g","node_id":"a"},"type":"graph_node_write_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":5,"version":5},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"graph":"g","node_id":"b"},"type":"graph_node_write_result"}
{"data":{"cursor":null,"has_more":false,"items":[{"graph":"g","node_id":"a","object_type":"person","timestamp":4,"version":4},{"graph":"g","node_id":"b","object_type":"person","timestamp":5,"version":5}]},"type":"graph_node_page"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `graph` | `string` | yes | — | Graph name. |
| `object_type` | `string` | yes | — | Object type name. |
| `as_of` | `integer` | no | — | Optional timestamp in microseconds. Reads the graph state visible at that instant. |
| `cursor` | `string` | no | — | Optional exclusive node id cursor. |
| `limit` | `integer` | no | 100 | Optional item limit. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`Page<GraphNodeDataOutput, String>`.

| Field | Type | Description |
|---|---|---|
| `has_more` | `boolean` |  |
| `items` | `GraphNodeDataOutput[]` | Nodes in this page. |
| `cursor` | `string` |  |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.graph_name`](https://stratadb.org/e/invalid_argument.engine.graph_name) | The graph request is invalid. |
| [`not_found.engine.graph`](https://stratadb.org/e/not_found.engine.graph) | The requested graph was not found. |
| [`invalid_argument.engine.graph_type_name`](https://stratadb.org/e/invalid_argument.engine.graph_type_name) | The graph request is invalid. |
| [`invalid_argument.engine.graph_node_id`](https://stratadb.org/e/invalid_argument.engine.graph_node_id) | The graph request is invalid. |

## Invocation

```text
strata graph nodes-by-type <graph> <object_type> [--as-of <integer>] [--cursor <string>] [--limit <integer>] [--branch <branch>] [--space <space>]
```

- Wire type: `graph_nodes_by_type`

## Related

- [Create graph](/docs/graph/create) — Create a named graph.
- [Add graph node](/docs/graph/node/add) — Add or replace a graph node.
- [All `graph` commands](/docs/graph/)
