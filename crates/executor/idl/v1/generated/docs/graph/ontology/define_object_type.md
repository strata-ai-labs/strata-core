---
title: "Define graph object type"
description: "Define a graph object type."
source: strata-core@1.0.0
section: graph
---

Declares an object type in the graph's ontology: a name plus property definitions (`value_type`, `required`). While the ontology is a draft, redefining a type replaces it freely. After `graph.ontology.freeze`, the ontology is immutable and this command fails with `failed_precondition.engine.graph_ontology_frozen`.

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Examples

Define an object (node) type.

### CLI

```console
$ strata graph create g
$ strata graph ontology define-object-type g person
$ strata graph ontology summary g
```

### Wire

```json
{"graph":"g","type":"graph_create"}
{"graph":"g","name":"person","type":"graph_define_object_type"}
{"graph":"g","type":"graph_ontology_summary"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"info":{"created_timestamp":3,"created_version":3,"edge_count":0,"graph":"g","node_count":0,"updated_timestamp":3,"updated_version":3}},"type":"graph_create_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"graph":"g","kind":"object","type_name":"person"},"type":"graph_ontology_write_result"}
{"data":{"graph":"g","object_types":[{"name":"person","node_count":0}],"status":"draft","timestamp":4,"version":4},"type":"graph_ontology_summary_result"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `graph` | `string` | yes | — | Graph name. |
| `name` | `string` | yes | — | Object type name. |
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
strata graph ontology define-object-type <graph> <name> [--properties <object>] [--branch <branch>] [--space <space>]
```

- Wire type: `graph_define_object_type`

## Related

- [Create graph](/docs/graph/create) — Create a named graph.
- [Read graph ontology summary](/docs/graph/ontology/summary) — Read the ontology with usage counts.
- [All `graph` commands](/docs/graph/)
