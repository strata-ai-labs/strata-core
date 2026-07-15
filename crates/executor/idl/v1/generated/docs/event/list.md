---
title: "List events"
description: "List events with optional type filter and cursor."
source: strata-core@1.0.0
section: event
---

Lists events from the selected branch and space in sequence order. An optional event type narrows the results, the sequence cursor continues a previous page, and the optional timestamp reads the log visible at that commit time.

Paginated responses use opaque cursors. Clients should pass the returned cursor back to the same command shape and must not parse cursor contents.

## Examples

List events in sequence order.

### CLI

```console
$ strata event append user.created {"id":1}
$ strata event append user.updated {"id":2}
$ strata event list
```

### Wire

```json
{"event_type":"user.created","payload":{"id":1},"type":"event_append"}
{"event_type":"user.updated","payload":{"id":2},"type":"event_append"}
{"type":"event_list"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `after_sequence` | `integer` | no | — | Optional exclusive sequence cursor. |
| `as_of` | `integer` | no | — | Optional timestamp in microseconds. |
| `event_type` | `string` | no | — | Optional event type filter. |
| `limit` | `integer` | no | — | Optional item limit. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`Page<EventVersionedData, u64>`.

| Field | Type | Description |
|---|---|---|
| `has_more` | `boolean` |  |
| `items` | `EventVersionedData[]` | Events in this page. |
| `cursor` | `integer` |  |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.event_type`](https://stratadb.org/e/invalid_argument.engine.event_type) | The event request is invalid. |
| [`invalid_argument.executor.limit`](https://stratadb.org/e/invalid_argument.executor.limit) | The requested limit is invalid. |

## Invocation

```text
strata event list [--after-sequence <integer>] [--as-of <integer>] [--event-type <string>] [--limit <integer>] [--branch <branch>] [--space <space>]
```

- Wire type: `event_list`

## Related

- [Append event](/docs/event/append) — Append one event to the branch event log.
- [All `event` commands](/docs/event/)
