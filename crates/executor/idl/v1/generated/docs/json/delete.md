---
title: "Delete JSON value"
description: "Delete a whole JSON document or one path inside it."
source: strata-core@1.0.0
section: json
---

Deletes the root path `$` to remove the whole document, or a nested path to remove one field or array element. Missing documents and paths produce a no-op delete acknowledgement rather than an error.

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Examples

Delete a document.

### CLI

```console
$ strata json set temp $ {"x":1}
$ strata json delete temp $
$ strata json exists temp
```

### Wire

```json
{"key":"temp","path":"$","type":"json_set","value":{"x":1}}
{"key":"temp","path":"$","type":"json_delete"}
{"key":"temp","type":"json_exists"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"key":"temp"},"type":"json_write_result"}
{"data":{"commit":{"delete_count":1,"durable":false,"put_count":0,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"deleted","matched":true},"key":"temp"},"type":"json_delete_result"}
{"data":false,"type":"bool"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `key` | `string` | yes | — | Document key. |
| `path` | `string` | yes | — | JSON path. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<JsonDelete>`.

| Field | Type | Description |
|---|---|---|
| `effect` | `MutationEffect` | Mutation effect facts. |
| `key` | `string` | Target document id. |
| `commit` | `CommitReceipt` | Commit receipt when a delete was applied. |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.json_document_id`](https://stratadb.org/e/invalid_argument.engine.json_document_id) | The JSON document request is invalid. |
| [`invalid_argument.engine.json_path`](https://stratadb.org/e/invalid_argument.engine.json_path) | The JSON path is invalid or cannot be applied to the selected value. |
| [`invalid_argument.engine.json_path_too_long`](https://stratadb.org/e/invalid_argument.engine.json_path_too_long) | The JSON path is invalid or cannot be applied to the selected value. |
| [`invalid_argument.engine.json_path_type`](https://stratadb.org/e/invalid_argument.engine.json_path_type) | The JSON path is invalid or cannot be applied to the selected value. |

## Invocation

```text
strata json delete <key> <path> [--branch <branch>] [--space <space>]
```

- Wire type: `json_delete`

## Related

- [Check JSON document existence](/docs/json/exists) — Check whether one JSON document exists.
- [Set JSON value](/docs/json/set) — Set a JSON value at a document path, creating the document when missing.
- [All `json` commands](/docs/json/)
