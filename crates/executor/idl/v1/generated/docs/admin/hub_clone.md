---
title: "Clone hub dataset"
description: "Clone a dataset from a hub into a new local database."
source: strata-core@1.0.0
section: admin
---

Clones a dataset from a hub into a new local database directory. Resolution, download, verification, reconstitution, and origin recording run once behind this command; the session database is not touched. The destination directory must not exist or must be empty. When the hub URL is not given, the layered resolver selects it from the flag, environment, and config layers.

Status commands return a scalar or compact status payload and do not mutate database state.

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `dataset` | `string` | yes | — | Dataset to clone. |
| `dest` | `string` | yes | — | Destination directory (must not exist, or be empty). |
| `hub_url` | `string` | no | — | Explicit hub URL; when absent the 5-layer resolver runs (flag, `STRATA_HUB_URL`, project config, global config). |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`StatusResponse<HubClone>`.

| Field | Type | Description |
|---|---|---|
| `branch` | `string` | Branch fetched. |
| `dataset` | `string` | Dataset cloned. |
| `dest` | `string` | Destination directory holding the new database. |
| `manifest_hash` | `string` | The bundle's manifest hash. |
| `object_count` | `integer` | Objects fetched. |
| `total_bytes` | `integer` | Total bytes fetched. |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`invalid_argument.executor.hub_dataset`](https://stratadb.org/e/invalid_argument.executor.hub_dataset) | The dataset name is not valid. |
| [`invalid_argument.executor.hub_branch`](https://stratadb.org/e/invalid_argument.executor.hub_branch) | The branch name is not valid. |
| [`invalid_argument.executor.hub_feature_disabled`](https://stratadb.org/e/invalid_argument.executor.hub_feature_disabled) | Hub support is not enabled in this build. |
| [`invalid_argument.executor.hub_url`](https://stratadb.org/e/invalid_argument.executor.hub_url) | The hub URL supplied for this invocation is invalid. |
| [`failed_precondition.executor.hub_clone`](https://stratadb.org/e/failed_precondition.executor.hub_clone) | The clone cannot proceed against this bundle or destination. |
| [`unavailable.executor.hub_transport`](https://stratadb.org/e/unavailable.executor.hub_transport) | The hub could not be reached or returned an error. |

## Invocation

```text
strata clone <dataset> <dest> [--hub-url <string>] [--branch <branch>] [--space <space>]
```

- Wire type: `hub_clone`

## Related

- [All `admin` commands](/docs/admin/)
