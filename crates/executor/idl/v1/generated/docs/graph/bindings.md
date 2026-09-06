---
title: "List graph bindings for entity"
description: "Find graph nodes bound to an entity."
source: strata-core@1.2.0
section: graph
---

Searches every graph in the selected branch and space for nodes whose entity binding matches the given target (primitive, space, key). This is the reverse index of node bindings: given an entity, find the graph facts attached to it. Results paginate by an opaque cursor.

Paginated responses use opaque cursors. Clients should pass the returned cursor back to the same command shape and must not parse cursor contents.

## Examples

List graph nodes bound to a product entity.

### CLI

```console
$ strata graph create kb
$ strata command run --command-json '{"binding":{"target":{"key":"user:1","primitive":"kv","space":"default"}},"graph":"kb","node_id":"ada","type":"graph_add_node"}'  # Bind the node to a KV entity so retrieval can cross primitives.
$ strata command run --command-json '{"target":{"key":"user:1","primitive":"kv","space":"default"},"type":"graph_bindings_for_entity"}'
```

### Wire

```json
{"graph":"kb","type":"graph_create"}
{"binding":{"target":{"key":"user:1","primitive":"kv","space":"default"}},"graph":"kb","node_id":"ada","type":"graph_add_node"}
{"target":{"key":"user:1","primitive":"kv","space":"default"},"type":"graph_bindings_for_entity"}
```

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `as_of` | `integer` | no | Optional read-as-of commit timestamp: the `timestamp` from `history` output (a commit-timeline position, not the `version`). Reads the graph state visible at that instant. |
| `as_of_time` | `integer` | no | Optional read-as-of wall-clock instant, in microseconds since the Unix epoch (UTC): the `committed_at` from a write ack. Resolves to the commit at or before that instant. Mutually exclusive with `as_of`. |
| `cursor` | `string` | no | Optional exclusive cursor. |
| `limit` | `integer` | no | Optional item limit. Defaults to 100. |
| `target` | `GraphBindingTarget` | yes | Entity target to search for. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`Page<GraphBindingHit, String>`.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space)
- [`invalid_argument.engine.graph_binding`](https://stratadb.org/e/invalid_argument.engine.graph_binding)

## Invocation

- CLI: via `strata command run` (no dedicated verb)
- Wire type: `graph_bindings_for_entity`
