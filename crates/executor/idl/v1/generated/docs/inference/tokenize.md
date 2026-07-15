---
title: "Tokenize text"
description: "Tokenize text with a local model."
source: strata-core@1.0.0
section: inference
---

Encodes text into the token id sequence a local model would see and returns the ids in order. Set `add_special` to include the model's special tokens (such as beginning-of-sequence markers). Tokenization is a local-only operation: it requires a build with the local execution feature and returns `inference.unsupported_operation` for cloud provider specs.

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `model` | `string` | yes | — | Model spec. |
| `text` | `string` | yes | — | Text to tokenize. |
| `add_special` | `boolean` | no | false | Whether to add special tokens. |

## Returns

`TokenIds`.

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
strata inference tokenize <model> <text> [--add-special <boolean>]
```

- Wire type: `inference_tokenize`

## Related

- [All `inference` commands](/docs/inference/)
