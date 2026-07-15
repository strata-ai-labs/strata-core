---
title: "Read event sequence range"
description: "Read a range of events by sequence number."
source: strata-core@1.0.0
section: event
---

Reads events from the selected branch and space by sequence range. The start sequence is inclusive and the optional end sequence is exclusive; reverse direction walks backward from the start sequence. An optional event type narrows the results.

Paginated responses use opaque cursors. Clients should pass the returned cursor back to the same command shape and must not parse cursor contents.

## Examples

Read a range of events by sequence.

### CLI

```console
$ strata event append user.created {"id":1}
$ strata event append user.updated {"id":2}
$ strata event range 0 forward
```

### Wire

```json
{"event_type":"user.created","payload":{"id":1},"type":"event_append"}
{"event_type":"user.updated","payload":{"id":2},"type":"event_append"}
{"direction":"forward","start_seq":0,"type":"event_range"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `direction` | `EventRangeDirection` | yes | — | Result ordering. |
| `start_seq` | `integer` | yes | — | Inclusive start sequence; with reverse direction, walk backward from this sequence. |
| `end_seq` | `integer` | no | — | Optional exclusive end sequence; with reverse direction, exclusive lower bound. |
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
strata event range <start_seq> <direction> [--end-seq <integer>] [--event-type <string>] [--limit <integer>] [--branch <branch>] [--space <space>]
```

- Wire type: `event_range`

## Related

- [Append event](/docs/event/append) — Append one event to the branch event log.
- [All `event` commands](/docs/event/)
