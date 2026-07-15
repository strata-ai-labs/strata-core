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

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"key":"Z3JlZXRpbmc="},"type":"write_result"}
{"data":{"found":true,"value":{"timestamp":3,"value":"aGVsbG8=","version":3}},"type":"kv_versioned_value"}
{"data":{"found":false,"value":null},"type":"kv_versioned_value"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `key` | `Bytes` | yes | — | Key bytes. |
| `as_of` | `integer` | no | — | Optional timestamp in microseconds. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`Maybe<VersionedValue>` — a miss returns nothing rather than raising.

| Field | Type | Description |
|---|---|---|
| `found` | `boolean` |  |
| `value` | `VersionedValue` |  |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.kv_key`](https://stratadb.org/e/invalid_argument.engine.kv_key) | The KV request is invalid. |

## Invocation

```text
strata kv get <key> [--as-of <integer>] [--branch <branch>] [--space <space>]
```

- Wire type: `kv_get`

## Related

- [Put KV value](/docs/kv/put) — Store or replace a KV value by key.
- [All `kv` commands](/docs/kv/)
