# L8A Test Plan: Lifecycle Scaffold

Status: draft test plan

Parent plan:
`docs/architecture/implementation-plans/M4/L8/l8a-lifecycle-scaffold-implementation-plan.md`

## Goal

Prove that the lifecycle scaffold is present, storage-owned, crate-private, and
ready for L8B-L8P behavior without importing product lifecycle policy, engine
DTOs, follower refresh, StrataHub behavior, public maintenance commands, raw
filesystem APIs, or lower-layer test helpers into production lifecycle code.

The suite must fail if L8A:

1. exposes public lifecycle runtime APIs;
2. imports engine, product, StrataHub, follower, filesystem, or public command
   concepts directly;
3. lets lower layers import `crate::lifecycle`;
4. uses public open/maintenance/recovery UX vocabulary as a production surface;
5. accepts impossible config or raw fact ordering;
6. collapses lower-layer source errors into untyped strings;
7. leaves generated lifecycle scaffold coverage as a placeholder;
8. implements open, recovery, maintenance, checkpoint, retention, quarantine,
   repair, or close behavior ahead of its owning slice.

## Test Locations

Use these locations:

1. `crates/storage-next/src/lifecycle/tests/mod.rs` for module-local scaffold tests.
2. `crates/storage-next/tests/lifecycle_source_guard.rs` for production L8
   source-boundary scans.
3. `crates/storage-next/tests/lifecycle_properties.rs` for generated scaffold
   route checks.
4. `crates/storage-next/src/testkit/lifecycle/mod.rs` or
   `crates/storage-next/src/testkit/lifecycle/` for hidden scaffold contract
   helpers.
5. `docs/architecture/implementation-plans/M4/L8/m4-l8-porting-log.md` for
   source-map, deferral, and sensitivity-probe recording.

No L8A test should require localfs, durable L4 services, L6 branch mutation
behavior, L7 commit execution, recovery orchestration, engine DTOs,
JSON/graph/vector/search/event modules, StrataHub modules, follower mode, old
public transaction handles, or product value types.

## Required Direct Tests

### 1. Module Construction

1. `LifecycleConfig::default()` succeeds.
2. Explicit valid config construction succeeds.
3. Invalid zero limits return typed `InvalidConfig`.
4. Lossy recovery is disabled by default.
5. Close timeout policy uses an explicit enum, not a boolean flag.
6. `LifecycleState` variants are constructible.
7. `StorageMode` variants are constructible.
8. `RecoveryHealth` variants are constructible.
9. `MaintenanceTaskKind` variants are constructible.
10. Empty/default `LifecycleStats` starts at zero for every counter.

### 2. Error Vocabulary

1. Every initial `LifecycleError` variant is constructible.
2. Displays are bounded and use storage lifecycle vocabulary.
3. Displays do not include row value bytes or object payload bytes.
4. Displays do not include product open policy, public maintenance command, or
   user recovery advice.
5. Displays do not include follower or IPC vocabulary.
6. Wrapped lower-layer errors are returned from `Error::source()`.
7. `LifecycleResult<T>` aliases the lifecycle error type.
8. There is no follower refresh, IPC, public command, or product recovery error
   variant in V1 L8.

### 3. Fact Shells

1. `StorageOpenPlan` accepts a cache-mode scaffold shape.
2. `StorageOpenPlan` accepts durable standard and durable always scaffold
   shapes.
3. `StorageOpenPlan` rejects cache plus durable policy when the scaffold exposes
   enough fields to validate it.
4. `StorageOpenPlan` has no product access mode field.
5. `StorageOpenPlan` has no IPC field.
6. `StorageOpenPlan` has no primitive registry field.
7. `StorageOpenPlan` has no StrataHub field.
8. `StorageOpenOutcome` reports mode, opened/created fact, recovery health,
   recovered visible version, recovered max commit version, optional durable
   recovery facts, optional backend/database/codec facts, and raw stats as
   storage facts.
9. Cache `StorageOpenOutcome` does not report durable recovery facts or product
   open acceptance.
10. `RecoveryHealth` distinguishes healthy, degraded, and failed shapes.
11. Recovery degradation class distinguishes data loss, policy downgrade, and
    telemetry-only facts.
12. `MaintenanceTaskKind` debug/equality output is deterministic.
13. Fact debug output does not expose product payload bytes.

### 4. Boundary Non-Behavior

L8A tests should not add permanent source assertions that block later L8B-L8P
behavior. Instead, module-local tests should prove the scaffold has no runtime
behavior by construction:

1. no test opens storage;
2. no test creates or loads a manifest;
3. no test appends or replays WAL;
4. no test publishes a snapshot;
5. no test mutates L6 branch state;
6. no test calls L7 commit or replay execution;
7. no test schedules a real maintenance task;
8. no test flushes, checkpoints, compacts, materializes, truncates, retains,
   quarantines, purges, repairs, or closes storage.

Later slices replace these absences with positive behavioral tests.

## Source Guard Requirements

`lifecycle_source_guard.rs` must scan production
`crates/storage-next/src/lifecycle/` files and fail on forbidden dependencies or
vocabulary.

### Required Forbidden Imports

1. engine crates;
2. product API/DTO modules;
3. StrataHub modules;
4. follower refresh modules;
5. public API modules;
6. `crate::testkit` in production lifecycle code;
7. raw filesystem/path APIs;
8. raw environment APIs.

Allowed storage-layer imports include `crate::backend`, `crate::layout`,
`crate::format`, `crate::service`, `crate::table`, `crate::branch`,
`crate::commit`, and `crate::row`, because L8 is the orchestration layer over
those lower layers.

### Required Forbidden Product Vocabulary

1. `Database::open`
2. `OpenOptions`
3. `ProductOpen`
4. `ProductRecovery`
5. `public maintenance`
6. `manual maintenance command`
7. `VersionedValue`
8. `EntityRef`
9. `JsonValue`
10. `Graph`
11. `Vector`
12. `Search`
13. `Embedding`
14. `Inference`
15. `StrataHub`
16. `Follower`
17. `refresh follower`
18. `TransactionContext`
19. `begin_transaction`

Neutral storage terms such as `open plan`, `recovery health`, `checkpoint`,
`quarantine`, `retention`, and `close outcome` are allowed.

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

L8 may depend on backend-owned traits and service APIs, but production lifecycle
code must not reach around L1/L4 and perform raw filesystem or environment work.

### Required Lower-Layer Upward-Import Guard

Production lower-layer files must not import `crate::lifecycle` from:

1. `src/backend/`;
2. `src/layout/`;
3. `src/format/`;
4. `src/service/`;
5. `src/table/`;
6. `src/branch/`;
7. `src/commit/`;
8. `src/row/`.

Tests and testkit may import lifecycle helpers only behind test/testkit targets.

### Required Public Surface Guard

Production lifecycle files must reject bare public items:

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
`mod lifecycle;`, not `pub mod lifecycle;`.

### Required Guard Self-Tests

The source guard should include fixture strings proving it catches:

1. forbidden engine import;
2. forbidden product DTO vocabulary;
3. forbidden public open-policy vocabulary;
4. forbidden StrataHub import;
5. forbidden follower vocabulary;
6. forbidden raw filesystem/path API;
7. forbidden raw environment API;
8. forbidden lower-layer upward lifecycle import;
9. forbidden bare public item;
10. allowed `pub(crate)` item;
11. allowed storage-owned terms such as `BranchId`, `CommitVersion`,
    `StorageRow`, `WalService`, `RecoveryHealth`, and `MaintenanceTask`.

## Generated Scaffold Harness

L8A should add a generated route that proves the future property harness is not
a placeholder.

The scaffold contract should exercise:

1. valid config construction;
2. invalid config rejection;
3. lifecycle state construction;
4. storage mode construction;
5. open plan construction;
6. open outcome construction;
7. recovery health construction;
8. maintenance task construction;
9. retention/quarantine/close fact construction;
10. error display checks;
11. error source-chain checks;
12. stats default checks;
13. source guard fixture checks.

The external `lifecycle_properties.rs` test should assert the route returns
nonzero counters for every scaffold category.

## Porting-Log Requirements

The `M4-L8A` entry must record:

1. current files read;
2. behavior preserved as vocabulary/source evidence;
3. behavior intentionally changed;
4. behavior retired from V1, especially follower mode, public maintenance
   commands, primitive reconstruction, and product recovery wording;
5. behavior deferred by owner slice;
6. tests and guards added;
7. sensitivity probes planned or run.

The entry should not claim open, recovery, maintenance, checkpoint, retention,
quarantine, repair, close, crash recovery, or L7 bootstrap behavior is
implemented.

## Cross-Feature Matrix

Mandatory L8A commands:

| Mode | Purpose | Command |
|---|---|---|
| lifecycle unit | scaffold module tests | `cargo test -p strata-storage-next --locked --lib lifecycle` |
| generated scaffold | lifecycle testkit route | `cargo test -p strata-storage-next --features testkit --locked --test lifecycle_properties` |
| no-default generated | no accidental default/localfs dependency | `cargo test -p strata-storage-next --no-default-features --features testkit --locked --test lifecycle_properties` |
| source guards | L8 purity | `cargo test -p strata-storage-next --locked --test lifecycle_source_guard` |
| wasm/no-default | browser-compatible scaffold | `cargo check -p strata-storage-next --no-default-features --features testkit --target wasm32-unknown-unknown --all-targets --locked` |
| lint | all-target/all-feature lint surface | `cargo clippy -p strata-storage-next --all-targets --all-features --locked -- -D warnings` |
| format | rustfmt stability | `cargo fmt --package strata-storage-next --check` |
| whitespace | patch hygiene | `git diff --check` |

Run the broader storage-next test suite if shared testkit exports or crate-root
module boundaries are touched.

## Sensitivity Probes

Before closing L8A, run targeted local mutations and verify failures:

1. add `use strata_engine_next;` to production lifecycle code;
2. add `use crate::api;` to production lifecycle code;
3. add `use crate::testkit;` to production lifecycle code;
4. add `let _: VersionedValue;` to production lifecycle code;
5. add `let _: EntityRef;` to production lifecycle code;
6. add `let _: StrataHub;` to production lifecycle code;
7. add `let _: Follower;` to production lifecycle code;
8. add `std::fs::read("x")` to production lifecycle code;
9. add `std::env::var("X")` to production lifecycle code;
10. change `pub(crate) struct` to `pub struct`;
11. add `use crate::lifecycle;` to production `commit/` or `branch/` code;
12. allow zero `max_maintenance_queue_depth`;
13. remove the generated scaffold counter for one category;
14. add a fake open behavior path to L8A and verify non-behavior tests or review
    checks catch scope creep.

Record the probe results in the L8A porting-log entry.

## Exit Gate

L8A test coverage is complete when:

1. direct lifecycle scaffold tests pass;
2. source guards have self-tests for every forbidden category;
3. generated scaffold harness exposes nonzero counters for every category if
   added;
4. no-default and wasm checks pass;
5. clippy, fmt, and whitespace checks pass;
6. porting log records the L8A source map, deferrals, and probes;
7. no open, recovery, maintenance, checkpoint, retention, quarantine, repair,
   close, crash recovery, or L7 bootstrap behavior was implemented ahead of its
   owning slice.
