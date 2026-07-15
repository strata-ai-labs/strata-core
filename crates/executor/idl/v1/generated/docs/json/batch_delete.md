---
title: "Batch delete JSON values"
description: "Delete multiple JSON documents or paths in one itemwise batch."
source: strata-core@1.0.0
section: json
---

Deletes multiple document/path entries and returns one positional mutation result per entry. Missing documents and paths are represented as no-op item results; applied items share one engine commit.

Itemwise batches return one positional item result per input item. The outer batch status summarizes whether all, some, or none of the items succeeded.

## Examples

Delete many documents in one commit.

### CLI

```console
$ strata command run --command-json '{"entries":[{"key":"a","path":"$","value":{"v":1}}],"type":"json_batch_set"}'
$ strata command run --command-json '{"entries":[{"key":"a","path":"$"}],"type":"json_batch_delete"}'
$ strata json exists a
```

### Wire

```json
{"entries":[{"key":"a","path":"$","value":{"v":1}}],"type":"json_batch_set"}
{"entries":[{"key":"a","path":"$"}],"type":"json_batch_delete"}
{"key":"a","type":"json_exists"}
```

### Output

One response per step, in order:

```json
{"data":{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"items":[{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":0,"result":{"document_version":1},"status":"ok"}],"mode":"itemwise","status":"ok"},"type":"json_batch_results"}
{"data":{"applied":true,"commit":{"delete_count":1,"durable":false,"put_count":0,"timestamp":4,"version":4},"items":[{"applied":true,"commit":{"delete_count":1,"durable":false,"put_count":0,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"deleted","matched":true},"error":null,"index":0,"result":{"document_version":null},"status":"ok"}],"mode":"itemwise","status":"ok"},"type":"json_batch_results"}
{"data":false,"type":"bool"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `entries` | `BatchJsonDeleteEntry[]` | yes | — | Entries to delete. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`BatchResult<JsonMutationItem>`.

| Field | Type | Description |
|---|---|---|
| `applied` | `boolean` |  |
| `items` | `BatchItem3[]` |  |
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
| [`invalid_argument.engine.json_path_type`](https://stratadb.org/e/invalid_argument.engine.json_path_type) | The JSON path is invalid or cannot be applied to the selected value. |
| [`invalid_argument.executor.json_batch_duplicate_key`](https://stratadb.org/e/invalid_argument.executor.json_batch_duplicate_key) | The JSON batch contains duplicate document targets. |

## Invocation

- CLI: via `strata command run` (no dedicated verb)
- Wire type: `json_batch_delete`

## Related

- [Batch set JSON values](/docs/json/batch_set) — Set multiple JSON values in one itemwise batch.
- [Check JSON document existence](/docs/json/exists) — Check whether one JSON document exists.
- [All `json` commands](/docs/json/)
