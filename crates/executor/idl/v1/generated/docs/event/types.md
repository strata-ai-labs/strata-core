---
title: "List event types"
description: "List distinct event types in the log."
source: strata-core@1.0.0
section: event
---

Lists the distinct event types visible in the selected branch and space in sorted order. The optional timestamp lists the types visible at that commit time.

Paginated responses use opaque cursors. Clients should pass the returned cursor back to the same command shape and must not parse cursor contents.

## Examples

List the distinct event types seen in the log.

### CLI

```console
$ strata event append user.created {"id":1}
$ strata event append user.updated {"id":2}
$ strata event types
```

### Wire

```json
{"event_type":"user.created","payload":{"id":1},"type":"event_append"}
{"event_type":"user.updated","payload":{"id":2},"type":"event_append"}
{"type":"event_list_types"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":3,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"event_type":"user.created","sequence":0},"type":"event_append_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":3,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"event_type":"user.updated","sequence":1},"type":"event_append_result"}
{"data":{"cursor":null,"has_more":false,"items":["user.created","user.updated"]},"type":"event_type_list"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `as_of` | `integer` | no | — | Optional timestamp in microseconds. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`Page<String, String>`.

| Field | Type | Description |
|---|---|---|
| `has_more` | `boolean` |  |
| `items` | `string[]` | Event types in this page. |
| `cursor` | `string` |  |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |

## Invocation

```text
strata event types [--as-of <integer>] [--branch <branch>] [--space <space>]
```

- Wire type: `event_list_types`

## Related

- [Append event](/docs/event/append) — Append one event to the branch event log.
- [All `event` commands](/docs/event/)
