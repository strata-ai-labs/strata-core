---
title: "Check KV existence"
description: "Check whether one KV key exists."
source: strata-core@1.0.0
section: kv
---

Returns a boolean status for one KV key without loading the stored value.

Status commands return a scalar or compact status payload and do not mutate database state.

## Examples

Check whether a key currently has a visible value.

### CLI

```console
$ strata kv put k v
$ strata kv exists k
$ strata kv exists absent
```

### Wire

```json
{"key":"aw==","type":"kv_put","value":"dg=="}
{"key":"aw==","type":"kv_exists"}
{"key":"YWJzZW50","type":"kv_exists"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"key":"aw=="},"type":"write_result"}
{"data":true,"type":"bool"}
{"data":false,"type":"bool"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `key` | `Bytes` | yes | — | Key to check. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`StatusValue<bool>`.

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.kv_key`](https://stratadb.org/e/invalid_argument.engine.kv_key) | The KV request is invalid. |

## Invocation

```text
strata kv exists <key> [--branch <branch>] [--space <space>]
```

- Wire type: `kv_exists`

## Related

- [Put KV value](/docs/kv/put) — Store or replace a KV value by key.
- [All `kv` commands](/docs/kv/)
