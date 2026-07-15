---
title: "Read KV history"
description: "Read retained version history for one KV key."
source: strata-core@1.0.0
section: kv
---

Returns retained history rows for a KV key when history is available. Missing or unavailable history maps to an optional-read result.

Optional reads distinguish present data from missing data. When version or timestamp facts exist on the executor output, SDK mappings should preserve them.

## Examples

A key that never existed has no history.

### CLI

```console
$ strata kv history absent
```

### Wire

```json
{"key":"YWJzZW50","type":"kv_history"}
```

### Output

One response per step, in order:

```json
{"data":null,"type":"version_history"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `key` | `Bytes` | yes | — | Key to read. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`Maybe<Vec<HistoryItem>>` — a miss returns nothing rather than raising.

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.kv_key`](https://stratadb.org/e/invalid_argument.engine.kv_key) | The KV request is invalid. |

## Invocation

```text
strata kv history <key> [--branch <branch>] [--space <space>]
```

- Wire type: `kv_history`

## Related

- [All `kv` commands](/docs/kv/)
