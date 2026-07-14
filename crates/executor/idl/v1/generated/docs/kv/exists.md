---
title: "Check KV existence"
description: "Check whether one KV key exists."
source: strata-core@1.0.0
section: kv
---

Returns a boolean status for one KV key without loading the stored value.

Status commands return a scalar or compact status payload and do not mutate database state.

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `key` | `Bytes` | yes | Key to check. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`StatusValue<bool>`.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space)
- [`invalid_argument.engine.kv_key`](https://stratadb.org/e/invalid_argument.engine.kv_key)

## Invocation

- CLI: `strata kv exists`
- Wire type: `kv_exists`
