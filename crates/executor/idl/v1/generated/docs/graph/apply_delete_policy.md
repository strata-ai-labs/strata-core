---
title: "Apply graph delete policy"
description: "Apply a delete policy to bound graph facts."
source: strata-core@1.0.0
section: graph
---

Applies an explicit policy to every graph node bound to the given entity target: `cascade` deletes the bound nodes and their incident edges, `detach` keeps the nodes but removes their bindings, and `keep_dangling` preserves the bindings so traversal can report the target's status. The acknowledgement reports how many bound nodes the policy covered.

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Examples

Cascade-delete graph facts bound to an entity.

### CLI

```console
$ strata graph create kb
$ strata graph add-node kb ada --binding {"target":{"key":"user:1","primitive":"kv","space":"default"}}
$ strata command run --command-json '{"policy":"cascade","target":{"key":"user:1","primitive":"kv","space":"default"},"type":"graph_apply_delete_policy"}'  # cascade removes the bound node and its incident edges.
$ strata graph get-node kb ada
```

### Wire

```json
{"graph":"kb","type":"graph_create"}
{"binding":{"target":{"key":"user:1","primitive":"kv","space":"default"}},"graph":"kb","node_id":"ada","type":"graph_add_node"}
{"policy":"cascade","target":{"key":"user:1","primitive":"kv","space":"default"},"type":"graph_apply_delete_policy"}
{"graph":"kb","node_id":"ada","type":"graph_get_node"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"info":{"created_timestamp":3,"created_version":3,"edge_count":0,"graph":"kb","node_count":0,"updated_timestamp":3,"updated_version":3}},"type":"graph_create_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"graph":"kb","node_id":"ada"},"type":"graph_node_write_result"}
{"data":{"commit":{"delete_count":1,"durable":false,"put_count":0,"timestamp":5,"version":5},"effect":{"affected_count":1,"applied":true,"kind":"deleted","matched":true},"policy":"cascade"},"type":"graph_delete_policy_result"}
{"data":{"found":false,"value":null},"type":"graph_node_result"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `policy` | `GraphDeletePolicy` | yes | — | Policy to apply: `cascade`, `detach`, or `keep_dangling`. |
| `target` | `GraphBindingTarget` | yes | — | The bound entity target. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<GraphDeletePolicyApply>`.

| Field | Type | Description |
|---|---|---|
| `effect` | `MutationEffect` | Mutation effect facts. The number of bound nodes the policy covered is reported by `effect.affected_count`. |
| `policy` | `string` | Applied policy: `cascade`, `detach`, or `keep_dangling`. |
| `commit` | `CommitReceipt` | Commit receipt when rows changed. |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.graph_binding`](https://stratadb.org/e/invalid_argument.engine.graph_binding) | The graph request is invalid. |

## Invocation

- CLI: via `strata command run` (no dedicated verb)
- Wire type: `graph_apply_delete_policy`

## Related

- [Create graph](/docs/graph/create) — Create a named graph.
- [Add graph node](/docs/graph/node/add) — Add or replace a graph node.
- [Get graph node](/docs/graph/node/get) — Read one graph node.
- [All `graph` commands](/docs/graph/)
