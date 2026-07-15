---
title: "Batch append events"
description: "Append multiple events in one commit."
source: strata-core@1.0.0
section: event
---

Appends multiple events to the selected branch and space in one engine commit. Sequences are assigned in entry order. Entries that fail validation report a positional item error while valid entries still append.

Itemwise batches return one positional item result per input item. The outer batch status summarizes whether all, some, or none of the items succeeded.

## Examples

Append many events in one commit.

### CLI

```console
$ strata command run --command-json '{"entries":[{"event_type":"user.created","payload":{"id":1}},{"event_type":"user.updated","payload":{"id":2}}],"type":"event_batch_append"}'
$ strata event count
```

### Wire

```json
{"entries":[{"event_type":"user.created","payload":{"id":1}},{"event_type":"user.updated","payload":{"id":2}}],"type":"event_batch_append"}
{"type":"event_count"}
```

### Output

One response per step, in order:

```json
{"data":{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":5,"timestamp":3,"version":3},"items":[{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":5,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":0,"result":{"event_type":"user.created","sequence":0},"status":"ok"},{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":5,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":1,"result":{"event_type":"user.updated","sequence":1},"status":"ok"}],"mode":"itemwise","status":"ok"},"type":"event_batch_append_results"}
{"data":{"count":2},"type":"event_count"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `entries` | `BatchEventEntry[]` | yes | — | Events to append. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`BatchResult<EventBatchAppendItem>`.

| Field | Type | Description |
|---|---|---|
| `applied` | `boolean` |  |
| `items` | `BatchItem9[]` |  |
| `mode` | `BatchMode` |  |
| `status` | `BatchStatus` |  |
| `commit` | `CommitReceipt` |  |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.event_batch`](https://stratadb.org/e/invalid_argument.engine.event_batch) | The event request is invalid. |
| [`invalid_argument.engine.event_type`](https://stratadb.org/e/invalid_argument.engine.event_type) | The event request is invalid. |
| [`invalid_argument.engine.event_payload`](https://stratadb.org/e/invalid_argument.engine.event_payload) | The event request is invalid. |
| [`invalid_argument.engine.event_payload_too_large`](https://stratadb.org/e/invalid_argument.engine.event_payload_too_large) | The event request is invalid. |

## Invocation

- CLI: via `strata command run` (no dedicated verb)
- Wire type: `event_batch_append`

## Related

- [Count events](/docs/event/count) — Count visible events in the log.
- [All `event` commands](/docs/event/)
