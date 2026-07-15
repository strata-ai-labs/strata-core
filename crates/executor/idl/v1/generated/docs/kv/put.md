---
title: "Put KV value"
description: "Store or replace a KV value by key."
source: strata-core@1.0.0
section: kv
---

Writes a binary value to the selected KV space. If the key already exists, Strata replaces the visible value and records a new version.

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Examples

Store a value, then replace it.

### CLI

```console
$ strata kv put setting v1
$ strata kv put setting v2  # replaces the visible value
$ strata kv get setting
```

### Wire

```json
{"key":"c2V0dGluZw==","type":"kv_put","value":"djE="}
{"key":"c2V0dGluZw==","type":"kv_put","value":"djI="}
{"key":"c2V0dGluZw==","type":"kv_get"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"key":"c2V0dGluZw=="},"type":"write_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"updated","matched":true},"key":"c2V0dGluZw=="},"type":"write_result"}
{"data":{"found":true,"value":{"timestamp":4,"value":"djI=","version":4}},"type":"kv_versioned_value"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `key` | `Bytes` | yes | — | Key bytes. |
| `value` | `Bytes` | yes | — | Value bytes. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<KvWrite>`.

| Field | Type | Description |
|---|---|---|
| `commit` | `CommitReceipt` | Commit receipt. |
| `effect` | `MutationEffect` | Mutation effect facts. |
| `key` | `Bytes` | Written key. |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.kv_key`](https://stratadb.org/e/invalid_argument.engine.kv_key) | The KV request is invalid. |

## Invocation

```text
strata kv put <key> <value> [--branch <branch>] [--space <space>]
```

- Wire type: `kv_put`

## Related

- [Get KV value](/docs/kv/get) — Read the current or historical value for one KV key.
- [All `kv` commands](/docs/kv/)
