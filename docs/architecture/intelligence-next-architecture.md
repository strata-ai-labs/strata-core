# Intelligence-Next Architecture

Status: V1 architecture draft

## Purpose

This document defines the target architecture for `intelligence-next`, the
Strata-aware model orchestration layer.

Intelligence-next sits above engine-next and inference-next:

```text
engine-next ----\
                 -> intelligence-next -> executor / cli / SDK / Strata AI
inference-next -/
```

Engine-next owns database semantics, deterministic retrieval, branches, entity
references, derived-state storage, recipes, and command behavior. Inference-next
owns model and provider execution. Intelligence-next is the layer that combines
those two worlds without letting either leak into the other.

The current `strata-intelligence` crate is small and generally well bounded.
It should not receive engine-style invasive surgery. The V1 work is to make its
contracts explicit, remove confusing feature and re-export surfaces, and define
repeatable patterns for model-assisted retrieval and derived-state workflows.

## Related Documents

Architecture anchors:

1. [strata-v1-architecture.md](./strata-v1-architecture.md)
2. [engine-next-architecture.md](./engine-next-architecture.md)
3. [inference-next-architecture.md](./inference-next-architecture.md)
4. [strata-ai-architecture.md](./strata-ai-architecture.md)
5. [runtime-resource-profile-architecture.md](./runtime-resource-profile-architecture.md)
6. [v1-error-and-diagnostics-contract.md](./v1-error-and-diagnostics-contract.md)
7. [v1-testing-and-conformance-plan.md](./v1-testing-and-conformance-plan.md)
8. [v1-engineering-standards.md](./v1-engineering-standards.md)

Engine contracts consumed by intelligence:

1. [engine-next/retrieval-and-derived-state-contract.md](./engine-next/retrieval-and-derived-state-contract.md)
2. [engine-next/control-plane-layout-contract.md](./engine-next/control-plane-layout-contract.md)
3. [engine-next/primitive-implementation-contract.md](./engine-next/primitive-implementation-contract.md)
4. [engine-next/entity-ref-and-relationship-layer-contract.md](./engine-next/entity-ref-and-relationship-layer-contract.md)
5. [engine-next/product-pathway-conformance-plan.md](./engine-next/product-pathway-conformance-plan.md)

Product direction:

1. [strata-v1-product-requirements.md](../product/strata-v1-product-requirements.md)
2. [pathways/retrieval-and-intelligence.md](../product/pathways/retrieval-and-intelligence.md)
3. [strata-autosearch-product-direction.md](../product/strata-autosearch-product-direction.md)

## Requirement Language

1. Must means V1 intelligence architecture is incomplete without it.
2. Should means expected for V1 unless a later architecture decision records a
   clear deferral.
3. May means allowed but not required for V1.

## Product Role

Intelligence-next exists so Strata can support:

1. Autoembedding of KV, JSON, and event values into shadow vectors.
2. Manual embedding operations.
3. Model-assisted query expansion.
4. Cross-encoder reranking.
5. RAG answer generation over engine search hits.
6. Explicit generation commands.
7. Model configuration and model lifecycle behavior surfaced through product
   APIs.
8. Future Autosearch workflows that tune retrieval recipes over branches.
9. Strata AI workflows that need database context and local model execution.

Intelligence-next should make AI features feel native to Strata without making
engine-next depend on models or making inference-next know about databases.

## Current Codebase Evidence

The current crate is `crates/intelligence`.

Verified high-level shape:

1. `strata-intelligence` depends on `strata-core`, `strata-engine`, and
   optional `strata-inference`.
2. It has no direct dependency on `strata-storage`.
3. Default features are empty.
4. The current `embed` feature enables inference, local model support, model
   download, and all model-dependent modules.
5. `anthropic`, `openai`, and `google` features forward to corresponding
   inference provider features.
6. Executor and CLI currently consume intelligence APIs and re-exported
   inference types instead of importing `strata-inference` directly.

Current files and responsibilities:

1. `src/lib.rs`
   - feature-gated modules
   - broad inference re-exports

2. `src/embed/mod.rs`
   - `EmbedModelState`
   - embedding model lazy load and retry behavior
   - query embedding helpers
   - cloud API key resolution through engine config

3. `src/embed/runtime.rs`
   - autoembedding queue
   - background refresh loop
   - engine scheduler submission
   - shadow vector writes into engine system collections
   - reindex flow over KV, JSON, and event records
   - runtime status counters

4. `src/embed/extract.rs`
   - deterministic text extraction from `strata_core::Value`
   - depth and length bounds

5. `src/embed/download.rs`
   - model download wrapper over inference model registry

6. `src/generate.rs`
   - `GenerateModelState`
   - local generation engine cache
   - uncached cloud generation engine creation

7. `src/expand.rs`
   - model-assisted typed query expansion
   - grammar-constrained generation
   - strategy filtering
   - hallucination guard

8. `src/expand_cache.rs`
   - persistent branch-local expansion cache in engine `_system_` space
   - FIFO eviction sidecar
   - fork inheritance through branch versioning

9. `src/rerank.rs`
   - cross-encoder scoring
   - blend with engine search scores
   - graceful fallback to original hits

10. `src/rag/`
    - prompt construction
    - sandwich ordering
    - type-tagged context
    - citation extraction
    - answer metadata

11. `src/shadow.rs`
    - older shadow-vector cleanup support without the full runtime queue

Current executor touchpoints:

1. Search handler calls intelligence for query embedding, expansion, rerank,
   and RAG answer generation.
2. KV, JSON, and event handlers call intelligence autoembedding hooks after
   source writes.
3. Embed and generate handlers use intelligence model state.
4. Model commands use intelligence re-exports of inference model registry.
5. Config commands control `auto_embed`, `embed_model`, and
   `embed_batch_size`.

## Binding V1 Decisions

1. **Intelligence-next depends on engine-next and inference-next only.**
   It may consume engine product APIs and inference model APIs. It must not
   import storage-next or bypass engine persistence.

2. **Intelligence-next owns model-dependent Strata behavior.**
   It owns when to call models for embeddings, expansion, reranking, RAG, or
   generation. It does not own the model runtime itself.

3. **Engine-next remains the source of database truth.**
   Intelligence-next may write derived state through engine APIs, but source
   rows, branch semantics, search indexes, vector collections, relationship
   facts, recipes, and control-plane layout are engine-owned.

4. **Inference-next remains the source of model execution truth.**
   Intelligence-next should not implement provider clients, llama.cpp FFI,
   model artifact download mechanics, tokenization runtimes, or provider JSON.

5. **No direct storage access.**
   Any intelligence state that must persist lives in engine-owned system space,
   branch-local system space, or engine-owned derived-state manifests. The
   intelligence crate does not open files, WAL, manifests, tables, snapshots,
   or storage rows.

6. **Model calls are explicit and observable.**
   Database open, recovery, checkpoint, compaction, and ordinary maintenance
   must not call model providers. Model calls happen from explicit product
   operations or configured background intelligence jobs.

7. **Model-assisted retrieval degrades gracefully but reports degradation.**
   Query expansion, reranking, and RAG may return original hits or no answer
   when a model is unavailable. The degraded path must be reflected in stats or
   diagnostics. Silent quality loss is not acceptable for V1 product behavior.

8. **Autoembedding is derived state, not source data.**
   Shadow vectors are rebuildable from source KV, JSON, and event rows plus the
   configured embedding policy. Autoembedding failures must not roll back the
   source write, but they must be visible through status, counters, and rebuild
   paths.

9. **Shadow vectors are engine-owned derived rows.**
   Intelligence-next decides what text to embed and when. Engine-next owns the
   vector capability, system collections, source references, branch-local
   visibility, and cleanup semantics.

10. **Recipe storage is engine-owned; model stage execution is
    intelligence-owned.**
    Engine-next resolves recipe structure and deterministic retrieval stages.
    Intelligence-next executes model-dependent stages referenced by those
    recipes.

11. **RAG policy is intelligence-owned.**
    Prompt construction, context ordering, type-tagged context, citation
    extraction, grounded-answer behavior, and RAG answer metadata belong in
    intelligence-next. Inference-next only generates text.

12. **Generation model state is runtime state, not durable database state.**
    Loaded local engines and cloud provider adapters live in process memory.
    Durable configuration stores model specs and provider settings, not loaded
    model handles.

13. **Cloud provider adapters are not cached across API key changes.**
    The current behavior is correct: local generation engines may be cached,
    but cloud provider adapters should be rebuilt from current config so key
    changes are respected.

14. **The current `embed` umbrella feature should be retired.**
    Target feature names should match product capability: embedding,
    generation, retrieval-augmentation, and provider pass-throughs. RAG is part
    of retrieval augmentation for V1. A feature named `embed` must not survive
    as a compatibility alias in the cutover-style rewrite.

15. **Broad inference re-exports should shrink.**
    Executor and CLI should not depend directly on inference, but
    intelligence-next should expose Strata-shaped model APIs rather than
    re-exporting the entire lower inference surface. Selected provider/model
    identifiers may be re-exported only when they are part of the product
    contract: `ProviderKind`, `ModelTask`, `ModelInfo`, `ModelSpec`, and
    `ResolvedModel`. Concrete inference engines, registry internals, provider
    traits, and provider-specific request types are not re-exported.

16. **Autosearch is intelligence-owned but not a V1 minimum.**
    Autosearch should eventually orchestrate branch-based recipe experiments
    using engine branches, engine retrieval, and inference model calls. V1
    should preserve the substrate, but full Autosearch can remain a follow-up
    product feature.

17. **Intelligence-next has no IPC transport responsibility.**
    It may define serializable operation outcomes consumed by command handlers.
    Executor, CLI, SDK, IPC runtime, and Strata AI own transport and command
    rendering.

18. **Secrets are consumed, not owned.**
    Intelligence-next may request provider credentials from engine-owned config
    or caller-supplied runtime context. It must not persist raw API keys or
    leak them in diagnostics.

19. **Explicit generation remains a feature-gated product utility.**
    Generation commands are legitimate intelligence operations. They should
    stay above engine and below command surfaces, use inference-next for model
    execution, and report model/provider diagnostics cleanly.

20. **External on-prem model stacks are post-V1.**
    Intelligence-next should consume model execution through the inference-next
    provider gateway so future vLLM, NIM, Ollama, LM Studio, llama.cpp server,
    and other OpenAI-compatible endpoint adapters do not require intelligence
    rewrites. V1 does not need to ship those adapters.

21. **Provider execution is a solved lower-layer concern.**
    Intelligence-next treats inference providers as opaque executors. It asks
    inference-next for model outputs, token counts, embeddings, ranking scores,
    capability facts, and diagnostics. It must not branch on provider-specific
    transport, endpoint compatibility, tokenization, HTTP shape, native runtime
    details, or model artifact layout.

22. **Embedding model mismatch is a rebuild-required freshness failure.**
    If a recipe selects an embedding model whose spec hash, dimension, or metric
    does not match the shadow-vector manifest, intelligence-next must not try to
    paper over the mismatch with a best-effort search. Engine retrieval detects
    the mismatch before model execution and surfaces
    `failed_precondition.embedding_model_mismatch`; intelligence-next may then
    schedule or request an explicit reindex.

23. **Model management is split by database awareness.**
    Inference-next owns registry mechanics, local artifact resolution,
    downloads, and provider capability facts. Intelligence-next owns
    database-aware model selection: recipe selection, branch-local policy,
    autoembedding policy, and mapping model diagnostics into Strata stage
    outcomes.

## Responsibilities

Intelligence-next owns:

1. Model-assisted operation orchestration.
2. Query embedding requests over engine configuration and inference models.
3. Autoembedding queues, flushing, reindex initiation, and status.
4. Text extraction policy for embedding source records.
5. Shadow-vector source mapping decisions.
6. Query expansion generation, filtering, and cache policy.
7. Reranking orchestration and score blending policy.
8. RAG prompt construction, context formatting, citation extraction, and answer
   metadata.
9. Generation model cache policy over inference engines.
10. Model lifecycle product helpers that need database context.
11. Intelligence diagnostics, degradation reasons, and stage timing.
12. Fake model/testkit support for deterministic product-path tests.
13. Future Autosearch orchestration over branches and recipes.

Intelligence-next does not own:

1. Storage persistence.
2. Engine primitive semantics.
3. Engine branch operations.
4. Engine search index mutation.
5. Engine recipe registry layout.
6. Vector collection storage.
7. Provider HTTP clients.
8. llama.cpp FFI.
9. Model artifact verification.
10. Provider-specific tokenization.
11. Provider endpoint compatibility shims.
12. IPC transport.
13. CLI parsing.
14. StrataHub clone, publish, or fleet sync.

## Engine Surface Consumed

Intelligence-next must consume engine-next through named product/internal
surfaces, never by reaching into storage or private engine tables.

The exact Rust names can be chosen during implementation, but the required
engine surfaces are:

1. Source-write observation hooks for KV, JSON, and event writes that need
   autoembedding work.
2. Shadow-vector write APIs that accept source `EntityRef`, source version,
   model spec hash, embedding dimension, distance metric, vector bytes, and
   derived-state provenance.
3. Source-delete and space-delete hooks that remove or schedule removal of
   matching shadow vectors.
4. Autoembedding policy read/write APIs over engine-owned control-plane rows.
5. Branch-local model configuration and credential lookup APIs, returning
   redacted diagnostics and consumable secrets only to authorized model calls.
6. Derived-state freshness and manifest APIs for checking recipe hash, model
   spec hash, dimension, metric, source coverage, and rebuild state.
7. Scheduler APIs for background derived-state jobs, flush, reindex, and
   bounded repair.
8. Branch-local system cache APIs for query expansion cache entries.
9. Retrieval input/output DTOs that expose stable hits, snippets, provenance,
   scores, and recipe fragments for expansion, rerank, and RAG stages.
10. Diagnostics APIs for publishing stage outcomes, degraded operation facts,
    and rebuild-required status.

These surfaces are engine-next obligations. Intelligence-next may wrap them in
ergonomic helpers, but it does not define alternative persistence contracts.

## Target Crate Shape

The exact file names can change during implementation, but intelligence-next
should stay domain-shaped and small:

```text
crates/intelligence-next/
  Cargo.toml
  src/
    lib.rs                       # public re-exports only
    api/                         # Strata-shaped requests, outcomes, stats
    error/                       # intelligence errors and diagnostics
    model/                       # model specs, model state, provider routing
    embedding/
      extract.rs                 # source row -> text
      runtime.rs                 # queue, flush, reindex, status
      shadow.rs                  # shadow-vector naming and cleanup helpers
    retrieval/
      expansion.rs               # query expansion stage
      expansion_cache.rs         # branch-local cache adapter
      rerank.rs                  # cross-encoder rerank stage
      rag.rs                     # answer generation stage
      prompt.rs                  # prompt/context construction
      citation.rs                # RAG citation parsing helper
    diagnostics/                 # stage diagnostics and explain output
    testkit/                     # fake model gateway and fixtures
```

This is not a mandate to create one file per current helper. It is a target
shape that keeps the concepts repeatable:

1. Model gateway.
2. Stage request.
3. Stage outcome.
4. Stage diagnostics.
5. Derived-state job.
6. Runtime state.
7. Test fixture.

Avoid feature-shaped one-off vocabulary. The implementation should not invent a
new named struct family for every retrieval option.

Future modules should be added only when they own shipped behavior.
`autosearch/` is the expected post-V1 home for branch-based recipe tuning, but
the V1 tree should not contain an empty module for it.

The target shape follows the V1 engineering standards. Roadmap labels and
cleanup-era labels must not become intelligence module names, feature flags,
test names, errors, telemetry fields, public APIs, recipe names, or stage
diagnostics. Temporary `intelligence-next` package naming is build-branch
scaffolding only; code inside the crate should use permanent retrieval,
embedding, model, diagnostics, and derived-state vocabulary.

## Repeatable Stage Pattern

Expansion, reranking, RAG, and future Autosearch steps should follow one common
stage pattern.

Every model-assisted stage should define:

1. Input:
   query, branch, recipe fragment, candidate hits, model spec, and runtime
   budget as applicable.

2. Output:
   transformed hits, generated answer, generated variants, or no-op result.

3. Diagnostics:
   whether the stage ran, skipped, degraded, failed, or returned no useful
   result.

4. Model use:
   provider kind, model spec, elapsed time, token counts where available, and
   whether network was used. These facts come from inference-next diagnostics;
   intelligence-next should not inspect provider internals to derive them.

5. Degradation behavior:
   what the caller receives when the stage cannot run.

6. Provenance:
   enough facts for search stats, RAG citations, recipe tuning, and future
   Autosearch comparison.

The stage pattern should replace scattered booleans such as `was_reranked` and
ad hoc `Option<RagAnswer>` semantics over time. The product can still render
simple outputs, but the internal contract should preserve why a stage did or
did not run.

## Autoembedding Contract

Autoembedding is the main background intelligence workflow.

V1 requirements:

1. Source writes remain authoritative.
2. Autoembedding is asynchronous derived-state work.
3. Source write success does not depend on embedding success.
4. Pending work is observable.
5. Failed work is counted.
6. Reindex can rebuild shadow vectors for a branch.
7. Shadow vectors carry source references.
8. Shadow vectors live in branch-local system collections through engine APIs.
9. Deleting a source record removes or schedules removal of its shadow vector.
10. Deleting a space removes shadow vectors for that space.
11. Changing embedding model requires explicit reindex. Until reindex
    completes, affected shadow-vector manifests are stale and rebuild-required.
12. The manifest for shadow vectors must include recipe or index configuration
    hash, embedding model spec hash, embedding dimension, distance metric,
    source coverage, source commit frontier, build status, and last failure
    code.
13. Model mismatch is detected by engine retrieval before inference model
    execution and surfaced as `failed_precondition.embedding_model_mismatch`.
14. Query embedding for shadow-vector search must be preceded by an engine
    freshness/manifest check when the recipe targets existing shadow vectors.
    Intelligence-next should not spend model work on a query embedding when the
    selected model cannot match the branch's shadow-vector family.

The current runtime already queues work, uses the engine scheduler, writes
shadow vectors with source refs, and exposes status counters. V1 should add a
clearer derived-state manifest/watermark story through engine-next so users can
tell whether embeddings are fresh, stale, rebuilding, or degraded.

When a cloned dataset references a local embedding model that is missing on the
new machine, source data remains usable. Autoembedding rebuild and any recipe
that requires that model report a missing-model or embedding-unavailable
diagnostic according to recipe policy; Strata must not silently substitute a
different model for existing shadow vectors.

## Retrieval-Augmentation Contract

Intelligence-next executes model-assisted retrieval stages over engine search
results.

### Query Expansion

Expansion should:

1. Use a generation model selected by recipe or config.
2. Produce typed variants such as lexical, vector, and hypothetical document
   queries.
3. Constrain or parse model output into typed variants.
4. Filter variants that fail strategy or hallucination guards.
5. Cache results in branch-local engine system space when configured.
6. Treat cache corruption as a miss.
7. Report whether expansion ran, hit cache, failed, or produced no variants.

### Reranking

Reranking should:

1. Select top candidates from engine retrieval.
2. Score candidate snippets with a ranking model.
3. Read top-N, blend weights, and score-combination policy from the engine
   recipe fragment.
4. Blend model scores with deterministic retrieval scores using that recipe
   policy.
5. Return original hits unchanged on degradation.
6. Report whether reranking ran, skipped due to too few candidates, failed to
   load a model, failed during scoring, or produced malformed score counts.

### RAG

RAG should:

1. Run only when the recipe requests answer generation.
2. Build bounded context from engine search hits.
3. Preserve entity references in type-tagged context.
4. Use ordering policy that reduces lost-in-the-middle behavior.
5. Call inference for answer generation.
6. Extract citations and validate them against available hits.
7. Return no answer or a canned no-context response according to policy.
8. Report model, elapsed time, token counts, source count, and degradation
   reasons.

## Model State And Configuration

Intelligence-next needs runtime model state but should keep it simple.

V1 requirements:

1. Loaded local generation engines may be cached by model spec.
2. Loaded embedding engines may be cached by model spec.
3. Cloud provider adapters should be recreated from current credentials.
4. Runtime state should live in process memory, not in durable database rows.
5. Durable config stores model specs, flags, and policy, not model handles.
6. Model lifecycle errors should preserve provider/model context.
7. Resource hints from engine runtime profiles may constrain batch size,
   context size, concurrency, and network/download permission.
8. A timeout or unhealthy result from a cached local engine evicts that cache
   entry before the next request for the same model spec.

The current `GenerateModelState` and `EmbedModelState` are the right shape in
spirit, but target APIs should return structured diagnostics rather than plain
strings.

## Feature Flags

The target feature model should keep the default small:

1. `default`
   No model execution features.

2. `embedding`
   Embedding lifecycle, manual embedding helpers, and autoembedding runtime.

3. `generation`
   Generation model lifecycle and explicit generation helpers.

4. `retrieval-augmentation`
   Query expansion, reranking, and RAG orchestration.

5. Provider features:
   - `local`
   - `anthropic`
   - `openai`
   - `google`
   - `openai-compatible` after V1, once the inference adapter exists

6. `testkit`
   Fake model gateway, fake stage outcomes, and deterministic fixtures.

Current `embed` is an implementation-era umbrella feature. It should be
removed during cutover so feature names do not misdescribe the crate.
Autosearch is a post-V1 capability and should not appear as a V1 feature until
it owns shipped behavior.

## Errors And Diagnostics

Intelligence-next should map inference and engine failures into
model-assisted product outcomes.

Required diagnostics:

1. Stage name.
2. Branch and recipe identity where available.
3. Model spec.
4. Provider kind.
5. Whether network was used.
6. Whether cached data was used. In V1 this applies to query expansion cache;
   RAG prompt/context caching is post-V1 unless a later product decision pulls
   it forward.
7. Stage timing.
8. Token counts where available.
9. Candidate counts.
10. Degradation reason.
11. Rebuild or reindex status for derived state.

Required error/degradation families:

1. Model unavailable.
2. Model load failed.
3. Provider unavailable.
4. Missing provider credential.
5. Unsupported provider or operation.
6. Unsupported request knob.
7. Prompt construction failed.
8. Citation parse failed or yielded no valid citations.
9. Expansion parse failed.
10. Rerank score count mismatch.
11. Derived-state stale.
12. Derived-state rebuild failed.
13. Engine read/write failed.

Most model-assisted search failures should not abort the entire search. They
should produce degraded stage outcomes. Write-side or control-plane failures
that affect source data must still surface as normal engine errors.

## Test And Conformance Plan

Required tests:

1. Default-feature compile test with no inference dependency.
2. Feature matrix compile tests for model features and provider features:
   - default,
   - `embedding`,
   - `generation`,
   - `retrieval-augmentation`,
   - each provider feature alone,
   - `embedding` + `generation` + `local`,
   - `embedding` + `retrieval-augmentation` + `local` + `openai`,
   - `embedding` + `generation` + `retrieval-augmentation` + `local` +
     `anthropic` + `openai` + `google` + `testkit`.
3. Dependency guard proving no `strata-storage` import.
4. Guard proving executor and CLI do not import inference directly. This should
   be a `cargo_metadata`-based dependency guard so it sees normal dependencies,
   dev-dependencies, and feature-gated edges.
5. Text extraction property tests for depth, length, ordering, and unicode
   boundaries.
6. Autoembedding queue tests:
   - source write queues work,
   - source delete removes pending and stored shadow vectors,
   - space delete removes matching shadow vectors,
   - reindex queues expected record families,
   - per-item embedding failures retry or mark only the failed source records.
7. Embedding model state tests for retry, stale model switching, health, and
   batch dimensionality.
8. Generation model state tests for caching, cloud no-cache behavior, eviction,
   and stale key avoidance.
9. Expansion parser/filter/cache tests.
10. Expansion cache fork-inheritance tests.
11. Rerank fallback and score-blending tests.
12. RAG prompt budget, type-tag, sandwich-order, and citation tests.
13. Fake inference provider tests for expansion, rerank, RAG, and generation.
14. Product-path tests through executor search with model stages enabled.
15. Redaction tests for provider credentials and prompt/document diagnostics.

Live model and provider tests should remain opt-in. Ordinary CI should use fake
providers and small deterministic fixtures.

## V1 Minimum

The V1 intelligence minimum is:

1. No storage dependency.
2. Minimal default feature set.
3. Explicit target feature names with no long-lived `embed` compatibility
   alias.
4. Strata-shaped model APIs instead of broad inference re-exports.
5. Query embedding helper.
6. Autoembedding queue, status, flush, cleanup, and reindex paths.
7. Query expansion stage with branch-local cache.
8. Reranking stage with graceful fallback.
9. RAG answer stage with prompt construction and citations.
10. Generation lifecycle for local and cloud providers.
11. Structured stage diagnostics.
12. Fake provider/testkit support.

## Open Questions And Closed Ownership

The open questions below should be closed before intelligence-next
implementation plans:

1. Which stage diagnostics become part of public search stats versus internal
   trace/log context?
2. What is the first Autosearch substrate we preserve without shipping the full
   optimizer?
3. What post-V1 endpoint capability schema should external on-prem model
   runtimes expose to intelligence stages?

Closed ownership:

1. The exact `StageOutcome` shape shared by expansion, rerank, RAG, and future
   Autosearch is owned by the engine retrieval and derived-state work tracked
   by `V1Q-004` before intelligence-next consumes it.

## Implementation Stance

Intelligence-next should stay small. Its value is not in owning another large
runtime; its value is in cleanly orchestrating model-dependent Strata behavior.

The implementation should:

1. Keep engine as the database authority.
2. Keep inference as the model execution authority.
3. Use repeatable stage and derived-state patterns.
4. Prefer structured outcomes over ad hoc booleans and `Option` semantics.
5. Preserve graceful degradation while making degradation visible.
6. Avoid creating a new one-off vocabulary for every intelligence feature.

If provider HTTP, llama.cpp pointers, tokenizer internals, model artifact
verification, provider JSON, or endpoint compatibility logic appear in
intelligence-next, the design is drifting downward into inference-next.
