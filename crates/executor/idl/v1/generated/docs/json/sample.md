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

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `count` | `integer` | no | Optional sample count. Defaults to 10. |
| `prefix` | `string` | no | Optional document key prefix. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`SamplePage<JsonSampleItem>`.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space)
- [`invalid_argument.engine.json_document_id`](https://stratadb.org/e/invalid_argument.engine.json_document_id)

## Invocation

- CLI: `strata json sample`
- Wire type: `json_sample`
