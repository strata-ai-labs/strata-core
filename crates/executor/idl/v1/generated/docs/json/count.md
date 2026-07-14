---
title: "Count JSON documents"
description: "Count visible JSON documents."
source: strata-core@1.0.0
section: json
---

Counts visible JSON documents in the selected branch and space, optionally constrained by a document key prefix.

Status commands return a scalar or compact status payload and do not mutate database state.

## Examples

Count stored documents.

### CLI

```console
$ strata json set a $ {"v":1}
$ strata json set b $ {"v":2}
$ strata json count
```

### Wire

```json
{"key":"a","path":"$","type":"json_set","value":{"v":1}}
{"key":"b","path":"$","type":"json_set","value":{"v":2}}
{"type":"json_count"}
```

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `as_of` | `integer` | no | Optional timestamp in microseconds. |
| `prefix` | `string` | no | Optional document key prefix. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`StatusValue<u64>`.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space)
- [`invalid_argument.engine.json_document_id`](https://stratadb.org/e/invalid_argument.engine.json_document_id)

## Invocation

- CLI: `strata json count`
- Wire type: `json_count`
