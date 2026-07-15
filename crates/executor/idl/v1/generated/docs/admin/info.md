---
title: "Read database info"
description: "Read database identity and a catalog summary."
source: strata-core@1.0.0
section: admin
---

Returns database identity and a catalog summary for one branch: engine version, open target, whether this open created the database, durability, the default branch, the active branch count, and the registered space count for the selected branch. The branch defaults to the handle branch when omitted.

Status commands return a scalar or compact status payload and do not mutate database state.

## Examples

Read database identity and catalog counts.

### CLI

```console
$ strata info
```

### Wire

```json
{"type":"info"}
```

### Output

One response per step, in order:

```json
{"data":{"branch_count":1,"created":true,"default_branch":"default","durable":false,"open":true,"space_count":1,"target":"cache","version":"1.0.0"},"type":"database_info"}
```

## Parameters

_No parameters._

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`StatusResponse<AdminDatabaseInfo>`.

| Field | Type | Description |
|---|---|---|
| `branch_count` | `integer` | Active branch count. |
| `created` | `boolean` | True when this open created a new database. |
| `default_branch` | `string` | Default product branch. |
| `durable` | `boolean` | True when storage is durable. |
| `open` | `boolean` | True while the database handle is open. |
| `space_count` | `integer` | Registered space count for the selected branch. |
| `target` | `AdminOpenTarget` | Open target. |
| `version` | `string` | Engine package version. |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.branch_name`](https://stratadb.org/e/invalid_argument.engine.branch_name) | The branch name is invalid. |

## Invocation

```text
strata info [--branch <branch>] [--space <space>]
```

- Wire type: `info`

## Related

- [All `admin` commands](/docs/admin/)
