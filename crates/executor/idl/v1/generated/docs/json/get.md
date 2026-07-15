---
title: "Get JSON value"
description: "Read the current or historical JSON value at a document path."
source: strata-core@1.0.0
section: json
---

Reads the JSON value at a path inside a document. Current reads return the value with commit metadata; passing a timestamp returns the bare value visible at that point in time. A missing document or path is a found-false result, distinct from a stored JSON null.

Optional reads distinguish present data from missing data. When version or timestamp facts exist on the executor output, SDK mappings should preserve them.

## Examples

Read a whole document, a value at a JSON path, or nothing.

### CLI

```console
$ strata json set user $ {"age":30,"name":"alice"}
$ strata json get user $
$ strata json get user $.name
$ strata json get absent $
```

### Wire

```json
{"key":"user","path":"$","type":"json_set","value":{"age":30,"name":"alice"}}
{"key":"user","path":"$","type":"json_get"}
{"key":"user","path":"$.name","type":"json_get"}
{"key":"absent","path":"$","type":"json_get"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"key":"user"},"type":"json_write_result"}
{"data":{"found":true,"value":{"document_version":1,"timestamp":3,"value":{"age":30,"name":"alice"},"version":3}},"type":"json_versioned_value"}
{"data":{"found":true,"value":{"document_version":1,"timestamp":3,"value":"alice","version":3}},"type":"json_versioned_value"}
{"data":{"found":false,"value":null},"type":"json_versioned_value"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `key` | `string` | yes | — | Document key. |
| `path` | `string` | yes | — | JSON path. |
| `as_of` | `integer` | no | — | Optional timestamp in microseconds. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`Maybe<JsonVersionedValue>` — a miss returns nothing rather than raising.

| Field | Type | Description |
|---|---|---|
| `found` | `boolean` |  |
| `value` | `JsonVersionedValue` |  |

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

```text
strata json get <key> <path> [--as-of <integer>] [--branch <branch>] [--space <space>]
```

- Wire type: `json_get`

## Related

- [Set JSON value](/docs/json/set) — Set a JSON value at a document path, creating the document when missing.
- [All `json` commands](/docs/json/)
