---
title: "List graph bindings for entity"
description: "Find graph nodes bound to an entity."
source: strata-core@1.0.0
section: graph
---

Searches every graph in the selected branch and space for nodes whose entity binding matches the given target (primitive, space, key). This is the reverse index of node bindings: given an entity, find the graph facts attached to it. Results paginate by an opaque cursor.

Paginated responses use opaque cursors. Clients should pass the returned cursor back to the same command shape and must not parse cursor contents.

## Examples

List graph nodes bound to a product entity.

### CLI

```console
$ strata graph create kb
$ strata graph add-node kb ada --binding {"target":{"key":"user:1","primitive":"kv","space":"default"}}  # Bind the node to a KV entity so retrieval can cross primitives.
$ strata command run --command-json '{"target":{"key":"user:1","primitive":"kv","space":"default"},"type":"graph_bindings_for_entity"}'
```

### Wire

```json
{"graph":"kb","type":"graph_create"}
{"binding":{"target":{"key":"user:1","primitive":"kv","space":"default"}},"graph":"kb","node_id":"ada","type":"graph_add_node"}
{"target":{"key":"user:1","primitive":"kv","space":"default"},"type":"graph_bindings_for_entity"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"info":{"created_timestamp":3,"created_version":3,"edge_count":0,"graph":"kb","node_count":0,"updated_timestamp":3,"updated_version":3}},"type":"graph_create_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"graph":"kb","node_id":"ada"},"type":"graph_node_write_result"}
{"data":{"cursor":null,"has_more":false,"items":[{"binding":{"target":{"key":"user:1","primitive":"kv","space":"default"}},"graph":"kb","node_id":"ada","timestamp":4,"version":4}]},"type":"graph_binding_page"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `target` | `GraphBindingTarget` | yes | — | Entity target to search for. |
| `as_of` | `integer` | no | — | Optional timestamp in microseconds. Reads the graph state visible at that instant. |
| `cursor` | `string` | no | — | Optional exclusive cursor. |
| `limit` | `integer` | no | 100 | Optional item limit. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`Page<GraphBindingHit, String>`.

| Field | Type | Description |
|---|---|---|
| `has_more` | `boolean` |  |
| `items` | `GraphBindingHit[]` | Binding hits in this page. |
| `cursor` | `string` |  |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.graph_binding`](https://stratadb.org/e/invalid_argument.engine.graph_binding) | The graph request is invalid. |

## Invocation

- CLI: via `strata command run` (no dedicated verb)
- Wire type: `graph_bindings_for_entity`

## Related

- [Create graph](/docs/graph/create) — Create a named graph.
- [Add graph node](/docs/graph/node/add) — Add or replace a graph node.
- [All `graph` commands](/docs/graph/)
