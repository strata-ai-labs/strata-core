---
title: "Fork branch from current head"
description: "Fork a new branch from the current head of a source branch."
source: strata-core@1.0.0
section: branch
---

Forks a new branch from the source branch's current head. The new branch sees all data visible on the source at fork time; later writes on either branch stay isolated. The returned branch summary records the parent name, fork version, and generation.

On the CLI, all three fork commands share the single verb `strata branch fork <SOURCE> <BRANCH>`: with no flags it runs this command, while `--version` routes to `branch.fork_at_version` and `--timestamp` routes to `branch.fork_at_timestamp` (both wire-surface commands).

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Examples

Fork a branch from another branch's head.

### CLI

```console
$ strata branch fork default experiment
$ strata branch list
```

### Wire

```json
{"branch":"experiment","source":"default","type":"branch_fork_current"}
{"type":"branch_list"}
```

### Output

One response per step, in order:

```json
{"data":{"branch_id":"1a29fdd4-745b-5b66-ad18-75b3cf51cef6","created_at":1,"deleted_at":null,"generation":1,"name":"experiment","parent":{"branch_id":"00000000-0000-0000-0000-000000000000","fork_timestamp":null,"fork_version":1,"generation":1,"name":"default"},"state_revision":0,"status":"active"},"type":"branch"}
{"data":{"cursor":null,"has_more":false,"items":[{"branch_id":"00000000-0000-0000-0000-000000000000","created_at":null,"deleted_at":null,"generation":1,"name":"default","parent":null,"state_revision":0,"status":"active"},{"branch_id":"1a29fdd4-745b-5b66-ad18-75b3cf51cef6","created_at":1,"deleted_at":null,"generation":1,"name":"experiment","parent":{"branch_id":"00000000-0000-0000-0000-000000000000","fork_timestamp":null,"fork_version":1,"generation":1,"name":"default"},"state_revision":0,"status":"active"}]},"type":"branches"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `source` | `string` | yes | — | Source branch name. |

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
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.branch_name`](https://stratadb.org/e/invalid_argument.engine.branch_name) | The branch name is invalid. |
| [`invalid_argument.engine.branch_name_reserved`](https://stratadb.org/e/invalid_argument.engine.branch_name_reserved) | The branch name is invalid. |
| [`already_exists.engine.branch`](https://stratadb.org/e/already_exists.engine.branch) | A branch with this name already exists. |

## Invocation

```text
strata branch fork <source> [--branch <branch>] [--space <space>]
```

- Wire type: `branch_fork_current`

## Related

- [List branches](/docs/branch/list) — List active branches with their lineage facts.
- [All `branch` commands](/docs/branch/)
