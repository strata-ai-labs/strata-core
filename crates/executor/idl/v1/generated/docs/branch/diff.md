---
title: "Compare branches"
description: "Compare two branches and report the entities that differ."
source: strata-core@1.0.0
section: branch
---

Compares two branches and reports the authored key-value and JSON entities that
differ, grouped by capability and space: entries `added` on `branch_b`,
`removed` relative to `branch_a`, and `modified` on both sides. The comparison
is directional from `branch_a` to `branch_b`. Derived rows are omitted; a
missing branch is rejected with `not_found.engine.branch`.

Status commands return a scalar or compact status payload and do not mutate database state.

## Examples

Compare a fork against the branch it came from.

### CLI

```console
$ strata kv put config base
$ strata branch fork default experiment
$ strata kv put config tuned --branch experiment  # diverge on the fork
$ strata branch diff default experiment
```

### Wire

```json
{"key":"Y29uZmln","type":"kv_put","value":"YmFzZQ=="}
{"branch":"experiment","source":"default","type":"branch_fork_current"}
{"branch":"experiment","key":"Y29uZmln","type":"kv_put","value":"dHVuZWQ="}
{"branch_a":"default","branch_b":"experiment","type":"branch_diff"}
```

## Parameters

| Name | Type | Required | Description |
|---|---|---|---|
| `at_timestamp` | `integer` | no | Optional timestamp in microseconds; compare each branch as of it. |
| `branch_a` | `string` | yes | The first branch (the `A` side). |
| `branch_b` | `string` | yes | The second branch (the `B` side). |

## Returns

`StatusResponse<BranchComparisonItem>`.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)
- [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch)
- [`invalid_argument.engine.branch_name`](https://stratadb.org/e/invalid_argument.engine.branch_name)
- [`invalid_argument.engine.branch_name_reserved`](https://stratadb.org/e/invalid_argument.engine.branch_name_reserved)

## Invocation

- CLI: `strata branch diff`
- Wire type: `branch_diff`
