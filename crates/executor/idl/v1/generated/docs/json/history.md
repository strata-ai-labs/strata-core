---
title: "Read JSON document history"
description: "Read retained version history for one JSON document."
source: strata-core@1.0.0
section: json
---

Returns retained full-document history rows for a JSON document, newest first, including delete tombstones. A document with no retained history maps to an optional-read result.

Optional reads distinguish present data from missing data. When version or timestamp facts exist on the executor output, SDK mappings should preserve them.

## Examples

A document that never existed has no history.

### CLI

```console
$ strata json history absent
```

### Wire

```json
{"key":"absent","type":"json_history"}
```

### Output

One response per step, in order:

```json
{"data":null,"type":"json_version_history"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `key` | `string` | yes | — | Document key. |

Plus the optional scope: `branch` and `space` (default to the session branch and the `"default"` space).

## Returns

`Maybe<Vec<JsonHistoryItem>>` — a miss returns nothing rather than raising.

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`invalid_argument.engine.product_space`](https://stratadb.org/e/invalid_argument.engine.product_space) | The requested space operation cannot be completed. |
| [`invalid_argument.engine.json_document_id`](https://stratadb.org/e/invalid_argument.engine.json_document_id) | The JSON document request is invalid. |

## Invocation

```text
strata json history <key> [--branch <branch>] [--space <space>]
```

- Wire type: `json_history`

## Related

- [All `json` commands](/docs/json/)
