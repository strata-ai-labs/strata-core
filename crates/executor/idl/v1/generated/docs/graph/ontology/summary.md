---
title: "Read graph ontology summary"
description: "Read the ontology with usage counts."
source: strata-core@1.0.0
section: graph
---

Reads the graph's ontology annotated with per-type usage: a node count for each object type and an edge count for each link type. Returns no data before any type has been declared. Accepts `as_of` for time travel.

Optional reads distinguish present data from missing data. When version or timestamp facts exist on the executor output, SDK mappings should preserve them.

## Examples

Summarize the ontology's object types.

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
| `as_of` | `integer` | no | — | Optional timestamp in microseconds. Reads the graph state visible at that instant. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`Maybe<GraphOntologySummaryData>` — a miss returns nothing rather than raising.

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
strata graph ontology summary <graph> [--as-of <integer>] [--branch <branch>] [--space <space>]
```

- Wire type: `graph_ontology_summary`

## Related

- [Create graph](/docs/graph/create) — Create a named graph.
- [Define graph object type](/docs/graph/ontology/define_object_type) — Define a graph object type.
- [All `graph` commands](/docs/graph/)
