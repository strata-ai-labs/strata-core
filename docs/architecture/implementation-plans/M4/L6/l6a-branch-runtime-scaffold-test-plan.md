# L6A Test Plan: Branch Runtime Scaffold

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/L6/l6a-branch-runtime-scaffold-implementation-plan.md`

## Goal

Prove that the branch runtime scaffold is present, storage-owned, and ready for
L6B-L6L behavior without importing commit runtime, lifecycle orchestration,
engine/product DTOs, or backend/filesystem mechanics.

The suite must fail if L6A:

1. leaks public API from production branch code;
2. imports `crate::commit`, `crate::lifecycle`, `crate::api`, or engine crates;
3. imports old product DTOs such as `VersionedValue`, `Value`, `Key`,
   `Namespace`, `TypeTag`, or `EntityRef`;
4. calls filesystem, path, backend, environment, WAL, or checkpoint APIs;
5. collapses L5/L4 source errors into untyped branch strings;
6. accepts impossible branch config or facts;
7. implements branch read/fork/materialization behavior prematurely;
8. leaves generated branch testkit coverage as a placeholder.

## Test Locations

Use these locations:

1. `crates/storage-next/src/branch/tests.rs` for module-local scaffold tests.
2. `crates/storage-next/tests/branch_lsm_source_guard.rs` for production L6
   source-boundary scans.
3. `crates/storage-next/tests/branch_lsm_properties.rs` for generated scaffold
   route checks.
4. `crates/storage-next/src/testkit/` for hidden branch-lsm scaffold contract
   helpers.
5. `docs/architecture/implementation-plans/M4/L6/m4-l6-porting-log.md` for
   sensitivity-probe and source-map recording.

No L6A test should need JSON, graph, vector, search, event, engine branch
workflow types, old `strata_core::VersionedValue`, or backend/localfs helpers.

## Required Direct Tests

### 1. Module Construction

1. `BranchRuntimeConfig::default()` succeeds.
2. Explicit valid config construction succeeds.
3. Invalid zero/empty limits return typed `InvalidConfig`.
4. `BranchReadBound::Latest` is constructible.
5. `BranchReadBound::AtVersion` preserves the requested `CommitVersion`.
6. `BranchReadBound::AtTimestamp` preserves the requested `Timestamp`.
7. Initial branch stats/facts default to zero/empty values.
8. Empty branch facts represent absent max version and absent timestamp range
   explicitly.

### 2. Error Vocabulary

1. Every initial `BranchRuntimeError` variant is constructible.
2. Displays are bounded and do not include row value bytes.
3. `BranchRuntimeResult<T>` aliases the branch error type.
4. Wrapped L5 `TableRuntimeError` is returned from `source()`.
5. Wrapped L4 publish/service error, if present in L6A, is returned from
   `source()`.
6. Branch-not-found and branch-already-exists displays include only opaque
   branch id, not product branch name.
7. Invalid config/fact errors identify the field involved.

### 3. Fact And Descriptor Shells

1. `BranchStateFacts` accepts an empty branch shape.
2. `BranchStateFacts` rejects timestamp min greater than max.
3. `BranchStateFacts` rejects max commit version inconsistent with zero rows,
   if that invariant is represented.
4. `BranchTableDescriptor` preserves table identity and table facts without
   constructing object names.
5. `InheritedLayerDescriptor` requires source branch id and fork version.
6. `InheritedLayerStatus` is a closed enum and has explicit initial status.
7. Descriptor displays/debug output do not expose product payload bytes.
8. Descriptor equality, if derived, is purely fact-based and deterministic.

### 4. Boundary Non-Behavior

L6A should assert that scaffold APIs do not accidentally implement behavior.

1. No production branch function performs latest reads.
2. No production branch function performs `getv` reads.
3. No production branch function performs timestamp/as-of reads.
4. No production branch function rewrites branch ids.
5. No production branch function creates inherited layers from source state.
6. No production branch function materializes inherited layers.
7. No production branch function installs immutable tables.
8. No production branch function schedules compaction or recovery.

These may be source-guard assertions rather than runtime tests.

## Source Guard Requirements

`branch_lsm_source_guard.rs` must scan production `crates/storage-next/src/branch/`
files and fail on forbidden dependencies or vocabulary.

### Required Forbidden Imports

1. `crate::commit`
2. `crate::lifecycle`
3. `crate::api`
4. `strata_engine`
5. `strata_engine_next`
6. `crates/engine`
7. `crate::backend`
8. direct `crate::service::wal`
9. direct `crate::service::checkpoint`
10. `crate::testkit` in production branch code

### Required Forbidden Product Vocabulary

1. `VersionedValue`
2. `Versioned<`
3. `strata_core::Value`
4. `strata_core::Key`
5. `Namespace`
6. `TypeTag`
7. `EntityRef`
8. `JsonValue`
9. `Graph`
10. `Vector`
11. `Search`
12. `TransactionContext`

Neutral words such as "versioned row chain" are allowed in comments/docs when
they refer to storage mechanics and do not name product DTOs.

### Required Forbidden IO Vocabulary

1. `std::fs`
2. `std::path`
3. `Path`
4. `PathBuf`
5. `File`
6. `OpenOptions`
7. `std::env`
8. `env::var`
9. `pread`
10. `mmap`
11. `memmap`
12. direct backend method names such as `read_object`, `publish_object`,
    `delete_object`, and `list_prefix`

### Required Public Surface Guard

Production branch files must reject bare public items:

1. `pub struct`
2. `pub enum`
3. `pub trait`
4. `pub type`
5. `pub fn`
6. `pub const`
7. `pub static`
8. `pub mod`
9. `pub use`

`pub(crate)` remains allowed.

### Required Guard Self-Tests

The source guard should include fixture strings proving it catches:

1. forbidden upper-layer import;
2. forbidden product DTO;
3. forbidden filesystem/path API;
4. forbidden direct backend API;
5. forbidden bare public item;
6. allowed `pub(crate)` item;
7. allowed storage-owned terms such as `BranchId`, `CommitVersion`,
   `Timestamp`, `StorageRow`, and `TableRuntimeFacts`.

## Generated Scaffold Harness

L6A should add a small generated route that proves the future property harness
is not a placeholder.

The generated scaffold check should exercise:

1. valid config construction;
2. invalid config rejection;
3. read-bound construction;
4. branch state facts construction;
5. invalid facts rejection;
6. inherited descriptor construction;
7. error display/source checks;
8. stats default/exposure checks.

The external `branch_lsm_properties.rs` test should assert the route returns
nonzero counters for every scaffold category.

## Porting-Log Requirements

The `M4-L6A` entry must record:

1. current files read;
2. behavior preserved from old branch state vocabulary;
3. intentional V1 changes;
4. deferred behavior by owner slice;
5. tests and guards added;
6. sensitivity probes run;
7. retirement status of old storage code.

The entry should not claim branch reads, fork, materialization, reachability,
compaction install, or snapshot install are implemented.

## Cross-Feature Matrix

Mandatory L6A commands:

| Mode | Purpose | Command |
|---|---|---|
| branch unit | scaffold module tests | `cargo test -p strata-storage-next --locked --lib branch` |
| generated scaffold | branch testkit route | `cargo test -p strata-storage-next --features testkit --locked --test branch_lsm_properties` |
| no-default generated | no accidental localfs/default dependency | `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test branch_lsm_properties` |
| source guards | L6 purity | `cargo test -p strata-storage-next --locked --test branch_lsm_source_guard` |
| wasm/no-default | browser-compatible scaffold | `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked` |
| lint | all-target/all-feature lint surface | `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings` |
| format | rustfmt stability | `cargo fmt --package strata-storage-next --check` |
| whitespace | patch hygiene | `git diff --check` |

Run `cargo test -p strata-storage-next --locked` if shared testkit, crate
exports, or existing integration tests are touched.

## Sensitivity Probes

Before closing L6A, run targeted local mutations and verify failures:

1. add `use crate::commit;` to production branch code;
2. add `use crate::lifecycle;` to production branch code;
3. add `let _: VersionedValue;` to production branch code;
4. add `let _: Value;` to production branch code;
5. add `std::fs::read("x")` to production branch code;
6. add `read_object` to production branch code;
7. change `pub(crate) struct` to `pub struct`;
8. allow zero `max_level_count`;
9. allow timestamp min greater than timestamp max;
10. remove the generated scaffold counter for one category.

Record the probe results in the L6A porting-log entry.

## Exit Gate

L6A test coverage is complete when:

1. direct branch scaffold tests pass;
2. source guards have self-tests for every forbidden category;
3. generated scaffold harness exposes nonzero counters for every category;
4. no-default and wasm checks pass;
5. clippy, fmt, and whitespace checks pass;
6. porting log records the L6A source map, deferrals, and probes;
7. no branch read/fork/materialization/reachability behavior was implemented
   ahead of its owning slice.
