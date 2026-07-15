---
title: "Set JSON value"
description: "Set a JSON value at a document path, creating the document when missing."
source: strata-core@1.0.0
section: json
---

Writes a JSON value at a path inside a document, creating the document and any missing intermediate objects when needed. Setting the root path `$` replaces the whole document; setting a nested path like `$.profile.name` updates one field and records a new document version.

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Examples

Store a JSON document, then read it back.

### CLI

```console
$ strata json set user $ {"age":30,"name":"alice"}
$ strata json get user $
```

### Wire

```json
{"key":"user","path":"$","type":"json_set","value":{"age":30,"name":"alice"}}
{"key":"user","path":"$","type":"json_get"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"key":"user"},"type":"json_write_result"}
{"data":{"found":true,"value":{"document_version":1,"timestamp":3,"value":{"age":30,"name":"alice"},"version":3}},"type":"json_versioned_value"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `key` | `string` | yes | — | Document key. |
| `path` | `string` | yes | — | JSON path. |
| `value` | `any` | yes | — | JSON value. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<JsonWrite>`.

| Field | Type | Description |
|---|---|---|
| `commit` | `CommitReceipt` | Commit receipt. |
| `effect` | `MutationEffect` | Mutation effect facts. |
| `key` | `string` | Written document id. |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.json_document_id`](https://stratadb.org/e/invalid_argument.engine.json_document_id) | The JSON document request is invalid. |
| [`invalid_argument.engine.json_path`](https://stratadb.org/e/invalid_argument.engine.json_path) | The JSON path is invalid or cannot be applied to the selected value. |
| [`invalid_argument.engine.json_path_too_long`](https://stratadb.org/e/invalid_argument.engine.json_path_too_long) | The JSON path is invalid or cannot be applied to the selected value. |
| [`invalid_argument.engine.json_path_not_found`](https://stratadb.org/e/invalid_argument.engine.json_path_not_found) | The JSON path is invalid or cannot be applied to the selected value. |
| [`invalid_argument.engine.json_path_type`](https://stratadb.org/e/invalid_argument.engine.json_path_type) | The JSON path is invalid or cannot be applied to the selected value. |
| [`invalid_argument.engine.json_value`](https://stratadb.org/e/invalid_argument.engine.json_value) | The JSON document request is invalid. |
| [`invalid_argument.engine.json_document_too_large`](https://stratadb.org/e/invalid_argument.engine.json_document_too_large) | The JSON document request is invalid. |
| [`invalid_argument.engine.json_document_too_deep`](https://stratadb.org/e/invalid_argument.engine.json_document_too_deep) | The JSON document request is invalid. |
| [`invalid_argument.engine.json_array_too_large`](https://stratadb.org/e/invalid_argument.engine.json_array_too_large) | The request contains invalid input. |

## Invocation

```text
strata json set <key> <path> <value> [--branch <branch>] [--space <space>]
```

- Wire type: `json_set`

## Related

- [Get JSON value](/docs/json/get) — Read the current or historical JSON value at a document path.
- [All `json` commands](/docs/json/)
