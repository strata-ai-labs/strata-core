---
title: "List JSON indexes"
description: "List JSON secondary indexes."
source: strata-core@1.0.0
section: json
---

Lists JSON secondary index definitions in the selected branch and space, including each index's field path and kind.

Paginated responses use opaque cursors. Clients should pass the returned cursor back to the same command shape and must not parse cursor contents.

## Parameters

_No parameters._

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`Page<JsonIndexDefinition, String>`.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space)

## Invocation

- CLI: `strata json index list`
- Wire type: `json_list_indexes`
