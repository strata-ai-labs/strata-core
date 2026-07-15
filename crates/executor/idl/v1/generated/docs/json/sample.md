---
title: "Sample JSON documents"
description: "Sample visible JSON documents."
source: strata-core@1.0.0
section: json
---

Returns a bounded sample of visible JSON documents plus the total matching count. Useful for inspecting document shape before writing queries or indexes.

Paginated responses use opaque cursors. Clients should pass the returned cursor back to the same command shape and must not parse cursor contents.

## Examples

A representative sample plus the total population size.

### CLI

```console
$ strata command run --command-json '{"entries":[{"key":"a","path":"$","value":{"v":1}},{"key":"b","path":"$","value":{"v":2}},{"key":"c","path":"$","value":{"v":3}}],"type":"json_batch_set"}'
$ strata json sample
```

### Wire

```json
{"entries":[{"key":"a","path":"$","value":{"v":1}},{"key":"b","path":"$","value":{"v":2}},{"key":"c","path":"$","value":{"v":3}}],"type":"json_batch_set"}
{"type":"json_sample"}
```

### Output

One response per step, in order:

```json
{"data":{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":3,"timestamp":3,"version":3},"items":[{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":3,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":0,"result":{"document_version":1},"status":"ok"},{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":3,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":1,"result":{"document_version":1},"status":"ok"},{"applied":true,"commit":{"delete_count":0,"durable":false,"put_count":3,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"error":null,"index":2,"result":{"document_version":1},"status":"ok"}],"mode":"itemwise","status":"ok"},"type":"json_batch_results"}
{"data":{"cursor":null,"has_more":false,"items":[{"document_version":1,"key":"a","timestamp":3,"value":{"v":1},"version":3},{"document_version":1,"key":"b","timestamp":3,"value":{"v":2},"version":3},{"document_version":1,"key":"c","timestamp":3,"value":{"v":3},"version":3}],"total_count":3},"type":"json_sample_result"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `count` | `integer` | no | 10 | Optional sample count. |
| `prefix` | `string` | no | — | Optional document key prefix. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`SamplePage<JsonSampleItem>`.

| Field | Type | Description |
|---|---|---|
| `has_more` | `boolean` |  |
| `items` | `JsonSampleItem[]` | Sampled documents. |
| `total_count` | `integer` | Total matching live documents. |
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
strata json sample [--count <integer>] [--prefix <string>] [--branch <branch>] [--space <space>]
```

- Wire type: `json_sample`

## Related

- [Batch set JSON values](/docs/json/batch_set) — Set multiple JSON values in one itemwise batch.
- [All `json` commands](/docs/json/)
