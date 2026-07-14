---
title: "Count KV keys"
description: "Count visible KV keys."
source: strata-core@1.0.0
section: kv
---

Counts visible KV keys in the selected branch and space, optionally constrained by a key prefix.

Status commands return a scalar or compact status payload and do not mutate database state.

## Examples

Count the visible keys.

### CLI

```console
$ strata kv put a 1
$ strata kv put b 2
$ strata kv count
```

### Wire

```json
{"key":"YQ==","type":"kv_put","value":"MQ=="}
{"key":"Yg==","type":"kv_put","value":"Mg=="}
{"type":"kv_count"}
```

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `as_of` | `integer` | no | Optional timestamp in microseconds. |
| `prefix` | `Bytes` | no | Optional key prefix. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`StatusValue<u64>`.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space)
- [`invalid_argument.engine.kv_key`](https://stratadb.org/e/invalid_argument.engine.kv_key)

## Invocation

- CLI: `strata kv count`
- Wire type: `kv_count`
