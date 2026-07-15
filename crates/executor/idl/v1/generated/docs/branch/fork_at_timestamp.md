---
title: "Fork branch at timestamp"
description: "Fork a new branch from a retained source timestamp."
source: strata-core@1.0.0
section: branch
---

Forks a new branch anchored at a retained source timestamp (microseconds, on Strata's logical commit clock). The engine resolves the timestamp to the covering retained commit; the returned parent lineage records both the fork timestamp and the resolved fork version. A timestamp outside retained history fails with `history_unavailable.engine.persistence_history`.

This command has no dedicated CLI verb: the CLI expresses it as `strata branch fork <SOURCE> <BRANCH> --timestamp <TIMESTAMP>` (one shared `branch fork` verb routes to all three fork commands, so only `branch.fork` owns the CLI path). It remains fully reachable through the generic wire surface — `strata command run`, MCP, and SDKs.

Successful mutations return an acknowledgement that identifies the affected target, the mutation effect, and commit facts when the operation changed stored state.

## Examples

Fork a branch at an earlier commit timestamp — time-travel into a branch.

### CLI

```console
$ strata kv put greeting original  # The receipt carries this commit's timestamp (microseconds).
$ strata kv put greeting updated
$ strata command run --command-json '{"branch":"snapshot","source":"default","timestamp":3,"type":"branch_fork_at_timestamp"}'  # snapshot forks default's history as of that instant.
$ strata kv get greeting --branch snapshot
```

### Wire

```json
{"key":"Z3JlZXRpbmc=","type":"kv_put","value":"b3JpZ2luYWw="}
{"key":"Z3JlZXRpbmc=","type":"kv_put","value":"dXBkYXRlZA=="}
{"branch":"snapshot","source":"default","timestamp":3,"type":"branch_fork_at_timestamp"}
{"branch":"snapshot","key":"Z3JlZXRpbmc=","type":"kv_get"}
```

### Output

One response per step, in order:

```json
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":3,"version":3},"effect":{"affected_count":1,"applied":true,"kind":"created","matched":false},"key":"Z3JlZXRpbmc="},"type":"write_result"}
{"data":{"commit":{"delete_count":0,"durable":false,"put_count":1,"timestamp":4,"version":4},"effect":{"affected_count":1,"applied":true,"kind":"updated","matched":true},"key":"Z3JlZXRpbmc="},"type":"write_result"}
{"data":{"branch_id":"39de3743-01cb-53e3-9cb2-371a4599ccdf","created_at":3,"deleted_at":null,"generation":1,"name":"snapshot","parent":{"branch_id":"00000000-0000-0000-0000-000000000000","fork_timestamp":3,"fork_version":3,"generation":1,"name":"default"},"state_revision":0,"status":"active"},"type":"branch"}
{"data":{"found":true,"value":{"timestamp":3,"value":"b3JpZ2luYWw=","version":3}},"type":"kv_versioned_value"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `source` | `string` | yes | — | Source branch name. |
| `timestamp` | `integer` | yes | — | Source timestamp in microseconds. |

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
| [`history_unavailable.engine.persistence_history`](https://stratadb.org/e/history_unavailable.engine.persistence_history) | The requested resource was not found. |

## Invocation

- CLI: via `strata command run` (no dedicated verb)
- Wire type: `branch_fork_at_timestamp`

## Related

- [Get KV value](/docs/kv/get) — Read the current or historical value for one KV key.
- [Put KV value](/docs/kv/put) — Store or replace a KV value by key.
- [All `branch` commands](/docs/branch/)
