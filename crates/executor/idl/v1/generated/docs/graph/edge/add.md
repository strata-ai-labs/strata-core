---
title: "Add graph edge"
description: "Add or replace a graph edge."
source: strata-core@1.0.0
section: graph
---

Adds a directed edge `src -[edge_type]-> dst` or replaces it if the same triple already exists. Both endpoints must already exist; writing an edge to a missing node fails with `invalid_argument.engine.graph_edge_endpoint`. Weight defaults to 1.0 and must not be negative. Once the graph's ontology is frozen, the edge type and its endpoint object types are validated against the declared link types.

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Examples

Add a typed edge between two nodes.

### CLI

```console
$ strata graph create social
$ strata graph add-node social alice
$ strata graph add-node social bob
$ strata graph add-edge social alice knows bob
$ strata graph get-edge social alice knows bob
```

### Wire

```json
{"graph":"social","type":"graph_create"}
{"graph":"social","node_id":"alice","type":"graph_add_node"}
{"graph":"social","node_id":"bob","type":"graph_add_node"}
{"dst":"bob","edge_type":"knows","graph":"social","src":"alice","type":"graph_add_edge"}
{"dst":"bob","edge_type":"knows","graph":"social","src":"alice","type":"graph_get_edge"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"info":{"created_timestamp":3,"created_version":3,"edge_count":0,"graph":"social","node_count":0,"updated_timestamp":3,"updated_version":3}},"type":"graph_create_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"graph":"social","node_id":"alice"},"type":"graph_node_write_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":5,"version":5},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"graph":"social","node_id":"bob"},"type":"graph_node_write_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":6,"version":6},"dst":"bob","edge_type":"knows","effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"graph":"social","src":"alice"},"type":"graph_edge_write_result"}
{"data":{"found":true,"value":{"dst":"bob","edge_type":"knows","graph":"social","src":"alice","timestamp":6,"version":6,"weight":1.0}},"type":"graph_edge_result"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `dst` | `string` | yes | — | Destination node id. |
| `edge_type` | `string` | yes | — | Edge type. |
| `graph` | `string` | yes | — | Graph name. |
| `src` | `string` | yes | — | Source node id. |
| `properties` | `any` | no | — | Optional edge properties. |
| `weight` | `number` | no | 1.0 | Optional edge weight. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<GraphEdgeWrite>`.

| Field | Type | Description |
|---|---|---|
| `commit` | `CommitReceipt` | Commit receipt. |
| `dst` | `string` | Destination node id. |
| `edge_type` | `string` | Edge type. |
| `effect` | `MutationEffect` | Mutation effect facts. |
| `graph` | `string` | Graph name. |
| `src` | `string` | Source node id. |

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
| [`invalid_argument.engine.graph_edge_type_reserved`](https://stratadb.org/e/invalid_argument.engine.graph_edge_type_reserved) | The graph request is invalid. |
| [`invalid_argument.engine.graph_edge_weight`](https://stratadb.org/e/invalid_argument.engine.graph_edge_weight) | The graph request is invalid. |
| [`invalid_argument.engine.graph_edge_endpoint`](https://stratadb.org/e/invalid_argument.engine.graph_edge_endpoint) | The graph request is invalid. |
| [`invalid_argument.engine.graph_properties`](https://stratadb.org/e/invalid_argument.engine.graph_properties) | The graph request is invalid. |
| [`invalid_argument.engine.graph_properties_too_large`](https://stratadb.org/e/invalid_argument.engine.graph_properties_too_large) | The graph request is invalid. |
| [`failed_precondition.engine.graph_negative_weight`](https://stratadb.org/e/failed_precondition.engine.graph_negative_weight) | The graph request is invalid. |
| [`failed_precondition.engine.graph_ontology_edge_type`](https://stratadb.org/e/failed_precondition.engine.graph_ontology_edge_type) | The graph request is invalid. |
| [`failed_precondition.engine.graph_ontology_endpoint_type`](https://stratadb.org/e/failed_precondition.engine.graph_ontology_endpoint_type) | The graph request is invalid. |

## Invocation

```text
strata graph add-edge <graph> <src> <edge_type> <dst> [--properties <any>] [--weight <number>] [--branch <branch>] [--space <space>]
```

- Wire type: `graph_add_edge`

## Related

- [Create graph](/docs/graph/create) — Create a named graph.
- [Get graph edge](/docs/graph/edge/get) — Read one graph edge.
- [Add graph node](/docs/graph/node/add) — Add or replace a graph node.
- [All `graph` commands](/docs/graph/)
