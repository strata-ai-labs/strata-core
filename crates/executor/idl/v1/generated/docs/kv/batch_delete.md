---
title: "Batch delete KV values"
description: "Delete multiple KV keys in one itemwise batch."
source: strata-core@1.0.0
section: kv
---

Deletes multiple KV keys and returns one positional mutation result per key. Missing keys are represented as no-op item results.

Itemwise batches return one positional item result per input item. The outer batch status summarizes whether all, some, or none of the items succeeded.

## Examples

Delete many keys in one commit.

### CLI

```console
$ strata command run --command-json '{"entries":[{"key":"YQ==","value":"MQ=="},{"key":"Yg==","value":"Mg=="}],"type":"kv_batch_put"}'
$ strata command run --command-json '{"keys":["YQ==","Yg=="],"type":"kv_batch_delete"}'
$ strata command run --command-json '{"keys":["YQ==","Yg=="],"type":"kv_batch_exists"}'
```

### Wire

```json
{"entries":[{"key":"YQ==","value":"MQ=="},{"key":"Yg==","value":"Mg=="}],"type":"kv_batch_put"}
{"keys":["YQ==","Yg=="],"type":"kv_batch_delete"}
{"keys":["YQ==","Yg=="],"type":"kv_batch_exists"}
```

### Output

One response per step, in order:

```json
{"data":{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":2,"timestamp":3,"version":3},"items":[{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":2,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":0,"result":{"key":"YQ=="},"status":"ok"},{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":2,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":1,"result":{"key":"Yg=="},"status":"ok"}],"mode":"itemwise","status":"ok"},"type":"batch_results"}
{"data":{"applied":true,"commit":{"delete_count":2,"durable":false,"put_count":0,"timestamp":4,"version":4},"items":[{"applied":true,"commit":{"delete_count":2,"durable":false,"put_count":0,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"deleted","matched":true},"error":null,"index":0,"result":{"key":"YQ=="},"status":"ok"},{"applied":true,"commit":{"delete_count":2,"durable":false,"put_count":0,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"deleted","matched":true},"error":null,"index":1,"result":{"key":"Yg=="},"status":"ok"}],"mode":"itemwise","status":"ok"},"type":"batch_results"}
{"data":{"applied":false,"commit":null,"items":[{"applied":false,"commit":null,"effect":null,"error":null,"index":0,"result":{"exists":false,"key":"YQ=="},"status":"ok"},{"applied":false,"commit":null,"effect":null,"error":null,"index":1,"result":{"exists":false,"key":"Yg=="},"status":"ok"}],"mode":"itemwise","status":"ok"},"type":"batch_exists_results"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `keys` | `Bytes[]` | yes | — | Keys to delete. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`BatchResult<KvMutationItem>`.

| Field | Type | Description |
|---|---|---|
| `applied` | `boolean` |  |
| `items` | `BatchItem[]` |  |
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
| [`invalid_argument.engine.kv_batch_duplicate_key`](https://stratadb.org/e/invalid_argument.engine.kv_batch_duplicate_key) | The KV batch contains duplicate keys. |
| [`invalid_argument.executor.kv_batch_duplicate_key`](https://stratadb.org/e/invalid_argument.executor.kv_batch_duplicate_key) | The KV batch contains duplicate keys. |

## Invocation

- CLI: via `strata command run` (no dedicated verb)
- Wire type: `kv_batch_delete`

## Related

- [Batch check KV existence](/docs/kv/batch_exists) — Check existence for multiple KV keys.
- [Batch put KV values](/docs/kv/batch_put) — Store multiple KV values in one itemwise batch.
- [All `kv` commands](/docs/kv/)
