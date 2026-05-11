# Engine-Next Target Crate Shape And Test Harness

Status: V1 architecture draft

## Purpose

The engine-next architecture is organized around buckets: API, runtime, commit,
branch, data capability, control plane, orchestration, retrieval, persistence,
diagnostics, IPC, and data movement. The Rust crate should not be organized as
milestone names or historical migrated modules.

This document pins the first target crate shape and test harness model. It is
not a Rust API spec. Exact signatures belong in implementation plans.

## Related Documents

1. `docs/architecture/engine-next-architecture.md`
2. `docs/architecture/engine-next/testing-and-conformance-plan.md`
3. `docs/architecture/engine-next/error-and-diagnostics-contract.md`
4. `docs/architecture/engine-next/primitive-implementation-contract.md`
5. `docs/architecture/engine-next/persistence-adapter-contract.md`
6. `docs/architecture/engine-next/ipc-and-command-boundary-contract.md`

## Package Naming

During parallel development, the package may be called `strata-engine-next` and
live under `crates/engine-next`.

After cutover, the canonical package should be `strata-engine` again. Users
should not learn a permanent `next` name.

Cutover implies removal, not coexistence: the existing `crates/engine` is
removed or archived, `crates/engine-next` is renamed to `crates/engine`, and
package names return to `strata-engine` in the cutover PR series.

## Crate Shape Principles

1. The crate tree should be product-domain-shaped, not cleanup-history-shaped.
2. The public surface should be small and re-exported intentionally.
3. Data capabilities should share repeatable structure.
4. Persistence should be the only normal storage-facing bucket.
5. Branch operations should use capability adapters, not hand-decode every row.
6. Retrieval should consume capability adapters and derived-state manifests.
7. Control-plane records should have stable homes and typed health.
8. Runtime should own open/lifecycle/resource policy, not data semantics.
9. Diagnostics should be a shared pattern, not one error type per helper.
10. Test harnesses should be reusable, deterministic, and explicitly invoked.

## Crate-Level Policy

Rules:

1. Engine-next should use `#![deny(unsafe_code)]` unless an implementation plan
   records a narrow exception.
2. Workspace lints are inherited; local relaxations require a written reason.
3. Public database APIs are synchronous unless a later product architecture
   explicitly introduces async.
4. Panics are bugs. Product failures return typed statuses.
5. Production code above `persistence/` must not import storage-next directly.
6. No public API should expose subsystem-instantiation hooks.
7. Optional model/provider work must be explicit and feature-gated.

## Target Directory Shape

Target shape:

```text
crates/engine-next/
  Cargo.toml
  src/
    lib.rs
    api/
    runtime/
    commit/
    branch/
    data/
      kv/
      json/
      event/
      vector/
      graph/
    entity/
    control/
    orchestration/
    retrieval/
    persistence/
    diagnostics/
    command/
    clone/
    config/
    test_support/
    testkit/
  tests/
    common/
      mod.rs
    api.rs
    runtime.rs
    persistence_adapter.rs
    commit.rs
    data_capability_conformance.rs
    entity_relationships.rs
    branching.rs
    temporal.rs
    control_plane.rs
    orchestration.rs
    retrieval.rs
    command_boundary.rs
    ipc.rs
    clone_artifacts.rs
    errors_and_diagnostics.rs
    product_pathways.rs
    removed_surface_guards.rs
  testdata/
    goldens/
      command/
      cli/
      errors/
      clone-artifacts/
  fuzz/
    Cargo.toml
    fuzz_targets/
```

The exact filenames can change. The important point is that the top-level
modules are engine product domains, and the test targets are harness families.

## Module Ownership

### `api`

Public engine surface and product DTOs.

Owns open options, handle shape, public data capability handles, branch/time
DTOs, health DTOs, and product errors.

Must not own storage keys, WAL/manifest/checkpoint DTOs, data capability
internals, or background job internals.

### `runtime`

Open, lifecycle, resource profile, access mode, scheduler ownership, default
branch bootstrap, same-path reuse, shutdown, and IPC fallback classification.

Must not own IPC transport, storage backend IO, branch merge semantics, or
model provider execution.

### `commit`

Internal unit of change.

Owns commit context, atomic engine batches, version/timestamp coordination,
commit guards, conflict checks, observer dispatch, derived commit tagging, and
commit-outcome diagnostics.

Must not expose public begin/commit/rollback workflows as V1 product surface.

### `branch`

Product branch semantics.

Owns branch names, lifecycle, DAG product model, fork, branch-from-time/version,
diff, promote/merge, copy/cherry-pick, restore/revert, conflict strategy, and
branch audit/control records.

Must use capability adapters for capability-specific interpretation.

### `data`

Data capability implementations over the branch-aware MVCC KV row substrate.

Submodules:

```text
data/
  kv/
  json/
  event/
  vector/
  graph/
```

Each capability should use the same internal pattern: facade, semantic types,
entity addressing, key/value codec, read path, write path, branch adapter,
retrieval adapter, relationship adapter where applicable, and diagnostics.

### `entity`

EntityRef, relationship binding support, canonical URI/string formatting,
entity validation, and relationship-layer shared helpers.

This module does not own graph storage or traversal. Graph owns graph facts;
entity owns cross-capability identity rules.

### `control`

Global `_system_` branch and branch-local `_system_` space layout.

Owns storage-space registry consumption, capability registry, recipe registry,
dataset/provenance records, branch-local projection manifests, watermarks,
derived-state health, and control-plane validation.

### `orchestration`

Cross-capability workflows and derived work.

Owns autoembedding coordination, shadow vector maintenance, graph relationship
coordination, projection/rebuild jobs, derived-state repair, and watermarks.

Must not hide cross-capability writes inside capability CRUD methods.

### `retrieval`

Deterministic retrieval over data capabilities and derived state.

Owns recipes, BM25/text retrieval, vector/hybrid fusion, graph-aware retrieval,
query expansion/rerank configuration, provenance, freshness policy, and search
diagnostics.

Model provider execution belongs above engine.

### `persistence`

Only normal storage-facing engine module.

Owns row address construction, storage-space ID resolution, storage read/write
selectors, commit plan adaptation, branch mechanic adaptation, timeline calls,
storage health adaptation, and storage-error mapping.

### `diagnostics`

Shared status, health, metrics, redaction, and error mapping helpers.

This module should keep the concept set small and align with
`docs/architecture/v1-error-and-diagnostics-contract.md`.

### `command`

Serializable command classification and command-boundary DTOs when those live
inside engine-next.

Owns read/write/maintenance classification, access-mode requirements, command
schema versioning facts, and command error status mapping. Executor/CLI may
still own transport and presentation.

### `clone`

`.strata` artifact validation and materialization policy.

Owns artifact manifest interpretation, provenance mapping, derived-state omit
or rebuild decisions, and partial materialization cleanup policy. Fetching from
a URL may live in CLI/SDK if that keeps engine network-neutral.

### `config`

Engine product configuration and resolved runtime policy.

Owns user-facing config DTOs, runtime-resource profile inputs, defaults,
capability enablement flags, and conversion into lower-layer storage/runtime
budgets. It must not become a second lifecycle module or a place for hidden
storage/backend decisions.

## Test Harness Shape

Engine-next should provide two test-support levels:

1. `test_support/`: crate-private helpers for unit tests.
2. `testkit/`: feature-gated cross-crate harness for integration and product
   conformance tests.

Feature-gated testkit APIs must be documented as test-only and must not become a
second product API.

Required harnesses:

1. Fake persistence.
2. Faulting persistence wrapper.
3. Deterministic clock/version source.
4. Data capability conformance harness.
5. Branch and temporal model harness.
6. Command and IPC golden harness.
7. Derived-state harness.
8. Clone artifact fixture builder.
9. Error/status assertion helpers.
10. Redaction assertion helpers.

## Forbidden Shapes

Engine-next should not introduce:

1. `graph/`, `vector/`, or `search/` as top-level peer crates or top-level
   architecture buckets.
2. One trait per branch operation per capability.
3. One error enum per helper method.
4. Public subsystem-instantiation hooks.
5. Storage imports outside `persistence/` in production code.
6. Public manual transaction-session architecture.
7. Follower-mode lifecycle paths.
8. Hidden network/provider work in engine.
9. Product docs that expose WAL, manifest, table, segment, or compaction
   internals as normal user concepts.

Current-code evidence sections may still mention historical `crates/graph`,
`crates/vector`, or search module paths. Those references are descriptive only.
The target crate shape folds graph and vector into `data/` capability modules
and search into `retrieval/`.

## Acceptance Criteria

This document is satisfied when:

1. Engine-next implementation plans use this domain-shaped crate tree.
2. Every new public module has a product or architecture reason.
3. Data capabilities share the same implementation pattern.
4. Persistence is the only normal storage-facing path.
5. The testkit supports the engine testing and conformance plan.
6. Removed cleanup-era concepts do not re-enter as permanent module names.
