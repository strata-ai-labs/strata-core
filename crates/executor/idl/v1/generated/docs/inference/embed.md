---
title: "Embed one or more texts"
description: "Embed one or more texts into vectors."
source: strata-core@1.0.0
section: inference
---

Embeds text with an embedding-capable model and returns one vector per input, in order. The `input` field takes either a single string or an array of strings, so single and batch embedding share one command. The vector dimension is fixed by the model. Local embedding models require a build with the local execution feature; cloud embedding providers (OpenAI, Google) require the matching provider feature and an API key.

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `input` | `EmbedInput` | yes | — | Text(s) to embed. |
| `model` | `string` | yes | — | Model spec. |
| `dimensions` | `integer` | no | — | Truncate to this many dimensions (matryoshka), then renormalize. |
| `input_type` | `InputType` | no | — | Query vs document role for instruction-tuned embedders. |
| `instruction` | `string` | no | — | Explicit instruction prefix override. |
| `normalize` | `boolean` | no | — | Force L2 normalization on/off (default per-model). |

## Returns

`EmbeddingsResponse`.

| Field | Type | Description |
|---|---|---|
| `data` | `EmbeddingItem[]` | One item per input, in order. |
| `dimension` | `integer` | Embedding dimension. |
| `model` | `string` | Resolved model spec. |
| `usage` | `Usage` | Token usage (when known). |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`inference.unsupported_operation`](https://stratadb.org/e/inference.unsupported_operation) | The inference operation is unsupported. |
| [`inference.missing_model`](https://stratadb.org/e/inference.missing_model) | The requested model is missing. |
| [`inference.model_load_failed`](https://stratadb.org/e/inference.model_load_failed) | The model could not be loaded. |
| [`inference.local_runtime_failed`](https://stratadb.org/e/inference.local_runtime_failed) | The local inference runtime failed. |
| [`inference.missing_api_key`](https://stratadb.org/e/inference.missing_api_key) | The provider API key is missing. |
| [`inference.provider_unavailable`](https://stratadb.org/e/inference.provider_unavailable) | The provider is unavailable. |
| [`inference.provider_malformed_response`](https://stratadb.org/e/inference.provider_malformed_response) | The provider returned a malformed response. |
| [`inference.unsupported_provider`](https://stratadb.org/e/inference.unsupported_provider) | The inference provider is unsupported. |
| [`inference.unsupported_parameter`](https://stratadb.org/e/inference.unsupported_parameter) | The inference parameter is unsupported. |
| [`inference.registry_corrupt`](https://stratadb.org/e/inference.registry_corrupt) | The local inference registry is corrupt. |

## Invocation

```text
strata inference embed <model> <request>
```

- Wire type: `inference_embed`

## Related

- [All `inference` commands](/docs/inference/)
