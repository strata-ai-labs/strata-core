# L7A Test Plan: Commit Runtime Scaffold

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/L7/l7a-commit-runtime-scaffold-implementation-plan.md`

## Goal

Prove that the commit runtime scaffold is present, storage-owned, crate-private,
and ready for L7B-L7N behavior without importing product transaction sessions,
engine DTOs, table internals, backend/layout mechanics, filesystem APIs, or
durable transaction-id concepts.

The suite must fail if L7A:

1. exposes public commit runtime APIs;
2. imports engine, product, backend, layout, table-internal, filesystem, or
   lifecycle concepts directly;
3. uses public transaction-session vocabulary as a production surface;
4. defines durable storage transaction-id fields or allocator errors;
5. accepts impossible config or visibility fact ordering;
6. collapses lower-layer source errors into untyped strings;
7. leaves generated commit-runtime scaffold coverage as a placeholder.

## Test Locations

Use these locations:

1. `crates/storage-next/src/commit/tests.rs` for module-local scaffold tests.
2. `crates/storage-next/tests/commit_runtime_source_guard.rs` for production
   L7 source-boundary scans.
3. `crates/storage-next/tests/commit_runtime_properties.rs` for generated
   scaffold route checks.
4. `crates/storage-next/src/testkit/commit_runtime.rs` for hidden scaffold
   contract helpers.
5. `docs/architecture/implementation-plans/M4/L7/m4-l7-porting-log.md` for
   source-map, deferral, and sensitivity-probe recording.

No L7A test should require localfs, L4 WAL services, L6 branch mutation
behavior, recovery orchestration, engine DTOs, JSON/graph/vector/search/event
modules, old public transaction handles, or product value types.

## Required Direct Tests

### 1. Module Construction

1. `CommitRuntimeConfig::default()` succeeds.
2. Explicit valid config construction succeeds.
3. Invalid zero limits return typed `InvalidConfig`.
4. Read-only diagnostics config is internal and does not expose a public
   transaction-session API.
5. `CommitPhase` variants are constructible.
6. `CommitDurabilityClass` variants are constructible.
7. Empty `CommitVisibilityFacts` represents absent allocated/durable/applied/
   visible/timeline versions explicitly.
8. Empty `CommitRuntimeStats` starts at zero for every counter.

### 2. Error Vocabulary

1. Every initial `CommitRuntimeError` variant is constructible.
2. Displays are bounded and use storage vocabulary.
3. Displays do not include row value bytes.
4. Displays do not include product branch names or public transaction-session
   claims.
5. Wrapped lower-layer errors are returned from `Error::source()`.
6. `CommitRuntimeResult<T>` aliases the commit error type.
7. There is no storage transaction-id overflow or transaction-id catch-up error
   variant in V1.

### 3. Fact Shells

1. `CommitVisibilityFacts` accepts an empty runtime shape.
2. `CommitVisibilityFacts` accepts monotonic allocated/durable/applied/visible
   orderings that are explicitly documented.
3. `CommitVisibilityFacts` rejects visible version greater than applied
   version.
4. `CommitVisibilityFacts` rejects applied version greater than allocated
   version when no replay override is present.
5. `CommitVisibilityFacts` keeps durable and visible facts separate.
6. `CommitRuntimeStats` equality/debug output is deterministic.
7. Fact debug output does not expose product payload bytes.

### 4. Boundary Non-Behavior

L7A tests should not add permanent source assertions that would block later
L7B-L7N behavior. Instead, module-local tests should prove the scaffold has no
runtime behavior by construction:

1. no test calls a commit apply path;
2. no test appends WAL;
3. no test mutates L6 branch state;
4. no test allocates a commit version;
5. no test constructs a `CommitBatch`;
6. no test writes timeline rows;
7. no test replays durable rows.

Later slices replace these absences with positive behavioral tests.

## Source Guard Requirements

`commit_runtime_source_guard.rs` must scan production
`crates/storage-next/src/commit/` files and fail on forbidden dependencies or
vocabulary.

### Required Forbidden Imports

1. engine crates;
2. product API/DTO modules;
3. `crate::table` internals;
4. `crate::backend`;
5. `crate::layout`;
6. `crate::lifecycle`;
7. `crate::api`;
8. `crate::testkit` in production commit code.

Allowed storage-layer imports include `crate::branch`, `crate::row`,
`crate::format::wal`, and `crate::service::wal`, because later L7 slices need
those surfaces.

### Required Forbidden Product Vocabulary

1. `TransactionContext`
2. `TransactionManager`
3. `begin_transaction`
4. `rollback`
5. `VersionedValue`
6. `Versioned<`
7. `strata_core::Value`
8. `strata_core::Key`
9. `Namespace`
10. `EntityRef`
11. `JsonValue`
12. `Graph`
13. `Vector`
14. `Search`

Neutral storage terms such as `CommitVersion`, `committed row`, and `commit
runtime` are allowed.

### Required Forbidden IO Vocabulary

1. `std::fs`
2. `std::path`
3. `Path`
4. `PathBuf`
5. `File`
6. `OpenOptions`
7. `std::env`
8. `env::var`
9. `mmap`
10. `memmap`
11. direct backend method names such as `read_object`, `publish_object`,
    `delete_object`, and `list_prefix`

### Required Public Surface Guard

Production commit files must reject bare public items:

1. `pub struct`
2. `pub enum`
3. `pub trait`
4. `pub type`
5. `pub fn`
6. `pub const`
7. `pub static`
8. `pub mod`
9. `pub use`

`pub(crate)` remains allowed. The crate root must continue using private
`mod commit;`, not `pub mod commit;`.

### Required Guard Self-Tests

The source guard should include fixture strings proving it catches:

1. forbidden engine import;
2. forbidden product DTO vocabulary;
3. forbidden public transaction-session vocabulary;
4. forbidden table-internal import;
5. forbidden backend/layout import;
6. forbidden filesystem/path API;
7. forbidden bare public item;
8. forbidden durable transaction-id vocabulary;
9. allowed `pub(crate)` item;
10. allowed storage-owned terms such as `BranchId`, `CommitVersion`,
    `Timestamp`, `StorageRow`, `WalRecord`, and `WalService`.

## Generated Scaffold Harness

L7A should add a generated route that proves the future property harness is not
a placeholder.

The scaffold contract should exercise:

1. valid config construction;
2. invalid config rejection;
3. phase and durability fact construction;
4. visibility fact construction;
5. invalid visibility fact rejection;
6. error display checks;
7. error source-chain checks;
8. stats default checks;
9. source guard fixture checks.

The external `commit_runtime_properties.rs` test should assert the route
returns nonzero counters for every scaffold category.

## Porting-Log Requirements

The `M4-L7A` entry must record:

1. current files read;
2. behavior preserved as vocabulary/source evidence;
3. behavior retired from V1, especially public transaction sessions and
   durable transaction ids;
4. behavior deferred by owner slice;
5. tests and guards added;
6. sensitivity probes planned or run.

The entry should not claim commit batch validation, version allocation,
timestamp allocation, WAL append, L6 apply, timeline writes, durable recovery,
or quiesce are implemented.

## Cross-Feature Matrix

Mandatory L7A commands:

| Mode | Purpose | Command |
|---|---|---|
| commit unit | scaffold module tests | `cargo test -p strata-storage-next --locked --lib commit` |
| generated scaffold | commit testkit route | `cargo test -p strata-storage-next --features testkit --locked --test commit_runtime_properties` |
| no-default generated | no accidental default/localfs dependency | `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test commit_runtime_properties` |
| source guards | L7 purity | `cargo test -p strata-storage-next --locked --test commit_runtime_source_guard` |
| wasm/no-default | browser-compatible scaffold | `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked` |
| lint | all-target/all-feature lint surface | `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings` |
| format | rustfmt stability | `cargo fmt --package strata-storage-next --check` |
| whitespace | patch hygiene | `git diff --check` |

Run the broader storage-next test suite if shared testkit exports or crate-root
module boundaries are touched.

## Sensitivity Probes

Before closing L7A, run targeted local mutations and verify failures:

1. add `use strata_engine_next;` to production commit code;
2. add `use crate::table::TableRow;` to production commit code;
3. add `use crate::backend;` to production commit code;
4. add `use crate::layout;` to production commit code;
5. add `let _: TransactionContext;` to production commit code;
6. add `let _: VersionedValue;` to production commit code;
7. add `let _: TransactionId;` to production commit code;
8. add `std::fs::read("x")` to production commit code;
9. change `pub(crate) struct` to `pub struct`;
10. allow zero `max_mutations_per_batch`;
11. allow visible version greater than applied version;
12. remove the generated scaffold counter for one category.

Record the probe results in the L7A porting-log entry.

## Exit Gate

L7A test coverage is complete when:

1. direct commit scaffold tests pass;
2. source guards have self-tests for every forbidden category;
3. generated scaffold harness exposes nonzero counters for every category;
4. no-default and wasm checks pass;
5. clippy, fmt, and whitespace checks pass;
6. porting log records the L7A source map, deferrals, and probes;
7. no commit batch, allocation, WAL, L6 apply, timeline, replay, or quiesce
   behavior was implemented ahead of its owning slice.
