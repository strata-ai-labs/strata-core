---
title: "Unload cached models"
description: "Unload cached inference models."
source: strata-core@1.0.0
section: inference
---

Removes cached model engines from the runtime to free memory. Pass a model spec to unload one entry, or omit it to unload every cached generation, embedding, and ranking model. The result reports whether any cached entry was actually removed. This affects only the in-memory runtime cache; it never deletes downloaded model files from disk.

## Examples

Evict cached models from the runtime (a no-op when nothing is loaded).

### CLI

```console
$ strata inference unload
```

### Wire

```json
{"type":"inference_unload"}
```

### Output

One response per step, in order:

```json
{"data":{"unloaded":false},"type":"inference_unload_result"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `model` | `string` | no | — | Optional model spec. |

## Returns

`UnloadResult`.

| Field | Type | Description |
|---|---|---|
| `unloaded` | `boolean` | True when a cached entry was removed. |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |

## Invocation

```text
strata inference unload [--model <string>]
```

- Wire type: `inference_unload`

## Related

- [All `inference` commands](/docs/inference/)
