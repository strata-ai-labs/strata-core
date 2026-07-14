---
title: "Get KV value"
description: "Read the current or historical value for one KV key."
source: strata-core@1.0.0
section: kv
---

Reads one KV key from the selected branch and space. The optional timestamp reads the value visible at that point in time when history is retained.

Optional reads distinguish present data from missing data. When version or timestamp facts exist on the executor output, SDK mappings should preserve them.

## Examples

Read a value back; a missing key returns nothing.

### CLI

```console
$ strata kv put greeting hello
$ strata kv get greeting
$ strata kv get absent
```

### Wire

```json
{"key":"Z3JlZXRpbmc=","type":"kv_put","value":"aGVsbG8="}
{"key":"Z3JlZXRpbmc=","type":"kv_get"}
{"key":"YWJzZW50","type":"kv_get"}
```

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `as_of` | `integer` | no | Optional timestamp in microseconds. |
| `key` | `Bytes` | yes | Key bytes. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`Maybe<VersionedValue>` — a miss returns nothing rather than raising.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space)
- [`invalid_argument.engine.kv_key`](https://stratadb.org/e/invalid_argument.engine.kv_key)

## Invocation

- CLI: `strata kv get`
- Wire type: `kv_get`
