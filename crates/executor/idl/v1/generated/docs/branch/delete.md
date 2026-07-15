---
title: "Delete branch"
description: "Delete an active branch and release its storage claims."
source: strata-core@1.0.0
section: branch
---

Deletes an active branch and reports the deleted branch summary, generation facts, and storage cleanup counts. The `default` branch refuses deletion with `invalid_argument.engine.branch_delete`. There is no merge in V1: work on a fork is either kept by continuing on that branch or discarded by deleting it.

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Examples

Delete a branch.

### CLI

```console
$ strata branch create temp
$ strata branch delete temp
$ strata branch list
```

### Wire

```json
{"branch":"temp","type":"branch_create"}
{"branch":"temp","type":"branch_delete"}
{"type":"branch_list"}
```

### Output

One response per step, in order:

```json
{"data":{"branch_id":"39d446db-cbca-54ec-b793-509e5325483b","created_at":3,"deleted_at":null,"generation":1,"name":"temp","parent":null,"state_revision":0,"status":"active"},"type":"branch"}
{"data":{"branch":{"branch_id":"39d446db-cbca-54ec-b793-509e5325483b","created_at":3,"deleted_at":6,"generation":1,"name":"temp","parent":null,"state_revision":2,"status":"deleted"},"cleanup":{"protected_tables":0,"releasable_tables":0,"removed_refs":0},"deleted":true,"effect":{"affected_count":1,"applied":true,"kind":"deleted","matched":true},"generation_after":1,"generation_before":1},"type":"branch_delete_result"}
{"data":{"cursor":null,"has_more":false,"items":[{"branch_id":"00000000-0000-0000-0000-000000000000","created_at":null,"deleted_at":null,"generation":1,"name":"default","parent":null,"state_revision":0,"status":"active"}]},"type":"branches"}
```

## Parameters

_No parameters._

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`MutationAck<BranchDelete>`.

| Field | Type | Description |
|---|---|---|
| `branch` | `BranchItem` | Deleted branch summary. |
| `deleted` | `boolean` | True when the branch was deleted. |
| `effect` | `MutationEffect` | Mutation effect facts. |
| `cleanup` | `BranchCleanupItem` | Cleanup facts. |
| `generation_after` | `integer` | Generation after delete. |
| `generation_before` | `integer` | Generation before delete. |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.branch_name`](https://stratadb.org/e/invalid_argument.engine.branch_name) | The branch name is invalid. |
| [`invalid_argument.engine.branch_name_reserved`](https://stratadb.org/e/invalid_argument.engine.branch_name_reserved) | The branch name is invalid. |
| [`invalid_argument.engine.branch_delete`](https://stratadb.org/e/invalid_argument.engine.branch_delete) | The request contains invalid input. |

## Invocation

```text
strata branch delete [--branch <branch>] [--space <space>]
```

- Wire type: `branch_delete`

## Related

- [Create empty branch](/docs/branch/create) — Create a new empty root branch.
- [List branches](/docs/branch/list) — List active branches with their lineage facts.
- [All `branch` commands](/docs/branch/)
