---
title: "List graphs"
description: "List graph names."
source: strata-core@1.0.0
section: graph
---

Lists graph names in lexicographic order. Accepts an optional item limit (default 100), an exclusive name cursor for continuation, and an `as_of` timestamp to list the graphs visible at an earlier instant.

Paginated responses use opaque cursors. Clients should pass the returned cursor back to the same command shape and must not parse cursor contents.

## Examples

List graphs.

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
| `as_of` | `integer` | no | — | Optional timestamp in microseconds. Reads the graph state visible at that instant. |
| `cursor` | `string` | no | — | Optional exclusive graph cursor. |
| `limit` | `integer` | no | 100 | Optional item limit. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`Page<String, String>`.

| Field | Type | Description |
|---|---|---|
| `has_more` | `boolean` |  |
| `items` | `string[]` | Graphs in this page. |
| `cursor` | `string` |  |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.graph_name`](https://stratadb.org/e/invalid_argument.engine.graph_name) | The graph request is invalid. |

## Invocation

```text
strata graph list [--as-of <integer>] [--cursor <string>] [--limit <integer>] [--branch <branch>] [--space <space>]
```

- Wire type: `graph_list`

## Related

- [Create graph](/docs/graph/create) — Create a named graph.
- [All `graph` commands](/docs/graph/)
