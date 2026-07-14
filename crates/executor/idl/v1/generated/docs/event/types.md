---
title: "List event types"
description: "List distinct event types in the log."
source: strata-core@1.0.0
section: event
---

Lists the distinct event types visible in the selected branch and space in sorted order. The optional timestamp lists the types visible at that commit time.

Paginated responses use opaque cursors. Clients should pass the returned cursor back to the same command shape and must not parse cursor contents.

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `as_of` | `integer` | no | Optional timestamp in microseconds. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`Page<String, String>`.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space)

## Invocation

- CLI: `strata event types`
- Wire type: `event_list_types`
