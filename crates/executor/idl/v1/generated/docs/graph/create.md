---
title: "Create graph"
description: "Create a named graph."
source: strata-core@1.0.0
section: graph
---

Creates an empty named graph in the selected space and returns its metadata, including node and edge counts (zero at creation) and the create commit coordinates. A database can hold many graphs; graph names are unique per branch and space. Creating a name that already exists fails with `already_exists.engine.graph`.

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Examples

Create a named graph.

### CLI

```console
$ strata graph create social
$ strata graph list
```

### Wire

```json
{"graph":"social","type":"graph_create"}
{"type":"graph_list"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"info":{"created_timestamp":3,"created_version":3,"edge_count":0,"graph":"social","node_count":0,"updated_timestamp":3,"updated_version":3}},"type":"graph_create_result"}
{"data":{"cursor":null,"has_more":false,"items":["social"]},"type":"graph_name_page"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `graph` | `string` | yes | — | Graph name. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<GraphInfoData>`.

| Field | Type | Description |
|---|---|---|
| `commit` | `CommitReceipt` | Commit receipt. |
| `effect` | `MutationEffect` | Mutation effect facts. |
| `info` | `GraphInfoData` | Created graph metadata. |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.graph_name`](https://stratadb.org/e/invalid_argument.engine.graph_name) | The graph request is invalid. |
| [`already_exists.engine.graph`](https://stratadb.org/e/already_exists.engine.graph) | A graph with this name already exists. |
| [`invalid_argument.engine.graph_name_reserved`](https://stratadb.org/e/invalid_argument.engine.graph_name_reserved) | The graph request is invalid. |

## Invocation

```text
strata graph create <graph> [--branch <branch>] [--space <space>]
```

- Wire type: `graph_create`

## Related

- [List graphs](/docs/graph/list) — List graph names.
- [All `graph` commands](/docs/graph/)
