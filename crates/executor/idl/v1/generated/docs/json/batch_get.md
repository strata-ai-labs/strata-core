---
title: "Batch get JSON values"
description: "Read multiple JSON values by document and path."
source: strata-core@1.0.0
section: json
---

Reads several document/path entries and returns positional item results. Each item records whether the value was found and includes version metadata when present.

Itemwise batches return one positional item result per input item. The outer batch status summarizes whether all, some, or none of the items succeeded.

## Examples

Read many documents at once.

### CLI

```console
$ strata command run --command-json '{"entries":[{"key":"a","path":"$","value":{"v":1}},{"key":"b","path":"$","value":{"v":2}}],"type":"json_batch_set"}'
$ strata command run --command-json '{"entries":[{"key":"a","path":"$"},{"key":"b","path":"$"}],"type":"json_batch_get"}'
```

### Wire

```json
{"entries":[{"key":"a","path":"$","value":{"v":1}},{"key":"b","path":"$","value":{"v":2}}],"type":"json_batch_set"}
{"entries":[{"key":"a","path":"$"},{"key":"b","path":"$"}],"type":"json_batch_get"}
```

### Output

One response per step, in order:

```json
{"data":{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":2,"timestamp":3,"version":3},"items":[{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":2,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":0,"result":{"document_version":1},"status":"ok"},{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":2,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":1,"result":{"document_version":1},"status":"ok"}],"mode":"itemwise","status":"ok"},"type":"json_batch_results"}
{"data":{"applied":false,"commit":null,"items":[{"applied":false,"commit":null,"effect":null,"error":null,"index":0,"result":{"document_version":1,"found":true,"timestamp":3,"value":{"v":1},"version":3},"status":"ok"},{"applied":false,"commit":null,"effect":null,"error":null,"index":1,"result":{"document_version":1,"found":true,"timestamp":3,"value":{"v":2},"version":3},"status":"ok"}],"mode":"itemwise","status":"ok"},"type":"json_batch_get_results"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `entries` | `BatchJsonGetEntry[]` | yes | — | Entries to read. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`BatchResult<Maybe<JsonValue>>`.

| Field | Type | Description |
|---|---|---|
| `applied` | `boolean` |  |
| `items` | `BatchItem4[]` |  |
| `mode` | `BatchMode` |  |
| `status` | `BatchStatus` |  |
| `commit` | `CommitReceipt` |  |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.json_document_id`](https://stratadb.org/e/invalid_argument.engine.json_document_id) | The JSON document request is invalid. |
| [`invalid_argument.engine.json_path`](https://stratadb.org/e/invalid_argument.engine.json_path) | The JSON path is invalid or cannot be applied to the selected value. |
| [`invalid_argument.engine.json_path_too_long`](https://stratadb.org/e/invalid_argument.engine.json_path_too_long) | The JSON path is invalid or cannot be applied to the selected value. |

## Invocation

- CLI: via `strata command run` (no dedicated verb)
- Wire type: `json_batch_get`

## Related

- [Batch set JSON values](/docs/json/batch_set) — Set multiple JSON values in one itemwise batch.
- [All `json` commands](/docs/json/)
