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

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `as_of` | `integer` | no | Optional timestamp in microseconds. Reads the graph state visible at that instant. |
| `graph` | `string` | yes | Graph name. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`Maybe<GraphOntologySummaryData>` — a miss returns nothing rather than raising.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space)
- [`invalid_argument.engine.graph_name`](https://stratadb.org/e/invalid_argument.engine.graph_name)
- [`not_found.engine.graph`](https://stratadb.org/e/not_found.engine.graph)

## Invocation

- CLI: `strata graph ontology summary`
- Wire type: `graph_ontology_summary`
