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

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"key":"YQ=="},"type":"write_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"key":"Yg=="},"type":"write_result"}
{"data":2,"type":"uint"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `as_of` | `integer` | no | — | Optional timestamp in microseconds. |
| `prefix` | `Bytes` | no | — | Optional key prefix. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`StatusValue<u64>`.

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.kv_key`](https://stratadb.org/e/invalid_argument.engine.kv_key) | The KV request is invalid. |

## Invocation

```text
strata kv count [--as-of <integer>] [--prefix <Bytes>] [--branch <branch>] [--space <space>]
```

- Wire type: `kv_count`

## Related

- [Put KV value](/docs/kv/put) — Store or replace a KV value by key.
- [All `kv` commands](/docs/kv/)
