---
title: "Create empty branch"
description: "Create a new empty root branch."
source: strata-core@1.0.0
section: branch
---

Creates an empty root branch with no parent and no data. This is not a fork: the new branch starts from nothing, and its `parent` is null. Use `branch.fork` to start from an existing branch's data. Creating a name that already exists fails with `already_exists.engine.branch`; names reserved for engine control data are rejected.

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Examples

Create a new empty branch.

### CLI

```console
$ strata branch create feature
$ strata branch list
```

### Wire

```json
{"branch":"feature","type":"branch_create"}
{"type":"branch_list"}
```

### Output

One response per step, in order:

```json
{"data":{"branch_id":"dc42122c-83b7-5436-89bc-9ffa4299697c","created_at":3,"deleted_at":null,"generation":1,"name":"feature","parent":null,"state_revision":0,"status":"active"},"type":"branch"}
{"data":{"cursor":null,"has_more":false,"items":[{"branch_id":"00000000-0000-0000-0000-000000000000","created_at":null,"deleted_at":null,"generation":1,"name":"default","parent":null,"state_revision":0,"status":"active"},{"branch_id":"dc42122c-83b7-5436-89bc-9ffa4299697c","created_at":3,"deleted_at":null,"generation":1,"name":"feature","parent":null,"state_revision":0,"status":"active"}]},"type":"branches"}
```

## Parameters

_No parameters._

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<BranchItem>`.

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
| [`invalid_argument.engine.branch_name`](https://stratadb.org/e/invalid_argument.engine.branch_name) | The branch name is invalid. |
| [`invalid_argument.engine.branch_name_reserved`](https://stratadb.org/e/invalid_argument.engine.branch_name_reserved) | The branch name is invalid. |
| [`already_exists.engine.branch`](https://stratadb.org/e/already_exists.engine.branch) | A branch with this name already exists. |

## Invocation

```text
strata branch create [--branch <branch>] [--space <space>]
```

- Wire type: `branch_create`

## Related

- [List branches](/docs/branch/list) — List active branches with their lineage facts.
- [All `branch` commands](/docs/branch/)
