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

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `key` | `Bytes` | yes | Key bytes. |
| `value` | `Bytes` | yes | Value bytes. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<KvWrite>`.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space)
- [`invalid_argument.engine.kv_key`](https://stratadb.org/e/invalid_argument.engine.kv_key)

## Invocation

- CLI: `strata kv put`
- Wire type: `kv_put`
