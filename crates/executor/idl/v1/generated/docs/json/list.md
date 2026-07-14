---
title: "List JSON document keys"
description: "List JSON document keys with optional prefix filtering."
source: strata-core@1.0.0
section: json
---

Lists visible JSON document keys in byte order. Prefix, cursor, limit, and timestamp parameters constrain the page returned by the executor.

Paginated responses use opaque cursors. Clients should pass the returned cursor back to the same command shape and must not parse cursor contents.

## Examples

List document keys under a prefix, in key order.

### CLI

```console
$ strata command run --command-json '{"entries":[{"key":"user:1","path":"$","value":{"v":1}},{"key":"user:2","path":"$","value":{"v":2}},{"key":"other","path":"$","value":{"v":3}}],"type":"json_batch_set"}'
$ strata json list --prefix user:
```

### Wire

```json
{"entries":[{"key":"user:1","path":"$","value":{"v":1}},{"key":"user:2","path":"$","value":{"v":2}},{"key":"other","path":"$","value":{"v":3}}],"type":"json_batch_set"}
{"prefix":"user:","type":"json_list"}
```

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `as_of` | `integer` | no | Optional timestamp in microseconds. |
| `cursor` | `string` | no | Optional document key cursor. |
| `limit` | `integer` | no | Optional item limit. |
| `prefix` | `string` | no | Optional document key prefix. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`Page<String, String>`.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space)
- [`invalid_argument.engine.json_document_id`](https://stratadb.org/e/invalid_argument.engine.json_document_id)

## Invocation

- CLI: `strata json list`
- Wire type: `json_list`
