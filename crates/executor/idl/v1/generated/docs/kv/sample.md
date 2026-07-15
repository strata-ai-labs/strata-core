---
title: "Sample KV rows"
description: "Sample visible KV rows."
source: strata-core@1.0.0
section: kv
---

Returns a deterministic bounded sample of visible KV rows plus the total matching count.

Paginated responses use opaque cursors. Clients should pass the returned cursor back to the same command shape and must not parse cursor contents.

## Examples

A representative sample plus the total population size.

### CLI

```console
$ strata command run --command-json '{"entries":[{"key":"YQ==","value":"MQ=="},{"key":"Yg==","value":"Mg=="},{"key":"Yw==","value":"Mw=="}],"type":"kv_batch_put"}'
$ strata kv sample
```

### Wire

```json
{"entries":[{"key":"YQ==","value":"MQ=="},{"key":"Yg==","value":"Mg=="},{"key":"Yw==","value":"Mw=="}],"type":"kv_batch_put"}
{"type":"kv_sample"}
```

### Output

One response per step, in order:

```json
{"data":{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":3,"timestamp":3,"version":3},"items":[{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":3,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":0,"result":{"key":"YQ=="},"status":"ok"},{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":3,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":1,"result":{"key":"Yg=="},"status":"ok"},{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":3,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":2,"result":{"key":"Yw=="},"status":"ok"}],"mode":"itemwise","status":"ok"},"type":"batch_results"}
{"data":{"cursor":null,"has_more":false,"items":[{"key":"YQ==","timestamp":3,"value":"MQ==","version":3},{"key":"Yg==","timestamp":3,"value":"Mg==","version":3},{"key":"Yw==","timestamp":3,"value":"Mw==","version":3}],"total_count":3},"type":"sample_result"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `count` | `integer` | no | 10 | Optional sample count. |
| `prefix` | `Bytes` | no | — | Optional key prefix. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`SamplePage<SampleItem>`.

| Field | Type | Description |
|---|---|---|
| `has_more` | `boolean` |  |
| `items` | `SampleItem[]` | Sampled rows. |
| `total_count` | `integer` | Total matching live rows. |
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
strata kv sample [--count <integer>] [--prefix <Bytes>] [--branch <branch>] [--space <space>]
```

- Wire type: `kv_sample`

## Related

- [Batch put KV values](/docs/kv/batch_put) — Store multiple KV values in one itemwise batch.
- [All `kv` commands](/docs/kv/)
