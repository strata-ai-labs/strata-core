---
title: "Ping database"
description: "Check that the database handle is live."
source: strata-core@1.0.0
section: admin
---

Lightweight liveness check. Returns the engine package version without touching branches, spaces, or primitive data. Use it to confirm the handle is open and responsive before issuing heavier commands.

Status commands return a scalar or compact status payload and do not mutate database state.

## Examples

Check the database handle is live.

### CLI

```console
$ strata ping
```

### Wire

```json
{"type":"ping"}
```

### Output

One response per step, in order:

```json
{"data":{"version":"1.0.0"},"type":"pong"}
```

## Parameters

_No parameters._

## Returns

`StatusResponse<PingInfo>`.

| Field | Type | Description |
|---|---|---|
| `version` | `string` | Engine package version. |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |

## Invocation

```text
strata ping
```

- Wire type: `ping`

## Related

- [All `admin` commands](/docs/admin/)
