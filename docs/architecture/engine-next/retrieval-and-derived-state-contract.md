# Engine-Next Retrieval And Derived-State Contract

Status: V1 architecture draft

## Purpose

This document defines the engine-next contract for retrieval, recipes, search
indexes, shadow vectors, graph-aware retrieval, derived-state health, rebuild
behavior, and provenance.

Strata's source of truth is branch-aware MVCC KV rows. Retrieval is the engine
layer that discovers, ranks, expands, explains, or grounds those rows through
capability adapters and derived state.

The target flow is:

```text
product retrieval request
  -> resolve branch, space, temporal context, and recipe
  -> validate source coverage and derived-state compatibility
  -> execute engine-owned deterministic retrieval stages
  -> allow upper intelligence/inference layers to run model stages around it
  -> enforce source-row constraints before returning results
  -> return hits, provenance, stage stats, and derived-state diagnostics
```

Retrieval must not become a second storage engine. It consumes source rows,
capability adapters, relationship facts, recipes, and rebuildable indexes. It
does not own user-authored data.

This document is not the full future query-language design. V1 may expose
simple search, graph-aware search, recipe-driven retrieval, and optional RAG.
Structured "find records related to X where Y" work should build on this
contract later, not bypass it.

The dependency boundary matters. Engine-next owns deterministic retrieval,
capability projection, recipe records, derived-state manifests, and source
validation. Intelligence/inference may wrap engine retrieval to generate
embeddings, expansions, reranker scores, or answers. Engine-next must not import
or call upward into intelligence/inference.

## Related Documents

Read this with:

1. `docs/product/strata-v1-product-requirements.md`
2. `docs/product/strata-v1-feature-inventory.md`
3. `docs/product/pathways/retrieval-and-intelligence.md`
4. `docs/product/strata-v1-graph-relationship-layer.md`
5. `docs/product/strata-v1-versioning-time-travel.md`
6. `docs/architecture/engine-next-architecture.md`
7. `docs/architecture/engine-next/README.md`
8. `docs/architecture/engine-next/primitive-implementation-contract.md`
9. `docs/architecture/engine-next/entity-ref-and-relationship-layer-contract.md`
10. `docs/architecture/engine-next/storage-space-id-registry.md`
11. `docs/architecture/engine-next/persistence-adapter-contract.md`
12. `docs/architecture/engine-next/branch-operation-and-capability-adapter-contract.md`
13. `docs/architecture/engine-next/temporal-context-and-timeline-resolver-contract.md`
14. `docs/architecture/engine-next/control-plane-layout-contract.md`
15. `docs/architecture/v1-error-and-diagnostics-contract.md`

Follow-up contracts that depend on this one:

1. IPC and serializable command-boundary contract.
2. Dataset clone artifact contract.
3. Public API and CLI surface cleanup checklist.
4. Product-pathway conformance plan.

## Requirement Language

1. Must means retrieval correctness or derived-state safety is incomplete
   without it.
2. Should means expected unless a later architecture decision records a clear
   deferral.
3. May means allowed but not required for V1.

## Current Code Evidence

The current codebase already has a sophisticated retrieval system, but the
ownership is not yet a clean contract.

Useful current mechanisms:

1. `crates/engine/src/search/substrate.rs` is a deterministic retrieval
   substrate that executes resolved recipes over BM25 and shadow-vector stages.
2. `crates/engine/src/search/recipe.rs` models retrieval recipes with BM25,
   vector, graph, expansion, fusion, rerank, transform, prompt, model, version
   output, and control sections.
3. `crates/engine/src/search/index.rs` implements segmented BM25-style indexes
   with document maps, postings, positions, tombstones, sealed segments,
   watermarks, and branch/space filtering.
4. `crates/engine/src/search/manifest.rs` persists search index manifests.
5. `crates/engine/src/search/searchable.rs` exposes a shared search adapter
   shape for capability-backed candidates and scores.
6. `crates/engine/src/vector/store/system.rs` stores autoembedding shadow
   vectors in branch-local `_system_` space and preserves source references for
   hybrid retrieval.
7. `crates/intelligence/src/expand.rs` performs query expansion through the
   intelligence layer and falls back when expansion fails.
8. `crates/intelligence/src/rerank.rs` reranks results and blends reranker
   scores with retrieval scores.
9. `crates/intelligence/src/rag/mod.rs` treats RAG as recipe-gated generation
   over retrieved context, not as a replacement for search hits.
10. `crates/intelligence/src/expand_cache.rs` stores query-expansion cache rows
    in branch-local `_system_` space.
11. Current retrieval already carries `EntityRef`, branch, space, optional
    time ranges, snapshot versions, and stage statistics.

Current weaknesses to correct in engine-next:

1. Search request fields mix product scope, retrieval knobs, temporal filters,
   and implementation parameters in one shape.
2. Some temporal filtering is post-filtered from source rows because indexes do
   not yet have a uniform temporal contract.
3. Derived-state manifests and health records are split across local files,
   system-space rows, and in-memory state.
4. Shadow vectors, search manifests, graph reverse maps, query expansion cache,
   and projection state do not share one derived-state lifecycle model.
5. Some model-dependent intelligence failures degrade by convention rather than
   through a shared recipe policy.
6. Result provenance exists but is not yet strong enough to explain every
   branch, time, recipe, index, model, and relationship decision.

The target keeps the good part:

```text
recipes + capability adapters + source-verified results
```

and removes the weak part:

```text
each retrieval feature invents its own state, freshness, and failure rules
```

## Definitions

### Retrieval

Retrieval is the engine-owned process of discovering and ranking source records
or relationship facts.

Retrieval includes:

1. Keyword search.
2. Vector similarity search.
3. Hybrid search.
4. Graph-aware retrieval.
5. Engine-side context assembly for upper-layer RAG.
6. Stage stats and provenance for model-dependent wrappers.
7. Recipe interpretation for deterministic stages.
8. Result explanation and provenance.

Retrieval does not include:

1. Direct key lookup.
2. Durable source-row writes.
3. Branch promotion semantics.
4. Storage checkpoint, WAL, compaction, or recovery mechanics.
5. Inference provider implementation.
6. Public transaction sessions.
7. Model execution for expansion, reranking, embedding, or generation.

Direct key lookup may share temporal context, EntityRef, provenance, and output
shapes with retrieval, but it is not a ranked retrieval operation.

### Search

Search is ranked retrieval from a text, vector, graph, hybrid, or recipe-defined
signal.

Search answers:

```text
which records are relevant to this text, vector, or relationship seed?
```

Search is an operator inside retrieval. It is not the whole future query model.

### Derived State

Derived state is rebuildable engine state created from source rows or
control-plane rows to accelerate, improve, explain, or cache retrieval.

Examples:

1. BM25/text index rows.
2. Search segment manifests.
3. Autoembedding shadow vectors.
4. ANN/vector index acceleration rows.
5. Graph reverse maps and traversal accelerators.
6. Retrieval projections.
7. Query expansion caches.
8. Prompt or context caches.
9. Watermarks, rebuild status, and health records.

Derived state must not be the only copy of user-authored data.

### Recipe

A recipe is an engine control-plane record that specifies how retrieval should
run.

Recipes may choose:

1. Source coverage.
2. BM25 parameters.
3. Vector collections or embedding model requirements.
4. Graph traversal behavior.
5. Expansion strategy for upper-layer execution.
6. Fusion method.
7. Reranking policy for upper-layer execution.
8. Transform limits and deduplication.
9. RAG prompt and context budget.
10. Model routing for upper layers.
11. Budget and degradation policy.

A recipe must not override hard request constraints such as branch, space,
temporal context, access mode, user filter, or explicit source scope.

### Source Coverage

Source coverage describes which authored rows a retrieval run is allowed to
consider.

It includes:

1. Branch.
2. Space or spaces.
3. Data capabilities.
4. Relationship scope.
5. Temporal context.
6. Access-mode and system-data policy.
7. Optional source predicates supplied by the product request.

Source coverage is part of correctness, not ranking. A recipe may narrow
coverage if the user selected that recipe knowingly. A recipe must not silently
broaden hard request coverage.

### Candidate

A candidate is an intermediate retrieval result before final validation,
fusion, reranking, or limit application.

Candidates may come from:

1. BM25 index hits.
2. Full-scan text projection hits.
3. User vector collection hits.
4. Autoembedding shadow-vector hits.
5. Graph traversal hits.
6. Relationship reverse-map hits.
7. Expansion fan-out hits.

Candidates are not user-visible results until final source validation and
provenance attachment complete.

### Hit

A hit is a user-visible retrieval result.

Every hit must carry at least:

1. `EntityRef`.
2. Rank or deterministic order.
3. Score or explicit unscored classification.
4. Source capability.
5. Branch and space context.
6. Observed version and timestamp when available.
7. Provenance explaining which stages contributed.

### Watermark

A watermark records the source frontier a derived-state family has processed.

Conceptually:

```text
derived family + branch + space + source coverage + recipe/index config
  -> processed commit version/timestamp + health + rebuild metadata
```

A watermark does not prove correctness by itself. It is valid only with the
derived-state manifest, source coverage, index configuration, and health state
that produced it.

### Freshness

Freshness describes whether derived state can satisfy a retrieval request
without leaking, missing, or misranking results.

Freshness classes:

1. Fresh.
2. Stale but source-filterable.
3. Stale and rebuild-required.
4. Unavailable.
5. Corrupt.

The exact Rust enum may differ. The classes must exist in diagnostics and
tests.

### Provenance

Provenance is the explanation record attached to retrieval output.

It should include:

1. Resolved recipe identity and version/hash.
2. Branch and temporal context.
3. Source coverage.
4. Derived-state families used.
5. Watermark or manifest facts where relevant.
6. Model identifiers for embedding, expansion, reranking, or generation.
7. Relationship paths where graph context affected a result.
8. Whether source validation or post-filtering removed candidates.
9. Degradation or budget exhaustion facts.

Provenance must be machine-readable enough for tests and Strata AI. Display text
may summarize it.

## Binding Decisions

1. **Source rows are authoritative.**
   Derived state may accelerate or improve retrieval, but final results must be
   valid under the selected branch, space, temporal context, access mode, and
   source coverage.

2. **Recipes choose strategy, not truth.**
   Recipes may decide which retrieval stages run and how candidates are ranked.
   They must not bypass hard constraints from the request.

3. **Retrieval consumes capability adapters.**
   Retrieval must not decode capability value bytes by hand. KV, JSON, event,
   vector, and graph expose search/projection/reference adapters that retrieval
   consumes.

4. **Derived state is owned by engine, not storage.**
   Storage stores rows. Engine owns search indexes, shadow vectors, graph
   reverse maps, projection manifests, watermarks, rebuild jobs, and retrieval
   health semantics.

5. **Control-plane rows describe derived state.**
   Recipes use `0x33`. Derived rows use `0x40..=0x45` according to the
   storage-space ID registry. Derived-state health and manifests use `0x45`.

6. **Temporal context is mandatory.**
   Retrieval must run under an explicit temporal context. `current`, `as_of`,
   `version`, and range behavior come from the temporal context contract.

7. **Historical retrieval must be honest.**
   If a derived index cannot satisfy a historical request exactly, retrieval
   must source-filter, rebuild, or refuse. It must not return current-index
   results as if they were historical.

8. **Graph-aware retrieval uses relationship facts.**
   Graph context comes from graph source rows and graph derived indexes. It must
   not require duplicating KV/JSON/event/vector payloads into graph nodes.

9. **Autoembedding is separate from user vectors.**
   User-managed vector collections are source data. Shadow embeddings are
   derived state. They must have different row families, names, health records,
   branch behavior, and diagnostics.

10. **Model-dependent stages are optional and observable.**
    Expansion, reranking, embedding, and generation may be disabled,
    unavailable, or budget-limited. The recipe must define whether upper layers
    skip, degrade, or fail. Engine-next records the recipe and retrieval
    provenance but does not execute provider calls.

11. **RAG does not replace retrieval.**
    RAG is answer generation over retrieved context. The retrieval hits and
    provenance remain visible even when generation succeeds.

12. **Budget exhaustion is not success without qualification.**
    Retrieval may return partial results under a budget, but output must mark
    truncation, skipped stages, and budget exhaustion.

13. **System rows are excluded by default.**
    Normal retrieval must not index or return `_system_` branch rows or
    branch-local `_system_` space rows. Diagnostic commands may use explicit
    control-plane retrieval paths.

14. **Query language is not part of this contract.**
    This contract defines the engine substrate required for search and future
    constrained discovery. It does not freeze CLI syntax, an expert filter
    grammar, SQL, Cypher, or a general query DSL.

## Retrieval Shape

The conceptual retrieval request contains:

```text
RetrievalRequest {
    branch,
    temporal_selector,
    spaces,
    source_scope,
    query_or_seed,
    recipe_selector,
    hard_filters,
    output_shape,
    budget,
    consistency_policy,
}
```

This is conceptual shape, not a required Rust type.

### Branch And Temporal Context

Retrieval must resolve branch and temporal context before executing stages.

Point retrieval reads use one resolved frontier. Range retrieval and compare
features may use multiple resolved frontiers, but they must name them
explicitly in provenance.

After resolution, every stage receives one of:

1. Current read view.
2. Version-bounded read view.
3. Timestamp-resolved read view.
4. Range context with explicit start and end frontiers.

Stages that cannot support the requested context must report that fact before
or during planning.

### Source Scope

V1 source scopes:

1. KV records.
2. JSON documents.
3. Events.
4. User vector records.
5. Graph records and relationships.

Control-plane records are excluded from normal source scopes.

Derived retrieval inputs, such as text indexes, retrieval projections, graph
reverse maps, vector indexes, and autoembedding shadow vectors, are selected by
recipe and freshness policy. They are not source scope.

### Retrieval Seed

The retrieval seed may be:

1. Text query.
2. Vector supplied by the caller.
3. EntityRef for relationship-aware retrieval.
4. Recipe-generated expansion variants.
5. RAG question text.

V1 should keep the public surface simple. Advanced seeds may be exposed through
recipe or SDK APIs before CLI syntax exists.

### Hard Filters

Hard filters are constraints. They are not ranking hints.

Examples:

1. Branch.
2. Space.
3. Temporal selector.
4. Capability/source scope.
5. Access policy.
6. User-provided field or tag filters supported by a capability adapter.
7. Relationship constraints supported by graph adapters.

If a stage cannot apply a hard filter internally, the retrieval planner may
allow it only if final source validation can enforce it without violating the
budget and result guarantees selected by the recipe.

### Output Shape

V1 output should support:

1. Entity refs.
2. Snippets or summaries where available.
3. Scores.
4. Rank.
5. Observed version/timestamp where available.
6. Stage stats.
7. Provenance.
8. Optional RAG answer and citations.

Exact key/value reads and history commands may share some output fields, but
they should not be routed through ranked retrieval just to get provenance.

## Recipe Contract

### Recipe Resolution

Recipe resolution order comes from the control-plane contract:

1. Branch-local recipe override.
2. Global built-in or shared recipe.
3. In-memory emergency default only when create-new or repair policy permits it.

Retrieval provenance must record:

1. Requested recipe name or inline recipe marker.
2. Resolved source: branch-local, global, inline, or emergency default.
3. Monotonic recipe registry version.
4. Recipe content hash.
5. Validation result.

Built-in recipe registry versions must be immutable once persisted in a V1
database. Changing a built-in recipe creates a new registry version and a new
recipe content hash.

### Recipe Sections

V1 recipes may contain these conceptual sections:

| Section | Purpose |
|---|---|
| Source | Branch/space/source coverage defaults where allowed. |
| Text retrieval | BM25/tokenization/stemming/phrase/proximity behavior. |
| Vector retrieval | User vector or shadow-vector collection behavior. |
| Graph retrieval | Relationship expansion, traversal, and graph scoring behavior. |
| Expansion | Query expansion and HyDE-like policy for upper layers. |
| Fusion | Candidate fusion such as RRF or weighted combination. |
| Rerank | Cross-encoder or model-based reranking policy for upper layers. |
| Transform | Deduplication, limit, grouping, and snippet behavior. |
| RAG | Prompt template, context budget, citation behavior for upper layers. |
| Models | Embedding, expansion, rerank, and generation model routing for upper layers. |
| Control | Budget, degradation, freshness, and failure policy. |

The exact schema may differ, but these responsibilities should not be mixed
into one opaque JSON bag.

### Recipe Safety Rules

1. A recipe must validate before execution.
2. Unknown recipe fields must either fail validation or be explicitly ignored
   under a forward-compatibility rule.
3. A recipe must not enable system-row retrieval through normal product search.
4. A recipe must not widen user-supplied branch, space, temporal, or access
   constraints.
5. A recipe that requires model runtime must declare fallback behavior.
6. A recipe that uses derived state must declare freshness policy.
7. A recipe that requests historical retrieval must declare whether source
   filtering is acceptable or exact derived-state compatibility is required.
8. A recipe that emits generated text must keep retrieval hits visible.

### Built-In Recipes

V1 should define a small set of built-in recipes. The names may change, but the
concepts should be stable:

1. Keyword retrieval.
2. Semantic retrieval.
3. Hybrid retrieval.
4. Graph-aware retrieval.
5. RAG-capable retrieval.

Raw recipe editing may remain advanced configuration. Product APIs and CLI
should prefer understandable presets.

## Stage Contract

Every retrieval stage must declare:

1. Inputs.
2. Source coverage it can honor internally.
3. Temporal compatibility.
4. Derived-state families it reads or writes.
5. Model requirements.
6. Failure behavior.
7. Stats it emits.
8. Provenance it contributes.

### Text Stage

The text stage owns keyword retrieval over text projections.

It may use:

1. BM25 or BM25-like scoring.
2. Tokenization.
3. Stemming.
4. Stopwords.
5. Phrase matching.
6. Proximity scoring.
7. Field boosts where the capability adapter exposes fields.

Text stage requirements:

1. It must name which capabilities and spaces contributed text.
2. It must return `EntityRef` provenance for every hit.
3. It must handle missing or stale indexes according to recipe policy.
4. It must not index system rows by default.
5. It must make full-scan fallback explicit if used.
6. It must source-validate hits when temporal or source filters cannot be
   proven by the index.

### Vector Stage

The vector stage owns vector similarity retrieval.

It may search:

1. User-managed vector collections.
2. Autoembedding shadow-vector collections.
3. Future ANN indexes.

Vector stage requirements:

1. It must validate embedding dimension and distance metric compatibility.
2. It must distinguish user vector source data from shadow-vector derived
   state.
3. It must return the source `EntityRef` for shadow-vector hits.
4. It must drop or diagnose orphan shadow vectors according to recipe policy.
5. It must enforce branch, space, and temporal constraints through source
   validation when the vector index cannot prove them internally.
6. It must record model identity when the query embedding was generated by
   an upper Strata layer and supplied to engine retrieval.
7. For shadow-vector search, it must compare the recipe-selected embedding
   model spec hash, embedding dimension, and distance metric against the
   shadow-vector manifest before calling an embedding model.
8. If the recipe-selected embedding model is incompatible with the manifest,
   the stage must report `failed_precondition.embedding_model_mismatch` and
   classify the shadow-vector family as stale and rebuild-required.
9. V1 vector `as_of` search is source-filtered by default: the vector stage may
   use a current or broader candidate generator only when every returned
   candidate is verified against source rows at the resolved frontier.
10. If the selected vector backend or index can miss historical candidates under
   source filtering, the stage must refuse the historical request with a
   temporal unsupported diagnostic. V1 does not require optimized historical
   vector indexes.

### Graph Stage

The graph stage owns relationship-aware retrieval.

It may:

1. Expand around seed entities.
2. Boost or filter candidates by graph proximity.
3. Return relationship paths.
4. Use reverse maps or traversal accelerators.
5. Interpret graph ontology facts where recipes request it.

Graph stage requirements:

1. It must resolve graph bindings through the EntityRef contract.
2. It must handle dangling, deleted, inaccessible, or history-trimmed targets
   explicitly.
3. It must not copy source payloads into graph nodes to make retrieval work.
4. It must respect branch-local and space-local relationship semantics.
5. It must attach relationship-path provenance when graph context changes rank,
   inclusion, or explanation.

### Expansion Stage

Expansion is model-dependent query rewriting or query generation performed by
the intelligence/inference layers around engine retrieval.

Requirements:

1. Expansion must be recipe-gated.
2. Expansion must not broaden hard source constraints.
3. Expansion variants must be typed enough to route to text, vector, or hybrid
   engine retrieval stages without string conventions.
4. Expansion cache rows must be discardable cache rows.
5. Cache keys must include every input that affects generated variants or the
   recipe must label cache reuse as intentionally approximate.
6. Expansion failure must follow recipe policy: skip, degrade, or fail.
7. Engine-next may persist recipe and cache metadata used by expansion, but it
   must not execute model calls directly.

### Fusion Stage

Fusion combines candidate lists.

Requirements:

1. Fusion must be deterministic for equal inputs.
2. Tie-breaking must be stable and independent of map iteration order.
3. Fusion must preserve per-stage contribution facts in provenance.
4. Fusion must not resurrect candidates removed by hard-filter validation.
5. Fusion must report when candidate lists were missing because stages were
   unavailable, stale, unsupported, or budget-skipped.

### Rerank Stage

Reranking is model-dependent scoring after candidate retrieval. It is executed
above engine retrieval.

Requirements:

1. Reranking must be recipe-gated.
2. Reranking must operate only on candidates already valid under hard source
   constraints.
3. Reranker model identity and version must be recorded.
4. Reranking failure must follow recipe policy.
5. Reranking must not hide original retrieval score contribution unless output
   intentionally omits debug details.
6. Engine-next must provide stable hit/provenance inputs so upper layers can
   rerank without re-reading storage directly.

### RAG Stage

RAG assembles context and optionally generates an answer. Engine-next may
assemble retrieval context and provenance. Intelligence/inference execute
generation.

Requirements:

1. RAG must be recipe-gated.
2. RAG context must be built from retrieval hits with source provenance.
3. Generated answers must include citation/provenance information where the
   output surface supports it.
4. Generation failure must not discard retrieval hits unless the user requested
   answer-only behavior and recipe policy allows failure.
5. Prompt templates and model routing are control-plane or configuration
   records, not hidden runtime state.
6. Engine-next must not call generation providers directly.

## Derived-State Families

Every derived-state family must declare:

1. Name.
2. Storage-space ID.
3. Source rows covered.
4. Branch and space scope.
5. Temporal compatibility.
6. Manifest row or file ownership.
7. Watermark semantics.
8. Rebuild behavior.
9. Clone/export behavior.
10. Branch workflow disposition.
11. Failure and corruption handling.
12. Test coverage.

### V1 Families

| Family | ID | Class | Notes |
|---|---|---|---|
| Text index | `0x40` | Derived | BM25 postings, term dictionaries, text index rows, and search lookup tables. |
| Shadow vectors | `0x41` | Derived | Autoembedding vectors and source-link rows. |
| Vector index | `0x42` | Derived | ANN acceleration over user or shadow vectors. |
| Graph index | `0x43` | Derived | Reverse maps and traversal accelerators. |
| Projections | `0x44` | Derived/cache | Retrieval projections, snippets, projected source payloads, expansion entries, prompt/context caches, and other discardable retrieval intermediates. |
| Health/manifests | `0x45` | Derived metadata | Watermarks, manifests, rebuild state, health records. |

The current implementation may still use sidecar files for search indexes or
vector acceleration. Engine-next should treat those sidecars as implementation
details behind the same derived-state manifest and health contract. A retrieval
run should not need to know whether a derived family is row-backed, file-backed,
or in-memory.

### Freshness Classes

Fresh means:

1. The derived family covers the requested source rows.
2. The watermark reaches the requested temporal frontier.
3. The configuration hash matches the recipe/index config.
4. The family is healthy.

Stale but source-filterable means:

1. The family may over-return candidates.
2. Final source validation can remove invalid hits.
3. Missing valid hits are not possible under the family contract.

Stale and rebuild-required means:

1. The family may miss valid hits or misrepresent rank.
2. Its model-dependent configuration may be incompatible with the requested
   recipe, such as an embedding model spec hash, dimension, or metric mismatch.
3. Retrieval must rebuild, fall back to a safe source scan if allowed, or refuse.

Unavailable means:

1. The family is absent, disabled, omitted from clone/import, or unsupported by
   the runtime mode.

Corrupt means:

1. The family failed validation.
2. Retrieval must not use it for normal results until repair or rebuild.

### Manifest And Watermark Rows

Derived-state manifests and watermarks live in branch-local `_system_` space
under `0x45` unless a global family is explicitly declared.

Minimum facts:

1. Derived family.
2. Manifest schema version.
3. Source coverage.
4. Source branch and spaces.
5. Source commit version frontier.
6. Source commit timestamp frontier.
7. Source capability registry version.
8. Recipe or index configuration hash where relevant.
9. Model spec hash where the derived family depends on model output.
10. Embedding dimension and distance metric where the family stores vectors.
11. Build status.
12. Last successful validation.
13. Last failure code and redacted message, if any.
14. Whether rows/files are safe to omit from clone/export.

### Branch Behavior

Branch workflows must assign each derived family one disposition:

1. Preserve after validation.
2. Mark stale.
3. Drop.
4. Rebuild synchronously.
5. Schedule rebuild.
6. Refuse operation.

Defaults:

1. Branch create from current/history should copy source rows and mark derived
   families stale or absent unless the family proves the inherited state is
   valid under the new branch identity.
2. Promotion should mark affected branch-local derived families stale unless a
   family can update transactionally with source mutations.
3. Selected copy and restore should mark affected derived families stale.
4. Branch delete should drop branch-local derived rows, sidecars, and health
   rows for that branch generation.
5. Clone/export should omit local-only derived state unless the artifact
   contract explicitly includes and validates it.

## Temporal Compatibility

Every retrieval stage and derived family must declare one temporal class:

1. Exact.
2. Source-filtered.
3. Current-only.
4. Unsupported.

### Exact

Exact means the stage can answer under the selected temporal context without
returning candidates that depend on newer source state or missing retained
source state.

### Source-Filtered

Source-filtered means the stage may use a broader candidate generator, then
validate every candidate against source rows at the resolved frontier.

Source-filtered is allowed only when the candidate generator cannot miss valid
hits required by the recipe's guarantee. If it can miss hits, the class is
current-only or unsupported for historical retrieval.

### Current-Only

Current-only means the stage can support current reads but not historical reads.

When a current-only stage appears in a historical retrieval request, the recipe
must choose:

1. Skip the stage.
2. Fail with a temporal unsupported diagnostic.
3. Rebuild a compatible view if a rebuild path exists.

### Unsupported

Unsupported means the stage cannot safely participate in the requested temporal
mode.

Retrieval must not silently use unsupported stages.

### Time Basis

Temporal retrieval must distinguish:

1. Commit time.
2. Event-domain time.
3. Field-derived time.
4. Model or index build time.

`as_of` resolves through the commit timeline. Event-time or field-time filters
are capability predicates and must be named as such by the API layer that
eventually exposes them. Retrieval must not collapse these into one generic
time field.

## Autoembedding Contract

Autoembedding is optional derived state.

Requirements:

1. Autoembedding policy is control-plane state.
2. Shadow vectors live in branch-local system space.
3. Shadow vector rows must carry source `EntityRef`.
4. Shadow vector rows must record embedding model identity and dimensionality.
5. Shadow vector manifests must record the embedding model spec hash,
   embedding dimension, distance metric, source coverage, and commit frontier.
6. A query-time model mismatch is stale and rebuild-required, not a best-effort
   degradation. The vector stage detects it during retrieval planning and
   surfaces `failed_precondition.embedding_model_mismatch`.
7. Shadow vectors must be separable from user vector collections in API output,
   diagnostics, branch workflows, and clone/export.
8. Missing shadow vectors must not corrupt source data.
9. Failed embedding work must mark derived-state health and follow recipe
   fallback policy.
10. Users with precomputed embeddings must be able to use user vector
   collections without enabling autoembedding.

Autoembedding should normally be asynchronous or backgroundable, but V1 may
support synchronous rebuild paths for tests, repair, and deterministic demos.

## Graph-Aware Retrieval Contract

Graph-aware retrieval uses the relationship layer.

Allowed uses:

1. Expand from retrieved hits to related entities.
2. Expand from a seed entity to related entities.
3. Boost hits that have graph proximity to a seed.
4. Provide relationship paths as explanation.
5. Use ontology facts as recipe-controlled retrieval context.

Rules:

1. Relationship facts are graph source rows.
2. Reverse maps and traversal indexes are derived state.
3. Traversal resolves bindings through the EntityRef contract.
4. Branch-local and space-local semantics are the default.
5. Dangling references must be reported or filtered according to explicit
   policy.
6. Graph-derived inclusion or ranking must be visible in provenance.

## Query Expansion Cache And Prompt Cache

Expansion caches and prompt/context caches are cache rows, not source rows.

Rules:

1. They live in branch-local `_system_` space.
2. They use derived/cache storage-space assignment, not user KV rows.
3. They must be safe to delete.
4. They must be omitted from normal clone artifacts.
5. They must not affect correctness if absent.
6. Cache keys must include all correctness-relevant inputs or the cache must be
   explicitly approximate.
7. Cache corruption must degrade to a miss unless the corruption indicates a
   wider control-plane problem.

The owner of model-output cache reads and writes may be an intelligence-layer
service, but the rows still live under engine control-plane rules and must use
the persistence adapter rather than raw system-space writes.

## Failure And Degradation

Retrieval failures should map to stable diagnostics through the V1 error
contract.

Expected failure classes:

| Failure | Expected behavior |
|---|---|
| Invalid recipe | Fail before retrieval starts. |
| Missing recipe | Fail or use documented default according to create/repair policy. |
| Unsupported stage | Fail or skip according to recipe policy. |
| Missing model | Fail or skip model-dependent stage according to recipe policy. |
| Model execution failure | Fail or degrade according to recipe policy. |
| Missing derived state | Rebuild, safe fallback, skip, or fail according to freshness policy. |
| Stale derived state | Source-filter, rebuild, skip, or fail according to temporal/freshness policy. |
| Corrupt derived state | Do not use; mark health and require repair or rebuild. |
| Budget exhausted | Return partial results only with explicit budget/truncation facts. |
| Source validation error | Drop candidate only if dropping is safe; otherwise fail. |
| Dangling relationship | Report, filter, or fail according to graph policy. |
| Historical unsupported | Fail with temporal unsupported diagnostic unless recipe selected skip behavior. |

Display messages are not the compatibility surface. Error class, code,
retryability, and structured context are.

## Provenance And Stats

Retrieval output must make enough facts visible for users, tests, and Strata AI
to understand what happened.

Minimum stats:

1. Resolved recipe identity.
2. Resolved branch and temporal context.
3. Stage list.
4. Per-stage elapsed time.
5. Per-stage candidate counts.
6. Fusion/rerank counts.
7. Derived families used.
8. Freshness class per family where relevant.
9. Budget exhaustion and truncation.
10. Model stages attempted, skipped, or failed.

Minimum hit provenance:

1. EntityRef.
2. Source capability.
3. Branch and space.
4. Observed version/timestamp where available.
5. Stage contributions.
6. Score contributions or final score.
7. Snippet/source projection identity where available.
8. Relationship path when graph context affected the hit.
9. Source validation status.

Generated answers must carry:

1. Retrieval context hit references.
2. Prompt template identity or hash.
3. Generation model identity.
4. Token/context budget facts.
5. Whether the answer was generated, skipped, or failed.

## Runtime Resource Policy

Retrieval must respect the runtime resource profile.

Low-resource devices may:

1. Use smaller candidate budgets.
2. Disable expensive derived-state builders by default.
3. Prefer scan or compact indexes over ANN indexes for small datasets.
4. Skip model-dependent stages unless explicitly enabled.
5. Run background rebuilds opportunistically.

High-resource devices may:

1. Build larger indexes.
2. Run more retrieval stages in parallel.
3. Enable richer default recipes.
4. Use larger candidate and rerank windows.

The same database must remain readable across profiles. Runtime resource policy
may affect performance and optional quality stages, not source-row correctness.

## Access, IPC, And Command Boundary Requirements

Retrieval commands crossing IPC or CLI boundaries must serialize:

1. Request scope.
2. Recipe selector.
3. Temporal selector.
4. Output shape.
5. Budget.
6. Structured error status.
7. Result provenance and stats.

IPC must not expose raw storage keys, raw storage-space IDs, search sidecar
paths, or internal manifest file names as normal product fields.

Read-only handles may run retrieval. They must not run rebuilds or write
derived-state health unless the command explicitly has maintenance authority.
If retrieval under a read-only handle needs a rebuild to satisfy correctness, it
must fail or use a safe read-only fallback.

## Clone, Import, And StrataHub

Clone/import behavior:

1. Source rows are authoritative.
2. Derived rows are optional unless the artifact contract explicitly includes
   them.
3. Imported derived rows must be validated against source coverage, manifest
   schema, recipe/index config, and watermark facts before use.
4. If derived rows are omitted, matching health rows must be omitted, reset, or
   marked rebuild-required.
5. Recipe provenance in retrieval output must survive clone when the recipe is
   part of the artifact.
6. If imported shadow-vector manifests name a model unavailable on the local
   machine, source rows remain readable. Model-dependent rebuild or retrieval
   reports the missing model or embedding-unavailable diagnostic; engine-next
   must not silently substitute another embedding model for those derived rows.

StrataHub or private hub implementations should be able to report:

1. Whether a dataset has search-ready derived state.
2. Which recipes are included.
3. Whether embeddings must be rebuilt locally.
4. Which model dependencies are required.
5. Whether graph/retrieval projections are present or rebuild-required.

## Conformance Requirements

The product-pathway conformance plan should include:

1. Keyword retrieval over KV, JSON, event, graph-capable text, branch, and space
   scopes.
2. Hybrid retrieval with shadow vectors and source references.
3. Graph-aware retrieval with relationship paths and dangling references.
4. Recipe resolution: branch-local, global, inline, invalid, missing, and
   emergency default cases.
5. Temporal retrieval: current, version, as_of, retained-history miss, and
   unsupported derived state.
6. Source validation: stale index over-return, source-filtered removal, orphan
   shadow vectors, cross-space leakage prevention.
7. Branch workflows: fork, promote, copy, restore, and delete derived-state
   dispositions.
8. Clone/import: derived rows included, omitted, corrupt, stale, and
   rebuild-required.
9. Low-resource profile: budget exhaustion, disabled model stages, smaller
   indexes, and deterministic partial results.
10. Read-only/IPC behavior: retrieval allowed, rebuild refused, structured
    diagnostics preserved.
11. RAG behavior: hits preserved when generation fails, citations/provenance
    attached when generation succeeds.
12. Cache behavior: expansion cache miss/corrupt/delete does not change
    correctness.

Tests must assert on structured provenance, freshness class, and error codes
where possible, not on prose messages.

## Implementation Guidance

1. Keep the concept set small: request, recipe, stage, candidate, hit,
   derived family, manifest, watermark, provenance.
2. Do not introduce one named struct per narrow retrieval variant unless the
   concept repeats across capabilities or tests.
3. Capability adapters should expose projection/search hooks; retrieval should
   not import capability internals.
4. The persistence adapter remains the only normal storage-facing path.
5. Derived-state services should use shared manifest/watermark patterns instead
   of custom health structs per subsystem.
6. Model-dependent logic belongs above deterministic retrieval. Engine may
   coordinate deterministic recipe execution, but provider execution belongs in
   intelligence/inference layers.
7. Prefer explicit skip/degrade/fail policy over hidden fallback.
8. Record enough provenance now so future constrained discovery can reuse this
   substrate without a second architecture.

## Deferred Questions

1. What exact public `find` or constrained discovery surface should V1 expose,
   if any?
2. Should field filters use a typed builder only, a small filter AST, CLI flags,
   an expert filter syntax, or multiple surfaces?
3. Which built-in recipes are required at V1 launch and which are optional?
4. Which derived families are row-backed versus sidecar-backed in
   implementation?
5. Which retrieval stages are allowed under cache/browser mode by default?
6. Historical vector search baseline.
   Closed for V1: source-filtered vector retrieval is sufficient when the
   candidate generator cannot miss valid historical candidates; otherwise the
   stage must refuse the temporal request. Exact temporal vector indexes are not
   required for V1.
7. What quality metrics should AutoResearch use when optimizing recipes?

These questions should be answered before implementing a public constrained
discovery surface. They do not block the retrieval and derived-state substrate
contract.
