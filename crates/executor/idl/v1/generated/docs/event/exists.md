---
title: "Check event existence"
description: "Check whether an event sequence exists."
source: strata-core@1.0.0
section: event
---

Checks whether one event sequence exists in the selected branch and space without returning the record.

Status commands return a scalar or compact status payload and do not mutate database state.

## Examples

Check whether an event sequence exists.

### CLI

```console
$ strata event append user.created {"id":1}
$ strata event exists 0
$ strata event exists 999
```

### Wire

```json
{"event_type":"user.created","payload":{"id":1},"type":"event_append"}
{"sequence":0,"type":"event_exists"}
{"sequence":999,"type":"event_exists"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":3,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"event_type":"user.created","sequence":0},"type":"event_append_result"}
{"data":true,"type":"bool"}
{"data":false,"type":"bool"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `sequence` | `integer` | yes | — | Event sequence. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`StatusValue<bool>`.

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |

## Invocation

```text
strata event exists <sequence> [--branch <branch>] [--space <space>]
```

- Wire type: `event_exists`

## Related

- [Append event](/docs/event/append) — Append one event to the branch event log.
- [All `event` commands](/docs/event/)
