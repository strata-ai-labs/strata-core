---
title: "Add graph node"
description: "Add or replace a graph node."
source: strata-core@1.0.0
section: graph
---

Adds a node to a graph or replaces it if the node id already exists. A node carries optional JSON properties, an optional declared object type (validated once the graph's ontology is frozen), and an optional entity binding that links the node to a row in another primitive. Cross-branch bindings are rejected.

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Examples

Add a node with an object type and properties.

### CLI

```console
$ strata graph create social
$ strata graph add-node social alice --object-type person --properties {"age":30}
$ strata graph get-node social alice
```

### Wire

```json
{"graph":"social","type":"graph_create"}
{"graph":"social","node_id":"alice","object_type":"person","properties":{"age":30},"type":"graph_add_node"}
{"graph":"social","node_id":"alice","type":"graph_get_node"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"info":{"created_timestamp":3,"created_version":3,"edge_count":0,"graph":"social","node_count":0,"updated_timestamp":3,"updated_version":3}},"type":"graph_create_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"graph":"social","node_id":"alice"},"type":"graph_node_write_result"}
{"data":{"found":true,"value":{"graph":"social","node_id":"alice","object_type":"person","properties":{"age":30},"timestamp":4,"version":4}},"type":"graph_node_result"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `graph` | `string` | yes | — | Graph name. |
| `node_id` | `string` | yes | — | Node id. |
| `binding` | `GraphEntityBinding` | no | — | Optional entity binding. |
| `object_type` | `string` | no | — | Optional declared object type (validated once the ontology is frozen). |
| `properties` | `any` | no | — | Optional node properties. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<GraphNodeWrite>`.

| Field | Type | Description |
|---|---|---|
| `commit` | `CommitReceipt` | Commit receipt. |
| `effect` | `MutationEffect` | Mutation effect facts. |
| `graph` | `string` | Graph name. |
| `node_id` | `string` | Node id. |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.graph_name`](https://stratadb.org/e/invalid_argument.engine.graph_name) | The graph request is invalid. |
| [`not_found.engine.graph`](https://stratadb.org/e/not_found.engine.graph) | The requested graph was not found. |
| [`invalid_argument.engine.graph_node_id`](https://stratadb.org/e/invalid_argument.engine.graph_node_id) | The graph request is invalid. |
| [`invalid_argument.engine.graph_properties`](https://stratadb.org/e/invalid_argument.engine.graph_properties) | The graph request is invalid. |
| [`invalid_argument.engine.graph_properties_too_large`](https://stratadb.org/e/invalid_argument.engine.graph_properties_too_large) | The graph request is invalid. |
| [`invalid_argument.engine.graph_type_hint`](https://stratadb.org/e/invalid_argument.engine.graph_type_hint) | The graph request is invalid. |
| [`invalid_argument.engine.graph_binding`](https://stratadb.org/e/invalid_argument.engine.graph_binding) | The graph request is invalid. |
| [`unsupported.engine.graph_binding_cross_branch`](https://stratadb.org/e/unsupported.engine.graph_binding_cross_branch) | Cross-branch graph relationship bindings are not supported. |
| [`failed_precondition.engine.graph_ontology_node_type`](https://stratadb.org/e/failed_precondition.engine.graph_ontology_node_type) | The graph request is invalid. |
| [`failed_precondition.engine.graph_ontology_required_property`](https://stratadb.org/e/failed_precondition.engine.graph_ontology_required_property) | The graph request is invalid. |

## Invocation

```text
strata graph add-node <graph> <node_id> [--binding <GraphEntityBinding>] [--object-type <string>] [--properties <any>] [--branch <branch>] [--space <space>]
```

- Wire type: `graph_add_node`

## Related

- [Create graph](/docs/graph/create) — Create a named graph.
- [Get graph node](/docs/graph/node/get) — Read one graph node.
- [All `graph` commands](/docs/graph/)
