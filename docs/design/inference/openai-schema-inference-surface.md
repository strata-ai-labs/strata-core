# Inference config surface: adopt the OpenAI body schema

Status: draft implementation plan · Owner: TBD · Scope: `strata-inference` →
`strata-executor` → IDL → CLI → SDK

## Context

Strata ships a real inference layer (`crates/inference`): vendored llama.cpp
(local, GGUF) plus Anthropic/OpenAI/Google cloud adapters, an HF model catalog,
and 12 executor commands (`inference.*`). But the **config surface is a
lowest-common-denominator completion API**. `GenerateRequest` is 9 fields
(`prompt, max_tokens, temperature, top_k, top_p, seed, stop_sequences,
stop_tokens, grammar`); the local sampler is hardcoded to `top_k → top_p → temp
→ dist`; context/model-load params are hardcoded (`n_gpu_layers = 999`, `n_ctx`
capped at 4096); **there is no chat-template handling anywhere** (local models
get the raw prompt; cloud adapters wrap it as a single `{role:"user"}` message —
no system prompt, no multi-turn, no assistant prefill); and provider-specific
knobs are silently dropped (Anthropic drops `top_p`; Google force-disables
thinking with `thinkingBudget:0`). `EmbedRequest` is `{ text }` — no pooling,
normalize, dimensions, or query/document distinction.

To let end users build AI modules on Strata, the inference layer must be a
**configurable product** — full sampling control, load-time model config, and
real prompt formats — **without exploding the executor or SDK surface area.**

## The decision (approach "a")

**Adopt the OpenAI Chat Completions + Embeddings *body schema* as the canonical
request/response for `inference.generate` and `inference.embed`, inside Strata's
own `{type,data}` envelope and error contract.** This is the surface-area-
minimizing move that `llama-cpp-python` (and vLLM/TGI/ollama/LM Studio) all make:
you inherit a schema every AI developer and coding agent already knows instead
of designing, naming, versioning, and teaching a bespoke one. llama.cpp- and
provider-specific knobs ride along as a small, **documented set of flat optional
extension fields** (the vLLM `extra_body` pattern). We adopt the OpenAI *body
schema*, not a byte-for-byte drop-in server — Strata stays Strata (its envelope,
error codes, and CLI conventions are unchanged).

Four load-bearing design choices follow:

1. **Messages + engine-owned chat templating.** The primary input is
   `messages: [{role, content}]`. The inference crate applies the correct prompt
   format: for local, a GGUF-metadata cascade (llama-cpp-python's exact
   precedence); for cloud, native message mapping. Raw-`prompt` completion mode
   stays as an escape hatch for base models.
2. **Two config tiers.** Per-call *sampling* lives in the request; per-model
   *load/context* params (`n_ctx`, `n_gpu_layers`, `n_batch`, threads,
   flash-attn) live in a separate `ModelConfig`, cache-keyed — never repeated on
   every call. Mirrors llama-cpp-python's `Llama(...)` constructor vs.
   `create_chat_completion(...)` split.
3. **Capability-checked parity — no silent drops.** A knob unsupported by the
   chosen provider is an explicit error (material) or a documented ignore
   (cosmetic), surfaced through the existing `inference.capability` command.
4. **Surface stays flat.** Command count goes **12 → 11** (embed + embed_batch
   collapse into one OpenAI-style `input: string | string[]`). The SDK is one
   `db.ai.chat(...)` + one `db.ai.embed(...)` + the model-management verbs — no
   per-provider namespaces.

## New inference DTOs (`crates/inference/src`)

Clean break — the inference commands are default-off and unexposed by the SDK,
so there is no compatibility burden. Replace the old DTOs.

### ChatRequest (OpenAI Chat Completions body + extensions)

```
ChatRequest {
  // --- OpenAI core (all optional except messages) ---
  messages: Vec<ChatMessage>,           // role: system|user|assistant|tool; content: String (parts = phase 2)
  max_tokens: Option<u32>,
  temperature: Option<f32>,
  top_p: Option<f32>,
  stop: Option<Vec<String>>,
  seed: Option<u64>,
  frequency_penalty: Option<f32>,
  presence_penalty: Option<f32>,
  logit_bias: Option<Map<u32, f32>>,
  logprobs: Option<bool>,
  top_logprobs: Option<u32>,
  response_format: Option<ResponseFormat>,   // { type: text | json_object | json_schema, json_schema? }
  tools: Option<Vec<Tool>>,                   // phase 2
  tool_choice: Option<ToolChoice>,            // phase 2
  // stream: reserved; reject `true` (streaming is post-V1)

  // --- Strata / llama.cpp extensions (flat, documented) ---
  top_k: Option<u32>,
  min_p: Option<f32>,
  typical_p: Option<f32>,
  tfs_z: Option<f32>,
  repeat_penalty: Option<f32>,
  repeat_last_n: Option<u32>,
  mirostat: Option<Mirostat>,           // { mode: 1|2, tau, eta }
  grammar: Option<String>,              // GBNF (local)
  stop_token_ids: Option<Vec<u32>>,

  // --- two-tier load config (optional; cache-keyed) ---
  model_config: Option<ModelConfig>,
}
```

`ChatResponse` (OpenAI-shaped, minimal):

```
ChatResponse {
  model: String,
  choices: Vec<ChatChoice>,   // { index, message: {role, content, tool_calls?}, finish_reason: stop|length|tool_calls|content_filter, logprobs? }
  usage: Usage,               // { prompt_tokens, completion_tokens, total_tokens }
}
```

A raw-completion path is retained: either a `prompt: Option<String>` alternative
on the same request (mutually exclusive with `messages`) or the existing
completion command. Prefer one command with `messages` XOR `prompt`.

### EmbedRequest (OpenAI Embeddings body — collapses embed + embed_batch)

```
EmbedRequest {
  input: OneOrMany<String>,     // single or batch — one command for both
  dimensions: Option<u32>,      // matryoshka truncation (+ renormalize)
  // extensions:
  normalize: Option<bool>,      // default per-model; cloud currently forced L2
  input_type: Option<InputType>,// query | document (instruction-tuned embedders)
  instruction: Option<String>,  // explicit instruction prefix override
}
EmbedResponse { model, data: Vec<{ index, embedding: Vec<f32> }>, usage, dimension }
```

Note: **pooling is a context-creation param** in llama.cpp (MEAN/CLS/LAST/RANK),
not per-call — it belongs in the embedding `ModelConfig`, not the request.

### RankRequest (light enrichment)

Add `top_n: Option<u32>`, `return_documents: Option<bool>`,
`instruction: Option<String>`. `RankResponse` items gain an optional `document`.

### ModelConfig (load-time, tier 2)

```
ModelConfig {
  n_ctx: Option<u32>,
  n_gpu_layers: Option<i32>,     // -1 = all (replaces hardcoded 999)
  n_batch: Option<u32>,
  n_threads: Option<u32>,
  flash_attn: Option<bool>,
  pooling: Option<Pooling>,      // embeddings only
  // advanced (later): rope_freq_base/scale, kv_cache_type
}
```

Cloud providers ignore `ModelConfig` (no load step) — capability-reported.

## Chat-template resolution (local, `crates/inference/src/llama`)

Implement llama-cpp-python's precedence exactly:

1. explicit template override (request/ModelConfig `chat_template`) →
2. named `chat_format` (small registry: chatml, llama-2/3, mistral, gemma, qwen,
   phi) →
3. **the GGUF model's `tokenizer.chat_template` metadata** →
4. fallback (chatml).

Mechanism: bind the vendored llama.cpp `llama_chat_apply_template` (+
`llama_model_chat_template` to read the embedded template) via FFI in
`llama/ffi.rs`, wrapped in `llama/context.rs`. This avoids writing a Jinja
engine. Verify both symbols exist in the vendored copy (~b5440); if
`llama_model_chat_template` is absent, read `tokenizer.chat_template` from GGUF
KV metadata directly. Assistant prefill = a trailing assistant message with
partial content, appended after templating.

For cloud, map `messages` natively: OpenAI = identity; Anthropic = hoist
`system` to the top-level `system` param, alternate user/assistant, support
assistant prefill; Google = `system_instruction` + `contents` with roles
`user`/`model`.

## Provider mapping + capability policy

Each provider mapper (`provider/{local,openai,anthropic,google}.rs`,
`cloud_embed.rs`) translates the unified request to native, honoring supported
fields. Policy: **material unsupported knob → typed error**
(`invalid_argument.inference.unsupported_option` or similar);
**cosmetic/no-op → documented ignore**. Stop force-disabling capabilities
(Google thinking) — pass through / expose as an extension where the provider
supports it. Extend `InferenceCapability` to advertise the richer matrix
(supported sampling knobs, response_format modes, tools) so callers can
pre-check. Local sampler chain is rebuilt from the extension fields in the
correct llama.cpp order (grammar → penalties → top_k → typical_p → top_p → min_p
→ tfs → temp → mirostat/dist).

## Layer-by-layer changes

- **`crates/inference`**: the DTO redesign above; local chat-templating + full
  sampler + `ModelConfig` threaded into context/model load (cache key becomes
  `(model_spec, model_config)`); cloud message/system/passthrough mapping;
  capability matrix; embed collapse + dimensions/pooling/normalize/input_type;
  logprobs + uniform `finish_reason`. Unsafe stays inside `local/` (rule 38).
- **`crates/executor`**: `Command::InferenceGenerate { model, request: ChatRequest }`;
  merge `InferenceEmbed`/`InferenceEmbedBatch` → `InferenceEmbed { model, request: EmbedRequest }`;
  update `Output` variants (`InferenceGeneration(ChatResponse)`,
  `InferenceEmbeddings(EmbedResponse)`); update the inline handlers in
  `executor/dispatch.rs`; add the new `inference.*` error codes to
  `error_registry.rs`. Net command count 12 → 11.
- **IDL** (`idl/v1/commands/inference.yaml`, prose, `kinds.yaml`): retag
  `inference.generate`/`inference.embed` to the new request/response models;
  delete `inference.embed_batch`; regenerate `command-index.json` + `schemas/*`
  + `cli-command-index.json`; re-bless fixtures (`verify-fixtures --update`).
  Consider marking the family feature-gated (today all `feature: core`).
- **CLI** (`crates/cli`): `strata inference generate` gains `--system`,
  repeatable `--message role:content`, `--messages-json`, and the common knob
  flags (`--top-k/--min-p/--repeat-penalty/--mirostat/--response-format/...`)
  plus `--json-body` for full control; `--n-ctx/--n-gpu-layers/...` for
  ModelConfig. `strata inference embed` accepts multiple inputs +
  `--dimensions`. Update `render.rs`.
- **SDK** (`strata-python`, gated on the wheel packaging decision — cloud-lean vs
  local): add a `db.ai` namespace — `chat(messages, model=…, **params) ->`
  message/response, `embed(input, model=…, **params)`, `rank(...)`,
  `models.list/pull`, `capability`, `unload` — plus a `db.ai.model(spec,
  **load_config)` handle that carries `ModelConfig` so load params are set once
  (the two-tier ergonomics over a stateless wire). Reads like the `openai` SDK.

## Phasing

- **A. Inference DTOs** — ChatRequest/Response, EmbedRequest (collapse),
  ModelConfig, Usage/FinishReason; wire-schemas derive.
- **B. Local provider** — chat-template cascade (FFI bind), full sampler chain,
  ModelConfig into load + cache key, logprobs, finish_reason.
- **C. Cloud providers** — messages/system/multi-turn/prefill, response_format,
  penalties/logit_bias/seed where supported, dimensions; capability matrix;
  remove silent drops.
- **D. Executor + IDL** — command/output changes, embed collapse, handlers,
  errors, regen + fixtures.
- **E. CLI** — verbs/flags/render.
- **F. SDK `db.ai`** — namespace + Model handle (after the cloud-vs-local wheel
  decision).
- **G. (phase 2) tools/function-calling + `response_format: json_schema`**
  end-to-end; multimodal content parts.

## Testing

Golden request/response fixtures per command; **chat-template correctness**
(templated prompt for chatml/llama/mistral/gemma/qwen vs. known-good strings, and
GGUF-metadata resolution); sampler determinism (seeded); capability-matrix tests
(each provider × each knob → supported/error/ignore); cloud mappers via mocked
HTTP (system hoisting, prefill, response_format); embed dimensions/normalize/
input_type; IDL drift + `verify-fixtures`.

## Out of scope (reserve / reject)

- **Streaming** — reserve `stream`, reject `true` (post-V1, `CLAUDE.md`).
- **Multimodal content parts** — string content first; typed parts phase 2.
- **On-prem OpenAI-compatible endpoint adapter** (vLLM/NIM/Ollama/…) — post-V1.
- **Byte-for-byte OpenAI drop-in server** — explicitly not the goal (approach
  "b" rejected); we adopt the body schema, not the transport.

## Open questions

1. **Extension fields flat vs. nested** — flat top-level optionals (this plan,
   vLLM-style) vs. a nested `extensions: { local: {...}, openai: {...} }` block.
   Flat is smaller and reads like the OpenAI SDK's extra kwargs; recommended.
2. **Raw-completion path** — one `generate` command with `messages` XOR `prompt`
   (recommended, fewer commands) vs. a separate completion command.
3. **SDK packaging** — cloud-lean abi3 wheel with local as an opt-in
   `stratadb[local]`/separate wheel, vs. one heavy wheel (llama.cpp + CUDA
   matrix). Independent of this surface design; blocks Phase F.
