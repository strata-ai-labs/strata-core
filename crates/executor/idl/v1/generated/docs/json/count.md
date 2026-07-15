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

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"key":"a"},"type":"json_write_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"key":"b"},"type":"json_write_result"}
{"data":2,"type":"uint"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `as_of` | `integer` | no | — | Optional timestamp in microseconds. |
| `prefix` | `string` | no | — | Optional document key prefix. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`StatusValue<u64>`.

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.json_document_id`](https://stratadb.org/e/invalid_argument.engine.json_document_id) | The JSON document request is invalid. |

## Invocation

```text
strata json count [--as-of <integer>] [--prefix <string>] [--branch <branch>] [--space <space>]
```

- Wire type: `json_count`

## Related

- [Set JSON value](/docs/json/set) — Set a JSON value at a document path, creating the document when missing.
- [All `json` commands](/docs/json/)
