# M8 / M8T Implementation Plan: Intelligence-Next Orchestration

Status: draft implementation plan

## Goal

Implement model-assisted Strata behavior over engine-next and inference-next.

## Inputs

1. `docs/architecture/intelligence-architecture.md`
2. `docs/architecture/engine/retrieval-and-derived-state-contract.md`
3. `docs/architecture/engine/primitive-implementation-contract.md`
4. `docs/architecture/inference-architecture.md`
5. `docs/product/strata-autosearch-product-direction.md`

All slices must follow the V1 engineering standards: permanent domain names,
concept-budget discipline, file/function thresholds, comment standards, and no
roadmap labels in production code vocabulary.

## Implementation Track

| Epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M8A` | Engine surface integration | Consume the named engine surfaces for source-row events, shadow vectors, policies, freshness, and branch-local secrets. | Intelligence does not reach around engine into storage. |
| `M8B` | Autoembedding runtime | Implement queue, flush, reindex, cleanup, status, model-mismatch handling, and stale-state reporting. | Source writes can succeed while autoembedding failure is reported separately. |
| `M8C` | Query embedding | Implement query embedding helpers over engine config and inference `Embedder`, including the manifest/freshness short-circuit that avoids model calls when the selected embedding model cannot match existing shadow vectors. | Missing model and dimension mismatch degrade through documented errors without unnecessary inference calls. |
| `M8D` | Retrieval augmentation stages | Implement query expansion, reranking, RAG prompt/context/citation behavior, and generation lifecycle. | Stages follow the repeatable stage outcome pattern. |
| `M8E` | Diagnostics and provenance | Record recipe version, recipe hash, model identity, stage decisions, degradation, cache use, and provider facts. | Users can explain why a model-assisted result was produced. |
| `M8F` | Boundary guards | Add guards proving intelligence imports engine and inference only through approved surfaces. | Executor and CLI do not import inference directly. |

## Test Track

| Test epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M8TA` | Fake-provider product tests | Use fake providers for expansion, embedding, rerank, RAG, and generation. | Tests run deterministically without network or real models. |
| `M8TB` | Autoembedding and embedding tests | Cover queueing, retry, stale state, model mismatch, cleanup, branch-local behavior, and query-embedding short-circuit before inference calls. | Autoembedding failure does not corrupt source data and incompatible search recipes do not spend model work. |
| `M8TC` | Recipe degradation tests | Exercise required, optional, and fallback stage behavior. | Degradation is reported through stable diagnostics. |
| `M8TD` | RAG and citation tests | Validate context construction, citation extraction, and refusal when evidence is unavailable. | RAG output is explainable and bounded by retrieved evidence. |
| `M8TE` | Dependency guards | Scan crate graph and imports. | Intelligence boundary violations fail fast. |

## Convergence Notes

1. `M8TE` lands early and stays active through all intelligence work.
2. `M8TA` lands with the fake-provider paths used by `M8B` through `M8D`.
3. `M8TB` lands with `M8B` and `M8C`.
4. `M8TC` and `M8TD` land with `M8D`.
5. `M8E` closes after all stage outputs can publish provenance and
   diagnostics.

## Slice Policy

Model-assisted behavior should be sliced by stage. Do not mix autoembedding,
rerank, RAG, and generation internals in one broad slice unless the slice is
only adding shared stage infrastructure.

## Non-Goals

1. No autosearch implementation for V1 unless separately pulled forward.
2. No provider HTTP or model artifact verification in intelligence-next.
3. No direct storage access.
4. No network sync or StrataHub fleet behavior.

## Milestone Exit Gate

M8 is complete when intelligence-next can provide V1 model-assisted behavior
through engine and inference boundaries with deterministic fake-provider tests.
The roadmap Test Gate Summary remains the canonical milestone gate; this plan
explains how M8 reaches it.
