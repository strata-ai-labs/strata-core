# M2 / M2T Implementation Plan: Storage-Next Testkit And Crate Skeleton

Status: draft implementation plan

## Goal

Make storage-next testable before durable behavior lands.

## Inputs

1. `docs/architecture/storage-architecture.md`
2. `docs/architecture/storage/target-crate-shape-and-test-harness.md`
3. `docs/architecture/storage/implementation-patterns.md`
4. `docs/architecture/v1-testing-and-conformance-plan.md`
5. `docs/architecture/v1-engineering-standards.md`

All slices must follow the V1 engineering standards: permanent domain names,
concept-budget discipline, file/function thresholds, comment standards, and no
roadmap labels in production code vocabulary.

## Current-Code Cross-Check Rule

Storage-next is allowed to use fresh code where the current implementation is
too entangled to move directly, but fresh code must not be treated as freehand
design.

Before each implementation slice:

1. Identify the current storage files that correspond to the responsibility
   being implemented.
2. Decide whether each component is being ported, adapted, or written fresh.
3. Preserve existing correct behavior unless the architecture documents
   explicitly change it.
4. Add or update tests when fresh code replaces current behavior, so the slice
   is verified by conformance or characterization rather than inspection alone.

For M2 specifically, backend and object contracts may be fresh boundary code,
but `M2C1` must inspect the current memory/cache and local filesystem paths
before implementing backend shells. Later durable slices should increasingly
port or adapt proven current behavior instead of inventing new mechanics.

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
| `M2TB` | Feature matrix | Check default, no-default, memory-only, localfs, testkit, and fault-injection builds using `cargo hack -p strata-storage-next --feature-powerset --depth 2 --locked check --all-targets` where practical. | Unsupported feature combinations fail loudly or are documented. |
| `M2TC` | WASM cache compile | Protect `wasm32-unknown-unknown` memory/cache compile path. This is a compile-only browser/cache substrate gate, not a durable browser runtime guarantee. | Localfs is not required for browser/cache builds. |
| `M2TD` | Testkit boundary guards | Prove testkit APIs are feature-gated and doc-hidden. | Normal production builds cannot reach test-only hooks. |

## Priority Order

M2 closes in this order unless implementation exposes a smaller sequencing
blocker.

| Priority | Code | Track | Closure condition | Why this order |
|---|---|---|---|---|
| 1 | `M2A` | Implementation | `crates/storage-next` exists, builds in its minimal configuration, inherits workspace lints, denies unsafe code, and exposes only the intended skeleton modules. | Backend contracts, testkit, and harnesses need a stable crate boundary first. |
| 2 | `M2B` | Implementation | Backend capability vocabulary and the minimal backend trait surface compile without durable WAL, manifest, table, branch, or commit behavior. | Memory/local backends and conformance tests need one capability contract to target. |
| 3 | `M2C` | Implementation | Memory/cache and local filesystem backend shells compile; memory can satisfy the claimed non-durable object operations. | Smoke conformance needs concrete backends, and later durable services need localfs shape in place. |
| 4 | `M2TA` | Test | Backend conformance smoke tests prove capability declarations and minimal memory/local behavior are deterministic. | Tests should validate the first backend contract before testkit and harnesses depend on it. |
| 5 | `M2D` | Implementation | Private test support, feature-gated public testkit, and faulting backend wrapper exist without leaking into normal production builds. | Fault and conformance harnesses need a real testkit to compose around. |
| 6 | `M2TD` | Test | Boundary guards prove testkit/fault APIs are feature-gated, doc-hidden, and unavailable from normal builds. | The testkit must be guarded before M3 starts relying on it. |
| 7 | `M2E` | Implementation | Golden-vector, fuzz, property, crash, and conformance harness directories and smoke invocations exist without claiming durable behavior. | Harness scaffolding should sit on the final M2 crate/testkit shape. |
| 8 | `M2TB` | Test | Feature matrix checks default, no-default, memory-only, localfs, testkit, and fault-injection combinations; unsupported combinations fail loudly or are documented. | Feature gates can only be audited after all M2 features exist. |
| 9 | `M2TC` | Test | `wasm32-unknown-unknown` memory/cache compile path is protected with localfs excluded. | WASM compile should close before M2 exits, after the memory-only feature shape is real. |

`M2TC` may run in parallel with `M2D` or `M2E` after `M2C` proves the
memory/cache-only build shape.

## Slice Record

| Slice | Parent | Title | Scope | Verification |
|---|---|---|---|---|
| `M2A1` | `M2A` | Storage-next crate skeleton | Add `crates/storage-next`, workspace membership, crate-level unsafe policy, empty/minimal public surface, initial module tree, and feature declarations for `localfs`, `testkit`, and `fault-injection`. Do not add durable behavior. | `cargo check -p strata-storage-next --locked`; `cargo clippy -p strata-storage-next --all-targets --locked -- -D warnings`. |
| `M2B1` | `M2B` | Backend capability contract | Add capability types and the minimal backend trait surface needed by memory, localfs, and conformance smoke tests. Keep durable publish, WAL, manifest, table, branch, and commit semantics out. Temporary dead-code lint expectations may appear only inside the new backend/object contract modules until concrete backends consume them in `M2C1`. | `cargo test -p strata-storage-next --locked`; documentation review against storage L1 and target crate-shape docs. |
| `M2C1` | `M2C` | Memory and local backend shells | Add memory/cache backend behavior for non-durable object operations and local filesystem backend shell/capability declarations. Localfs should not imply durable publish semantics yet. | `cargo test -p strata-storage-next --locked`; backend smoke tests under `M2TA`. |
| `M2TA1` | `M2TA` | Backend conformance smoke | Add private backend conformance tests for capability declarations and minimal object behavior claimed by memory/local shells. Keep the suite inside `src/backend` until the M2D testkit exists. | `cargo test -p strata-storage-next backend::conformance --locked`; `cargo test -p strata-storage-next --no-default-features backend::conformance --locked`; `cargo test -p strata-storage-next --locked`. |
| `M2D1` | `M2D` | Storage-next testkit foundation | Add crate-private `test_support`, feature-gated `testkit`, and a faulting backend wrapper over the minimal backend trait. Public testkit items must be `#[doc(hidden)]` and marked test-only. | `cargo test -p strata-storage-next --features testkit,fault-injection --locked`; `cargo test -p strata-storage-next --no-default-features --features testkit,fault-injection --locked`; `cargo check -p strata-storage-next --features testkit --locked`; production build without testkit. |
| `M2TD1` | `M2TD` | Testkit boundary guards | Add tests that prove testkit and fault-injection APIs are unavailable without their features and hidden/test-only when enabled. | `cargo test -p strata-storage-next --test testkit_boundary --locked`; `cargo test -p strata-storage-next --features testkit,fault-injection --test testkit_boundary --locked`; full storage-next test suite. |
| `M2E1` | `M2E` | Harness scaffolding | Add empty/smoke golden-vector, fuzz, property, crash, and conformance harness locations and manual invocation notes. Harnesses must not assert durable behavior before M3. | Smoke harness invocations documented in storage target crate-shape style; `cargo test -p strata-storage-next --locked`. |
| `M2TB1` | `M2TB` | Storage-next feature matrix | Check default, no-default, memory-only, localfs, testkit, fault-injection, perf-trace, and pairwise feature combinations. Use `cargo hack` where available; otherwise record equivalent explicit cargo commands. | `cargo hack -p strata-storage-next --feature-powerset --depth 2 --locked check --all-targets` passes; `cargo test -p strata-storage-next --no-default-features --features testkit --test backend_conformance --locked` proves unsupported backend selection fails loudly. |
| `M2TC1` | `M2TC` | WASM memory/cache compile gate | Add or document the compile-only wasm32 memory/cache check with `localfs` excluded. This is not a durable browser runtime guarantee. | `cargo check -p strata-storage-next --no-default-features --target wasm32-unknown-unknown --all-targets --locked` passes; `cargo test -p strata-storage-next --test testkit_boundary --locked localfs_feature_is_rejected_for_wasm_builds` proves default-feature wasm builds fail clearly because `localfs` is not supported on wasm32. |

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
