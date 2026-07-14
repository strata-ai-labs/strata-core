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

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `as_of` | `integer` | no | Optional timestamp in microseconds. |
| `cursor` | `Bytes` | no | Optional key cursor. |
| `limit` | `integer` | no | Optional item limit. Defaults to 100. |
| `prefix` | `Bytes` | no | Optional key prefix. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`Page<Bytes, Bytes>`.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space)
- [`invalid_argument.engine.kv_key`](https://stratadb.org/e/invalid_argument.engine.kv_key)

## Invocation

- CLI: `strata kv list`
- Wire type: `kv_list`
