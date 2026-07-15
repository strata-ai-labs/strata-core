---
title: "Batch get KV values"
description: "Read multiple KV values by key."
source: strata-core@1.0.0
section: kv
---

Reads several KV keys and returns positional item results. Each item records whether the corresponding key was found and includes value metadata when present.

Itemwise batches return one positional item result per input item. The outer batch status summarizes whether all, some, or none of the items succeeded.

## Examples

Read many keys at once; a missing key comes back as null.

### CLI

```console
$ strata command run --command-json '{"entries":[{"key":"YQ==","value":"MQ=="},{"key":"Yg==","value":"Mg=="}],"type":"kv_batch_put"}'
$ strata command run --command-json '{"keys":["YQ==","Yg==","bWlzc2luZw=="],"type":"kv_batch_get"}'
```

### Wire

```json
{"entries":[{"key":"YQ==","value":"MQ=="},{"key":"Yg==","value":"Mg=="}],"type":"kv_batch_put"}
{"keys":["YQ==","Yg==","bWlzc2luZw=="],"type":"kv_batch_get"}
```

### Output

One response per step, in order:

```json
{"data":{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":2,"timestamp":3,"version":3},"items":[{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":2,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":0,"result":{"key":"YQ=="},"status":"ok"},{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":2,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":1,"result":{"key":"Yg=="},"status":"ok"}],"mode":"itemwise","status":"ok"},"type":"batch_results"}
{"data":{"applied":false,"commit":null,"items":[{"applied":false,"commit":null,"effect":null,"error":null,"index":0,"result":{"found":true,"key":"YQ==","timestamp":3,"value":"MQ==","version":3},"status":"ok"},{"applied":false,"commit":null,"effect":null,"error":null,"index":1,"result":{"found":true,"key":"Yg==","timestamp":3,"value":"Mg==","version":3},"status":"ok"},{"applied":false,"commit":null,"effect":null,"error":null,"index":2,"result":{"found":false,"key":"bWlzc2luZw==","timestamp":null,"value":null,"version":null},"status":"miss"}],"mode":"itemwise","status":"partial"},"type":"batch_get_results"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `keys` | `Bytes[]` | yes | — | Keys to read. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`BatchResult<Maybe<Bytes>>`.

| Field | Type | Description |
|---|---|---|
| `applied` | `boolean` |  |
| `items` | `BatchItem2[]` |  |
| `mode` | `BatchMode` |  |
| `status` | `BatchStatus` |  |
| `commit` | `CommitReceipt` |  |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.kv_key`](https://stratadb.org/e/invalid_argument.engine.kv_key) | The KV request is invalid. |
| [`invalid_argument.engine.kv_batch`](https://stratadb.org/e/invalid_argument.engine.kv_batch) | The KV request is invalid. |

## Invocation

- CLI: via `strata command run` (no dedicated verb)
- Wire type: `kv_batch_get`

## Related

- [Batch put KV values](/docs/kv/batch_put) — Store multiple KV values in one itemwise batch.
- [All `kv` commands](/docs/kv/)
