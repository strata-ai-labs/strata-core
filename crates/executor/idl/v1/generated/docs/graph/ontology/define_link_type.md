---
title: "Define graph link type"
description: "Define a graph link type."
source: strata-core@1.0.0
section: graph
---

Declares a link type in the graph's ontology: a name, its source and target object types, an optional cardinality hint (for example `many-to-one`), and property definitions. Source and target must name declared object types by the time the ontology is frozen. After freezing, this command fails with `failed_precondition.engine.graph_ontology_frozen`.

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Examples

Define a link (edge) type between two object types.

### CLI

```console
$ strata graph create g
$ strata graph ontology define-object-type g person
$ strata graph ontology define-link-type g knows person person
$ strata graph ontology get g
```

### Wire

```json
{"graph":"g","type":"graph_create"}
{"graph":"g","name":"person","type":"graph_define_object_type"}
{"graph":"g","name":"knows","source":"person","target":"person","type":"graph_define_link_type"}
{"graph":"g","type":"graph_get_ontology"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"info":{"created_timestamp":3,"created_version":3,"edge_count":0,"graph":"g","node_count":0,"updated_timestamp":3,"updated_version":3}},"type":"graph_create_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"graph":"g","kind":"object","type_name":"person"},"type":"graph_ontology_write_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":5,"version":5},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"graph":"g","kind":"link","type_name":"knows"},"type":"graph_ontology_write_result"}
{"data":{"graph":"g","link_types":[{"name":"knows","source":"person","target":"person"}],"object_types":[{"name":"person"}],"status":"draft","timestamp":5,"version":5},"type":"graph_ontology_result"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `graph` | `string` | yes | — | Graph name. |
| `name` | `string` | yes | — | Link type name. |
| `source` | `string` | yes | — | Declared source object type. |
| `target` | `string` | yes | — | Declared target object type. |
| `cardinality` | `string` | no | — | Optional cardinality hint (e.g. `one-to-many`). |
| `properties` | `object` | no | — | Declared properties by name. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<GraphOntologyWrite>`.

| Field | Type | Description |
|---|---|---|
| `commit` | `CommitReceipt` | Commit receipt. |
| `effect` | `MutationEffect` | Mutation effect facts. |
| `graph` | `string` | Graph name. |
| `kind` | `string` | Type kind: `object` or `link`. |
| `type_name` | `string` | Defined type name. |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.graph_name`](https://stratadb.org/e/invalid_argument.engine.graph_name) | The graph request is invalid. |
| [`not_found.engine.graph`](https://stratadb.org/e/not_found.engine.graph) | The requested graph was not found. |
| [`invalid_argument.engine.graph_type_name`](https://stratadb.org/e/invalid_argument.engine.graph_type_name) | The graph request is invalid. |
| [`invalid_argument.engine.graph_type_name_reserved`](https://stratadb.org/e/invalid_argument.engine.graph_type_name_reserved) | The graph request is invalid. |
| [`invalid_argument.engine.graph_property_name`](https://stratadb.org/e/invalid_argument.engine.graph_property_name) | The graph request is invalid. |
| [`failed_precondition.engine.graph_ontology_frozen`](https://stratadb.org/e/failed_precondition.engine.graph_ontology_frozen) | The graph request is invalid. |

## Invocation

```text
strata graph ontology define-link-type <graph> <name> <source> <target> [--cardinality <string>] [--properties <object>] [--branch <branch>] [--space <space>]
```

- Wire type: `graph_define_link_type`

## Related

- [Create graph](/docs/graph/create) — Create a named graph.
- [Define graph object type](/docs/graph/ontology/define_object_type) — Define a graph object type.
- [Read graph ontology](/docs/graph/ontology/get) — Read the graph ontology.
- [All `graph` commands](/docs/graph/)
