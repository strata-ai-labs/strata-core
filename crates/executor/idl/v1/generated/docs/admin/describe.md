---
title: "Describe database"
description: "Read a compact description of the database."
source: strata-core@1.0.0
section: admin
---

Returns a compact description of the database for one branch: engine version, open target, the default and described branches, all active branches, the registered product spaces, per-primitive counts (KV, JSON, event, plus vector-collection and graph summaries), the sanitized config, and the available capabilities. The branch defaults to the handle branch when omitted.

Status commands return a scalar or compact status payload and do not mutate database state.

## Examples

Describe the database.

### CLI

```console
$ strata describe
```

### Wire

```json
{"type":"describe"}
```

### Output

One response per step, in order:

```json
{"data":{"branch":"default","branches":["default"],"capabilities":{"arrow":true,"event":true,"graph_core":true,"inference":true,"json":true,"kv":true,"vector":true,"vector_index":true},"config":{"created":true,"default_branch":"default","durable":false,"target":"cache"},"default_branch":"default","primitives":{"event_count":0,"graphs":[],"json_count":0,"kv_count":0,"vector_collections":[]},"spaces":["default"],"target":"cache","version":"1.0.0"},"type":"described"}
```

## Parameters

_No parameters._

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`StatusResponse<AdminDescribe>`.

| Field | Type | Description |
|---|---|---|
| `branch` | `string` | Described branch. |
| `branches` | `string[]` | Active branches. |
| `capabilities` | `AdminCapabilities` | Available rebuilt capabilities. |
| `config` | `AdminConfig` | Sanitized config. |
| `default_branch` | `string` | Default product branch. |
| `primitives` | `AdminPrimitives` | Primitive summaries. |
| `spaces` | `string[]` | Registered product spaces on the described branch. |
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
strata describe [--branch <branch>] [--space <space>]
```

- Wire type: `describe`

## Related

- [All `admin` commands](/docs/admin/)
