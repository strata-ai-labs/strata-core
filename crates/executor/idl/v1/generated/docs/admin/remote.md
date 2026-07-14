---
title: "Read remote origin"
description: "Read where this database was cloned from."
source: strata-core@1.0.0
section: admin
---

Reads the remote origin recorded when this database was cloned from a hub: the remote URL, dataset, branch, manifest hash, fetch time, and per-branch base frontier. Returns a null origin when the database was created locally and never cloned.

Optional reads distinguish present data from missing data. When version or timestamp facts exist on the executor output, SDK mappings should preserve them.

## Examples

Read where this database was cloned from (a local database has no origin).

### CLI

```console
$ strata remote
```

### Wire

```json
{"type":"remote_get"}
```

## Parameters

_No parameters._

## Returns

`Maybe<RemoteOriginInfo>` — a miss returns nothing rather than raising.

## Errors

- [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed)

## Invocation

- CLI: `strata remote`
- Wire type: `remote_get`
