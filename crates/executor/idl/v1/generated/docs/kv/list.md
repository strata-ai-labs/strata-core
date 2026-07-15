---
title: "List KV keys"
description: "List KV keys with optional prefix filtering."
source: strata-core@1.0.0
section: kv
---

Lists visible KV keys in byte order. Prefix, cursor, limit, and timestamp parameters constrain the page returned by the executor.

Paginated responses use opaque cursors. Clients should pass the returned cursor back to the same command shape and must not parse cursor contents.

## Examples

List keys under a prefix, in key order.

### CLI

```console
$ strata command run --command-json '{"entries":[{"key":"dXNlcjox","value":"YQ=="},{"key":"dXNlcjoy","value":"Yg=="},{"key":"b3RoZXI=","value":"Yw=="}],"type":"kv_batch_put"}'
$ strata kv list --prefix user:
```

### Wire

```json
{"entries":[{"key":"dXNlcjox","value":"YQ=="},{"key":"dXNlcjoy","value":"Yg=="},{"key":"b3RoZXI=","value":"Yw=="}],"type":"kv_batch_put"}
{"prefix":"dXNlcjo=","type":"kv_list"}
```

### Output

One response per step, in order:

```json
{"data":{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":3,"timestamp":3,"version":3},"items":[{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":3,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":0,"result":{"key":"dXNlcjox"},"status":"ok"},{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":3,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":1,"result":{"key":"dXNlcjoy"},"status":"ok"},{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":3,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":2,"result":{"key":"b3RoZXI="},"status":"ok"}],"mode":"itemwise","status":"ok"},"type":"batch_results"}
{"data":{"cursor":null,"has_more":false,"items":["dXNlcjox","dXNlcjoy"]},"type":"keys_page"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `as_of` | `integer` | no | — | Optional timestamp in microseconds. |
| `cursor` | `Bytes` | no | — | Optional key cursor. |
| `limit` | `integer` | no | 100 | Optional item limit. |
| `prefix` | `Bytes` | no | — | Optional key prefix. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`Page<Bytes, Bytes>`.

| Field | Type | Description |
|---|---|---|
| `has_more` | `boolean` |  |
| `items` | `Bytes[]` | Keys in this page. |
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
strata kv list [--as-of <integer>] [--cursor <Bytes>] [--limit <integer>] [--prefix <Bytes>] [--branch <branch>] [--space <space>]
```

- Wire type: `kv_list`

## Related

- [Batch put KV values](/docs/kv/batch_put) — Store multiple KV values in one itemwise batch.
- [All `kv` commands](/docs/kv/)
