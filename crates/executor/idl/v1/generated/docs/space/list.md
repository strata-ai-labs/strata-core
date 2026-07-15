---
title: "List product spaces"
description: "List product spaces on a branch."
source: strata-core@1.0.0
section: space
---

Lists the product space names cataloged on the target branch as a terminal page. Every branch has a `default` space; additional spaces isolate data namespaces within the same branch.

Paginated responses use opaque cursors. Clients should pass the returned cursor back to the same command shape and must not parse cursor contents.

## Examples

List product spaces.

### CLI

```console
$ strata space create app
$ strata space list
```

### Wire

```json
{"space":"app","type":"space_create"}
{"type":"space_list"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"space":"app"},"type":"space_create_result"}
{"data":{"cursor":null,"has_more":false,"items":["app","default"]},"type":"space_list"}
```

## Parameters

_No parameters._

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`Page<String, String>`.

| Field | Type | Description |
|---|---|---|
| `has_more` | `boolean` |  |
| `items` | `string[]` | Spaces in this page. |
| `cursor` | `string` |  |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |

## Invocation

```text
strata space list [--branch <branch>] [--space <space>]
```

- Wire type: `space_list`

## Related

- [Create product space](/docs/space/create) — Create a product space on a branch.
- [All `space` commands](/docs/space/)
