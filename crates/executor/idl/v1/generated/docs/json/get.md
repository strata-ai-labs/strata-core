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

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `as_of` | `integer` | no | Optional timestamp in microseconds. |
| `key` | `string` | yes | Document key. |
| `path` | `string` | yes | JSON path. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`Maybe<JsonVersionedValue>` — a miss returns nothing rather than raising.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space)
- [`invalid_argument.engine.json_document_id`](https://stratadb.org/e/invalid_argument.engine.json_document_id)
- [`invalid_argument.engine.json_path`](https://stratadb.org/e/invalid_argument.engine.json_path)
- [`invalid_argument.engine.json_path_too_long`](https://stratadb.org/e/invalid_argument.engine.json_path_too_long)

## Invocation

- CLI: `strata json get`
- Wire type: `json_get`
