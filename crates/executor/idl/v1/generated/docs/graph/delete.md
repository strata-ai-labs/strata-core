---
title: "Delete graph"
description: "Delete a graph and its visible data."
source: strata-core@1.0.0
section: graph
---

Deletes a named graph and every visible node, edge, binding, and ontology row it owns. Deleting a graph that does not exist is not an error: the acknowledgement reports `deleted: false` with a `not_found` effect. Earlier states remain readable through time travel on other commands.

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Examples

Delete a graph.

### CLI

```console
$ strata graph create temp
$ strata graph delete temp
$ strata graph list
```

### Wire

```json
{"graph":"temp","type":"graph_create"}
{"graph":"temp","type":"graph_delete"}
{"type":"graph_list"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"info":{"created_timestamp":3,"created_version":3,"edge_count":0,"graph":"temp","node_count":0,"updated_timestamp":3,"updated_version":3}},"type":"graph_create_result"}
{"data":{"commit":{"delete_count":1,"durable":false,"put_count":0,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"deleted","matched":true},"graph":"temp"},"type":"graph_delete_result"}
{"data":{"cursor":null,"has_more":false,"items":[]},"type":"graph_name_page"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `graph` | `string` | yes | — | Graph name. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<GraphDelete>`.

| Field | Type | Description |
|---|---|---|
| `effect` | `MutationEffect` | Mutation effect facts. |
| `graph` | `string` | Graph name. |
| `commit` | `CommitReceipt` | Commit receipt when a delete was applied. |
| `dst` | `string` | Deleted edge destination for edge deletes. |
| `edge_type` | `string` | Deleted edge type for edge deletes. |
| `node_id` | `string` | Deleted node id for node deletes. |
| `src` | `string` | Deleted edge source for edge deletes. |

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
strata graph delete <graph> [--branch <branch>] [--space <space>]
```

- Wire type: `graph_delete`

## Related

- [Create graph](/docs/graph/create) — Create a named graph.
- [List graphs](/docs/graph/list) — List graph names.
- [All `graph` commands](/docs/graph/)
