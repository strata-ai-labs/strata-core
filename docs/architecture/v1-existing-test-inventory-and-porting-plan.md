# V1 Existing Test Inventory And Porting Plan

Status: V1 architecture draft

## Purpose

The existing Strata test suite is valuable evidence, but it is not
automatically the V1 test suite.

Some current tests protect real product and storage guarantees. Others freeze
cleanup-era architecture, old crate boundaries, temporary compatibility
surfaces, follower mode, public transaction sessions, disk-backed cache,
subsystem wiring, or implementation details that V1 is explicitly changing.

This document defines how existing tests are inventoried, classified, ported,
rewritten, archived, or deleted as the V1 roadmap proceeds.

## Related Documents

1. `docs/architecture/strata-v1-implementation-roadmap.md`
2. `docs/architecture/v1-testing-and-conformance-plan.md`
3. `docs/architecture/storage/target-crate-shape-and-test-harness.md`
4. `docs/architecture/engine/testing-and-conformance-plan.md`
5. `docs/architecture/engine/product-pathway-conformance-plan.md`
6. `docs/product/strata-v1-product-requirements.md`
7. `docs/product/strata-v1-feature-inventory.md`

## Binding Rule

Existing tests are evidence, not authority.

No existing test should be ported to the V1 line merely because it exists.
Every test must answer:

1. What behavior does this test protect?
2. Is that behavior required, optional, redesigned, or removed in V1?
3. Which V1 layer owns the behavior?
4. Which milestone/test track should carry the test?
5. Should the test be kept, rewritten, archived, or deleted?

## Parallel Test Track

Every roadmap milestone has a matching test track:

```text
M1   implementation milestone
M1T  test track for M1

M1A  implementation epic
M1TA test epic

M1A1  implementation slice
M1TA1 test slice
```

No milestone is complete until both the implementation track and the test track
pass their gates.

Test-track work is not cleanup after implementation. It is a parallel workstream
that makes the implementation trustworthy.

## Current Test Surface Snapshot

Snapshot date: 2026-05-11.
Snapshot commit: `346aa965`.

The current repository contains these major test areas:

| Area | Current location | Approximate count from scan | Likely V1 disposition |
|---|---|---:|---|
| Root intelligence tests | `tests/intelligence/` | 30 files | Keep or rewrite into `M8T`; many are product-path evidence. |
| Root integration tests | `tests/integration/` | 22 files | Keep or rewrite into `M5T`, `M6T`, and `M9T`. |
| Root engine tests | `tests/engine/` | 19 files | Rewrite into engine-next conformance tests; delete removed-surface tests. |
| Root executor tests | `tests/executor/` | 13 files | Rewrite into `M9T` command/API tests. |
| Root transaction runtime tests | `tests/transaction_runtime/` | 11 files | Mostly archive/rewrite; public manual transaction sessions are removed. |
| Root durability tests | `tests/durability/` | 8 files | Strong keep/rewrite candidates for `M3T` and `M4T`. |
| Root storage tests | `tests/storage/` | 5 files | Rewrite into storage-next L5-L9 conformance/property tests. |
| Shared root fixtures | `tests/fixtures/` | 7 files | Keep only if the V1 pathway still needs them. Legacy branch bundle fixtures likely delete/archive. |
| Engine crate integration tests | `crates/engine/tests/` | 23 files | Rewrite into `M5T` and `M6T`, plus archive characterization tests. |
| Engine inline database tests | `crates/engine/src/database/tests/` | 10 files | Rewrite into engine-next lifecycle/open/recovery tests. |
| Storage segmented tests | `crates/storage/src/segmented/tests/` | 15 files | High-value evidence for storage-next L5-L8, but likely rewritten against new names/contracts. |
| Intelligence crate tests | `crates/intelligence/tests/` | 4 files | Keep/rewrite into `M8T`. |
| Benchmarks | `benchmarks/`, `crates/engine/benches/` | multiple benches and baselines | Carry into M10 performance gate after benchmark policy is refreshed. |
| Proptest regressions | `tests/proptest-regressions/`, `crates/storage/proptest-regressions/` | regression corpora | Keep if corresponding property tests survive. |
| Cleanup guard tests | root `*_surface*.rs`, consolidation characterization tests such as `tests/storage_surface_imports.rs`, `tests/engine_surface_imports.rs`, and `tests/engine_security_surface.rs` | several files | Archive or rewrite as V1 dependency/public-surface guards. |

Counts are from a repository scan and are meant to size the problem, not freeze
an exact inventory.

## Classification Actions

Every existing test gets one action.

### Keep

Keep when the test already protects a V1 guarantee and can move with minimal
change.

Examples:

1. Codec corruption handling.
2. WAL-before-visible recovery behavior.
3. Branch isolation product behavior.
4. Stable error redaction.
5. Search result determinism that remains part of V1.
6. Graph relationship behavior that maps to the new relationship contract.

### Rewrite

Rewrite when the behavior survives but the old test is coupled to implementation
details that V1 changes.

Examples:

1. Tests that construct old storage keys directly.
2. Tests that assert old module names or subsystem wiring.
3. Tests that use old transaction session APIs to prove an internal commit
   guarantee.
4. Tests that assert old recovery message text instead of V1 error codes.
5. Tests that assume primitive-specific storage tags instead of storage-space
   IDs.

### Archive

Archive when the test is useful historical evidence but should not execute in
the V1 test suite.

Examples:

1. Characterization tests for old engine/storage consolidation work.
2. Tests documenting why a bug happened in the old architecture when the V1
   contract covers the behavior differently.
3. Tests for old internal names that help reviewers understand migration
   history but should not constrain V1 implementation.

Archived tests may move to an archive directory or remain only as referenced
evidence in docs. They should not run in normal V1 CI.

### Delete

Delete when the behavior is explicitly removed or harmful to preserve.

Examples:

1. Follower mode tests.
2. Disk-backed cache tests.
3. Public manual transaction session tests.
4. Branch bundle tests, unless rewritten as dataset clone artifact tests.
5. Tags/notes tests.
6. Tests that exist only to keep old/new compatibility shims alive.

## V1 Test Types

Existing tests should be ported into the right V1 test family.

| Test type | Purpose | Typical milestone |
|---|---|---|
| Unit | local type/module invariant | Any `M*T` |
| Property | generated invariant over many states | `M1T`, `M3T`, `M4T`, `M6T` |
| Golden | durable bytes or command/wire status | `M3T`, `M6T`, `M9T` |
| Fuzz | decoders, parsers, command payloads, cursor state | `M3T`, `M4T`, `M6T`, `M9T` |
| Fault injection | explicit failure windows | `M2T`, `M3T`, `M4T`, `M5T`, `M7T`, `M8T` |
| Crash recovery | process death between durable transitions | `M3T`, `M4T`, `M10T` |
| Backend conformance | backend capability and behavior parity | `M2T`, `M3T`, `M4T` |
| Product path | user-visible workflow | `M6T`, `M8T`, `M9T`, `M10T` |
| Dependency guard | forbidden crate edges and public-surface leakage | every milestone that changes boundaries |
| Benchmark | regression and scaling behavior | `M10T`, with earlier smoke gates where useful |

## Milestone Test Track Map

### M0T: Architecture/Test Inventory Track

Purpose: classify existing tests before implementation starts.

Work:

1. Generate the full current test inventory.
2. Assign each test a classification action.
3. Assign each test a target milestone.
4. Identify tests protecting removed behavior.
5. Identify tests that must become V1 guard tests.
6. Identify test fixtures that are reusable versus legacy-only.

Exit gate:

1. Every existing test file has an action.
2. Every keep/rewrite test has a target `M*T` track.
3. Deleted/archived tests are tied to a V1 product or architecture decision.

### M1T: Core-Next Tests

Likely sources:

1. `tests/core_foundation_surface.rs`
2. any current core crate tests, once inventoried

V1 test focus:

1. `BranchId` serialization/parse/display.
2. `CommitVersion` ordering and boundary behavior.
3. `Timestamp` representation and arithmetic.
4. Type-local validation errors.
5. No Strata-crate dependency guard.

### M2T: Storage Testkit And Backend Skeleton Tests

Likely sources:

1. `tests/storage/`
2. storage test hooks
3. backend-like tests embedded in durability suites

V1 test focus:

1. Memory/cache backend conformance.
2. Local filesystem capability declarations.
3. Faulting backend wrapper.
4. Testkit public/private boundary.
5. `wasm32-unknown-unknown` cache compile gate.

### M3T: Storage Backend/Layout/Format/Durable Services Tests

Likely sources:

1. `tests/durability/`
2. `crates/storage/src/segmented/tests/publish_failures.rs`
3. codec/recovery tests under engine that currently exercise storage mechanics
4. storage proptest regressions

V1 test focus:

1. Object layout property tests.
2. Format golden vectors.
3. Format fuzz targets.
4. WAL/manifest/snapshot durable publish fault windows.
5. Strict decoder behavior.
6. Cache mode absence of durable object services.

### M4T: Storage Table/Branch/Commit/Recovery/L9 Tests

Likely sources:

1. `crates/storage/src/segmented/tests/`
2. `tests/storage/`
3. `tests/durability/`
4. recovery tests currently under `crates/engine/tests/`

V1 test focus:

1. Table model/property tests.
2. Branch visibility model tests.
3. Commit timeline tests.
4. Retention, tombstone, TTL, compaction tests.
5. Crash recovery for durable local modes.
6. L9 storage API conformance.

### M5T: Engine Persistence And Control Plane Tests

Likely sources:

1. `crates/engine/tests/recovery_storage_policy.rs`
2. `tests/storage_surface_imports.rs`
3. engine surface guard tests
4. branch/control-store recovery tests

V1 test focus:

1. Persistence adapter uses storage-next L9 only.
2. Storage error mapping.
3. Storage-space registry validation.
4. `_system_` branch and system-space layout.
5. Runtime resource profile propagation.
6. Forbidden storage import guards.

### M6T: Engine Product Semantics Tests

Likely sources:

1. `tests/engine/`
2. `tests/integration/`
3. `crates/engine/tests/`
4. `tests/transaction_runtime/`
5. graph/vector/search characterization tests

V1 test focus:

1. KV/JSON/event/vector/graph capability conformance.
2. EntityRef and relationship-layer behavior.
3. Branch create, branch-from-version, branch-from-time, compare, promote,
   copy, restore, revert, cherry-pick, delete.
4. Latest, `getv`, history, and `as_of`.
5. Search/retrieval derived-state freshness.
6. Public write and batch semantics.
7. Removed-surface guards for
   `docs/architecture/v1-removed-surfaces.md`.

Every milestone test epic should map to one of the V1 test types in this
document. If a milestone needs a custom gate that does not map cleanly, its
implementation plan must name the custom gate and why it is necessary.

### M7T: Inference-Next Tests

Likely sources:

1. current inference tests, once inventoried
2. intelligence model lifecycle tests where they actually test provider
   behavior

V1 test focus:

1. No-default build.
2. Feature matrix build.
3. Model-spec parser.
4. Provider request/response mapping.
5. Error class and retry policy mapping.
6. Redaction.
7. Fake providers.
8. Unsafe audit/lifecycle tests for local runtime.

### M8T: Intelligence-Next Tests

Likely sources:

1. `crates/intelligence/tests/`
2. `tests/intelligence/`
3. executor model command tests where they cover intelligence behavior

V1 test focus:

1. Query embedding helpers.
2. Autoembedding queue/reindex/status/failure behavior.
3. Expansion cache.
4. Reranking degradation and score blending.
5. RAG prompt/context/citation behavior.
6. Structured stage diagnostics.
7. Dependency guard: intelligence does not import storage; executor/CLI do not
   import inference directly.

### M9T: Executor/CLI/API Cutover Tests

Likely sources:

1. `tests/executor/`
2. `tests/executor_runtime.rs`
3. `tests/cli_external_suite.py`
4. `tests/cli_external_suite_manifest.json`
5. command serialization tests

V1 test focus:

1. Product command surface.
2. IPC/local command behavior.
3. Read-only write rejection.
4. Structured error serialization.
5. Dataset clone CLI path.
6. Removed command guards.
7. Docs terminology scan.

### M10T: V1 Readiness Tests

Likely sources:

1. `benchmarks/`
2. `crates/engine/benches/`
3. stress and adversarial suites under root tests
4. long-running randomized tests

V1 test focus:

1. Full storage crash/fault matrix.
2. Full product pathway conformance.
3. Runtime resource profile matrix.
4. Benchmarks and regression thresholds.
5. Dependency graph audit.
6. Public API surface audit.
7. Final documentation scan.

## Inventory Record Format

The full inventory lives at:

1. `docs/architecture/v1-test-inventory.md`

M0TE populates that file. It should use a simple markdown table unless the
inventory becomes too large to review comfortably, in which case M0TE may
switch it to a machine-readable file and leave a short markdown summary.

Minimum fields:

| Field | Meaning |
|---|---|
| `path` | Current test file path. |
| `current_area` | Current suite or crate area. |
| `behavior` | Behavior the test protects. |
| `v1_decision` | Required, Optional, Redesign, Remove, or Evidence-only. |
| `action` | Keep, Rewrite, Archive, or Delete. |
| `target_track` | `M1T` through `M10T`, or none for delete/archive. |
| `target_epic` | Optional test epic once planned. |
| `reason` | Short reason for action. |
| `fixtures` | Fixtures or data files used. |
| `notes` | Important migration notes. |

Example:

| path | behavior | v1_decision | action | target_track | reason |
|---|---|---|---|---|---|
| `crates/engine/tests/follower_tests.rs` | follower open/refresh behavior | Remove | Delete | none | Follower mode is not a V1 product path. |
| `tests/durability/wal_lifecycle.rs` | WAL lifecycle behavior | Required | Rewrite | `M3T` | Behavior survives, but storage-next owns WAL service. |
| `tests/executor/session_transactions.rs` | public transaction commands | Remove/Redesign | Archive or Delete | none | Public manual transaction sessions are removed; internal commit tests move to storage/engine tracks. |

## Archive Policy

Archived tests should not run in normal V1 CI.

Canonical archive location:

1. `tests/archive/`

Documentation may link to archived tests from architecture docs, but archived
test source should live under `tests/archive/` when it has ongoing local value.
If the test has no ongoing local value, the inventory may reference only the
historical commit instead of copying the file.

An archived test must include:

1. Why it is archived.
2. What V1 decision replaced it.
3. Whether any V1 test inherited part of its intent.

## Delete Policy

Deleting a test is acceptable when the behavior is explicitly removed.

Deletion should cite one of:

1. Product requirement.
2. Feature inventory decision.
3. Engine/storage architecture decision.
4. Public API cleanup checklist.
5. This inventory plan.

Do not delete a test merely because it fails on the V1 branch. First classify
the behavior.

## Porting Rules

1. Port product guarantees, not old helper names.
2. Assert stable error codes, not old prose.
3. Prefer conformance harnesses over one-off copies.
4. Prefer model/property tests for branch, timeline, table, and visibility
   behavior.
5. Prefer fault/crash tests for durability behavior.
6. Keep fixtures only when their product pathway survives.
7. Rewrite tests that import storage directly from above engine unless they are
   storage tests, benches, fuzz targets, diagnostic tools, or approved
   migration/verification tools.
8. Removed-surface tests should become guard tests that prove the surface is
   gone, not tests that preserve the old behavior.

## First Work Item

The first test-track work item should be:

```text
M0TE: Existing Test Inventory
M0TE1: Generate inventory table from current repository tests
M0TE2: Classify removed-behavior tests
M0TE3: Classify high-value keep/rewrite tests
M0TE4: Assign keep/rewrite tests to M1T-M10T
```

This should happen before substantial V1 implementation starts. It prevents the
old suite from silently dragging obsolete architecture into the new line.

## Acceptance Criteria

This plan is sufficient when:

1. Existing tests are explicitly treated as evidence, not authority.
2. Every milestone has a matching test track.
3. The keep/rewrite/archive/delete actions are defined.
4. Current test locations are mapped to likely V1 tracks.
5. Removed behavior has a clear delete/archive path.
6. High-value behavior has a clear keep/rewrite path.
7. The first inventory epic is defined as `M0TE`.
