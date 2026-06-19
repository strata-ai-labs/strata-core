# Inference Runtime Port Implementation Plan

Status: draft implementation plan

## Problem

The current `strata-inference` crate is already close to the right lower-layer
boundary for model execution. It owns local llama.cpp execution, cloud provider
adapters, model registry behavior, embedding, generation, and reranking. The
right V1 move is therefore a careful port into a new crate shape, not a
rewrite.

At the same time, the current `strata-intelligence` crate mixes two different
things:

1. pure model runtime lifecycle helpers that are portable;
2. Strata-aware behavior such as autoembedding, shadow-vector writes, search
   recipe execution, RAG over engine hits, and database config reads.

This plan creates an `inference-next` crate by porting the runtime portions of
`strata-inference` and the pure model lifecycle portions of
`strata-intelligence`. Executor integration should happen through a stable
command-shaped facade. Engine and storage must remain uninvolved.

## Design Stance

1. **Port first.** Copy working logic when the existing implementation already
   has the right ownership. Rewrite only when required by boundary, safety,
   error, or command-contract gaps.
2. **No database semantics in inference.** The inference crate must not know
   about branches, spaces, commits, search recipes, shadow vectors, storage
   rows, or engine config.
3. **Executor owns the public serialized API.** The public user/API command
   layer remains in `executor-next`. Inference provides a stable runtime facade
   and DTOs for executor to call.
4. **No provider internals in executor.** Executor may call inference runtime
   methods, but it must not import llama modules, cloud provider modules, model
   registry internals, or native FFI.
5. **No hidden network behavior.** Model downloads and cloud provider calls
   happen only from explicit model commands or explicit generation/embedding
   requests with network permission.
6. **Structured failures before product exposure.** User-actionable failures
   must map to stable inference error codes before executor exposes them as
   product command results.
7. **Default build is lightweight.** The new inference crate should not build
   llama.cpp or cloud providers by default. Product builds opt into local and
   provider features explicitly.
8. **Real integration evidence is required.** Fake providers are useful only
   for narrow command serialization, error mapping, and secret-redaction tests.
   Runtime parity must be proven with real GGUF files and real provider API
   keys in an explicit integration lane.

## Old Evidence

Port from these files unless a section below explicitly says not to:

- `crates/inference/Cargo.toml`
- `crates/inference/build.rs`
- `crates/inference/src/lib.rs`
- `crates/inference/src/error.rs`
- `crates/inference/src/generate.rs`
- `crates/inference/src/embed.rs`
- `crates/inference/src/cloud_embed.rs`
- `crates/inference/src/rank.rs`
- `crates/inference/src/provider/local.rs`
- `crates/inference/src/provider/anthropic.rs`
- `crates/inference/src/provider/openai.rs`
- `crates/inference/src/provider/google.rs`
- `crates/inference/src/llama/context.rs`
- `crates/inference/src/llama/ffi.rs`
- `crates/inference/src/registry/mod.rs`
- `crates/inference/src/registry/catalog.rs`
- `crates/inference/src/registry/download.rs`
- `crates/inference/examples/validate_inference.rs`
- `crates/intelligence/src/generate.rs`
- `crates/intelligence/src/embed/mod.rs`
- `crates/intelligence/src/embed/download.rs`

Do not port these into inference:

- `crates/intelligence/src/embed/runtime.rs`
- `crates/intelligence/src/embed/extract.rs`
- `crates/intelligence/src/expand.rs`
- `crates/intelligence/src/expand_cache.rs`
- `crates/intelligence/src/rerank.rs`, except pure rerank runtime call shape
- `crates/intelligence/src/rag/`
- `crates/intelligence/src/shadow.rs`
- old executor command handlers as runtime logic

Those files contain Strata-aware behavior and belong in a later intelligence
layer over engine plus inference.

## Current Targets

- `crates/inference-next/Cargo.toml`
- `crates/inference-next/build.rs`
- `crates/inference-next/src/lib.rs`
- `crates/inference-next/src/api/`
- `crates/inference-next/src/error/`
- `crates/inference-next/src/model/`
- `crates/inference-next/src/registry/`
- `crates/inference-next/src/provider/`
- `crates/inference-next/src/local/`
- `crates/inference-next/src/runtime/`
- `crates/inference-next/src/testkit/`
- `crates/inference-next/tests/`
- `crates/executor-next/Cargo.toml`
- `crates/executor-next/src/command.rs`
- `crates/executor-next/src/output.rs`
- `crates/executor-next/src/types.rs`
- `crates/executor-next/src/executor.rs`
- `crates/executor-next/tests/`

## Crate Shape

The target crate should use permanent domain names inside the code. The package
name can remain the temporary integration-branch crate name, but modules,
types, test names, and comments should use model/runtime/provider vocabulary.

```text
crates/inference-next/
  Cargo.toml
  build.rs
  src/
    lib.rs
    api/
      mod.rs
      capability.rs
      generation.rs
      embedding.rs
      ranking.rs
      provider.rs
    error/
      mod.rs
      code.rs
    model/
      mod.rs
      spec.rs
      task.rs
    registry/
      mod.rs
      catalog.rs
      download.rs
    provider/
      mod.rs
      anthropic.rs
      google.rs
      openai.rs
    local/
      mod.rs
      ffi.rs
      context.rs
      generation.rs
      embedding.rs
      ranking.rs
    runtime/
      mod.rs
      cache.rs
      loader.rs
      secrets.rs
    testkit/
      mod.rs
      fake.rs
```

Exact filenames may change during implementation, but these ownership
boundaries should not.

## Feature Model

`strata-inference-next` should start with:

| Feature | Default | Purpose |
| --- | --- | --- |
| `default` | yes | Empty/minimal runtime-free build. |
| `local` | no | Enables llama.cpp build and local GGUF runtime. |
| `download` | no | Enables model downloads and hash verification. |
| `anthropic` | no | Enables Anthropic generation provider. |
| `openai` | no | Enables OpenAI generation and embedding provider. |
| `google` | no | Enables Google generation and embedding provider. |
| `testkit` | no | Enables minimal deterministic fixtures for command/error tests. |

The old `strata-inference` default currently enables `local`. Do not carry that
forward. The default crate build must compile without the native vendor tree,
without network provider dependencies, and without model artifacts.

## Public Runtime API

Port and stabilize these lower-layer types:

- `ProviderKind`
- `ModelTask`
- `ModelInfo`
- `ModelSpec`
- `ResolvedModel`
- `GenerateRequest`
- `GenerateResponse`
- `StopReason`
- `EmbedRequest`
- `EmbedResponse`
- `EmbedItemOutcome`
- `RankRequest`
- `RankResponse`
- `RankItemOutcome`
- `InferenceCapability`
- `InferenceError`
- task traits:
  - `Generator`
  - `Embedder`
  - `Reranker`
- runtime facade:
  - `InferenceRuntime`
  - `InferenceRuntimeConfig`
  - `ModelCacheStatus`

The broad old `InferenceEngine` trait may be kept internally or as a
compatibility helper, but executor should use the task-specific facade.

## Executor Command Surface

Executor remains the public serialized command layer. Add an optional
`inference` feature to `executor-next` that depends on `strata-inference-next`
with no default provider features. Provider features can be forwarded
explicitly.

Initial command set:

| Command | Output |
| --- | --- |
| `InferenceModelsList` | list of model info |
| `InferenceModelsLocal` | list of locally available model info |
| `InferenceModelsPull` | pulled model path and metadata |
| `InferenceModelCapability` | provider/model capability facts |
| `InferenceGenerate` | generated text, token counts, stop reason, diagnostics |
| `InferenceTokenize` | token ids and count |
| `InferenceDetokenize` | text |
| `InferenceEmbed` | embedding vector or item outcome |
| `InferenceEmbedBatch` | ordered item outcomes |
| `InferenceRank` | ordered rank outcomes |
| `InferenceUnload` | unload acknowledgement |

Command names may be shortened during implementation only if the executor
command namespace remains unambiguous. Do not expose provider-specific command
variants.

## Porting Map

| Existing code | Target | Port stance |
| --- | --- | --- |
| `GenerateRequest`, `GenerateResponse`, `StopReason` | `api/generation.rs` | Port fields, add serde and diagnostics. |
| `ProviderKind`, `parse_model_spec` | `api/provider.rs`, `model/spec.rs` | Port then tighten parser to target grammar. |
| `InferenceError` | `error/` | Replace string-only variants with stable codes while preserving source context. |
| `GenerationEngine` | `runtime/loader.rs` plus task engine internals | Port provider dispatch, narrow task trait surface. |
| `EmbeddingEngine` | `local/embedding.rs` | Port local embedding and batch logic. |
| `CloudEmbeddingEngine` | provider adapters plus embedder facade | Port OpenAI/Google batch behavior. |
| `RankingEngine` | `local/ranking.rs` | Port local cross-encoder scoring. |
| `provider/*.rs` | `provider/*.rs` | Port request builders and parsers; reject unsupported explicit knobs. |
| `llama/*.rs` | `local/*.rs` | Port only after backend lifecycle fix and unsafe audit tasks are in place. |
| `registry/*` | `registry/*` | Port catalog and local resolution; improve verification hooks. |
| `GenerateModelState` | `runtime/cache.rs` | Port model-cache idea, remove database extension assumptions. |
| `EmbedModelState` | `runtime/cache.rs` | Port lazy/retry idea, remove engine config reads. |
| `embed/download.rs` | `registry/download.rs` wrapper if still useful | Fold into registry API. |

## Implementation Order

### I1. Workspace And Crate Scaffold

1. Add `crates/inference-next` to the workspace.
2. Set version to `1.0.0`.
3. Add minimal features with empty default.
4. Add `#![deny(unsafe_code)]` at the crate root.
5. Allow unsafe only in the local runtime module when the `local` feature is
   enabled.
6. Add dependency guard tests proving no engine, storage, intelligence,
   executor, or CLI imports.
7. Add a no-default compile test before porting provider code.

Exit when `cargo test -p strata-inference-next --no-default-features` passes.

### I2. API DTOs And Error Codes

1. Port provider and task vocabulary.
2. Add serde derives for request/response/output-facing DTOs.
3. Add `ModelSpec` and deterministic first-colon parsing.
4. Add `InferenceCapability`.
5. Replace string-only errors with stable codes:
   - `inference.invalid_request`
   - `inference.missing_model`
   - `inference.model_load_failed`
   - `inference.unsupported_provider`
   - `inference.unsupported_operation`
   - `inference.unsupported_parameter`
   - `inference.missing_api_key`
   - `inference.provider_auth_failed`
   - `inference.provider_rate_limited`
   - `inference.provider_timeout`
   - `inference.provider_unavailable`
   - `inference.provider_malformed_response`
   - `inference.download_disabled`
   - `inference.download_failed`
   - `inference.download_verification_failed`
   - `inference.local_runtime_failed`
   - `inference.registry_corrupt`
   - `inference.io_failure`
6. Preserve source context for logs without exposing provider secrets or full
   prompt/document text by default.

Exit when API serde, parser, and error mapping tests pass.

### I3. Registry And Model Resolution

1. Port the static catalog.
2. Port `STRATA_MODELS_DIR` and user-model-dir resolution.
3. Port local listing, available listing, alias lookup, quant parsing, and
   local path resolution.
4. Keep model binaries outside databases.
5. Port download lock/temp/rename behavior behind `download`.
6. Add explicit verification behavior for known size and optional SHA-256.
7. Preserve helpful missing-model diagnostics.

Exit when registry tests pass with no network and download tests pass using a
local fixture or dependency-injected fetcher. The download slice is not closed
until a gated real model download or preseeded real GGUF resolution test proves
the resolved artifact can be loaded by the local runtime.

### I4. Cloud Provider Port

1. Port Anthropic, OpenAI, and Google request builders and response parsers.
2. Keep provider modules private behind task traits and runtime facade.
3. Add explicit timeout configuration.
4. Add network-disabled policy checks before HTTP execution.
5. Keep API keys caller-owned and redacted.
6. Stop silently ignoring material explicit request knobs. Provider adapters
   must either honor the knob or return `inference.unsupported_parameter`.
7. Keep live HTTP calls out of normal unit tests, but require gated live
   provider tests before declaring provider parity.

Exit when cloud feature matrix tests pass without network access and the gated
provider integration lane has executed real generation/embedding calls for each
enabled provider with real API keys.

### I5. Local Runtime Port And Unsafe Lifecycle

1. Port llama FFI and context code under `local/`.
2. Fix global backend lifecycle before exposing local runtime:
   - initialize once;
   - reference count or process-lifetime manage backend free;
   - prove dropping one engine cannot free the backend under another live
     engine.
3. Convert panic-prone null assertions to error returns where they can be
   reached from model execution.
4. Preserve existing context drop order: context before model.
5. Port local generation, embedding, batch embedding, tokenization,
   detokenization, and ranking.
6. Fix sampler cleanup on partial sampler construction failure.
7. Keep real-model tests environment-gated, but require them before accepting
   local runtime parity.
8. Record unsafe audit notes as part of implementation.

Exit when local feature builds in an environment with the vendor tree and
local constructor/error tests pass without model artifacts. The local runtime
slice is not complete until generation, embedding, tokenization, and ranking
have run against real GGUF files through the public runtime facade.

### I6. Runtime Facade And Caches

1. Add `InferenceRuntime`.
2. Port generation cache behavior from `GenerateModelState` without database
   extensions.
3. Port embedding lazy-load and retry behavior from `EmbedModelState` without
   engine config reads.
4. Add cache capacity, unload, status, stale-error retry, and health checks.
5. Treat cloud engines as cheap/stateless unless a provider-specific reason to
   cache exists.
6. Accept secrets through request/runtime secret resolver interfaces, not
   through persistent model registry metadata.
7. Add item-level outcomes for embed batch and rank.

Exit when deterministic cache tests pass and at least one gated real local
runtime and real cloud-provider path verifies cache load, reuse, unload, and
retry behavior through the public runtime facade.

### I7. Minimal Deterministic Test Harness

1. Add deterministic fake generator, embedder, and reranker.
2. Support configured success, unsupported parameter, missing model, auth
   failure, rate limit, timeout, malformed response, partial item failure, and
   latency.
3. Make fake providers available only through `testkit`.
4. Keep the harness small. It should not model provider-specific behavior,
   model quality, tokenization details, ranking semantics, or local runtime
   lifecycle.
5. Use the harness for command round-trip, executor delegation, error mapping,
   redaction, and retry branch tests only.

Exit when inference and executor tests use the harness only for deterministic
contract branches and real integration tests cover actual model/provider
execution.

### I8. Executor Integration

1. Add optional `inference` feature to `executor-next`.
2. Add command and output DTOs.
3. Add an executor-held `InferenceRuntime` or runtime handle that is independent
   of the engine database handle.
4. Route every inference command through the runtime facade.
5. Map inference errors into executor errors without losing stable code/class.
6. Add Rust convenience methods that build serialized commands.
7. Add source guards:
   - executor may import public inference facade/types;
   - executor may not import provider modules, local modules, FFI modules, or
     registry internals.
8. Do not add inference behavior to engine.

Exit when executor command round-trip tests pass and executor commands can
drive both the deterministic harness and the gated real inference runtime.

### I9. Closeout And Migration

1. Record which old inference files were fully ported, partially ported, or
   deliberately left behind.
2. Record which old intelligence files were split and where the remaining work
   belongs.
3. Fix the old no-default example issue or mark it as retired once the new
   example exists.
4. Keep old crates compiling until the cutover decision retires them.
5. Update architecture docs to point to the new crate once the command surface
   passes.

## Sub-Plan Triggers

Create focused sub-plans under `docs/architecture/implementation-plans/` if
any of these slices grows beyond a small reviewable patch:

1. local llama runtime and unsafe lifecycle;
2. structured error/code conversion;
3. runtime cache and executor integration;
4. minimal deterministic test harness;
5. model download verification;
6. provider unsupported-parameter semantics;
7. migration of old intelligence model lifecycle code.

The parent plan remains the source of truth for order and boundaries. Sub-plans
should cite this document and should not expand inference ownership into
database semantics.

## Source Guards

Add source/dependency guards proving:

1. `crates/inference-next` does not import:
   - `strata_engine`
   - `strata_engine_next`
   - `strata_storage`
   - `strata_storage_next`
   - `strata_intelligence`
   - `strata_executor`
   - `strata_executor_next`
   - CLI crates
2. `crates/inference-next/src/local/**` is the only place allowed to contain
   unsafe local native code.
3. `crates/executor-next` imports only public inference facade/API modules.
4. No provider API key appears in Debug output, error Display output, or test
   snapshots.
5. No benchmark, example, or integration tool bypasses the public runtime
   facade.

## Stop Conditions

Stop implementation and split the work if:

1. local llama build requires broad build-system changes;
2. provider parameter semantics require product decisions;
3. executor integration needs persistent database config or search/RAG context;
4. deterministic test harness design starts encoding provider-specific,
   model-quality, search recipe, or database behavior;
5. structured errors require changing the global executor error contract;
6. any slice tempts a rewrite of working provider or registry logic without a
   documented reason.

## Exit Criteria

The port is complete when:

1. `strata-inference-next` exists with empty default features.
2. API, registry, provider mapping, deterministic harness, and runtime cache
   tests pass.
3. Local runtime builds and gated real GGUF integration tests pass.
4. Executor can execute model list/local/pull, generate, tokenize, detokenize,
   embed, embed batch, rank, and unload through serialized commands.
5. No engine or storage crate depends on inference.
6. No executor code imports provider internals or local runtime internals.
7. Remaining intelligence behavior is explicitly deferred to the later
   Strata-aware model orchestration layer.
