---
title: "Create JSON index"
description: "Create a JSON secondary index on a field path."
source: strata-core@1.0.0
section: json
---

Creates a secondary index over one JSON field path with a numeric, tag, or text kind. Existing documents are indexed at creation and future writes maintain the index automatically. The current wire response is a transitional bare index definition.

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Examples

Create a secondary index on a JSON field.

### CLI

```console
$ strata json index create by_name $.name tag
$ strata json index list
```

### Wire

```json
{"field_path":"$.name","index_type":"tag","name":"by_name","type":"json_create_index"}
{"type":"json_list_indexes"}
```

### Output

One response per step, in order:

```json
{"data":{"created_timestamp":3,"created_version":3,"field_path":"name","index_type":"tag","name":"by_name","space":"default"},"type":"json_index_definition"}
{"data":{"cursor":null,"has_more":false,"items":[{"created_timestamp":3,"created_version":3,"field_path":"name","index_type":"tag","name":"by_name","space":"default"}]},"type":"json_index_list"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `field_path` | `string` | yes | — | Indexed field path. |
| `index_type` | `JsonIndexType` | yes | — | Index kind. |
| `name` | `string` | yes | — | Index name. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<JsonIndexCreate>`.

| Field | Type | Description |
|---|---|---|
| `created_timestamp` | `integer` |  |
| `created_version` | `integer` |  |
| `field_path` | `string` |  |
| `index_type` | `JsonIndexType` |  |
| `name` | `string` |  |
| `space` | `string` |  |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.json_index_name`](https://stratadb.org/e/invalid_argument.engine.json_index_name) | The JSON index request is invalid. |
| [`invalid_argument.engine.json_index_name_reserved`](https://stratadb.org/e/invalid_argument.engine.json_index_name_reserved) | The JSON index request is invalid. |
| [`invalid_argument.engine.json_path`](https://stratadb.org/e/invalid_argument.engine.json_path) | The JSON path is invalid or cannot be applied to the selected value. |
| [`invalid_argument.engine.json_path_too_long`](https://stratadb.org/e/invalid_argument.engine.json_path_too_long) | The JSON path is invalid or cannot be applied to the selected value. |
| [`already_exists.engine.json_index`](https://stratadb.org/e/already_exists.engine.json_index) | A JSON index with this name already exists. |

## Invocation

```text
strata json index create <name> <field_path> <index_type> [--branch <branch>] [--space <space>]
```

- Wire type: `json_create_index`

## Related

- [List JSON indexes](/docs/json/index/list) — List JSON secondary indexes.
- [All `json` commands](/docs/json/)
