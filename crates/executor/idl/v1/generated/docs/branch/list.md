---
title: "List branches"
description: "List active branches with their lineage facts."
source: strata-core@1.0.0
section: branch
---

Lists every active branch as a terminal page. Each item carries the branch name, deterministic branch id, generation, status, parent lineage (fork version and timestamp when forked), and logical creation version.

Paginated responses use opaque cursors. Clients should pass the returned cursor back to the same command shape and must not parse cursor contents.

## Examples

List branches.

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

## Returns

`Page<BranchItem, String>`.

| Field | Type | Description |
|---|---|---|
| `has_more` | `boolean` |  |
| `items` | `BranchItem[]` | Branches in this page. |
| `cursor` | `string` |  |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |

## Invocation

```text
strata branch list
```

- Wire type: `branch_list`

## Related

- [Create empty branch](/docs/branch/create) — Create a new empty root branch.
- [All `branch` commands](/docs/branch/)
