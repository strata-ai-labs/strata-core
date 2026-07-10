# L6A Implementation Plan: Branch Runtime Scaffold

Status: draft implementation plan

Parent plan:
`docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-implementation-plan.md`

Test plan:
`docs/architecture/implementation-plans/M4/L6/l6a-branch-runtime-scaffold-test-plan.md`

## Objective

Create the storage-next branch-runtime scaffold without implementing branch
read behavior yet.

L6A establishes:

1. the `crates/storage-next/src/branch/` module shape;
2. branch-local configuration shells;
3. branch-runtime error vocabulary;
4. branch state/read/result/fact shells;
5. source-boundary guards for L6 purity;
6. a generated-property harness stub for later L6 slices;
7. the first M4-L6 porting-log entry.

L6A should make later L6B-L6L work easier without smuggling in commit runtime,
lifecycle orchestration, product DTOs, or old `SegmentedStore` structure.

## Inputs

1. `docs/architecture/storage/l6-branch-isolated-lsm-runtime.md`
2. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-implementation-plan.md`
3. `docs/architecture/implementation-plans/m4-l6-branch-lsm-runtime-test-plan.md`
4. `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md`
5. `crates/storage-next/src/branch/mod.rs`
6. `crates/storage-next/src/row/mod.rs`
7. `crates/storage-next/src/table/mod.rs`
8. `crates/storage/src/segmented/mod.rs`
9. `crates/storage/src/key_encoding.rs`
10. `crates/storage/src/memtable.rs`
11. `crates/storage/src/merge_iter.rs`
12. `crates/storage/src/seekable.rs`

## Existing-Code Source Map

| Current file | L6A evidence | L6A action |
|---|---|---|
| `crates/storage/src/segmented/mod.rs` | Names and responsibilities for `BranchState`, `BranchSnapshot`, inherited layers, version/timestamp facts, and branch metrics. | Extract vocabulary into storage-next branch shells; do not port behavior yet. |
| `crates/storage/src/key_encoding.rs` | Branch id is part of physical storage key and commit version orders newest first. | Record ordering facts in comments/tests only; concrete helpers land in L6B. |
| `crates/storage/src/memtable.rs` | Active/frozen state roles and branch-local row-chain facts. | Reference L5 `MutableTable`/`FrozenTable` as future dependencies; do not build branch state yet. |
| `crates/storage/src/merge_iter.rs` | MVCC selection and history behavior. | Reserve read-bound/result vocabulary; no MVCC implementation in L6A. |
| `crates/storage/src/seekable.rs` | Inherited key rewriting and fork gates. | Reserve inherited-layer status/source vocabulary; concrete rewrite helpers land in L6B/L6F. |
| `crates/storage/src/segmented/ref_registry.rs` | Runtime shared-table registry. | Reserve reachability/ref fact vocabulary; implementation lands in L6I. |

## Scope

L6A implements scaffolding only:

1. branch module submodule declarations;
2. branch runtime error/result type;
3. branch runtime config shell;
4. branch read-bound enum shell;
5. branch read-result/fact shells;
6. branch state/table/inherited-layer descriptor shells;
7. branch runtime stats/facts shells;
8. source guard tests;
9. small unit tests for construction, display, and field access;
10. testkit/property harness placeholders with nonzero counters;
11. porting-log entry.

L6A does not implement:

1. row-key validation or branch-id rewriting;
2. append committed rows;
3. latest/getv/as-of/history reads;
4. prefix/range scans;
5. active/frozen rotation;
6. immutable table install;
7. fork or inherited-layer read behavior;
8. materialization;
9. reachability/refcount registry behavior;
10. branch compaction;
11. snapshot row install;
12. commit-version allocation;
13. WAL-before-visible discipline;
14. lifecycle/recovery orchestration;
15. public API exposure.

## Module Layout

Target initial layout:

```text
crates/storage-next/src/branch/
  mod.rs
  config.rs
  error.rs
  facts.rs
  read.rs
  state.rs
  tests.rs
```

The exact split may change during implementation, but L6A should avoid one
large `mod.rs`. The scaffold should leave clear ownership for later slices:

1. `config.rs`: L6 tuning and limits.
2. `error.rs`: typed branch runtime errors and `BranchRuntimeResult`.
3. `facts.rs`: branch/table/reachability/read stats and durable fact shells.
4. `read.rs`: read-bound and selected-row/result vocabulary.
5. `state.rs`: `BranchState`/`BranchView`/`InheritedLayer` shells.
6. `tests.rs`: module-local scaffold tests.

## Proposed Type Surface

Names may change if responsibilities remain intact. All production types stay
`pub(crate)`.

### `BranchRuntimeConfig`

Suggested fields:

```text
BranchRuntimeConfig {
    max_level_count: usize,
    max_inherited_layers: usize,
    max_frozen_tables: usize,
}
```

Rules:

1. defaults must be valid;
2. zero limits that would make the runtime unusable are rejected;
3. L6A config is branch-runtime configuration only, not durability mode,
   commit policy, lifecycle scheduling, or product branch policy.

### `BranchRuntimeError`

Initial variants should cover scaffold and later-slice routes without encoding
behavior prematurely:

```text
BranchRuntimeError::InvalidConfig { field }
BranchRuntimeError::InvalidBranchState { reason }
BranchRuntimeError::BranchNotFound { branch_id }
BranchRuntimeError::BranchAlreadyExists { branch_id }
BranchRuntimeError::InvalidBranchRow { reason }
BranchRuntimeError::InvalidReadBound { reason }
BranchRuntimeError::InvalidInheritedLayer { reason }
BranchRuntimeError::InvalidReachability { reason }
BranchRuntimeError::TableRuntime { source }
BranchRuntimeError::Publish { source }
```

Rules:

1. keep displays bounded;
2. preserve L5/L4 source errors where wrapped;
3. do not include product branch names or payload bytes in display strings;
4. do not collapse table decode/build failures into branch-not-found errors.

### `BranchReadBound`

Suggested shape:

```text
BranchReadBound::Latest
BranchReadBound::AtVersion(CommitVersion)
BranchReadBound::AtTimestamp(Timestamp)
```

L6A only defines the vocabulary. L6B/L6D/L6G implement comparisons and read
semantics.

### `BranchVisibleRow`

Suggested fields:

```text
BranchVisibleRow {
    row: StorageRow,
    source: BranchRowSource,
}
```

The row carries commit version, timestamp, expiry, tombstone flag, and value
bytes. The source records whether the row came from active, frozen, owned
immutable table, or inherited layer. L6A may use a minimal shell if the exact
source facts need later refinement.

### `BranchHistoryRow`

Suggested fields:

```text
BranchHistoryRow {
    row: StorageRow,
    source: BranchRowSource,
}
```

History rows may include tombstones. L6A should make that explicit in docs or
type names so later code does not silently filter them.

### `BranchStateFacts`

Suggested fields:

```text
BranchStateFacts {
    branch_id: BranchId,
    active_rows: u64,
    frozen_table_count: usize,
    owned_table_count: usize,
    inherited_layer_count: usize,
    max_commit_version: Option<CommitVersion>,
    timestamp_min: Option<Timestamp>,
    timestamp_max: Option<Timestamp>,
}
```

Rules:

1. facts are mechanical, not product diagnostics;
2. absent min/max facts must be represented explicitly for empty branches;
3. impossible counts or timestamp ranges are rejected.

### `BranchTableDescriptor`

Suggested shell:

```text
BranchTableDescriptor {
    identity: TableIdentity,
    facts: TableRuntimeFacts,
    level: BranchLevel,
}
```

L6A should not construct object names. Table identity remains an L5/L4 fact
until L6I defines durable reachability payloads.

### `InheritedLayerDescriptor`

Suggested shell:

```text
InheritedLayerDescriptor {
    source_branch_id: BranchId,
    fork_version: CommitVersion,
    status: InheritedLayerStatus,
    table_count: usize,
}
```

Rules:

1. source branch id and fork version are required facts;
2. status is a storage recovery/materialization fact, not product branch UX;
3. table references remain opaque until L6F/L6I.

### `BranchRuntimeStats`

Initial counters may include:

1. branch count;
2. active row count;
3. frozen table count;
4. owned immutable table count;
5. inherited layer count;
6. read-view count or captured-view counter if implemented later.

Stats must be deterministic and mechanical.

## Source Guard Policy

L6A must add or extend source guards for `crates/storage-next/src/branch/`.

Production branch code may import:

1. `crate::row`;
2. `crate::table`;
3. narrow L4 service error/fact types when wrapped by errors;
4. `strata_core_next::{BranchId, CommitVersion, Timestamp}`;
5. standard collections and sync primitives.

Production branch code must not import or contain:

1. `crate::commit`;
2. `crate::lifecycle`;
3. `crate::api`;
4. engine crates;
5. old `strata_core::VersionedValue`;
6. old product `Value`, `Key`, `Namespace`, `TypeTag`, `EntityRef`;
7. filesystem/path/backend APIs;
8. `std::env`;
9. WAL append, checkpoint scheduling, recovery orchestration, or public API
   mapping vocabulary as behavior.

Guard tests may include forbidden strings in test fixtures only when proving
the guard catches them.

## Implementation Steps

### L6A-A: Source Audit And Porting Entry

1. Read the current-storage files listed above.
2. Add an `M4-L6A` entry to `m4-l6-porting-log.md`.
3. Record preserved branch-state vocabulary and deferred behavior.

Exit: porting log has a concrete L6A entry before production code changes.

### L6A-B: Module Skeleton

1. Split `crates/storage-next/src/branch/mod.rs` into submodules.
2. Keep all re-exports `pub(crate)`.
3. Add module-level comments that describe L6 as branch-isolated storage
   mechanics, not product branch workflows.

Exit: branch module compiles with no behavior and no upper-layer imports.

### L6A-C: Error And Result Vocabulary

1. Add `BranchRuntimeError`.
2. Add `BranchRuntimeResult<T>`.
3. Implement `Display`, `Error::source`, and source-preserving wrappers for L5
   and L4 where needed.
4. Add bounded display tests.

Exit: branch errors are typed and preserve source chains.

### L6A-D: Config, Bounds, Facts, And Stats

1. Add `BranchRuntimeConfig`.
2. Add `BranchReadBound`.
3. Add branch facts/stats shells.
4. Validate impossible config and facts.

Exit: later slices have storage-owned types for branch state and reads without
adding behavior.

### L6A-E: State Descriptor Shells

1. Add `BranchStateFacts` or equivalent.
2. Add `BranchTableDescriptor` or equivalent.
3. Add `InheritedLayerDescriptor` and `InheritedLayerStatus` shells.
4. Add minimal constructors/accessors.

Exit: descriptors can be constructed in tests and expose only storage facts.

### L6A-F: Source Guards And Testkit Route

1. Add `tests/branch_lsm_source_guard.rs`.
2. Add or extend `tests/branch_lsm_properties.rs` as a non-placeholder harness.
3. Add hidden testkit scaffold route/counter for branch runtime.
4. Add sensitivity probes for forbidden imports/vocabulary.

Exit: source guards fail on upper-layer/product/backend leakage.

### L6A-G: Documentation Closeout

1. Update parent L6 plan links if names changed.
2. Record tests and commands in the L6A porting-log entry.
3. Run the L6A closeout commands.

Exit: L6A is ready for L6B row identity and read-bound helpers.

## Expected File Changes

Likely touched files:

1. `crates/storage-next/src/branch/mod.rs`
2. `crates/storage-next/src/branch/config.rs`
3. `crates/storage-next/src/branch/error.rs`
4. `crates/storage-next/src/branch/facts.rs`
5. `crates/storage-next/src/branch/read.rs`
6. `crates/storage-next/src/branch/state.rs`
7. `crates/storage-next/src/branch/tests.rs`
8. `crates/storage-next/src/testkit/mod.rs`
9. `crates/storage-next/src/testkit/branch_lsm.rs` or equivalent route
10. `crates/storage-next/tests/branch_lsm_properties.rs`
11. `crates/storage-next/tests/branch_lsm_source_guard.rs`
12. `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md`

Do not expect L6A to touch table implementation, L4 services, commit runtime,
lifecycle runtime, or public APIs.

## Verification Commands

Minimum L6A closeout commands:

```sh
cargo test -p strata-storage-next --locked --lib branch
cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties
cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties
cargo test -p strata-storage-next --locked --test branch_lsm_source_guard
cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked
cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings
cargo fmt --package strata-storage-next --check
git diff --check
```

Run the full package test if L6A changes shared testkit or crate exports:

```sh
cargo test -p strata-storage-next --locked
```

## Sensitivity Probes

Before marking L6A complete, temporarily introduce each local mutation and
confirm the targeted guard or test fails:

1. import `crate::commit` from production `branch/`;
2. import `crate::lifecycle` from production `branch/`;
3. mention old `VersionedValue` in production branch code;
4. mention old product `Value` or `Key` in production branch code;
5. call `std::fs` or `Path` from production branch code;
6. expose a bare public branch type or function;
7. construct invalid config with zero level count;
8. construct impossible branch facts with timestamp min greater than max;
9. display a branch error containing row value bytes.

## Exit Gate

L6A is complete when:

1. branch module structure exists and compiles;
2. all production branch surfaces are `pub(crate)`;
3. branch runtime config, error, read-bound, fact, descriptor, and stat shells
   exist with focused tests;
4. source guards reject upper-layer, product, filesystem, backend, and public
   API leakage;
5. generated/property harness route is non-placeholder;
6. the L6A porting-log entry records preserved, changed, deferred, and
   legacy-retained behavior;
7. no branch read, fork, materialization, reachability, compaction, or snapshot
   install behavior has been implemented prematurely.
