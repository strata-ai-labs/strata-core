---
title: "Get event"
description: "Read one event by sequence number."
source: strata-core@1.0.0
section: event
---

Reads one event from the selected branch and space by sequence number. The optional timestamp reads the record visible at that commit time. The record carries its payload, append timestamp, and hash-chain facts.

Optional reads distinguish present data from missing data. When version or timestamp facts exist on the executor output, SDK mappings should preserve them.

## Examples

Read an event by its sequence number.

### CLI

```console
$ strata event append user.created {"id":1}
$ strata event get 0
$ strata event get 999
```

### Wire

```json
{"event_type":"user.created","payload":{"id":1},"type":"event_append"}
{"sequence":0,"type":"event_get"}
{"sequence":999,"type":"event_get"}
```

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `as_of` | `integer` | no | Optional timestamp in microseconds. |
| `sequence` | `integer` | yes | Event sequence. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`Maybe<EventVersionedData>` — a miss returns nothing rather than raising.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space)

## Invocation

- CLI: `strata event get`
- Wire type: `event_get`
