---
title: "List graph bindings for entity"
description: "Find graph nodes bound to an entity."
source: strata-core@1.0.0
section: graph
---

Searches every graph in the selected branch and space for nodes whose entity binding matches the given target (primitive, space, key). This is the reverse index of node bindings: given an entity, find the graph facts attached to it. Results paginate by an opaque cursor.

Paginated responses use opaque cursors. Clients should pass the returned cursor back to the same command shape and must not parse cursor contents.

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `as_of` | `integer` | no | Optional timestamp in microseconds. Reads the graph state visible at that instant. |
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
