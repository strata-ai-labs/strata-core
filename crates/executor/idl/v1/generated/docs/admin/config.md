---
title: "Read sanitized config"
description: "Read sanitized configuration facts."
source: strata-core@1.0.0
section: admin
---

Returns sanitized configuration facts: the open target, whether this open created the database, durability, and the default branch. Only allowlisted, non-sensitive facts are exposed; no filesystem paths, credentials, or provider keys are ever returned.

Status commands return a scalar or compact status payload and do not mutate database state.

## Examples

Read sanitized configuration facts.

### CLI

```console
$ strata config get
```

### Wire

```json
{"type":"config_get"}
```

### Output

One response per step, in order:

```json
{"data":{"created":true,"default_branch":"default","durable":false,"target":"cache"},"type":"config"}
```

## Parameters

_No parameters._

## Returns

`StatusResponse<AdminConfig>`.

| Field | Type | Description |
|---|---|---|
| `created` | `boolean` | True when this open created a new database. |
| `default_branch` | `string` | Default product branch. |
| `durable` | `boolean` | True when storage is durable. |
| `target` | `AdminOpenTarget` | Open target. |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |

## Invocation

```text
strata config get
```

- Wire type: `config_get`

## Related

- [All `admin` commands](/docs/admin/)
