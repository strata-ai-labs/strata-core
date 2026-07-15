---
title: "Read database metrics"
description: "Read lightweight database metrics."
source: strata-core@1.0.0
section: admin
---

Returns lightweight database metrics: the open target, durability, whether the handle is open, the active branch count, the registered space count for the selected branch, and the control-plane health status. The branch defaults to the handle branch when omitted.

Status commands return a scalar or compact status payload and do not mutate database state.

## Examples

Read lightweight database metrics.

### CLI

```console
$ strata metrics
```

### Wire

```json
{"type":"metrics"}
```

### Output

One response per step, in order:

```json
{"data":{"branch_count":1,"control_status":"healthy","durable":false,"open":true,"space_count":1,"target":"cache"},"type":"metrics"}
```

## Parameters

_No parameters._

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`StatusResponse<AdminMetrics>`.

| Field | Type | Description |
|---|---|---|
| `branch_count` | `integer` | Active branch count. |
| `control_status` | `AdminHealthStatus` | Control-plane health status. |
| `durable` | `boolean` | True when storage is durable. |
| `open` | `boolean` | True while the database handle is open. |
| `space_count` | `integer` | Registered space count for the selected branch. |
| `target` | `AdminOpenTarget` | Open target. |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.branch_name`](https://stratadb.org/e/invalid_argument.engine.branch_name) | The branch name is invalid. |

## Invocation

```text
strata metrics [--branch <branch>] [--space <space>]
```

- Wire type: `metrics`

## Related

- [All `admin` commands](/docs/admin/)
