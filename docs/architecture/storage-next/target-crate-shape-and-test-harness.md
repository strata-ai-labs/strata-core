# Storage-Next Target Crate Shape And Test Harness

Status: V1 architecture draft

## Purpose

The storage-next architecture is described as L1-L9 because layers are the right
way to reason about responsibility and failure boundaries. The Rust crate should
not be organized as `l1`, `l2`, `l3`, and so on.

The crate should be organized around durable storage concepts:

1. Backends.
2. Object names and layout.
3. Durable byte formats.
4. Durable services.
5. Tables.
6. Branch-local LSM state.
7. Commit runtime.
8. Lifecycle, recovery, and maintenance.
9. Engine-facing API.

This document defines the target crate shape and the test harness model. It is
not a Rust API spec. Exact signatures belong in the layer implementation plans.

## Related Documents

1. `docs/architecture/storage-next-architecture.md`
2. `docs/architecture/storage-next/implementation-patterns.md`
3. `docs/architecture/v1-testing-and-conformance-plan.md`
4. `docs/architecture/v1-error-and-diagnostics-contract.md`
5. `docs/architecture/storage-next/l9-storage-api-boundary.md`
6. `docs/architecture/storage-next/storage-space-id-registry.md`
7. `docs/architecture/storage-next/commit-timeline-substrate.md`
8. `docs/architecture/v1-engineering-standards.md`

## Package Naming

During parallel development, the package may be called `strata-storage-next` and
live under `crates/storage-next`.

After cutover, the canonical package should be `strata-storage` again. Users
should not learn a permanent `next` name.

Examples in this document use `strata-storage-next` for clarity. Replace that
with `strata-storage` after cutover.

Cutover implies removal, not coexistence: the existing `crates/storage` is
removed, `crates/storage-next` is renamed to `crates/storage`, and package names
return to `strata-storage` in the cutover PR series.

## Crate Shape Principles

1. The crate tree should be domain-shaped, not layer-number-shaped.
2. The public surface should be small and re-exported from `lib.rs`.
3. Lower durable mechanics should not import product semantics.
4. Raw filesystem calls should live only in the local filesystem backend.
5. Backends should be capability-driven, not feature-name-driven.
6. Formats should not perform IO.
7. Tables should not know branch product semantics.
8. Branch mechanics should not know JSON, graph, vector, search, event, or
   intelligence meaning.
9. Commit runtime should not expose public user transaction sessions.
10. Lifecycle should orchestrate lower services without owning product policy.
11. Test harnesses should be reusable, deterministic, and explicitly invoked.

## Standards Application

This crate-shape document applies
`docs/architecture/v1-engineering-standards.md` to storage-next.

Rules:

1. L1-L9 labels are architecture-reading aids only. They must not become
   module names, file names, type names, feature flags, tests, metrics, errors,
   or comments inside production code.
2. The target module names in this document are permanent storage-domain names:
   `backend`, `object`, `layout`, `row`, `format`, `service`, `table`,
   `branch`, `commit`, `lifecycle`, `observability`, `error`, `config`, `api`,
   `test_support`, and `testkit`.
3. Temporary `strata-storage-next` and `crates/storage-next` names are
   build-branch scaffolding only. Cutover removes the suffix; code inside the
   crate should already use permanent domain vocabulary.
4. Public and crate-wide types should use the standards suffixes such as
   `Id`, `Address`, `Key`, `Options`, `Config`, `Plan`, `Record`, `Entry`,
   `Facts`, `Outcome`, `Stats`, `Report`, and `Error`.
5. Test target names must describe behavior or conformance families, not
   roadmap labels.
6. The word "helper" may appear only as generic prose for private or test
   support. Do not create production types or modules named `Helper`.

## Crate-Level Policy

Storage-next should start with strict crate-level policy rather than inheriting
cleanup-era compromises.

Rules:

1. The crate root should use `#![deny(unsafe_code)]`.
2. Workspace lints are inherited; storage-next should not add local lint
   relaxations unless an implementation plan records the reason.
3. Public storage APIs must be synchronous and must not expose `async`,
   `Future`, tokio, async-std, or runtime-specific types.
4. Panics are bugs. Ordinary invalid input, corruption, unsupported
   capabilities, IO failures, and recovery failures must return typed errors.
5. Direct filesystem calls are allowed only in the local filesystem backend.
6. Durable format behavior must not depend on cargo features.

## Target Directory Shape

Target shape:

```text
crates/storage-next/
  Cargo.toml
  src/
    lib.rs
    api/
    backend/
    object/
    layout/
    row/
    format/
    service/
    table/
    branch/
    commit/
    lifecycle/
    observability/
    error/
    config/
    test_support/
    testkit/
  tests/
    common/
      mod.rs
    backend_conformance.rs
    object_layout_properties.rs
    format_golden.rs
    service_fault_windows.rs
    table_properties.rs
    branch_lsm_properties.rs
    timeline_properties.rs
    commit_runtime_faults.rs
    lifecycle_recovery.rs
    api_conformance.rs
    crash_recovery.rs
    stress.rs
  testdata/
    goldens/
      storage-format-v1/
  fuzz/
    README.md
    fuzz_targets/
      README.md
```

The exact filenames can change. The important point is that the top-level
modules are durable storage domains, and the test targets are harness families.

## Module Ownership

### `api`

Engine-facing storage boundary.

Owns:

1. `Storage` or equivalent runtime handle.
2. Open/create entry points.
3. Engine-facing read, commit, branch-mechanic, checkpoint, recovery, close,
   health, metrics, and maintenance methods.
4. Public storage boundary DTOs.

Must not own:

1. Backend implementation details.
2. Durable byte format parsers.
3. Engine data capability semantics.
4. Product error wording.

Maps to:

1. L9 directly.
2. L8 through lifecycle runtime.
3. L7 through commit runtime.
4. L6 through branch runtime.

### `backend`

Backend IO contract and backend implementations.

Owns:

1. Backend trait or equivalent backend contract.
2. Backend capability declarations.
3. Backend metadata and conditional publish facts.
4. Memory/cache backend.
5. Local filesystem backend.
6. Future OpenDAL backend family, when implemented.

Must not own:

1. Strata object-family policy.
2. WAL semantics.
3. Manifest semantics.
4. Table semantics.
5. Recovery policy.

Maps to L1.

Target submodule shape:

```text
backend/
  mod.rs       # trait, capabilities, metadata, shared backend errors
  memory.rs    # always-available memory/cache backend
  local_fs.rs  # local filesystem backend; only place std::fs is allowed
```

A future OpenDAL adapter should be added as its own backend submodule only when
it has a real implementation and honest capability reporting.

### `object`

Validated object names and prefixes.

Owns:

1. `ObjectName`.
2. `ObjectPrefix`.
3. Low-level validation rules.
4. Backend-safe namespace mapping primitives.

Must not own:

1. Database object family policy.
2. Filesystem calls.
3. Durable format bytes.

Maps to the primitive part of L2.

### `layout`

Database object layout.

Owns:

1. Object families such as manifest, WAL, table, snapshot, temporary,
   quarantine, and writer lock/lease.
2. Constructors that produce validated `ObjectName` and `ObjectPrefix` values.
3. The rule that object names do not carry a storage format version prefix by
   default.

Must not own:

1. Backend IO.
2. Durable byte parsing.
3. Recovery policy.

Maps to the database-layout part of L2.

### `row`

Generic storage row model.

Owns:

1. Physical storage key representation.
2. Opaque storage space/family ID.
3. Row value bytes.
4. Tombstone metadata.
5. TTL metadata, if retained in storage row metadata.
6. Version and timestamp fields needed by storage.

Must not own:

1. `EntityRef`.
2. JSON paths.
3. Graph node/edge semantics.
4. Vector collection semantics.
5. Event stream semantics.
6. Search document semantics.

Maps across L3, L6, L7, and L9.

`row` owns the in-memory storage row shape manipulated by L5, L6, and L7.
`format` owns durable byte encoding for those rows and keys. Conversion happens
at durable boundaries such as table build/read, WAL encode/decode, and snapshot
encode/decode.

### `format`

Durable byte encoders and decoders.

Owns:

1. WAL segment and record formats.
2. Commit payload format.
3. Manifest format.
4. Snapshot container and section envelope format.
5. Table byte format.
6. Storage row and internal key durable encoding.
7. Compression and checksum framing.
8. Optional encryption codec framing if retained.

Must not own:

1. Backend IO.
2. Recovery policy.
3. Engine primitive DTOs.
4. Product error wording.

Maps to L3.

### `service`

Durable object services over backend, layout, and format.

Owns:

1. Durable publisher.
2. WAL service.
3. Manifest service.
4. Snapshot service.
5. Table object publication service.
6. Service-level fault windows and typed publish outcomes.

Must not own:

1. Table algorithms.
2. Branch visibility.
3. Commit ordering.
4. Product checkpoint UX.

Maps to L4.

### `table`

Mutable and immutable table mechanics.

Owns:

1. Mutable table.
2. Frozen table view.
3. Immutable table builder.
4. Immutable table reader.
5. Block reader.
6. Index and filter mechanics.
7. Block cache with explicit database ownership.
8. Raw point lookup.
9. Raw prefix and range cursors.
10. Sorted merge cursor.
11. Generic table compaction.

Must not own:

1. Branch topology.
2. Inherited layers.
3. WAL commit ordering.
4. Recovery orchestration.
5. User maintenance commands.

Maps to L5.

### `branch`

Branch-local LSM mechanics.

Owns:

1. Branch-local active and frozen table ownership.
2. Branch-local immutable level ownership.
3. Latest, version-bounded, timestamp-bounded, history, prefix, and range
   visibility over storage rows.
4. COW inherited layers.
5. Fork-version gates.
6. Inherited key rewriting.
7. Materialization mechanics.
8. Branch-safe tombstone behavior.
9. Shared table reachability facts.
10. Branch-local compaction state transitions.

Must not own:

1. Product merge/cherry-pick/revert semantics.
2. Primitive-aware diff.
3. IPC.
4. Product branch UX.

Maps to L6.

### `commit`

Internal commit runtime.

Owns:

1. `CommitBatch` or equivalent internal commit unit.
2. Single-branch mutating commit path.
3. Commit-version allocation.
4. Commit timestamp assignment.
5. Commit timeline persistence through `commit/timeline.rs` or an equivalent
   commit-owned submodule.
6. Optional read-set/CAS validation facts.
7. Per-branch commit guard.
8. Commit quiesce guard.
9. WAL-before-visible ordering through `service`.
10. Visible-version publication after branch apply.
11. Durable-but-not-visible and ambiguous-commit classification.
12. Recovery catch-up for commit version, timeline, and transaction ID
    allocators.

Must not own:

1. Public transaction sessions.
2. User-facing ACID transaction commands.
3. Engine primitive write sets.
4. IPC command handling.

Maps to L7.

The commit timeline is a storage-owned system row family under
`storage_space_id = 0x01`. The commit module owns writing timeline rows as part
of the internal commit unit. `row` owns the in-memory row shape, `format` owns
the durable encoding, and `lifecycle` owns recovery validation/catch-up.

### `lifecycle`

Open, recovery, checkpoint, maintenance, retention, quarantine, and close.

Owns:

1. Storage open/create sequencing.
2. Recovery replay orchestration.
3. Recovery health facts.
4. Checkpoint execution.
5. Snapshot retention and pruning.
6. Flush scheduling.
7. Compaction scheduling.
8. Materialization scheduling.
9. Quarantine, reclaim, and purge protocol.
10. Maintenance executor.
11. Close and shutdown ordering.

Must not own:

1. Product open policy.
2. IPC.
3. Product recovery wording.
4. Primitive snapshot meaning.

Maps to L8.

### `observability`

Raw storage metrics, health facts, and diagnostic facts.

Owns:

1. Storage health facts.
2. Recovery facts.
3. Maintenance facts.
4. Table/cache metrics.
5. Backend capability facts.
6. Diagnostic counters.

Must not own:

1. Product display text.
2. CLI rendering.
3. StrataHub business logic.

Maps across L8 and L9.

### `error`

Storage-local parent error and shared storage error classification.

Owns:

1. Storage-local error categories.
2. Source chains.
3. Mechanical failure detail.
4. Mapping helpers inside storage.

Must not own:

1. `StrataError`.
2. Product error text.
3. Executor or IPC status.

Maps across all layers and must align with
`docs/architecture/v1-error-and-diagnostics-contract.md`.

The module should be a directory, not a single `error.rs` file. The V1 error
contract needs enough structure for storage-local errors, stable
classifications, source chains, detail/context fields, and classification
helpers that engine mapping code can consume without turning one file into a
dumping ground.

### `config`

Storage runtime configuration after validation.

Owns:

1. Storage mode.
2. Backend selection.
3. Runtime limits.
4. Cache sizing.
5. Durability knobs.
6. Maintenance knobs.
7. Fault/test options behind test features only.

Must not own:

1. Engine product open policy.
2. CLI config parsing.
3. User-facing default explanations.

The module should be a directory, not a single `config.rs` file. Runtime
configuration, backend selection, cache sizing, durability knobs, pressure
control, rate limiting, memory accounting, and test/fault options should have
clear submodules instead of one large configuration file.

Storage config consumes resolved storage runtime budgets. It must not probe
host RAM, inspect CPU counts, classify devices, choose product resource
profiles, or mutate user-facing config defaults. Those responsibilities belong
to the engine-owned resource planner described in
[runtime-resource-profile-architecture.md](../runtime-resource-profile-architecture.md).

## Dependency Direction

The crate should keep dependencies broadly acyclic:

```text
api
  -> lifecycle
  -> commit
  -> branch
  -> table
  -> service
  -> format
  -> layout/object
  -> backend
```

This diagram is directional, not literal. Some shared modules sit to the side:

```text
row, error, config, observability
```

`strata-core-next` is the only Strata crate storage-next may depend on. The
dependency should be added only when implementation code actually needs shared
identifiers or representation types such as branch IDs, commit versions,
transaction IDs, timestamps, and transparent newtypes. Storage-next must not
depend on engine-next or any product crate.

Allowed cross-links:

1. `backend` may accept `object::ObjectName` as an opaque validated name.
2. `service` may call `backend`, `layout`, and `format`.
3. `table` may use `format`, `row`, and service-provided object readers.
4. `branch` may use `table`, `row`, and reachability facts.
5. `commit` may use `branch`, `service`, `row`, and commit payload formats.
6. `lifecycle` may orchestrate `service`, `table`, `branch`, and `commit`.
7. `api` may call `lifecycle`, `commit`, and `branch` through stable storage
   runtime methods.

Disallowed cross-links:

1. `format` must not call `backend`.
2. `table` must not call local filesystem APIs.
3. `branch` must not call engine code.
4. `commit` must not import engine primitive DTOs.
5. `lifecycle` must not import executor, CLI, intelligence, inference, IPC, or
   StrataHub code.
6. Upper crates above engine-next must not import storage-next in normal
   production code.

## Public Surface Shape

`lib.rs` should re-export a small boundary.

Likely public families:

1. Open/config:
   - `StorageOpenOptions`
   - `StorageMode`
   - `StorageBackendAddress`
   - `StorageCapabilities`

2. Runtime:
   - `Storage`
   - `StorageResult`
   - `StorageError`

3. Commit/read:
   - `CommitBatch`
   - `CommitOutcome`
   - `ReadBound`
   - `HistoryEntry`
   - `ScanOutcome`

4. Branch mechanics:
   - `StorageBranchId` or core-owned branch ID if selected.
   - `BranchCreatePlan`
   - `BranchDeleteOutcome`
   - `MaterializeOutcome`

5. Lifecycle:
   - `OpenOutcome`
   - `RecoveryFacts`
   - `RecoveryHealth`
   - `MaintenanceFacts`
   - `CloseOutcome`

6. Observability:
   - `StorageHealth`
   - `StorageMetrics`

These names are directional. The implementation plan should keep the final list
short. The test is whether engine-next can consume storage without importing
internal modules.

## Feature Flags

Target feature flags:

1. `default = ["localfs"]`.
   Native default includes the local filesystem backend.

2. `localfs`.
   Enables the local filesystem backend and all direct `std::fs` use. This
   feature is not compatible with `wasm32-unknown-unknown`.

3. Memory/cache backend.
   Always available and not gated behind a cargo feature. Browser/WASM builds
   should use `default-features = false` to avoid compiling `localfs`.

4. `testkit`.
   Exposes test harness helpers to integration tests, fuzz targets, and engine
   product tests. It must not be enabled by normal production dependencies.

5. `fault-injection`.
   Enables deterministic fault hooks. It should imply `testkit` if the hooks
   are needed outside crate unit tests.

6. `perf-trace`.
   Optional low-level tracing and counters.

Do not add an `opendal` feature until an actual adapter implementation exists.
If the feature is reserved before implementation, enabling it must fail loudly
with `compile_error!` rather than silently doing nothing.

Avoid:

1. `engine-internal`.
   Engine should consume L9, not private storage internals.

2. Feature flags that change durable format.
   Durable format is selected by the format spec and database metadata, not
   cargo features.

## Test Harness Shape

Storage-next should use three test harness scopes.

### Private Unit Test Support

Location:

```text
module-local #[cfg(test)] mod tests
src/test_support/
```

Compiled with:

```rust
#[cfg(test)]
```

Purpose:

1. Internal model builders.
2. Private invariant checkers.
3. Module-local fixtures.
4. Storage-row generators used by in-crate tests.

Private test support can access crate-private internals. It is not part of the
public or feature-gated surface. Prefer module-local `#[cfg(test)] mod tests`
for small fixtures; use `src/test_support/` only for shared private helpers.

### Feature-Gated Integration Testkit

Location:

```text
src/testkit/
```

Compiled with:

```rust
#[cfg(any(test, feature = "testkit"))]
```

Purpose:

1. Backend conformance harness.
2. Fault backend.
3. Crash harness helpers.
4. Golden-vector fixture loaders.
5. Error assertion helpers.
6. Public L9 conformance helpers.

Rules:

1. It must be clearly named `testkit`.
2. It must be unavailable in normal production builds.
3. It must not become a second storage API.
4. Engine-next may use it only in tests or fault-injection builds.
5. Any helper needed in production should move into a real storage module with a
   documented owner.
6. Public testkit items should be `#[doc(hidden)]`.
7. Public testkit items should be marked deprecated with a clear test-only note
   unless Rust tooling makes that too noisy for test builds.

### Integration Test Support

Location:

```text
tests/common/
```

Purpose:

1. Black-box helpers that use only public or feature-gated storage APIs.
2. Temp-directory management.
3. Test backend selection.
4. Process-level crash orchestration.
5. Shared command-line/env parsing for ignored stress tests.

Integration test support should not reach into private modules.

Shared integration helpers should live in `tests/common/mod.rs` and be included
from each integration test with `mod common;`. Do not create `tests/common.rs`,
which would compile as its own integration-test binary.

## Test Target Shape

Recommended integration test targets:

1. `backend_conformance.rs`
   Backend contract tests for memory/cache and local filesystem.

2. `object_layout_properties.rs`
   Object name and prefix property tests.

3. `format_golden.rs`
   Durable byte golden vectors.

4. `service_fault_windows.rs`
   Durable publisher, WAL, manifest, snapshot, and table publication failure
   windows.

5. `table_properties.rs`
   Table builder/reader/cursor/compaction property tests.

6. `branch_lsm_properties.rs`
   Branch visibility, inherited layers, materialization, retention, and
   reachability property tests.

7. `timeline_properties.rs`
   Timestamp-to-version and version-to-timestamp resolution, duplicate
   timestamp ordering, retention gaps, compaction preservation, and recovery
   catch-up.

8. `commit_runtime_faults.rs`
   WAL-before-visible, durable-but-not-visible, ambiguous commit, commit guard,
   commit timeline persistence, and replay behavior.

9. `lifecycle_recovery.rs`
   Open, recovery, checkpoint, timeline corruption/rebuild, retention,
   quarantine, maintenance, and close.

10. `api_conformance.rs`
   L9 engine-facing boundary conformance tests.

11. `crash_recovery.rs`
    Process-level crash/reopen tests. Mark slow cases `#[ignore]`.

12. `stress.rs`
    Long-running randomized tests. Mark all tests `#[ignore]`.

Unit tests can live beside their modules. The test targets above exist to keep
cross-module contracts visible and invokable.

## Fuzz Target Shape

M2 may keep `fuzz/` as documentation-only scaffolding. Do not create
`fuzz/Cargo.toml` until storage-next has a byte parser, durable codec, or
state-machine harness worth fuzzing.

Once cargo-fuzz is used, storage-next should own a `fuzz/` package with targets
named by byte-oriented durable input, not layer number.

Suggested targets:

1. `object_name_parse`
2. `format_wal_record`
3. `format_manifest`
4. `format_snapshot_envelope`
5. `format_commit_payload`
6. `format_table_block`
7. `format_timeline_row`
8. `recovery_object_inventory`
9. `api_command_payload` after the command boundary is defined.

Fuzz targets should fail on panic, unbounded allocation, unexpected successful
decode, and wrong error class.

Model-based state exploration belongs in property tests unless an implementation
plan deliberately builds an `arbitrary`-driven fuzz harness. In particular,
branch visibility and table cursor state should start in
`branch_lsm_properties.rs` and `table_properties.rs`, not as default cargo-fuzz
targets.

## Golden Vector Shape

Golden vectors should live under:

```text
crates/storage-next/testdata/goldens/storage-format-v1/
```

Guidelines:

1. Golden files are checked in.
2. Tests verify goldens by default.
3. Tests never rewrite goldens during normal execution.
4. Regeneration requires an explicit command.
5. Golden fixture metadata records format name, version, codec, checksum, and
   test purpose.

Since the repo does not currently have an `xtask` crate, the first
implementation can use a focused ignored test or small binary for regeneration.
If regeneration grows beyond one format, add an `xtask` wrapper.

## Manual Invocation Model

This section defines local developer invocation. CI can mirror these commands
later, but CI policy should be written separately.

### Fast Storage Check

Run the normal storage-next test set:

```bash
cargo test -p strata-storage-next
```

After cutover:

```bash
cargo test -p strata-storage
```

### All Non-Ignored Storage Tests

Run all non-ignored targets with test features:

```bash
cargo test -p strata-storage-next --features testkit,fault-injection --all-targets
```

### Backend Conformance

Run all backend conformance cases:

```bash
cargo test -p strata-storage-next --features testkit --test backend_conformance
```

Run a specific backend:

```bash
STRATA_STORAGE_TEST_BACKEND=memory \
  cargo test -p strata-storage-next --features testkit --test backend_conformance

STRATA_STORAGE_TEST_BACKEND=localfs \
  cargo test -p strata-storage-next --features testkit --test backend_conformance
```

### Property Tests

V1 storage-next uses `proptest` for property tests by default. The
`PROPTEST_CASES` environment variable is therefore part of the local developer
workflow unless a later implementation plan deliberately changes frameworks.

Run normal property tests:

```bash
cargo test -p strata-storage-next --test object_layout_properties
cargo test -p strata-storage-next --test table_properties
cargo test -p strata-storage-next --test branch_lsm_properties
cargo test -p strata-storage-next --test timeline_properties
```

Increase generated cases locally:

```bash
PROPTEST_CASES=4096 cargo test -p strata-storage-next --test branch_lsm_properties
```

### Fault-Window Tests

Run deterministic fault-window tests:

```bash
cargo test -p strata-storage-next \
  --features testkit,fault-injection \
  --test service_fault_windows

cargo test -p strata-storage-next \
  --features testkit,fault-injection \
  --test commit_runtime_faults
```

### Crash-Recovery Tests

Crash tests should be ignored by default and run explicitly:

```bash
cargo test -p strata-storage-next \
  --features testkit,fault-injection \
  --test crash_recovery \
  -- --ignored --test-threads=1 --nocapture
```

Useful local options:

```bash
STRATA_STORAGE_KEEP_TEST_DIR=1
STRATA_STORAGE_CRASH_CASES=128
STRATA_STORAGE_TEST_ROOT=/tmp/strata-storage-tests
```

Process-level crash tests should use `std::process::Command` to spawn a child
test runner, stop it at marked crash points, and reopen the database in the
parent. Unix can use signals; Windows needs an explicit `TerminateProcess` path
or equivalent. The implementation plan must account for that portability
difference.

### Stress Tests

Stress tests should be ignored by default:

```bash
cargo test -p strata-storage-next \
  --features testkit,fault-injection \
  --test stress \
  -- --ignored --nocapture
```

Useful local options:

```bash
STRATA_STORAGE_STRESS_SEED=12345
STRATA_STORAGE_STRESS_SECONDS=60
```

### Fuzz Tests

If cargo-fuzz is used:

```bash
cargo +nightly fuzz run format_wal_record
cargo +nightly fuzz run format_manifest
cargo +nightly fuzz run format_table_block
```

Corpus and artifact directories should stay under the fuzz package defaults
unless a later fuzzing plan changes them.

### Wasm Cache Check

The memory/cache backend should compile on `wasm32-unknown-unknown` with local
filesystem support disabled:

```bash
cargo check -p strata-storage-next --no-default-features --target wasm32-unknown-unknown --all-targets
cargo test -p strata-storage-next --test testkit_boundary localfs_feature_is_rejected_for_wasm_builds
```

The V1 gate is compile-only until a wasm test runner is chosen. The important
constraint is that `localfs` is not compiled into the wasm build. A wasm build
with default features must fail clearly rather than silently compiling the local
filesystem backend.

### Golden Vector Checks

Default check:

```bash
cargo test -p strata-storage-next --test format_golden
```

Regeneration should be explicit. The exact command can be chosen during L3
implementation. Acceptable first forms:

```bash
cargo test -p strata-storage-next --test format_golden -- --ignored --nocapture
```

or, if an `xtask` crate is introduced later:

```bash
cargo run -p xtask -- storage-goldens update
```

The implementation plan must choose one and document it before the first golden
vectors land.

## Environment Variables

Standard local test environment variables:

| Variable | Purpose |
| --- | --- |
| `STRATA_STORAGE_TEST_BACKEND` | Select `memory`, `localfs`, or future backend for conformance tests. |
| `STRATA_STORAGE_TEST_ROOT` | Override temp root for local filesystem and crash tests. |
| `STRATA_STORAGE_KEEP_TEST_DIR` | Preserve temporary test directories for debugging. |
| `STRATA_STORAGE_CRASH_CASES` | Limit or expand crash-case count. |
| `STRATA_STORAGE_STRESS_SEED` | Reproduce stress/randomized failures. |
| `STRATA_STORAGE_STRESS_SECONDS` | Bound stress test runtime. |
| `PROPTEST_CASES` | Standard proptest case count override. |

Tests should print the seed, backend, and temp directory when a failure occurs.

## Benchmark Boundary

Performance benchmarks remain governed by the workspace benchmark harness,
currently the separate benchmarks workspace/package. Storage-next should not add
its own `benches/` directory until a focused benchmark plan exists. Existing
project benchmark obligations, including comparison and workload benchmarks
called out by repo policy, remain binding unless that policy is updated.

## CI Boundary

This document intentionally defines local/manual invocation, not CI policy.

When CI is written, it should classify these commands into tiers:

1. Per-PR fast tests.
2. Per-PR storage conformance.
3. Per-PR wasm cache compile/conformance once the wasm runner exists.
4. Nightly crash and stress tests.
5. Scheduled fuzzing.
6. Release-gate golden vector and format compatibility checks.

CI should call the same underlying test targets. It should not require a
separate hidden harness.

## Anti-Patterns

Avoid:

1. `src/l1`, `src/l2`, `src/l3` module names.
2. A revived `segmented` mega-module.
3. A revived `durability` mega-module.
4. Raw `std::fs` outside `backend::local_fs`.
5. Test-only production APIs hidden behind vague feature names.
6. Engine importing storage internals through a feature flag.
7. Separate fault systems per service.
8. Golden tests that rewrite fixtures during normal runs.
9. Crash tests that run by default in the fast test suite.
10. Stress tests without reproducible seeds.
11. Tests asserting display strings instead of error codes and classes.

## Acceptance Criteria

The crate shape is ready for implementation planning when:

1. L1-L9 are represented by domain modules, not layer-number modules.
2. `lib.rs` has a small engine-facing public surface.
3. Backend, object layout, format, service, table, branch, commit, lifecycle,
   and API responsibilities are distinct.
4. The dependency direction has no cycles that require hidden compatibility
   layers or temporary connector modules.
5. The testkit is explicit and unavailable in normal production builds.
6. Backend conformance, fault-window, crash, fuzz, golden, property, and stress
   tests have documented invocation paths.
7. No normal production crate above engine-next needs to import storage-next
   internals.
