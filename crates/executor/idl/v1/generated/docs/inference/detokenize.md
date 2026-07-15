---
title: "Detokenize token ids"
description: "Detokenize token ids with a local model."
source: strata-core@1.0.0
section: inference
---

Decodes an ordered list of token ids back into text using a local model's vocabulary, returning the reconstructed string. Detokenization is a local-only operation: it requires a build with the local execution feature and returns `inference.unsupported_operation` for cloud provider specs.

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `ids` | `integer[]` | yes | — | Token ids. |
| `model` | `string` | yes | — | Model spec. |

## Returns

`DetokenizedText`.

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`inference.unsupported_operation`](https://stratadb.org/e/inference.unsupported_operation) | The inference operation is unsupported. |
| [`inference.missing_model`](https://stratadb.org/e/inference.missing_model) | The requested model is missing. |
| [`inference.model_load_failed`](https://stratadb.org/e/inference.model_load_failed) | The model could not be loaded. |
| [`inference.local_runtime_failed`](https://stratadb.org/e/inference.local_runtime_failed) | The local inference runtime failed. |
| [`inference.registry_corrupt`](https://stratadb.org/e/inference.registry_corrupt) | The local inference registry is corrupt. |

## Invocation

```text
strata inference detokenize <model> <ids>
```

- Wire type: `inference_detokenize`

## Related

- [All `inference` commands](/docs/inference/)
