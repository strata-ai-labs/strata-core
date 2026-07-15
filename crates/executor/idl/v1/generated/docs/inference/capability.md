---
title: "Report model capabilities"
description: "Report capabilities for a model spec."
source: strata-core@1.0.0
section: inference
---

Parses a model spec into a provider and model name and reports what that combination supports without running the model. The result states whether generation, tokenization, embedding, and ranking are available, whether the operation requires network access or an API key, whether this binary was compiled with the provider feature needed to execute, whether the runtime currently permits network calls, and the known embedding dimension. Model specs are catalog names (`tinyllama`), catalog `name:quant` pairs (`tinyllama:q8_0`), local GGUF paths, or provider specs (`anthropic:claude-...`).

## Examples

Report a model's capabilities without a network call.

### CLI

```console
$ strata inference capability openai:gpt-4o-mini  # Pure metadata — no request is sent to the provider.
```

### Wire

```json
{"model":"openai:gpt-4o-mini","type":"inference_model_capability"}
```

### Output

One response per step, in order:

```json
{"data":{"can_embed":true,"can_generate":true,"can_rank":false,"can_tokenize":false,"embedding_dim":0,"model":"gpt-4o-mini","network_enabled":true,"provider":"openai","provider_feature_enabled":true,"requires_api_key":true,"requires_network":true,"supports_json_object":true,"supports_json_schema":true,"supports_logprobs":true,"supports_tools":true},"type":"inference_capability"}
```

## Parameters

| Name | Type | Required | Default | Description |
|---|---|---|---|---|
| `model` | `string` | yes | — | Model spec. |

## Returns

`InferenceCapability`.

| Field | Type | Description |
|---|---|---|
| `can_embed` | `boolean` | Whether embedding is supported. |
| `can_generate` | `boolean` | Whether generation is supported. |
| `can_rank` | `boolean` | Whether ranking is supported. |
| `can_tokenize` | `boolean` | Whether tokenization is supported. |
| `embedding_dim` | `integer` | Known embedding dimension, if available. |
| `model` | `string` | Model name or path after provider parsing. |
| `network_enabled` | `boolean` | Whether this runtime configuration currently allows network access. |
| `provider` | `ProviderKind` | Provider kind. |
| `provider_feature_enabled` | `boolean` | Whether this binary was compiled with the provider feature needed for execution. |
| `requires_api_key` | `boolean` | Whether the provider requires an API key. |
| `requires_network` | `boolean` | Whether the operation requires network access. |
| `supports_json_object` | `boolean` | Whether `response_format: json_object` is honored. |
| `supports_json_schema` | `boolean` | Whether `response_format: json_schema` (structured output) is honored. |
| `supports_logprobs` | `boolean` | Whether `logprobs` are returned in the response. |
| `supports_tools` | `boolean` | Whether chat requests may offer `tools` (function calling). |

## Errors

| Code | Meaning |
|---|---|
| [`failed_precondition.engine.runtime_closed`](https://stratadb.org/e/failed_precondition.engine.runtime_closed) | The runtime is closed. |
| [`not_found.engine.branch`](https://stratadb.org/e/not_found.engine.branch) | The requested branch was not found. |
| [`inference.invalid_request`](https://stratadb.org/e/inference.invalid_request) | The inference request is invalid. |

## Invocation

```text
strata inference capability <model>
```

- Wire type: `inference_model_capability`

## Related

- [All `inference` commands](/docs/inference/)
