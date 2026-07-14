---
title: "Count events"
description: "Count visible events in the log."
source: strata-core@1.0.0
section: event
---

Counts events visible in the selected branch and space. The optional timestamp counts the events visible at that commit time.

Status commands return a scalar or compact status payload and do not mutate database state.

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `as_of` | `integer` | no | Optional timestamp in microseconds. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`StatusValue<u64>`.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space)

## Invocation

- CLI: `strata event count`
- Wire type: `event_count`
