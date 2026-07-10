# M6 / M6T Implementation Plan: Engine-Next Product Semantics

Status: draft implementation plan

## Goal

Implement the V1 database product behavior over the engine persistence adapter.

## Inputs

1. `docs/architecture/engine/primitive-implementation-contract.md`
2. `docs/architecture/engine/entity-ref-and-relationship-layer-contract.md`
3. `docs/architecture/engine/branch-operation-and-capability-adapter-contract.md`
4. `docs/architecture/engine/temporal-context-and-timeline-resolver-contract.md`
5. `docs/architecture/engine/retrieval-and-derived-state-contract.md`
6. `docs/architecture/engine/ipc-and-command-boundary-contract.md`
7. `docs/architecture/engine/dataset-clone-artifact-contract.md`
8. `docs/architecture/engine/product-pathway-conformance-plan.md`
9. `docs/architecture/intelligence-architecture.md`
10. `docs/architecture/v1-removed-surfaces.md`

All slices must follow the V1 engineering standards: permanent domain names,
concept-budget discipline, file/function thresholds, comment standards, and no
roadmap labels in production code vocabulary.

## Implementation Track

| Epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M6A` | Data capability pattern | Implement the repeatable capability adapter pattern over the KV row substrate. | New capabilities use shared write/read/derive patterns. |
| `M6B` | KV, JSON, and events | Implement direct KV behavior, JSON document behavior, and immutable event records. | Public writes map to clear commit semantics without manual transactions. |
| `M6C` | Vector and graph relationships | Implement user vectors, shadow-vector addressing, graph relationship bindings, reverse maps, and delete policies. | Cross-capability relationships use `EntityRef` consistently. |
| `M6D` | Branch and temporal behavior | Implement branch create, branch-from-version, branch-from-time, compare, promote, copy, restore, revert, cherry-pick, delete, latest, `getv`, history, and `as_of`. | Branch/time behavior passes product pathway conformance. |
| `M6E` | Retrieval and derived state | Define the shared engine `StageOutcome` DTO, then implement recipes, BM25 substrate, search stages, freshness checks, derived-state manifests, and graph-aware retrieval substrate. | Retrieval degrades through structured diagnostics and intelligence-next can consume the stage outcome vocabulary. |
| `M6F` | Command and IPC semantics | Implement serializable command classification for local and IPC-backed handles. | Same command semantics work in-process and over local IPC. |
| `M6G` | Clone artifact substrate | Implement dataset clone artifact validation, export/import substrate, provenance rows, and pre-V1 rejection. | Clone artifacts are validated before open/import mutates state. |
| `M6H` | Removed surfaces | Remove the public surfaces listed in `docs/architecture/v1-removed-surfaces.md`, including normal-user `flush`, `compact`, `checkpoint`, `gc`, `repair`, retention, and manual recovery commands. | Guard tests prove removed public surfaces do not return. |

## Test Track

| Test epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M6TA` | Product pathway tests | Implement required V1 pathway conformance over engine-next. | Required pathways pass in cache and durable local modes where applicable. |
| `M6TB` | Capability tests | Cover KV, JSON, event, vector, graph, and relationship behavior. | Each capability follows the shared adapter contract. |
| `M6TC` | Branch and time tests | Model branch operations, conflict behavior, history, and timestamp resolution. | Product behavior matches branching and temporal contracts. |
| `M6TD` | Retrieval tests | Cover recipes, BM25, vector substrate, graph-aware retrieval, freshness, and degradation. | Retrieval reports deterministic outcomes and diagnostics. |
| `M6TE` | IPC command tests | Exercise local and IPC command classification. | IPC transport does not change command semantics. |
| `M6TF` | Removed-surface guards | Scan public API, CLI, docs, and dependency graph for removed product surfaces. | Removed behavior cannot be accidentally reintroduced. |

## Intermediate Gates

1. Data capability gate: `M6A` through `M6C` plus `M6TB`.
2. Branch and temporal gate: `M6D` plus `M6TC`.
3. Retrieval substrate gate: `M6E` plus `M6TD`; this gate must close before
   intelligence-next depends on stage outcomes.
4. Command/clone gate: `M6F` and `M6G` plus `M6TE`.
5. Cleanup gate: `M6H` plus `M6TF`.

## Convergence Notes

1. `M6TA` grows throughout M6 and closes only after all required product
   pathways for implemented surfaces pass.
2. `M6TB` lands with `M6A` through `M6C`.
3. `M6TC` lands with `M6D`.
4. `M6TD` lands with `M6E`.
5. `M6TE` lands with `M6F`.
6. `M6TF` lands with `M6H`.
7. `M6A` through `M6F` complete the engine surfaces required by
   intelligence-next "Engine Surface Consumed."

## Slice Policy

Prefer vertical slices by user-visible behavior after the capability pattern is
established. Each public behavior slice must include product-path or contract
tests in the same milestone.

## Non-Goals

1. No inference provider execution.
2. No intelligence orchestration.
3. No network server mode.
4. No hidden sync.
5. No public maintenance or manual transaction workflow.

## Milestone Exit Gate

M6 is complete when engine-next provides the V1 database product surface over
storage-next and all removed old surfaces are guarded. The roadmap Test Gate
Summary remains the canonical milestone gate; this plan explains how M6 reaches
it.
