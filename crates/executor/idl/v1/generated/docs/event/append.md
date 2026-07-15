---
title: "Append event"
description: "Append one event to the branch event log."
source: strata-core@1.0.0
section: event
---

Appends one event to the selected branch and space. Strata assigns the next sequence number, stamps the event with its append timestamp, and links it into the tamper-evident hash chain. Events are immutable once appended.

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Examples

Append an event to the log.

### CLI

```console
$ strata event append user.created {"id":1}
$ strata event count
```

### Wire

```json
{"event_type":"user.created","payload":{"id":1},"type":"event_append"}
{"type":"event_count"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":3,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"event_type":"user.created","sequence":0},"type":"event_append_result"}
{"data":{"count":1},"type":"event_count"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `event_type` | `string` | yes | — | Event type. |
| `payload` | `any` | yes | — | Event payload. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<EventAppend>`.

| Field | Type | Description |
|---|---|---|
| `commit` | `CommitReceipt` | Commit receipt. |
| `effect` | `MutationEffect` | Mutation effect facts. |
| `event_type` | `string` | Appended event type. |
| `sequence` | `integer` | Assigned sequence. |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.event_type`](https://stratadb.org/e/invalid_argument.engine.event_type) | The event request is invalid. |
| [`invalid_argument.engine.event_payload`](https://stratadb.org/e/invalid_argument.engine.event_payload) | The event request is invalid. |
| [`invalid_argument.engine.event_payload_too_large`](https://stratadb.org/e/invalid_argument.engine.event_payload_too_large) | The event request is invalid. |

## Invocation

```text
strata event append <event_type> <payload> [--branch <branch>] [--space <space>]
```

- Wire type: `event_append`

## Related

- [Count events](/docs/event/count) — Count visible events in the log.
- [All `event` commands](/docs/event/)
