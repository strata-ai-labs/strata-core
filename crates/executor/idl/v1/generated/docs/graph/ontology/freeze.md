---
title: "Freeze graph ontology"
description: "Freeze the graph ontology."
source: strata-core@1.0.0
section: graph
---

Validates the draft ontology and freezes it. Validation requires at least one declared type and rejects link types whose source or target reference undeclared object types (`failed_precondition.engine.graph_ontology_freeze`). After freezing, writes enforce declared node object types, required properties, and link-type endpoint rules; the ontology itself can no longer change (`failed_precondition.engine.graph_ontology_frozen`).

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Examples

Freeze the ontology so its types can no longer change.

### CLI

```console
$ strata graph create g
$ strata graph ontology define-object-type g person
$ strata graph ontology freeze g
$ strata graph ontology get g
```

### Wire

```json
{"graph":"g","type":"graph_create"}
{"graph":"g","name":"person","type":"graph_define_object_type"}
{"graph":"g","type":"graph_freeze_ontology"}
{"graph":"g","type":"graph_get_ontology"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"info":{"created_timestamp":3,"created_version":3,"edge_count":0,"graph":"g","node_count":0,"updated_timestamp":3,"updated_version":3}},"type":"graph_create_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"graph":"g","kind":"object","type_name":"person"},"type":"graph_ontology_write_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":5,"version":5},"graph":"g","link_types":0,"object_types":1},"type":"graph_ontology_freeze_result"}
{"data":{"graph":"g","object_types":[{"name":"person"}],"status":"frozen","timestamp":5,"version":5},"type":"graph_ontology_result"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `graph` | `string` | yes | — | Graph name. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<GraphOntologyFreeze>`.

| Field | Type | Description |
|---|---|---|
| `commit` | `CommitReceipt` | Commit receipt. |
| `graph` | `string` | Graph name. |
| `link_types` | `integer` | Frozen link type count. |
| `object_types` | `integer` | Frozen object type count. |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.graph_name`](https://stratadb.org/e/invalid_argument.engine.graph_name) | The graph request is invalid. |
| [`not_found.engine.graph`](https://stratadb.org/e/not_found.engine.graph) | The requested graph was not found. |
| [`failed_precondition.engine.graph_ontology_freeze`](https://stratadb.org/e/failed_precondition.engine.graph_ontology_freeze) | The graph request is invalid. |
| [`failed_precondition.engine.graph_ontology_frozen`](https://stratadb.org/e/failed_precondition.engine.graph_ontology_frozen) | The graph request is invalid. |

## Invocation

```text
strata graph ontology freeze <graph> [--branch <branch>] [--space <space>]
```

- Wire type: `graph_freeze_ontology`

## Related

- [Create graph](/docs/graph/create) — Create a named graph.
- [Define graph object type](/docs/graph/ontology/define_object_type) — Define a graph object type.
- [Read graph ontology](/docs/graph/ontology/get) — Read the graph ontology.
- [All `graph` commands](/docs/graph/)
