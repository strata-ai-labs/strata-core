# V1 Document Inventory

Status: M0A document inventory

## Purpose

This document records the architecture, product, specification, evidence, and
implementation-planning documents that make up the V1 rewrite reading path.

It is an inventory, not a replacement for the documents it lists. M0A uses this
file to close one question before implementation starts: every required
document either exists, is explicitly historical evidence, or is assigned to a
later milestone.

## Inventory Rules

1. Binding documents define V1 product, architecture, contracts, specs, or
   implementation order.
2. Evidence documents describe the current codebase or completed cleanup work.
   They are useful for implementation, but target architecture documents win
   when there is a conflict.
3. Historical documents are not target architecture. They may explain why the
   codebase looks the way it does.
4. Placeholder documents are intentionally present but incomplete. Their owner
   milestone must be named.
5. Deferred documents are not required before the next milestone starts.

## Product Anchors

| Document | Status | Role |
|---|---|---|
| `docs/product/strata-v1-product-requirements.md` | Exists | Binding product anchor. |
| `docs/product/strata-v1-feature-inventory.md` | Exists | Binding feature scope inventory. |
| `docs/product/strata-v1-user-pathways.md` | Exists | Binding pathway index. |
| `docs/product/strata-v1-non-functional-requirements.md` | Exists | Binding first-pass NFR anchor. |
| `docs/product/pathways/data-capabilities.md` | Exists | Pathway detail. |
| `docs/product/pathways/branching-versioning-time-travel.md` | Exists | Pathway detail. |
| `docs/product/pathways/retrieval-and-intelligence.md` | Exists | Pathway detail. |
| `docs/product/pathways/runtime-and-portability.md` | Exists | Pathway detail. |
| `docs/product/pathways/operations-and-interfaces.md` | Exists | Pathway detail. |

## Focused Product Direction

| Document | Status | Role |
|---|---|---|
| `docs/product/strata-v1-branching-direction.md` | Exists | Binding branching product direction. |
| `docs/product/strata-v1-graph-relationship-layer.md` | Exists | Binding graph relationship-layer direction. |
| `docs/product/strata-v1-versioning-time-travel.md` | Exists | Binding time-travel product direction. |
| `docs/product/stratahub-product-direction.md` | Exists | Product direction for StrataHub. |
| `docs/product/strata-autosearch-product-direction.md` | Exists | Post-V1 product direction and substrate guidance. |
| `docs/product/strata-v1-architecture-support-matrix.md` | Exists | Product-to-architecture coverage check. |

## Architecture Anchors

| Document | Status | Role |
|---|---|---|
| `docs/architecture/strata-v1-architecture.md` | Exists | Binding high-level V1 architecture anchor. |
| `docs/architecture/core-architecture.md` | Exists | Binding core-next architecture. |
| `docs/architecture/storage-architecture.md` | Exists | Binding storage-next architecture. |
| `docs/architecture/engine-architecture.md` | Exists | Binding engine-next architecture. |
| `docs/architecture/inference-architecture.md` | Exists | Binding inference-next architecture. |
| `docs/architecture/intelligence-architecture.md` | Exists | Binding intelligence-next architecture. |
| `docs/architecture/stratahub-substrate-architecture.md` | Exists | Binding V1 StrataHub substrate architecture. |
| `docs/architecture/runtime-resource-profile-architecture.md` | Exists | Binding runtime resource profile architecture. |

## Cross-Cutting V1 Contracts

| Document | Status | Role |
|---|---|---|
| `docs/architecture/v1-error-and-diagnostics-contract.md` | Exists | Binding cross-layer error and diagnostics contract. |
| `docs/architecture/v1-testing-and-conformance-plan.md` | Exists | Binding top-level test and conformance strategy. |
| `docs/architecture/v1-engineering-standards.md` | Exists | Binding coding, naming, comment, and file-shape standards. |
| `docs/architecture/v1-engineering-standards-baseline.md` | Exists | M0TD standards scan baseline over current source and docs. |
| `docs/architecture/v1-existing-test-inventory-and-porting-plan.md` | Exists | Binding strategy for classifying existing tests. |
| `docs/architecture/v1-test-inventory.md` | Exists | M0TE-populated canonical test inventory. |
| `docs/architecture/v1-removed-surfaces.md` | Exists | Binding removed-surface list. |
| `docs/architecture/v1-cutover-pr-series.md` | Placeholder | M9G-owned cutover sequence and promotion plan. |
| `docs/architecture/v1-boundary-baseline.md` | Exists | M0TC factual crate-boundary baseline. |
| `docs/architecture/v1-document-inventory.md` | Exists | M0A canonical document inventory. |
| `docs/architecture/v1-open-question-register.md` | Exists | M0B canonical open-question ownership register. |

## Storage-Next Documents

| Document | Status | Role |
|---|---|---|
| `docs/architecture/storage/README.md` | Exists | Storage-next document index and reading order. |
| `docs/architecture/storage/l1-backend-io.md` | Exists | Binding L1 conceptual contract. |
| `docs/architecture/storage/l2-object-layout.md` | Exists | Binding L2 conceptual contract. |
| `docs/architecture/storage/l3-durable-format-codec.md` | Exists | Binding L3 conceptual contract. |
| `docs/architecture/storage/l4-log-manifest-snapshot-services.md` | Exists | Binding L4 conceptual contract. |
| `docs/architecture/storage/l5-table-runtime.md` | Exists | Binding L5 conceptual contract. |
| `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md` | Exists | Binding L6 conceptual contract. |
| `docs/architecture/storage/l7-commit-runtime.md` | Exists | Binding L7 conceptual contract. |
| `docs/architecture/storage/l8-lifecycle-recovery-maintenance.md` | Exists | Binding L8 conceptual contract. |
| `docs/architecture/storage/l9-storage-api-boundary.md` | Exists | Binding L9 conceptual contract. |
| `docs/architecture/storage/implementation-patterns.md` | Exists | Binding repeatable implementation patterns. |
| `docs/architecture/storage/target-crate-shape-and-test-harness.md` | Exists | Binding target crate shape and test harness plan. |
| `docs/architecture/storage/storage-space-id-registry.md` | Exists | Binding storage-owned storage-space registry. |
| `docs/architecture/storage/commit-timeline-substrate.md` | Exists | Binding commit timeline placement contract. |
| `docs/spec/strata-storage-format-v1.md` | Exists | Draft public storage format specification; unstable until M3 freeze. |

## Engine-Next Documents

| Document | Status | Role |
|---|---|---|
| `docs/architecture/engine/README.md` | Exists | Engine-next contract index and reading order. |
| `docs/architecture/engine/primitive-implementation-contract.md` | Exists | Binding data-capability implementation contract. |
| `docs/architecture/engine/entity-ref-and-relationship-layer-contract.md` | Exists | Binding EntityRef and relationship-layer contract. |
| `docs/architecture/engine/storage-space-id-registry.md` | Exists | Binding engine-owned storage-space registry. |
| `docs/architecture/engine/persistence-adapter-contract.md` | Exists | Binding engine-to-storage adapter contract. |
| `docs/architecture/engine/branch-operation-and-capability-adapter-contract.md` | Exists | Binding branch operation contract. |
| `docs/architecture/engine/temporal-context-and-timeline-resolver-contract.md` | Exists | Binding temporal context and timeline contract. |
| `docs/architecture/engine/control-plane-layout-contract.md` | Exists | Binding control-plane layout contract. |
| `docs/architecture/engine/retrieval-and-derived-state-contract.md` | Exists | Binding retrieval and derived-state contract. |
| `docs/architecture/engine/ipc-and-command-boundary-contract.md` | Exists | Binding IPC and serializable command boundary contract. |
| `docs/architecture/engine/dataset-clone-artifact-contract.md` | Exists | Binding dataset clone artifact contract. |
| `docs/architecture/engine/public-api-and-cli-surface-cleanup-checklist.md` | Exists | Binding public API and CLI cleanup checklist. |
| `docs/architecture/engine/product-pathway-conformance-plan.md` | Exists | Binding engine product-path conformance plan. |
| `docs/architecture/engine/error-and-diagnostics-contract.md` | Exists | Binding engine-specific error contract. |
| `docs/architecture/engine/testing-and-conformance-plan.md` | Exists | Binding engine test and conformance plan. |
| `docs/architecture/engine/target-crate-shape-and-test-harness.md` | Exists | Binding target crate shape and test harness plan. |

## Implementation Plans

| Document | Status | Role |
|---|---|---|
| `docs/architecture/strata-v1-implementation-roadmap.md` | Exists | Binding milestone roadmap and sequencing. |
| `docs/architecture/v1-progress-tracker.md` | Exists | Current V1 execution ledger and issue/PR label protocol. |
| `docs/architecture/implementation-plans/m0-m0t-implementation-plan.md` | Exists | M0 architecture freeze and tracking plan. |
| `docs/architecture/implementation-plans/m1-m1t-implementation-plan.md` | Exists | M1 core-next implementation plan. |
| `docs/architecture/implementation-plans/m2-m2t-implementation-plan.md` | Exists | M2 storage backend/object/format foundation plan. |
| `docs/architecture/implementation-plans/m3-m3t-implementation-plan.md` | Exists | M3 storage durable services plan. |
| `docs/architecture/implementation-plans/m4-m4t-implementation-plan.md` | Exists | M4 storage table/branch/commit/API plan. |
| `docs/architecture/implementation-plans/m5-m5t-implementation-plan.md` | Exists | M5 engine foundation plan. |
| `docs/architecture/implementation-plans/m6-m6t-implementation-plan.md` | Exists | M6 engine capabilities and product surface plan. |
| `docs/architecture/implementation-plans/m7-m7t-implementation-plan.md` | Exists | M7 inference-next plan. |
| `docs/architecture/implementation-plans/m8-m8t-implementation-plan.md` | Exists | M8 intelligence-next plan. |
| `docs/architecture/implementation-plans/m9-m9t-implementation-plan.md` | Exists | M9 executor/CLI/SDK/cutover plan. |
| `docs/architecture/implementation-plans/m10-m10t-implementation-plan.md` | Exists | M10 V1 readiness and release hardening plan. |

## Current-Code Evidence

These documents remain useful evidence for implementation and reviews. They are
not binding V1 target architecture when they conflict with the V1 architecture
anchors.

| Document | Status | Role |
|---|---|---|
| `docs/core/core-charter.md` | Exists | Current/historical core scope evidence. |
| `docs/core/core-crate-map.md` | Exists | Current core crate map evidence. |
| `docs/storage/storage-charter.md` | Exists | Current storage scope evidence. |
| `docs/storage/storage-crate-map.md` | Exists | Current storage crate map evidence. |
| `docs/storage/v1-storage-consumption-contract.md` | Exists | Current consolidated engine/storage boundary evidence. |
| `docs/storage/storage-engine-ownership-audit.md` | Exists | Current ownership audit evidence. |
| `docs/storage/concurrency-crate-map.md` | Exists | Historical lower-runtime crate map evidence. |
| `docs/storage/durability-crate-map.md` | Exists | Historical lower-runtime crate map evidence. |
| `docs/engine/engine-consolidation-plan.md` | Exists | Active milestone-free consolidation closeout evidence. |
| `docs/engine/engine-crate-map.md` | Exists | Current engine crate map evidence. |
| `docs/engine/follower-mode-removal-plan.md` | Exists | Active current-code cleanup plan; V1 still removes follower mode. |
| `docs/core/archive/error-research.md` | Historical | Historical error-model research used as evidence by the V1 diagnostics contract. |
| `docs/core/archive/core-error-review.md` | Historical | Historical core error ownership review used as evidence by the V1 diagnostics contract. |
| `docs/engine/archive/engine-error-architecture.md` | Historical | Historical engine error architecture used as evidence by the V1 diagnostics contract. |

## Audits

| Document | Status | Role |
|---|---|---|
| `docs/audits/llama-ffi-unsafe-audit.md` | Placeholder | M7E-owned unsafe audit required before local llama.cpp runtime is V1-ready. |

## Historical Documents

| Document | Status | Role |
|---|---|---|
| `docs/architecture/next-charter.md` | Historical | Superseded original next-generation charter. Not binding for V1. |
| `docs/core/archive/` | Historical archive | Core cleanup-era plans and reviews. |
| `docs/storage/archive/` | Historical archive | Storage cleanup-era implementation plans. |
| `docs/engine/archive/` | Historical archive | Engine/storage cleanup-era plans and closeout records. |

## Explicit Deferrals

| Missing or incomplete document | Owner | Decision |
|---|---|---|
| Final public API and CLI implementation detail beyond M9 | `M9` | Deferred to M9 slices; M9 plan is sufficient for M0A. |
| Completed cutover PR sequence | `M9G` | Placeholder exists at `docs/architecture/v1-cutover-pr-series.md`. |
| Completed llama.cpp unsafe audit | `M7E` | Placeholder exists at `docs/audits/llama-ffi-unsafe-audit.md`. |
| Stable storage format compatibility promise | `M3` | Draft spec exists; M3 freezes golden vectors and byte compatibility. |
| StrataHub sync and fleet-management implementation plans | Post-V1 | Substrate architecture exists; sync and fleet management are not V1 implementation blockers. |

## M0A Closure

M0A is closed when:

1. Every document listed as `Exists` is present in the repository.
2. `docs/architecture/next-charter.md` is clearly marked historical.
3. Placeholder and deferred documents have named owners.
4. Active non-archive V1 docs have no referenced documents missing from this
   inventory.
5. Active V1 docs can refer to this inventory instead of relying on memory.

At M0A capture time, no binding V1 architecture or contract document is missing.
