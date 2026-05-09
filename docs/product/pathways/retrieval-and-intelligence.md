# Retrieval And Intelligence Pathways

Status: Draft pathway group

This document expands the V1 pathways for search, retrieval, recipes,
auto-embedding, model management, and intentional generation utilities.

## Pathway 16: Run Keyword Search

### Goal

A user searches stored text with BM25-style ranking across supported data
capabilities.

### Flow

1. Store text-bearing KV, JSON, event, graph, or other supported records.
2. Ensure indexing is enabled or current.
3. Run a keyword search.
4. Receive ranked hits with snippets and entity references.
5. Inspect or open the source records.

### Surface

Search API, CLI search command, index configuration, entity references, branch
and space filters, search stats.

### Guarantees

Keyword search must use clear source coverage, return traceable entity
references, respect branch and space context, and degrade honestly when indexes
are missing, stale, or rebuilding.

### Failures

Missing index, stale index, unsupported source type, invalid query, timeout,
budget exhaustion, and read errors should surface clearly.

### V1 Decision

Required.

### Cleanup

Keep keyword search as a core retrieval mode. Define exact source coverage and
temporal correctness before making strong historical search claims.

## Pathway 17: Run Semantic Or Hybrid Search

### Goal

A user searches by meaning using stored embeddings, optional query embeddings,
and fused keyword/vector results.

### Flow

1. Store vectors directly or enable embedding generation.
2. Select semantic or hybrid retrieval.
3. Provide a query.
4. Strata obtains or receives a query embedding where required.
5. Strata searches vector and keyword sources as configured.
6. The user receives fused ranked results.

### Surface

Search API, vector collections, embedding configuration, search recipes, model
configuration, query embedding input, search stats.

### Guarantees

Semantic and hybrid search must explain which stages ran, which models or
vectors were used, how results were fused, and what happens when embeddings are
unavailable.

### Failures

Missing embedding model, missing vector collection, dimension mismatch, model
runtime error, stale index, unsupported backend, and budget exhaustion should
surface clearly.

### V1 Decision

Required.

### Cleanup

Keep hybrid retrieval as part of the integrated product. Keep model-dependent
quality features honest and optional where model runtime is unavailable.

## Pathway 18: Run Graph-Aware Retrieval

### Goal

A user expands, boosts, or explains search results through relationship-layer
graph context when graph data is present.

### Flow

1. Store or derive graph relationships.
2. Run a retrieval query with graph-aware mode or recipe.
3. Strata retrieves initial keyword or vector hits.
4. Strata expands or scores hits through graph neighborhoods.
5. The user receives results with relationship explanation where available.

### Surface

Search recipes, graph relationship layer, entity references, graph traversal,
search output, explain/provenance fields.

### Guarantees

Graph-aware retrieval must not require duplicating payloads into graph nodes,
must respect branch, space, and temporal context, and must clearly indicate when
graph context was used.

### Failures

Missing graph, dangling entity reference, unsupported traversal mode, stale
relationship index, timeout, and graph context unavailable should surface
without breaking ordinary search.

### V1 Decision

Required.

### Cleanup

Tie graph-aware retrieval to the relationship-layer contract. Do not let hidden
derived graph state silently change search results without observability.

## Pathway 19: Use Search Recipes

### Goal

A user selects or configures named retrieval recipes such as keyword, semantic,
hybrid, graph, default, or RAG.

### Flow

1. Choose a built-in, branch-local, or inline recipe.
2. Run search with that recipe.
3. Strata resolves recipe defaults and overrides.
4. Strata runs the configured retrieval stages.
5. The user sees results and stats that explain the recipe behavior.

### Surface

Recipe config, search API, CLI recipe commands, branch-local config, model
routing, retrieval stats.

### Guarantees

Recipes must be understandable, versionable where branch-local, and explicit
about stages, model requirements, temporal behavior, and fallback behavior.

### Failures

Invalid recipe, missing model, unsupported stage, recursive or incompatible
config, stale branch-local recipe state, and budget exhaustion should surface
clearly.

### V1 Decision

Required.

### Cleanup

Keep recipes for retrieval configuration, but avoid exposing implementation-only
recipe knobs as public product requirements.

## Pathway 20: Use Query Expansion And Reranking

### Goal

A user improves retrieval quality through configured expansion, HyDE-style
variants, fusion, and reranking.

### Flow

1. Select a recipe that enables expansion or reranking.
2. Provide a query.
3. Strata generates or receives expanded query variants where configured.
4. Strata retrieves and fuses candidates.
5. Strata reranks candidates where configured.
6. The user sees final results and stats.

### Surface

Search recipes, model configuration, expansion settings, rerank settings,
search stats, result provenance.

### Guarantees

Expansion and reranking must be explicit, observable in stats, bounded by
budgets, and honest when model runtime is missing.

### Failures

Model load failure, provider failure, unsupported reranker, timeout, invalid
recipe, and budget exhaustion should degrade or fail according to the recipe.

### V1 Decision

Optional.

### Cleanup

Keep expansion and reranking as quality features layered on retrieval. Do not
make them required for basic search.

## Pathway 21: Ask Retrieval-Backed Questions

### Goal

A user receives search results plus an optional generated answer grounded in
retrieved context.

### Flow

1. Select a RAG-capable recipe.
2. Ask a question.
3. Strata retrieves relevant context.
4. Strata reranks or filters context where configured.
5. Strata calls the configured generation runtime.
6. The user receives hits, answer, and provenance where available.

### Surface

Search API, recipe prompt config, model config, generation runtime, result
provenance, token and latency stats.

### Guarantees

Generated answers must be grounded in retrieved context, must expose when
generation was skipped or failed, and must not hide retrieval results when the
model layer fails.

### Failures

No model configured, model load failure, provider error, prompt error, token
limit, empty retrieval context, timeout, and budget exhaustion should surface
or degrade predictably.

### V1 Decision

Optional.

### Cleanup

Keep RAG as an intelligence utility layered on retrieval. Make answer generation
optional and visible, not a replacement for search results.

## Pathway 22: Configure Auto-Embedding And Indexing

### Goal

A user explicitly enables, observes, repairs, or reindexes branch-local shadow
embeddings stored in system space.

### Flow

1. Configure embedding model and source coverage.
2. Enable indexing for selected branches or spaces.
3. Write or import source records.
4. Strata creates or updates shadow embeddings.
5. The user inspects index status or triggers repair/reindex where needed.

### Surface

Embedding config, search recipes, system-space shadow vectors, indexing status,
repair/reindex commands, model configuration.

### Guarantees

Auto-embedding must be explicit, branch-local, observable, repairable, and
separate from user-managed vector collections.

### Failures

Missing model, model runtime failure, unsupported source type, stale index,
partial reindex, system-space corruption, and backend limitation should surface
clearly.

### V1 Decision

Optional.

### Cleanup

Keep auto-embedding separate from standalone vector storage. Do not force users
with precomputed embeddings into Strata's auto-embedding workflow.

## Pathway 23: Manage Models And Inference Configuration

### Goal

A user lists, pulls, configures, and inspects local or provider-backed model
runtime support.

### Flow

1. Inspect available model providers or local models.
2. Pull or configure a model where supported.
3. Set model usage for embedding, reranking, or generation.
4. Run retrieval or generation workflows.
5. Inspect model status and errors.

### Surface

Model commands, provider config, credentials, local model cache, recipe config,
health/info output.

### Guarantees

Model management must avoid leaking secrets, make provider behavior explicit,
and fail clearly when model support is not compiled or configured.

### Failures

Missing provider, missing credentials, model not found, download failure,
incompatible model, unavailable runtime, and network failure should surface
clearly.

### V1 Decision

Optional.

### Cleanup

Keep model management only where the compiled product supports it. Align CLI
help and docs with actual model features.

## Pathway 24: Generate, Tokenize, And Detokenize Text

### Goal

A user runs intentional intelligence utilities through a configured inference
runtime.

### Flow

1. Configure or select a model.
2. Run generation, tokenization, or detokenization explicitly.
3. Strata executes through the configured runtime.
4. The user receives text, tokens, stats, or a clear model-layer error.

### Surface

Generation commands, tokenize/detokenize commands, model config, provider
settings, runtime stats.

### Guarantees

These commands must be explicit utilities, must not run as hidden side effects,
and must expose model/runtime errors cleanly.

### Failures

Model missing, invalid prompt, tokenization failure, provider error, timeout,
unsupported compiled feature, and credential failure should surface clearly.

### V1 Decision

Optional.

### Cleanup

Keep generation, tokenization, and detokenization because they are intentional
inference-layer calls. Do not present them as mandatory database primitives.
