---
title: "Generate text"
description: "Generate text with an inference model."
source: strata-core@1.0.0
section: inference
---

Runs a text-generation request against a local or cloud model and returns the completion, the reason generation stopped, and the provider-reported prompt and completion token counts. The request controls the maximum completion tokens, sampling temperature, top-k and top-p cutoffs, an optional deterministic seed, string and token-id stop sequences, and an optional GBNF grammar for constrained generation. Chat models expect their chat template already applied in the prompt. Local models require a build with the local execution feature; cloud providers require the matching provider feature and an API key.

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `model` | `string` | yes | — | Model spec. |
| `frequency_penalty` | `number` | no | — | Frequency penalty. |
| `grammar` | `string` | no | — | GBNF grammar for constrained generation (local). |
| `logit_bias` | `object` | no | — | Per-token logit bias (token id → bias). |
| `logprobs` | `boolean` | no | — | Whether to return log-probabilities. |
| `max_tokens` | `integer` | no | — | Maximum completion tokens. |
| `messages` | `ChatMessage[]` | no | — | Chat messages (system/user/assistant/tool). |
| `min_p` | `number` | no | — | Min-p sampling cutoff. |
| `mirostat` | `Mirostat` | no | — | Mirostat sampling. |
| `model_config` | `ModelConfig` | no | — | Per-model load/context configuration. |
| `presence_penalty` | `number` | no | — | Presence penalty. |
| `prompt` | `string` | no | — | Raw completion prompt (base models / full control). |
| `repeat_last_n` | `integer` | no | — | Repetition penalty look-back window. |
| `repeat_penalty` | `number` | no | — | Repetition penalty. |
| `response_format` | `ResponseFormat` | no | — | Output format constraint. |
| `seed` | `integer` | no | — | Deterministic sampling seed. |
| `stop` | `string[]` | no | — | Stop sequences. |
| `stop_token_ids` | `integer[]` | no | — | Token-id stop sequences (local). |
| `temperature` | `number` | no | — | Sampling temperature. |
| `tfs_z` | `number` | no | — | Tail-free sampling z. |
| `tool_choice` | `ToolChoice` | no | — | How the model should choose among `tools`. |
| `tools` | `Tool[]` | no | — | Tools (functions) the model may call. |
| `top_k` | `integer` | no | — | Top-k sampling cutoff. |
| `top_logprobs` | `integer` | no | — | Number of top log-probabilities to return per token. |
| `top_p` | `number` | no | — | Nucleus sampling cutoff. |
| `typical_p` | `number` | no | — | Typical-p (locally typical) sampling. |

## Returns

`ChatResponse`.

| Field | Type | Description |
|---|---|---|
| `choices` | `ChatChoice[]` | Generation choices (one today). |
| `model` | `string` | Resolved model spec. |
| `usage` | `Usage` | Token usage. |

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
| [`inference.provider_auth_failed`](https://stratadb.org/e/inference.provider_auth_failed) | The provider rejected authentication. |
| [`inference.provider_unavailable`](https://stratadb.org/e/inference.provider_unavailable) | The provider is unavailable. |
| [`inference.provider_timeout`](https://stratadb.org/e/inference.provider_timeout) | The provider request timed out. |
| [`inference.provider_rate_limited`](https://stratadb.org/e/inference.provider_rate_limited) | The provider rate limit was reached. |
| [`inference.invalid_request`](https://stratadb.org/e/inference.invalid_request) | The inference request is invalid. |
| [`inference.provider_malformed_response`](https://stratadb.org/e/inference.provider_malformed_response) | The provider returned a malformed response. |
| [`inference.unsupported_provider`](https://stratadb.org/e/inference.unsupported_provider) | The inference provider is unsupported. |
| [`inference.unsupported_parameter`](https://stratadb.org/e/inference.unsupported_parameter) | The inference parameter is unsupported. |
| [`inference.registry_corrupt`](https://stratadb.org/e/inference.registry_corrupt) | The local inference registry is corrupt. |

## Invocation

```text
strata inference generate <model> <request>
```

- Wire type: `inference_generate`

## Related

- [All `inference` commands](/docs/inference/)
