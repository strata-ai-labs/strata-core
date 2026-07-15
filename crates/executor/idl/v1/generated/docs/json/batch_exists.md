---
title: "Batch check JSON document existence"
description: "Check existence for multiple JSON documents."
source: strata-core@1.0.0
section: json
---

Checks several JSON documents and returns positional boolean status values. The response preserves the input order.

Itemwise batches return one positional item result per input item. The outer batch status summarizes whether all, some, or none of the items succeeded.

## Examples

Check existence for many document keys at once.

### CLI

```console
$ strata command run --command-json '{"entries":[{"key":"a","path":"$","value":{"v":1}}],"type":"json_batch_set"}'
$ strata command run --command-json '{"keys":["a","missing"],"type":"json_batch_exists"}'
```

### Wire

```json
{"entries":[{"key":"a","path":"$","value":{"v":1}}],"type":"json_batch_set"}
{"keys":["a","missing"],"type":"json_batch_exists"}
```

### Output

One response per step, in order:

```json
{"data":{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"items":[{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":0,"result":{"document_version":1},"status":"ok"}],"mode":"itemwise","status":"ok"},"type":"json_batch_results"}
{"data":{"applied":false,"commit":null,"items":[{"applied":false,"commit":null,"effect":null,"error":null,"index":0,"result":{"exists":true},"status":"ok"},{"applied":false,"commit":null,"effect":null,"error":null,"index":1,"result":{"exists":false},"status":"ok"}],"mode":"itemwise","status":"ok"},"type":"json_batch_exists_results"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `keys` | `string[]` | yes | — | Document keys to check. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`BatchResult<BatchExistsPresence>`.

| Field | Type | Description |
|---|---|---|
| `applied` | `boolean` |  |
| `items` | `BatchItem6[]` |  |
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

## Invocation

- CLI: via `strata command run` (no dedicated verb)
- Wire type: `json_batch_exists`

## Related

- [Batch set JSON values](/docs/json/batch_set) — Set multiple JSON values in one itemwise batch.
- [All `json` commands](/docs/json/)
