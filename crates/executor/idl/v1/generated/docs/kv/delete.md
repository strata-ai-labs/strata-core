---
title: "Delete KV value"
description: "Delete one visible KV key."
source: strata-core@1.0.0
section: kv
---

Deletes the current visible value for a KV key. Missing keys produce a no-op delete acknowledgement rather than a read-style missing value.

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Examples

Delete a key; it is no longer visible afterward.

### CLI

```console
$ strata kv put temp scratch
$ strata kv delete temp
$ strata kv exists temp
```

### Wire

```json
{"key":"dGVtcA==","type":"kv_put","value":"c2NyYXRjaA=="}
{"key":"dGVtcA==","type":"kv_delete"}
{"key":"dGVtcA==","type":"kv_exists"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"key":"dGVtcA=="},"type":"write_result"}
{"data":{"commit":{"delete_count":1,"durable":false,"put_count":0,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"deleted","matched":true},"key":"dGVtcA=="},"type":"delete_result"}
{"data":false,"type":"bool"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `key` | `Bytes` | yes | — | Key bytes. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<KvDelete>`.

| Field | Type | Description |
|---|---|---|
| `effect` | `MutationEffect` | Mutation effect facts. |
| `key` | `Bytes` | Target key. |
| `commit` | `CommitReceipt` | Commit receipt when a delete was applied. |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.kv_key`](https://stratadb.org/e/invalid_argument.engine.kv_key) | The KV request is invalid. |

## Invocation

```text
strata kv delete <key> [--branch <branch>] [--space <space>]
```

- Wire type: `kv_delete`

## Related

- [Check KV existence](/docs/kv/exists) — Check whether one KV key exists.
- [Put KV value](/docs/kv/put) — Store or replace a KV value by key.
- [All `kv` commands](/docs/kv/)
