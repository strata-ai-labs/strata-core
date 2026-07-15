---
title: "Read database health"
description: "Read control-plane health facts."
source: strata-core@1.0.0
section: admin
---

Returns control-plane health facts: the worst overall status plus per-subsystem status for identity, registry, branch catalog, and the optional branch-local space catalog. Also reports the default branch and active branch count. A healthy result means every required control-plane fact is present and readable.

Status commands return a scalar or compact status payload and do not mutate database state.

## Examples

Read control-plane health.

### CLI

```console
$ strata health
```

### Wire

```json
{"type":"health"}
```

### Output

One response per step, in order:

```json
{"data":{"branch_catalog":"healthy","branch_count":1,"default_branch":"default","identity":"healthy","registry":"healthy","space_catalog":"healthy","status":"healthy"},"type":"health"}
```

## Parameters

_No parameters._

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`StatusResponse<AdminHealth>`.

| Field | Type | Description |
|---|---|---|
| `branch_catalog` | `AdminControlStatus` | Branch catalog status. |
| `branch_count` | `integer` | Active branch count. |
| `default_branch` | `string` | Default product branch. |
| `identity` | `AdminControlStatus` | Database identity status. |
| `registry` | `AdminControlStatus` | Registry status. |
| `status` | `AdminHealthStatus` | Worst health status. |
| `space_catalog` | `AdminControlStatus` | Optional branch-local space catalog status. |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.branch_name`](https://stratadb.org/e/invalid_argument.engine.branch_name) | The branch name is invalid. |

## Invocation

```text
strata health [--branch <branch>] [--space <space>]
```

- Wire type: `health`

## Related

- [All `admin` commands](/docs/admin/)
