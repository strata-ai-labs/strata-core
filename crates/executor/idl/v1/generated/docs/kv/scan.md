---
title: "Scan KV rows"
description: "Scan KV rows with values and version facts."
source: strata-core@1.0.0
section: kv
---

Scans visible KV rows starting at an optional key. Each item includes the key, value, and commit metadata exposed by the executor output.

Paginated responses use opaque cursors. Clients should pass the returned cursor back to the same command shape and must not parse cursor contents.

## Examples

Scan full rows from the start, in key order.

### CLI

```console
$ strata command run --command-json '{"entries":[{"key":"YQ==","value":"MQ=="},{"key":"Yg==","value":"Mg=="}],"type":"kv_batch_put"}'
$ strata kv scan
```

### Wire

```json
{"entries":[{"key":"YQ==","value":"MQ=="},{"key":"Yg==","value":"Mg=="}],"type":"kv_batch_put"}
{"type":"kv_scan"}
```

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `limit` | `integer` | no | Optional row limit. |
| `start` | `Bytes` | no | Optional inclusive start key. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`Page<ScanItem, Bytes>`.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space)
- [`invalid_argument.engine.kv_key`](https://stratadb.org/e/invalid_argument.engine.kv_key)

## Invocation

- CLI: `strata kv scan`
- Wire type: `kv_scan`
