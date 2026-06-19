# Inference Runtime Port Test Plan

Status: implemented normal-CI coverage; local GGUF lifecycle coverage remains
workstation-gated

Related implementation plan:

- `docs/architecture/implementation-plans/inference-runtime-port-implementation-plan.md`

## Purpose

Prove that the inference runtime port preserves working model execution
behavior from the old inference crate while exposing a safer, command-friendly
surface to executor. Tests should prevent accidental rewrites, hidden database
dependencies, provider-secret leaks, and native runtime lifecycle regressions.

The test strategy is:

1. use pure unit tests for API, parser, registry, provider JSON, and errors;
2. use real GGUF files and real provider API keys in a gated integration lane
   before claiming runtime parity;
3. use fake providers only as a narrow deterministic harness for command
   serialization, error mapping, retry branches, and secret-redaction tests;
4. use source guards to preserve boundaries;
5. verify ported behavior against old tests where old behavior is still valid.

## Test Matrix

| Area | No Default | Cloud Providers | Local Runtime | Executor Feature |
| --- | --- | --- | --- | --- |
| Compile | Required | Required by default | Required where vendor exists | Required |
| Unit tests | Required | Required | Required | Required |
| Serde round-trip | Required | Required | Required | Required |
| Model parser | Required | Required | Required | Required |
| Registry no-network behavior | Required | Required | Required | Optional |
| Provider JSON mapping | Not applicable | Required | Not applicable | Via deterministic harness |
| Deterministic harness | Narrow with testkit | Narrow with testkit | Narrow with testkit | Narrow with testkit |
| Real provider calls | Not run | Required gated integration | Not applicable | Required gated integration |
| Real GGUF integration | Not applicable | Not applicable | Required gated integration | Required gated integration |
| Source guards | Required | Required | Required | Required |

## Required Commands

Use exact commands where possible. Cloud providers are part of the default
inference product surface. Local feature commands require a populated llama.cpp
vendor tree and should not be part of ordinary CI until that setup is stable.

```sh
cargo test -p strata-inference-next
cargo test -p strata-inference-next --no-default-features
cargo test -p strata-inference-next --no-default-features --features testkit
cargo test -p strata-inference-next --no-default-features --features anthropic
cargo test -p strata-inference-next --no-default-features --features openai
cargo test -p strata-inference-next --no-default-features --features google
cargo test -p strata-inference-next --no-default-features --features anthropic,openai,google
cargo test -p strata-inference-next --no-default-features --features download,testkit
cargo test -p strata-executor-next --no-default-features --features inference,testkit
```

Local/native commands:

```sh
cargo test -p strata-inference-next --no-default-features --features local
cargo test -p strata-inference-next --no-default-features --features local,download
cargo test -p strata-executor-next --features inference,inference-local,testkit
```

Gated real integration commands:

```sh
STRATA_RUN_LOCAL_INFERENCE_INTEGRATION=1 \
STRATA_INFERENCE_GENERATION_GGUF=/path/to/generation.gguf \
STRATA_INFERENCE_EMBEDDING_GGUF=/path/to/embedding.gguf \
STRATA_INFERENCE_RANKING_GGUF=/path/to/ranking.gguf \
cargo test -p strata-inference-next --no-default-features --features local --test local_integration

STRATA_RUN_PROVIDER_INFERENCE_INTEGRATION=1 \
ANTHROPIC_API_KEY=... \
OPENAI_API_KEY=... \
GOOGLE_API_KEY=... \
cargo test -p strata-inference-next --no-default-features --features anthropic,openai,google --test provider_integration

STRATA_RUN_EXECUTOR_INFERENCE_INTEGRATION=1 \
STRATA_INFERENCE_GENERATION_GGUF=/path/to/generation.gguf \
ANTHROPIC_API_KEY=... \
OPENAI_API_KEY=... \
GOOGLE_API_KEY=... \
cargo test -p strata-executor-next --features inference,inference-local --test inference_integration
```

The exact test filenames may change during implementation, but the crate must
provide a documented way to run real local and real provider integration tests.
The environment variables above are the intended contract unless implementation
discovers a better naming scheme and updates this document in the same patch.

The old crate currently fails `cargo test -p strata-inference
--no-default-features` because `examples/validate_inference.rs` imports
generation types that are feature-gated out. The new crate must not repeat that
failure mode.

## API Contract Tests

### Request And Response Serde

Add JSON round-trip tests for:

1. `GenerateRequest`
2. `GenerateResponse`
3. `EmbedRequest`
4. `EmbedResponse`
5. `EmbedItemOutcome`
6. `RankRequest`
7. `RankResponse`
8. `RankItemOutcome`
9. `InferenceCapability`
10. `ModelInfo`
11. `ModelSpec`
12. `ProviderKind`
13. `ModelTask`
14. `StopReason`

Assertions:

1. omitted optional fields deserialize to documented defaults;
2. explicit zero, empty, and boundary values survive round-trip;
3. provider and task names serialize to stable strings;
4. diagnostics fields do not require provider-specific payloads;
5. secret-like fields are never included in normal outputs.

### Default Values

Test defaults for:

1. generation max tokens;
2. generation temperature;
3. top-k and top-p;
4. stop sequences;
5. stop tokens;
6. grammar;
7. embed batch behavior;
8. rank top-level options;
9. runtime timeout and network policy.

Defaults must be documented and must not trigger network or model download.

### Capability Tests

For every provider/task combination:

1. declare supported operations;
2. declare supported request knobs;
3. declare whether network is required;
4. declare whether auth is required;
5. declare known embedding dimension when available;
6. declare timeout behavior;
7. declare whether tokenizer operations are available.

Tests should assert that executor can reject unsupported explicit user intent
before execution when capability facts are available.

## Model Spec Parser Tests

Port existing model-spec parser tests and add stricter V1 cases:

1. bare local model name;
2. `local:model`;
3. `local:qwen3:1.7b:q8_0`;
4. `anthropic:model`;
5. `openai:model`;
6. `google:model`;
7. provider names with invalid casing if parser is case-sensitive;
8. leading whitespace;
9. trailing whitespace;
10. empty provider;
11. empty model;
12. unknown provider;
13. opaque provider model ids containing `/`, `.`, `-`, and additional `:`;
14. round-trip display where supported.

Add generated tests that build random model suffixes after the first colon and
assert the parser keeps the suffix opaque.

## Error Contract Tests

### Error Codes

Construct every stable error code and assert:

1. code string;
2. error class;
3. retry policy;
4. redacted user-facing message;
5. optional source context for logs;
6. serde round-trip;
7. conversion into executor error preserves code/class.

Required codes:

1. `inference.invalid_request`
2. `inference.missing_model`
3. `inference.model_load_failed`
4. `inference.unsupported_provider`
5. `inference.unsupported_operation`
6. `inference.unsupported_parameter`
7. `inference.missing_api_key`
8. `inference.provider_auth_failed`
9. `inference.provider_rate_limited`
10. `inference.provider_timeout`
11. `inference.provider_unavailable`
12. `inference.provider_malformed_response`
13. `inference.download_disabled`
14. `inference.download_failed`
15. `inference.download_verification_failed`
16. `inference.local_runtime_failed`
17. `inference.registry_corrupt`
18. `inference.io_failure`

### Redaction

Use fake keys and prompts containing obvious sentinels:

1. API key in provider struct;
2. API key in runtime config;
3. API key in failed HTTP path;
4. API key in Debug output;
5. prompt containing a sentinel secret;
6. document text containing a sentinel secret;
7. malformed provider body containing a sentinel secret.

Assert normal Display, Debug, serde output, and executor output do not leak
secrets. Internal source chains may include only redacted context.

## Registry Tests

Port existing catalog and registry tests:

1. catalog names and aliases are unique;
2. variant filenames are unique;
3. default quant exists;
4. all variants have nonzero size;
5. embedding models have dimensions;
6. generation/ranking models report dimension zero unless known otherwise;
7. alias lookup is deterministic;
8. quant override lookup is deterministic;
9. unknown model gives `inference.missing_model`;
10. unknown quant lists available quants without panic;
11. local model path resolves under the model directory;
12. zero-length local files are rejected;
13. local listing ignores unrelated files;
14. local listing ignores zero-length files;
15. `STRATA_MODELS_DIR` override works;
16. default model directory is stable and user-local.

## Download Tests

Download unit tests must not hit Hugging Face in normal CI. Use a local
fixture server or injected byte source for failure-path coverage. A separate
gated integration test should use either a real download or a preseeded real
GGUF file and then load that artifact through the local runtime.

Cases:

1. destination directory creation;
2. temp file write;
3. final rename only after complete write;
4. zero-length existing file is removed and redownloaded;
5. nonzero existing file is reused only if verification policy allows it;
6. content-length much smaller than catalog size is rejected;
7. content-length much larger than catalog size is rejected;
8. incomplete stream removes temp file;
9. write error removes temp file;
10. SHA-256 mismatch removes temp file and returns
    `inference.download_verification_failed`;
11. SHA-256 match publishes final file;
12. concurrent lock waits for another writer;
13. stale lock is reclaimed;
14. lock timeout returns deterministic error;
15. network-disabled policy prevents download before fetch starts.

## Cloud Provider Tests

Port request builder and response parser tests for Anthropic, OpenAI, and
Google.

### Request Builders

For each provider:

1. valid minimal generation request;
2. temperature zero is handled as documented;
3. max tokens zero is rejected before HTTP;
4. stop sequences are mapped correctly;
5. top-k supported or rejected;
6. top-p supported or rejected;
7. seed supported or rejected;
8. token stop ids supported or rejected;
9. grammar/constrained output supported or rejected;
10. prompt with quotes, Unicode, and newlines is encoded correctly;
11. model name appears in the correct field or URL;
12. API key never appears in URL when the provider supports headers.

Where old behavior silently ignored a material knob, new tests should expect a
structured unsupported-parameter result unless the product decision explicitly
keeps the old behavior.

### Response Parsers

For each provider:

1. normal completion;
2. max-token stop;
3. safety/content-filter/cancelled stop;
4. missing usage defaults or diagnostics;
5. missing text returns malformed-response error;
6. invalid JSON returns malformed-response error;
7. provider error JSON maps to provider error code;
8. empty choices/candidates/content arrays return malformed-response error;
9. unknown stop reason maps to a deterministic diagnostic;
10. extra fields are ignored.

### HTTP Error Mapping

Map provider failures:

1. 400 invalid request;
2. 401 or 403 auth failure;
3. 429 rate limit;
4. timeout;
5. 500/502/503 provider unavailable;
6. DNS/connect failure;
7. response body read failure.

Each maps to a stable inference code and retry policy.

### Live Provider Integration

Provider parity is not complete until real calls have been made with real API
keys. These tests must be explicitly gated by environment variables so normal
CI does not spend tokens or depend on external service uptime.

For each enabled provider:

1. minimal generation request succeeds;
2. explicit low token limit is honored;
3. provider usage/token diagnostics are captured when available;
4. unsupported explicit knobs return structured unsupported-parameter errors;
5. missing key returns `inference.missing_api_key`;
6. invalid key returns provider auth failure without leaking the key;
7. timeout is bounded by runtime configuration;
8. OpenAI and Google embedding requests return finite vectors with known or
   reported dimensions where supported.

## Embedding Tests

### DTO And Outcome Tests

1. single embed success;
2. empty input behavior;
3. batch success preserves order;
4. batch empty input returns empty response;
5. per-item failure preserves positional outcome;
6. provider-wide failure fails the whole request;
7. returned dimension matches response metadata;
8. normalization flag or documented normalization behavior is visible.

### Local Embedding Tests

Without real models:

1. nonexistent GGUF path maps to local runtime/model-load failure;
2. invalid UTF-8 path maps to local runtime failure;
3. null byte path maps to invalid request or local runtime failure;
4. batch splitting logic can use a small deterministic local harness only for
   branch coverage that is impractical to trigger through real GGUF files.

With gated real model:

1. MiniLM embedding has expected dimension;
2. L2 norm is approximately one for nonempty text;
3. empty text returns documented zero-vector behavior;
4. batch equals single results within tolerance;
5. long text truncates without panic.

## Ranking Tests

Without real models:

1. empty passages returns empty result;
2. missing model maps to missing model;
3. cloud ranking returns unsupported operation unless a provider implements it;
4. item order is preserved;
5. partial item failure is represented only in deterministic harness tests for
   branch coverage; real ranking behavior must use real model/provider output.

With gated real model:

1. single passage returns finite score;
2. multiple passages preserve order;
3. obviously relevant passage scores above irrelevant passage for fixture query;
4. long query/passage truncates without panic.

## Generation Tests

### DTO Tests

1. generation request defaults;
2. explicit sampler knobs round-trip;
3. stop reason display/serde;
4. prompt token and completion token counts survive response serialization;
5. diagnostics omit prompt text by default.

### Local Generation Tests

Without real models:

1. nonexistent GGUF path returns local runtime/model-load failure;
2. zero context override is rejected;
3. tokenizer unavailable before load is not exposed through executor;
4. sampler construction failure cleans up any created sampler objects if this
   requires an injected local binding for unreachable native failure branches.

With gated real model:

1. deterministic greedy generation with a small prompt;
2. max tokens zero returns documented output or validation error;
3. prompt longer than context returns context error;
4. stop token stops generation;
5. stop sequence truncates earliest match;
6. encode then decode round-trip is sane for fixture text.

## Local Runtime Unsafe Lifecycle Tests

The local runtime must have focused tests because the old implementation had a
global backend lifecycle hazard.

Required tests:

1. loading two local runtime handles initializes backend once;
2. dropping one runtime while another is live does not call backend free;
3. final backend free behavior is either process-lifetime or ref-counted and
   documented;
4. context drops before model;
5. failed context creation frees model;
6. null model load returns error, not panic;
7. null vocab returns error where reachable, not panic;
8. null logits returns error where reachable, not panic;
9. null embedding pointer returns error, not panic;
10. batch allocation/free is paired on error paths;
11. sampler allocation/free is paired on error paths;
12. `Send` and `Sync` claims are covered by comments and stress tests.

If direct testing requires FFI shims, create a local-runtime sub-plan rather
than hiding the risk in untested unsafe code.

## Runtime Cache Tests

Port the useful behavior from old intelligence model state tests, but remove
database extension assumptions.

Generation cache:

1. default cache is empty;
2. failed local model load is cached according to documented retry policy;
3. unload removes cached entry;
4. same model returns same cached entry;
5. concurrent same-model load coalesces;
6. different models can be generated independently;
7. capacity limit evicts only non-current entries;
8. cloud engines are not cached unless explicitly configured;
9. changing API key does not use stale cloud state;
10. unhealthy local engine is evicted before retry.

Embedding cache:

1. default state has no dimension;
2. same model returns cached engine;
3. model change reloads;
4. unhealthy engine reloads;
5. retry limit is enforced;
6. unload/reset clears retry state;
7. embedding dimension is reported only after successful load;
8. provider secrets are not stored in model metadata.

Ranking cache:

1. same local rank model reuses engine;
2. unsupported cloud ranking is not cached as a local engine;
3. unload works;
4. failures have retry policy consistent with model-load errors.

## Minimal Deterministic Harness

The deterministic harness exists to make command contracts, redaction, retry
branches, and malformed-response handling cheap and repeatable. It must not be
treated as behavioral proof of provider quality, tokenization, local runtime
lifecycle, embedding dimensions, ranking semantics, or model output.

Harness tests should cover:

1. deterministic generation text;
2. deterministic token counts;
3. deterministic embedding vectors;
4. deterministic ranking scores;
5. configured latency without sleeps in unit tests;
6. missing model;
7. invalid request;
8. unsupported parameter;
9. unsupported operation;
10. missing API key;
11. auth failure;
12. rate limit;
13. timeout;
14. provider unavailable;
15. malformed provider response;
16. partial embedding item failure;
17. partial ranking item failure;
18. secret redaction.

Executor and future intelligence tests may use this harness for contract tests,
but any claim that inference behavior works must be backed by the gated real
GGUF/provider integration lane.

## Executor Command Tests

Run executor tests with `--features inference,testkit`.

### Command Round Trip

Serialize and deserialize:

1. `InferenceModelsList`
2. `InferenceModelsLocal`
3. `InferenceModelsPull`
4. `InferenceModelCapability`
5. `InferenceGenerate`
6. `InferenceTokenize`
7. `InferenceDetokenize`
8. `InferenceEmbed`
9. `InferenceEmbedBatch`
10. `InferenceRank`
11. `InferenceUnload`

Assert stable command names and backward-compatible field defaults.

### Output Round Trip

Serialize and deserialize:

1. model list output;
2. model pull output;
3. capability output;
4. generation output;
5. token id output;
6. text output;
7. single embedding output;
8. batch embedding output with item failures;
9. rank output with item failures;
10. unload output;
11. inference error output with stable code/class.

### Behavior Through Deterministic Harness

1. model list returns fake plus catalog entries as configured;
2. model local returns configured local entries;
3. model pull refuses when network is disabled;
4. generate returns configured text;
5. generate unsupported explicit knob maps to executor error;
6. tokenize works only for tokenizer-capable fake model;
7. detokenize works only for tokenizer-capable fake model;
8. embed returns deterministic vector;
9. embed batch preserves order;
10. rank preserves order;
11. unload removes local cached model;
12. provider secret is not present in output.

### Behavior Through Real Runtime

Run gated executor integration tests that use the same public commands against
real inference runtime configuration:

1. local model list sees preseeded GGUF files;
2. local generate returns nonempty output through `InferenceGenerate`;
3. local tokenize and detokenize round-trip fixture text through executor;
4. local embed returns a finite vector through `InferenceEmbed`;
5. local embed batch preserves order through `InferenceEmbedBatch`;
6. local rank preserves item identity and returns finite scores through
   `InferenceRank`;
7. cloud generation succeeds through a real provider API key;
8. cloud embedding succeeds for providers that support embedding;
9. unload removes the cached local runtime and a subsequent request reloads;
10. no prompt text, document text, provider response body, or API key is
    emitted in normal command output.

### Executor Delegation Guards

1. executor convenience methods call `execute(Command::Inference...)`;
2. executor code does not import `provider::`, `local::`, `ffi`, or registry
   internals from inference;
3. executor code does not construct provider HTTP requests;
4. executor code does not call llama APIs;
5. executor code does not read/write model files directly;
6. executor code maps inference errors without string parsing where possible.

## Source And Dependency Guards

Add guard tests or scripts for:

1. inference crate has no engine/storage/intelligence/executor/CLI dependency;
2. inference source has no storage, branch, space, commit, WAL, table,
   snapshot, search recipe, shadow vector, RAG, or database open imports;
3. local unsafe code is isolated under the local runtime module;
4. provider modules are private;
5. executor imports only public inference facade/API modules;
6. examples compile under no-default or are correctly feature-gated;
7. no test fixture hardcodes real API keys;
8. no committed benchmark or integration output contains provider responses.

## Gated Real Integration Tests

Real model and provider integration tests are required before closing the
inference port, but they must not gate normal CI because they need local model
artifacts, native runtime setup, network access, and real provider credentials.

Local integration tests require:

1. populated llama.cpp vendor tree;
2. model files in `STRATA_MODELS_DIR` or default model directory;
3. explicit environment variable such as
   `STRATA_RUN_LOCAL_INFERENCE_INTEGRATION=1`.

Provider integration tests require:

1. provider feature enabled;
2. explicit API key;
3. explicit environment variable such as
   `STRATA_RUN_PROVIDER_INFERENCE_INTEGRATION=1`;
4. low token limits;
5. redacted output.

Integration assertions should be conservative:

1. request succeeds;
2. output has expected shape;
3. token/vector counts are sane;
4. no secret leaks;
5. timeout is bounded.

## Regression Tests From Old Crates

Port old tests when the behavior remains valid:

1. `GenerateRequest` defaults and clone tests;
2. `ProviderKind` parsing/display tests, adjusted for target casing decision;
3. stop reason display tests;
4. registry alias and quant tests;
5. provider request/response JSON tests;
6. cloud embedding parser tests;
7. L2 normalization tests;
8. stop-sequence truncation tests;
9. generation cache tests;
10. embedding load retry tests.

Do not port tests that assert old undesirable behavior:

1. silent ignoring of material explicit knobs;
2. broad inference re-exports through intelligence;
3. default native local build;
4. examples that fail under no-default;
5. database config reads inside inference runtime.

## Closeout Checklist

The test plan is complete when:

1. no-default inference tests pass;
2. cloud feature matrix passes without network;
3. local feature tests pass where vendor setup exists;
4. deterministic harness covers executor command serialization, redaction, and
   error branches without pretending to validate real model behavior;
5. executor inference command serde and behavior tests pass;
6. source guards prove crate boundaries;
7. old portable tests are either ported or listed as deliberately retired;
8. gated real GGUF and provider integration tests are documented and have been
   run before parity closeout, but are not required for ordinary CI;
9. no secret redaction test fails;
10. local runtime unsafe lifecycle hazards have direct tests or a focused
    sub-plan before local runtime is product-enabled.
