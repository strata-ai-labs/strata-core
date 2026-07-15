---
title: "Count events"
description: "Count visible events in the log."
source: strata-core@1.0.0
section: event
---

Counts events visible in the selected branch and space. The optional timestamp counts the events visible at that commit time.

Status commands return a scalar or compact status payload and do not mutate database state.

## Examples

Count the events in the log.

### CLI

```console
$ strata event append user.created {"id":1}
$ strata event append user.updated {"id":2}
$ strata event count
```

### Wire

```json
{"event_type":"user.created","payload":{"id":1},"type":"event_append"}
{"event_type":"user.updated","payload":{"id":2},"type":"event_append"}
{"type":"event_count"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":3,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"event_type":"user.created","sequence":0},"type":"event_append_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":3,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"event_type":"user.updated","sequence":1},"type":"event_append_result"}
{"data":{"count":2},"type":"event_count"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `as_of` | `integer` | no | — | Optional timestamp in microseconds. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`StatusValue<u64>`.

| Field | Type | Description |
|---|---|---|
| `count` | `integer` | Visible event count. |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |

## Invocation

```text
strata event count [--as-of <integer>] [--branch <branch>] [--space <space>]
```

- Wire type: `event_count`

## Related

- [Append event](/docs/event/append) — Append one event to the branch event log.
- [All `event` commands](/docs/event/)
