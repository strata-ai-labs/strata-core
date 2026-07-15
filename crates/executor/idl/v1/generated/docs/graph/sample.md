---
title: "Sample graph nodes"
description: "Sample graph nodes."
source: strata-core@1.0.0
section: graph
---

Returns a deterministic representative sample of nodes from a graph: the total live node count and up to `count` nodes (default 10), evenly strided over the ordered node ids. Latest state only.

Paginated responses use opaque cursors. Clients should pass the returned cursor back to the same command shape and must not parse cursor contents.

## Examples

A representative sample of nodes plus the total count.

### CLI

```console
$ strata graph create g
$ strata graph add-node g a
$ strata graph add-node g b
$ strata graph sample g
```

### Wire

```json
{"graph":"g","type":"graph_create"}
{"graph":"g","node_id":"a","type":"graph_add_node"}
{"graph":"g","node_id":"b","type":"graph_add_node"}
{"graph":"g","type":"graph_sample"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"info":{"created_timestamp":3,"created_version":3,"edge_count":0,"graph":"g","node_count":0,"updated_timestamp":3,"updated_version":3}},"type":"graph_create_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"graph":"g","node_id":"a"},"type":"graph_node_write_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":5,"version":5},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"graph":"g","node_id":"b"},"type":"graph_node_write_result"}
{"data":{"cursor":null,"has_more":false,"items":[{"graph":"g","node_id":"a","timestamp":4,"version":4},{"graph":"g","node_id":"b","timestamp":5,"version":5}],"total_count":2},"type":"graph_sample_result"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `graph` | `string` | yes | — | Graph name. |
| `count` | `integer` | no | 10 | Optional sample count. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`SamplePage<GraphNodeDataOutput>`.

| Field | Type | Description |
|---|---|---|
| `has_more` | `boolean` |  |
| `items` | `GraphNodeDataOutput[]` | Sampled nodes. |
| `total_count` | `integer` | Total live nodes in the graph. |
| `cursor` | `string` |  |

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
strata graph sample <graph> [--count <integer>] [--branch <branch>] [--space <space>]
```

- Wire type: `graph_sample`

## Related

- [Create graph](/docs/graph/create) — Create a named graph.
- [Add graph node](/docs/graph/node/add) — Add or replace a graph node.
- [All `graph` commands](/docs/graph/)
