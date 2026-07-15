---
title: "Scan JSON documents"
description: "Scan JSON documents with values and version facts."
source: strata-core@1.0.0
section: json
---

Scans visible JSON documents starting at an optional document key. Each item includes the key, full document value, and commit metadata exposed by the executor output.

Paginated responses use opaque cursors. Clients should pass the returned cursor back to the same command shape and must not parse cursor contents.

## Examples

Scan documents from the start, in key order.

### CLI

```console
$ strata command run --command-json '{"entries":[{"key":"a","path":"$","value":{"v":1}},{"key":"b","path":"$","value":{"v":2}}],"type":"json_batch_set"}'
$ strata json scan
```

### Wire

```json
{"entries":[{"key":"a","path":"$","value":{"v":1}},{"key":"b","path":"$","value":{"v":2}}],"type":"json_batch_set"}
{"type":"json_scan"}
```

### Output

One response per step, in order:

```json
{"data":{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":2,"timestamp":3,"version":3},"items":[{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":2,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":0,"result":{"document_version":1},"status":"ok"},{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":2,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":1,"result":{"document_version":1},"status":"ok"}],"mode":"itemwise","status":"ok"},"type":"json_batch_results"}
{"data":{"cursor":null,"has_more":false,"items":[{"document_version":1,"key":"a","timestamp":3,"value":{"v":1},"version":3},{"document_version":1,"key":"b","timestamp":3,"value":{"v":2},"version":3}]},"type":"json_scan_result"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `limit` | `integer` | no | — | Optional row limit. |
| `start` | `string` | no | — | Optional inclusive start document key. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`Page<JsonSampleItem, String>`.

| Field | Type | Description |
|---|---|---|
| `has_more` | `boolean` |  |
| `items` | `JsonSampleItem[]` | Documents in this page. |
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
strata json scan [--limit <integer>] [--start <string>] [--branch <branch>] [--space <space>]
```

- Wire type: `json_scan`

## Related

- [Batch set JSON values](/docs/json/batch_set) — Set multiple JSON values in one itemwise batch.
- [All `json` commands](/docs/json/)
