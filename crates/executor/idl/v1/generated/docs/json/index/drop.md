---
title: "Drop JSON index"
description: "Drop a JSON secondary index by name."
source: strata-core@1.0.0
section: json
---

Drops the named JSON secondary index and its stored entries. Documents are unaffected. The current wire response is a transitional boolean status.

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `name` | `string` | yes | Index name. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<JsonIndexDrop>`.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space)
- [`invalid_argument.engine.json_index_name`](https://stratadb.org/e/invalid_argument.engine.json_index_name)
- [`invalid_argument.engine.json_index_name_reserved`](https://stratadb.org/e/invalid_argument.engine.json_index_name_reserved)

## Invocation

- CLI: `strata json index drop`
- Wire type: `json_drop_index`
