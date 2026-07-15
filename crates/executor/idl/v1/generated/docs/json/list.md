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

### Output

One response per step, in order:

```json
{"data":{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":3,"timestamp":3,"version":3},"items":[{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":3,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":0,"result":{"document_version":1},"status":"ok"},{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":3,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":1,"result":{"document_version":1},"status":"ok"},{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":3,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":2,"result":{"document_version":1},"status":"ok"}],"mode":"itemwise","status":"ok"},"type":"json_batch_results"}
{"data":{"cursor":null,"has_more":false,"items":["user:1","user:2"]},"type":"json_list_result"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `as_of` | `integer` | no | — | Optional timestamp in microseconds. |
| `cursor` | `string` | no | — | Optional document key cursor. |
| `limit` | `integer` | no | — | Optional item limit. |
| `prefix` | `string` | no | — | Optional document key prefix. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`Page<String, String>`.

| Field | Type | Description |
|---|---|---|
| `has_more` | `boolean` |  |
| `items` | `string[]` | Keys in this page. |
| `cursor` | `string` |  |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.json_document_id`](https://stratadb.org/e/invalid_argument.engine.json_document_id) | The JSON document request is invalid. |

## Invocation

```text
strata json list [--as-of <integer>] [--cursor <string>] [--limit <integer>] [--prefix <string>] [--branch <branch>] [--space <space>]
```

- Wire type: `json_list`

## Related

- [Batch set JSON values](/docs/json/batch_set) — Set multiple JSON values in one itemwise batch.
- [All `json` commands](/docs/json/)
