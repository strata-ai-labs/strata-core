---
title: "Read one branch"
description: "Read one branch summary by name."
source: strata-core@1.0.0
section: branch
---

Reads the summary for one branch: name, deterministic branch id, generation, status, parent lineage, and logical creation version. A branch that does not exist is a `not_found.engine.branch` error, not an empty result.

Status commands return a scalar or compact status payload and do not mutate database state.

## Examples

Read one branch by name.

### CLI

```console
$ strata branch create feature
$ strata branch get feature
```

### Wire

```json
{"branch":"feature","type":"branch_create"}
{"branch":"feature","type":"branch_get"}
```

### Output

One response per step, in order:

```json
{"data":{"branch_id":"dc42122c-83b7-5436-89bc-9ffa4299697c","created_at":3,"deleted_at":null,"generation":1,"name":"feature","parent":null,"state_revision":0,"status":"active"},"type":"branch"}
{"data":{"branch_id":"dc42122c-83b7-5436-89bc-9ffa4299697c","created_at":3,"deleted_at":null,"generation":1,"name":"feature","parent":null,"state_revision":0,"status":"active"},"type":"branch"}
```

## Parameters

_No parameters._

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`StatusResponse<BranchItem>`.

| Field | Type | Description |
|---|---|---|
| `branch_id` | `string` |  |
| `generation` | `integer` |  |
| `name` | `string` |  |
| `state_revision` | `integer` |  |
| `status` | `BranchStatus` |  |
| `created_at` | `integer` |  |
| `deleted_at` | `integer` |  |
| `parent` | `BranchParentItem` |  |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.branch_name`](https://stratadb.org/e/invalid_argument.engine.branch_name) | The branch name is invalid. |
| [`invalid_argument.engine.branch_name_reserved`](https://stratadb.org/e/invalid_argument.engine.branch_name_reserved) | The branch name is invalid. |

## Invocation

```text
strata branch get [--branch <branch>] [--space <space>]
```

- Wire type: `branch_get`

## Related

- [Create empty branch](/docs/branch/create) — Create a new empty root branch.
- [All `branch` commands](/docs/branch/)
