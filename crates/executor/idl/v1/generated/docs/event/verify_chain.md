---
title: "Verify event chain"
description: "Verify event log density and hash linkage."
source: strata-core@1.0.0
section: event
---

Verifies that the visible event log in the selected branch and space is dense and hash-linked: sequences are contiguous from zero, the genesis record links to the all-zeros hash, and every record's hash matches its content and predecessor.

Status commands return a scalar or compact status payload and do not mutate database state.

## Examples

Verify the integrity of the event hash chain.

### CLI

```console
$ strata event append user.created {"id":1}
$ strata event verify-chain
```

### Wire

```json
{"event_type":"user.created","payload":{"id":1},"type":"event_append"}
{"type":"event_verify_chain"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":3,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"event_type":"user.created","sequence":0},"type":"event_append_result"}
{"data":{"error":null,"first_invalid":null,"length":1,"valid":true},"type":"event_chain_verification"}
```

## Parameters

_No parameters._

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`StatusValue<EventChainVerification>`.

| Field | Type | Description |
|---|---|---|
| `length` | `integer` |  |
| `valid` | `boolean` |  |
| `error` | `string` |  |
| `first_invalid` | `integer` |  |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |

## Invocation

```text
strata event verify-chain [--branch <branch>] [--space <space>]
```

- Wire type: `event_verify_chain`

## Related

- [Append event](/docs/event/append) — Append one event to the branch event log.
- [All `event` commands](/docs/event/)
