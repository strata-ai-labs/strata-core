# M2 / M2T Implementation Plan: Storage-Next Testkit And Crate Skeleton

Status: draft implementation plan

## Goal

Make storage-next testable before durable behavior lands.

## Inputs

1. `docs/architecture/storage-next-architecture.md`
2. `docs/architecture/storage-next/target-crate-shape-and-test-harness.md`
3. `docs/architecture/storage-next/implementation-patterns.md`
4. `docs/architecture/v1-testing-and-conformance-plan.md`
5. `docs/architecture/v1-engineering-standards.md`

All slices must follow the V1 engineering standards: permanent domain names,
concept-budget discipline, file/function thresholds, comment standards, and no
roadmap labels in production code vocabulary.

## Implementation Track

| Epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M2A` | Crate skeleton | Create storage-next with crate-level policy, feature gates, module tree, and dependency rules. | Crate builds with memory/cache-only features. |
| `M2B` | Backend contract shell | Define backend capabilities and the minimal backend trait surface. | Backends declare capabilities without durable services. |
| `M2C` | Memory and local backend shells | Add memory/cache backend and local filesystem backend skeletons. | Both compile; memory backend can satisfy non-durable operations. |
| `M2D` | Testkit foundation | Add feature-gated testkit, private test support, and faulting backend wrapper. | Testkit is unavailable in normal production builds. |
| `M2E` | Harness scaffolding | Add golden-vector, fuzz, property, and crash harness directories and invocation stubs. | Harnesses run empty/smoke checks without false product claims. |

## Test Track

| Test epic | Title | Scope | Exit gate |
|---|---|---|---|
| `M2TA` | Backend conformance smoke | Test memory/cache backend and local backend capability declarations. | Capability mismatches fail deterministically. |
| `M2TB` | Feature matrix | Check default, no-default, memory-only, localfs, testkit, and fault-injection builds using `cargo hack check --workspace --feature-powerset --depth 2` where practical. | Unsupported feature combinations fail loudly or are documented. |
| `M2TC` | WASM cache compile | Protect `wasm32-unknown-unknown` memory/cache compile path. This is a compile-only browser/cache substrate gate, not a durable browser runtime guarantee. | Localfs is not required for browser/cache builds. |
| `M2TD` | Testkit boundary guards | Prove testkit APIs are feature-gated and doc-hidden. | Normal production builds cannot reach test-only hooks. |

## Convergence Notes

1. `M2TA` lands with `M2B` and `M2C`.
2. `M2TB` and `M2TC` close before any downstream milestone relies on the
   storage-next crate shape.
3. `M2TD` closes before M3 fault or conformance harnesses use the testkit.

## Slice Policy

Do not implement durable WAL, manifest, table, branch, or commit behavior in
M2. The skeleton exists to make later implementation testable, not to sneak in
semantics.

## Non-Goals

1. No durable publish semantics.
2. No object-store/OpenDAL backend.
3. No table format implementation.
4. No engine-facing storage API.

## Milestone Exit Gate

M2 is complete when storage-next has a clean crate shape, explicit testkit, and
backend harnesses ready for the lower storage mechanics. The roadmap Test Gate
Summary remains the canonical milestone gate; this plan explains how M2 reaches
it.
