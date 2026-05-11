# M5 / M5T Implementation Plan: Engine-Next Persistence Adapter And Control Plane

Status: draft implementation plan

## Goal

Create the engine-next persistence boundary and control plane before product
data capabilities are implemented.

## Inputs

1. `docs/architecture/engine-next-architecture.md`
2. `docs/architecture/engine-next/target-crate-shape-and-test-harness.md`
3. `docs/architecture/engine-next/persistence-adapter-contract.md`
4. `docs/architecture/engine-next/control-plane-layout-contract.md`
5. `docs/architecture/engine-next/storage-space-id-registry.md`
6. `docs/architecture/runtime-resource-profile-architecture.md`
7. `docs/architecture/intelligence-next-architecture.md`

All slices must follow the V1 engineering standards: permanent domain names,
concept-budget discipline, file/function thresholds, comment standards, and no
roadmap labels in production code vocabulary.

## Implementation Track

| Epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M5A` | Engine crate skeleton | Create engine-next with target module buckets, crate-level policy, feature gates, and no old subsystem architecture. | Crate builds with persistence and control modules empty or stubbed. |
| `M5B` | Persistence adapter | Implement the only normal engine path to storage-next L9. | Dependency guards prove storage imports are isolated to persistence. |
| `M5C` | Physical row encoding | Implement storage-space ID routing and product-reference-to-row-key encoding. | Engine owns all product reference encoding; storage sees opaque row keys. |
| `M5D` | Control-plane layout | Implement `_system_` branch and branch-local `_system_` space bootstrap and validation. | Registry rows are created, validated, and fail closed when corrupt. |
| `M5E` | Runtime resource profile | Resolve host facts and user config into storage, engine, derived-state, and inference hints, then pass storage budgets through the M4 L9 constructor/config boundary. | Resolved budgets are passed downward without global state. |
| `M5F` | Error mapping | Map storage diagnostics into engine product errors while preserving source chains. | Public errors do not expose storage enum names as product API. |

## Test Track

| Test epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M5TA` | Dependency guards | Scan crate graph and source imports. | No engine module outside persistence imports storage-next. |
| `M5TB` | Persistence fake tests | Test engine behavior against fake L9 storage outcomes. | Adapter handles success, corruption, unavailable backend, and ambiguous commit facts. |
| `M5TC` | Control-plane bootstrap tests | Cover new database, matching registry, missing rows, corrupt rows, and version mismatch. | Failures use stable error codes. |
| `M5TD` | Resource profile tests | Exercise edge, desktop, server, unknown, and explicit profiles. | Budgets are deterministic and explainable. |
| `M5TE` | Error mapping tests | Validate class, code, source, redaction, and retry behavior. | Engine errors obey the global diagnostics contract. |

## Convergence Notes

1. `M5TA` lands with `M5B` and remains active through all later engine work.
2. `M5TB` lands before product capability work starts.
3. `M5TC` lands with `M5D`.
4. `M5TD` lands with `M5E`.
5. `M5D` and `M5E` begin the engine surfaces required by intelligence-next
   "Engine Surface Consumed"; M6 completes the product-facing parts.

## Slice Policy

M5 slices should not implement KV, JSON, event, vector, graph, retrieval, or
branch product behavior except minimal smoke paths needed to prove persistence
and control-plane bootstrap.

## Non-Goals

1. No product data capability implementation.
2. No IPC service implementation.
3. No intelligence or inference integration.
4. No public compatibility layer for old engine internals.

## Milestone Exit Gate

M5 is complete when engine-next can open cache and durable databases, bootstrap
control-plane rows, and read/write opaque engine rows through the persistence
adapter only. The roadmap Test Gate Summary remains the canonical milestone
gate; this plan explains how M5 reaches it.
