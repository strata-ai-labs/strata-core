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

## Parameters

_No parameters._

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`StatusResponse<AdminDatabaseInfo>`.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.branch_name`](https://stratadb.org/e/invalid_argument.engine.branch_name)

## Invocation

- CLI: `strata info`
- Wire type: `info`
