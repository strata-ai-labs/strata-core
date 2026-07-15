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

### Output

One response per step, in order:

```json
{"data":{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":2,"timestamp":3,"version":3},"items":[{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":2,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":0,"result":{"key":"YQ=="},"status":"ok"},{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":2,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":1,"result":{"key":"Yg=="},"status":"ok"}],"mode":"itemwise","status":"ok"},"type":"batch_results"}
{"data":{"cursor":null,"has_more":false,"items":[{"key":"YQ==","timestamp":3,"value":"MQ==","version":3},{"key":"Yg==","timestamp":3,"value":"Mg==","version":3}]},"type":"kv_scan_result"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `limit` | `integer` | no | — | Optional row limit. |
| `start` | `Bytes` | no | — | Optional inclusive start key. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`Page<ScanItem, Bytes>`.

| Field | Type | Description |
|---|---|---|
| `has_more` | `boolean` |  |
| `items` | `ScanItem[]` | Rows in this page. |
| `cursor` | `Bytes` |  |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.kv_key`](https://stratadb.org/e/invalid_argument.engine.kv_key) | The KV request is invalid. |

## Invocation

```text
strata kv scan [--limit <integer>] [--start <Bytes>] [--branch <branch>] [--space <space>]
```

- Wire type: `kv_scan`

## Related

- [Batch put KV values](/docs/kv/batch_put) — Store multiple KV values in one itemwise batch.
- [All `kv` commands](/docs/kv/)
