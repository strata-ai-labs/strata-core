---
title: "Check JSON document existence"
description: "Check whether one JSON document exists."
source: strata-core@1.0.0
section: json
---

Returns a boolean status for one JSON document key without loading the stored document.

Status commands return a scalar or compact status payload and do not mutate database state.

## Examples

Check whether a document exists.

### CLI

```console
$ strata json set user $ {"name":"alice"}
$ strata json exists user
$ strata json exists absent
```

### Wire

```json
{"key":"user","path":"$","type":"json_set","value":{"name":"alice"}}
{"key":"user","type":"json_exists"}
{"key":"absent","type":"json_exists"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"key":"user"},"type":"json_write_result"}
{"data":true,"type":"bool"}
{"data":false,"type":"bool"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `key` | `string` | yes | — | Document key. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`StatusValue<bool>`.

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.json_document_id`](https://stratadb.org/e/invalid_argument.engine.json_document_id) | The JSON document request is invalid. |

## Invocation

```text
strata json exists <key> [--branch <branch>] [--space <space>]
```

- Wire type: `json_exists`

## Related

- [Set JSON value](/docs/json/set) — Set a JSON value at a document path, creating the document when missing.
- [All `json` commands](/docs/json/)
